//! Bounded, streaming reader for existing sitemap files.
//!
//! [`inspect`] reads a `<urlset>` or `<sitemapindex>` document, plain or
//! gzip-compressed, checks its structure, validates every entry with the same
//! rules as the generator ([`sitemap_validation`]), and hands each valid
//! entry to a callback. Memory stays bounded: entries are not collected,
//! only a 128-bit fingerprint per URL when deduplication is on.
//!
//! # Hostile input
//!
//! The reader is meant for files fetched from anywhere:
//!
//! * a `DOCTYPE` stops reading ([`DiagnosticCode::XmlDoctypeForbidden`]), so
//!   no entity can be declared, expanded or fetched (no XXE, no "billion
//!   laughs");
//! * only the five predefined entities and character references are
//!   decoded; anything else is a well-formedness error;
//! * the **uncompressed** size is capped ([`ReaderLimits::max_bytes`],
//!   50 MiB by default, the protocol limit), which also bounds gzip bombs;
//! * element depth and the text length of one element are capped;
//! * only UTF-8 is accepted, as required by the sitemap protocol.
//!
//! Every limit produces a diagnostic and stops reading; the report then has
//! `complete == false`. Only I/O errors of the underlying source are returned
//! as `Err`.

use std::collections::{BTreeMap, HashSet};
use std::error::Error as StdError;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufRead, BufReader, Read};

use flate2::bufread::MultiGzDecoder;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use sitemap_model::limits::{MAX_SITEMAP_BYTES, MAX_SITEMAPS_PER_INDEX, MAX_URLS_PER_SITEMAP};
use sitemap_model::{
    Diagnostic, DiagnosticCode, IndexEntry, Location, RawAlternate, RawImage, RawUrlRecord,
    UrlEntry,
};
use sitemap_validation::{ValidationMode, ValidationOptions, Validator};

/// Namespace of the sitemap protocol.
pub const SITEMAP_NS: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";
/// Namespace of `xhtml:link`.
pub const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
/// Namespace of Google image sitemaps.
pub const IMAGE_NS: &str = "http://www.google.com/schemas/sitemap-image/1.1";

/// Resource limits applied while reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReaderLimits {
    /// Maximum uncompressed size in bytes. Default: 50 MiB (protocol limit).
    pub max_bytes: u64,
    /// Maximum element nesting depth. Default: 32 (sitemaps need at most 5).
    pub max_depth: usize,
    /// Maximum text length of one element, in bytes. Default: 64 KiB.
    pub max_text_bytes: usize,
}

impl Default for ReaderLimits {
    fn default() -> Self {
        Self {
            max_bytes: MAX_SITEMAP_BYTES,
            max_depth: 32,
            max_text_bytes: 64 * 1024,
        }
    }
}

/// Options of [`inspect`].
#[derive(Clone, Debug)]
pub struct InspectOptions {
    /// Resource limits.
    pub limits: ReaderLimits,
    /// Validation profile applied to `<url>` entries. Index entries are
    /// always checked with the standard profile (they have no `changefreq`
    /// or `priority`).
    pub validation: ValidationOptions,
    /// Report repeated `loc` values with `DUPLICATE_URL`. Default: `true`.
    pub dedupe: bool,
}

impl Default for InspectOptions {
    fn default() -> Self {
        Self {
            limits: ReaderLimits::default(),
            validation: ValidationOptions::default(),
            dedupe: true,
        }
    }
}

/// Kind of document, from its root element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentKind {
    /// `<urlset>`
    Urlset,
    /// `<sitemapindex>`
    SitemapIndex,
}

impl DocumentKind {
    /// Root element name.
    pub fn as_str(self) -> &'static str {
        match self {
            DocumentKind::Urlset => "urlset",
            DocumentKind::SitemapIndex => "sitemapindex",
        }
    }
}

/// A valid entry read from the document.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A `<url>` of a `<urlset>`.
    Url(UrlEntry),
    /// A `<sitemap>` of a `<sitemapindex>`.
    Sitemap(IndexEntry),
}

/// Result of [`inspect`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InspectReport {
    /// Kind of document, when the root element was accepted.
    pub kind: Option<DocumentKind>,
    /// The input was gzip-compressed.
    pub compressed: bool,
    /// Uncompressed bytes read.
    pub xml_bytes: u64,
    /// The whole document was read. `false` when a limit or a fatal error
    /// stopped reading; counts then cover only the part read.
    pub complete: bool,
    /// `<url>` or `<sitemap>` elements found.
    pub entries: u64,
    /// Entries without any error, duplicates excluded.
    pub valid_entries: u64,
    /// Entries dropped as duplicates of an earlier `loc`.
    pub duplicates: u64,
    /// Valid entries with `lastmod`.
    pub with_lastmod: u64,
    /// Valid entries with at least one alternate.
    pub with_alternates: u64,
    /// Valid entries with at least one image.
    pub with_images: u64,
    /// Elements of namespaces the reader does not model (for example video
    /// or news), by namespace URI. They are skipped, not validated.
    pub unsupported_extensions: BTreeMap<String, u64>,
    /// Findings, in document order.
    pub diagnostics: Vec<Diagnostic>,
}

impl InspectReport {
    /// Number of error-level diagnostics.
    pub fn error_count(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.is_error()).count()
    }
}

/// Reads and checks one sitemap or sitemap index document.
///
/// `on_item` receives every valid, non-duplicate entry in document order.
/// Returns `Err` only for I/O errors of `input` itself.
pub fn inspect<R: Read>(
    input: R,
    options: &InspectOptions,
    on_item: impl FnMut(Item),
) -> io::Result<InspectReport> {
    let mut source = BufReader::new(SourceReader(input));
    let compressed = source
        .fill_buf()
        .map_err(|e| source_error(&e).unwrap_or(e))?
        .starts_with(&[0x1f, 0x8b]);
    let decoded: Box<dyn Read + '_> = if compressed {
        Box::new(MultiGzDecoder::new(source))
    } else {
        Box::new(source)
    };
    let limited = LimitedReader {
        inner: decoded,
        read: 0,
        max: options.limits.max_bytes,
    };
    let mut state = State::new(options, on_item);
    state.report.compressed = compressed;
    state.run(NsReader::from_reader(BufReader::new(limited)))?;
    Ok(state.report)
}

// ---------------------------------------------------------------------------
// I/O adapters

/// Marks errors of the caller's reader, so they are returned as `Err` and
/// not mistaken for corrupt gzip data.
struct SourceReader<R>(R);

#[derive(Debug)]
struct SourceError(io::Error);

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl StdError for SourceError {}

impl<R: Read> Read for SourceReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0
            .read(buf)
            .map_err(|e| io::Error::other(SourceError(e)))
    }
}

#[derive(Debug)]
struct LimitExceeded;

impl fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("size limit exceeded")
    }
}

impl StdError for LimitExceeded {}

/// Fails as soon as more than `max` uncompressed bytes are read.
struct LimitedReader<R> {
    inner: R,
    read: u64,
    max: u64,
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Allow one byte past the limit so that "exactly max" is accepted.
        let room = (self.max + 1).saturating_sub(self.read);
        if room == 0 {
            return Err(io::Error::other(LimitExceeded));
        }
        let cap = buf.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..cap])?;
        self.read += n as u64;
        if self.read > self.max {
            return Err(io::Error::other(LimitExceeded));
        }
        Ok(n)
    }
}

/// Rebuilds the caller's error (same kind and message) when `error` wraps it.
fn source_error(error: &io::Error) -> Option<io::Error> {
    let source = error.get_ref()?.downcast_ref::<SourceError>()?;
    Some(io::Error::new(source.0.kind(), source.0.to_string()))
}

fn inner_is<T: StdError + 'static>(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|e| e.is::<T>())
}

// ---------------------------------------------------------------------------
// Parser state

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Loc,
    Lastmod,
    Changefreq,
    Priority,
    ImageLoc,
}

impl Field {
    fn core(local: &str, kind: DocumentKind) -> Option<(Field, u8)> {
        match (kind, local) {
            (_, "loc") => Some((Field::Loc, 0)),
            (_, "lastmod") => Some((Field::Lastmod, 1)),
            (DocumentKind::Urlset, "changefreq") => Some((Field::Changefreq, 2)),
            (DocumentKind::Urlset, "priority") => Some((Field::Priority, 3)),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Field::Loc => "loc",
            Field::Lastmod => "lastmod",
            Field::Changefreq => "changefreq",
            Field::Priority => "priority",
            Field::ImageLoc => "image:loc",
        }
    }
}

#[derive(Default)]
struct EntryBuilder {
    raw: RawUrlRecord,
    /// Bit set of core fields seen (bit = XSD position).
    seen: u8,
    last_core: Option<u8>,
    extension_seen: bool,
    order_reported: bool,
    /// Inside `<image:image>`: the `image:loc` text, once read.
    image: Option<Option<String>>,
}

/// Why reading stopped.
enum Stop {
    /// A diagnostic was recorded.
    Fatal,
    /// The caller's reader failed.
    Source(io::Error),
}

struct State<'o, F> {
    options: &'o InspectOptions,
    validator: Validator,
    index_validator: Validator,
    on_item: F,
    report: InspectReport,
    seen: HashSet<u128>,
    depth: usize,
    root_closed: bool,
    /// Skip every event while `depth >= skip`.
    skip: Option<usize>,
    entry: Option<EntryBuilder>,
    field: Option<(Field, usize)>,
    text: String,
    limit_reported: bool,
}

type Resolved<'a> = ResolveResult<'a>;

fn namespace_of(resolved: &Resolved<'_>) -> String {
    match resolved {
        ResolveResult::Bound(ns) => ns.as_ref().to_owned(),
        ResolveResult::Unbound | ResolveResult::Unknown(_) => String::new(),
    }
}

impl<'o, F: FnMut(Item)> State<'o, F> {
    fn new(options: &'o InspectOptions, on_item: F) -> Self {
        let index_options = ValidationOptions {
            mode: ValidationMode::Standard,
            ..options.validation.clone()
        };
        Self {
            options,
            validator: Validator::new(options.validation.clone()),
            index_validator: Validator::new(index_options),
            on_item,
            report: InspectReport::default(),
            seen: HashSet::new(),
            depth: 0,
            root_closed: false,
            skip: None,
            entry: None,
            field: None,
            text: String::new(),
            limit_reported: false,
        }
    }

    fn run<B: BufRead>(&mut self, mut reader: NsReader<B>) -> io::Result<()> {
        let mut buf = Vec::new();
        let outcome = loop {
            buf.clear();
            let step = match reader.read_resolved_event_into(&mut buf) {
                Ok((resolved, event)) => {
                    let ns = namespace_of(&resolved);
                    self.event(ns, event)
                }
                Err(error) => Err(self.xml_error(error, reader.error_position())),
            };
            match step {
                Ok(true) => continue,
                Ok(false) => break Ok(()),
                Err(stop) => break Err(stop),
            }
        };
        self.report.xml_bytes = reader.buffer_position();
        match outcome {
            Ok(()) => {
                self.report.complete = true;
                Ok(())
            }
            Err(Stop::Fatal) => Ok(()),
            Err(Stop::Source(error)) => Err(error),
        }
    }

    fn fatal(&mut self, code: DiagnosticCode, message: impl Into<String>) -> Stop {
        self.report
            .diagnostics
            .push(Diagnostic::new(code, message, Location::default()));
        Stop::Fatal
    }

    fn xml_error(&mut self, error: quick_xml::Error, position: u64) -> Stop {
        if let quick_xml::Error::Io(io_error) = &error {
            if inner_is::<LimitExceeded>(io_error) {
                let max = self.options.limits.max_bytes;
                return self.fatal(
                    DiagnosticCode::SitemapTooLarge,
                    format!("uncompressed document is larger than {max} bytes"),
                );
            }
            if let Some(original) = source_error(io_error) {
                return Stop::Source(original);
            }
            if self.report.compressed {
                return self.fatal(
                    DiagnosticCode::GzipInvalid,
                    format!("gzip data cannot be decompressed: {io_error}"),
                );
            }
        }
        self.fatal(
            DiagnosticCode::XmlMalformed,
            format!("not well-formed XML at byte {position}: {error}"),
        )
    }

    /// Returns `Ok(false)` at the end of the document.
    fn event(&mut self, ns: String, event: Event<'_>) -> Result<bool, Stop> {
        match event {
            Event::Start(start) => {
                self.open(&ns, &start)?;
            }
            Event::Empty(start) => {
                self.open(&ns, &start)?;
                let local = local_name(&start);
                self.close(&ns, &local)?;
            }
            Event::End(end) => {
                let local = end.local_name().as_ref().to_owned();
                self.close(&ns, &local)?;
            }
            Event::Text(text) => {
                let content = text.xml10_content();
                self.push_text(&content)?;
            }
            Event::CData(data) => {
                let content = data.into_inner();
                self.push_text(&content)?;
            }
            Event::GeneralRef(reference) => {
                let decoded = match reference.resolve_char_ref() {
                    Ok(Some(c)) => c.to_string(),
                    Ok(None) => match reference.as_ref() {
                        "lt" => "<".to_owned(),
                        "gt" => ">".to_owned(),
                        "amp" => "&".to_owned(),
                        "apos" => "'".to_owned(),
                        "quot" => "\"".to_owned(),
                        other => {
                            return Err(self.fatal(
                                DiagnosticCode::XmlMalformed,
                                format!("undefined entity '&{other};' (no DTD is processed)"),
                            ));
                        }
                    },
                    Err(error) => {
                        return Err(self.fatal(
                            DiagnosticCode::XmlMalformed,
                            format!("invalid character reference: {error}"),
                        ));
                    }
                };
                self.push_text(&decoded)?;
            }
            Event::Decl(decl) => {
                if let Some(Ok(encoding)) = decl.encoding() {
                    if !encoding.eq_ignore_ascii_case("utf-8") {
                        return Err(self.fatal(
                            DiagnosticCode::XmlEncodingUnsupported,
                            format!("declared encoding '{encoding}' is not UTF-8"),
                        ));
                    }
                }
            }
            Event::DocType(_) => {
                return Err(self.fatal(
                    DiagnosticCode::XmlDoctypeForbidden,
                    "DOCTYPE declarations are refused; a sitemap never needs one",
                ));
            }
            Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => {
                if self.report.kind.is_none() && !self.has_fatal() {
                    return Err(self.fatal(DiagnosticCode::XmlMalformed, "no root element"));
                }
                if self.depth > 0 {
                    return Err(self.fatal(
                        DiagnosticCode::XmlMalformed,
                        "unexpected end of document: elements are not closed",
                    ));
                }
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn has_fatal(&self) -> bool {
        self.report.diagnostics.iter().any(Diagnostic::is_error)
    }

    fn push_text(&mut self, content: &str) -> Result<(), Stop> {
        if self.skip.is_some() || self.field.is_none() {
            return Ok(());
        }
        if self.text.len() + content.len() > self.options.limits.max_text_bytes {
            let max = self.options.limits.max_text_bytes;
            return Err(self.fatal(
                DiagnosticCode::XmlTextTooLong,
                format!("element text is longer than {max} bytes"),
            ));
        }
        self.text.push_str(content);
        Ok(())
    }

    fn warn(&mut self, code: DiagnosticCode, message: String, field: Option<&str>) {
        let mut location = Location::record(self.report.entries, None);
        if self.entry.is_none() {
            location = Location::default();
        }
        if let Some(field) = field {
            location = location.with_field(field);
        }
        self.report
            .diagnostics
            .push(Diagnostic::new(code, message, location));
    }

    fn open(&mut self, ns: &str, start: &BytesStart<'_>) -> Result<(), Stop> {
        self.depth += 1;
        if self.depth > self.options.limits.max_depth {
            let max = self.options.limits.max_depth;
            return Err(self.fatal(
                DiagnosticCode::XmlDepthExceeded,
                format!("elements are nested deeper than {max} levels"),
            ));
        }
        if self.skip.is_some() {
            return Ok(());
        }
        let local = local_name(start);
        match self.depth {
            1 => self.open_root(ns, &local),
            2 => {
                self.open_entry(ns, &local);
                Ok(())
            }
            3 => {
                self.open_child(ns, &local, start);
                Ok(())
            }
            _ => {
                self.open_nested(ns, &local);
                Ok(())
            }
        }
    }

    fn open_root(&mut self, ns: &str, local: &str) -> Result<(), Stop> {
        if self.root_closed {
            return Err(self.fatal(
                DiagnosticCode::XmlMalformed,
                "content after the root element",
            ));
        }
        let kind = match (ns, local) {
            (SITEMAP_NS, "urlset") => DocumentKind::Urlset,
            (SITEMAP_NS, "sitemapindex") => DocumentKind::SitemapIndex,
            _ => {
                let shown = if ns.is_empty() { "no namespace" } else { ns };
                return Err(self.fatal(
                    DiagnosticCode::XmlRootInvalid,
                    format!(
                        "root element <{local}> ({shown}) is not urlset or sitemapindex in {SITEMAP_NS}"
                    ),
                ));
            }
        };
        self.report.kind = Some(kind);
        Ok(())
    }

    fn kind(&self) -> DocumentKind {
        self.report.kind.unwrap_or(DocumentKind::Urlset)
    }

    fn open_entry(&mut self, ns: &str, local: &str) {
        let expected = match self.kind() {
            DocumentKind::Urlset => "url",
            DocumentKind::SitemapIndex => "sitemap",
        };
        if ns == SITEMAP_NS && local == expected {
            self.report.entries += 1;
            self.entry = Some(EntryBuilder::default());
            self.check_entry_limit();
        } else if ns == SITEMAP_NS {
            self.warn(
                DiagnosticCode::XmlUnexpectedElement,
                format!("unexpected <{local}> inside <{}>", self.kind().as_str()),
                None,
            );
            self.skip = Some(self.depth);
        } else {
            self.count_extension(ns);
            self.skip = Some(self.depth);
        }
    }

    fn check_entry_limit(&mut self) {
        let (limit, code) = match self.kind() {
            DocumentKind::Urlset => (
                MAX_URLS_PER_SITEMAP,
                DiagnosticCode::SitemapUrlLimitExceeded,
            ),
            DocumentKind::SitemapIndex => {
                (MAX_SITEMAPS_PER_INDEX, DiagnosticCode::IndexLimitExceeded)
            }
        };
        if self.report.entries > u64::from(limit) && !self.limit_reported {
            self.limit_reported = true;
            self.report.diagnostics.push(Diagnostic::new(
                code,
                format!("more than {limit} entries in one document"),
                Location::default(),
            ));
        }
    }

    fn count_extension(&mut self, ns: &str) {
        let key = if ns.is_empty() { "(no namespace)" } else { ns };
        *self
            .report
            .unsupported_extensions
            .entry(key.to_owned())
            .or_insert(0) += 1;
    }

    fn open_child(&mut self, ns: &str, local: &str, start: &BytesStart<'_>) {
        let kind = self.kind();
        let depth = self.depth;
        let Some(entry) = self.entry.as_mut() else {
            self.skip = Some(depth);
            return;
        };
        if ns == SITEMAP_NS {
            let Some((field, position)) = Field::core(local, kind) else {
                self.skip = Some(depth);
                self.warn(
                    DiagnosticCode::XmlUnexpectedElement,
                    format!("unexpected <{local}> in an entry"),
                    Some(local),
                );
                return;
            };
            if entry.seen & (1 << position) != 0 {
                self.skip = Some(depth);
                self.warn(
                    DiagnosticCode::XmlUnexpectedElement,
                    format!("<{local}> is repeated; only the first one is used"),
                    Some(local),
                );
                return;
            }
            let out_of_order =
                entry.extension_seen || entry.last_core.is_some_and(|last| position < last);
            entry.seen |= 1 << position;
            entry.last_core = Some(position);
            if out_of_order && !entry.order_reported {
                entry.order_reported = true;
                self.warn(
                    DiagnosticCode::XmlElementOrder,
                    format!(
                        "<{local}> is out of the XSD order (loc, lastmod, changefreq, priority, then extensions)"
                    ),
                    Some(local),
                );
            }
            self.field = Some((field, depth));
            self.text.clear();
            return;
        }
        entry.extension_seen = true;
        match (kind, ns, local) {
            (DocumentKind::Urlset, XHTML_NS, "link") => {
                let mut rel = None;
                let mut alternate = RawAlternate::default();
                for attribute in start.attributes().flatten() {
                    let Ok(value) = attribute.normalized_value(XmlVersion::Implicit1_0) else {
                        continue;
                    };
                    match attribute.key.local_name().as_ref() {
                        "rel" => rel = Some(value.into_owned()),
                        "hreflang" => alternate.hreflang = Some(value.into_owned()),
                        "href" => alternate.href = Some(value.into_owned()),
                        _ => {}
                    }
                }
                if rel.as_deref() == Some("alternate") {
                    entry.raw.alternates.push(alternate);
                } else {
                    self.warn(
                        DiagnosticCode::XmlUnexpectedElement,
                        "xhtml:link without rel=\"alternate\" is ignored".to_owned(),
                        Some("xhtml:link"),
                    );
                }
                // Children of xhtml:link are meaningless.
                self.skip = Some(depth + 1);
            }
            (DocumentKind::Urlset, IMAGE_NS, "image") => {
                entry.image = Some(None);
            }
            _ => {
                self.count_extension(ns);
                self.skip = Some(depth);
            }
        }
    }

    fn open_nested(&mut self, ns: &str, local: &str) {
        let depth = self.depth;
        let in_image = self.entry.as_ref().is_some_and(|e| e.image.is_some());
        if depth == 4 && in_image && self.field.is_none() && ns == IMAGE_NS && local == "loc" {
            self.field = Some((Field::ImageLoc, depth));
            self.text.clear();
            return;
        }
        if self.field.is_some() {
            self.warn(
                DiagnosticCode::XmlUnexpectedElement,
                format!("unexpected element <{local}> inside a text field"),
                None,
            );
        }
        // Other image children (caption, title, ...) were retired by Google
        // in 2022 and are ignored.
        self.skip = Some(depth);
    }

    fn close(&mut self, ns: &str, local: &str) -> Result<(), Stop> {
        let depth = self.depth;
        self.depth = depth.saturating_sub(1);
        if let Some(skip) = self.skip {
            if depth > skip {
                return Ok(());
            }
            if depth == skip {
                self.skip = None;
                return Ok(());
            }
            self.skip = None;
        }
        if let Some((field, field_depth)) = self.field {
            if field_depth == depth {
                self.field = None;
                return self.finish_field(field);
            }
        }
        match depth {
            1 => self.root_closed = true,
            2 => self.finish_entry(),
            3 if ns == IMAGE_NS && local == "image" => {
                if let Some(entry) = self.entry.as_mut() {
                    if let Some(loc) = entry.image.take() {
                        entry.raw.images.push(RawImage { loc });
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finish_field(&mut self, field: Field) -> Result<(), Stop> {
        let text = std::mem::take(&mut self.text);
        if text.chars().any(forbidden_xml_char) {
            return Err(self.fatal(
                DiagnosticCode::XmlMalformed,
                format!(
                    "<{}> contains characters forbidden by XML 1.0",
                    field.name()
                ),
            ));
        }
        let value = text.trim_matches(|c| matches!(c, ' ' | '\t' | '\r' | '\n'));
        let Some(entry) = self.entry.as_mut() else {
            return Ok(());
        };
        match field {
            Field::Loc => entry.raw.loc = Some(value.to_owned()),
            Field::Lastmod => entry.raw.lastmod = Some(value.to_owned()),
            Field::Changefreq => entry.raw.changefreq = Some(value.to_owned()),
            Field::ImageLoc => {
                if let Some(image) = entry.image.as_mut() {
                    *image = Some(value.to_owned());
                }
            }
            Field::Priority => match value.parse::<f64>() {
                Ok(number) if number.is_finite() => entry.raw.priority = Some(number),
                _ => {
                    let diagnostic = Diagnostic::new(
                        DiagnosticCode::PriorityInvalid,
                        "priority is not a number",
                        Location::record(self.report.entries, None).with_field("priority"),
                    )
                    .with_value(value)
                    .with_suggestion("Use a decimal number between 0.0 and 1.0.");
                    self.report.diagnostics.push(diagnostic);
                    entry.raw.priority = None;
                }
            },
        }
        Ok(())
    }

    fn finish_entry(&mut self) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        let location = Location::record(self.report.entries, None);
        let had_priority_error = self.report.diagnostics.last().is_some_and(|d| {
            d.code == DiagnosticCode::PriorityInvalid && d.location.record == location.record
        });
        let item = match self.kind() {
            DocumentKind::Urlset => self
                .validator
                .validate(&entry.raw, &location, &mut self.report.diagnostics)
                .filter(|_| !had_priority_error)
                .map(Item::Url),
            DocumentKind::SitemapIndex => {
                let raw = RawUrlRecord {
                    loc: entry.raw.loc,
                    lastmod: entry.raw.lastmod,
                    ..RawUrlRecord::default()
                };
                self.index_validator
                    .validate(&raw, &location, &mut self.report.diagnostics)
                    .map(|url| {
                        Item::Sitemap(IndexEntry {
                            loc: url.loc,
                            lastmod: url.lastmod,
                        })
                    })
            }
        };
        let Some(item) = item else {
            return;
        };
        let loc = match &item {
            Item::Url(url) => url.loc.as_str(),
            Item::Sitemap(sitemap) => sitemap.loc.as_str(),
        };
        if self.options.dedupe && !self.seen.insert(fingerprint(loc)) {
            self.report.duplicates += 1;
            let diagnostic = Diagnostic::new(
                DiagnosticCode::DuplicateUrl,
                "URL already listed earlier in the document",
                location.with_field("loc"),
            )
            .with_value(loc);
            self.report.diagnostics.push(diagnostic);
            return;
        }
        self.report.valid_entries += 1;
        match &item {
            Item::Url(url) => {
                self.report.with_lastmod += u64::from(url.lastmod.is_some());
                self.report.with_alternates += u64::from(!url.alternates.is_empty());
                self.report.with_images += u64::from(!url.images.is_empty());
            }
            Item::Sitemap(sitemap) => {
                self.report.with_lastmod += u64::from(sitemap.lastmod.is_some());
            }
        }
        (self.on_item)(item);
    }
}

fn local_name(start: &BytesStart<'_>) -> String {
    start.local_name().as_ref().to_owned()
}

/// Characters XML 1.0 forbids in a document.
fn forbidden_xml_char(c: char) -> bool {
    matches!(c, '\u{0}'..='\u{8}' | '\u{B}' | '\u{C}' | '\u{E}'..='\u{1F}' | '\u{FFFE}' | '\u{FFFF}')
}

/// 128-bit in-process fingerprint, as used by the generator.
fn fingerprint(text: &str) -> u128 {
    let mut a = DefaultHasher::new();
    0u8.hash(&mut a);
    text.hash(&mut a);
    let mut b = DefaultHasher::new();
    1u8.hash(&mut b);
    text.hash(&mut b);
    (u128::from(a.finish()) << 64) | u128::from(b.finish())
}

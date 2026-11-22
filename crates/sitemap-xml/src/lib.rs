//! Streaming XML serialization of sitemaps (`<urlset>`) and sitemap indexes
//! (`<sitemapindex>`).
//!
//! Output is deterministic (ADR-0006): UTF-8, `\n` line endings, two-space
//! indentation, elements in the order of the sitemaps.org XML schema (`loc`,
//! `lastmod`, `changefreq`, `priority`, then extension elements). Writers count
//! the bytes they emit so callers can enforce file size limits exactly.

#![forbid(unsafe_code)]

use std::fmt;
use std::io::{self, Write};

use sitemap_model::{IndexEntry, News, UrlEntry, Video};

/// The XML declaration that starts every document.
pub const XML_DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
/// Namespace of the sitemap protocol.
pub const SITEMAP_NS: &str = "http://www.sitemaps.org/schemas/sitemap/0.9";
/// Namespace of `xhtml:link` (hreflang alternates).
pub const XHTML_NS: &str = "http://www.w3.org/1999/xhtml";
/// Namespace of Google's image extension.
pub const IMAGE_NS: &str = "http://www.google.com/schemas/sitemap-image/1.1";
/// Namespace of Google's video extension.
pub const VIDEO_NS: &str = "http://www.google.com/schemas/sitemap-video/1.1";
/// Namespace of Google's news extension.
pub const NEWS_NS: &str = "http://www.google.com/schemas/sitemap-news/0.9";
/// Closing tag of a `<urlset>` document.
pub const URLSET_FOOTER: &str = "</urlset>\n";
/// Closing tag of a `<sitemapindex>` document.
pub const INDEX_FOOTER: &str = "</sitemapindex>\n";

/// Errors raised while writing XML.
#[derive(Debug)]
pub enum XmlError {
    /// The underlying writer failed.
    Io(io::Error),
    /// Text contains a character that XML 1.0 forbids.
    ForbiddenCharacter {
        /// The character.
        ch: char,
    },
    /// An entry uses an extension whose namespace the document did not declare.
    UndeclaredNamespace {
        /// The namespace prefix.
        prefix: &'static str,
    },
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            XmlError::Io(error) => write!(f, "write failed: {error}"),
            XmlError::ForbiddenCharacter { ch } => {
                write!(
                    f,
                    "character U+{:04X} is not allowed in XML 1.0",
                    u32::from(*ch)
                )
            }
            XmlError::UndeclaredNamespace { prefix } => {
                write!(f, "namespace '{prefix}' is used but was not declared")
            }
        }
    }
}

impl std::error::Error for XmlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            XmlError::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for XmlError {
    fn from(error: io::Error) -> Self {
        XmlError::Io(error)
    }
}

/// Extension namespaces declared on a `<urlset>` element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Namespaces {
    /// `xmlns:xhtml`, required by alternates.
    pub xhtml: bool,
    /// `xmlns:image`, required by images.
    pub image: bool,
    /// `xmlns:video`, required by videos.
    pub video: bool,
    /// `xmlns:news`, required by news metadata.
    pub news: bool,
}

impl Namespaces {
    /// No extension namespace.
    pub const NONE: Namespaces = Namespaces {
        xhtml: false,
        image: false,
        video: false,
        news: false,
    };
    /// The namespaces the engine declares by default on every streamed
    /// file: `xhtml` and `image`. Other extensions are declared on demand
    /// (`output.extensions`), so that existing output stays byte-identical.
    pub const DEFAULT: Namespaces = Namespaces {
        xhtml: true,
        image: true,
        video: false,
        news: false,
    };
    /// Every extension supported by this version.
    pub const ALL: Namespaces = Namespaces {
        xhtml: true,
        image: true,
        video: true,
        news: true,
    };

    /// Namespaces needed by one entry.
    pub fn required_by(entry: &UrlEntry) -> Namespaces {
        Namespaces {
            xhtml: !entry.alternates.is_empty(),
            image: !entry.images.is_empty(),
            video: !entry.videos.is_empty(),
            news: entry.news.is_some(),
        }
    }

    /// Prefixes used by `needed` that `self` does not declare.
    pub fn missing(self, needed: Namespaces) -> Vec<&'static str> {
        let mut prefixes = Vec::new();
        if needed.xhtml && !self.xhtml {
            prefixes.push("xhtml");
        }
        if needed.image && !self.image {
            prefixes.push("image");
        }
        if needed.video && !self.video {
            prefixes.push("video");
        }
        if needed.news && !self.news {
            prefixes.push("news");
        }
        prefixes
    }

    /// Namespaces needed by a set of entries.
    pub fn required_by_all<'a>(entries: impl IntoIterator<Item = &'a UrlEntry>) -> Namespaces {
        entries.into_iter().fold(Namespaces::NONE, |acc, entry| {
            acc.union(Namespaces::required_by(entry))
        })
    }

    /// Union of two sets.
    pub fn union(self, other: Namespaces) -> Namespaces {
        Namespaces {
            xhtml: self.xhtml || other.xhtml,
            image: self.image || other.image,
            video: self.video || other.video,
            news: self.news || other.news,
        }
    }

    /// Checks that `self` declares everything `needed` uses.
    pub fn check_covers(self, needed: Namespaces) -> Result<(), XmlError> {
        match self.missing(needed).first() {
            Some(prefix) => Err(XmlError::UndeclaredNamespace { prefix }),
            None => Ok(()),
        }
    }
}

/// The declaration and opening `<urlset>` tag.
pub fn urlset_header(namespaces: Namespaces) -> String {
    let mut header = String::with_capacity(256);
    header.push_str(XML_DECLARATION);
    header.push_str("<urlset xmlns=\"");
    header.push_str(SITEMAP_NS);
    header.push('"');
    if namespaces.xhtml {
        header.push_str(" xmlns:xhtml=\"");
        header.push_str(XHTML_NS);
        header.push('"');
    }
    if namespaces.image {
        header.push_str(" xmlns:image=\"");
        header.push_str(IMAGE_NS);
        header.push('"');
    }
    if namespaces.video {
        header.push_str(" xmlns:video=\"");
        header.push_str(VIDEO_NS);
        header.push('"');
    }
    if namespaces.news {
        header.push_str(" xmlns:news=\"");
        header.push_str(NEWS_NS);
        header.push('"');
    }
    header.push_str(">\n");
    header
}

/// The declaration and opening `<sitemapindex>` tag.
pub fn index_header() -> String {
    format!("{XML_DECLARATION}<sitemapindex xmlns=\"{SITEMAP_NS}\">\n")
}

/// Appends `text` to `out` with the five XML special characters escaped.
///
/// Fails on characters that XML 1.0 forbids (C0 controls other than tab, line
/// feed and carriage return, and U+FFFE / U+FFFF); nothing is written then.
pub fn escape_into(out: &mut Vec<u8>, text: &str) -> Result<(), XmlError> {
    if let Some(ch) = text.chars().find(|&ch| is_forbidden(ch)) {
        return Err(XmlError::ForbiddenCharacter { ch });
    }
    let mut last = 0;
    for (i, byte) in text.bytes().enumerate() {
        let replacement: &[u8] = match byte {
            b'&' => b"&amp;",
            b'<' => b"&lt;",
            b'>' => b"&gt;",
            b'"' => b"&quot;",
            b'\'' => b"&apos;",
            _ => continue,
        };
        out.extend_from_slice(&text.as_bytes()[last..i]);
        out.extend_from_slice(replacement);
        last = i + 1;
    }
    out.extend_from_slice(&text.as_bytes()[last..]);
    Ok(())
}

fn is_forbidden(ch: char) -> bool {
    sitemap_model::is_xml_forbidden(ch)
}

fn element(out: &mut Vec<u8>, indent: &str, name: &str, text: &str) -> Result<(), XmlError> {
    out.extend_from_slice(indent.as_bytes());
    out.push(b'<');
    out.extend_from_slice(name.as_bytes());
    out.push(b'>');
    escape_into(out, text)?;
    out.extend_from_slice(b"</");
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(b">\n");
    Ok(())
}

/// Appends the complete `<url>` block of `entry` to `out`.
///
/// On error, `out` may contain a partial block; callers reuse a scratch buffer
/// and discard it.
pub fn encode_url(entry: &UrlEntry, out: &mut Vec<u8>) -> Result<(), XmlError> {
    out.extend_from_slice(b"  <url>\n");
    element(out, "    ", "loc", entry.loc.as_str())?;
    if let Some(lastmod) = &entry.lastmod {
        element(out, "    ", "lastmod", lastmod.as_str())?;
    }
    if let Some(changefreq) = entry.changefreq {
        element(out, "    ", "changefreq", changefreq.as_str())?;
    }
    if let Some(priority) = entry.priority {
        element(out, "    ", "priority", &priority.to_xml_string())?;
    }
    for alternate in &entry.alternates {
        out.extend_from_slice(b"    <xhtml:link rel=\"alternate\" hreflang=\"");
        escape_into(out, alternate.hreflang.as_str())?;
        out.extend_from_slice(b"\" href=\"");
        escape_into(out, alternate.href.as_str())?;
        out.extend_from_slice(b"\"/>\n");
    }
    for image in &entry.images {
        out.extend_from_slice(b"    <image:image>\n");
        element(out, "      ", "image:loc", image.loc.as_str())?;
        out.extend_from_slice(b"    </image:image>\n");
    }
    for video in &entry.videos {
        encode_video(video, out)?;
    }
    if let Some(news) = &entry.news {
        encode_news(news, out)?;
    }
    out.extend_from_slice(b"  </url>\n");
    Ok(())
}

/// Writes `<news:news>` in the order of Google's `sitemap-news/0.9` XSD.
fn encode_news(news: &News, out: &mut Vec<u8>) -> Result<(), XmlError> {
    out.extend_from_slice(b"    <news:news>\n      <news:publication>\n");
    element(out, "        ", "news:name", &news.publication_name)?;
    element(out, "        ", "news:language", news.language.as_str())?;
    out.extend_from_slice(b"      </news:publication>\n");
    element(
        out,
        "      ",
        "news:publication_date",
        news.publication_date.as_str(),
    )?;
    element(out, "      ", "news:title", &news.title)?;
    out.extend_from_slice(b"    </news:news>\n");
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// Writes `<video:video>` with children in the order of Google's
/// `sitemap-video/1.1` XSD sequence.
fn encode_video(video: &Video, out: &mut Vec<u8>) -> Result<(), XmlError> {
    const IN: &str = "      ";
    out.extend_from_slice(b"    <video:video>\n");
    element(out, IN, "video:thumbnail_loc", video.thumbnail_loc.as_str())?;
    element(out, IN, "video:title", &video.title)?;
    element(out, IN, "video:description", &video.description)?;
    if let Some(url) = &video.content_loc {
        element(out, IN, "video:content_loc", url.as_str())?;
    }
    if let Some(url) = &video.player_loc {
        element(out, IN, "video:player_loc", url.as_str())?;
    }
    if let Some(duration) = video.duration {
        element(out, IN, "video:duration", &duration.to_string())?;
    }
    if let Some(date) = &video.expiration_date {
        element(out, IN, "video:expiration_date", date.as_str())?;
    }
    if let Some(rating) = video.rating_xml() {
        element(out, IN, "video:rating", &rating)?;
    }
    if let Some(count) = video.view_count {
        element(out, IN, "video:view_count", &count.to_string())?;
    }
    if let Some(date) = &video.publication_date {
        element(out, IN, "video:publication_date", date.as_str())?;
    }
    for tag in &video.tags {
        element(out, IN, "video:tag", tag)?;
    }
    if let Some(value) = video.family_friendly {
        element(out, IN, "video:family_friendly", yes_no(value))?;
    }
    if let Some(restriction) = &video.restriction {
        let countries: Vec<&str> = restriction.countries.iter().map(|c| c.as_str()).collect();
        out.extend_from_slice(IN.as_bytes());
        out.extend_from_slice(b"<video:restriction relationship=\"");
        out.extend_from_slice(restriction.relationship.as_str().as_bytes());
        out.extend_from_slice(b"\">");
        escape_into(out, &countries.join(" "))?;
        out.extend_from_slice(b"</video:restriction>\n");
    }
    if let Some(value) = video.requires_subscription {
        element(out, IN, "video:requires_subscription", yes_no(value))?;
    }
    if let Some(uploader) = &video.uploader {
        out.extend_from_slice(IN.as_bytes());
        out.extend_from_slice(b"<video:uploader");
        if let Some(info) = &uploader.info {
            out.extend_from_slice(b" info=\"");
            escape_into(out, info.as_str())?;
            out.push(b'"');
        }
        out.push(b'>');
        escape_into(out, &uploader.name)?;
        out.extend_from_slice(b"</video:uploader>\n");
    }
    if let Some(platform) = &video.platform {
        let platforms: Vec<&str> = platform.platforms.iter().map(|p| p.as_str()).collect();
        out.extend_from_slice(IN.as_bytes());
        out.extend_from_slice(b"<video:platform relationship=\"");
        out.extend_from_slice(platform.relationship.as_str().as_bytes());
        out.extend_from_slice(b"\">");
        escape_into(out, &platforms.join(" "))?;
        out.extend_from_slice(b"</video:platform>\n");
    }
    if let Some(value) = video.live {
        element(out, IN, "video:live", yes_no(value))?;
    }
    out.extend_from_slice(b"    </video:video>\n");
    Ok(())
}

/// Appends the `<sitemap>` block of an index entry to `out`.
pub fn encode_index_entry(entry: &IndexEntry, out: &mut Vec<u8>) -> Result<(), XmlError> {
    out.extend_from_slice(b"  <sitemap>\n");
    element(out, "    ", "loc", entry.loc.as_str())?;
    if let Some(lastmod) = &entry.lastmod {
        element(out, "    ", "lastmod", lastmod.as_str())?;
    }
    out.extend_from_slice(b"  </sitemap>\n");
    Ok(())
}

/// Writes a `<urlset>` document incrementally.
///
/// The header is written by [`UrlsetWriter::new`], the footer by
/// [`UrlsetWriter::finish`]. Byte and URL counts include everything written.
pub struct UrlsetWriter<W: Write> {
    inner: W,
    namespaces: Namespaces,
    scratch: Vec<u8>,
    urls: u64,
    bytes: u64,
}

impl<W: Write> UrlsetWriter<W> {
    /// Writes the header and returns the writer.
    pub fn new(mut inner: W, namespaces: Namespaces) -> Result<Self, XmlError> {
        let header = urlset_header(namespaces);
        inner.write_all(header.as_bytes())?;
        Ok(Self {
            inner,
            namespaces,
            scratch: Vec::with_capacity(1024),
            urls: 0,
            bytes: header.len() as u64,
        })
    }

    /// Serializes and writes one entry.
    pub fn write_entry(&mut self, entry: &UrlEntry) -> Result<(), XmlError> {
        self.namespaces
            .check_covers(Namespaces::required_by(entry))?;
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        let result = encode_url(entry, &mut scratch).and_then(|()| self.write_block(&scratch));
        self.scratch = scratch;
        result
    }

    /// Writes a block previously produced by [`encode_url`]. The caller is
    /// responsible for having checked the namespaces of its entry.
    pub fn write_encoded(&mut self, block: &[u8]) -> Result<(), XmlError> {
        self.write_block(block)
    }

    fn write_block(&mut self, block: &[u8]) -> Result<(), XmlError> {
        self.inner.write_all(block)?;
        self.urls += 1;
        self.bytes += block.len() as u64;
        Ok(())
    }

    /// Declared namespaces.
    pub fn namespaces(&self) -> Namespaces {
        self.namespaces
    }

    /// URLs written so far.
    pub fn urls(&self) -> u64 {
        self.urls
    }

    /// Bytes written so far (without the footer).
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Writes the footer and returns the inner writer and the total size.
    pub fn finish(mut self) -> Result<(W, u64), XmlError> {
        self.inner.write_all(URLSET_FOOTER.as_bytes())?;
        self.inner.flush()?;
        Ok((self.inner, self.bytes + URLSET_FOOTER.len() as u64))
    }
}

/// Writes a complete `<sitemapindex>` document and returns its size in bytes.
pub fn write_sitemap_index<W: Write>(mut out: W, entries: &[IndexEntry]) -> Result<u64, XmlError> {
    let mut buffer = index_header().into_bytes();
    for entry in entries {
        encode_index_entry(entry, &mut buffer)?;
    }
    buffer.extend_from_slice(INDEX_FOOTER.as_bytes());
    out.write_all(&buffer)?;
    out.flush()?;
    Ok(buffer.len() as u64)
}

/// Serializes a complete `<urlset>` document in memory, declaring exactly the
/// namespaces the entries use.
pub fn urlset_to_vec(entries: &[UrlEntry]) -> Result<Vec<u8>, XmlError> {
    let namespaces = Namespaces::required_by_all(entries);
    let mut writer = UrlsetWriter::new(Vec::new(), namespaces)?;
    for entry in entries {
        writer.write_entry(entry)?;
    }
    Ok(writer.finish()?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sitemap_model::{ChangeFreq, LanguageTag, Priority, SitemapUrl, W3cDateTime};

    fn url(s: &str) -> SitemapUrl {
        SitemapUrl::new(s).unwrap()
    }

    #[test]
    fn escapes_special_characters() {
        let mut out = Vec::new();
        escape_into(&mut out, "a&b<c>d\"e'f").unwrap();
        assert_eq!(out, b"a&amp;b&lt;c&gt;d&quot;e&apos;f");
    }

    #[test]
    fn rejects_forbidden_characters() {
        for ch in ['\u{0}', '\u{1}', '\u{B}', '\u{1F}', '\u{FFFE}', '\u{FFFF}'] {
            let mut out = Vec::new();
            let error = escape_into(&mut out, &format!("a{ch}b")).unwrap_err();
            assert!(matches!(error, XmlError::ForbiddenCharacter { ch: c } if c == ch));
            assert!(out.is_empty());
        }
        let mut out = Vec::new();
        escape_into(&mut out, "tab\tlf\ncr\r é 😀").unwrap();
    }

    #[test]
    fn serializes_full_entry_in_schema_order() {
        let entry = UrlEntry::new(url("https://example.com/fr?a=1&b=2"))
            .with_lastmod(W3cDateTime::parse("2026-11-21").unwrap())
            .with_changefreq(ChangeFreq::Weekly)
            .with_priority(Priority::new(0.8).unwrap())
            .with_alternate(
                LanguageTag::parse("fr").unwrap(),
                url("https://example.com/fr?a=1&b=2"),
            )
            .with_alternate(
                LanguageTag::parse("x-default").unwrap(),
                url("https://example.com/"),
            )
            .with_image(url("https://cdn.example.com/a.png"));
        let xml = String::from_utf8(urlset_to_vec(&[entry]).unwrap()).unwrap();
        let expected = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\" xmlns:image=\"http://www.google.com/schemas/sitemap-image/1.1\">\n",
            "  <url>\n",
            "    <loc>https://example.com/fr?a=1&amp;b=2</loc>\n",
            "    <lastmod>2026-11-21</lastmod>\n",
            "    <changefreq>weekly</changefreq>\n",
            "    <priority>0.8</priority>\n",
            "    <xhtml:link rel=\"alternate\" hreflang=\"fr\" href=\"https://example.com/fr?a=1&amp;b=2\"/>\n",
            "    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"https://example.com/\"/>\n",
            "    <image:image>\n",
            "      <image:loc>https://cdn.example.com/a.png</image:loc>\n",
            "    </image:image>\n",
            "  </url>\n",
            "</urlset>\n",
        );
        assert_eq!(xml, expected);
    }

    #[test]
    fn declares_only_needed_namespaces() {
        let xml = urlset_to_vec(&[UrlEntry::new(url("https://example.com/"))]).unwrap();
        let xml = String::from_utf8(xml).unwrap();
        assert!(!xml.contains("xmlns:xhtml"));
        assert!(!xml.contains("xmlns:image"));
    }

    #[test]
    fn refuses_undeclared_namespace() {
        let entry =
            UrlEntry::new(url("https://example.com/")).with_image(url("https://example.com/a.png"));
        let mut writer = UrlsetWriter::new(Vec::new(), Namespaces::NONE).unwrap();
        assert!(matches!(
            writer.write_entry(&entry),
            Err(XmlError::UndeclaredNamespace { prefix: "image" })
        ));
    }

    #[test]
    fn counts_bytes_exactly() {
        let entries = [
            UrlEntry::new(url("https://example.com/a")),
            UrlEntry::new(url("https://example.com/b")),
        ];
        let mut writer = UrlsetWriter::new(Vec::new(), Namespaces::NONE).unwrap();
        for entry in &entries {
            writer.write_entry(entry).unwrap();
        }
        assert_eq!(writer.urls(), 2);
        let (buffer, total) = writer.finish().unwrap();
        assert_eq!(total, buffer.len() as u64);
    }

    #[test]
    fn writes_index() {
        let entries = [
            IndexEntry {
                loc: url("https://example.com/sitemap-1.xml"),
                lastmod: Some(W3cDateTime::parse("2026-11-20").unwrap()),
            },
            IndexEntry {
                loc: url("https://example.com/sitemap-2.xml"),
                lastmod: None,
            },
        ];
        let mut out = Vec::new();
        let size = write_sitemap_index(&mut out, &entries).unwrap();
        assert_eq!(size, out.len() as u64);
        let expected = concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
            "  <sitemap>\n",
            "    <loc>https://example.com/sitemap-1.xml</loc>\n",
            "    <lastmod>2026-11-20</lastmod>\n",
            "  </sitemap>\n",
            "  <sitemap>\n",
            "    <loc>https://example.com/sitemap-2.xml</loc>\n",
            "  </sitemap>\n",
            "</sitemapindex>\n",
        );
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    /// Property-style test: random URLs with reserved characters always yield
    /// a well-formed document whose `<loc>` values round-trip.
    #[test]
    fn random_entries_are_well_formed() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        const ALPHABET: &[u8] = b"abcXYZ019-._~!$&'()*+,;=:@/?%<>\"";
        let mut entries = Vec::new();
        for _ in 0..500 {
            let len = (next() % 40) as usize;
            let path: String = (0..len)
                .map(|_| char::from(ALPHABET[(next() % ALPHABET.len() as u64) as usize]))
                .collect();
            if let Ok(loc) = SitemapUrl::new(&format!("https://example.com/{path}")) {
                entries.push(UrlEntry::new(loc));
            }
        }
        assert!(entries.len() > 400);
        let xml = urlset_to_vec(&entries).unwrap();

        use quick_xml::events::Event;
        let mut reader = quick_xml::Reader::from_reader(xml.as_slice());
        let mut buf = Vec::new();
        let mut locs: Vec<String> = Vec::new();
        let mut in_loc = false;
        loop {
            // Any parse error fails the test: the document must be well-formed.
            match reader.read_event_into(&mut buf).unwrap() {
                Event::Start(e) if e.name().0 == "loc" => {
                    in_loc = true;
                    locs.push(String::new());
                }
                Event::End(e) if e.name().0 == "loc" => in_loc = false,
                Event::Text(t) if in_loc => {
                    let text = t.xml10_content();
                    locs.last_mut().unwrap().push_str(&text);
                }
                Event::GeneralRef(r) if in_loc => {
                    let resolved = match &*r.into_inner() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        other => panic!("unexpected entity {other:?}"),
                    };
                    locs.last_mut().unwrap().push_str(resolved);
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
        let expected: Vec<&str> = entries.iter().map(|e| e.loc.as_str()).collect();
        assert_eq!(locs, expected);
    }
}

#[cfg(test)]
mod video_tests {
    use super::*;
    use sitemap_model::SitemapUrl;

    fn url(s: &str) -> SitemapUrl {
        SitemapUrl::new(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn video_namespace_is_declared_only_when_needed() {
        const { assert!(!Namespaces::DEFAULT.video) };
        let entry = UrlEntry::new(url("https://example.com/w")).with_video(Video::new(
            url("https://example.com/t.jpg"),
            "T",
            "D",
            url("https://cdn.example.com/v.mp4"),
        ));
        let xml =
            String::from_utf8(urlset_to_vec(std::slice::from_ref(&entry)).unwrap_or_default())
                .unwrap_or_default();
        assert!(xml.contains("xmlns:video=\"http://www.google.com/schemas/sitemap-video/1.1\""));
        assert!(
            xml.contains("<video:content_loc>https://cdn.example.com/v.mp4</video:content_loc>")
        );
        assert_eq!(
            Namespaces::DEFAULT
                .check_covers(Namespaces::required_by(&entry))
                .map_err(|e| e.to_string()),
            Err(XmlError::UndeclaredNamespace { prefix: "video" }.to_string())
        );
    }
}

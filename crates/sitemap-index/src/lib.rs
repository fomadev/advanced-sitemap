//! Partitioning and sitemap index planning (ADR-0005).
//!
//! [`Partitioner`] routes validated entries to sitemap files ("partitions")
//! without ever exceeding the configured limits: each `<url>` block is encoded
//! into a scratch buffer first, so its exact size is known before it is
//! written. [`plan_layout`] then decides the public file names and whether a
//! sitemap index is needed.

#![forbid(unsafe_code)]

mod fnv;
mod jump;

use std::fmt;
use std::io;
use std::num::NonZeroU32;

use sitemap_model::limits::{
    MAX_NEWS_PER_SITEMAP, MAX_SITEMAP_BYTES, MAX_SITEMAPS_PER_INDEX, MAX_URLS_PER_SITEMAP,
};
use sitemap_model::{IndexEntry, SitemapUrl, UrlEntry, W3cDateTime};
use sitemap_storage::{ArtifactName, ArtifactStore, ArtifactWriter, Publication};
use sitemap_xml::{Namespaces, URLSET_FOOTER, UrlsetWriter, XmlError, encode_url, urlset_header};

pub use fnv::fnv1a64;

/// Digest of a published file: the first 16 bytes of its BLAKE3 hash
/// (same function as the snapshot's `digest128`).
pub type FileDigest = [u8; 16];

/// Passes writes through and hashes every byte written (BLAKE3), so that
/// the digest of a published file is known without reading it back.
pub struct HashingWriter<W> {
    inner: W,
    hasher: blake3::Hasher,
}

impl<W: io::Write> HashingWriter<W> {
    /// Wraps `inner`.
    pub fn new(inner: W) -> Self {
        Self {
            inner,
            hasher: blake3::Hasher::new(),
        }
    }

    /// Returns the inner writer and the digest of everything written.
    pub fn into_parts(self) -> (W, FileDigest) {
        let mut digest = [0u8; 16];
        digest.copy_from_slice(&self.hasher.finalize().as_bytes()[..16]);
        (self.inner, digest)
    }
}

impl<W: io::Write> io::Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
pub use jump::jump_consistent_hash;

/// How entries are assigned to partitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PartitionStrategy {
    /// Fill partitions in input order; open a new one when a limit is reached.
    Sequential,
    /// `partition = fnv1a64(loc) mod partitions`. A URL keeps its partition as
    /// long as the partition count is unchanged. A full partition is an error.
    StableHash {
        /// Number of partitions.
        partitions: NonZeroU32,
    },
    /// `partition = jump_consistent_hash(fnv1a64(loc), partitions)` (ADR-0012).
    /// Like `StableHash` while the count is unchanged; growing it from N to
    /// N+1 moves about 1/(N+1) of the URLs, all to the new partition. A full
    /// partition is an error.
    ConsistentHash {
        /// Number of partitions.
        partitions: NonZeroU32,
    },
}

impl PartitionStrategy {
    /// Partition count of a hash strategy, `None` for `Sequential`.
    pub fn hash_partitions(self) -> Option<NonZeroU32> {
        match self {
            PartitionStrategy::Sequential => None,
            PartitionStrategy::StableHash { partitions }
            | PartitionStrategy::ConsistentHash { partitions } => Some(partitions),
        }
    }

    /// Protocol name: `sequential`, `stable-hash` or `consistent-hash`.
    pub fn name(self) -> &'static str {
        match self {
            PartitionStrategy::Sequential => "sequential",
            PartitionStrategy::StableHash { .. } => "stable-hash",
            PartitionStrategy::ConsistentHash { .. } => "consistent-hash",
        }
    }

    /// 1-based partition of `loc` under a hash strategy, `None` for
    /// `Sequential` (which depends on input order).
    pub fn hash_partition(self, loc: &str) -> Option<u32> {
        let key = fnv1a64(loc.as_bytes());
        match self {
            PartitionStrategy::Sequential => None,
            PartitionStrategy::StableHash { partitions } => {
                Some((key % u64::from(partitions.get())) as u32 + 1)
            }
            PartitionStrategy::ConsistentHash { partitions } => {
                Some(jump_consistent_hash(key, partitions.get()) + 1)
            }
        }
    }
}

/// Per-file limits. Lower them to keep a safety margin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartitionLimits {
    max_urls: u32,
    max_bytes: u64,
}

/// Invalid partition limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LimitsError(String);

impl fmt::Display for LimitsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LimitsError {}

/// Smallest accepted byte limit: room for the header, the footer and a few entries.
pub const MIN_PARTITION_BYTES: u64 = 4_096;

impl PartitionLimits {
    /// Validates limits: `1..=50_000` URLs and `4_096..=52_428_800` bytes.
    pub fn new(max_urls: u32, max_bytes: u64) -> Result<Self, LimitsError> {
        if max_urls == 0 || max_urls > MAX_URLS_PER_SITEMAP {
            return Err(LimitsError(format!(
                "maxUrls must be between 1 and {MAX_URLS_PER_SITEMAP}, got {max_urls}"
            )));
        }
        if !(MIN_PARTITION_BYTES..=MAX_SITEMAP_BYTES).contains(&max_bytes) {
            return Err(LimitsError(format!(
                "maxBytes must be between {MIN_PARTITION_BYTES} and {MAX_SITEMAP_BYTES}, got {max_bytes}"
            )));
        }
        Ok(Self {
            max_urls,
            max_bytes,
        })
    }

    /// Maximum URLs per file.
    pub fn max_urls(self) -> u32 {
        self.max_urls
    }

    /// Maximum bytes per file.
    pub fn max_bytes(self) -> u64 {
        self.max_bytes
    }
}

impl Default for PartitionLimits {
    fn default() -> Self {
        Self {
            max_urls: MAX_URLS_PER_SITEMAP,
            max_bytes: MAX_SITEMAP_BYTES,
        }
    }
}

/// A closed partition, staged in the store.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PartitionSummary {
    /// 1-based partition number.
    pub id: u32,
    /// Name under which the partition is staged (`<stem>-<id>.xml`).
    pub staged_name: ArtifactName,
    /// Number of URLs.
    pub urls: u64,
    /// Exact size in bytes.
    pub bytes: u64,
    /// Latest `lastmod` of the partition's entries (UTC comparison).
    pub lastmod: Option<W3cDateTime>,
    /// Digest of the file's bytes, when known.
    pub digest: Option<FileDigest>,
}

impl PartitionSummary {
    /// Partition `id`, staged as `staged_name`, holding `urls` URLs in
    /// `bytes` bytes; no `lastmod` and no digest yet.
    pub fn new(id: u32, staged_name: ArtifactName, urls: u64, bytes: u64) -> Self {
        PartitionSummary {
            id,
            staged_name,
            urls,
            bytes,
            lastmod: None,
            digest: None,
        }
    }
}

/// Why an entry could not be placed.
#[derive(Debug)]
#[non_exhaustive]
pub enum PushError {
    /// The entry alone exceeds the byte limit of a file.
    EntryTooLarge {
        /// Size of the document if it contained only this entry.
        bytes: u64,
        /// Configured limit.
        limit: u64,
    },
    /// A hash partition (`stable-hash` or `consistent-hash`) is full.
    PartitionFull {
        /// 1-based partition number.
        partition: u32,
        /// Configured URL limit.
        max_urls: u32,
        /// Configured byte limit.
        max_bytes: u64,
    },
    /// A hash partition already holds 1,000 news entries, the Google
    /// News limit per file.
    NewsLimit {
        /// 1-based partition number.
        partition: u32,
    },
    /// Serialization failed.
    Xml(XmlError),
    /// The store failed.
    Io(io::Error),
}

impl fmt::Display for PushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PushError::EntryTooLarge { bytes, limit } => write!(
                f,
                "a document containing only this entry would be {bytes} bytes, above the {limit}-byte limit"
            ),
            PushError::PartitionFull {
                partition,
                max_urls,
                max_bytes,
            } => write!(
                f,
                "hash partition {partition} is full ({max_urls} URLs or {max_bytes} bytes)"
            ),
            PushError::NewsLimit { partition } => write!(
                f,
                "hash partition {partition} already holds {MAX_NEWS_PER_SITEMAP} news entries"
            ),
            PushError::Xml(error) => write!(f, "{error}"),
            PushError::Io(error) => write!(f, "store error: {error}"),
        }
    }
}

impl std::error::Error for PushError {}

impl From<XmlError> for PushError {
    fn from(error: XmlError) -> Self {
        match error {
            XmlError::Io(error) => PushError::Io(error),
            other => PushError::Xml(other),
        }
    }
}

impl From<io::Error> for PushError {
    fn from(error: io::Error) -> Self {
        PushError::Io(error)
    }
}

struct OpenPartition {
    id: u32,
    name: ArtifactName,
    writer: UrlsetWriter<HashingWriter<Box<dyn ArtifactWriter>>>,
    lastmod: Option<W3cDateTime>,
    /// `<news:news>` elements written.
    news: u32,
}

impl OpenPartition {
    fn close(self) -> Result<PartitionSummary, PushError> {
        let urls = self.writer.urls();
        let (hashing, bytes) = self.writer.finish()?;
        let (artifact, digest) = hashing.into_parts();
        let stored = artifact.finish()?;
        debug_assert_eq!(stored, bytes);
        Ok(PartitionSummary {
            id: self.id,
            staged_name: self.name,
            urls,
            bytes,
            lastmod: self.lastmod,
            digest: Some(digest),
        })
    }
}

/// Routes entries to staged partition files.
pub struct Partitioner<'s> {
    store: &'s mut dyn ArtifactStore,
    strategy: PartitionStrategy,
    limits: PartitionLimits,
    namespaces: Namespaces,
    stem: String,
    header_len: u64,
    slots: Vec<Option<OpenPartition>>,
    closed: Vec<PartitionSummary>,
    next_sequential_id: u32,
    scratch: Vec<u8>,
}

impl<'s> Partitioner<'s> {
    /// A partitioner writing `<stem>-<n>.xml` files into `store`, declaring
    /// `namespaces` on every file.
    pub fn new(
        store: &'s mut dyn ArtifactStore,
        strategy: PartitionStrategy,
        limits: PartitionLimits,
        namespaces: Namespaces,
        stem: &str,
    ) -> Self {
        let slot_count = strategy.hash_partitions().map_or(1, |n| n.get() as usize);
        Self {
            store,
            strategy,
            limits,
            namespaces,
            stem: stem.to_owned(),
            header_len: urlset_header(namespaces).len() as u64,
            slots: (0..slot_count).map(|_| None).collect(),
            closed: Vec::new(),
            next_sequential_id: 1,
            scratch: Vec::with_capacity(1024),
        }
    }

    /// The encoded `<url>` block of the last entry passed to [`push`]
    /// (complete when `push` succeeded).
    ///
    /// [`push`]: Partitioner::push
    pub fn last_block(&self) -> &[u8] {
        &self.scratch
    }

    /// Places one entry. Returns the 1-based partition number.
    pub fn push(&mut self, entry: &UrlEntry) -> Result<u32, PushError> {
        self.namespaces
            .check_covers(Namespaces::required_by(entry))?;
        self.scratch.clear();
        encode_url(entry, &mut self.scratch)?;
        let block = self.scratch.len() as u64;
        let footer = URLSET_FOOTER.len() as u64;

        let alone = self.header_len + block + footer;
        if alone > self.limits.max_bytes {
            return Err(PushError::EntryTooLarge {
                bytes: alone,
                limit: self.limits.max_bytes,
            });
        }

        let slot = self
            .strategy
            .hash_partition(entry.loc.as_str())
            .map_or(0, |partition| partition as usize - 1);

        let is_news = entry.news.is_some();
        if let Some(open) = &self.slots[slot] {
            let news_full = is_news && open.news >= MAX_NEWS_PER_SITEMAP;
            let full = open.writer.urls() >= u64::from(self.limits.max_urls)
                || open.writer.bytes() + block + footer > self.limits.max_bytes
                || news_full;
            if full {
                match self.strategy {
                    PartitionStrategy::Sequential => {
                        if let Some(open) = self.slots[slot].take() {
                            self.closed.push(open.close()?);
                        }
                    }
                    PartitionStrategy::StableHash { .. }
                    | PartitionStrategy::ConsistentHash { .. }
                        if news_full =>
                    {
                        return Err(PushError::NewsLimit { partition: open.id });
                    }
                    PartitionStrategy::StableHash { .. }
                    | PartitionStrategy::ConsistentHash { .. } => {
                        return Err(PushError::PartitionFull {
                            partition: open.id,
                            max_urls: self.limits.max_urls,
                            max_bytes: self.limits.max_bytes,
                        });
                    }
                }
            }
        }

        if self.slots[slot].is_none() {
            let id = match self.strategy {
                PartitionStrategy::Sequential => {
                    let id = self.next_sequential_id;
                    self.next_sequential_id += 1;
                    id
                }
                PartitionStrategy::StableHash { .. } | PartitionStrategy::ConsistentHash { .. } => {
                    slot as u32 + 1
                }
            };
            self.slots[slot] = Some(self.open(id)?);
        }

        let Some(open) = self.slots[slot].as_mut() else {
            return Err(PushError::Io(io::Error::other(
                "partition slot unexpectedly empty",
            )));
        };
        open.writer.write_encoded(&self.scratch)?;
        open.news += u32::from(is_news);
        if let Some(lastmod) = &entry.lastmod {
            let newer = open
                .lastmod
                .as_ref()
                .is_none_or(|current| lastmod.cmp_instant(current).is_gt());
            if newer {
                open.lastmod = Some(lastmod.clone());
            }
        }
        Ok(open.id)
    }

    fn open(&mut self, id: u32) -> Result<OpenPartition, PushError> {
        let name = partition_name(&self.stem, id)?;
        let artifact = self.store.create(&name)?;
        let writer = UrlsetWriter::new(HashingWriter::new(artifact), self.namespaces)?;
        Ok(OpenPartition {
            id,
            name,
            writer,
            lastmod: None,
            news: 0,
        })
    }

    /// Closes every open partition and returns all partitions by id. When no
    /// entry was pushed, a single empty partition is produced so that the
    /// output always contains a valid document.
    pub fn finish(self) -> Result<Vec<PartitionSummary>, PushError> {
        self.close_all(true)
    }

    /// Like [`finish`], but returns no partition when nothing was pushed
    /// (used by the delta mode, which writes only the touched partitions).
    ///
    /// [`finish`]: Partitioner::finish
    pub fn finish_partial(self) -> Result<Vec<PartitionSummary>, PushError> {
        self.close_all(false)
    }

    fn close_all(mut self, empty_fallback: bool) -> Result<Vec<PartitionSummary>, PushError> {
        for slot in &mut self.slots {
            if let Some(open) = slot.take() {
                self.closed.push(open.close()?);
            }
        }
        if self.closed.is_empty() && empty_fallback {
            let empty = self.open(1)?;
            self.closed.push(empty.close()?);
        }
        self.closed.sort_by_key(|p| p.id);
        Ok(self.closed)
    }

    /// Closes a hash partition now instead of at [`finish`], to release its
    /// buffers. Pushing to it again would start the file over, so only
    /// callers that route each partition's entries together (the delta
    /// mode) use it. Does nothing for `Sequential` or an unopened partition.
    ///
    /// [`finish`]: Partitioner::finish
    pub fn close_partition(&mut self, partition: u32) -> Result<(), PushError> {
        if self.strategy.hash_partitions().is_none() {
            return Ok(());
        }
        let slot = partition.checked_sub(1).map(|s| s as usize);
        if let Some(open) = slot
            .and_then(|s| self.slots.get_mut(s))
            .and_then(Option::take)
        {
            self.closed.push(open.close()?);
        }
        Ok(())
    }

    /// The store, for reading published artifacts between pushes.
    pub fn store(&self) -> &dyn ArtifactStore {
        &*self.store
    }
}

/// `<stem>-<id>.xml`
pub fn partition_name(stem: &str, id: u32) -> io::Result<ArtifactName> {
    ArtifactName::new(format!("{stem}-{id}.xml")).map_err(|e| io::Error::other(e.to_string()))
}

/// When to produce a sitemap index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_enums,
    reason = "auto, always and never cover every index policy"
)]
pub enum IndexMode {
    /// Only when there is more than one partition.
    #[default]
    Auto,
    /// Always, even for a single partition.
    Always,
    /// Never; every partition is published under its own name.
    Never,
}

/// Public base URL of the published files (where `<stem>-<n>.xml` is served).
///
/// The path is treated as a directory: a missing trailing slash is added, so
/// `https://example.com/sitemaps` and `https://example.com/sitemaps/` are
/// equivalent. Query strings and fragments are rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicBaseUrl(SitemapUrl);

impl PublicBaseUrl {
    /// Validates a base URL.
    pub fn new(url: SitemapUrl) -> Result<Self, String> {
        let text = url.as_str();
        if text.contains('?') || url.has_fragment() {
            return Err(format!(
                "public base URL {text} must not contain a query string or fragment"
            ));
        }
        if text.ends_with('/') {
            Ok(Self(url))
        } else {
            SitemapUrl::new(&format!("{text}/"))
                .map(Self)
                .map_err(|e| e.to_string())
        }
    }

    /// The base URL (always ending with `/`).
    pub fn url(&self) -> &SitemapUrl {
        &self.0
    }
}

/// Output layout settings.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Layout {
    /// File stem (`sitemap` gives `sitemap.xml`, `sitemap-1.xml`...).
    pub stem: String,
    /// Index policy.
    pub index: IndexMode,
    /// Public base URL, required when an index is produced.
    pub public_base_url: Option<PublicBaseUrl>,
}

impl Layout {
    /// A layout with file stem `stem`, an index only when needed and no
    /// public base URL.
    pub fn new(stem: impl Into<String>) -> Self {
        Layout {
            stem: stem.into(),
            index: IndexMode::Auto,
            public_base_url: None,
        }
    }

    /// Name of the single sitemap or of the index: `<stem>.xml`.
    pub fn main_name(&self) -> io::Result<ArtifactName> {
        ArtifactName::new(format!("{}.xml", self.stem)).map_err(|e| io::Error::other(e.to_string()))
    }
}

/// What to publish.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct LayoutPlan {
    /// Partition publications, in order.
    pub partitions: Vec<Publication>,
    /// The index to write, if any: its name and entries.
    pub index: Option<(ArtifactName, Vec<IndexEntry>)>,
}

/// The layout cannot be produced.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutError {
    /// An index is needed but no public base URL is configured.
    PublicBaseUrlMissing {
        /// Number of partitions.
        partitions: usize,
    },
    /// More partitions than an index may reference.
    IndexLimitExceeded {
        /// Number of partitions.
        partitions: usize,
    },
    /// A file name could not be built.
    InvalidName(String),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::PublicBaseUrlMissing { partitions } => write!(
                f,
                "{partitions} sitemap files require an index, but no public base URL is configured"
            ),
            LayoutError::IndexLimitExceeded { partitions } => write!(
                f,
                "{partitions} sitemap files exceed the {MAX_SITEMAPS_PER_INDEX} files an index may reference"
            ),
            LayoutError::InvalidName(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Decides public names and the index for a set of partitions.
pub fn plan_layout(
    partitions: &[PartitionSummary],
    layout: &Layout,
) -> Result<LayoutPlan, LayoutError> {
    let main = layout
        .main_name()
        .map_err(|e| LayoutError::InvalidName(e.to_string()))?;
    let needs_index = match layout.index {
        IndexMode::Always => true,
        IndexMode::Never => false,
        IndexMode::Auto => partitions.len() > 1,
    };

    if !needs_index {
        let publications = if partitions.len() == 1 && layout.index == IndexMode::Auto {
            vec![Publication::new(partitions[0].staged_name.clone(), main)]
        } else {
            partitions
                .iter()
                .map(|p| Publication::new(p.staged_name.clone(), p.staged_name.clone()))
                .collect()
        };
        return Ok(LayoutPlan {
            partitions: publications,
            index: None,
        });
    }

    if partitions.len() > MAX_SITEMAPS_PER_INDEX as usize {
        return Err(LayoutError::IndexLimitExceeded {
            partitions: partitions.len(),
        });
    }
    let base = layout
        .public_base_url
        .as_ref()
        .ok_or(LayoutError::PublicBaseUrlMissing {
            partitions: partitions.len(),
        })?;

    let mut entries = Vec::with_capacity(partitions.len());
    let mut publications = Vec::with_capacity(partitions.len());
    for partition in partitions {
        let loc = base
            .url()
            .join(partition.staged_name.as_str())
            .map_err(|e| LayoutError::InvalidName(e.to_string()))?;
        entries.push(IndexEntry::new(loc, partition.lastmod.clone()));
        publications.push(Publication::new(
            partition.staged_name.clone(),
            partition.staged_name.clone(),
        ));
    }
    Ok(LayoutPlan {
        partitions: publications,
        index: Some((main, entries)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sitemap_storage::MemoryStore;

    fn entry(i: usize) -> UrlEntry {
        UrlEntry::new(SitemapUrl::new(&format!("https://example.com/page/{i}")).unwrap())
    }

    fn run(
        strategy: PartitionStrategy,
        limits: PartitionLimits,
        count: usize,
    ) -> (Result<Vec<PartitionSummary>, PushError>, MemoryStore) {
        let store = MemoryStore::new();
        let mut handle = store.clone();
        let mut partitioner =
            Partitioner::new(&mut handle, strategy, limits, Namespaces::NONE, "sitemap");
        for i in 0..count {
            if let Err(error) = partitioner.push(&entry(i)) {
                return (Err(error), store);
            }
        }
        (partitioner.finish(), store)
    }

    fn news_entry(i: usize) -> UrlEntry {
        entry(i).with_news(sitemap_model::News::new(
            "The Example Times",
            sitemap_model::NewsLanguage::parse("en").unwrap(),
            W3cDateTime::parse("2026-11-21").unwrap(),
            format!("Article {i}"),
        ))
    }

    fn push_mixed(
        strategy: PartitionStrategy,
        count: usize,
    ) -> Result<Vec<PartitionSummary>, PushError> {
        let mut store = MemoryStore::new();
        let mut namespaces = Namespaces::NONE;
        namespaces.news = true;
        let mut partitioner = Partitioner::new(
            &mut store,
            strategy,
            PartitionLimits::default(),
            namespaces,
            "sitemap",
        );
        for i in 0..count {
            // Two news entries for one plain entry.
            let e = if i % 3 == 2 { entry(i) } else { news_entry(i) };
            partitioner.push(&e)?;
        }
        partitioner.finish()
    }

    #[test]
    fn sequential_starts_a_new_file_after_1000_news() {
        // 3,000 entries: 2,000 news and 1,000 plain.
        let parts = push_mixed(PartitionStrategy::Sequential, 3_000).unwrap();
        let counts: Vec<u64> = parts.iter().map(|p| p.urls).collect();
        // The 1,001st news entry (entry 1,500) opens file 2; plain entries
        // never trigger a split.
        assert_eq!(counts, [1_500, 1_500]);
    }

    #[test]
    fn stable_hash_reports_the_news_limit() {
        let one = PartitionStrategy::StableHash {
            partitions: NonZeroU32::MIN,
        };
        assert!(matches!(
            push_mixed(one, 3_000),
            Err(PushError::NewsLimit { partition: 1 })
        ));
    }

    #[test]
    fn sequential_splits_exactly_on_url_count() {
        let limits = PartitionLimits::new(3, MAX_SITEMAP_BYTES).unwrap();
        let (parts, _) = run(PartitionStrategy::Sequential, limits, 7);
        let parts = parts.unwrap();
        let counts: Vec<u64> = parts.iter().map(|p| p.urls).collect();
        assert_eq!(counts, [3, 3, 1]);
        let names: Vec<&str> = parts.iter().map(|p| p.staged_name.as_str()).collect();
        assert_eq!(names, ["sitemap-1.xml", "sitemap-2.xml", "sitemap-3.xml"]);

        let (parts, _) = run(PartitionStrategy::Sequential, limits, 6);
        assert_eq!(
            parts.unwrap().len(),
            2,
            "exactly full partitions do not create an extra file"
        );
    }

    #[test]
    fn sequential_respects_byte_limit_exactly() {
        // URLs page/1000..page/9999 all have the same encoded length.
        let header = urlset_header(Namespaces::NONE).len() as u64;
        let footer = URLSET_FOOTER.len() as u64;
        let mut sample = Vec::new();
        encode_url(&entry(1000), &mut sample).unwrap();
        let max_bytes = MIN_PARTITION_BYTES;
        let per_file = (max_bytes - header - footer) / sample.len() as u64;
        let limits = PartitionLimits::new(50_000, max_bytes).unwrap();
        let count = (per_file as usize) * 2 + 1;
        let store = MemoryStore::new();
        let mut handle = store.clone();
        let mut partitioner = Partitioner::new(
            &mut handle,
            PartitionStrategy::Sequential,
            limits,
            Namespaces::NONE,
            "sitemap",
        );
        let mut block_len = None;
        for i in 1000..1000 + count {
            let e = entry(i);
            let mut b = Vec::new();
            encode_url(&e, &mut b).unwrap();
            block_len.get_or_insert(b.len());
            assert_eq!(Some(b.len()), block_len);
            partitioner.push(&e).unwrap();
        }
        let parts = partitioner.finish().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].urls, per_file);
        assert_eq!(parts[1].urls, per_file);
        assert_eq!(parts[2].urls, 1);
        for part in &parts {
            assert!(part.bytes <= max_bytes);
        }
        // A full partition plus one more block would have crossed the limit.
        assert!(parts[0].bytes + sample.len() as u64 > max_bytes);
        let published = store.staged_count().unwrap();
        assert_eq!(published, 3);
    }

    #[test]
    fn entry_too_large_is_reported() {
        let limits = PartitionLimits::new(10, MIN_PARTITION_BYTES).unwrap();
        let big = UrlEntry::new(
            SitemapUrl::new(&format!("https://example.com/{}", "a".repeat(2000))).unwrap(),
        )
        .with_alternate(
            sitemap_model::LanguageTag::parse("en").unwrap(),
            SitemapUrl::new(&format!("https://example.com/{}", "b".repeat(2000))).unwrap(),
        );
        let store = MemoryStore::new();
        let mut handle = store.clone();
        let mut partitioner = Partitioner::new(
            &mut handle,
            PartitionStrategy::Sequential,
            limits,
            Namespaces::ALL,
            "s",
        );
        assert!(matches!(
            partitioner.push(&big),
            Err(PushError::EntryTooLarge { .. })
        ));
    }

    #[test]
    fn stable_hash_is_deterministic_and_reports_overflow() {
        let partitions = NonZeroU32::new(4).unwrap();
        let strategy = PartitionStrategy::StableHash { partitions };
        let (a, _) = run(strategy, PartitionLimits::default(), 1000);
        let (b, _) = run(strategy, PartitionLimits::default(), 1000);
        let a = a.unwrap();
        assert_eq!(a, b.unwrap());
        assert_eq!(a.iter().map(|p| p.urls).sum::<u64>(), 1000);
        assert_eq!(a.len(), 4);

        // Each URL lands in fnv1a64(loc) % 4.
        let store = MemoryStore::new();
        let mut handle = store.clone();
        let mut partitioner = Partitioner::new(
            &mut handle,
            strategy,
            PartitionLimits::default(),
            Namespaces::NONE,
            "s",
        );
        for i in 0..50 {
            let e = entry(i);
            let expected = (fnv1a64(e.loc.as_str().as_bytes()) % 4) as u32 + 1;
            assert_eq!(partitioner.push(&e).unwrap(), expected);
        }

        let tight = PartitionLimits::new(100, MAX_SITEMAP_BYTES).unwrap();
        let (overflow, _) = run(strategy, tight, 1000);
        assert!(matches!(overflow, Err(PushError::PartitionFull { .. })));
    }

    #[test]
    fn consistent_hash_places_by_jump_hash() {
        let strategy = PartitionStrategy::ConsistentHash {
            partitions: NonZeroU32::new(5).unwrap(),
        };
        let (a, _) = run(strategy, PartitionLimits::default(), 1000);
        let (b, _) = run(strategy, PartitionLimits::default(), 1000);
        let a = a.unwrap();
        assert_eq!(a, b.unwrap());
        assert_eq!(a.len(), 5);
        assert_eq!(a.iter().map(|p| p.urls).sum::<u64>(), 1000);

        let store = MemoryStore::new();
        let mut handle = store.clone();
        let mut partitioner = Partitioner::new(
            &mut handle,
            strategy,
            PartitionLimits::default(),
            Namespaces::NONE,
            "s",
        );
        for i in 0..50 {
            let e = entry(i);
            let expected = jump_consistent_hash(fnv1a64(e.loc.as_str().as_bytes()), 5) + 1;
            assert_eq!(partitioner.push(&e).unwrap(), expected);
            assert_eq!(strategy.hash_partition(e.loc.as_str()), Some(expected));
        }

        let tight = PartitionLimits::new(100, MAX_SITEMAP_BYTES).unwrap();
        let (overflow, _) = run(strategy, tight, 1000);
        assert!(matches!(overflow, Err(PushError::PartitionFull { .. })));
    }

    #[test]
    fn closing_a_partition_early_keeps_it_in_the_result() {
        let strategy = PartitionStrategy::ConsistentHash {
            partitions: NonZeroU32::new(3).unwrap(),
        };
        let store = MemoryStore::new();
        let mut handle = store.clone();
        let mut partitioner = Partitioner::new(
            &mut handle,
            strategy,
            PartitionLimits::default(),
            Namespaces::NONE,
            "s",
        );
        let mut placed = Vec::new();
        for i in 0..30 {
            placed.push(partitioner.push(&entry(i)).unwrap());
        }
        partitioner.close_partition(placed[0]).unwrap();
        partitioner.close_partition(99).unwrap(); // unknown: ignored
        let parts = partitioner.finish_partial().unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts.iter().map(|p| p.urls).sum::<u64>(), 30);
        assert!(parts.windows(2).all(|w| w[0].id < w[1].id));
    }

    #[test]
    fn strategy_names_and_counts() {
        let n = NonZeroU32::new(3).unwrap();
        assert_eq!(PartitionStrategy::Sequential.name(), "sequential");
        assert_eq!(
            PartitionStrategy::StableHash { partitions: n }.name(),
            "stable-hash"
        );
        assert_eq!(
            PartitionStrategy::ConsistentHash { partitions: n }.name(),
            "consistent-hash"
        );
        assert_eq!(PartitionStrategy::Sequential.hash_partitions(), None);
        assert_eq!(
            PartitionStrategy::Sequential.hash_partition("https://example.com/"),
            None
        );
        assert_eq!(
            PartitionStrategy::StableHash { partitions: n }.hash_partition("https://example.com/"),
            Some((0x0c8b_41cf_dcb3_c914u64 % 3) as u32 + 1)
        );
    }

    #[test]
    fn empty_input_yields_one_empty_partition() {
        let (parts, store) = run(PartitionStrategy::Sequential, PartitionLimits::default(), 0);
        let parts = parts.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].urls, 0);
        assert_eq!(store.staged_count().unwrap(), 1);
    }

    #[test]
    fn lastmod_is_the_latest_instant() {
        let store = MemoryStore::new();
        let mut handle = store.clone();
        let mut partitioner = Partitioner::new(
            &mut handle,
            PartitionStrategy::Sequential,
            PartitionLimits::default(),
            Namespaces::NONE,
            "sitemap",
        );
        for (i, date) in [
            "2026-01-02",
            "2026-03-01T10:00:00+05:00",
            "2026-03-01T04:59:00Z",
        ]
        .iter()
        .enumerate()
        {
            partitioner
                .push(&entry(i).with_lastmod(W3cDateTime::parse(date).unwrap()))
                .unwrap();
        }
        let parts = partitioner.finish().unwrap();
        assert_eq!(
            parts[0].lastmod.as_ref().unwrap().as_str(),
            "2026-03-01T10:00:00+05:00"
        );
    }

    fn summary(id: u32) -> PartitionSummary {
        PartitionSummary {
            id,
            staged_name: partition_name("sitemap", id).unwrap(),
            urls: 1,
            bytes: 100,
            lastmod: None,
            digest: None,
        }
    }

    #[test]
    fn layout_rules() {
        let base =
            PublicBaseUrl::new(SitemapUrl::new("https://example.com/maps").unwrap()).unwrap();
        assert_eq!(base.url().as_str(), "https://example.com/maps/");
        let mut layout = Layout {
            stem: "sitemap".into(),
            index: IndexMode::Auto,
            public_base_url: None,
        };

        let single = plan_layout(&[summary(1)], &layout).unwrap();
        assert!(single.index.is_none());
        assert_eq!(single.partitions[0].target.as_str(), "sitemap.xml");

        assert_eq!(
            plan_layout(&[summary(1), summary(2)], &layout),
            Err(LayoutError::PublicBaseUrlMissing { partitions: 2 })
        );

        layout.public_base_url = Some(base);
        let multi = plan_layout(&[summary(1), summary(2)], &layout).unwrap();
        let (name, entries) = multi.index.unwrap();
        assert_eq!(name.as_str(), "sitemap.xml");
        assert_eq!(
            entries[1].loc.as_str(),
            "https://example.com/maps/sitemap-2.xml"
        );

        layout.index = IndexMode::Always;
        let always = plan_layout(&[summary(1)], &layout).unwrap();
        assert_eq!(always.partitions[0].target.as_str(), "sitemap-1.xml");
        assert!(always.index.is_some());

        layout.index = IndexMode::Never;
        let never = plan_layout(&[summary(1), summary(2)], &layout).unwrap();
        assert!(never.index.is_none());
        assert_eq!(never.partitions[1].target.as_str(), "sitemap-2.xml");

        assert!(PublicBaseUrl::new(SitemapUrl::new("https://example.com/?a=1").unwrap()).is_err());
    }

    #[test]
    fn limits_validation() {
        assert!(PartitionLimits::new(0, MAX_SITEMAP_BYTES).is_err());
        assert!(PartitionLimits::new(50_001, MAX_SITEMAP_BYTES).is_err());
        assert!(PartitionLimits::new(1, MAX_SITEMAP_BYTES + 1).is_err());
        assert!(PartitionLimits::new(1, MIN_PARTITION_BYTES - 1).is_err());
    }
}

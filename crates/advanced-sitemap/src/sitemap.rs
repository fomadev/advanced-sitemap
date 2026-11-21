use std::collections::HashSet;
use std::fmt;
use std::io::Write;

use sitemap_model::UrlEntry;
use sitemap_model::limits::{MAX_IMAGES_PER_URL, MAX_SITEMAP_BYTES, MAX_URLS_PER_SITEMAP};
use sitemap_xml::{XmlError, urlset_to_vec};

/// A single sitemap document held in memory.
///
/// Intended for small and medium sites. It enforces the per-file protocol
/// limits and declares exactly the namespaces it uses. For large or streamed
/// datasets, use [`crate::Engine`], which partitions automatically.
#[derive(Clone, Debug, PartialEq)]
pub struct Sitemap {
    entries: Vec<UrlEntry>,
}

/// Builds a [`Sitemap`].
#[derive(Clone, Debug, Default)]
pub struct SitemapBuilder {
    entries: Vec<UrlEntry>,
}

/// Why a [`Sitemap`] cannot be built or serialized.
#[derive(Debug)]
pub enum SitemapError {
    /// More than 50,000 URLs.
    TooManyUrls {
        /// Number of URLs.
        count: usize,
    },
    /// The same URL was added twice.
    DuplicateUrl(String),
    /// An entry has more than 1,000 images.
    TooManyImages(String),
    /// The serialized document exceeds 50 MiB.
    TooLarge {
        /// Size in bytes.
        bytes: u64,
    },
    /// Serialization failed.
    Xml(XmlError),
}

impl fmt::Display for SitemapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SitemapError::TooManyUrls { count } => write!(
                f,
                "{count} URLs exceed the {MAX_URLS_PER_SITEMAP} allowed in one sitemap; use Engine to partition"
            ),
            SitemapError::DuplicateUrl(url) => write!(f, "URL {url} was added more than once"),
            SitemapError::TooManyImages(url) => {
                write!(f, "URL {url} has more than {MAX_IMAGES_PER_URL} images")
            }
            SitemapError::TooLarge { bytes } => write!(
                f,
                "the sitemap is {bytes} bytes, above the {MAX_SITEMAP_BYTES}-byte limit; use Engine to partition"
            ),
            SitemapError::Xml(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SitemapError {}

impl From<XmlError> for SitemapError {
    fn from(error: XmlError) -> Self {
        SitemapError::Xml(error)
    }
}

impl SitemapBuilder {
    /// Adds an entry (a [`crate::SitemapUrl`] converts into an entry).
    pub fn url(mut self, entry: impl Into<UrlEntry>) -> Self {
        self.entries.push(entry.into());
        self
    }

    /// Adds several entries.
    pub fn urls<E: Into<UrlEntry>>(mut self, entries: impl IntoIterator<Item = E>) -> Self {
        self.entries.extend(entries.into_iter().map(Into::into));
        self
    }

    /// Checks the limits and builds the sitemap.
    pub fn build(self) -> Result<Sitemap, SitemapError> {
        if self.entries.len() > MAX_URLS_PER_SITEMAP as usize {
            return Err(SitemapError::TooManyUrls {
                count: self.entries.len(),
            });
        }
        let mut seen = HashSet::with_capacity(self.entries.len());
        for entry in &self.entries {
            if !seen.insert(entry.loc.as_str()) {
                return Err(SitemapError::DuplicateUrl(entry.loc.as_str().to_owned()));
            }
            if entry.images.len() > MAX_IMAGES_PER_URL {
                return Err(SitemapError::TooManyImages(entry.loc.as_str().to_owned()));
            }
        }
        Ok(Sitemap {
            entries: self.entries,
        })
    }
}

impl Sitemap {
    /// Starts a builder.
    pub fn builder() -> SitemapBuilder {
        SitemapBuilder::default()
    }

    /// The entries, in insertion order.
    pub fn entries(&self) -> &[UrlEntry] {
        &self.entries
    }

    /// Serializes the document.
    pub fn to_xml_bytes(&self) -> Result<Vec<u8>, SitemapError> {
        let bytes = urlset_to_vec(&self.entries)?;
        if bytes.len() as u64 > MAX_SITEMAP_BYTES {
            return Err(SitemapError::TooLarge {
                bytes: bytes.len() as u64,
            });
        }
        Ok(bytes)
    }

    /// Serializes the document as a string.
    pub fn to_xml(&self) -> Result<String, SitemapError> {
        let bytes = self.to_xml_bytes()?;
        // The writer only emits UTF-8.
        String::from_utf8(bytes)
            .map_err(|e| SitemapError::Xml(XmlError::Io(std::io::Error::other(e.to_string()))))
    }

    /// Writes the document to `out` and returns its size.
    pub fn write_to(&self, mut out: impl Write) -> Result<u64, SitemapError> {
        let bytes = self.to_xml_bytes()?;
        out.write_all(&bytes).map_err(XmlError::Io)?;
        Ok(bytes.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangeFreq, Priority, SitemapUrl, W3cDateTime};

    #[test]
    fn builds_minimal_sitemap() {
        let sitemap = Sitemap::builder()
            .url(SitemapUrl::new("https://example.com/").unwrap())
            .build()
            .unwrap();
        assert_eq!(
            sitemap.to_xml().unwrap(),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n  <url>\n    <loc>https://example.com/</loc>\n  </url>\n</urlset>\n"
        );
    }

    #[test]
    fn rejects_duplicates_and_too_many_urls() {
        let url = SitemapUrl::new("https://example.com/").unwrap();
        let err = Sitemap::builder()
            .url(url.clone())
            .url(url)
            .build()
            .unwrap_err();
        assert!(matches!(err, SitemapError::DuplicateUrl(_)));

        let many = (0..=MAX_URLS_PER_SITEMAP)
            .map(|i| SitemapUrl::new(&format!("https://example.com/{i}")).unwrap());
        assert!(matches!(
            Sitemap::builder().urls(many).build(),
            Err(SitemapError::TooManyUrls { .. })
        ));
    }

    #[test]
    fn full_entries() {
        let entry = UrlEntry::new(SitemapUrl::new("https://example.com/a").unwrap())
            .with_lastmod(W3cDateTime::parse("2026-11-21").unwrap())
            .with_changefreq(ChangeFreq::Daily)
            .with_priority(Priority::new(1.0).unwrap());
        let mut out = Vec::new();
        let size = Sitemap::builder()
            .url(entry)
            .build()
            .unwrap()
            .write_to(&mut out)
            .unwrap();
        assert_eq!(size, out.len() as u64);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("<priority>1.0</priority>"));
    }
}

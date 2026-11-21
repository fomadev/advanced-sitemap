use crate::{ChangeFreq, LanguageTag, Priority, SitemapUrl, Video, W3cDateTime};

/// An alternate language version of a page (`<xhtml:link rel="alternate">`).
#[derive(Clone, Debug, PartialEq)]
pub struct Alternate {
    /// The `hreflang` attribute.
    pub hreflang: LanguageTag,
    /// The URL of the alternate version.
    pub href: SitemapUrl,
}

/// An image attached to a page (`<image:image>`).
///
/// Only `<image:loc>` is modelled: Google stopped using the other image tags
/// (`caption`, `title`, `geo_location`, `license`) in 2022.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// The image URL.
    pub loc: SitemapUrl,
}

/// A validated `<url>` entry.
///
/// Every field holds a validated type, so any `UrlEntry` can be serialized
/// without further checks. Collection limits (for example at most 1,000
/// images) are enforced by the validator, not by this type.
#[derive(Clone, Debug, PartialEq)]
pub struct UrlEntry {
    /// `<loc>`
    pub loc: SitemapUrl,
    /// `<lastmod>`
    pub lastmod: Option<W3cDateTime>,
    /// `<changefreq>`
    pub changefreq: Option<ChangeFreq>,
    /// `<priority>`
    pub priority: Option<Priority>,
    /// `<xhtml:link rel="alternate">` elements, in input order.
    pub alternates: Vec<Alternate>,
    /// `<image:image>` elements, in input order.
    pub images: Vec<Image>,
    /// `<video:video>` elements, in input order.
    pub videos: Vec<Video>,
}

impl UrlEntry {
    /// An entry with only a location.
    pub fn new(loc: SitemapUrl) -> Self {
        Self {
            loc,
            lastmod: None,
            changefreq: None,
            priority: None,
            alternates: Vec::new(),
            images: Vec::new(),
            videos: Vec::new(),
        }
    }

    /// Sets `<lastmod>`.
    pub fn with_lastmod(mut self, lastmod: W3cDateTime) -> Self {
        self.lastmod = Some(lastmod);
        self
    }

    /// Sets `<changefreq>`.
    pub fn with_changefreq(mut self, changefreq: ChangeFreq) -> Self {
        self.changefreq = Some(changefreq);
        self
    }

    /// Sets `<priority>`.
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Appends an alternate language link.
    pub fn with_alternate(mut self, hreflang: LanguageTag, href: SitemapUrl) -> Self {
        self.alternates.push(Alternate { hreflang, href });
        self
    }

    /// Appends an image.
    pub fn with_image(mut self, loc: SitemapUrl) -> Self {
        self.images.push(Image { loc });
        self
    }

    /// Appends a video.
    pub fn with_video(mut self, video: Video) -> Self {
        self.videos.push(video);
        self
    }
}

impl From<SitemapUrl> for UrlEntry {
    fn from(loc: SitemapUrl) -> Self {
        UrlEntry::new(loc)
    }
}

/// A `<sitemap>` entry of a sitemap index.
#[derive(Clone, Debug, PartialEq)]
pub struct IndexEntry {
    /// Public URL of the child sitemap.
    pub loc: SitemapUrl,
    /// Last modification of the child sitemap.
    pub lastmod: Option<W3cDateTime>,
}

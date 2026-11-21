//! Limits defined by the sitemap protocol (sitemaps.org) and by Google's
//! documented extensions. They are the defaults and the upper bounds of the
//! engine configuration.

/// Maximum number of `<url>` elements in one sitemap file.
pub const MAX_URLS_PER_SITEMAP: u32 = 50_000;

/// Maximum uncompressed size of one sitemap file, in bytes (50 MiB).
pub const MAX_SITEMAP_BYTES: u64 = 52_428_800;

/// Maximum number of `<sitemap>` elements in one sitemap index.
pub const MAX_SITEMAPS_PER_INDEX: u32 = 50_000;

/// URLs must be shorter than 2,048 characters (sitemaps.org).
pub const MAX_URL_LENGTH: usize = 2_047;

/// Maximum number of `<image:image>` elements per `<url>` (Google).
pub const MAX_IMAGES_PER_URL: usize = 1_000;

/// Maximum number of `<video:tag>` elements per video (Google).
pub const MAX_VIDEO_TAGS: usize = 32;

/// Maximum length of `<video:description>`, in characters (Google).
pub const MAX_VIDEO_DESCRIPTION_CHARS: usize = 2_048;

/// Maximum length of `<video:title>`, in characters (Google's XSD; the
/// current documentation states no limit).
pub const MAX_VIDEO_TITLE_CHARS: usize = 100;

/// Maximum `<video:duration>`, in seconds (8 hours, Google).
pub const MAX_VIDEO_DURATION_SECONDS: u32 = 28_800;

/// Maximum length of `<video:uploader>`, in characters (Google).
pub const MAX_VIDEO_UPLOADER_CHARS: usize = 255;

/// Maximum number of `<news:news>` elements in one sitemap file (Google).
pub const MAX_NEWS_PER_SITEMAP: u32 = 1_000;

/// Google News only uses articles published in the last two days, in seconds.
pub const NEWS_MAX_AGE_SECONDS: i64 = 2 * 24 * 3600;

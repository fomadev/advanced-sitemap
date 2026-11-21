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

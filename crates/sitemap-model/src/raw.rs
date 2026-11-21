/// An unchecked URL record, as received from a source.
///
/// Required fields are `Option`s on purpose: a missing value becomes a
/// diagnostic during validation instead of a decoding failure, so every problem
/// of a record is reported at once.
///
/// With the `serde` feature, this type is the JSON representation of a URL
/// record in protocol version 1 (see `schemas/url-record.v1.schema.json`).
/// Unknown fields are rejected to catch typos such as `lastMod`; `url` is
/// accepted as an alias of `loc` for migration from `next-advanced-sitemap`.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RawUrlRecord {
    /// Page URL.
    #[cfg_attr(feature = "serde", serde(alias = "url", default))]
    pub loc: Option<String>,
    /// W3C Datetime.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub lastmod: Option<String>,
    /// Change frequency keyword.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub changefreq: Option<String>,
    /// Priority between 0.0 and 1.0.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub priority: Option<f64>,
    /// Alternate language versions.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub alternates: Vec<RawAlternate>,
    /// Images.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub images: Vec<RawImage>,
    /// Videos (Google video extension).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub videos: Vec<RawVideo>,
}

impl RawUrlRecord {
    /// A record with only a location.
    pub fn with_loc(loc: impl Into<String>) -> Self {
        Self {
            loc: Some(loc.into()),
            ..Self::default()
        }
    }
}

/// An unchecked alternate language link.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RawAlternate {
    /// Language code or `x-default`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub hreflang: Option<String>,
    /// Alternate URL.
    #[cfg_attr(feature = "serde", serde(default))]
    pub href: Option<String>,
}

/// An unchecked image.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct RawImage {
    /// Image URL.
    #[cfg_attr(feature = "serde", serde(default))]
    pub loc: Option<String>,
}

/// An unchecked video (Google video extension). JSON keys are camelCase.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "camelCase")
)]
pub struct RawVideo {
    /// Thumbnail URL (required).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub thumbnail_loc: Option<String>,
    /// Title (required).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub title: Option<String>,
    /// Description (required, at most 2,048 characters).
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub description: Option<String>,
    /// URL of the media file. At least one of `contentLoc` and `playerLoc` is required.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub content_loc: Option<String>,
    /// URL of a player for this video.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub player_loc: Option<String>,
    /// Duration in seconds, 1 to 28,800.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub duration: Option<i64>,
    /// W3C Datetime after which the video is no longer shown.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub expiration_date: Option<String>,
    /// Rating, 0.0 to 5.0.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub rating: Option<f64>,
    /// Number of views.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub view_count: Option<u64>,
    /// W3C Datetime of first publication.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub publication_date: Option<String>,
    /// Tags, at most 32.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub tags: Vec<String>,
    /// Shown when SafeSearch is on.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub family_friendly: Option<bool>,
    /// Country restriction.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub restriction: Option<RawVideoRestriction>,
    /// A subscription is needed to watch.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub requires_subscription: Option<bool>,
    /// Uploader.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub uploader: Option<RawVideoUploader>,
    /// Platform restriction.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub platform: Option<RawVideoPlatform>,
    /// The video is a livestream.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub live: Option<bool>,
}

/// An unchecked `video:restriction`.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "camelCase")
)]
pub struct RawVideoRestriction {
    /// `allow` or `deny`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub relationship: Option<String>,
    /// ISO 3166-1 alpha-2 country codes.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub countries: Vec<String>,
}

/// An unchecked `video:platform`.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "camelCase")
)]
pub struct RawVideoPlatform {
    /// `allow` or `deny`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub relationship: Option<String>,
    /// `web`, `mobile` or `tv`.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub platforms: Vec<String>,
}

/// An unchecked `video:uploader`.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(
    feature = "serde",
    serde(deny_unknown_fields, rename_all = "camelCase")
)]
pub struct RawVideoUploader {
    /// Uploader name.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub name: Option<String>,
    /// URL of a page about the uploader.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub info: Option<String>,
}

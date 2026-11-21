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

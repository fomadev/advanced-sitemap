//! Google video sitemap extension (`video:video`, namespace
//! `http://www.google.com/schemas/sitemap-video/1.1`).
//!
//! Only the tags Google still documents are modelled. `category`,
//! `gallery_loc`, `price`, `tvshow` and the `allow_embed` / `autoplay`
//! attributes of `player_loc` were removed from Google's documentation in
//! 2022 and are not emitted.

use std::fmt;

use crate::{SitemapUrl, W3cDateTime};

/// `allow` or `deny`, the `relationship` attribute of `restriction` and
/// `platform`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relationship {
    /// Only the listed values are allowed.
    Allow,
    /// The listed values are denied.
    Deny,
}

impl Relationship {
    /// Parses `allow` or `deny` (lowercase, as in the schema).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Relationship::Allow),
            "deny" => Some(Relationship::Deny),
            _ => None,
        }
    }

    /// The attribute value.
    pub fn as_str(self) -> &'static str {
        match self {
            Relationship::Allow => "allow",
            Relationship::Deny => "deny",
        }
    }
}

/// A platform of `video:platform`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Platform {
    /// Desktop and laptop browsers.
    Web,
    /// Mobile browsers.
    Mobile,
    /// TV browsers.
    Tv,
}

impl Platform {
    /// Parses `web`, `mobile` or `tv` (lowercase, as in the schema).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "web" => Some(Platform::Web),
            "mobile" => Some(Platform::Mobile),
            "tv" => Some(Platform::Tv),
            _ => None,
        }
    }

    /// The list value.
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Web => "web",
            Platform::Mobile => "mobile",
            Platform::Tv => "tv",
        }
    }
}

/// An ISO 3166-1 alpha-2 country code, stored in uppercase.
///
/// Only the format (two ASCII letters) is checked, not whether the code is
/// currently assigned.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CountryCode(String);

impl CountryCode {
    /// Accepts two ASCII letters in any case.
    pub fn parse(value: &str) -> Option<Self> {
        (value.len() == 2 && value.bytes().all(|b| b.is_ascii_alphabetic()))
            .then(|| CountryCode(value.to_ascii_uppercase()))
    }

    /// The uppercase code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `video:restriction`: countries where the video may or may not be shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoRestriction {
    /// `relationship` attribute.
    pub relationship: Relationship,
    /// Countries, in input order, without duplicates.
    pub countries: Vec<CountryCode>,
}

/// `video:platform`: platforms where the video may or may not be shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoPlatform {
    /// `relationship` attribute.
    pub relationship: Relationship,
    /// Platforms, in input order, without duplicates.
    pub platforms: Vec<Platform>,
}

/// `video:uploader`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoUploader {
    /// Uploader name, at most 255 characters.
    pub name: String,
    /// `info` attribute: page about the uploader, on the same host as `loc`.
    pub info: Option<SitemapUrl>,
}

/// A validated `video:video` element.
#[derive(Clone, Debug, PartialEq)]
pub struct Video {
    /// `video:thumbnail_loc` (required).
    pub thumbnail_loc: SitemapUrl,
    /// `video:title` (required, non-empty).
    pub title: String,
    /// `video:description` (required, non-empty, at most 2,048 characters).
    pub description: String,
    /// `video:content_loc`: the media file. At least one of `content_loc`
    /// and `player_loc` is present, and neither equals the page `loc`.
    pub content_loc: Option<SitemapUrl>,
    /// `video:player_loc`: a player for this video.
    pub player_loc: Option<SitemapUrl>,
    /// `video:duration` in seconds, 1 to 28,800.
    pub duration: Option<u32>,
    /// `video:expiration_date`.
    pub expiration_date: Option<W3cDateTime>,
    /// `video:rating`, 0.0 to 5.0.
    pub rating: Option<f64>,
    /// `video:view_count`.
    pub view_count: Option<u64>,
    /// `video:publication_date`.
    pub publication_date: Option<W3cDateTime>,
    /// `video:tag`, at most 32.
    pub tags: Vec<String>,
    /// `video:family_friendly`.
    pub family_friendly: Option<bool>,
    /// `video:restriction`.
    pub restriction: Option<VideoRestriction>,
    /// `video:requires_subscription`.
    pub requires_subscription: Option<bool>,
    /// `video:uploader`.
    pub uploader: Option<VideoUploader>,
    /// `video:platform`.
    pub platform: Option<VideoPlatform>,
    /// `video:live`.
    pub live: Option<bool>,
}

impl Video {
    /// A video with its required fields and a content URL.
    pub fn new(
        thumbnail_loc: SitemapUrl,
        title: impl Into<String>,
        description: impl Into<String>,
        content_loc: SitemapUrl,
    ) -> Self {
        Self {
            thumbnail_loc,
            title: title.into(),
            description: description.into(),
            content_loc: Some(content_loc),
            player_loc: None,
            duration: None,
            expiration_date: None,
            rating: None,
            view_count: None,
            publication_date: None,
            tags: Vec::new(),
            family_friendly: None,
            restriction: None,
            requires_subscription: None,
            uploader: None,
            platform: None,
            live: None,
        }
    }
}

/// Formats a rating with up to 2 decimals, without trailing zeros (`4.5`, `5.0`).
pub(crate) fn format_rating(value: f64) -> String {
    let mut text = format!("{value:.2}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text
}

impl Video {
    /// `video:rating` as written in XML.
    pub fn rating_xml(&self) -> Option<String> {
        self.rating.map(format_rating)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_format() {
        assert_eq!(format_rating(4.5), "4.5");
        assert_eq!(format_rating(5.0), "5.0");
        assert_eq!(format_rating(0.0), "0.0");
        assert_eq!(format_rating(3.25), "3.25");
    }

    #[test]
    fn country_codes_are_uppercased() {
        assert_eq!(CountryCode::parse("fr").unwrap().as_str(), "FR");
        assert!(CountryCode::parse("FRA").is_none());
        assert!(CountryCode::parse("F1").is_none());
    }

    #[test]
    fn keywords() {
        assert_eq!(Relationship::parse("allow"), Some(Relationship::Allow));
        assert_eq!(Relationship::parse("Allow"), None);
        assert_eq!(Platform::parse("tv"), Some(Platform::Tv));
        assert_eq!(Platform::parse("desktop"), None);
    }
}

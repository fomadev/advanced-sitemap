//! Data model of the Advanced Sitemap engine.
//!
//! The model has two layers:
//!
//! * **Raw records** ([`RawUrlRecord`]) hold unchecked strings exactly as they
//!   arrive from a source (JSON, JSONL, a framework adapter...).
//! * **Typed values** ([`SitemapUrl`], [`W3cDateTime`], [`Priority`],
//!   [`UrlEntry`]...) can only be built through validating constructors, so a
//!   typed entry is valid by construction and can be serialized without further
//!   checks.
//!
//! The crate also defines [`Diagnostic`]s, the structured findings produced by
//! every stage of the engine, and the protocol [`limits`].

#![forbid(unsafe_code)]

mod datetime;
mod diagnostic;
mod entry;
mod hreflang;
pub mod limits;
mod news;
mod raw;
mod scalar;
mod sitemap_url;
mod video;

pub use datetime::{DateTimeError, DateTimePrecision, W3cDateTime};
pub use diagnostic::{Diagnostic, DiagnosticCode, Location, Severity};
pub use entry::{Alternate, Image, IndexEntry, UrlEntry};
pub use hreflang::{LanguageTag, LanguageTagError};
pub use news::{News, NewsLanguage};
pub use raw::{
    RawAlternate, RawImage, RawNews, RawNewsPublication, RawUrlRecord, RawVideo, RawVideoPlatform,
    RawVideoRestriction, RawVideoUploader,
};
pub use scalar::{ChangeFreq, ChangeFreqError, Priority, PriorityError};
pub use sitemap_url::{SitemapUrl, UrlError};
pub use video::{
    CountryCode, Platform, Relationship, Video, VideoPlatform, VideoRestriction, VideoUploader,
};

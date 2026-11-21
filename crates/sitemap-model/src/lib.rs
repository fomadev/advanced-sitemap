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

mod datetime;
mod diagnostic;
mod entry;
mod hreflang;
pub mod limits;
mod raw;
mod scalar;
mod sitemap_url;

pub use datetime::{DateTimeError, DateTimePrecision, W3cDateTime};
pub use diagnostic::{Diagnostic, DiagnosticCode, Location, Severity};
pub use entry::{Alternate, Image, IndexEntry, UrlEntry};
pub use hreflang::{LanguageTag, LanguageTagError};
pub use raw::{RawAlternate, RawImage, RawUrlRecord};
pub use scalar::{ChangeFreq, ChangeFreqError, Priority, PriorityError};
pub use sitemap_url::{SitemapUrl, UrlError};

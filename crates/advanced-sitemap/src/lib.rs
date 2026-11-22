//! # Advanced Sitemap
//!
//! A framework-independent sitemap engine: validation with structured
//! diagnostics, deterministic streaming XML generation, exact partitioning
//! under protocol limits, automatic sitemap indexes and staged publication.
//!
//! ## Small sitemaps, in memory
//!
//! ```
//! use advanced_sitemap::{Sitemap, SitemapUrl};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let sitemap = Sitemap::builder()
//!     .url(SitemapUrl::new("https://example.com/")?)
//!     .build()?;
//! let xml = sitemap.to_xml()?;
//! assert!(xml.contains("<loc>https://example.com/</loc>"));
//! # Ok(())
//! # }
//! ```
//!
//! ## Any size, streamed into a store
//!
//! ```
//! use advanced_sitemap::{Engine, EngineConfig, RunMode, RawUrlRecord, MemoryStore, records};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = Engine::new(EngineConfig::default())?;
//! let mut store = MemoryStore::new();
//! let input = records([
//!     RawUrlRecord::with_loc("https://example.com/"),
//!     RawUrlRecord::with_loc("https://example.com/about"),
//! ]);
//! let report = engine.run(input, &mut store, RunMode::Publish)?;
//! assert!(report.succeeded());
//! assert!(store.published()?.contains_key("sitemap.xml"));
//! # Ok(())
//! # }
//! ```

mod config;
pub mod diff;
mod engine;
mod hreflang;
pub mod inspect;
mod manifest;
pub mod plan;
mod report;
mod sitemap;

pub use config::{InputSpec, ResolvedConfig, open_input, resolve_config};
pub use engine::{
    Engine, EngineConfig, EngineError, InvalidPolicy, PublishPolicy, RunMode, SnapshotSettings,
    records,
};
pub use manifest::{Manifest, ManifestArtifact};
pub use report::{
    ArtifactKind, ArtifactReport, ChangesReport, GenerationReport, RunStatus, SeverityCounts, Stats,
};
pub use sitemap::{Sitemap, SitemapBuilder, SitemapError};

pub use sitemap_index::{
    IndexMode, Layout, PartitionLimits, PartitionStrategy, PublicBaseUrl, fnv1a64,
};
pub use sitemap_model::{
    Alternate, ChangeFreq, DateTimePrecision, Diagnostic, DiagnosticCode, Image, IndexEntry,
    LanguageTag, Location, Priority, RawAlternate, RawImage, RawUrlRecord, Severity, SitemapUrl,
    UrlEntry, UrlError, W3cDateTime, limits,
};
pub use sitemap_protocol::{self as protocol, ConfigDocument, SourceRecord};
pub use sitemap_reader::{
    self as reader, DocumentKind, InspectOptions, InspectReport, Item, ReaderLimits,
};
pub use sitemap_robots::{RobotsConfig, RobotsGroup, RobotsTxt};
pub use sitemap_storage::{
    ArtifactName, ArtifactStore, ArtifactWriter, FsStore, MemoryStore, NullStore, Publication,
    StoreDescription,
};
pub use sitemap_validation::{ValidationMode, ValidationOptions, Validator};
pub use sitemap_xml::{self as xml, Namespaces};

/// Version of the engine.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

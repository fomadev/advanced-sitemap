//! Inspection of existing sitemap files (`advanced-sitemap inspect`).
//!
//! Wraps [`sitemap_reader::inspect`] with a serializable report. Only local
//! files and readers are supported: fetching remote sitemaps needs an SSRF
//! guard and is not implemented yet.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::Serialize;
use sitemap_model::{Diagnostic, Severity};
use sitemap_reader::{InspectOptions, InspectReport, Item};

use crate::report::SeverityCounts;

/// Serializable result of inspecting one document.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInspection {
    /// Path or label of the input (`-` for standard input).
    pub path: String,
    /// `urlset`, `sitemapindex`, or `null` when the root was refused.
    pub kind: Option<&'static str>,
    /// The input was gzip-compressed.
    pub compressed: bool,
    /// Uncompressed bytes read.
    pub xml_bytes: u64,
    /// The whole document was read.
    pub complete: bool,
    /// Entries found.
    pub entries: u64,
    /// Entries without errors, duplicates excluded.
    pub valid_entries: u64,
    /// Duplicate entries.
    pub duplicates: u64,
    /// Valid entries with `lastmod`.
    pub with_lastmod: u64,
    /// Valid entries with alternates.
    pub with_alternates: u64,
    /// Valid entries with images.
    pub with_images: u64,
    /// Valid entries with videos.
    pub with_videos: u64,
    /// Valid entries with news metadata.
    pub with_news: u64,
    /// `<news:news>` elements found.
    pub news_elements: u64,
    /// Skipped elements of namespaces that are not validated, by namespace.
    pub unsupported_extensions: BTreeMap<String, u64>,
    /// Counts by severity.
    pub severity_counts: SeverityCounts,
    /// Findings, in document order.
    pub diagnostics: Vec<Diagnostic>,
}

impl FileInspection {
    /// Builds the serializable view of a reader report.
    pub fn new(path: impl Into<String>, report: InspectReport) -> Self {
        let mut counts = SeverityCounts::default();
        for diagnostic in &report.diagnostics {
            match diagnostic.severity {
                Severity::Error => counts.error += 1,
                Severity::Warning => counts.warning += 1,
                Severity::Recommendation => counts.recommendation += 1,
                Severity::Info => counts.info += 1,
            }
        }
        Self {
            path: path.into(),
            kind: report.kind.map(|k| k.as_str()),
            compressed: report.compressed,
            xml_bytes: report.xml_bytes,
            complete: report.complete,
            entries: report.entries,
            valid_entries: report.valid_entries,
            duplicates: report.duplicates,
            with_lastmod: report.with_lastmod,
            with_alternates: report.with_alternates,
            with_images: report.with_images,
            with_videos: report.with_videos,
            with_news: report.with_news,
            news_elements: report.news_elements,
            unsupported_extensions: report.unsupported_extensions,
            severity_counts: counts,
            diagnostics: report.diagnostics,
        }
    }

    /// `true` when the document was fully read without error diagnostics.
    pub fn is_valid(&self) -> bool {
        self.complete && self.severity_counts.error == 0
    }
}

/// Inspects a reader. Valid entries are passed to `on_item`.
pub fn inspect_reader<R: Read>(
    label: impl Into<String>,
    input: R,
    options: &InspectOptions,
    on_item: impl FnMut(Item),
) -> io::Result<FileInspection> {
    let report = sitemap_reader::inspect(input, options, on_item)?;
    Ok(FileInspection::new(label, report))
}

/// Inspects a local file (plain XML or gzip).
pub fn inspect_file(path: &Path, options: &InspectOptions) -> io::Result<FileInspection> {
    let file = File::open(path)?;
    inspect_reader(path.display().to_string(), file, options, |_| {})
}

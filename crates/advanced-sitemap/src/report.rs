use std::collections::BTreeMap;

use serde::Serialize;
use sitemap_model::{Diagnostic, DiagnosticCode, Location, Severity};
use sitemap_storage::StoreDescription;

/// Outcome of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    /// Artifacts were published.
    Published,
    /// Nothing was published because of blocking errors.
    Failed,
    /// Dry run: the input would publish successfully.
    Valid,
    /// Dry run: the input has blocking errors.
    Invalid,
}

/// Counters of a run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Records read from the source.
    pub records: u64,
    /// Records that passed validation.
    pub valid_records: u64,
    /// Records rejected (decoding or validation errors).
    pub invalid_records: u64,
    /// Valid records dropped as duplicates.
    pub duplicates: u64,
    /// URLs written to sitemap files.
    pub urls_written: u64,
}

/// Number of diagnostics per severity (exact, even when the list is truncated).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SeverityCounts {
    /// Errors.
    pub error: u64,
    /// Warnings.
    pub warning: u64,
    /// Recommendations.
    pub recommendation: u64,
    /// Information.
    pub info: u64,
}

/// Kind of artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    /// A `<urlset>` file.
    Sitemap,
    /// A `<sitemapindex>` file.
    Index,
    /// The JSON manifest.
    Manifest,
    /// robots.txt.
    Robots,
}

/// An artifact produced by the run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactReport {
    /// Public file name.
    pub name: String,
    /// Kind.
    pub kind: ArtifactKind,
    /// Number of `<url>` (sitemap) or `<sitemap>` (index) elements.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<u64>,
    /// Exact size in bytes.
    pub bytes: u64,
}

/// Store information included in reports.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreReport {
    /// Store kind.
    pub kind: &'static str,
    /// Location.
    pub location: String,
    /// Per-artifact atomic publication.
    pub atomic_per_artifact: bool,
    /// Whole-set atomic publication.
    pub atomic_set: bool,
}

impl From<StoreDescription> for StoreReport {
    fn from(d: StoreDescription) -> Self {
        Self {
            kind: d.kind,
            location: d.location,
            atomic_per_artifact: d.atomic_per_artifact,
            atomic_set: d.atomic_set,
        }
    }
}

/// The result of a run, serializable as the version 1 report document
/// (`schemas/report.v1.schema.json`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationReport {
    /// Report format version.
    pub schema_version: &'static str,
    /// Engine version.
    pub engine_version: &'static str,
    /// Outcome.
    pub status: RunStatus,
    /// Counters.
    pub stats: Stats,
    /// Artifacts published (or that would be published in a dry run).
    pub artifacts: Vec<ArtifactReport>,
    /// Store used.
    pub store: StoreReport,
    /// Diagnostics per severity.
    pub severity_counts: SeverityCounts,
    /// Diagnostics per code.
    pub diagnostic_counts: BTreeMap<&'static str, u64>,
    /// Diagnostics, in order of discovery, limited to `maxDiagnostics`.
    pub diagnostics: Vec<Diagnostic>,
}

impl GenerationReport {
    /// `true` for [`RunStatus::Published`] and [`RunStatus::Valid`].
    pub fn succeeded(&self) -> bool {
        matches!(self.status, RunStatus::Published | RunStatus::Valid)
    }

    /// Serializes the report as pretty JSON.
    pub fn to_json(&self) -> String {
        // Serializing plain data structures cannot fail.
        serde_json::to_string_pretty(self).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

/// Collects diagnostics with a cap on the stored list and exact counters.
pub(crate) struct DiagnosticCollector {
    max: usize,
    list: Vec<Diagnostic>,
    by_code: BTreeMap<&'static str, u64>,
    by_severity: SeverityCounts,
    truncated: bool,
}

impl DiagnosticCollector {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            max,
            list: Vec::new(),
            by_code: BTreeMap::new(),
            by_severity: SeverityCounts::default(),
            truncated: false,
        }
    }

    pub(crate) fn push(&mut self, diagnostic: Diagnostic) {
        *self.by_code.entry(diagnostic.code.as_str()).or_insert(0) += 1;
        match diagnostic.severity {
            Severity::Error => self.by_severity.error += 1,
            Severity::Warning => self.by_severity.warning += 1,
            Severity::Recommendation => self.by_severity.recommendation += 1,
            Severity::Info => self.by_severity.info += 1,
        }
        if self.list.len() < self.max {
            self.list.push(diagnostic);
        } else {
            self.truncated = true;
        }
    }

    pub(crate) fn has_errors(&self) -> bool {
        self.by_severity.error > 0
    }

    pub(crate) fn finish(
        mut self,
    ) -> (Vec<Diagnostic>, BTreeMap<&'static str, u64>, SeverityCounts) {
        if self.truncated {
            let kept = self.list.len();
            let total: u64 = self.by_code.values().sum();
            let notice = Diagnostic::new(
                DiagnosticCode::DiagnosticsTruncated,
                format!("{total} diagnostics were produced; the first {kept} are listed and counts are exact"),
                Location::default(),
            )
            .with_suggestion("Raise validation.maxDiagnostics to list more.");
            *self.by_code.entry(notice.code.as_str()).or_insert(0) += 1;
            self.by_severity.info += 1;
            self.list.push(notice);
        }
        (self.list, self.by_code, self.by_severity)
    }
}

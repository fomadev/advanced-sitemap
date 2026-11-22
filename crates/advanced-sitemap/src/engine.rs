use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use sitemap_index::{
    IndexMode, Layout, LayoutError, PartitionLimits, PartitionStrategy, PartitionSummary,
    Partitioner, PushError, plan_layout,
};
use sitemap_model::{Diagnostic, DiagnosticCode, Location, RawUrlRecord, UrlEntry};
use sitemap_protocol::{DecodeError, SourceRecord};
use sitemap_robots::RobotsConfig;
use sitemap_storage::{ArtifactName, ArtifactStore, Publication};
use sitemap_validation::{ValidationOptions, Validator};
use sitemap_xml::{Namespaces, XmlError, write_sitemap_index};

use sitemap_snapshot::{
    Digest, Record as SnapshotRecord, SnapshotBuilder, diff, digest128, open_file as open_snapshot,
};

mod delta;

use crate::hreflang::ReciprocityChecker;
use crate::manifest::Manifest;
use crate::report::ChangesReport;
use crate::report::{
    ArtifactKind, ArtifactReport, DiagnosticCollector, GenerationReport, RunStatus, Stats,
};

/// What to do with invalid records.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InvalidPolicy {
    /// Publish nothing if any record is invalid. Validation still runs to the
    /// end so that the report lists every problem.
    #[default]
    Fail,
    /// Exclude invalid records and publish the others.
    Skip,
}

/// Whether to publish the artifacts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunMode {
    /// Publish on success.
    Publish,
    /// Run the whole pipeline, then discard the staged artifacts.
    DryRun,
}

/// Engine configuration.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// Validation profile, site and reference time.
    pub validation: ValidationOptions,
    /// Invalid record policy.
    pub on_invalid: InvalidPolicy,
    /// Drop repeated URLs, keeping the first occurrence.
    pub dedupe: bool,
    /// Maximum number of diagnostics kept in the report (counts stay exact).
    pub max_diagnostics: usize,
    /// Partitioning strategy.
    pub strategy: PartitionStrategy,
    /// Per-file limits.
    pub limits: PartitionLimits,
    /// File names and index policy.
    pub layout: Layout,
    /// Publish `<stem>.manifest.json`.
    pub manifest: bool,
    /// Extension namespaces declared on every sitemap file. Entries that use
    /// another extension are rejected with `EXTENSION_NOT_DECLARED`.
    /// Default: [`Namespaces::DEFAULT`] (`xhtml` and `image`).
    pub namespaces: Namespaces,
    /// robots.txt published with the sitemaps. The generated sitemap (or
    /// index) is added as a `Sitemap` record when `public_base_url` is set.
    pub robots: Option<RobotsConfig>,
    /// Which artifacts a publishing run replaces. Default: [`PublishPolicy::All`].
    pub publish: PublishPolicy,
    /// URL-level snapshot and change report. Default: `None` (off).
    pub snapshot: Option<SnapshotSettings>,
    /// Check hreflang return links and conflicting codes across entries.
    /// Costs about 110 bytes per alternate link (measured). Default: `false`.
    pub hreflang_reciprocity: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            validation: ValidationOptions::default(),
            on_invalid: InvalidPolicy::Fail,
            dedupe: true,
            max_diagnostics: 1_000,
            strategy: PartitionStrategy::Sequential,
            limits: PartitionLimits::default(),
            layout: Layout {
                stem: "sitemap".to_owned(),
                index: IndexMode::Auto,
                public_base_url: None,
            },
            manifest: true,
            namespaces: Namespaces::DEFAULT,
            robots: None,
            hreflang_reciprocity: false,
            publish: PublishPolicy::All,
            snapshot: None,
        }
    }
}

/// Where and how the URL-level snapshot is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotSettings {
    /// State directory. The snapshot is `<directory>/<stem>.snapshot`; sorted
    /// runs are spilled here while building it. Keep it out of the public
    /// output directory.
    pub directory: PathBuf,
    /// Memory used to sort records before spilling to disk, in bytes.
    pub memory_budget: usize,
    /// Maximum URLs listed per change kind in the report (counts stay exact).
    pub max_listed: usize,
}

impl SnapshotSettings {
    /// Settings with a 64 MiB sort budget and 1,000 listed URLs per kind.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            memory_budget: 64 << 20,
            max_listed: 1_000,
        }
    }
}

/// Which artifacts a publishing run replaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PublishPolicy {
    /// Republish every artifact (the behaviour before 0.2).
    #[default]
    All,
    /// Republish only artifacts whose bytes differ from the published file.
    /// The published result is identical to `All`; unchanged files keep
    /// their modification time and are not rewritten.
    Changed,
}

/// Errors that stop a run (as opposed to diagnostics, which are reported).
#[derive(Debug)]
pub enum EngineError {
    /// Invalid configuration.
    Config(String),
    /// Reading the source or writing to the store failed.
    Io(io::Error),
    /// The input document is unusable.
    Protocol(sitemap_protocol::ProtocolError),
    /// Serialization failed for a reason validation should have prevented.
    Xml(XmlError),
    /// The delta mode cannot reproduce a full generation; nothing was
    /// written. The message says why and what to run instead.
    DeltaRefused(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Config(message) => write!(f, "configuration error: {message}"),
            EngineError::Io(error) => write!(f, "I/O error: {error}"),
            EngineError::Protocol(error) => write!(f, "{error}"),
            EngineError::Xml(error) => write!(f, "XML error: {error}"),
            EngineError::DeltaRefused(reason) => write!(f, "delta refused: {reason}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<io::Error> for EngineError {
    fn from(error: io::Error) -> Self {
        EngineError::Io(error)
    }
}

impl From<sitemap_protocol::ProtocolError> for EngineError {
    fn from(error: sitemap_protocol::ProtocolError) -> Self {
        EngineError::Protocol(error)
    }
}

/// Wraps in-memory raw records as an engine source.
pub fn records(
    records: impl IntoIterator<Item = RawUrlRecord>,
) -> impl Iterator<Item = io::Result<SourceRecord>> {
    records.into_iter().enumerate().map(|(i, record)| {
        Ok(SourceRecord {
            record: i as u64 + 1,
            line: None,
            result: Ok(record),
        })
    })
}

enum Input {
    Raw(SourceRecord),
    Typed(u64, UrlEntry),
}

struct StreamOutcome {
    stats: Stats,
    collector: DiagnosticCollector,
    /// `None` when writing stopped (fail policy or structural error).
    partitions: Option<Vec<PartitionSummary>>,
    snapshot: Option<SnapshotBuilder>,
    /// Partitions kept as published, without a staged file (delta mode).
    reused: Vec<PartitionSummary>,
    /// Skip staged artifacts identical to the published ones.
    compare_with_published: bool,
    /// Partitions whose records are copied from the previous snapshot into
    /// the new one, already sorted (delta mode).
    keep_previous: Option<HashSet<u32>>,
}

type Completed = (
    RunStatus,
    Stats,
    DiagnosticCollector,
    Vec<ArtifactReport>,
    Option<SnapshotBuilder>,
);

/// The sitemap engine. Stateless and reusable: each run is independent.
#[derive(Clone, Debug)]
pub struct Engine {
    config: EngineConfig,
    validator: Validator,
}

impl Engine {
    /// Checks the configuration and builds an engine.
    pub fn new(config: EngineConfig) -> Result<Self, EngineError> {
        let stem = &config.layout.stem;
        let stem_ok = !stem.is_empty()
            && stem.len() <= 64
            && stem
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
        if !stem_ok {
            return Err(EngineError::Config(format!(
                "file stem {stem:?} must be 1-64 characters from [A-Za-z0-9_-]"
            )));
        }
        if let Some(robots) = &config.robots {
            let (_, diagnostics) = robots.build();
            let errors: Vec<String> = diagnostics
                .iter()
                .filter(|d| d.is_error())
                .map(ToString::to_string)
                .collect();
            if !errors.is_empty() {
                return Err(EngineError::Config(format!(
                    "invalid robots configuration: {}",
                    errors.join("; ")
                )));
            }
        }
        let validator = Validator::new(config.validation.clone());
        Ok(Self { config, validator })
    }

    /// The configuration.
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    fn manifest_name(&self) -> Result<ArtifactName, EngineError> {
        ArtifactName::new(format!("{}.manifest.json", self.config.layout.stem))
            .map_err(|e| EngineError::Config(e.to_string()))
    }

    /// Validates raw records and generates sitemaps into `store`.
    pub fn run<I>(
        &self,
        source: I,
        store: &mut dyn ArtifactStore,
        mode: RunMode,
    ) -> Result<GenerationReport, EngineError>
    where
        I: IntoIterator<Item = io::Result<SourceRecord>>,
    {
        self.execute(source.into_iter().map(|r| r.map(Input::Raw)), store, mode)
    }

    /// Generates sitemaps from already-typed entries (they are valid by
    /// construction; deduplication and partitioning still apply).
    pub fn run_entries<I>(
        &self,
        entries: I,
        store: &mut dyn ArtifactStore,
        mode: RunMode,
    ) -> Result<GenerationReport, EngineError>
    where
        I: IntoIterator<Item = UrlEntry>,
    {
        let inputs = entries
            .into_iter()
            .enumerate()
            .map(|(i, entry)| Ok(Input::Typed(i as u64 + 1, entry)));
        self.execute(inputs, store, mode)
    }

    fn execute(
        &self,
        inputs: impl Iterator<Item = io::Result<Input>>,
        store: &mut dyn ArtifactStore,
        mode: RunMode,
    ) -> Result<GenerationReport, EngineError> {
        let description = store.describe();
        store.set_compare_with_published(
            mode == RunMode::Publish && self.config.publish == PublishPolicy::Changed,
        );
        let outcome = match self.stream(inputs, store) {
            Ok(outcome) => outcome,
            Err(error) => {
                let _ = store.discard();
                return Err(error);
            }
        };
        self.finish_run(outcome, store, mode, description)
    }

    /// Writes the index, manifest and snapshot, publishes or discards, and
    /// builds the report.
    fn finish_run(
        &self,
        mut outcome: StreamOutcome,
        store: &mut dyn ArtifactStore,
        mode: RunMode,
        description: sitemap_storage::StoreDescription,
    ) -> Result<GenerationReport, EngineError> {
        let keep_previous = outcome.keep_previous.take();
        match self.complete(outcome, store, mode) {
            Ok((status, stats, mut collector, artifacts, snapshot)) => {
                let changes = match snapshot {
                    Some(builder) if matches!(status, RunStatus::Published | RunStatus::Valid) => {
                        Some(self.finish_snapshot(
                            builder,
                            keep_previous.as_ref(),
                            mode,
                            &mut collector,
                        )?)
                    }
                    _ => None,
                };
                let (diagnostics, diagnostic_counts, severity_counts) = collector.finish();
                Ok(GenerationReport {
                    schema_version: sitemap_protocol::REPORT_SCHEMA_VERSION,
                    engine_version: crate::ENGINE_VERSION,
                    status,
                    stats,
                    artifacts,
                    store: description.into(),
                    severity_counts,
                    diagnostic_counts,
                    diagnostics,
                    changes,
                })
            }
            Err(error) => {
                let _ = store.discard();
                Err(error)
            }
        }
    }

    /// Reads, validates, deduplicates and partitions every input.
    fn stream(
        &self,
        inputs: impl Iterator<Item = io::Result<Input>>,
        store: &mut dyn ArtifactStore,
    ) -> Result<StreamOutcome, EngineError> {
        let mut stats = Stats::default();
        let mut collector = DiagnosticCollector::new(self.config.max_diagnostics);
        let mut seen: HashSet<u128> = HashSet::new();
        let mut scratch = Vec::new();
        let fail_fast = self.config.on_invalid == InvalidPolicy::Fail;

        // Every file declares the configured extension namespaces: the
        // content of a streamed source is not known in advance.
        let mut partitioner = Partitioner::new(
            store,
            self.config.strategy,
            self.config.limits,
            self.config.namespaces,
            &self.config.layout.stem,
        );
        let mut writing = true;
        let mut reciprocity = self
            .config
            .hreflang_reciprocity
            .then(ReciprocityChecker::default);
        let mut snapshot = self
            .config
            .snapshot
            .as_ref()
            .map(|s| SnapshotBuilder::new(&s.directory, s.memory_budget));

        for input in inputs {
            let input = input?;
            stats.records += 1;

            let (location, entry) = match input {
                Input::Typed(record, entry) => (Location::record(record, None), Some(entry)),
                Input::Raw(source) => {
                    let location = Location::record(source.record, source.line);
                    match source.result {
                        Ok(raw) => {
                            scratch.clear();
                            let entry = self.validator.validate(&raw, &location, &mut scratch);
                            for diagnostic in scratch.drain(..) {
                                collector.push(diagnostic);
                            }
                            (location, entry)
                        }
                        Err(error) => {
                            let code = match error {
                                DecodeError::Json(_) => DiagnosticCode::InputParseError,
                                DecodeError::LineTooLong { .. } => DiagnosticCode::InputLineTooLong,
                            };
                            collector.push(
                                Diagnostic::new(code, error.to_string(), location.clone())
                                    .with_suggestion(
                                        "Each record must be a JSON object matching the URL record schema.",
                                    ),
                            );
                            (location, None)
                        }
                    }
                }
            };

            let entry = entry.filter(|entry| {
                let missing = self
                    .config
                    .namespaces
                    .missing(Namespaces::required_by(entry));
                if missing.is_empty() {
                    return true;
                }
                collector.push(
                    Diagnostic::new(
                        DiagnosticCode::ExtensionNotDeclared,
                        format!(
                            "the entry uses the {} extension, which is not declared",
                            missing.join(", ")
                        ),
                        location.clone(),
                    )
                    .with_suggestion(format!(
                        "Add {} to output.extensions in the configuration.",
                        missing
                            .iter()
                            .map(|p| format!("\"{p}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                );
                false
            });
            let Some(entry) = entry else {
                stats.invalid_records += 1;
                if fail_fast {
                    writing = false;
                }
                continue;
            };
            stats.valid_records += 1;

            if self.config.dedupe && !seen.insert(fingerprint(entry.loc.as_str())) {
                stats.duplicates += 1;
                collector.push(
                    Diagnostic::new(
                        DiagnosticCode::DuplicateUrl,
                        "URL already listed; this occurrence is dropped",
                        location.with_field("loc"),
                    )
                    .with_value(entry.loc.as_str())
                    .with_suggestion("Remove repeated URLs from the source."),
                );
                continue;
            }

            if !writing {
                continue;
            }
            match partitioner.push(&entry) {
                Ok(partition) => {
                    stats.urls_written += 1;
                    if let Some(builder) = snapshot.as_mut() {
                        builder.push(SnapshotRecord {
                            loc: entry.loc.as_str().to_owned(),
                            content: digest128(partitioner.last_block()),
                            partition,
                        })?;
                    }
                    if let Some(checker) = reciprocity.as_mut() {
                        checker.record(&entry, location.record.unwrap_or(0), !self.config.dedupe);
                    }
                }
                Err(PushError::EntryTooLarge { bytes, limit }) => {
                    stats.invalid_records += 1;
                    stats.valid_records -= 1;
                    collector.push(
                        Diagnostic::new(
                            DiagnosticCode::EntryTooLarge,
                            format!(
                                "this entry alone needs {bytes} bytes, above the {limit}-byte file limit"
                            ),
                            location,
                        )
                        .with_suggestion("Raise partitioning.maxBytes or reduce the entry's alternates and images."),
                    );
                    if fail_fast {
                        writing = false;
                    }
                }
                Err(error @ (PushError::PartitionFull { .. } | PushError::NewsLimit { .. })) => {
                    collector.push(
                        Diagnostic::new(
                            DiagnosticCode::PartitionLimitExceeded,
                            error.to_string(),
                            location,
                        )
                        .with_suggestion(
                            "Increase partitioning.partitions; `advanced-sitemap explain` suggests a value.",
                        ),
                    );
                    writing = false;
                }
                Err(PushError::Io(error)) => return Err(EngineError::Io(error)),
                Err(PushError::Xml(error)) => return Err(EngineError::Xml(error)),
            }
        }

        if let Some(checker) = reciprocity {
            // With deduplication, `seen` already holds every accepted loc.
            let known = self.config.dedupe.then_some(&seen);
            for diagnostic in checker.finish(known) {
                collector.push(diagnostic);
            }
        }

        let partitions = if writing {
            match partitioner.finish() {
                Ok(partitions) => Some(partitions),
                Err(PushError::Io(error)) => return Err(EngineError::Io(error)),
                Err(PushError::Xml(error)) => return Err(EngineError::Xml(error)),
                Err(other) => return Err(EngineError::Io(io::Error::other(other.to_string()))),
            }
        } else {
            None
        };

        Ok(StreamOutcome {
            stats,
            collector,
            partitions,
            snapshot,
            reused: Vec::new(),
            compare_with_published: self.config.publish == PublishPolicy::Changed,
            keep_previous: None,
        })
    }

    /// Plans the layout, writes the index and manifest, and publishes or discards.
    fn complete(
        &self,
        outcome: StreamOutcome,
        store: &mut dyn ArtifactStore,
        mode: RunMode,
    ) -> Result<Completed, EngineError> {
        let StreamOutcome {
            stats,
            mut collector,
            partitions,
            snapshot,
            reused,
            compare_with_published,
            keep_previous: _,
        } = outcome;
        let failed_status = match mode {
            RunMode::Publish => RunStatus::Failed,
            RunMode::DryRun => RunStatus::Invalid,
        };
        let blocked = self.config.on_invalid == InvalidPolicy::Fail && collector.has_errors();

        let mut partitions = match partitions {
            Some(partitions) if !blocked => partitions,
            _ => {
                store.discard()?;
                return Ok((failed_status, stats, collector, Vec::new(), None));
            }
        };
        let reused_names: HashSet<String> = reused
            .iter()
            .map(|p| p.staged_name.as_str().to_owned())
            .collect();
        partitions.extend(reused);
        partitions.sort_by_key(|p| p.id);

        if stats.urls_written == 0 {
            collector.push(
                Diagnostic::new(
                    DiagnosticCode::EmptySitemap,
                    "no URL was written; the sitemap is empty",
                    Location::default(),
                )
                .with_suggestion("Check the source and the validation diagnostics."),
            );
        }

        let plan = match plan_layout(&partitions, &self.config.layout) {
            Ok(plan) => plan,
            Err(error) => {
                let (code, suggestion) = match error {
                    LayoutError::PublicBaseUrlMissing { .. } => (
                        DiagnosticCode::PublicBaseUrlMissing,
                        "Set publicBaseUrl to the public URL of the output directory.",
                    ),
                    LayoutError::IndexLimitExceeded { .. } => (
                        DiagnosticCode::IndexLimitExceeded,
                        "Split the site into several independent sitemap sets.",
                    ),
                    LayoutError::InvalidName(reason) => return Err(EngineError::Config(reason)),
                };
                collector.push(
                    Diagnostic::new(code, error.to_string(), Location::default())
                        .with_suggestion(suggestion),
                );
                store.discard()?;
                return Ok((failed_status, stats, collector, Vec::new(), None));
            }
        };

        let mut publications: Vec<Publication> = plan.partitions.clone();
        let mut artifacts: Vec<ArtifactReport> = plan
            .partitions
            .iter()
            .zip(&partitions)
            .map(|(publication, partition)| ArtifactReport {
                name: publication.target.as_str().to_owned(),
                kind: ArtifactKind::Sitemap,
                unchanged: reused_names.contains(publication.staged.as_str()),
                entries: Some(partition.urls),
                bytes: partition.bytes,
            })
            .collect();
        // Reused partitions have no staged file: they stay as published.
        publications.retain(|p| !reused_names.contains(p.staged.as_str()));

        if let Some((name, entries)) = &plan.index {
            let writer = store.create(name)?;
            let mut writer = writer;
            let bytes = write_sitemap_index(&mut writer, entries).map_err(|e| match e {
                XmlError::Io(error) => EngineError::Io(error),
                other => EngineError::Xml(other),
            })?;
            writer.finish()?;
            publications.push(Publication {
                staged: name.clone(),
                target: name.clone(),
            });
            artifacts.push(ArtifactReport {
                name: name.as_str().to_owned(),
                kind: ArtifactKind::Index,
                unchanged: false,
                entries: Some(entries.len() as u64),
                bytes,
            });
        }

        if let Some(robots) = &self.config.robots {
            let (built, diagnostics) = robots.build();
            for diagnostic in diagnostics {
                collector.push(diagnostic);
            }
            if let Some(mut robots) = built {
                let entry_point = plan
                    .index
                    .as_ref()
                    .map(|(name, _)| name.clone())
                    .or_else(|| match plan.partitions.as_slice() {
                        [single] => Some(single.target.clone()),
                        _ => None,
                    });
                match (&self.config.layout.public_base_url, entry_point) {
                    (Some(base), Some(name)) => {
                        if let Ok(url) = base.url().join(name.as_str()) {
                            robots.add_sitemap(url);
                        }
                    }
                    _ => collector.push(
                        Diagnostic::new(
                            DiagnosticCode::RobotsSitemapNotListed,
                            "robots.txt cannot list the generated sitemap without publicBaseUrl",
                            Location::artifact("robots.txt"),
                        )
                        .with_suggestion(
                            "Set publicBaseUrl, or list the sitemap in robots.sitemaps.",
                        ),
                    ),
                }
                let (text, warning) = robots.render_checked();
                if let Some(warning) = warning {
                    collector.push(warning);
                }
                let name = ArtifactName::new("robots.txt")
                    .map_err(|e| EngineError::Config(e.to_string()))?;
                let mut writer = store.create(&name)?;
                writer.write_all(text.as_bytes())?;
                writer.finish()?;
                publications.push(Publication {
                    staged: name.clone(),
                    target: name.clone(),
                });
                artifacts.push(ArtifactReport {
                    name: name.as_str().to_owned(),
                    kind: ArtifactKind::Robots,
                    unchanged: false,
                    entries: None,
                    bytes: text.len() as u64,
                });
            }
        }

        if self.config.manifest {
            let name = self.manifest_name()?;
            let manifest = Manifest::from_artifacts(&artifacts, stats.urls_written);
            if let Some(previous) = store
                .read_published(&name)?
                .as_deref()
                .and_then(Manifest::parse)
            {
                for old in previous.artifacts {
                    if !artifacts.iter().any(|a| a.name == old.name) {
                        collector.push(
                            Diagnostic::new(
                                DiagnosticCode::StaleArtifact,
                                format!(
                                    "{} was published by the previous run and is no longer produced",
                                    old.name
                                ),
                                Location::artifact(&old.name),
                            )
                            .with_suggestion(
                                "Delete it manually once nothing references it; the engine never deletes files.",
                            ),
                        );
                    }
                }
            }
            let bytes = manifest.to_json_bytes();
            let mut writer = store.create(&name)?;
            writer.write_all(&bytes)?;
            writer.finish()?;
            publications.push(Publication {
                staged: name.clone(),
                target: name.clone(),
            });
            artifacts.push(ArtifactReport {
                name: name.as_str().to_owned(),
                kind: ArtifactKind::Manifest,
                unchanged: false,
                entries: None,
                bytes: bytes.len() as u64,
            });
        }

        if mode == RunMode::Publish && compare_with_published {
            let mut kept = Vec::with_capacity(publications.len());
            for publication in publications {
                if store.staged_matches_published(&publication.staged, &publication.target)? {
                    if let Some(artifact) = artifacts
                        .iter_mut()
                        .find(|a| a.name == publication.target.as_str())
                    {
                        artifact.unchanged = true;
                    }
                } else {
                    kept.push(publication);
                }
            }
            publications = kept;
        }

        let status = match mode {
            RunMode::Publish => {
                store.publish(&publications)?;
                RunStatus::Published
            }
            RunMode::DryRun => {
                store.discard()?;
                RunStatus::Valid
            }
        };
        Ok((status, stats, collector, artifacts, snapshot))
    }
}

impl EngineConfig {
    /// Canonical description of the settings that shape the published bytes
    /// (lifecycle invariant I7). Changing its text changes every snapshot's
    /// configuration digest, so it is versioned.
    pub(crate) fn output_settings(&self) -> String {
        let c = self;
        let strategy = match c.strategy {
            PartitionStrategy::Sequential => "sequential".to_owned(),
            PartitionStrategy::StableHash { partitions } => format!("stable-hash:{partitions}"),
            PartitionStrategy::ConsistentHash { partitions } => {
                format!("consistent-hash:{partitions}")
            }
        };
        let index = match c.layout.index {
            IndexMode::Auto => "auto",
            IndexMode::Always => "always",
            IndexMode::Never => "never",
        };
        let ns = c.namespaces;
        format!(
            "settings-v1\nengine={}\nstem={}\nmax-urls={}\nmax-bytes={}\nstrategy={strategy}\nindex={index}\npublic-base-url={}\nnamespaces={}{}{}{}\n",
            crate::ENGINE_VERSION,
            c.layout.stem,
            c.limits.max_urls(),
            c.limits.max_bytes(),
            c.layout
                .public_base_url
                .as_ref()
                .map_or("", |u| u.url().as_str()),
            if ns.xhtml { "xhtml," } else { "" },
            if ns.image { "image," } else { "" },
            if ns.video { "video," } else { "" },
            if ns.news { "news," } else { "" },
        )
    }

    /// Path of the URL-level snapshot, when one is kept.
    pub(crate) fn snapshot_path(&self) -> Option<PathBuf> {
        self.snapshot
            .as_ref()
            .map(|s| s.directory.join(format!("{}.snapshot", self.layout.stem)))
    }
}

impl Engine {
    /// Writes the new snapshot, compares it with the previous one, and keeps
    /// it only for a publishing run (it is written after the sitemaps: I3).
    ///
    /// With `keep_previous`, the records of those partitions are copied from
    /// the previous snapshot, merged in order with the builder's.
    fn finish_snapshot(
        &self,
        builder: SnapshotBuilder,
        keep_previous: Option<&HashSet<u32>>,
        mode: RunMode,
        collector: &mut DiagnosticCollector,
    ) -> Result<ChangesReport, EngineError> {
        let Some(settings) = &self.config.snapshot else {
            return Err(EngineError::Config("snapshot settings missing".to_owned()));
        };
        fs::create_dir_all(&settings.directory)?;
        let path = settings
            .directory
            .join(format!("{}.snapshot", self.config.layout.stem));
        let temp = settings
            .directory
            .join(format!("{}.snapshot.tmp", self.config.layout.stem));
        let config = digest128(self.config.output_settings().as_bytes());

        let file = File::create(&temp)?;
        let mut out = io::BufWriter::new(file);
        match keep_previous {
            None => builder.finish(&config, &mut out)?,
            Some(keep) => {
                let previous = open_snapshot(&path)?
                    .ok_or_else(|| io::Error::other("the previous snapshot disappeared"))?;
                let kept = previous.filter(|record| {
                    record
                        .as_ref()
                        .map_or(true, |record| keep.contains(&record.partition))
                });
                builder.finish_merged(&config, kept, &mut out)?
            }
        };
        let file = out.into_inner().map_err(io::IntoInnerError::into_error)?;
        file.sync_all()?;
        drop(file);

        let result = self.compare_snapshots(&path, &temp, &config, settings.max_listed, collector);
        let finished = match (&result, mode) {
            (Ok(_), RunMode::Publish) => fs::rename(&temp, &path),
            _ => fs::remove_file(&temp),
        };
        let report = result?;
        finished?;
        Ok(report)
    }

    fn compare_snapshots(
        &self,
        previous: &Path,
        current: &Path,
        config: &Digest,
        max_listed: usize,
        collector: &mut DiagnosticCollector,
    ) -> Result<ChangesReport, EngineError> {
        let open_current = || -> io::Result<_> {
            open_snapshot(current)?.ok_or_else(|| io::Error::other("new snapshot disappeared"))
        };
        let attempt = match open_snapshot(previous) {
            Ok(Some(old)) => {
                let config_changed = old.config() != config;
                diff(old, open_current()?, max_listed).map(|set| Some((set, config_changed)))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        };
        let (set, previous_snapshot, config_changed) = match attempt {
            Ok(Some((set, changed))) => (set, true, changed),
            Ok(None) => (
                diff(std::iter::empty(), open_current()?, max_listed)?,
                false,
                false,
            ),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                collector.push(
                    Diagnostic::new(
                        DiagnosticCode::SnapshotInvalid,
                        format!("the previous snapshot cannot be read ({error}); it is replaced"),
                        Location::artifact(previous.display().to_string()),
                    )
                    .with_suggestion(
                        "Nothing to do: the change report starts again from this run.",
                    ),
                );
                (
                    diff(std::iter::empty(), open_current()?, max_listed)?,
                    false,
                    false,
                )
            }
            Err(error) => return Err(EngineError::Io(error)),
        };
        Ok(ChangesReport::new(set, previous_snapshot, config_changed))
    }
}

/// 128-bit in-process fingerprint used for deduplication. Not persisted, so
/// the standard library hasher (fixed keys) is sufficient; two differently
/// tagged 64-bit hashes make accidental collisions negligible.
pub(crate) fn fingerprint(text: &str) -> u128 {
    let mut a = DefaultHasher::new();
    0u8.hash(&mut a);
    text.hash(&mut a);
    let mut b = DefaultHasher::new();
    1u8.hash(&mut b);
    text.hash(&mut b);
    (u128::from(a.finish()) << 64) | u128::from(b.finish())
}

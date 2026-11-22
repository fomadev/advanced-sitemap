//! `delta` mode (ADR-0013): applies a change feed to the published sitemaps
//! without reading the source.
//!
//! The published files are the state. Only the partitions that the feed
//! touches are read back (with the bounded reader), checked against the
//! URL-level snapshot, changed, and written again with their entries in URL
//! order. The other partitions are kept as published. The result is the
//! output of a full generation from the same URLs sorted by `loc`, provided
//! the previous publication was itself produced that way (lifecycle
//! invariant I1).
//!
//! The run is refused, before anything is written, when it cannot meet I1:
//! no hash strategy, no snapshot, settings changed since the snapshot
//! (I7), cross-entry hreflang checks enabled, a touched file modified outside
//! the engine or holding elements the reader does not keep, or a previous
//! publication that does not name its partitions `<stem>-<n>.xml`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;

use sitemap_index::{PartitionSummary, PushError, partition_name};
use sitemap_model::{Diagnostic, DiagnosticCode, Location, RawUrlRecord, UrlEntry, W3cDateTime};
use sitemap_protocol::{ChangeRecord, DecodeError, SourceRecord};
use sitemap_reader::{InspectOptions, Item, inspect};
use sitemap_snapshot::{
    Digest, Record as SnapshotRecord, SequenceDigest, SnapshotBuilder, digest128,
};
use sitemap_storage::ArtifactStore;
use sitemap_xml::{Namespaces, encode_url};

use super::{Engine, EngineError, InvalidPolicy, RunMode, StreamOutcome, open_snapshot};
use crate::manifest::Manifest;
use crate::report::{DiagnosticCollector, GenerationReport, Stats};

fn refuse(reason: impl Into<String>) -> EngineError {
    EngineError::DeltaRefused(reason.into())
}

/// What the feed asks for each URL: the new entry, or `None` to delete it.
type Operations = BTreeMap<String, Option<UrlEntry>>;

impl Engine {
    /// Applies a change feed to the sitemaps published in `store`
    /// (`delta` mode, ADR-0013). Requires a hash strategy, `snapshot`
    /// settings and a previous publication made with the same settings.
    ///
    /// Operations are applied in order; the last one on a URL wins.
    pub fn apply_changes<I>(
        &self,
        changes: I,
        store: &mut dyn ArtifactStore,
        mode: RunMode,
    ) -> Result<GenerationReport, EngineError>
    where
        I: IntoIterator<Item = io::Result<SourceRecord<ChangeRecord>>>,
    {
        let description = store.describe();
        let config = &self.config;
        let _state_lock = self.lock_destination(store)?;
        if config.strategy.hash_partitions().is_none() {
            return Err(refuse(
                "the delta mode needs the stable-hash or consistent-hash strategy",
            ));
        }
        if config.hreflang_reciprocity {
            return Err(refuse(
                "hreflang reciprocity is checked across all entries; run a full generation",
            ));
        }
        if !config.manifest {
            return Err(refuse(
                "the delta mode needs the manifest of the previous publication",
            ));
        }
        let (Some(settings), Some(snapshot_path)) = (&config.snapshot, config.snapshot_path())
        else {
            return Err(refuse(
                "the delta mode needs state.directory (the URL snapshot)",
            ));
        };
        let settings_digest = digest128(config.output_settings().as_bytes());

        let mut stats = Stats::default();
        let mut collector = DiagnosticCollector::new(config.max_diagnostics);
        let operations = self.read_operations(changes, &mut stats, &mut collector)?;
        let blocked = config.on_invalid == InvalidPolicy::Fail && collector.has_errors();
        if blocked {
            store.set_compare_with_published(false);
            let outcome = StreamOutcome {
                stats,
                collector,
                partitions: None,
                snapshot: None,
                reused: Vec::new(),
                compare_with_published: false,
                keep_previous: None,
            };
            return self.finish_run(outcome, store, mode, description);
        }

        // Partitions touched by the feed, with their operations.
        let mut touched: BTreeMap<u32, Vec<(&String, &Option<UrlEntry>)>> = BTreeMap::new();
        for (loc, operation) in &operations {
            if let Some(partition) = config.strategy.hash_partition(loc) {
                touched.entry(partition).or_default().push((loc, operation));
            }
        }

        // First pass over the snapshot: expected content of the touched
        // partitions, URL count of every partition.
        let previous = self.read_previous(&snapshot_path, &settings_digest, &touched)?;

        // The previous manifest and index describe the partitions kept as
        // published.
        let stem = &config.layout.stem;
        let manifest = store
            .read_published(&self.manifest_name()?)?
            .as_deref()
            .and_then(Manifest::parse)
            .ok_or_else(|| refuse("the previous publication has no readable manifest"))?;
        let lastmods = self.previous_index_lastmods(&*store, &manifest)?;
        let mut reused = Vec::new();
        for (&partition, &urls) in &previous.counts {
            if touched.contains_key(&partition) {
                continue;
            }
            let name = partition_name(stem, partition)?;
            let artifact = manifest
                .artifacts
                .iter()
                .find(|a| a.name == name.as_str() && a.kind == "sitemap")
                .ok_or_else(|| {
                    refuse(format!(
                        "the previous publication does not list {name}; run a full generation"
                    ))
                })?;
            if artifact.entries != Some(urls) {
                return Err(refuse(format!(
                    "{name} lists {:?} URLs but the snapshot has {urls}; run a full generation",
                    artifact.entries
                )));
            }
            reused.push(PartitionSummary {
                id: partition,
                lastmod: lastmods.get(name.as_str()).cloned().flatten(),
                staged_name: name,
                urls,
                bytes: artifact.bytes,
            });
        }
        let kept_urls: u64 = reused.iter().map(|p| p.urls).sum();

        store.set_compare_with_published(mode == RunMode::Publish);
        let written = self.rewrite_touched(
            store,
            &touched,
            &previous,
            settings,
            &mut stats,
            &mut collector,
        );
        let (partitions, builder) = match written {
            Ok(Some(done)) => done,
            Ok(None) => {
                // A partition is full: reported, nothing is published.
                let outcome = StreamOutcome {
                    stats,
                    collector,
                    partitions: None,
                    snapshot: None,
                    reused: Vec::new(),
                    compare_with_published: false,
                    keep_previous: None,
                };
                return self.finish_run(outcome, store, mode, description);
            }
            Err(error) => {
                let _ = store.discard();
                return Err(error);
            }
        };

        // A single file is published under the main name (no index): only a
        // staged partition can be renamed.
        if partitions.is_empty()
            && reused.len() == 1
            && config.layout.index == sitemap_index::IndexMode::Auto
        {
            let _ = store.discard();
            return Err(refuse(
                "only one partition would remain and it is not rewritten; run a full generation",
            ));
        }

        // The records of the partitions kept as published are copied from
        // the previous snapshot when the new one is written, already sorted.
        let kept: HashSet<u32> = reused.iter().map(|p| p.id).collect();
        stats.urls_written += kept_urls;
        let outcome = StreamOutcome {
            stats,
            collector,
            partitions: Some(partitions),
            snapshot: Some(builder),
            reused,
            compare_with_published: true,
            keep_previous: Some(kept),
        };
        self.finish_run(outcome, store, mode, description)
    }

    /// Decodes and validates the feed. The last operation on a URL wins;
    /// earlier ones are counted as duplicates.
    fn read_operations<I>(
        &self,
        changes: I,
        stats: &mut Stats,
        collector: &mut DiagnosticCollector,
    ) -> Result<Operations, EngineError>
    where
        I: IntoIterator<Item = io::Result<SourceRecord<ChangeRecord>>>,
    {
        let mut operations = Operations::new();
        let mut scratch = Vec::new();
        for item in changes {
            let item = item?;
            stats.records += 1;
            let location = Location::record(item.record, item.line);
            let (raw, delete) = match item.result {
                Ok(ChangeRecord::Upsert { url }) => (url, false),
                Ok(ChangeRecord::Delete { loc }) => (
                    RawUrlRecord {
                        loc: Some(loc),
                        ..RawUrlRecord::default()
                    },
                    true,
                ),
                Err(error) => {
                    let code = match error {
                        DecodeError::Json(_) => DiagnosticCode::InputParseError,
                        DecodeError::LineTooLong { .. } => DiagnosticCode::InputLineTooLong,
                    };
                    collector.push(
                        Diagnostic::new(code, error.to_string(), location).with_suggestion(
                            "Each line must be {\"op\":\"upsert\",\"url\":{...}} or {\"op\":\"delete\",\"loc\":\"...\"}.",
                        ),
                    );
                    stats.invalid_records += 1;
                    continue;
                }
            };
            scratch.clear();
            let entry = self.validator.validate(&raw, &location, &mut scratch);
            for diagnostic in scratch.drain(..) {
                collector.push(diagnostic);
            }
            let entry = entry.filter(|entry| {
                let missing = self
                    .config
                    .namespaces
                    .missing(Namespaces::required_by(entry));
                if !missing.is_empty() {
                    collector.push(Diagnostic::new(
                        DiagnosticCode::ExtensionNotDeclared,
                        format!(
                            "the entry uses the {} extension, which is not declared",
                            missing.join(", ")
                        ),
                        location.clone(),
                    ));
                }
                missing.is_empty()
            });
            let Some(entry) = entry else {
                stats.invalid_records += 1;
                continue;
            };
            stats.valid_records += 1;
            let loc = entry.loc.as_str().to_owned();
            let operation = (!delete).then_some(entry);
            if operations.insert(loc, operation).is_some() {
                stats.duplicates += 1;
            }
        }
        Ok(operations)
    }

    fn read_previous(
        &self,
        path: &std::path::Path,
        settings_digest: &Digest,
        touched: &BTreeMap<u32, Vec<(&String, &Option<UrlEntry>)>>,
    ) -> Result<Previous, EngineError> {
        let reader = open_snapshot(path)
            .map_err(|e| {
                refuse(format!(
                    "the snapshot cannot be read ({e}); run a full generation"
                ))
            })?
            .ok_or_else(|| refuse("no snapshot yet; run a full generation first"))?;
        if reader.config() != settings_digest {
            return Err(refuse(
                "the settings changed since the last publication; run a full generation",
            ));
        }
        let mut previous = Previous::default();
        for record in reader {
            let record = record.map_err(|e| {
                refuse(format!(
                    "the snapshot cannot be read ({e}); run a full generation"
                ))
            })?;
            *previous.counts.entry(record.partition).or_default() += 1;
            if touched.contains_key(&record.partition) {
                previous
                    .expected
                    .entry(record.partition)
                    .or_default()
                    .update(&record.loc, &record.content);
            }
        }
        Ok(previous)
    }

    /// `lastmod` of each child listed in the previous index, by file name.
    fn previous_index_lastmods(
        &self,
        store: &dyn ArtifactStore,
        manifest: &Manifest,
    ) -> Result<HashMap<String, Option<W3cDateTime>>, EngineError> {
        let mut lastmods = HashMap::new();
        let Some(index) = manifest.artifacts.iter().find(|a| a.kind == "index") else {
            return Ok(lastmods);
        };
        let Some(base) = &self.config.layout.public_base_url else {
            return Ok(lastmods);
        };
        let name = sitemap_storage::ArtifactName::new(index.name.clone())
            .map_err(|e| EngineError::Config(e.to_string()))?;
        let bytes = store
            .read_published(&name)?
            .ok_or_else(|| refuse(format!("{} is listed but not published", index.name)))?;
        let mut children = Vec::new();
        let report = inspect(&bytes[..], &InspectOptions::default(), |item| {
            if let Item::Sitemap(entry) = item {
                children.push(entry);
            }
        })?;
        if !report.complete || report.diagnostics.iter().any(Diagnostic::is_error) {
            return Err(refuse(format!(
                "{} cannot be read back; run a full generation",
                index.name
            )));
        }
        let prefix = base.url().as_str();
        for child in children {
            if let Some(file) = child.loc.as_str().strip_prefix(prefix) {
                lastmods.insert(file.to_owned(), child.lastmod);
            }
        }
        Ok(lastmods)
    }

    /// Reads, checks, changes and rewrites each touched partition, one at a
    /// time. `Ok(None)` when a partition overflows (reported).
    fn rewrite_touched(
        &self,
        store: &mut dyn ArtifactStore,
        touched: &BTreeMap<u32, Vec<(&String, &Option<UrlEntry>)>>,
        previous: &Previous,
        settings: &super::SnapshotSettings,
        stats: &mut Stats,
        collector: &mut DiagnosticCollector,
    ) -> Result<Option<(Vec<PartitionSummary>, SnapshotBuilder)>, EngineError> {
        let config = &self.config;
        let mut builder = SnapshotBuilder::new(&settings.directory, settings.memory_budget);
        let mut partitioner = sitemap_index::Partitioner::new(
            store,
            config.strategy,
            config.limits,
            config.namespaces,
            &config.layout.stem,
        );
        let empty = SequenceDigest::default();
        for (&partition, operations) in touched {
            let expected = previous.expected.get(&partition).unwrap_or(&empty);
            let mut entries = read_partition(
                partitioner.store(),
                &config.layout.stem,
                partition,
                expected,
                config.limits.max_bytes(),
            )?;
            for (loc, operation) in operations {
                match operation {
                    Some(entry) => {
                        entries.insert((*loc).clone(), entry.clone());
                    }
                    None => {
                        entries.remove(*loc);
                    }
                }
            }
            for (loc, entry) in entries {
                match partitioner.push(&entry) {
                    Ok(placed) => {
                        stats.urls_written += 1;
                        builder.push(SnapshotRecord {
                            loc,
                            content: digest128(partitioner.last_block()),
                            partition: placed,
                        })?;
                    }
                    Err(
                        error @ (PushError::PartitionFull { .. } | PushError::NewsLimit { .. }),
                    ) => {
                        collector.push(
                            Diagnostic::new(
                                DiagnosticCode::PartitionLimitExceeded,
                                error.to_string(),
                                Location::default(),
                            )
                            .with_value(&loc)
                            .with_suggestion(
                                "Increase partitioning.partitions and run a full generation.",
                            ),
                        );
                        return Ok(None);
                    }
                    Err(PushError::EntryTooLarge { bytes, limit }) => {
                        collector.push(Diagnostic::new(
                            DiagnosticCode::EntryTooLarge,
                            format!(
                                "this entry alone needs {bytes} bytes, above the {limit}-byte file limit"
                            ),
                            Location::default(),
                        )
                        .with_value(&loc));
                        return Ok(None);
                    }
                    Err(PushError::Io(error)) => return Err(EngineError::Io(error)),
                    Err(PushError::Xml(error)) => return Err(EngineError::Xml(error)),
                }
            }
            // Each partition is complete: release its buffers now.
            partitioner
                .close_partition(partition)
                .map_err(|e| match e {
                    PushError::Io(error) => EngineError::Io(error),
                    PushError::Xml(error) => EngineError::Xml(error),
                    other => EngineError::Io(io::Error::other(other.to_string())),
                })?;
        }
        let partitions = partitioner.finish_partial().map_err(|e| match e {
            PushError::Io(error) => EngineError::Io(error),
            PushError::Xml(error) => EngineError::Xml(error),
            other => EngineError::Io(io::Error::other(other.to_string())),
        })?;
        Ok(Some((partitions, builder)))
    }
}

/// What the previous snapshot says.
#[derive(Default)]
struct Previous {
    /// URLs per partition.
    counts: BTreeMap<u32, u64>,
    /// Expected content of the touched partitions (snapshot records in
    /// `loc` order), in constant memory per partition.
    expected: HashMap<u32, SequenceDigest>,
}

/// Reads a published partition back and checks it against the snapshot:
/// same URLs, same encoded content (compared as a [`SequenceDigest`] in
/// `loc` order). Returns its entries by URL.
fn read_partition(
    store: &dyn ArtifactStore,
    stem: &str,
    partition: u32,
    expected: &SequenceDigest,
    max_bytes: u64,
) -> Result<BTreeMap<String, UrlEntry>, EngineError> {
    let mut entries = BTreeMap::new();
    if expected.records() == 0 {
        return Ok(entries);
    }
    let name = partition_name(stem, partition)?;
    let bytes = store
        .read_published(&name)?
        .ok_or_else(|| refuse(format!("{name} is in the snapshot but not published")))?;
    let mut options = InspectOptions::default();
    options.limits.max_bytes = max_bytes;
    let report = inspect(&bytes[..], &options, |item| {
        if let Item::Url(entry) = item {
            entries.insert(entry.loc.as_str().to_owned(), entry);
        }
    })?;
    drop(bytes);
    let modified = |detail: &str| {
        refuse(format!(
            "{name} differs from the snapshot ({detail}): it was changed outside the engine; run a full generation"
        ))
    };
    if !report.complete || report.diagnostics.iter().any(Diagnostic::is_error) {
        return Err(modified("it cannot be read back without errors"));
    }
    if !report.unsupported_extensions.is_empty() {
        return Err(refuse(format!(
            "{name} holds elements the reader does not keep ({}); run a full generation",
            report
                .unsupported_extensions
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let mut found = SequenceDigest::default();
    let mut block = Vec::new();
    for (loc, entry) in &entries {
        block.clear();
        encode_url(entry, &mut block).map_err(EngineError::Xml)?;
        found.update(loc, &digest128(&block));
    }
    if found.records() != expected.records() {
        return Err(modified(&format!(
            "{} entries instead of {}",
            found.records(),
            expected.records()
        )));
    }
    if found.finish() != expected.finish() {
        return Err(modified("an entry has different content"));
    }
    Ok(entries)
}

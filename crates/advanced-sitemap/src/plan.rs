//! Execution planning and the `explain` report.
//!
//! [`explain`] describes what a run would do without writing anything. It
//! reads at most a sample of the input. Every quantity says whether it was
//! measured, estimated from the sample, or taken from the configuration.

use std::fs::File;
use std::io::{BufRead, BufReader};

use serde::Serialize;
use sitemap_index::{IndexMode, PartitionStrategy};
use sitemap_model::Location;
use sitemap_protocol::SourceFormat;
use sitemap_validation::{ValidationMode, Validator};
use sitemap_xml::{URLSET_FOOTER, encode_url, urlset_header};

use crate::config::{InputSpec, ResolvedConfig, open_input};
use crate::engine::{Engine, EngineError, InvalidPolicy};

mod execution;

pub use execution::{Candidate, CostModel, ExecutionPlan};

/// A number with its provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum Quantity {
    /// Measured on the actual input.
    Measured(u64),
    /// Extrapolated from a sample.
    Estimated(u64),
    /// Taken from the configuration.
    Configured(u64),
    /// Not known before running.
    Unknown,
}

impl std::fmt::Display for Quantity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Quantity::Measured(v) => write!(f, "{v} (measured)"),
            Quantity::Estimated(v) => write!(f, "~{v} (estimated)"),
            Quantity::Configured(v) => write!(f, "{v} (configured)"),
            Quantity::Unknown => f.write_str("unknown"),
        }
    }
}

impl Quantity {
    /// The number, whatever its provenance; `None` when unknown.
    pub fn value(self) -> Option<u64> {
        match self {
            Quantity::Measured(v) | Quantity::Estimated(v) | Quantity::Configured(v) => Some(v),
            Quantity::Unknown => None,
        }
    }

    fn derive(self, value: u64) -> Quantity {
        match self {
            Quantity::Measured(_) | Quantity::Configured(_) => Quantity::Measured(value),
            Quantity::Estimated(_) => Quantity::Estimated(value),
            Quantity::Unknown => Quantity::Unknown,
        }
    }
}

/// Measurements on the sampled records.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleStats {
    /// Records sampled.
    pub records: u64,
    /// Valid records in the sample.
    pub valid: u64,
    /// Invalid records in the sample.
    pub invalid: u64,
    /// Average `<url>` block size of the valid sampled records, in bytes.
    pub average_url_bytes: u64,
}

/// Input section of a plan.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputPlan {
    /// `inline`, `json` or `jsonl`.
    pub source: &'static str,
    /// `in-memory` or `streaming`.
    pub reading: &'static str,
    /// Size of the input file.
    pub bytes: Quantity,
    /// Number of records.
    pub records: Quantity,
    /// Sample measurements.
    pub sample: Option<SampleStats>,
}

/// Partitioning section of a plan.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitioningPlan {
    /// `sequential`, `stable-hash` or `consistent-hash`.
    pub strategy: &'static str,
    /// Configured partition count (hash strategies).
    pub partitions: Quantity,
    /// URLs per file.
    pub max_urls: u32,
    /// Bytes per file.
    pub max_bytes: u64,
    /// Sitemap files the run would produce.
    pub files: Quantity,
    /// Suggested partition count for a hash strategy (20% headroom), when computable.
    pub suggested_partitions: Option<u64>,
}

/// Effect of the configured partitioning on the previous publication,
/// computed from its URL-level snapshot (`state.directory`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    /// URLs of the previous publication.
    pub previous_urls: u64,
    /// Highest partition number of the previous publication.
    pub previous_partitions: u32,
    /// The settings that shape the output differ from the previous run's.
    pub config_changed: bool,
    /// Previous URLs that the configured strategy would put in another file.
    /// `measured` (exact) for a hash strategy; `unknown` for `sequential`,
    /// whose placement depends on the input order.
    pub moved: Quantity,
    /// URLs the fullest partition would hold, counting the previous URLs
    /// only (hash strategies).
    pub fullest_partition_urls: Option<u64>,
}

/// What a run would do.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Input.
    pub input: InputPlan,
    /// `standard` or `strict`.
    pub validation: &'static str,
    /// `fail` or `skip`.
    pub on_invalid: &'static str,
    /// Deduplication enabled.
    pub dedupe: bool,
    /// Memory used by deduplication.
    pub dedupe_memory_bytes: Quantity,
    /// Partitioning.
    pub partitioning: PartitioningPlan,
    /// `none`, `single-file`, or `index`.
    pub output_layout: &'static str,
    /// Output directory.
    pub output_directory: String,
    /// Migration from the previous publication, when a snapshot is kept and
    /// readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration: Option<MigrationPlan>,
    /// Execution mode the run would use, the modes considered and their
    /// estimated costs.
    pub execution: ExecutionPlan,
    /// Observations and warnings.
    pub notes: Vec<String>,
}

/// Peak memory per deduplicated URL (128-bit fingerprint plus hash table
/// overhead, including the moment the table grows): 43 to 54 bytes
/// measured at 1 M and 10 M URLs. Reported as an estimate.
const DEDUPE_BYTES_PER_URL: u64 = 48;

/// Builds a plan, sampling at most `sample_size` records, with the
/// reference cost model. No artifact is written.
pub fn explain(resolved: &ResolvedConfig, sample_size: usize) -> Result<Plan, EngineError> {
    explain_with(resolved, sample_size, &CostModel::REFERENCE)
}

/// Plans an `apply` of `changes` without writing anything: validates the
/// feed, finds the partitions it touches, checks every precondition of the
/// delta mode, and compares its cost with regenerating from the source.
pub fn explain_apply<I>(
    resolved: &ResolvedConfig,
    changes: I,
    model: &CostModel,
) -> Result<ExecutionPlan, EngineError>
where
    I: IntoIterator<
        Item = std::io::Result<sitemap_protocol::SourceRecord<sitemap_protocol::ChangeRecord>>,
    >,
{
    let engine = Engine::new(resolved.engine.clone())?;
    let store = sitemap_storage::FsStore::new(&resolved.output_dir);
    let plan = engine.plan_delta(changes, &store)?;
    let url_bytes = plan
        .touched_bytes
        .checked_div(plan.touched_urls)
        .unwrap_or(170);
    let facts = execution::DeltaFacts {
        source_available: resolved.input.is_some(),
        url_bytes,
        snapshot_budget: resolved
            .engine
            .snapshot
            .as_ref()
            .map_or(0, |s| s.memory_budget as u64),
        plan,
    };
    Ok(execution::plan_apply(
        &facts,
        model,
        engine.state_token(&store)?,
    ))
}

/// Like [`explain`], with another cost model.
pub fn explain_with(
    resolved: &ResolvedConfig,
    sample_size: usize,
    model: &CostModel,
) -> Result<Plan, EngineError> {
    let config = &resolved.engine;
    let mut notes = Vec::new();

    let Some(input) = &resolved.input else {
        return Err(EngineError::Config(
            "no input: provide `urls`, `source` or --input".into(),
        ));
    };

    let (source, reading, bytes, mut records) = match input {
        InputSpec::Inline(values) => (
            "inline",
            "in-memory",
            Quantity::Unknown,
            Quantity::Measured(values.len() as u64),
        ),
        InputSpec::File { path, format, .. } => {
            let size = std::fs::metadata(path)
                .map_err(|e| EngineError::Config(format!("{}: {e}", path.display())))?
                .len();
            match format {
                SourceFormat::Json => (
                    "json",
                    "in-memory",
                    Quantity::Measured(size),
                    Quantity::Unknown,
                ),
                SourceFormat::Jsonl => (
                    "jsonl",
                    "streaming",
                    Quantity::Measured(size),
                    estimate_jsonl_records(path, size, sample_size)?,
                ),
            }
        }
    };

    // Sample records: validate and encode them exactly as a run would.
    let validator = Validator::new(config.validation.clone());
    let mut sample = SampleStats {
        records: 0,
        valid: 0,
        invalid: 0,
        average_url_bytes: 0,
    };
    let mut block_total = 0u64;
    let mut scratch = Vec::new();
    let mut block = Vec::new();
    let mut reader = open_input(input)?;
    for item in reader.by_ref().take(sample_size) {
        let item = item?;
        sample.records += 1;
        let entry = item.result.ok().and_then(|raw| {
            scratch.clear();
            validator.validate(
                &raw,
                &Location::record(item.record, item.line),
                &mut scratch,
            )
        });
        match entry {
            Some(entry) => {
                block.clear();
                if encode_url(&entry, &mut block).is_ok() {
                    sample.valid += 1;
                    block_total += block.len() as u64;
                } else {
                    sample.invalid += 1;
                }
            }
            None => sample.invalid += 1,
        }
    }
    let exhausted = reader.next().is_none();
    if sample.valid > 0 {
        sample.average_url_bytes = block_total.div_ceil(sample.valid);
    }
    if matches!(
        input,
        InputSpec::File {
            format: SourceFormat::Json,
            ..
        }
    ) {
        // The JSON reader holds the whole array: the count is exact.
        records = Quantity::Measured(sample.records + reader.count() as u64);
    } else if exhausted && records.value().is_some() {
        records = Quantity::Measured(sample.records);
    }

    if sample.invalid > 0 {
        notes.push(format!(
            "{} of {} sampled records are invalid; run `validate` for details",
            sample.invalid, sample.records
        ));
    }

    // Valid URLs expected, extrapolated from the sample's valid ratio.
    let valid_urls = records
        .value()
        .map(|n| (n * sample.valid).checked_div(sample.records).unwrap_or(0));
    let valid_quantity = match (
        valid_urls,
        sample.records > 0 && sample.records < records.value().unwrap_or(0),
    ) {
        (Some(v), true) => Quantity::Estimated(v),
        (Some(v), false) => records.derive(v),
        (None, _) => Quantity::Unknown,
    };

    let header = urlset_header(config.namespaces).len() as u64;
    let footer = URLSET_FOOTER.len() as u64;
    let room = config.limits.max_bytes().saturating_sub(header + footer);
    let max_urls = u64::from(config.limits.max_urls());
    let per_file = room
        .checked_div(sample.average_url_bytes)
        .map_or(max_urls, |n| n.clamp(1, max_urls));

    let (strategy, partitions_quantity, files, suggested) = match config.strategy {
        PartitionStrategy::Sequential => {
            let files = valid_quantity.value().map_or(Quantity::Unknown, |n| {
                valid_quantity.derive(n.div_ceil(per_file).max(1))
            });
            ("sequential", Quantity::Unknown, files, None)
        }
        PartitionStrategy::StableHash { partitions }
        | PartitionStrategy::ConsistentHash { partitions } => {
            let configured = u64::from(partitions.get());
            let suggested = valid_quantity
                .value()
                .map(|n| (n * 5).div_ceil(per_file * 4).max(1));
            if let Some(n) = valid_quantity.value() {
                let per_partition = n.div_ceil(configured);
                if per_partition > per_file {
                    notes.push(format!(
                        "about {per_partition} URLs per partition expected, above the ~{per_file} that fit in a file: the run would fail with PARTITION_LIMIT_EXCEEDED"
                    ));
                }
            }
            (
                config.strategy.name(),
                Quantity::Configured(configured),
                Quantity::Configured(configured),
                suggested,
            )
        }
    };

    let multiple = files.value().is_some_and(|n| n > 1);
    let output_layout = match config.layout.index {
        IndexMode::Always => "index",
        IndexMode::Never => {
            if multiple {
                "files-without-index"
            } else {
                "single-file"
            }
        }
        IndexMode::Auto => {
            if multiple {
                "index"
            } else {
                "single-file"
            }
        }
    };
    if output_layout == "index" && config.layout.public_base_url.is_none() {
        notes.push(
            "an index is expected but publicBaseUrl is not set: the run would fail with PUBLIC_BASE_URL_MISSING".into(),
        );
    }
    if matches!(
        input,
        InputSpec::File {
            format: SourceFormat::Json,
            ..
        }
    ) {
        notes.push("JSON arrays are loaded in memory; use JSONL to stream large inputs".into());
    }

    let migration = match config.snapshot_path() {
        Some(path) => match measure_migration(config, &path) {
            Ok(migration) => migration,
            Err(error) => {
                notes.push(format!(
                    "the previous snapshot cannot be read ({error}); no migration cost"
                ));
                None
            }
        },
        None => None,
    };
    if let Some(m) = &migration {
        if let Quantity::Measured(moved) = m.moved
            && moved > 0
        {
            notes.push(format!(
                    "PARTITION_MIGRATION_COST: {moved} of the {} previously published URLs would change file",
                    m.previous_urls
                ));
        }
        if let Some(fullest) = m.fullest_partition_urls {
            let limit = u64::from(config.limits.max_urls());
            if fullest * 10 > limit * 9 {
                notes.push(format!(
                    "PARTITION_HEADROOM_LOW: a partition would hold {fullest} of its {limit} URLs with the previous URLs alone; increase partitions"
                ));
            }
        }
    }

    let dedupe_memory = if config.dedupe {
        valid_quantity.value().map_or(Quantity::Unknown, |n| {
            Quantity::Estimated(n * DEDUPE_BYTES_PER_URL)
        })
    } else {
        Quantity::Measured(0)
    };

    let engine = Engine::new(config.clone())?;
    let store = sitemap_storage::FsStore::new(&resolved.output_dir);
    let facts = execution::Facts {
        urls: valid_quantity,
        url_bytes: sample.average_url_bytes,
        input_bytes: bytes,
        files,
        published: engine.previous_publication_exists(&store)?,
        snapshot: config.snapshot.is_some(),
        snapshot_budget: config
            .snapshot
            .as_ref()
            .map_or(0, |s| s.memory_budget as u64),
        dedupe: config.dedupe,
    };
    let execution = execution::plan_generate(
        &facts,
        config.publish.mode_name(),
        model,
        engine.state_token(&store)?,
    );

    Ok(Plan {
        input: InputPlan {
            source,
            reading,
            bytes,
            records,
            sample: (sample.records > 0).then_some(sample),
        },
        validation: match config.validation.mode {
            ValidationMode::Standard => "standard",
            ValidationMode::Strict => "strict",
        },
        on_invalid: match config.on_invalid {
            InvalidPolicy::Fail => "fail",
            InvalidPolicy::Skip => "skip",
        },
        dedupe: config.dedupe,
        dedupe_memory_bytes: dedupe_memory,
        partitioning: PartitioningPlan {
            strategy,
            partitions: partitions_quantity,
            max_urls: config.limits.max_urls(),
            max_bytes: config.limits.max_bytes(),
            files,
            suggested_partitions: suggested,
        },
        output_layout,
        output_directory: resolved.output_dir.display().to_string(),
        migration,
        execution,
        notes,
    })
}

/// Estimates the number of records of a JSONL file from the average size of
/// its first non-blank lines. Exact when the sample reaches the end of file.
fn estimate_jsonl_records(
    path: &std::path::Path,
    size: u64,
    sample_size: usize,
) -> Result<Quantity, EngineError> {
    let file =
        File::open(path).map_err(|e| EngineError::Config(format!("{}: {e}", path.display())))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let (mut lines, mut bytes) = (0u64, 0u64);
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(Quantity::Measured(lines));
        }
        bytes += read as u64;
        if !line.iter().all(u8::is_ascii_whitespace) {
            lines += 1;
        }
        if lines as usize >= sample_size {
            break;
        }
    }
    if bytes == 0 {
        return Ok(Quantity::Measured(0));
    }
    Ok(Quantity::Estimated(size * lines / bytes))
}

/// Reads the previous snapshot and recomputes the placement of every URL
/// under the configured strategy. `Ok(None)` when there is no snapshot yet.
fn measure_migration(
    config: &crate::EngineConfig,
    path: &std::path::Path,
) -> std::io::Result<Option<MigrationPlan>> {
    let Some(reader) = sitemap_snapshot::open_file(path)? else {
        return Ok(None);
    };
    let config_changed =
        *reader.config() != sitemap_snapshot::digest128(config.output_settings().as_bytes());
    let mut loads = vec![
        0u64;
        config
            .strategy
            .hash_partitions()
            .map_or(0, |n| n.get() as usize)
    ];
    let (mut previous_urls, mut previous_partitions, mut moved) = (0u64, 0u32, 0u64);
    for record in reader {
        let record = record?;
        previous_urls += 1;
        previous_partitions = previous_partitions.max(record.partition);
        if let Some(partition) = config.strategy.hash_partition(&record.loc) {
            if partition != record.partition {
                moved += 1;
            }
            if let Some(load) = loads.get_mut(partition as usize - 1) {
                *load += 1;
            }
        }
    }
    let hashed = config.strategy.hash_partitions().is_some();
    Ok(Some(MigrationPlan {
        previous_urls,
        previous_partitions,
        config_changed,
        moved: if hashed {
            Quantity::Measured(moved)
        } else {
            Quantity::Unknown
        },
        fullest_partition_urls: loads.iter().copied().max(),
    }))
}

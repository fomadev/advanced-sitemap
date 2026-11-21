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
use sitemap_xml::{Namespaces, URLSET_FOOTER, encode_url, urlset_header};

use crate::config::{InputSpec, ResolvedConfig, open_input};
use crate::engine::{EngineError, InvalidPolicy};

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
    fn value(self) -> Option<u64> {
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
    /// `sequential` or `stable-hash`.
    pub strategy: &'static str,
    /// Configured partition count (stable-hash).
    pub partitions: Quantity,
    /// URLs per file.
    pub max_urls: u32,
    /// Bytes per file.
    pub max_bytes: u64,
    /// Sitemap files the run would produce.
    pub files: Quantity,
    /// Suggested stable-hash partition count (20% headroom), when computable.
    pub suggested_partitions: Option<u64>,
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
    /// Observations and warnings.
    pub notes: Vec<String>,
}

/// Approximate memory per deduplicated URL (128-bit fingerprint plus hash
/// table overhead). An approximation, reported as an estimate.
const DEDUPE_BYTES_PER_URL: u64 = 32;

/// Builds a plan, sampling at most `sample_size` records. No artifact is written.
pub fn explain(resolved: &ResolvedConfig, sample_size: usize) -> Result<Plan, EngineError> {
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

    let header = urlset_header(Namespaces::ALL).len() as u64;
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
        PartitionStrategy::StableHash { partitions } => {
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
                "stable-hash",
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

    let dedupe_memory = if config.dedupe {
        valid_quantity.value().map_or(Quantity::Unknown, |n| {
            Quantity::Estimated(n * DEDUPE_BYTES_PER_URL)
        })
    } else {
        Quantity::Measured(0)
    };

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

use std::fs::File;
use std::io::{self, BufReader};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use sitemap_index::{IndexMode, Layout, PartitionLimits, PartitionStrategy, PublicBaseUrl};
use sitemap_model::limits::{MAX_SITEMAP_BYTES, MAX_URLS_PER_SITEMAP};
use sitemap_model::{SitemapUrl, W3cDateTime};
use sitemap_protocol::{
    ConfigDocument, DEFAULT_MAX_DOCUMENT_BYTES, DEFAULT_MAX_LINE_BYTES, ExtensionSetting,
    IndexSetting, InvalidPolicySetting, JsonArrayReader, JsonlReader, PublishSetting, SourceFormat,
    SourceRecord, StrategySetting, ValidationModeSetting,
};
use sitemap_validation::{ValidationMode, ValidationOptions};
use sitemap_xml::Namespaces;

use crate::engine::{EngineConfig, EngineError, InvalidPolicy, PublishPolicy, SnapshotSettings};

/// Where records come from.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum InputSpec {
    /// Records embedded in the configuration document.
    Inline(Vec<serde_json::Value>),
    /// A file.
    File {
        /// Path to the file.
        path: PathBuf,
        /// Format.
        format: SourceFormat,
        /// Maximum JSONL line length.
        max_line_bytes: usize,
    },
}

/// A configuration document resolved into engine settings.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct ResolvedConfig {
    /// Engine settings.
    pub engine: EngineConfig,
    /// Input, if the document defines one.
    pub input: Option<InputSpec>,
    /// Output directory.
    pub output_dir: PathBuf,
}

impl ResolvedConfig {
    /// Settings `engine`, reading `input` and publishing to `output_dir`.
    pub fn new(engine: EngineConfig, input: Option<InputSpec>, output_dir: PathBuf) -> Self {
        ResolvedConfig {
            engine,
            input,
            output_dir,
        }
    }
}

fn config_error(message: impl Into<String>) -> EngineError {
    EngineError::Config(message.into())
}

/// A setting value the protocol knows but this engine version does not.
fn unsupported(field: &str, value: impl std::fmt::Debug) -> EngineError {
    config_error(format!(
        "{field}: {value:?} is not supported by this engine version"
    ))
}

/// Converts a configuration document into engine settings. Relative paths are
/// resolved against `base_dir` (normally the directory of the document).
pub fn resolve_config(doc: ConfigDocument, base_dir: &Path) -> Result<ResolvedConfig, EngineError> {
    let site = doc
        .site
        .as_deref()
        .map(|s| SitemapUrl::new(s).map_err(|e| config_error(format!("site: {e}"))))
        .transpose()?;
    let public_base_url = doc
        .public_base_url
        .as_deref()
        .map(|s| {
            SitemapUrl::new(s)
                .map_err(|e| e.to_string())
                .and_then(PublicBaseUrl::new)
                .map_err(|e| config_error(format!("publicBaseUrl: {e}")))
        })
        .transpose()?;
    let reference_time = doc
        .reference_time
        .as_deref()
        .map(|s| W3cDateTime::parse(s).map_err(|e| config_error(format!("referenceTime: {e}"))))
        .transpose()?;

    let strategy = match doc.partitioning.strategy {
        StrategySetting::Sequential => {
            if doc.partitioning.partitions.is_some() {
                return Err(config_error(
                    "partitioning.partitions is only used by the stable-hash and consistent-hash strategies",
                ));
            }
            PartitionStrategy::Sequential
        }
        StrategySetting::StableHash | StrategySetting::ConsistentHash => {
            let name = if doc.partitioning.strategy == StrategySetting::StableHash {
                "stable-hash"
            } else {
                "consistent-hash"
            };
            let partitions = doc
                .partitioning
                .partitions
                .and_then(NonZeroU32::new)
                .ok_or_else(|| {
                    config_error(format!(
                        "partitioning.partitions (>= 1) is required by the {name} strategy"
                    ))
                })?;
            if doc.partitioning.strategy == StrategySetting::StableHash {
                PartitionStrategy::StableHash { partitions }
            } else {
                PartitionStrategy::ConsistentHash { partitions }
            }
        }
        other => return Err(unsupported("partitioning.strategy", other)),
    };
    let limits = PartitionLimits::new(
        doc.partitioning.max_urls.unwrap_or(MAX_URLS_PER_SITEMAP),
        doc.partitioning.max_bytes.unwrap_or(MAX_SITEMAP_BYTES),
    )
    .map_err(|e| config_error(format!("partitioning: {e}")))?;

    let input = match (doc.urls, doc.source) {
        (Some(values), None) => Some(InputSpec::Inline(values)),
        (None, Some(source)) => {
            let format = source
                .format
                .or_else(|| SourceFormat::from_path(&source.path))
                .ok_or_else(|| {
                    config_error(format!(
                        "source.format is required: cannot infer it from {:?}",
                        source.path
                    ))
                })?;
            Some(InputSpec::File {
                path: base_dir.join(&source.path),
                format,
                max_line_bytes: source
                    .max_line_bytes
                    .map_or(DEFAULT_MAX_LINE_BYTES, |n| n as usize),
            })
        }
        (None, None) => None,
        (Some(_), Some(_)) => {
            return Err(config_error("`urls` and `source` are mutually exclusive"));
        }
    };

    let mut validation = ValidationOptions::default();
    validation.mode = match doc.validation.mode {
        ValidationModeSetting::Standard => ValidationMode::Standard,
        ValidationModeSetting::Strict => ValidationMode::Strict,
        other => return Err(unsupported("validation.mode", other)),
    };
    validation.site = site;
    validation.reference_time = reference_time;
    let mut layout = Layout::new(doc.output.file_stem);
    layout.index = match doc.output.index {
        IndexSetting::Auto => IndexMode::Auto,
        IndexSetting::Always => IndexMode::Always,
        IndexSetting::Never => IndexMode::Never,
        other => return Err(unsupported("output.index", other)),
    };
    layout.public_base_url = public_base_url;
    let mut namespaces = Namespaces::DEFAULT;
    for extension in &doc.output.extensions {
        let mut extra = Namespaces::NONE;
        match extension {
            ExtensionSetting::Video => extra.video = true,
            ExtensionSetting::News => extra.news = true,
            other => return Err(unsupported("output.extensions", other)),
        }
        namespaces = namespaces.union(extra);
    }

    let engine = EngineConfig {
        validation,
        on_invalid: match doc.validation.on_invalid {
            InvalidPolicySetting::Fail => InvalidPolicy::Fail,
            InvalidPolicySetting::Skip => InvalidPolicy::Skip,
            other => return Err(unsupported("validation.onInvalid", other)),
        },
        dedupe: doc.validation.dedupe,
        hreflang_reciprocity: doc.validation.hreflang_reciprocity,
        publish: match doc.output.publish {
            PublishSetting::All => PublishPolicy::All,
            PublishSetting::Changed => PublishPolicy::Changed,
            PublishSetting::Auto => PublishPolicy::Auto,
            other => return Err(unsupported("output.publish", other)),
        },
        max_diagnostics: doc.validation.max_diagnostics as usize,
        strategy,
        limits,
        layout,
        manifest: doc.output.manifest,
        namespaces,
        robots: doc.robots,
        snapshot: doc
            .state
            .map(|state| {
                let mut settings = SnapshotSettings::new(base_dir.join(state.directory));
                if let Some(max) = state.max_feed_operations {
                    if max == 0 {
                        return Err(config_error("state.maxFeedOperations must be at least 1"));
                    }
                    settings.max_feed_operations = max;
                }
                if let Some(max) = state.max_listed {
                    settings.max_listed = max as usize;
                }
                Ok(settings)
            })
            .transpose()?,
        expected_state: None,
    };

    Ok(ResolvedConfig {
        engine,
        input,
        output_dir: base_dir.join(doc.output.directory.as_deref().unwrap_or(".")),
    })
}

/// Opens an input as a stream of source records.
pub fn open_input(
    input: &InputSpec,
) -> Result<Box<dyn Iterator<Item = io::Result<SourceRecord>>>, EngineError> {
    match input {
        InputSpec::Inline(values) => Ok(Box::new(JsonArrayReader::from_values(values.clone()))),
        InputSpec::File {
            path,
            format,
            max_line_bytes,
        } => {
            let file = File::open(path).map_err(|e| {
                EngineError::Io(io::Error::new(e.kind(), format!("{}: {e}", path.display())))
            })?;
            match format {
                SourceFormat::Jsonl => Ok(Box::new(JsonlReader::with_max_line_bytes(
                    BufReader::with_capacity(256 * 1024, file),
                    *max_line_bytes,
                ))),
                SourceFormat::Json => Ok(Box::new(JsonArrayReader::from_reader(
                    BufReader::new(file),
                    DEFAULT_MAX_DOCUMENT_BYTES,
                )?)),
                other => Err(EngineError::Config(format!(
                    "source format {other:?} is not supported by this engine version"
                ))),
            }
        }
    }
}

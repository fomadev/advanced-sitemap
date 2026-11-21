use serde::{Deserialize, Serialize};

use crate::{DEFAULT_MAX_DOCUMENT_BYTES, ProtocolError, check_schema_version};

/// Configuration document, protocol version 1.
///
/// Every section is optional and has documented defaults. Unknown fields are
/// rejected. Exactly one of `urls` (inline records) and `source` (a file) may
/// be given; a CLI may also provide the input separately.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigDocument {
    /// `"1.0"`.
    pub schema_version: String,
    /// Expected site (scheme, host, port). Enables `HOST_MISMATCH`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    /// Public URL of the directory where sitemap files are served. Required
    /// when an index is produced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_base_url: Option<String>,
    /// W3C Datetime used by time-dependent rules. Never read from the clock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_time: Option<String>,
    /// Validation settings.
    #[serde(default)]
    pub validation: ValidationSection,
    /// Partitioning settings.
    #[serde(default)]
    pub partitioning: PartitioningSection,
    /// Output settings.
    #[serde(default)]
    pub output: OutputSection,
    /// Input file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSection>,
    /// Inline records. Kept as raw JSON values so that each record is decoded
    /// individually and a bad record becomes a diagnostic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urls: Option<Vec<serde_json::Value>>,
}

impl ConfigDocument {
    /// A version 1.0 document with default settings and no input.
    pub fn new() -> Self {
        Self {
            schema_version: "1.0".to_owned(),
            site: None,
            public_base_url: None,
            reference_time: None,
            validation: ValidationSection::default(),
            partitioning: PartitioningSection::default(),
            output: OutputSection::default(),
            source: None,
            urls: None,
        }
    }

    /// Parses a document and checks its version and structural constraints.
    pub fn from_json(text: &str) -> Result<Self, ProtocolError> {
        if text.len() as u64 > DEFAULT_MAX_DOCUMENT_BYTES {
            return Err(ProtocolError::TooLarge {
                limit: DEFAULT_MAX_DOCUMENT_BYTES,
            });
        }
        // Check the version first so that a newer document gets a version
        // error rather than an "unknown field" error.
        let probe: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| ProtocolError::InvalidDocument(e.to_string()))?;
        match probe
            .get("schemaVersion")
            .and_then(serde_json::Value::as_str)
        {
            Some(version) => check_schema_version(version)?,
            None => {
                return Err(ProtocolError::UnsupportedVersion(
                    "schemaVersion is required (for example \"1.0\")".to_owned(),
                ));
            }
        }
        let document: ConfigDocument = serde_json::from_value(probe)
            .map_err(|e| ProtocolError::InvalidDocument(e.to_string()))?;
        if document.urls.is_some() && document.source.is_some() {
            return Err(ProtocolError::InvalidDocument(
                "`urls` and `source` are mutually exclusive".to_owned(),
            ));
        }
        Ok(document)
    }
}

impl Default for ConfigDocument {
    fn default() -> Self {
        Self::new()
    }
}

/// Validation profile.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationModeSetting {
    /// Protocol conformance.
    #[default]
    Standard,
    /// Production profile.
    Strict,
}

/// What to do with invalid records.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InvalidPolicySetting {
    /// Publish nothing if any record is invalid (default).
    #[default]
    Fail,
    /// Exclude invalid records and publish the rest.
    Skip,
}

/// `validation` section.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationSection {
    /// Profile. Default `standard`.
    #[serde(default)]
    pub mode: ValidationModeSetting,
    /// Invalid record policy. Default `fail`.
    #[serde(default)]
    pub on_invalid: InvalidPolicySetting,
    /// Drop repeated URLs (keeping the first). Default `true`.
    #[serde(default = "default_true")]
    pub dedupe: bool,
    /// Maximum diagnostics kept in the report. Default 1000.
    #[serde(default = "default_max_diagnostics")]
    pub max_diagnostics: u32,
}

impl Default for ValidationSection {
    fn default() -> Self {
        Self {
            mode: ValidationModeSetting::default(),
            on_invalid: InvalidPolicySetting::default(),
            dedupe: true,
            max_diagnostics: default_max_diagnostics(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_max_diagnostics() -> u32 {
    1_000
}

/// Partitioning strategy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StrategySetting {
    /// Fill files in input order.
    #[default]
    Sequential,
    /// Assign by `fnv1a64(loc) mod partitions`.
    StableHash,
}

/// `partitioning` section.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PartitioningSection {
    /// Strategy. Default `sequential`.
    #[serde(default)]
    pub strategy: StrategySetting,
    /// Number of partitions, required by `stable-hash`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partitions: Option<u32>,
    /// URLs per file. Default and maximum 50000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_urls: Option<u32>,
    /// Bytes per file. Default and maximum 52428800.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
}

/// Index policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IndexSetting {
    /// Index only when there is more than one file.
    #[default]
    Auto,
    /// Always write an index.
    Always,
    /// Never write an index.
    Never,
}

/// `output` section.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputSection {
    /// Output directory, relative to the configuration file. Default `.`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// File stem. Default `sitemap`.
    #[serde(default = "default_stem")]
    pub file_stem: String,
    /// Index policy. Default `auto`.
    #[serde(default)]
    pub index: IndexSetting,
    /// Write `<stem>.manifest.json`. Default `true`.
    #[serde(default = "default_true")]
    pub manifest: bool,
    /// Extension namespaces to declare in addition to `xhtml` and `image`,
    /// which are always declared. Since protocol 1.1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<ExtensionSetting>,
}

/// An optional sitemap extension.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionSetting {
    /// Google video extension (`xmlns:video`).
    Video,
}

impl Default for OutputSection {
    fn default() -> Self {
        Self {
            directory: None,
            file_stem: default_stem(),
            index: IndexSetting::default(),
            manifest: true,
            extensions: Vec::new(),
        }
    }
}

fn default_stem() -> String {
    "sitemap".to_owned()
}

/// Input format.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SourceFormat {
    /// A JSON array of records, loaded in memory.
    Json,
    /// One JSON record per line, streamed.
    Jsonl,
}

impl SourceFormat {
    /// Infers the format from a file extension (`.json`, `.jsonl`, `.ndjson`).
    pub fn from_path(path: &str) -> Option<Self> {
        let lower = path.to_ascii_lowercase();
        if lower.ends_with(".jsonl") || lower.ends_with(".ndjson") {
            Some(SourceFormat::Jsonl)
        } else if lower.ends_with(".json") {
            Some(SourceFormat::Json)
        } else {
            None
        }
    }

    /// Lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceFormat::Json => "json",
            SourceFormat::Jsonl => "jsonl",
        }
    }
}

/// `source` section.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSection {
    /// Format. Inferred from the extension when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<SourceFormat>,
    /// Path, relative to the configuration file.
    pub path: String,
    /// Maximum length of a JSONL line, in bytes. Default 1 MiB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_line_bytes: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_document_uses_defaults() {
        let doc = ConfigDocument::from_json(r#"{"schemaVersion":"1.0"}"#).unwrap();
        assert_eq!(doc, ConfigDocument::new());
        assert!(doc.validation.dedupe);
        assert_eq!(doc.output.file_stem, "sitemap");
    }

    #[test]
    fn full_document() {
        let doc = ConfigDocument::from_json(
            r#"{
              "schemaVersion": "1.0",
              "site": "https://example.com",
              "publicBaseUrl": "https://example.com/sitemaps/",
              "referenceTime": "2026-11-21T00:00:00Z",
              "validation": {"mode": "strict", "onInvalid": "skip", "dedupe": false, "maxDiagnostics": 10},
              "partitioning": {"strategy": "stable-hash", "partitions": 8, "maxUrls": 40000, "maxBytes": 40000000},
              "output": {"directory": "dist", "fileStem": "pages", "index": "always", "manifest": false},
              "source": {"format": "jsonl", "path": "urls.jsonl", "maxLineBytes": 65536}
            }"#,
        )
        .unwrap();
        assert_eq!(doc.validation.mode, ValidationModeSetting::Strict);
        assert_eq!(doc.partitioning.strategy, StrategySetting::StableHash);
        assert_eq!(doc.output.index, IndexSetting::Always);
        assert_eq!(doc.source.unwrap().format, Some(SourceFormat::Jsonl));
    }

    #[test]
    fn rejects_unknown_fields_and_bad_versions() {
        let err = ConfigDocument::from_json(r#"{"schemaVersion":"1.0","sitee":"x"}"#).unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDocument(_)));
        let err =
            ConfigDocument::from_json(r#"{"schemaVersion":"1.0","validation":{"mode":"lenient"}}"#)
                .unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDocument(_)));
        let err = ConfigDocument::from_json(r#"{"schemaVersion":"1.4","newField":1}"#).unwrap_err();
        assert!(matches!(err, ProtocolError::UnsupportedVersion(_)));
        let err = ConfigDocument::from_json(r#"{}"#).unwrap_err();
        assert!(matches!(err, ProtocolError::UnsupportedVersion(_)));
        let err = ConfigDocument::from_json(
            r#"{"schemaVersion":"1.0","urls":[],"source":{"path":"a.jsonl"}}"#,
        )
        .unwrap_err();
        assert!(matches!(err, ProtocolError::InvalidDocument(_)));
    }

    #[test]
    fn format_inference() {
        assert_eq!(
            SourceFormat::from_path("a.JSONL"),
            Some(SourceFormat::Jsonl)
        );
        assert_eq!(
            SourceFormat::from_path("a.ndjson"),
            Some(SourceFormat::Jsonl)
        );
        assert_eq!(SourceFormat::from_path("a.json"), Some(SourceFormat::Json));
        assert_eq!(SourceFormat::from_path("a.csv"), None);
    }
}

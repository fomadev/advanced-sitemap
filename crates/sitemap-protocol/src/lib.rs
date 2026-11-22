//! Language-independent contracts of Advanced Sitemap (ADR-0001).
//!
//! * [`ConfigDocument`]: the versioned configuration document
//!   (`schemas/config.v1.schema.json`).
//! * URL records: [`sitemap_model::RawUrlRecord`] with its serde representation
//!   (`schemas/url-record.v1.schema.json`).
//! * Change feeds for the `delta` mode: [`ChangeRecord`].
//! * Readers for JSON arrays and JSON Lines that decode records one by one, so
//!   that a malformed record becomes a diagnostic instead of aborting the run.
//!
//! Version policy: a document declares `schemaVersion: "MAJOR.MINOR"`. This
//! engine accepts major 1 with a minor up to [`SUPPORTED_MINOR`]; a newer minor
//! is refused explicitly rather than silently ignoring fields it may contain.

#![forbid(unsafe_code)]

mod change;
mod config;
mod reader;

pub use change::ChangeRecord;

pub use config::{
    ConfigDocument, ExtensionSetting, IndexSetting, InvalidPolicySetting, OutputSection,
    PartitioningSection, PublishSetting, SourceFormat, SourceSection, StateSection,
    StrategySetting, ValidationModeSetting, ValidationSection,
};
pub use reader::{
    DEFAULT_MAX_DOCUMENT_BYTES, DEFAULT_MAX_LINE_BYTES, DecodeError, JsonArrayReader, JsonlReader,
    SourceRecord,
};

use std::fmt;

/// Supported major version of the documents.
pub const SUPPORTED_MAJOR: u32 = 1;
/// Highest supported minor version for [`SUPPORTED_MAJOR`].
pub const SUPPORTED_MINOR: u32 = 1;
/// Version written in reports produced by this engine.
pub const REPORT_SCHEMA_VERSION: &str = "1.0";

/// A document cannot be used.
#[derive(Debug)]
pub enum ProtocolError {
    /// `schemaVersion` is missing, malformed or unsupported.
    UnsupportedVersion(String),
    /// The document is not valid JSON or does not match the schema.
    InvalidDocument(String),
    /// The document exceeds the size limit.
    TooLarge {
        /// Limit in bytes.
        limit: u64,
    },
    /// Reading failed.
    Io(std::io::Error),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::UnsupportedVersion(message) => f.write_str(message),
            ProtocolError::InvalidDocument(message) => write!(f, "invalid document: {message}"),
            ProtocolError::TooLarge { limit } => {
                write!(f, "document exceeds the {limit}-byte limit")
            }
            ProtocolError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<std::io::Error> for ProtocolError {
    fn from(error: std::io::Error) -> Self {
        ProtocolError::Io(error)
    }
}

/// Checks a `schemaVersion` value against the supported range.
// `minor <= SUPPORTED_MINOR` is trivially `minor == 0` today but must keep
// working when SUPPORTED_MINOR grows.
#[allow(clippy::absurd_extreme_comparisons)]
pub fn check_schema_version(version: &str) -> Result<(), ProtocolError> {
    let parsed = version
        .split_once('.')
        .and_then(|(major, minor)| Some((major.parse::<u32>().ok()?, minor.parse::<u32>().ok()?)));
    match parsed {
        Some((SUPPORTED_MAJOR, minor)) if minor <= SUPPORTED_MINOR => Ok(()),
        Some((major, minor)) => Err(ProtocolError::UnsupportedVersion(format!(
            "schemaVersion {major}.{minor} is not supported; this engine supports {SUPPORTED_MAJOR}.0 to {SUPPORTED_MAJOR}.{SUPPORTED_MINOR}"
        ))),
        None => Err(ProtocolError::UnsupportedVersion(format!(
            "schemaVersion must look like \"1.0\", got {version:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_versions() {
        assert!(check_schema_version("1.0").is_ok());
        assert!(check_schema_version("1.1").is_ok());
        assert!(check_schema_version("1.2").is_err());
        assert!(check_schema_version("2.0").is_err());
        assert!(check_schema_version("1").is_err());
        assert!(check_schema_version("v1.0").is_err());
    }
}

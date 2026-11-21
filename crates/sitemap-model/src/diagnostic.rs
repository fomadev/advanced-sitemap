use std::fmt;

/// How serious a [`Diagnostic`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Severity {
    /// Blocking: the record or the generation cannot be accepted.
    Error,
    /// Probably wrong, the output is still valid.
    Warning,
    /// A best practice is not followed.
    Recommendation,
    /// Informational.
    Info,
}

impl Severity {
    /// Lowercase name.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Recommendation => "recommendation",
            Severity::Info => "info",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

macro_rules! diagnostic_codes {
    ($( $(#[$doc:meta])* $variant:ident => $code:literal, $severity:ident; )*) => {
        /// Stable diagnostic codes.
        ///
        /// Codes are a public contract (ADR-0003): adding one is a minor change,
        /// renaming or removing one is a breaking change. Each code is documented
        /// in `docs/concepts/diagnostics.md`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum DiagnosticCode {
            $( $(#[$doc])* $variant, )*
        }

        impl DiagnosticCode {
            /// Every code, in declaration order.
            pub const ALL: &'static [DiagnosticCode] = &[ $( DiagnosticCode::$variant, )* ];

            /// The stable textual code.
            pub fn as_str(self) -> &'static str {
                match self { $( DiagnosticCode::$variant => $code, )* }
            }

            /// Severity in the `standard` validation profile.
            pub fn default_severity(self) -> Severity {
                match self { $( DiagnosticCode::$variant => Severity::$severity, )* }
            }
        }
    };
}

diagnostic_codes! {
    /// A record could not be decoded (malformed JSON, wrong field type, unknown field).
    InputParseError => "INPUT_PARSE_ERROR", Error;
    /// A JSONL line exceeds the configured maximum length.
    InputLineTooLong => "INPUT_LINE_TOO_LONG", Error;
    /// A required field (`loc`, `hreflang`, `href`, image `loc`) is absent.
    RequiredFieldMissing => "REQUIRED_FIELD_MISSING", Error;
    /// A URL is malformed (whitespace, control character, parse failure).
    UrlInvalid => "URL_INVALID", Error;
    /// A URL does not use `http` or `https`, or is relative.
    UrlUnsupportedScheme => "URL_UNSUPPORTED_SCHEME", Error;
    /// A URL is 2,048 characters or longer.
    UrlTooLong => "URL_TOO_LONG", Error;
    /// A URL embeds a user name or password.
    UrlHasCredentials => "URL_HAS_CREDENTIALS", Error;
    /// A URL has a fragment, which search engines ignore.
    UrlHasFragment => "URL_HAS_FRAGMENT", Warning;
    /// A URL uses `http` instead of `https` (error in the strict profile).
    UrlNotHttps => "URL_NOT_HTTPS", Recommendation;
    /// A URL's origin differs from the configured site (error in the strict profile).
    HostMismatch => "HOST_MISMATCH", Warning;
    /// The same URL appears more than once; later occurrences are dropped.
    DuplicateUrl => "DUPLICATE_URL", Warning;
    /// `lastmod` is not a W3C Datetime.
    LastmodInvalid => "LASTMOD_INVALID", Error;
    /// `lastmod` is absent (strict profile only).
    LastmodMissing => "LASTMOD_MISSING", Error;
    /// `lastmod` is later than the configured reference time.
    LastmodInFuture => "LASTMOD_IN_FUTURE", Warning;
    /// `changefreq` is not a protocol keyword.
    ChangefreqInvalid => "CHANGEFREQ_INVALID", Error;
    /// `changefreq` is absent (strict profile only).
    ChangefreqMissing => "CHANGEFREQ_MISSING", Error;
    /// `priority` is not between 0.0 and 1.0.
    PriorityOutOfRange => "PRIORITY_OUT_OF_RANGE", Error;
    /// `priority` is absent (strict profile only).
    PriorityMissing => "PRIORITY_MISSING", Error;
    /// `hreflang` is not `x-default` or a valid language code.
    HreflangInvalid => "HREFLANG_INVALID", Error;
    /// The same `hreflang` appears twice in one entry.
    HreflangDuplicate => "HREFLANG_DUPLICATE", Warning;
    /// An entry has alternates but does not list itself among them.
    AlternateSelfMissing => "ALTERNATE_SELF_MISSING", Recommendation;
    /// More than 1,000 images in one entry.
    ImageLimitExceeded => "IMAGE_LIMIT_EXCEEDED", Error;
    /// One entry alone does not fit in a sitemap file under the configured byte limit.
    EntryTooLarge => "ENTRY_TOO_LARGE", Error;
    /// A stable-hash partition reached its URL or byte limit.
    PartitionLimitExceeded => "PARTITION_LIMIT_EXCEEDED", Error;
    /// More than 50,000 sitemap files would be referenced by the index.
    IndexLimitExceeded => "INDEX_LIMIT_EXCEEDED", Error;
    /// An index is needed but no public base URL is configured.
    PublicBaseUrlMissing => "PUBLIC_BASE_URL_MISSING", Error;
    /// No URL was written.
    EmptySitemap => "EMPTY_SITEMAP", Warning;
    /// A previously published artifact is no longer produced (never deleted automatically).
    StaleArtifact => "STALE_ARTIFACT", Info;
    /// The diagnostic list reached its limit; counts remain exact.
    DiagnosticsTruncated => "DIAGNOSTICS_TRUNCATED", Info;
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for DiagnosticCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Where a diagnostic applies. Every field is optional.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Location {
    /// 1-based position of the record in the source.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub record: Option<u64>,
    /// 1-based line in the source file, when known.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub line: Option<u64>,
    /// Field path, e.g. `loc` or `alternates[1].hreflang`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub field: Option<String>,
    /// Artifact (output file) concerned.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub artifact: Option<String>,
}

impl Location {
    /// Location of a record.
    pub fn record(record: u64, line: Option<u64>) -> Self {
        Self {
            record: Some(record),
            line,
            ..Self::default()
        }
    }

    /// The same location narrowed to a field.
    pub fn with_field(&self, field: impl Into<String>) -> Self {
        Self {
            field: Some(field.into()),
            ..self.clone()
        }
    }

    /// Location of an artifact.
    pub fn artifact(name: impl Into<String>) -> Self {
        Self {
            artifact: Some(name.into()),
            ..Self::default()
        }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if let Some(line) = self.line {
            parts.push(format!("line {line}"));
        } else if let Some(record) = self.record {
            parts.push(format!("record {record}"));
        }
        if let Some(field) = &self.field {
            parts.push(field.clone());
        }
        if let Some(artifact) = &self.artifact {
            parts.push(artifact.clone());
        }
        f.write_str(&parts.join(", "))
    }
}

/// A structured finding.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Diagnostic {
    /// Stable code.
    pub code: DiagnosticCode,
    /// Effective severity (after the validation profile is applied).
    pub severity: Severity,
    /// Human-readable explanation.
    pub message: String,
    /// Where it applies.
    pub location: Location,
    /// The offending value, truncated to 256 characters.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub value: Option<String>,
    /// How to fix it.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub suggestion: Option<String>,
}

const MAX_VALUE_CHARS: usize = 256;

impl Diagnostic {
    /// A diagnostic with the code's default severity.
    pub fn new(code: DiagnosticCode, message: impl Into<String>, location: Location) -> Self {
        Self {
            code,
            severity: code.default_severity(),
            message: message.into(),
            location,
            value: None,
            suggestion: None,
        }
    }

    /// Attaches the offending value (truncated to 256 characters).
    pub fn with_value(mut self, value: &str) -> Self {
        let truncated: String = value.chars().take(MAX_VALUE_CHARS).collect();
        self.value = Some(if truncated.len() < value.len() {
            format!("{truncated}…")
        } else {
            truncated
        });
        self
    }

    /// Attaches a suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Overrides the severity.
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// `true` when the severity is [`Severity::Error`].
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}] {}", self.severity, self.code, self.message)?;
        let location = self.location.to_string();
        if !location.is_empty() {
            write!(f, " ({location})")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_screaming_snake_case() {
        let mut seen = std::collections::HashSet::new();
        for code in DiagnosticCode::ALL {
            let text = code.as_str();
            assert!(seen.insert(text), "duplicate code {text}");
            assert!(
                text.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'),
                "{text}"
            );
        }
    }

    #[test]
    fn value_is_truncated() {
        let long = "x".repeat(300);
        let diagnostic = Diagnostic::new(DiagnosticCode::UrlInvalid, "bad", Location::default())
            .with_value(&long);
        let value = diagnostic.value.unwrap();
        assert_eq!(value.chars().count(), MAX_VALUE_CHARS + 1);
        assert!(value.ends_with('…'));
    }

    #[test]
    fn display_includes_location() {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::LastmodInvalid,
            "lastmod is not a W3C Datetime",
            Location::record(3, Some(3)).with_field("lastmod"),
        );
        assert_eq!(
            diagnostic.to_string(),
            "error [LASTMOD_INVALID] lastmod is not a W3C Datetime (line 3, lastmod)"
        );
    }
}

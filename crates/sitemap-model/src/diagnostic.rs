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
    /// A hash partition (`stable-hash` or `consistent-hash`) reached its URL or byte limit.
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
    /// An inspected document is not well-formed XML; reading stops.
    XmlMalformed => "XML_MALFORMED", Error;
    /// An inspected document declares a DOCTYPE, which is refused (entity expansion attacks); reading stops.
    XmlDoctypeForbidden => "XML_DOCTYPE_FORBIDDEN", Error;
    /// An inspected document declares an encoding other than UTF-8; reading stops.
    XmlEncodingUnsupported => "XML_ENCODING_UNSUPPORTED", Error;
    /// The root element is not `urlset` or `sitemapindex` in the sitemaps.org namespace; reading stops.
    XmlRootInvalid => "XML_ROOT_INVALID", Error;
    /// An element is unknown at its position in the sitemaps.org namespace, or repeated; it is ignored.
    XmlUnexpectedElement => "XML_UNEXPECTED_ELEMENT", Warning;
    /// Elements are not in the XSD order (loc, lastmod, changefreq, priority, then extensions).
    XmlElementOrder => "XML_ELEMENT_ORDER", Warning;
    /// Elements are nested deeper than the configured limit; reading stops.
    XmlDepthExceeded => "XML_DEPTH_EXCEEDED", Error;
    /// The text of one element is longer than the configured limit; reading stops.
    XmlTextTooLong => "XML_TEXT_TOO_LONG", Error;
    /// The uncompressed document is larger than the configured limit (50 MiB by default); reading stops.
    SitemapTooLarge => "SITEMAP_TOO_LARGE", Error;
    /// A sitemap file contains more than 50,000 `<url>` elements.
    SitemapUrlLimitExceeded => "SITEMAP_URL_LIMIT_EXCEEDED", Error;
    /// A gzip-compressed document cannot be decompressed; reading stops.
    GzipInvalid => "GZIP_INVALID", Error;
    /// `priority` text in an inspected document is not a number.
    PriorityInvalid => "PRIORITY_INVALID", Error;
    /// An entry uses an extension whose namespace is not declared in `output.extensions`.
    ExtensionNotDeclared => "EXTENSION_NOT_DECLARED", Error;
    /// A video has neither `contentLoc` nor `playerLoc`.
    VideoLocationMissing => "VIDEO_LOCATION_MISSING", Error;
    /// A video `contentLoc` or `playerLoc` equals the page `loc`.
    VideoLocationSameAsPage => "VIDEO_LOCATION_SAME_AS_PAGE", Error;
    /// A video title or description is empty, or the description is longer than 2,048 characters.
    VideoTextInvalid => "VIDEO_TEXT_INVALID", Error;
    /// A video title is longer than 100 characters (refused by Google's XSD).
    VideoTitleTooLong => "VIDEO_TITLE_TOO_LONG", Warning;
    /// A video duration is not between 1 and 28,800 seconds.
    VideoDurationOutOfRange => "VIDEO_DURATION_OUT_OF_RANGE", Error;
    /// A video rating is not between 0.0 and 5.0.
    VideoRatingOutOfRange => "VIDEO_RATING_OUT_OF_RANGE", Error;
    /// A video expiration or publication date is not a W3C Datetime.
    VideoDateInvalid => "VIDEO_DATE_INVALID", Error;
    /// A video expiration date is not later than the reference time.
    VideoExpired => "VIDEO_EXPIRED", Warning;
    /// A video restriction has an invalid relationship or country code.
    VideoRestrictionInvalid => "VIDEO_RESTRICTION_INVALID", Error;
    /// A video platform has an invalid relationship or platform value.
    VideoPlatformInvalid => "VIDEO_PLATFORM_INVALID", Error;
    /// A video uploader is empty or longer than 255 characters.
    VideoUploaderInvalid => "VIDEO_UPLOADER_INVALID", Error;
    /// A video uploader `info` page is not on the same host as the page `loc`.
    VideoUploaderHostMismatch => "VIDEO_UPLOADER_HOST_MISMATCH", Warning;
    /// More than 32 tags on one video.
    VideoTagLimitExceeded => "VIDEO_TAG_LIMIT_EXCEEDED", Error;
    /// A video value in an inspected document cannot be read (not a number, not yes/no).
    VideoValueInvalid => "VIDEO_VALUE_INVALID", Error;
    /// A news publication name or title is empty.
    NewsTextInvalid => "NEWS_TEXT_INVALID", Error;
    /// A news language is not a two- or three-letter ISO 639 code, `zh-cn` or `zh-tw`.
    NewsLanguageInvalid => "NEWS_LANGUAGE_INVALID", Error;
    /// A news publication date is not a W3C Datetime.
    NewsDateInvalid => "NEWS_DATE_INVALID", Error;
    /// A news article is older than two days at the reference time; Google News ignores it.
    NewsTooOld => "NEWS_TOO_OLD", Warning;
    /// A sitemap file contains more than 1,000 `<news:news>` elements.
    NewsLimitExceeded => "NEWS_LIMIT_EXCEEDED", Error;
    /// A robots.txt group has no user agent.
    RobotsGroupInvalid => "ROBOTS_GROUP_INVALID", Error;
    /// A robots.txt user agent is empty or contains whitespace, a control character, `:` or `#`.
    RobotsUserAgentInvalid => "ROBOTS_USER_AGENT_INVALID", Error;
    /// A robots.txt user agent uses characters outside the RFC 9309 product token (letters, `-`, `_`).
    RobotsUserAgentNonstandard => "ROBOTS_USER_AGENT_NONSTANDARD", Warning;
    /// A robots.txt path does not start with `/` or `*`, or contains whitespace, a control character or `#`.
    RobotsPathInvalid => "ROBOTS_PATH_INVALID", Error;
    /// The same user agent appears in several robots.txt groups (crawlers merge them).
    RobotsDuplicateUserAgent => "ROBOTS_DUPLICATE_USER_AGENT", Warning;
    /// robots.txt is larger than 500 KiB; crawlers may ignore the rest (RFC 9309).
    RobotsTooLarge => "ROBOTS_TOO_LARGE", Warning;
    /// robots.txt does not list the generated sitemap because `publicBaseUrl` is not set.
    RobotsSitemapNotListed => "ROBOTS_SITEMAP_NOT_LISTED", Warning;
    /// An alternate page in the run does not link back to the page that lists it.
    HreflangReturnLinkMissing => "HREFLANG_RETURN_LINK_MISSING", Warning;
    /// Two pages disagree on the language code of one of them.
    HreflangConflict => "HREFLANG_CONFLICT", Warning;
    /// Alternate links point outside the run, so their return links were not checked.
    HreflangTargetNotListed => "HREFLANG_TARGET_NOT_LISTED", Info;
    /// The previous snapshot cannot be read (damaged, truncated or another format); it is ignored and replaced.
    SnapshotInvalid => "SNAPSHOT_INVALID", Warning;
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

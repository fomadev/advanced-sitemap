//! Validation of raw sitemap records.
//!
//! [`Validator::validate`] turns a [`RawUrlRecord`] into a typed [`UrlEntry`],
//! reporting every problem as a [`Diagnostic`] instead of stopping at the first
//! one. A record with at least one error-level diagnostic yields no entry.
//!
//! Validation never alters data: profiles only change which rules run and how
//! severe their findings are.

use std::collections::HashSet;

mod news;
mod video;

use sitemap_model::limits::MAX_IMAGES_PER_URL;
use sitemap_model::{
    Alternate, ChangeFreq, Diagnostic, DiagnosticCode, Image, LanguageTag, Location, Priority,
    RawUrlRecord, Severity, SitemapUrl, UrlEntry, UrlError, W3cDateTime,
};

/// Validation profile.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ValidationMode {
    /// Protocol conformance. Best-practice findings are warnings or recommendations.
    #[default]
    Standard,
    /// Production profile: `https` and same-origin URLs are required, and
    /// `lastmod`, `changefreq` and `priority` must be present.
    Strict,
}

/// Validator configuration.
#[derive(Clone, Debug, Default)]
pub struct ValidationOptions {
    /// Profile.
    pub mode: ValidationMode,
    /// Expected site. When set, page URLs whose origin (scheme, host, port)
    /// differs are reported with `HOST_MISMATCH`.
    pub site: Option<SitemapUrl>,
    /// Reference instant for time-dependent rules (`LASTMOD_IN_FUTURE`).
    /// Without it those rules do not run: the engine never reads the clock.
    pub reference_time: Option<W3cDateTime>,
}

/// Validates records against a profile.
#[derive(Clone, Debug)]
pub struct Validator {
    options: ValidationOptions,
    site_origin: Option<String>,
}

impl Validator {
    /// Builds a validator.
    pub fn new(options: ValidationOptions) -> Self {
        let site_origin = options.site.as_ref().map(SitemapUrl::origin);
        Self {
            options,
            site_origin,
        }
    }

    /// The options in use.
    pub fn options(&self) -> &ValidationOptions {
        &self.options
    }

    /// Effective severity of a code under the configured profile.
    pub fn severity(&self, code: DiagnosticCode) -> Severity {
        match (self.options.mode, code) {
            (
                ValidationMode::Strict,
                DiagnosticCode::UrlNotHttps | DiagnosticCode::HostMismatch,
            ) => Severity::Error,
            _ => code.default_severity(),
        }
    }

    fn emit(&self, out: &mut Vec<Diagnostic>, diagnostic: Diagnostic) {
        let severity = self.severity(diagnostic.code);
        out.push(diagnostic.with_severity(severity));
    }

    /// Validates one record.
    ///
    /// Diagnostics are appended to `out`. Returns `None` when at least one
    /// error-level diagnostic was produced for this record.
    pub fn validate(
        &self,
        raw: &RawUrlRecord,
        location: &Location,
        out: &mut Vec<Diagnostic>,
    ) -> Option<UrlEntry> {
        let first = out.len();
        let strict = self.options.mode == ValidationMode::Strict;

        let loc = match raw.loc.as_deref() {
            None => {
                self.emit(out, missing(location, "loc"));
                None
            }
            Some(value) => self.parse_url(value, location, "loc", out),
        };

        if let Some(loc) = &loc {
            if loc.has_fragment() {
                self.emit(
                    out,
                    Diagnostic::new(
                        DiagnosticCode::UrlHasFragment,
                        "URL has a fragment; search engines ignore everything after '#'",
                        location.with_field("loc"),
                    )
                    .with_value(loc.as_str())
                    .with_suggestion("Remove the fragment or list the URL without it."),
                );
            }
            if !loc.is_https() {
                self.emit(
                    out,
                    Diagnostic::new(
                        DiagnosticCode::UrlNotHttps,
                        "URL uses http instead of https",
                        location.with_field("loc"),
                    )
                    .with_value(loc.as_str())
                    .with_suggestion("Serve the page over https and list the https URL."),
                );
            }
            if let Some(expected) = &self.site_origin {
                let actual = loc.origin();
                if &actual != expected {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::HostMismatch,
                            format!("URL origin {actual} differs from the site origin {expected}"),
                            location.with_field("loc"),
                        )
                        .with_value(loc.as_str())
                        .with_suggestion(
                            "List only URLs of the configured site, or fix the `site` setting.",
                        ),
                    );
                }
            }
        }

        let lastmod = match raw.lastmod.as_deref() {
            Some(value) => match W3cDateTime::parse(value) {
                Ok(date) => {
                    self.check_future(&date, location, out);
                    Some(date)
                }
                Err(error) => {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::LastmodInvalid,
                            format!("lastmod is not a W3C Datetime: {error}"),
                            location.with_field("lastmod"),
                        )
                        .with_value(value)
                        .with_suggestion(
                            "Use YYYY-MM-DD or YYYY-MM-DDThh:mm:ss+hh:mm (time zone required with a time).",
                        ),
                    );
                    None
                }
            },
            None => {
                if strict {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::LastmodMissing,
                            "lastmod is required by the strict profile",
                            location.with_field("lastmod"),
                        )
                        .with_suggestion("Provide the date the page content last changed."),
                    );
                }
                None
            }
        };

        let changefreq = match raw.changefreq.as_deref() {
            Some(value) => match value.parse::<ChangeFreq>() {
                Ok(freq) => Some(freq),
                Err(error) => {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::ChangefreqInvalid,
                            error.to_string(),
                            location.with_field("changefreq"),
                        )
                        .with_value(value)
                        .with_suggestion("Use a lowercase protocol keyword, or omit the field."),
                    );
                    None
                }
            },
            None => {
                if strict {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::ChangefreqMissing,
                            "changefreq is required by the strict profile",
                            location.with_field("changefreq"),
                        ),
                    );
                }
                None
            }
        };

        let priority = match raw.priority {
            Some(value) => match Priority::new(value) {
                Ok(priority) => Some(priority),
                Err(error) => {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::PriorityOutOfRange,
                            error.to_string(),
                            location.with_field("priority"),
                        )
                        .with_value(&value.to_string())
                        .with_suggestion("Use a value between 0.0 and 1.0, or omit the field."),
                    );
                    None
                }
            },
            None => {
                if strict {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::PriorityMissing,
                            "priority is required by the strict profile",
                            location.with_field("priority"),
                        ),
                    );
                }
                None
            }
        };

        let alternates = self.validate_alternates(raw, loc.as_ref(), location, out);
        let images = self.validate_images(raw, location, out);
        let videos = self.validate_videos(raw, loc.as_ref(), location, out);
        let news = self.validate_news(raw, location, out);

        let has_error = out[first..].iter().any(Diagnostic::is_error);
        match loc {
            Some(loc) if !has_error => Some(UrlEntry {
                loc,
                lastmod,
                changefreq,
                priority,
                alternates,
                images,
                videos,
                news,
            }),
            _ => None,
        }
    }

    fn check_future(&self, date: &W3cDateTime, location: &Location, out: &mut Vec<Diagnostic>) {
        if let Some(reference) = &self.options.reference_time {
            if date.cmp_instant(reference).is_gt() {
                self.emit(
                    out,
                    Diagnostic::new(
                        DiagnosticCode::LastmodInFuture,
                        format!("lastmod is later than the reference time {reference}"),
                        location.with_field("lastmod"),
                    )
                    .with_value(date.as_str())
                    .with_suggestion("Check the source clock or time zone of this date."),
                );
            }
        }
    }

    fn validate_alternates(
        &self,
        raw: &RawUrlRecord,
        loc: Option<&SitemapUrl>,
        location: &Location,
        out: &mut Vec<Diagnostic>,
    ) -> Vec<Alternate> {
        let mut alternates = Vec::with_capacity(raw.alternates.len());
        let mut seen_tags = HashSet::new();
        for (i, alt) in raw.alternates.iter().enumerate() {
            let tag_field = format!("alternates[{i}].hreflang");
            let href_field = format!("alternates[{i}].href");

            let tag = match alt.hreflang.as_deref() {
                None => {
                    self.emit(out, missing(location, &tag_field));
                    None
                }
                Some(value) => match LanguageTag::parse(value) {
                    Ok(tag) => Some(tag),
                    Err(error) => {
                        self.emit(
                            out,
                            Diagnostic::new(
                                DiagnosticCode::HreflangInvalid,
                                error.to_string(),
                                location.with_field(&tag_field),
                            )
                            .with_value(value),
                        );
                        None
                    }
                },
            };
            let href = match alt.href.as_deref() {
                None => {
                    self.emit(out, missing(location, &href_field));
                    None
                }
                Some(value) => self.parse_url(value, location, &href_field, out),
            };

            if let Some(tag) = &tag {
                if !seen_tags.insert(tag.clone()) {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::HreflangDuplicate,
                            format!("hreflang {tag} is listed more than once for this URL"),
                            location.with_field(&tag_field),
                        )
                        .with_value(tag.as_str())
                        .with_suggestion("Keep a single alternate per language code."),
                    );
                }
            }
            if let (Some(hreflang), Some(href)) = (tag, href) {
                alternates.push(Alternate { hreflang, href });
            }
        }

        if let Some(loc) = loc {
            if !alternates.is_empty() && !alternates.iter().any(|alt| alt.href == *loc) {
                self.emit(
                    out,
                    Diagnostic::new(
                        DiagnosticCode::AlternateSelfMissing,
                        "the URL has alternates but does not list itself among them",
                        location.with_field("alternates"),
                    )
                    .with_suggestion(
                        "Add an alternate whose href is the page URL with its own language code.",
                    ),
                );
            }
        }
        alternates
    }

    fn validate_images(
        &self,
        raw: &RawUrlRecord,
        location: &Location,
        out: &mut Vec<Diagnostic>,
    ) -> Vec<Image> {
        if raw.images.len() > MAX_IMAGES_PER_URL {
            self.emit(
                out,
                Diagnostic::new(
                    DiagnosticCode::ImageLimitExceeded,
                    format!(
                        "{} images listed; at most {MAX_IMAGES_PER_URL} are allowed per URL",
                        raw.images.len()
                    ),
                    location.with_field("images"),
                )
                .with_suggestion("Keep the most relevant images only."),
            );
        }
        let mut images = Vec::with_capacity(raw.images.len().min(MAX_IMAGES_PER_URL));
        for (i, image) in raw.images.iter().enumerate() {
            let field = format!("images[{i}].loc");
            match image.loc.as_deref() {
                None => self.emit(out, missing(location, &field)),
                Some(value) => {
                    if let Some(loc) = self.parse_url(value, location, &field, out) {
                        images.push(Image { loc });
                    }
                }
            }
        }
        images
    }

    fn parse_url(
        &self,
        value: &str,
        location: &Location,
        field: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<SitemapUrl> {
        match SitemapUrl::new(value) {
            Ok(url) => Some(url),
            Err(error) => {
                let (code, suggestion) = match &error {
                    UrlError::UnsupportedScheme => (
                        DiagnosticCode::UrlUnsupportedScheme,
                        "Use an absolute URL starting with https:// (or http://).",
                    ),
                    UrlError::TooLong { .. } => (
                        DiagnosticCode::UrlTooLong,
                        "Shorten the URL to fewer than 2,048 characters.",
                    ),
                    UrlError::HasCredentials => (
                        DiagnosticCode::UrlHasCredentials,
                        "Remove the user name and password from the URL.",
                    ),
                    UrlError::ContainsWhitespace => (
                        DiagnosticCode::UrlInvalid,
                        "Trim surrounding whitespace and percent-encode inner spaces as %20.",
                    ),
                    UrlError::Empty | UrlError::ContainsControlCharacter | UrlError::Parse(_) => (
                        DiagnosticCode::UrlInvalid,
                        "Fix the URL so that it is a valid absolute URL.",
                    ),
                };
                self.emit(
                    out,
                    Diagnostic::new(code, error.to_string(), location.with_field(field))
                        .with_value(value)
                        .with_suggestion(suggestion),
                );
                None
            }
        }
    }
}

fn missing(location: &Location, field: &str) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::RequiredFieldMissing,
        format!("required field `{field}` is missing"),
        location.with_field(field),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sitemap_model::{RawAlternate, RawImage};

    fn run(validator: &Validator, raw: &RawUrlRecord) -> (Option<UrlEntry>, Vec<Diagnostic>) {
        let mut out = Vec::new();
        let entry = validator.validate(raw, &Location::record(1, Some(1)), &mut out);
        (entry, out)
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&'static str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    fn standard() -> Validator {
        Validator::new(ValidationOptions::default())
    }

    #[test]
    fn valid_full_record() {
        let raw = RawUrlRecord {
            loc: Some("https://example.com/fr/".into()),
            lastmod: Some("2026-11-21".into()),
            changefreq: Some("weekly".into()),
            priority: Some(0.8),
            alternates: vec![
                RawAlternate {
                    hreflang: Some("fr".into()),
                    href: Some("https://example.com/fr/".into()),
                },
                RawAlternate {
                    hreflang: Some("en".into()),
                    href: Some("https://example.com/en/".into()),
                },
            ],
            images: vec![RawImage {
                loc: Some("https://cdn.example.com/a.png".into()),
            }],
            ..RawUrlRecord::default()
        };
        let (entry, diagnostics) = run(&standard(), &raw);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let entry = entry.unwrap();
        assert_eq!(entry.alternates.len(), 2);
        assert_eq!(entry.images.len(), 1);
    }

    #[test]
    fn collects_every_error_of_a_record() {
        let raw = RawUrlRecord {
            loc: None,
            lastmod: Some("21/11/2026".into()),
            changefreq: Some("Daily".into()),
            priority: Some(1.5),
            alternates: vec![RawAlternate {
                hreflang: Some("english".into()),
                href: None,
            }],
            images: vec![RawImage {
                loc: Some("/a.png".into()),
            }],
            ..RawUrlRecord::default()
        };
        let (entry, diagnostics) = run(&standard(), &raw);
        assert!(entry.is_none());
        assert_eq!(
            codes(&diagnostics),
            [
                "REQUIRED_FIELD_MISSING",
                "LASTMOD_INVALID",
                "CHANGEFREQ_INVALID",
                "PRIORITY_OUT_OF_RANGE",
                "HREFLANG_INVALID",
                "REQUIRED_FIELD_MISSING",
                "URL_UNSUPPORTED_SCHEME",
            ]
        );
        assert_eq!(
            diagnostics[5].location.field.as_deref(),
            Some("alternates[0].href")
        );
        assert_eq!(diagnostics[1].value.as_deref(), Some("21/11/2026"));
    }

    #[test]
    fn warnings_do_not_reject_the_record() {
        let raw = RawUrlRecord::with_loc("http://example.com/a#top");
        let (entry, diagnostics) = run(&standard(), &raw);
        assert!(entry.is_some());
        assert_eq!(codes(&diagnostics), ["URL_HAS_FRAGMENT", "URL_NOT_HTTPS"]);
        assert_eq!(diagnostics[1].severity, Severity::Recommendation);
    }

    #[test]
    fn strict_profile_promotes_and_requires_fields() {
        let validator = Validator::new(ValidationOptions {
            mode: ValidationMode::Strict,
            site: Some(SitemapUrl::new("https://example.com/").unwrap()),
            reference_time: None,
        });
        let (entry, diagnostics) =
            run(&validator, &RawUrlRecord::with_loc("http://other.example/"));
        assert!(entry.is_none());
        assert_eq!(
            codes(&diagnostics),
            [
                "URL_NOT_HTTPS",
                "HOST_MISMATCH",
                "LASTMOD_MISSING",
                "CHANGEFREQ_MISSING",
                "PRIORITY_MISSING"
            ]
        );
        assert!(diagnostics.iter().all(Diagnostic::is_error));
    }

    #[test]
    fn host_mismatch_is_a_warning_in_standard_profile() {
        let validator = Validator::new(ValidationOptions {
            site: Some(SitemapUrl::new("https://example.com").unwrap()),
            ..ValidationOptions::default()
        });
        let (entry, diagnostics) = run(
            &validator,
            &RawUrlRecord::with_loc("https://www.example.com/"),
        );
        assert!(entry.is_some());
        assert_eq!(codes(&diagnostics), ["HOST_MISMATCH"]);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
    }

    #[test]
    fn future_lastmod_needs_reference_time() {
        let raw = RawUrlRecord {
            lastmod: Some("2030-01-01".into()),
            ..RawUrlRecord::with_loc("https://example.com/")
        };
        let (_, diagnostics) = run(&standard(), &raw);
        assert!(diagnostics.is_empty());

        let validator = Validator::new(ValidationOptions {
            reference_time: Some(W3cDateTime::parse("2026-11-21T00:00:00Z").unwrap()),
            ..ValidationOptions::default()
        });
        let (entry, diagnostics) = run(&validator, &raw);
        assert!(entry.is_some());
        assert_eq!(codes(&diagnostics), ["LASTMOD_IN_FUTURE"]);
    }

    #[test]
    fn alternate_rules() {
        let raw = RawUrlRecord {
            alternates: vec![
                RawAlternate {
                    hreflang: Some("en".into()),
                    href: Some("https://example.com/en".into()),
                },
                RawAlternate {
                    hreflang: Some("EN".into()),
                    href: Some("https://example.com/en2".into()),
                },
            ],
            ..RawUrlRecord::with_loc("https://example.com/fr")
        };
        let (entry, diagnostics) = run(&standard(), &raw);
        assert!(entry.is_some());
        assert_eq!(
            codes(&diagnostics),
            ["HREFLANG_DUPLICATE", "ALTERNATE_SELF_MISSING"]
        );
    }

    #[test]
    fn image_limit() {
        let raw = RawUrlRecord {
            images: (0..=MAX_IMAGES_PER_URL)
                .map(|i| RawImage {
                    loc: Some(format!("https://cdn.example.com/{i}.png")),
                })
                .collect(),
            ..RawUrlRecord::with_loc("https://example.com/")
        };
        let (entry, diagnostics) = run(&standard(), &raw);
        assert!(entry.is_none());
        assert_eq!(codes(&diagnostics), ["IMAGE_LIMIT_EXCEEDED"]);
    }

    #[test]
    fn url_error_codes() {
        let check = |url: &str, code: &str| {
            let (_, diagnostics) = run(&standard(), &RawUrlRecord::with_loc(url));
            assert_eq!(codes(&diagnostics), [code], "{url}");
        };
        check("example.com", "URL_UNSUPPORTED_SCHEME");
        check("https://u:p@example.com/", "URL_HAS_CREDENTIALS");
        check(" https://example.com/", "URL_INVALID");
        check(
            &format!("https://example.com/{}", "a".repeat(2100)),
            "URL_TOO_LONG",
        );
    }
}

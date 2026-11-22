//! Validation of the Google News extension.

use sitemap_model::limits::NEWS_MAX_AGE_SECONDS;
use sitemap_model::{
    Diagnostic, DiagnosticCode, Location, News, NewsLanguage, RawUrlRecord, W3cDateTime,
};

use crate::{Validator, missing};

impl Validator {
    pub(crate) fn validate_news(
        &self,
        raw: &RawUrlRecord,
        location: &Location,
        out: &mut Vec<Diagnostic>,
    ) -> Option<News> {
        let news = raw.news.as_ref()?;
        let first = out.len();

        let publication = news.publication.as_ref();
        if publication.is_none() {
            self.emit(out, missing(location, "news.publication"));
        }
        let name = publication.and_then(|p| {
            self.news_text(p.name.as_deref(), location, "news.publication.name", out)
        });
        let language = publication.and_then(|p| match p.language.as_deref() {
            None => {
                self.emit(out, missing(location, "news.publication.language"));
                None
            }
            Some(value) => {
                let parsed = NewsLanguage::parse(value);
                if parsed.is_none() {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::NewsLanguageInvalid,
                            "news language must be a two- or three-letter ISO 639 code, zh-cn or zh-tw",
                            location.with_field("news.publication.language"),
                        )
                        .with_value(value)
                        .with_suggestion("Use the language code only, for example \"en\", not \"en-gb\"."),
                    );
                }
                parsed
            }
        });

        let publication_date = match news.publication_date.as_deref() {
            None => {
                self.emit(out, missing(location, "news.publicationDate"));
                None
            }
            Some(value) => match W3cDateTime::parse(value) {
                Ok(date) => Some(date),
                Err(error) => {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::NewsDateInvalid,
                            format!("news publication date is not a W3C Datetime: {error}"),
                            location.with_field("news.publicationDate"),
                        )
                        .with_value(value),
                    );
                    None
                }
            },
        };
        if let (Some(date), Some(reference)) = (&publication_date, &self.options.reference_time)
            && reference.unix_seconds() - date.unix_seconds() > NEWS_MAX_AGE_SECONDS
        {
            self.emit(
                out,
                Diagnostic::new(
                    DiagnosticCode::NewsTooOld,
                    "article is older than two days; Google News only uses recent articles",
                    location.with_field("news.publicationDate"),
                )
                .with_value(date.as_str())
                .with_suggestion("Remove the news metadata (or the URL) from the news sitemap."),
            );
        }

        let title = self.news_text(news.title.as_deref(), location, "news.title", out);

        if out[first..].iter().any(Diagnostic::is_error) {
            return None;
        }
        Some(News::new(name?, language?, publication_date?, title?))
    }

    fn news_text(
        &self,
        value: Option<&str>,
        location: &Location,
        field: &str,
        out: &mut Vec<Diagnostic>,
    ) -> Option<String> {
        match value.map(str::trim) {
            None => {
                self.emit(out, missing(location, field));
                None
            }
            Some("") => {
                self.emit(
                    out,
                    Diagnostic::new(
                        DiagnosticCode::NewsTextInvalid,
                        format!("`{field}` is empty"),
                        location.with_field(field),
                    ),
                );
                None
            }
            Some(text) => match sitemap_model::xml_forbidden_char(text) {
                Some(ch) => {
                    self.emit(
                        out,
                        Diagnostic::new(
                            DiagnosticCode::NewsTextInvalid,
                            format!(
                                "`{field}` contains U+{:04X}, which XML 1.0 forbids",
                                u32::from(ch)
                            ),
                            location.with_field(field),
                        )
                        .with_suggestion("Remove control characters from the text."),
                    );
                    None
                }
                None => Some(text.to_owned()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use sitemap_model::{RawNews, RawNewsPublication};

    use super::*;
    use crate::ValidationOptions;

    fn record(news: RawNews) -> RawUrlRecord {
        {
            let mut this = RawUrlRecord::default();
            this.loc = Some("https://example.com/article".into());
            this.news = Some(news);
            this
        }
    }

    fn valid() -> RawNews {
        {
            let mut this = RawNews::default();
            this.publication = Some({
                let mut this = RawNewsPublication::default();
                this.name = Some("The Example Times".into());
                this.language = Some("EN".into());
                this
            });
            this.publication_date = Some("2026-11-20T08:00:00Z".into());
            this.title = Some("Companies A, B in merger talks".into());
            this
        }
    }

    fn codes(raw: &RawUrlRecord, options: ValidationOptions) -> (bool, Vec<&'static str>) {
        let mut out = Vec::new();
        let entry = Validator::new(options).validate(raw, &Location::record(1, None), &mut out);
        (
            entry.is_some(),
            out.iter().map(|d| d.code.as_str()).collect(),
        )
    }

    #[test]
    fn valid_news_is_normalized() {
        let mut out = Vec::new();
        let entry = Validator::new(ValidationOptions::default())
            .validate(&record(valid()), &Location::record(1, None), &mut out)
            .unwrap();
        assert!(out.is_empty());
        assert_eq!(entry.news.unwrap().language.as_str(), "en");
    }

    #[test]
    fn invalid_news_reports_each_problem() {
        let raw = record({
            let mut this = RawNews::default();
            this.publication = Some({
                let mut this = RawNewsPublication::default();
                this.name = Some(" ".into());
                this.language = Some("en-gb".into());
                this
            });
            this.publication_date = Some("yesterday".into());
            this.title = None;
            this
        });
        let (ok, found) = codes(&raw, ValidationOptions::default());
        assert!(!ok);
        assert_eq!(
            found,
            [
                "NEWS_TEXT_INVALID",
                "NEWS_LANGUAGE_INVALID",
                "NEWS_DATE_INVALID",
                "REQUIRED_FIELD_MISSING"
            ]
        );
        let (_, found) = codes(&record(RawNews::default()), ValidationOptions::default());
        assert_eq!(found, ["REQUIRED_FIELD_MISSING"; 3]);
    }

    #[test]
    fn old_articles_get_a_warning_with_a_reference_time() {
        let options = |time: &str| ValidationOptions {
            reference_time: Some(W3cDateTime::parse(time).unwrap()),
            ..ValidationOptions::default()
        };
        let raw = record(valid());
        assert_eq!(codes(&raw, options("2026-11-22T08:00:00Z")), (true, vec![]));
        assert_eq!(
            codes(&raw, options("2026-11-22T08:00:01Z")),
            (true, vec!["NEWS_TOO_OLD"])
        );
    }
}

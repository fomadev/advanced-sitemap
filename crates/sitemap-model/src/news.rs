//! Google News sitemap extension (`news:news`, namespace
//! `http://www.google.com/schemas/sitemap-news/0.9`).
//!
//! Only the five tags Google documents are modelled: `publication` (`name`,
//! `language`), `publication_date` and `title`. The schema also defines
//! `access`, `genres`, `keywords` and `stock_tickers`, which Google's
//! documentation no longer lists; they are not emitted.

use std::fmt;

use crate::W3cDateTime;

/// Language of a news publication: an ISO 639 code of two or three letters,
/// or `zh-cn` / `zh-tw`, stored in lowercase (the schema pattern).
///
/// Only the format is checked, not whether the code is assigned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewsLanguage(String);

impl NewsLanguage {
    /// Accepts the schema pattern `zh-cn|zh-tw|[a-z]{2,3}`, in any case.
    pub fn parse(value: &str) -> Option<Self> {
        let lower = value.to_ascii_lowercase();
        let ok = lower == "zh-cn"
            || lower == "zh-tw"
            || ((2..=3).contains(&lower.len()) && lower.bytes().all(|b| b.is_ascii_lowercase()));
        ok.then_some(NewsLanguage(lower))
    }

    /// The lowercase code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NewsLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A validated `news:news` element.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct News {
    /// `news:publication/news:name`: the publication name as shown on Google
    /// News, without anything in parentheses.
    pub publication_name: String,
    /// `news:publication/news:language`.
    pub language: NewsLanguage,
    /// `news:publication_date`: first publication of the article.
    pub publication_date: W3cDateTime,
    /// `news:title`: the article title as shown on the site.
    pub title: String,
}

impl News {
    /// News metadata for an article.
    pub fn new(
        publication_name: impl Into<String>,
        language: NewsLanguage,
        publication_date: W3cDateTime,
        title: impl Into<String>,
    ) -> Self {
        Self {
            publication_name: publication_name.into(),
            language,
            publication_date,
            title: title.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages() {
        assert_eq!(NewsLanguage::parse("EN").unwrap().as_str(), "en");
        assert_eq!(NewsLanguage::parse("zh-TW").unwrap().as_str(), "zh-tw");
        assert!(NewsLanguage::parse("fra").is_some());
        assert!(NewsLanguage::parse("en-gb").is_none());
        assert!(NewsLanguage::parse("e").is_none());
        assert!(NewsLanguage::parse("engl").is_none());
    }
}

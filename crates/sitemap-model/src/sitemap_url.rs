use std::fmt;

use url::Url;

use crate::limits::MAX_URL_LENGTH;

/// An absolute `http` or `https` URL that is safe to place in a sitemap.
///
/// Construction follows ADR-0004:
///
/// * the raw input must start with `http://` or `https://` (case-insensitive)
///   and must not contain whitespace or control characters — the engine never
///   repairs a URL silently;
/// * the value is parsed with the WHATWG URL standard and stored in its
///   serialized form (ASCII, lowercase IDNA host, percent-encoded path, default
///   port removed). This normalization never merges two distinct resources:
///   path case, trailing slash and query string are preserved;
/// * embedded credentials are rejected;
/// * the serialized form must be shorter than 2,048 characters.
///
/// A fragment (`#...`) is accepted by the type; the validator reports it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SitemapUrl {
    url: Url,
}

/// Why a string is not a valid [`SitemapUrl`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrlError {
    /// The input is empty.
    Empty,
    /// The input contains a whitespace character (leading, trailing or inner).
    ContainsWhitespace,
    /// The input contains a control character.
    ContainsControlCharacter,
    /// The input does not start with `http://` or `https://`.
    UnsupportedScheme,
    /// The WHATWG parser rejected the input.
    Parse(String),
    /// The URL carries a user name or password.
    HasCredentials,
    /// The serialized URL is too long.
    TooLong {
        /// Length of the serialized URL, in characters.
        length: usize,
    },
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UrlError::Empty => f.write_str("URL is empty"),
            UrlError::ContainsWhitespace => f.write_str("URL contains whitespace"),
            UrlError::ContainsControlCharacter => f.write_str("URL contains a control character"),
            UrlError::UnsupportedScheme => f.write_str("URL must start with http:// or https://"),
            UrlError::Parse(reason) => write!(f, "URL cannot be parsed: {reason}"),
            UrlError::HasCredentials => f.write_str("URL must not contain a user name or password"),
            UrlError::TooLong { length } => write!(
                f,
                "URL is {length} characters long; the maximum is {MAX_URL_LENGTH}"
            ),
        }
    }
}

impl std::error::Error for UrlError {}

impl SitemapUrl {
    /// Validates `raw` and builds a sitemap URL.
    pub fn new(raw: &str) -> Result<Self, UrlError> {
        if raw.is_empty() {
            return Err(UrlError::Empty);
        }
        if raw.chars().any(char::is_whitespace) {
            return Err(UrlError::ContainsWhitespace);
        }
        if raw.chars().any(char::is_control) {
            return Err(UrlError::ContainsControlCharacter);
        }
        if !has_http_prefix(raw) {
            return Err(UrlError::UnsupportedScheme);
        }

        let url = Url::parse(raw).map_err(|error| UrlError::Parse(error.to_string()))?;
        // The prefix check guarantees the scheme, but keep the type invariant explicit.
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(UrlError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(UrlError::HasCredentials);
        }
        let length = url.as_str().chars().count();
        if length > MAX_URL_LENGTH {
            return Err(UrlError::TooLong { length });
        }
        Ok(Self { url })
    }

    /// The serialized URL, as written to the sitemap.
    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }

    /// `true` when the scheme is `https`.
    pub fn is_https(&self) -> bool {
        self.url.scheme() == "https"
    }

    /// `true` when the URL has a fragment (`#...`).
    pub fn has_fragment(&self) -> bool {
        self.url.fragment().is_some()
    }

    /// The host (ASCII, lowercase), when the URL has one.
    pub fn host(&self) -> Option<&str> {
        self.url.host_str()
    }

    /// The ASCII serialization of the origin (`scheme://host[:port]`).
    pub fn origin(&self) -> String {
        self.url.origin().ascii_serialization()
    }

    /// Resolves `relative` against this URL (used to build index locations).
    pub fn join(&self, relative: &str) -> Result<SitemapUrl, UrlError> {
        let joined = self
            .url
            .join(relative)
            .map_err(|error| UrlError::Parse(error.to_string()))?;
        SitemapUrl::new(joined.as_str())
    }
}

fn has_http_prefix(raw: &str) -> bool {
    let lower_prefix = |prefix: &str| {
        raw.len() >= prefix.len()
            && raw.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
    };
    lower_prefix("http://") || lower_prefix("https://")
}

impl fmt::Display for SitemapUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for SitemapUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SitemapUrl({})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_serializes_whatwg_form() {
        let url = SitemapUrl::new("HTTPS://Example.COM:443").unwrap();
        assert_eq!(url.as_str(), "https://example.com/");
        assert!(url.is_https());
        assert_eq!(url.origin(), "https://example.com");
    }

    #[test]
    fn preserves_path_case_query_and_trailing_slash() {
        let url = SitemapUrl::new("https://example.com/Blog/Post/?b=2&a=1").unwrap();
        assert_eq!(url.as_str(), "https://example.com/Blog/Post/?b=2&a=1");
    }

    #[test]
    fn percent_encodes_non_ascii_and_converts_idna() {
        let url = SitemapUrl::new("https://bücher.example/café").unwrap();
        assert_eq!(url.as_str(), "https://xn--bcher-kva.example/caf%C3%A9");
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert_eq!(SitemapUrl::new(""), Err(UrlError::Empty));
        assert_eq!(
            SitemapUrl::new(" https://example.com/"),
            Err(UrlError::ContainsWhitespace)
        );
        assert_eq!(
            SitemapUrl::new("https://example.com/a b"),
            Err(UrlError::ContainsWhitespace)
        );
        assert_eq!(
            SitemapUrl::new("https://example.com/\u{1}"),
            Err(UrlError::ContainsControlCharacter)
        );
        assert_eq!(
            SitemapUrl::new("ftp://example.com/"),
            Err(UrlError::UnsupportedScheme)
        );
        assert_eq!(
            SitemapUrl::new("https:example.com"),
            Err(UrlError::UnsupportedScheme)
        );
        assert_eq!(
            SitemapUrl::new("/relative"),
            Err(UrlError::UnsupportedScheme)
        );
        assert_eq!(
            SitemapUrl::new("https://user:pw@example.com/"),
            Err(UrlError::HasCredentials)
        );
        assert!(matches!(
            SitemapUrl::new("https://exa mple"),
            Err(UrlError::ContainsWhitespace)
        ));
        assert!(matches!(
            SitemapUrl::new("https://[::1/"),
            Err(UrlError::Parse(_))
        ));
    }

    #[test]
    fn enforces_length_limit_on_serialized_form() {
        let base = "https://example.com/";
        let ok = format!("{base}{}", "a".repeat(MAX_URL_LENGTH - base.len()));
        assert!(SitemapUrl::new(&ok).is_ok());
        let too_long = format!("{ok}a");
        assert_eq!(
            SitemapUrl::new(&too_long),
            Err(UrlError::TooLong {
                length: MAX_URL_LENGTH + 1
            })
        );
    }

    #[test]
    fn reports_fragment_and_joins() {
        let url = SitemapUrl::new("https://example.com/a#section").unwrap();
        assert!(url.has_fragment());
        let base = SitemapUrl::new("https://example.com/sitemaps/").unwrap();
        assert_eq!(
            base.join("sitemap-1.xml").unwrap().as_str(),
            "https://example.com/sitemaps/sitemap-1.xml"
        );
    }
}

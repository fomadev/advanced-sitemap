use std::fmt;

/// An `hreflang` value for an alternate link: `x-default`, or a language code
/// optionally followed by a script and/or a region.
///
/// Syntax accepted (a subset of BCP 47 matching Google's documented usage):
///
/// * language: 2 or 3 letters (ISO 639-1, or ISO 639-2/3);
/// * optional script: 4 letters (ISO 15924), e.g. `zh-Hant`;
/// * optional region: 2 letters (ISO 3166-1 alpha-2) or 3 digits (UN M.49).
///
/// Only the syntax is checked, not the registries. The value is normalized to
/// the conventional case (`en-GB`, `zh-Hant-TW`, `x-default`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LanguageTag(String);

/// Why a string is not a valid [`LanguageTag`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LanguageTagError;

impl fmt::Display for LanguageTagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "hreflang must be x-default or a language code such as en, en-GB, zh-Hant or es-419",
        )
    }
}

impl std::error::Error for LanguageTagError {}

impl LanguageTag {
    /// Validates and normalizes an `hreflang` value.
    pub fn parse(raw: &str) -> Result<Self, LanguageTagError> {
        if raw.eq_ignore_ascii_case("x-default") {
            return Ok(Self("x-default".to_owned()));
        }

        let mut parts = raw.split('-');
        let language = parts.next().ok_or(LanguageTagError)?;
        if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_alphabetic())
        {
            return Err(LanguageTagError);
        }
        let mut normalized = language.to_ascii_lowercase();

        let mut rest: Vec<&str> = parts.collect();
        if let Some(script) = rest.first().copied()
            && script.len() == 4
            && script.bytes().all(|b| b.is_ascii_alphabetic())
        {
            normalized.push('-');
            let mut chars = script.chars();
            if let Some(first) = chars.next() {
                normalized.push(first.to_ascii_uppercase());
            }
            normalized.extend(chars.map(|c| c.to_ascii_lowercase()));
            rest.remove(0);
        }
        if let Some(region) = rest.first().copied() {
            let alpha = region.len() == 2 && region.bytes().all(|b| b.is_ascii_alphabetic());
            let numeric = region.len() == 3 && region.bytes().all(|b| b.is_ascii_digit());
            if !(alpha || numeric) {
                return Err(LanguageTagError);
            }
            normalized.push('-');
            normalized.push_str(&region.to_ascii_uppercase());
            rest.remove(0);
        }
        if !rest.is_empty() {
            return Err(LanguageTagError);
        }
        Ok(Self(normalized))
    }

    /// The normalized value.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` for `x-default`.
    pub fn is_x_default(&self) -> bool {
        self.0 == "x-default"
    }
}

impl fmt::Display for LanguageTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_valid_tags() {
        let n = |raw: &str| LanguageTag::parse(raw).unwrap().as_str().to_owned();
        assert_eq!(n("en"), "en");
        assert_eq!(n("EN-gb"), "en-GB");
        assert_eq!(n("zh-hant"), "zh-Hant");
        assert_eq!(n("zh-HANT-tw"), "zh-Hant-TW");
        assert_eq!(n("es-419"), "es-419");
        assert_eq!(n("fil"), "fil");
        assert_eq!(n("X-Default"), "x-default");
        assert!(LanguageTag::parse("x-default").unwrap().is_x_default());
    }

    #[test]
    fn rejects_invalid_tags() {
        for raw in [
            "", "e", "english", "en_GB", "en-", "en-G", "en-GBR", "en-GB-x", "12", "en-12",
            "fr-FR-FR",
        ] {
            assert!(LanguageTag::parse(raw).is_err(), "{raw} should be rejected");
        }
    }
}

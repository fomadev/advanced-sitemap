use std::fmt;
use std::str::FromStr;

/// The `<changefreq>` values defined by sitemaps.org.
///
/// Supported for compatibility; Google documents that it ignores this field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeFreq {
    /// `always`
    Always,
    /// `hourly`
    Hourly,
    /// `daily`
    Daily,
    /// `weekly`
    Weekly,
    /// `monthly`
    Monthly,
    /// `yearly`
    Yearly,
    /// `never`
    Never,
}

impl ChangeFreq {
    /// All values, in protocol order.
    pub const ALL: [ChangeFreq; 7] = [
        ChangeFreq::Always,
        ChangeFreq::Hourly,
        ChangeFreq::Daily,
        ChangeFreq::Weekly,
        ChangeFreq::Monthly,
        ChangeFreq::Yearly,
        ChangeFreq::Never,
    ];

    /// The protocol keyword.
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeFreq::Always => "always",
            ChangeFreq::Hourly => "hourly",
            ChangeFreq::Daily => "daily",
            ChangeFreq::Weekly => "weekly",
            ChangeFreq::Monthly => "monthly",
            ChangeFreq::Yearly => "yearly",
            ChangeFreq::Never => "never",
        }
    }
}

/// The value is not one of the protocol keywords (matching is case-sensitive,
/// as in the sitemaps.org XML schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeFreqError;

impl fmt::Display for ChangeFreqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "changefreq must be one of always, hourly, daily, weekly, monthly, yearly, never",
        )
    }
}

impl std::error::Error for ChangeFreqError {}

impl FromStr for ChangeFreq {
    type Err = ChangeFreqError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ChangeFreq::ALL
            .into_iter()
            .find(|value| value.as_str() == s)
            .ok_or(ChangeFreqError)
    }
}

impl fmt::Display for ChangeFreq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A `<priority>` value between 0.0 and 1.0 inclusive.
///
/// Serialized with at least one and at most four decimal places, so `1`
/// becomes `1.0` and `0.75` stays `0.75`. Supported for compatibility; Google
/// documents that it ignores this field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Priority(f64);

/// The value is not a finite number between 0.0 and 1.0.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorityError {
    /// The rejected value.
    pub value: f64,
}

impl fmt::Display for PriorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "priority must be between 0.0 and 1.0, got {}",
            self.value
        )
    }
}

impl std::error::Error for PriorityError {}

impl Priority {
    /// Validates a priority value.
    pub fn new(value: f64) -> Result<Self, PriorityError> {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            Ok(Self(value))
        } else {
            Err(PriorityError { value })
        }
    }

    /// The numeric value.
    pub fn value(self) -> f64 {
        self.0
    }

    /// The textual form written to the sitemap.
    pub fn to_xml_string(self) -> String {
        let mut text = format!("{:.4}", self.0);
        while text.ends_with('0') && !text.ends_with(".0") {
            text.pop();
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changefreq_round_trips_and_is_case_sensitive() {
        for value in ChangeFreq::ALL {
            assert_eq!(value.as_str().parse::<ChangeFreq>(), Ok(value));
        }
        assert!("Daily".parse::<ChangeFreq>().is_err());
        assert!("biweekly".parse::<ChangeFreq>().is_err());
    }

    #[test]
    fn priority_bounds() {
        assert!(Priority::new(0.0).is_ok());
        assert!(Priority::new(1.0).is_ok());
        assert!(Priority::new(-0.1).is_err());
        assert!(Priority::new(1.01).is_err());
        assert!(Priority::new(f64::NAN).is_err());
        assert!(Priority::new(f64::INFINITY).is_err());
    }

    #[test]
    fn priority_serialization_is_lossless_to_four_places() {
        let s = |v: f64| Priority::new(v).unwrap().to_xml_string();
        assert_eq!(s(1.0), "1.0");
        assert_eq!(s(0.0), "0.0");
        assert_eq!(s(0.5), "0.5");
        assert_eq!(s(0.75), "0.75");
        assert_eq!(s(0.1234), "0.1234");
        assert_eq!(s(0.12345), "0.1235");
    }
}

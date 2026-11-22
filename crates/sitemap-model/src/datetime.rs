use std::cmp::Ordering;
use std::fmt;

/// A date or date-time in the W3C Datetime profile of ISO 8601, the format
/// required by `<lastmod>` (<https://www.w3.org/TR/NOTE-datetime>).
///
/// Accepted forms:
///
/// | Precision | Example |
/// |---|---|
/// | Year | `2026` |
/// | Month | `2026-11` |
/// | Day | `2026-11-21` |
/// | Minutes | `2026-11-21T09:30+01:00` |
/// | Seconds | `2026-11-21T09:30:15Z` |
/// | Fraction | `2026-11-21T09:30:15.250-05:00` |
///
/// A time component requires a time zone designator (`Z` or `±hh:mm`), as the
/// W3C profile mandates. The original text is preserved and written as-is;
/// comparisons use the UTC instant (a partial date is the start of its period).
#[derive(Clone, Debug)]
pub struct W3cDateTime {
    raw: String,
    precision: DateTimePrecision,
    /// Seconds since 1970-01-01T00:00:00Z.
    unix_seconds: i64,
    /// Fractional part, in nanoseconds (truncated after 9 digits).
    nanos: u32,
}

/// The precision of a [`W3cDateTime`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    clippy::exhaustive_enums,
    reason = "the W3C Datetime profile defines exactly these precisions"
)]
pub enum DateTimePrecision {
    /// `YYYY`
    Year,
    /// `YYYY-MM`
    Month,
    /// `YYYY-MM-DD`
    Day,
    /// `YYYY-MM-DDThh:mmTZD`
    Minute,
    /// `YYYY-MM-DDThh:mm:ssTZD`
    Second,
    /// `YYYY-MM-DDThh:mm:ss.sTZD`
    Fraction,
}

/// Why a string is not a valid [`W3cDateTime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateTimeError {
    reason: &'static str,
}

impl DateTimeError {
    const fn new(reason: &'static str) -> Self {
        Self { reason }
    }

    /// Human-readable reason.
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for DateTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason)
    }
}

impl std::error::Error for DateTimeError {}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn digits(&mut self, count: usize, what: &'static str) -> Result<u32, DateTimeError> {
        let end = self.pos + count;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(DateTimeError::new(what))?;
        let mut value = 0u32;
        for &b in slice {
            if !b.is_ascii_digit() {
                return Err(DateTimeError::new(what));
            }
            value = value * 10 + u32::from(b - b'0');
        }
        self.pos = end;
        Ok(value)
    }

    fn eat(&mut self, expected: u8) -> bool {
        if self.bytes.get(self.pos) == Some(&expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn at_end(&self) -> bool {
        self.pos == self.bytes.len()
    }
}

fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date
/// (algorithm from H. Hinnant, "chrono-compatible low-level date algorithms").
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl W3cDateTime {
    /// Parses and validates a W3C Datetime string.
    pub fn parse(raw: &str) -> Result<Self, DateTimeError> {
        let mut c = Cursor {
            bytes: raw.as_bytes(),
            pos: 0,
        };

        let year = c.digits(4, "expected a four-digit year (YYYY)")?;
        if year == 0 {
            return Err(DateTimeError::new("year 0000 is not valid"));
        }
        let mut month = 1;
        let mut day = 1;
        let mut precision = DateTimePrecision::Year;

        if c.eat(b'-') {
            month = c.digits(2, "expected a two-digit month (MM)")?;
            if !(1..=12).contains(&month) {
                return Err(DateTimeError::new("month must be between 01 and 12"));
            }
            precision = DateTimePrecision::Month;
            if c.eat(b'-') {
                day = c.digits(2, "expected a two-digit day (DD)")?;
                if day == 0 || day > days_in_month(year, month) {
                    return Err(DateTimeError::new("day does not exist in this month"));
                }
                precision = DateTimePrecision::Day;
            }
        }

        let mut seconds_of_day = 0i64;
        let mut nanos = 0u32;
        let mut offset_seconds = 0i64;

        if precision == DateTimePrecision::Day && c.eat(b'T') {
            let hour = c.digits(2, "expected a two-digit hour (hh)")?;
            if !c.eat(b':') {
                return Err(DateTimeError::new("expected ':' between hours and minutes"));
            }
            let minute = c.digits(2, "expected two-digit minutes (mm)")?;
            if hour > 23 || minute > 59 {
                return Err(DateTimeError::new("time of day is out of range"));
            }
            let mut second = 0;
            precision = DateTimePrecision::Minute;
            if c.eat(b':') {
                second = c.digits(2, "expected two-digit seconds (ss)")?;
                if second > 59 {
                    return Err(DateTimeError::new("seconds must be between 00 and 59"));
                }
                precision = DateTimePrecision::Second;
                if c.eat(b'.') {
                    let start = c.pos;
                    let mut scale = 100_000_000u32;
                    while let Some(b) = c.peek().filter(u8::is_ascii_digit) {
                        nanos += u32::from(b - b'0') * scale;
                        scale /= 10;
                        c.pos += 1;
                    }
                    if c.pos == start {
                        return Err(DateTimeError::new(
                            "expected at least one digit after the decimal point",
                        ));
                    }
                    precision = DateTimePrecision::Fraction;
                }
            }
            seconds_of_day = i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);

            // Time zone designator, mandatory with a time component.
            if c.eat(b'Z') {
                offset_seconds = 0;
            } else if let Some(sign @ (b'+' | b'-')) = c.peek() {
                c.pos += 1;
                let oh = c.digits(2, "expected a two-digit offset hour")?;
                if !c.eat(b':') {
                    return Err(DateTimeError::new("expected ':' in the time zone offset"));
                }
                let om = c.digits(2, "expected two-digit offset minutes")?;
                if oh > 23 || om > 59 {
                    return Err(DateTimeError::new("time zone offset is out of range"));
                }
                let magnitude = i64::from(oh) * 3600 + i64::from(om) * 60;
                offset_seconds = if sign == b'+' { magnitude } else { -magnitude };
            } else {
                return Err(DateTimeError::new(
                    "a time requires a time zone designator (Z or +hh:mm / -hh:mm)",
                ));
            }
        }

        if !c.at_end() {
            return Err(DateTimeError::new("unexpected trailing characters"));
        }

        let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
        let unix_seconds = days * 86_400 + seconds_of_day - offset_seconds;

        Ok(Self {
            raw: raw.to_owned(),
            precision,
            unix_seconds,
            nanos,
        })
    }

    /// The original text.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The precision of the original text.
    pub fn precision(&self) -> DateTimePrecision {
        self.precision
    }

    /// Seconds since the Unix epoch of the instant (start of period for partial dates).
    pub fn unix_seconds(&self) -> i64 {
        self.unix_seconds
    }

    fn instant(&self) -> (i64, u32) {
        (self.unix_seconds, self.nanos)
    }

    /// Compares the UTC instants of two values, ignoring their textual form.
    pub fn cmp_instant(&self, other: &Self) -> Ordering {
        self.instant().cmp(&other.instant())
    }
}

/// Equality is textual: two values are equal when they serialize identically.
impl PartialEq for W3cDateTime {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl Eq for W3cDateTime {}

impl fmt::Display for W3cDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_all_w3c_precisions() {
        let cases = [
            ("2026", DateTimePrecision::Year),
            ("2026-11", DateTimePrecision::Month),
            ("2026-11-21", DateTimePrecision::Day),
            ("2026-11-21T09:30Z", DateTimePrecision::Minute),
            ("2026-11-21T09:30:15+01:00", DateTimePrecision::Second),
            ("2026-11-21T09:30:15.250-05:30", DateTimePrecision::Fraction),
        ];
        for (raw, precision) in cases {
            let parsed = W3cDateTime::parse(raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
            assert_eq!(parsed.precision(), precision, "{raw}");
            assert_eq!(parsed.as_str(), raw);
        }
    }

    #[test]
    fn rejects_malformed_values() {
        for raw in [
            "",
            "26",
            "2026-1",
            "2026-13",
            "2026-00-10",
            "2026-02-30",
            "2025-02-29",
            "2026-11-21T09:30",
            "2026-11-21T24:00Z",
            "2026-11-21T09:60Z",
            "2026-11-21T09:30:60Z",
            "2026-11-21T09:30:15.Z",
            "2026-11-21t09:30Z",
            "2026-11-21T09:30z",
            "2026-11-21T09:30+0100",
            "2026-11-21 09:30Z",
            "0000-01-01",
            "yesterday",
            "2026-11-21Z",
        ] {
            assert!(W3cDateTime::parse(raw).is_err(), "{raw} should be rejected");
        }
    }

    #[test]
    fn handles_leap_years() {
        assert!(W3cDateTime::parse("2024-02-29").is_ok());
        assert!(W3cDateTime::parse("2000-02-29").is_ok());
        assert!(W3cDateTime::parse("1900-02-29").is_err());
    }

    #[test]
    fn computes_utc_instants() {
        assert_eq!(W3cDateTime::parse("1970-01-01").unwrap().unix_seconds(), 0);
        assert_eq!(
            W3cDateTime::parse("2026-11-21T00:00:00Z")
                .unwrap()
                .unix_seconds(),
            1_795_219_200
        );
        let a = W3cDateTime::parse("2026-11-21T10:00:00+01:00").unwrap();
        let b = W3cDateTime::parse("2026-11-21T09:00:00Z").unwrap();
        assert_eq!(a.cmp_instant(&b), Ordering::Equal);
        assert_ne!(a, b, "equality is textual");
        let c = W3cDateTime::parse("2026-11-21T09:00:00.5Z").unwrap();
        assert_eq!(b.cmp_instant(&c), Ordering::Less);
        let year = W3cDateTime::parse("2026").unwrap();
        let jan = W3cDateTime::parse("2026-01-01").unwrap();
        assert_eq!(year.cmp_instant(&jan), Ordering::Equal);
    }
}

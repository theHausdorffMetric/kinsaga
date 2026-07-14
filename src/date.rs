//! Date parsing with uncertainty support.
//!
//! Supports ISO 8601 dates with optional `?` suffix for uncertainty:
//! - `1987` - year only
//! - `1987-03` - year and month
//! - `1987-03-03` - full date
//! - `1987?` - uncertain year
//! - `1987-03?` - uncertain month

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Error type for date parsing.
#[derive(Debug, Error)]
pub enum DateError {
    #[error("invalid date format: {0}")]
    InvalidFormat(String),

    #[error("invalid year: {0}")]
    InvalidYear(String),

    #[error("invalid month: {0}")]
    InvalidMonth(String),

    #[error("invalid day: {0}")]
    InvalidDay(String),
}

/// A parsed chronicle date with optional uncertainty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChronicleDate {
    /// Original date string
    pub raw: String,

    /// Year (if parseable)
    pub year: Option<u16>,

    /// Month (1-12, if present)
    pub month: Option<u8>,

    /// Day (1-31, if present)
    pub day: Option<u8>,

    /// Whether the date is uncertain (ends with ?)
    pub uncertain: bool,
}

impl ChronicleDate {
    /// Parse a date string.
    pub fn parse(s: &str) -> Result<Self, DateError> {
        s.parse()
    }

    /// Returns true if this date has full precision (year, month, day).
    pub fn is_complete(&self) -> bool {
        self.year.is_some() && self.month.is_some() && self.day.is_some()
    }

    /// Returns the sort key for ordering dates chronologically.
    /// Format: (year, month, day) with 0 for missing components.
    pub fn sort_key(&self) -> (u16, u8, u8) {
        (
            self.year.unwrap_or(0),
            self.month.unwrap_or(0),
            self.day.unwrap_or(0),
        )
    }
}

impl FromStr for ChronicleDate {
    type Err = DateError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let raw = s.to_string();

        // Check for uncertainty marker
        let (date_part, uncertain) = if let Some(stripped) = s.strip_suffix('?') {
            (stripped, true)
        } else {
            (s, false)
        };

        // Split by dash
        let parts: Vec<&str> = date_part.split('-').collect();

        let (year, month, day) = match parts.len() {
            1 => {
                // Year only: "1987" or "1987?"
                let year = parse_year(parts[0])?;
                (Some(year), None, None)
            }
            2 => {
                // Year and month: "1987-03"
                let year = parse_year(parts[0])?;
                let month = parse_month(parts[1])?;
                (Some(year), Some(month), None)
            }
            3 => {
                // Full date: "1987-03-03"
                let year = parse_year(parts[0])?;
                let month = parse_month(parts[1])?;
                let day = parse_day(parts[2])?;
                (Some(year), Some(month), Some(day))
            }
            _ => {
                return Err(DateError::InvalidFormat(raw));
            }
        };

        Ok(ChronicleDate {
            raw,
            year,
            month,
            day,
            uncertain,
        })
    }
}

impl fmt::Display for ChronicleDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

impl PartialOrd for ChronicleDate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChronicleDate {
    /// Orders chronologically by [`sort_key`](Self::sort_key); sort-key ties
    /// (e.g. "1987" vs "1987?") are broken by the remaining fields so that
    /// `cmp` returning `Equal` coincides exactly with `==`, as the `Ord`
    /// contract requires.
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key()).then_with(|| {
            (self.year, self.month, self.day, self.uncertain, &self.raw).cmp(&(
                other.year,
                other.month,
                other.day,
                other.uncertain,
                &other.raw,
            ))
        })
    }
}

fn parse_year(s: &str) -> Result<u16, DateError> {
    s.parse::<u16>()
        .map_err(|_| DateError::InvalidYear(s.to_string()))
}

fn parse_month(s: &str) -> Result<u8, DateError> {
    let month = s
        .parse::<u8>()
        .map_err(|_| DateError::InvalidMonth(s.to_string()))?;

    if !(1..=12).contains(&month) {
        return Err(DateError::InvalidMonth(s.to_string()));
    }

    Ok(month)
}

fn parse_day(s: &str) -> Result<u8, DateError> {
    let day = s
        .parse::<u8>()
        .map_err(|_| DateError::InvalidDay(s.to_string()))?;

    if !(1..=31).contains(&day) {
        return Err(DateError::InvalidDay(s.to_string()));
    }

    Ok(day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_year_only() {
        let date = ChronicleDate::parse("1987").unwrap();
        assert_eq!(date.year, Some(1987));
        assert_eq!(date.month, None);
        assert_eq!(date.day, None);
        assert!(!date.uncertain);
    }

    #[test]
    fn test_parse_year_month() {
        let date = ChronicleDate::parse("1987-03").unwrap();
        assert_eq!(date.year, Some(1987));
        assert_eq!(date.month, Some(3));
        assert_eq!(date.day, None);
        assert!(!date.uncertain);
    }

    #[test]
    fn test_parse_full_date() {
        let date = ChronicleDate::parse("1987-03-15").unwrap();
        assert_eq!(date.year, Some(1987));
        assert_eq!(date.month, Some(3));
        assert_eq!(date.day, Some(15));
        assert!(!date.uncertain);
        assert!(date.is_complete());
    }

    #[test]
    fn test_parse_uncertain_year() {
        let date = ChronicleDate::parse("1987?").unwrap();
        assert_eq!(date.year, Some(1987));
        assert!(date.uncertain);
    }

    #[test]
    fn test_parse_uncertain_full_date() {
        let date = ChronicleDate::parse("1987-03-15?").unwrap();
        assert_eq!(date.year, Some(1987));
        assert_eq!(date.month, Some(3));
        assert_eq!(date.day, Some(15));
        assert!(date.uncertain);
    }

    #[test]
    fn test_date_ordering() {
        let d1 = ChronicleDate::parse("1986").unwrap();
        let d2 = ChronicleDate::parse("1987").unwrap();
        let d3 = ChronicleDate::parse("1987-03").unwrap();
        let d4 = ChronicleDate::parse("1987-03-15").unwrap();

        assert!(d1 < d2);
        assert!(d2 < d3);
        assert!(d3 < d4);
    }

    #[test]
    fn test_ord_consistent_with_eq() {
        let certain = ChronicleDate::parse("1987").unwrap();
        let uncertain = ChronicleDate::parse("1987?").unwrap();

        // Same sort key, but not equal — cmp must not return Equal
        assert_ne!(certain, uncertain);
        assert_ne!(certain.cmp(&uncertain), Ordering::Equal);
        assert!(certain < uncertain, "certain date sorts before uncertain on ties");

        let same = ChronicleDate::parse("1987").unwrap();
        assert_eq!(certain, same);
        assert_eq!(certain.cmp(&same), Ordering::Equal);
    }

    #[test]
    fn test_display() {
        let date = ChronicleDate::parse("1987-03?").unwrap();
        assert_eq!(format!("{}", date), "1987-03?");
    }

    #[test]
    fn test_invalid_month() {
        assert!(ChronicleDate::parse("1987-13").is_err());
        assert!(ChronicleDate::parse("1987-00").is_err());
    }

    #[test]
    fn test_invalid_day() {
        assert!(ChronicleDate::parse("1987-03-32").is_err());
        assert!(ChronicleDate::parse("1987-03-00").is_err());
    }
}

//! Date-or-datetime values for task frontmatter fields (`due`, `start`, `completed`).
//!
//! Per [ADR 0115 §8](../../../docs/adr/0115-tasks-and-projects-as-typed-notes.md),
//! these fields accept either an ISO 8601 date (`YYYY-MM-DD`) or a full RFC 3339
//! datetime (`YYYY-MM-DDTHH:MM:SS±HH:MM` / `...Z`). A datetime without an explicit
//! offset is treated as the system's local timezone at parse time.
//!
//! This module is a parse-and-format helper. The values themselves live in
//! [`VaultEntry::properties`](super::entry::VaultEntry) as strings; typed access
//! happens at read time in [`TaskView`](super::entry::TaskView).

use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone};

/// Either a date (no time component) or a datetime with timezone offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateOrDateTime {
    Date(NaiveDate),
    DateTime(DateTime<FixedOffset>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid date or datetime: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

impl DateOrDateTime {
    /// Parse a frontmatter value string into a `DateOrDateTime`.
    ///
    /// Accepted forms:
    /// - `YYYY-MM-DD` → `Date`
    /// - `YYYY-MM-DDTHH:MM:SSZ` or `YYYY-MM-DDTHH:MM:SS±HH:MM` → `DateTime` with explicit offset
    /// - `YYYY-MM-DDTHH:MM:SS` (no offset) → `DateTime` in system local timezone at parse time
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        let trimmed = s.trim();
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
            return Ok(Self::Date(date));
        }
        if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
            return Ok(Self::DateTime(dt));
        }
        if let Ok(naive) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
            return local_naive_to_fixed(naive)
                .map(Self::DateTime)
                .ok_or_else(|| ParseError(s.to_string()));
        }
        Err(ParseError(s.to_string()))
    }

    /// Round-trippable string form for writing back to frontmatter.
    pub fn to_storage_string(&self) -> String {
        match self {
            Self::Date(d) => d.format("%Y-%m-%d").to_string(),
            Self::DateTime(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
        }
    }

    /// Date component for day-granularity filters (drops the time).
    pub fn to_naive_date(&self) -> NaiveDate {
        match self {
            Self::Date(d) => *d,
            Self::DateTime(dt) => dt.date_naive(),
        }
    }

    /// True if this value carries a time-of-day component.
    pub fn has_time(&self) -> bool {
        matches!(self, Self::DateTime(_))
    }
}

/// Promote a naive datetime to a `DateTime<FixedOffset>` using the system local timezone.
fn local_naive_to_fixed(naive: NaiveDateTime) -> Option<DateTime<FixedOffset>> {
    let local = Local.from_local_datetime(&naive).single()?;
    Some(local.fixed_offset())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_date_only() {
        let v = DateOrDateTime::parse("2026-05-20").unwrap();
        assert!(matches!(v, DateOrDateTime::Date(_)));
        assert_eq!(
            v.to_naive_date(),
            NaiveDate::from_ymd_opt(2026, 5, 20).unwrap()
        );
        assert!(!v.has_time());
    }

    #[test]
    fn parses_datetime_with_z_offset() {
        let v = DateOrDateTime::parse("2026-05-20T14:00:00Z").unwrap();
        assert!(matches!(v, DateOrDateTime::DateTime(_)));
        assert_eq!(
            v.to_naive_date(),
            NaiveDate::from_ymd_opt(2026, 5, 20).unwrap()
        );
        assert!(v.has_time());
    }

    #[test]
    fn parses_datetime_with_positive_offset() {
        let v = DateOrDateTime::parse("2026-05-20T14:00:00+02:00").unwrap();
        if let DateOrDateTime::DateTime(dt) = v {
            assert_eq!(dt.offset().local_minus_utc(), 2 * 3600);
        } else {
            panic!("expected DateTime");
        }
    }

    #[test]
    fn parses_datetime_with_negative_offset() {
        let v = DateOrDateTime::parse("2026-05-20T14:00:00-08:00").unwrap();
        if let DateOrDateTime::DateTime(dt) = v {
            assert_eq!(dt.offset().local_minus_utc(), -8 * 3600);
        } else {
            panic!("expected DateTime");
        }
    }

    #[test]
    fn parses_naive_datetime_as_local_time() {
        let v = DateOrDateTime::parse("2026-05-20T14:00:00").unwrap();
        assert!(matches!(v, DateOrDateTime::DateTime(_)));
        // Date component is preserved regardless of offset
        assert_eq!(
            v.to_naive_date(),
            NaiveDate::from_ymd_opt(2026, 5, 20).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(DateOrDateTime::parse("not a date").is_err());
        assert!(DateOrDateTime::parse("2026-13-01").is_err()); // invalid month
        assert!(DateOrDateTime::parse("").is_err());
        assert!(DateOrDateTime::parse("2026/05/20").is_err()); // wrong separator
    }

    #[test]
    fn trims_whitespace() {
        let v = DateOrDateTime::parse("  2026-05-20  ").unwrap();
        assert_eq!(
            v.to_naive_date(),
            NaiveDate::from_ymd_opt(2026, 5, 20).unwrap()
        );
    }

    #[test]
    fn date_round_trip() {
        let original = "2026-05-20";
        let parsed = DateOrDateTime::parse(original).unwrap();
        assert_eq!(parsed.to_storage_string(), original);
    }

    #[test]
    fn datetime_round_trip_with_z() {
        // RFC 3339 normalizes "Z" to "+00:00" in the round-trip
        let parsed = DateOrDateTime::parse("2026-05-20T14:00:00Z").unwrap();
        let serialized = parsed.to_storage_string();
        // Both forms round-trip to the same value
        let reparsed = DateOrDateTime::parse(&serialized).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn datetime_round_trip_with_offset() {
        let original = "2026-05-20T14:00:00+02:00";
        let parsed = DateOrDateTime::parse(original).unwrap();
        assert_eq!(parsed.to_storage_string(), original);
    }

    #[test]
    fn to_naive_date_drops_time() {
        let v = DateOrDateTime::parse("2026-05-20T23:59:59+02:00").unwrap();
        assert_eq!(
            v.to_naive_date(),
            NaiveDate::from_ymd_opt(2026, 5, 20).unwrap()
        );
    }
}

//! Times on the command line: `HH:MM` today, a local date and time, or an
//! RFC 3339 time with offset.

use jiff::civil::{DateTime, Time};
use jiff::tz::TimeZone;
use jiff::{Timestamp, ZonedDateTime};

/// Parses `input` with `now` for `HH:MM`, which is always today in the zone
/// of `now`, also when the time has passed: never tomorrow.
pub(crate) fn parse(input: &str, now: &ZonedDateTime) -> Result<Timestamp, String> {
    let input = input.trim();
    if let Ok(timestamp) = input.parse::<Timestamp>() {
        return Ok(timestamp);
    }
    let zone = now.time_zone();
    // The parser of times also takes the time of a date and time, so only
    // input without a date is a time of today.
    let has_date = input.contains(['-', 'T', 't', ' ']);
    if has_date && let Ok(datetime) = input.parse::<DateTime>() {
        return local(datetime, zone);
    }
    if !has_date && let Ok(time) = input.parse::<Time>() {
        return local(now.date().to_datetime(time), zone);
    }
    Err(format!(
        "invalid time {input:?}: use HH:MM for today, a local time like \
         2026-11-27T07:50, or RFC 3339 like 2026-11-27T07:50:00+01:00"
    ))
}

fn local(datetime: DateTime, zone: &TimeZone) -> Result<Timestamp, String> {
    datetime
        .to_zoned_datetime(zone.clone())
        .map(|zoned| zoned.timestamp())
        .map_err(|error| format!("invalid time {datetime}: {error}"))
}

/// Parses a time for clap, relative to now in the local zone.
pub(crate) fn arg(input: &str) -> Result<Timestamp, String> {
    parse(input, &ZonedDateTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> ZonedDateTime {
        "2026-11-27T09:15:00+01:00[Europe/Berlin]".parse().unwrap()
    }

    fn parsed(input: &str) -> String {
        parse(input, &now()).unwrap().to_string()
    }

    #[test]
    fn parses_times_of_today() {
        assert_eq!(parsed("07:50"), "2026-11-27T06:50:00Z");
        // Past times stay today.
        assert_eq!(parsed("00:10"), "2026-11-26T23:10:00Z");
        assert_eq!(parsed(" 23:59 "), "2026-11-27T22:59:00Z");
        assert_eq!(parsed("2026-11-28T07:50"), "2026-11-28T06:50:00Z");
        assert_eq!(parsed("2026-11-28T07:50:00+01:00"), "2026-11-28T06:50:00Z");
        assert_eq!(parsed("2026-11-28T06:50:00Z"), "2026-11-28T06:50:00Z");
        // Summer time.
        assert_eq!(parsed("2026-07-01T07:50"), "2026-07-01T05:50:00Z");
    }

    #[test]
    fn rejects_other_input() {
        for input in ["", "7.50", "25:00", "tomorrow", "2026-02-30T07:50"] {
            let error = parse(input, &now()).unwrap_err();
            assert!(error.starts_with("invalid time"), "{input}: {error}");
        }
        assert!(arg("07:50").is_ok());
        // Beyond the last time there is.
        let error = parse("9999-12-31T23:59", &now()).unwrap_err();
        assert!(
            error.starts_with("invalid time 9999-12-31T23:59"),
            "{error}"
        );
    }
}

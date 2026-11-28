//! The time of a search: an instant, or a local time that means the time at
//! the stop, like the times of the results.

use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::TimeZone;

/// The time of a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryTime {
    /// An instant, like `2026-11-27T07:50:00+01:00`.
    At(Timestamp),
    /// A date and time in the time zone of the stop, like
    /// `2026-11-27T07:50`.
    Local(DateTime),
    /// A time of today in the time zone of the stop, like `07:50`: today
    /// also when the time has passed, never tomorrow.
    Today(Time),
}

impl QueryTime {
    /// The instant of this time in `zone`, with `now` for the date of
    /// [`QueryTime::Today`].
    ///
    /// # Errors
    ///
    /// A local time beyond the times there are, as an error message.
    pub fn in_zone(&self, zone: &TimeZone, now: Timestamp) -> Result<Timestamp, String> {
        let datetime = match *self {
            Self::At(timestamp) => return Ok(timestamp),
            Self::Local(datetime) => datetime,
            Self::Today(time) => today(zone, now).to_datetime(time),
        };
        datetime
            .to_zoned_datetime(zone.clone())
            .map(|zoned| zoned.timestamp())
            .map_err(|error| format!("invalid time {datetime}: {error}"))
    }

    /// Whether the instant depends on the time zone.
    pub fn is_local(&self) -> bool {
        !matches!(self, Self::At(_))
    }
}

fn today(zone: &TimeZone, now: Timestamp) -> Date {
    now.to_zoned_datetime(zone.clone()).date()
}

impl From<Timestamp> for QueryTime {
    fn from(timestamp: Timestamp) -> Self {
        Self::At(timestamp)
    }
}

impl FromStr for QueryTime {
    type Err = String;

    /// `HH:MM`, a local date and time, or RFC 3339 with an offset.
    fn from_str(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if let Ok(timestamp) = input.parse::<Timestamp>() {
            return Ok(Self::At(timestamp));
        }
        // The parser of times also takes the time of a date and time, so only
        // input without a date is a time of today.
        let has_date = input.contains(['-', 'T', 't', ' ']);
        if has_date && let Ok(datetime) = input.parse::<DateTime>() {
            return Ok(Self::Local(datetime));
        }
        if !has_date && let Ok(time) = input.parse::<Time>() {
            return Ok(Self::Today(time));
        }
        Err(format!(
            "invalid time {input:?}: use HH:MM for today, a local time like \
             2026-11-27T07:50, or RFC 3339 like 2026-11-27T07:50:00+01:00"
        ))
    }
}

impl fmt::Display for QueryTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::At(timestamp) => timestamp.fmt(f),
            Self::Local(datetime) => datetime.fmt(f),
            Self::Today(time) => time.fmt(f),
        }
    }
}

/// The time zone named `name`, if it is known.
pub(crate) fn zone(name: Option<&str>) -> Option<TimeZone> {
    TimeZone::get(name?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-11-27T08:15:00Z".parse().unwrap()
    }

    fn berlin() -> TimeZone {
        TimeZone::get("Europe/Berlin").unwrap()
    }

    fn resolved(input: &str, zone: &TimeZone) -> String {
        let time: QueryTime = input.parse().unwrap();
        time.in_zone(zone, now()).unwrap().to_string()
    }

    #[test]
    fn reads_local_times_in_the_zone() {
        let berlin = berlin();
        assert_eq!(resolved("07:50", &berlin), "2026-11-27T06:50:00Z");
        // Past times stay today.
        assert_eq!(resolved("00:10", &berlin), "2026-11-26T23:10:00Z");
        assert_eq!(resolved(" 23:59 ", &berlin), "2026-11-27T22:59:00Z");
        assert_eq!(
            resolved("2026-11-28T07:50", &berlin),
            "2026-11-28T06:50:00Z"
        );
        // Summer time.
        assert_eq!(
            resolved("2026-07-01T07:50", &berlin),
            "2026-07-01T05:50:00Z"
        );
        assert_eq!(resolved("07:50", &TimeZone::UTC), "2026-11-27T07:50:00Z");
        // Today is the date in the zone: already the 28th in Tokyo.
        let tokyo = TimeZone::get("Asia/Tokyo").unwrap();
        let late: Timestamp = "2026-11-27T20:00:00Z".parse().unwrap();
        let time: QueryTime = "07:50".parse().unwrap();
        assert_eq!(
            time.in_zone(&tokyo, late).unwrap().to_string(),
            "2026-11-27T22:50:00Z"
        );
    }

    #[test]
    fn keeps_instants() {
        let berlin = berlin();
        for input in ["2026-11-28T07:50:00+01:00", "2026-11-28T06:50:00Z"] {
            let time: QueryTime = input.parse().unwrap();
            assert!(!time.is_local());
            assert_eq!(resolved(input, &berlin), "2026-11-28T06:50:00Z");
            assert_eq!(resolved(input, &TimeZone::UTC), "2026-11-28T06:50:00Z");
        }
        let at: Timestamp = "2026-11-28T06:50:00Z".parse().unwrap();
        assert_eq!(QueryTime::from(at), QueryTime::At(at));
        assert_eq!(QueryTime::from(at).to_string(), "2026-11-28T06:50:00Z");
        assert!("07:50".parse::<QueryTime>().unwrap().is_local());
        assert_eq!(
            "07:50".parse::<QueryTime>().unwrap().to_string(),
            "07:50:00"
        );
        let local: QueryTime = "2026-11-28T07:50".parse().unwrap();
        assert!(local.is_local());
        assert_eq!(local.to_string(), "2026-11-28T07:50:00");
    }

    #[test]
    fn rejects_other_input() {
        for input in ["", "7.50", "25:00", "tomorrow", "2026-02-30T07:50"] {
            let error = input.parse::<QueryTime>().unwrap_err();
            assert!(error.starts_with("invalid time"), "{input}: {error}");
        }
        // Beyond the last time there is.
        let time: QueryTime = "9999-12-31T23:59".parse().unwrap();
        let error = time.in_zone(&berlin(), now()).unwrap_err();
        assert!(
            error.starts_with("invalid time 9999-12-31T23:59"),
            "{error}"
        );
    }

    #[test]
    fn finds_zones_by_name() {
        assert!(zone(Some("Europe/Berlin")).is_some());
        assert!(zone(Some("Nowhere/Else")).is_none());
        assert!(zone(None).is_none());
    }
}

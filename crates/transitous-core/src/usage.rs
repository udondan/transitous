//! The usage counter: the requests of the last 24 hours, in a file shared by
//! every process on the machine, so the CLI and the MCP server keep to the
//! limits together.
//!
//! - At most [`PER_MINUTE`] requests per minute, all endpoints together.
//! - At most `plan_daily_limit` `plan` requests per 24 hours, with a warning
//!   from [`WARN_PERCENT`] percent on.
//!
//! Only requests that reach the server count: cached answers do not.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::Error;
use crate::model::{EndpointUsage, UsageSummary};

pub const PER_MINUTE: u32 = 20;
pub const WARN_PERCENT: u32 = 80;
const MINUTE: i64 = 60;
const DAY: i64 = 24 * 60 * 60;

/// An endpoint of the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Endpoint {
    Geocode,
    ReverseGeocode,
    Plan,
    Stoptimes,
    Trip,
}

impl Endpoint {
    pub const ALL: [Self; 5] = [
        Self::Geocode,
        Self::ReverseGeocode,
        Self::Plan,
        Self::Stoptimes,
        Self::Trip,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Geocode => "geocode",
            Self::ReverseGeocode => "reverse-geocode",
            Self::Plan => "plan",
            Self::Stoptimes => "stoptimes",
            Self::Trip => "trip",
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    requests: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    endpoint: Endpoint,
    at: Timestamp,
}

/// The counter file and the limits.
#[derive(Debug, Clone)]
pub struct Usage {
    path: PathBuf,
    plan_daily_limit: u32,
}

impl Usage {
    /// The counter `usage.json` in `state_dir`, else in the temporary
    /// directory.
    pub fn new(state_dir: Option<&Path>, plan_daily_limit: u32) -> Self {
        let dir = state_dir.map_or_else(|| std::env::temp_dir().join("transitous"), Path::to_owned);
        Self {
            path: dir.join("usage.json"),
            plan_daily_limit,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Counts a request to `endpoint` at `now`, if the limits allow it.
    /// Returns a warning when the `plan` requests near the daily limit.
    pub fn admit(&self, endpoint: Endpoint, now: Timestamp) -> Result<Option<String>, Error> {
        self.update(now, |state| {
            let in_minute: Vec<&Entry> = since(state, now, MINUTE).collect();
            if in_minute.len() >= PER_MINUTE as usize {
                return Err(Error::RateLimited {
                    retry_after_seconds: retry_after(in_minute[0].at, MINUTE, now),
                });
            }
            let mut warning = None;
            if endpoint == Endpoint::Plan {
                let plans: Vec<&Entry> = since(state, now, DAY)
                    .filter(|entry| entry.endpoint == Endpoint::Plan)
                    .collect();
                let used = u32::try_from(plans.len()).unwrap_or(u32::MAX);
                if used >= self.plan_daily_limit {
                    return Err(Error::DailyLimit {
                        limit: self.plan_daily_limit,
                        retry_after_seconds: plans
                            .first()
                            .map_or(MINUTE.unsigned_abs(), |e| retry_after(e.at, DAY, now)),
                    });
                }
                let used = used + 1;
                if u64::from(used) * 100
                    >= u64::from(self.plan_daily_limit) * u64::from(WARN_PERCENT)
                {
                    warning = Some(format!(
                        "{used} of {} plan requests in 24 hours used",
                        self.plan_daily_limit
                    ));
                }
            }
            state.requests.push(Entry { endpoint, at: now });
            Ok(warning)
        })
    }

    /// The requests of the last 24 hours.
    pub fn summary(&self, now: Timestamp) -> Result<UsageSummary, Error> {
        self.update(now, |state| {
            let count = |endpoint| {
                since(state, now, DAY)
                    .filter(|entry| entry.endpoint == endpoint)
                    .count()
            };
            let plans = u32::try_from(count(Endpoint::Plan)).unwrap_or(u32::MAX);
            Ok(UsageSummary {
                last_24h: Endpoint::ALL
                    .into_iter()
                    .map(|endpoint| EndpointUsage {
                        endpoint: endpoint.as_str().to_owned(),
                        requests: u32::try_from(count(endpoint)).unwrap_or(u32::MAX),
                    })
                    .collect(),
                last_minute: u32::try_from(since(state, now, MINUTE).count()).unwrap_or(u32::MAX),
                per_minute_limit: PER_MINUTE,
                plan_daily_limit: self.plan_daily_limit,
                plan_remaining: self.plan_daily_limit.saturating_sub(plans),
                path: self.path.display().to_string(),
            })
        })
    }

    /// Runs `change` on the state with the file locked, after dropping the
    /// entries older than a day, and writes the state back.
    fn update<T>(
        &self,
        now: Timestamp,
        change: impl FnOnce(&mut State) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let fail = |source| Error::Usage {
            path: self.path.display().to_string(),
            source,
        };
        std::fs::create_dir_all(self.path.parent().unwrap_or(Path::new(""))).map_err(fail)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
            .map_err(fail)?;
        file.lock().map_err(fail)?;
        let mut text = String::new();
        file.read_to_string(&mut text).map_err(fail)?;
        // A damaged counter starts over rather than blocking every request.
        let mut state: State = serde_json::from_str(&text).unwrap_or_default();
        state
            .requests
            .retain(|entry| now.as_second() - entry.at.as_second() < DAY);
        let result = change(&mut state)?;
        write(&mut file, &state).map_err(fail)?;
        Ok(result)
    }
}

fn write(file: &mut File, state: &State) -> std::io::Result<()> {
    let text = serde_json::to_vec(state).map_err(std::io::Error::other)?;
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(&text)
}

/// The entries of the last `seconds`, oldest first.
fn since(state: &State, now: Timestamp, seconds: i64) -> impl Iterator<Item = &Entry> {
    state
        .requests
        .iter()
        .filter(move |entry| now.as_second() - entry.at.as_second() < seconds)
}

/// Seconds until `oldest` leaves a window of `seconds`, at least one.
fn retry_after(oldest: Timestamp, seconds: i64, now: Timestamp) -> u64 {
    (oldest.as_second() + seconds - now.as_second())
        .max(1)
        .unsigned_abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(1_800_000_000 + second).unwrap()
    }

    fn usage(limit: u32) -> (tempfile::TempDir, Usage) {
        let dir = tempfile::tempdir().unwrap();
        let usage = Usage::new(Some(&dir.path().join("state")), limit);
        (dir, usage)
    }

    #[test]
    fn allows_twenty_requests_a_minute() {
        let (_dir, usage) = usage(720);
        for second in 0..20 {
            usage.admit(Endpoint::Stoptimes, at(second)).unwrap();
        }
        let error = usage.admit(Endpoint::Geocode, at(30)).unwrap_err();
        assert!(
            matches!(
                error,
                Error::RateLimited {
                    retry_after_seconds: 30
                }
            ),
            "{error:?}"
        );
        usage.admit(Endpoint::Geocode, at(60)).unwrap();
    }

    #[test]
    fn limits_plan_requests_per_day() {
        let (_dir, usage) = usage(5);
        let mut warnings = Vec::new();
        for minute in 0..5 {
            warnings.push(usage.admit(Endpoint::Plan, at(minute * 60)).unwrap());
        }
        assert_eq!(warnings[..3], [None, None, None]);
        assert_eq!(
            warnings[3].as_deref(),
            Some("4 of 5 plan requests in 24 hours used")
        );
        let error = usage.admit(Endpoint::Plan, at(3600)).unwrap_err();
        assert!(
            matches!(
                error,
                Error::DailyLimit {
                    limit: 5,
                    retry_after_seconds: 82_800
                }
            ),
            "{error:?}"
        );
        // Other endpoints go on, and a day later plan does too.
        usage.admit(Endpoint::Trip, at(3600)).unwrap();
        usage.admit(Endpoint::Plan, at(DAY)).unwrap();
    }

    #[test]
    fn a_zero_limit_refuses_every_plan() {
        let (_dir, usage) = usage(0);
        let error = usage.admit(Endpoint::Plan, at(0)).unwrap_err();
        assert!(
            matches!(
                error,
                Error::DailyLimit {
                    retry_after_seconds: 60,
                    ..
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn sums_up_the_last_day() {
        let (_dir, usage) = usage(720);
        usage.admit(Endpoint::Plan, at(0)).unwrap();
        usage.admit(Endpoint::Plan, at(DAY - 100)).unwrap();
        usage.admit(Endpoint::Geocode, at(DAY - 10)).unwrap();
        let summary = usage.summary(at(DAY)).unwrap();
        let counts: Vec<(&str, u32)> = summary
            .last_24h
            .iter()
            .map(|e| (e.endpoint.as_str(), e.requests))
            .collect();
        assert_eq!(
            counts,
            [
                ("geocode", 1),
                ("reverse-geocode", 0),
                ("plan", 1),
                ("stoptimes", 0),
                ("trip", 0)
            ]
        );
        assert_eq!(summary.last_minute, 1);
        assert_eq!(summary.plan_remaining, 719);
        assert!(summary.path.ends_with("usage.json"), "{}", summary.path);
    }

    #[test]
    fn a_damaged_counter_starts_over() {
        let (_dir, usage) = usage(720);
        std::fs::create_dir_all(usage.path().parent().unwrap()).unwrap();
        std::fs::write(usage.path(), "not json").unwrap();
        usage.admit(Endpoint::Plan, at(0)).unwrap();
        assert_eq!(usage.summary(at(1)).unwrap().last_24h[2].requests, 1);
    }

    #[test]
    fn reports_a_counter_it_cannot_write() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").unwrap();
        let usage = Usage::new(Some(&blocker.join("state")), 720);
        let error = usage.admit(Endpoint::Plan, at(0)).unwrap_err();
        assert_eq!(error.status(), "usage_error");
        assert!(error.to_string().contains("usage counter"), "{error}");
    }

    #[test]
    fn falls_back_to_the_temporary_directory() {
        let usage = Usage::new(None, 720);
        assert!(usage.path().starts_with(std::env::temp_dir()));
    }
}

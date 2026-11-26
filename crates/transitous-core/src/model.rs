//! The output: the same types for JSON, YAML and the structured content of
//! the MCP tools. Times are RFC 3339 with the offset of the stop's time
//! zone, like `2026-11-26T21:22:00+01:00`. Delays are whole minutes, late
//! positive, early negative.
//!
//! Texts from the API (stop names, alert texts) are data: they may contain
//! anything.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::status::{Evaluation, Realtime, Reason, Status};

/// Longest alert text, in bytes. Longer texts are cut.
pub const MAX_ALERT_TEXT: usize = 2048;

/// The data sources and the terms of Transitous. Keep it with the data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Attribution {
    pub text: String,
    /// The timetable sources and their licenses.
    pub sources_url: String,
    /// The usage rules of the API.
    pub usage_rules_url: String,
}

impl Default for Attribution {
    fn default() -> Self {
        Self {
            text: "Data: Transitous (https://transitous.org) and its sources, \
                   map data © OpenStreetMap contributors. Transitous is run by \
                   volunteers, without guarantee."
                .to_owned(),
            sources_url: "https://transitous.org/sources/".to_owned(),
            usage_rules_url: "https://transitous.org/api/".to_owned(),
        }
    }
}

/// A stop, as found by name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StopMatch {
    /// The ID to pass instead of the name.
    pub id: String,
    pub name: String,
    /// The town or district, to tell stops of the same name apart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    pub lat: f64,
    pub lon: f64,
    /// The modes that serve the stop, like `SUBWAY`.
    pub modes: Vec<String>,
}

impl From<&motis::Match> for StopMatch {
    fn from(found: &motis::Match) -> Self {
        Self {
            id: found.id.clone(),
            name: found.name.clone(),
            area: area(found),
            lat: found.lat,
            lon: found.lon,
            modes: found.modes.iter().map(|m| m.as_str().to_owned()).collect(),
        }
    }
}

/// The area to show: the default one, else the most specific.
pub(crate) fn area(found: &motis::Match) -> Option<String> {
    found
        .areas
        .iter()
        .find(|area| area.default == Some(true))
        .or(found.areas.first())
        .map(|area| area.name.clone())
}

/// The answer to a stop search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FindStopResult {
    pub query: String,
    pub stops: Vec<StopMatch>,
    pub attribution: Attribution,
    pub warnings: Vec<String>,
}

/// A stop of a connection or departure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
}

impl From<&motis::Place> for Location {
    fn from(place: &motis::Place) -> Self {
        Self {
            id: place.stop_id.clone(),
            name: place.name.clone(),
        }
    }
}

/// A time by the schedule and the expected or actual one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Time {
    pub scheduled: String,
    /// The best known time: with realtime data the expected or actual one,
    /// else the schedule.
    pub actual: String,
    /// `actual` minus `scheduled`. Absent without realtime data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_minutes: Option<i64>,
}

impl Time {
    pub(crate) fn new(
        scheduled: Timestamp,
        actual: Timestamp,
        realtime: bool,
        tz: Option<&str>,
    ) -> Self {
        Self {
            scheduled: format_time(scheduled, tz),
            actual: format_time(actual, tz),
            delay_minutes: realtime.then(|| delay_minutes(scheduled, actual)),
        }
    }
}

/// Where a leg starts or ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StopEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(flatten)]
    pub time: Time,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_track: Option<String>,
    /// Whether boarding or alighting here is cancelled.
    pub cancelled: bool,
}

/// One leg of a connection: a ride or a walk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Leg {
    /// Like `SUBWAY`, `BUS` or `WALK`.
    pub mode: String,
    /// The line, like `U5`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headsign: Option<String>,
    /// For `transitous journey`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trip_id: Option<String>,
    pub from: StopEvent,
    pub to: StopEvent,
    pub duration_minutes: i64,
    /// Whether realtime data exists for this leg.
    pub realtime: bool,
    /// Whether the trip is cancelled.
    pub cancelled: bool,
    /// Whether passengers stay in the vehicle from the previous leg.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stay_seated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<Alert>,
}

impl From<&motis::Leg> for Leg {
    fn from(leg: &motis::Leg) -> Self {
        let realtime = leg.real_time;
        let event = |place: &motis::Place, scheduled, actual, side_cancelled| StopEvent {
            id: place.stop_id.clone(),
            name: place.name.clone(),
            time: Time::new(scheduled, actual, realtime, place.tz.as_deref()),
            track: place.track.clone(),
            scheduled_track: place.scheduled_track.clone(),
            cancelled: place.cancelled == Some(true) || side_cancelled,
        };
        let not_allowed = |kind: &Option<motis::PickupDropoffType>| {
            *kind == Some(motis::PickupDropoffType::NotAllowed)
        };
        let transit = leg.mode.is_transit();
        Self {
            mode: leg.mode.as_str().to_owned(),
            line: non_empty(leg.display_name.as_deref()),
            operator: non_empty(leg.agency_name.as_deref()),
            headsign: non_empty(leg.headsign.as_deref()),
            trip_id: leg.trip_id.clone(),
            from: event(
                &leg.from,
                leg.scheduled_start_time,
                leg.start_time,
                transit && not_allowed(&leg.from.pickup_type),
            ),
            to: event(
                &leg.to,
                leg.scheduled_end_time,
                leg.end_time,
                transit && not_allowed(&leg.to.dropoff_type),
            ),
            duration_minutes: minutes(leg.end_time.as_second() - leg.start_time.as_second()),
            realtime,
            cancelled: leg.cancelled == Some(true),
            stay_seated: leg.interline_with_previous_leg == Some(true),
            alerts: alerts([&leg.alerts, &leg.from.alerts, &leg.to.alerts]),
        }
    }
}

/// A connection with its status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Connection {
    /// What the caller has to know first; see the reasons.
    pub status: Status,
    /// Why the status is not `ok`, most important first.
    pub reasons: Vec<Reason>,
    /// Whether the rides have realtime data: all, some or none.
    pub realtime: Realtime,
    pub departure: Time,
    pub arrival: Time,
    pub duration_minutes: i64,
    pub transfers: i64,
    /// The largest delay of a ride, at departure or arrival.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_delay_minutes: Option<i64>,
    pub legs: Vec<Leg>,
}

impl Connection {
    pub(crate) fn new(itinerary: &motis::Itinerary, evaluation: Evaluation) -> Self {
        let legs: Vec<Leg> = itinerary.legs.iter().map(Leg::from).collect();
        let first = itinerary.legs.first();
        let last = itinerary.legs.last();
        // Walks have no realtime data of their own: their times follow the
        // ride they lead to or come from.
        let mut rides = itinerary.legs.iter().filter(|leg| leg.mode.is_transit());
        let departs_realtime = rides.next().is_some_and(|leg| leg.real_time);
        let arrives_realtime = itinerary
            .legs
            .iter()
            .rev()
            .find(|leg| leg.mode.is_transit())
            .is_some_and(|leg| leg.real_time);
        let departure = first.map_or_else(
            || Time::new(itinerary.start_time, itinerary.start_time, false, None),
            |leg| {
                Time::new(
                    leg.scheduled_start_time,
                    leg.start_time,
                    departs_realtime,
                    leg.from.tz.as_deref(),
                )
            },
        );
        let arrival = last.map_or_else(
            || Time::new(itinerary.end_time, itinerary.end_time, false, None),
            |leg| {
                Time::new(
                    leg.scheduled_end_time,
                    leg.end_time,
                    arrives_realtime,
                    leg.to.tz.as_deref(),
                )
            },
        );
        let start = first.map_or(itinerary.start_time, |leg| leg.start_time);
        let end = last.map_or(itinerary.end_time, |leg| leg.end_time);
        Self {
            status: evaluation.status,
            reasons: evaluation.reasons,
            realtime: evaluation.realtime,
            departure,
            arrival,
            duration_minutes: minutes(end.as_second() - start.as_second()),
            transfers: itinerary.transfers,
            max_delay_minutes: evaluation.max_delay_minutes,
            legs,
        }
    }
}

/// Cursors for the next page, to pass as `page_cursor`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PageCursor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earlier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub later: Option<String>,
}

impl PageCursor {
    pub(crate) fn new(earlier: &str, later: &str) -> Self {
        Self {
            earlier: non_empty(Some(earlier)),
            later: non_empty(Some(later)),
        }
    }
}

/// The answer to a connection search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TripResult {
    pub from: Location,
    pub to: Location,
    /// Sorted by departure.
    pub connections: Vec<Connection>,
    pub page_cursor: PageCursor,
    pub attribution: Attribution,
    pub warnings: Vec<String>,
}

/// A departure or arrival at a stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Departure {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headsign: Option<String>,
    /// For `transitous journey`.
    pub trip_id: String,
    /// The stop or platform, with the time of the departure or arrival.
    pub stop: StopEvent,
    pub realtime: bool,
    /// Whether this departure or arrival is cancelled.
    pub cancelled: bool,
    /// Whether the whole trip is cancelled.
    pub trip_cancelled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<Alert>,
}

impl Departure {
    pub(crate) fn new(stop_time: &motis::StopTime, arrivals: bool) -> Self {
        let place = &stop_time.place;
        let (scheduled, actual) = if arrivals {
            (place.scheduled_arrival, place.arrival)
        } else {
            (place.scheduled_departure, place.departure)
        };
        let scheduled = scheduled.or(actual).unwrap_or_default();
        let actual = actual.unwrap_or(scheduled);
        Self {
            mode: stop_time.mode.as_str().to_owned(),
            line: non_empty(Some(&stop_time.display_name)),
            operator: non_empty(Some(&stop_time.agency_name)),
            headsign: non_empty(Some(&stop_time.headsign)),
            trip_id: stop_time.trip_id.clone(),
            stop: StopEvent {
                id: place.stop_id.clone(),
                name: place.name.clone(),
                time: Time::new(scheduled, actual, stop_time.real_time, place.tz.as_deref()),
                track: place.track.clone(),
                scheduled_track: place.scheduled_track.clone(),
                cancelled: stop_time.cancelled,
            },
            realtime: stop_time.real_time,
            cancelled: stop_time.cancelled || stop_time.trip_cancelled,
            trip_cancelled: stop_time.trip_cancelled,
            alerts: alerts([&place.alerts]),
        }
    }
}

/// The departures or arrivals at a stop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DeparturesResult {
    pub stop: Location,
    /// Whether these are arrivals instead of departures.
    pub arrivals: bool,
    pub departures: Vec<Departure>,
    pub page_cursor: PageCursor,
    pub attribution: Attribution,
    pub warnings: Vec<String>,
}

/// A stop of a trip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct JourneyStop {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    /// Absent at the first stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrival: Option<Time>,
    /// Absent at the last stop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departure: Option<Time>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_track: Option<String>,
    /// Whether the trip skips this stop.
    pub cancelled: bool,
}

impl JourneyStop {
    fn new(place: &motis::Place, realtime: bool) -> Self {
        let time = |scheduled: Option<Timestamp>, actual: Option<Timestamp>| {
            let scheduled = scheduled.or(actual)?;
            Some(Time::new(
                scheduled,
                actual.unwrap_or(scheduled),
                realtime,
                place.tz.as_deref(),
            ))
        };
        Self {
            id: place.stop_id.clone(),
            name: place.name.clone(),
            arrival: time(place.scheduled_arrival, place.arrival),
            departure: time(place.scheduled_departure, place.departure),
            track: place.track.clone(),
            scheduled_track: place.scheduled_track.clone(),
            cancelled: place.cancelled == Some(true)
                || (place.pickup_type == Some(motis::PickupDropoffType::NotAllowed)
                    && place.dropoff_type == Some(motis::PickupDropoffType::NotAllowed)),
        }
    }
}

/// One trip with all its stops.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct JourneyResult {
    pub trip_id: String,
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headsign: Option<String>,
    pub realtime: bool,
    /// Whether the trip is cancelled.
    pub cancelled: bool,
    pub stops: Vec<JourneyStop>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<Alert>,
    pub attribution: Attribution,
    pub warnings: Vec<String>,
}

impl JourneyResult {
    pub(crate) fn new(trip_id: &str, itinerary: &motis::Itinerary, warnings: Vec<String>) -> Self {
        let rides: Vec<&motis::Leg> = itinerary
            .legs
            .iter()
            .filter(|leg| leg.mode.is_transit())
            .collect();
        let first = rides.first().copied();
        let realtime = rides.iter().any(|leg| leg.real_time);
        let mut stops: Vec<JourneyStop> = first
            .map(|leg| JourneyStop::new(&leg.from, leg.real_time))
            .into_iter()
            .collect();
        for leg in &rides {
            stops.extend(
                leg.intermediate_stops
                    .iter()
                    .chain([&leg.to])
                    .map(|place| JourneyStop::new(place, leg.real_time)),
            );
        }
        // The first stop has no arrival and the last no departure.
        if let Some(stop) = stops.first_mut() {
            stop.arrival = None;
        }
        if let Some(stop) = stops.last_mut() {
            stop.departure = None;
        }
        Self {
            trip_id: trip_id.to_owned(),
            mode: first.map_or_else(String::new, |leg| leg.mode.as_str().to_owned()),
            line: first.and_then(|leg| non_empty(leg.display_name.as_deref())),
            operator: first.and_then(|leg| non_empty(leg.agency_name.as_deref())),
            headsign: first.and_then(|leg| non_empty(leg.headsign.as_deref())),
            realtime,
            cancelled: rides.iter().any(|leg| leg.cancelled == Some(true)),
            stops,
            alerts: alerts(rides.iter().map(|leg| &leg.alerts)),
            attribution: Attribution::default(),
            warnings,
        }
    }
}

/// A service alert.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Alert {
    pub header: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Like `NO_SERVICE` or `DETOUR`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<String>,
    /// `INFO`, `WARNING` or `SEVERE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// When the alert applies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub periods: Vec<Period>,
}

/// A period of an alert. An absent end means until further notice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Period {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

impl From<&motis::Alert> for Alert {
    fn from(alert: &motis::Alert) -> Self {
        Self {
            header: truncate(&alert.header_text),
            description: truncate(&alert.description_text),
            effect: alert.effect.as_ref().map(|e| e.as_str().to_owned()),
            severity: alert.severity_level.as_ref().map(|s| s.as_str().to_owned()),
            url: alert.url.clone(),
            periods: alert
                .impact_period
                .iter()
                .map(|period| Period {
                    start: period.start.map(|t| format_time(t, None)),
                    end: period.end.map(|t| format_time(t, None)),
                })
                .collect(),
        }
    }
}

/// The alerts of several lists, each once.
fn alerts<'a>(lists: impl IntoIterator<Item = &'a Vec<motis::Alert>>) -> Vec<Alert> {
    let mut out: Vec<Alert> = Vec::new();
    for alert in lists.into_iter().flatten().map(Alert::from) {
        if !out.contains(&alert) {
            out.push(alert);
        }
    }
    out
}

/// The usage of the API in the last 24 hours, by this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UsageSummary {
    /// Requests per endpoint in the last 24 hours.
    pub last_24h: Vec<EndpointUsage>,
    /// Requests in the last minute, all endpoints.
    pub last_minute: u32,
    pub per_minute_limit: u32,
    pub plan_daily_limit: u32,
    pub plan_remaining: u32,
    /// The counter file.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EndpointUsage {
    pub endpoint: String,
    pub requests: u32,
}

/// An error as output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorBody {
    pub error: String,
    /// Like `ambiguous_stop` or `rate_limited`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    /// For `ambiguous_stop`: the stops to choose from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<StopMatch>,
}

/// RFC 3339 in the time zone `tz`, else UTC.
pub(crate) fn format_time(time: Timestamp, tz: Option<&str>) -> String {
    let zone = tz
        .and_then(|name| TimeZone::get(name).ok())
        .unwrap_or(TimeZone::UTC);
    time.to_zoned_datetime(zone)
        .strftime("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

/// Seconds as whole minutes, rounded half away from zero.
pub(crate) fn minutes(seconds: i64) -> i64 {
    let sign = seconds.signum();
    sign * ((seconds.abs() + 30) / 60)
}

pub(crate) fn delay_minutes(scheduled: Timestamp, actual: Timestamp) -> i64 {
    minutes(actual.as_second() - scheduled.as_second())
}

fn non_empty(text: Option<&str>) -> Option<String> {
    text.filter(|text| !text.is_empty()).map(str::to_owned)
}

/// At most [`MAX_ALERT_TEXT`] bytes, cut at a character boundary.
fn truncate(text: &str) -> String {
    if text.len() <= MAX_ALERT_TEXT {
        return text.to_owned();
    }
    let mut end = MAX_ALERT_TEXT - '…'.len_utf8();
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_times_in_the_zone_of_the_stop() {
        let time: Timestamp = "2026-11-26T20:22:00Z".parse().unwrap();
        assert_eq!(
            format_time(time, Some("Europe/Berlin")),
            "2026-11-26T21:22:00+01:00"
        );
        assert_eq!(format_time(time, None), "2026-11-26T20:22:00+00:00");
        assert_eq!(
            format_time(time, Some("Nowhere/Else")),
            "2026-11-26T20:22:00+00:00"
        );
    }

    #[test]
    fn a_stop_without_pickup_and_dropoff_is_skipped() {
        use motis::PickupDropoffType::{Normal, NotAllowed};
        let stop = |pickup, dropoff| motis::Place {
            pickup_type: Some(pickup),
            dropoff_type: Some(dropoff),
            ..motis::Place::default()
        };
        assert!(JourneyStop::new(&stop(NotAllowed, NotAllowed), true).cancelled);
        assert!(!JourneyStop::new(&stop(NotAllowed, Normal), true).cancelled);
        assert!(!JourneyStop::new(&stop(Normal, NotAllowed), true).cancelled);
    }

    #[test]
    fn rounds_to_whole_minutes() {
        assert_eq!(minutes(0), 0);
        assert_eq!(minutes(29), 0);
        assert_eq!(minutes(30), 1);
        assert_eq!(minutes(-90), -2);
        assert_eq!(minutes(-89), -1);
        assert_eq!(minutes(600), 10);
    }

    #[test]
    fn cuts_long_texts() {
        assert_eq!(truncate("short"), "short");
        let long = "ä".repeat(MAX_ALERT_TEXT);
        let cut = truncate(&long);
        assert!(cut.len() <= MAX_ALERT_TEXT, "{}", cut.len());
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn lists_each_alert_once() {
        let alert = motis::Alert {
            header_text: "Works".into(),
            effect: Some(motis::AlertEffect::Detour),
            severity_level: Some(motis::AlertSeverityLevel::Warning),
            impact_period: vec![motis::TimeRange {
                start: Some("2026-11-26T20:00:00Z".parse().unwrap()),
                end: None,
            }],
            ..motis::Alert::default()
        };
        let same = vec![alert.clone()];
        let out = alerts([&same, &same]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].effect.as_deref(), Some("DETOUR"));
        assert_eq!(out[0].severity.as_deref(), Some("WARNING"));
        assert_eq!(
            out[0].periods[0].start.as_deref(),
            Some("2026-11-26T20:00:00+00:00")
        );
        assert_eq!(out[0].periods[0].end, None);
    }

    #[test]
    fn shows_the_default_area() {
        let area_named = |name: &str, default| motis::Area {
            name: name.into(),
            default,
            ..motis::Area::default()
        };
        let mut found = motis::Match {
            areas: vec![
                area_named("Innenstadt", None),
                area_named("Frankfurt", Some(true)),
            ],
            ..motis::Match::default()
        };
        assert_eq!(area(&found).as_deref(), Some("Frankfurt"));
        found.areas[1].default = None;
        assert_eq!(area(&found).as_deref(), Some("Innenstadt"));
        found.areas.clear();
        assert_eq!(area(&found), None);
    }

    #[test]
    fn a_journey_without_rides_is_empty() {
        let result = JourneyResult::new("t", &motis::Itinerary::default(), Vec::new());
        assert_eq!(result.stops, Vec::<JourneyStop>::new());
        assert_eq!(result.mode, "");
        assert!(!result.realtime);
    }

    #[test]
    fn each_end_has_the_realtime_of_its_ride() {
        let time: Timestamp = "2026-11-26T20:00:00Z".parse().unwrap();
        let ride = |real_time| motis::Leg {
            mode: motis::Mode::Bus,
            scheduled_start_time: time,
            start_time: time,
            scheduled_end_time: time,
            end_time: time,
            real_time,
            ..motis::Leg::default()
        };
        let walk = motis::Leg {
            mode: motis::Mode::Walk,
            ..ride(false)
        };
        let delays = |legs: Vec<motis::Leg>| {
            let itinerary = motis::Itinerary {
                legs,
                ..motis::Itinerary::default()
            };
            let evaluation = crate::status::evaluate(&itinerary, time, 2);
            let connection = Connection::new(&itinerary, evaluation);
            (
                connection.departure.delay_minutes,
                connection.arrival.delay_minutes,
            )
        };
        assert_eq!(
            delays(vec![walk.clone(), ride(true), ride(false), walk.clone()]),
            (Some(0), None)
        );
        assert_eq!(delays(vec![ride(false), ride(true)]), (None, Some(0)));
        assert_eq!(delays(vec![walk]), (None, None));
    }

    #[test]
    fn a_connection_without_legs_takes_the_itinerary_times() {
        let itinerary = motis::Itinerary {
            start_time: "2026-11-26T20:00:00Z".parse().unwrap(),
            end_time: "2026-11-26T20:30:00Z".parse().unwrap(),
            ..motis::Itinerary::default()
        };
        let connection = Connection::new(
            &itinerary,
            crate::status::evaluate(&itinerary, itinerary.start_time, 2),
        );
        assert_eq!(connection.duration_minutes, 30);
        assert_eq!(connection.departure.scheduled, "2026-11-26T20:00:00+00:00");
        assert_eq!(connection.arrival.delay_minutes, None);
    }
}

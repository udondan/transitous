//! The requests of the API, one type per endpoint.
//!
//! Fields left at `None` or empty are not sent, so the server uses its
//! default. List parameters are sent comma-separated.

use std::fmt::Display;

use jiff::Timestamp;
use serde::de::DeserializeOwned;

use crate::enums::{Direction, LocationType, Mode, RealtimeMode};
use crate::types::{Itinerary, Match, PlanResponse, StoptimesResponse};

/// A request to one endpoint, sent with [`Client::send`](crate::Client::send).
pub trait Request {
    /// The path below the base URL, without a leading slash.
    const PATH: &'static str;

    /// What the endpoint answers.
    type Response: DeserializeOwned;

    /// The query parameters, in a fixed order.
    fn query(&self) -> Vec<(&'static str, String)>;
}

/// Collects query parameters, skipping the unset ones.
#[derive(Default)]
struct Query(Vec<(&'static str, String)>);

impl Query {
    fn set(mut self, name: &'static str, value: impl Display) -> Self {
        self.0.push((name, value.to_string()));
        self
    }

    fn opt(self, name: &'static str, value: Option<impl Display>) -> Self {
        match value {
            Some(value) => self.set(name, value),
            None => self,
        }
    }

    fn list(self, name: &'static str, values: &[impl Display]) -> Self {
        if values.is_empty() {
            return self;
        }
        let joined = values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.set(name, joined)
    }
}

/// Finds stops, addresses and places by name (`/api/v1/geocode`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Geocode {
    pub text: String,
    /// Preferred languages of the names, like `de`.
    pub language: Vec<String>,
    /// Only matches of these kinds.
    pub types: Vec<LocationType>,
    /// Only stops served by these modes.
    pub modes: Vec<Mode>,
    /// Prefer matches near this position, as `lat,lon`.
    pub place: Option<String>,
    /// How strongly to prefer matches near [`Geocode::place`].
    pub place_bias: Option<f64>,
    pub num_results: Option<u32>,
}

impl Geocode {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }
}

impl Request for Geocode {
    const PATH: &'static str = "api/v1/geocode";
    type Response = Vec<Match>;

    fn query(&self) -> Vec<(&'static str, String)> {
        Query::default()
            .set("text", &self.text)
            .list("language", &self.language)
            .list("type", &self.types)
            .list("mode", &self.modes)
            .opt("place", self.place.as_ref())
            .opt("placeBias", self.place_bias)
            .opt("numResults", self.num_results)
            .0
    }
}

/// Finds stops, addresses and places near a position
/// (`/api/v1/reverse-geocode`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReverseGeocode {
    /// The position, as `lat,lon`.
    pub place: String,
    pub types: Vec<LocationType>,
    pub num_results: Option<u32>,
}

impl ReverseGeocode {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self {
            place: format!("{lat},{lon}"),
            ..Self::default()
        }
    }
}

impl Request for ReverseGeocode {
    const PATH: &'static str = "api/v1/reverse-geocode";
    type Response = Vec<Match>;

    fn query(&self) -> Vec<(&'static str, String)> {
        Query::default()
            .set("place", &self.place)
            .list("type", &self.types)
            .opt("numResults", self.num_results)
            .0
    }
}

/// Plans connections between two places (`/api/v6/plan`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// A stop ID or `lat,lon`.
    pub from_place: String,
    /// A stop ID or `lat,lon`.
    pub to_place: String,
    /// The departure, or the arrival with [`Plan::arrive_by`]. The server
    /// takes the current time if unset.
    pub time: Option<Timestamp>,
    /// Whether [`Plan::time`] is the latest arrival.
    pub arrive_by: Option<bool>,
    /// The minimum number of connections.
    pub num_itineraries: Option<u32>,
    pub max_itineraries: Option<u32>,
    /// A cursor of a previous response, for earlier or later connections.
    pub page_cursor: Option<String>,
    /// In seconds.
    pub search_window: Option<u32>,
    pub timetable_view: Option<bool>,
    pub transit_modes: Vec<Mode>,
    pub max_transfers: Option<u32>,
    pub detailed_transfers: Option<bool>,
    pub detailed_legs: Option<bool>,
    pub join_interlined_legs: Option<bool>,
    pub realtime_mode: Option<RealtimeMode>,
    pub language: Vec<String>,
}

impl Plan {
    pub fn new(from_place: impl Into<String>, to_place: impl Into<String>) -> Self {
        Self {
            from_place: from_place.into(),
            to_place: to_place.into(),
            ..Self::default()
        }
    }
}

impl Request for Plan {
    const PATH: &'static str = "api/v6/plan";
    type Response = PlanResponse;

    fn query(&self) -> Vec<(&'static str, String)> {
        Query::default()
            .set("fromPlace", &self.from_place)
            .set("toPlace", &self.to_place)
            .opt("time", self.time)
            .opt("arriveBy", self.arrive_by)
            .opt("numItineraries", self.num_itineraries)
            .opt("maxItineraries", self.max_itineraries)
            .opt("pageCursor", self.page_cursor.as_ref())
            .opt("searchWindow", self.search_window)
            .opt("timetableView", self.timetable_view)
            .list("transitModes", &self.transit_modes)
            .opt("maxTransfers", self.max_transfers)
            .opt("detailedTransfers", self.detailed_transfers)
            .opt("detailedLegs", self.detailed_legs)
            .opt("joinInterlinedLegs", self.join_interlined_legs)
            .opt("realtimeMode", self.realtime_mode.as_ref())
            .list("language", &self.language)
            .0
    }
}

/// Departures or arrivals at a stop (`/api/v6/stoptimes`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stoptimes {
    pub stop_id: String,
    /// The server takes the current time if unset.
    pub time: Option<Timestamp>,
    /// Arrivals instead of departures.
    pub arrive_by: Option<bool>,
    /// The minimum number of events.
    pub n: Option<u32>,
    pub mode: Vec<Mode>,
    /// The minimum period to search, in seconds.
    pub window: Option<u32>,
    pub page_cursor: Option<String>,
    pub with_alerts: Option<bool>,
    pub realtime_mode: Option<RealtimeMode>,
    pub direction: Option<Direction>,
    pub language: Vec<String>,
}

impl Stoptimes {
    pub fn new(stop_id: impl Into<String>) -> Self {
        Self {
            stop_id: stop_id.into(),
            ..Self::default()
        }
    }
}

impl Request for Stoptimes {
    const PATH: &'static str = "api/v6/stoptimes";
    type Response = StoptimesResponse;

    fn query(&self) -> Vec<(&'static str, String)> {
        Query::default()
            .set("stopId", &self.stop_id)
            .opt("time", self.time)
            .opt("arriveBy", self.arrive_by)
            .opt("n", self.n)
            .list("mode", &self.mode)
            .opt("window", self.window)
            .opt("pageCursor", self.page_cursor.as_ref())
            .opt("withAlerts", self.with_alerts)
            .opt("realtimeMode", self.realtime_mode.as_ref())
            .opt("direction", self.direction.as_ref())
            .list("language", &self.language)
            .0
    }
}

/// One trip with all its stops (`/api/v6/trip`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trip {
    pub trip_id: String,
    pub detailed_legs: Option<bool>,
    pub join_interlined_legs: Option<bool>,
    pub language: Vec<String>,
}

impl Trip {
    pub fn new(trip_id: impl Into<String>) -> Self {
        Self {
            trip_id: trip_id.into(),
            ..Self::default()
        }
    }
}

impl Request for Trip {
    const PATH: &'static str = "api/v6/trip";
    type Response = Itinerary;

    fn query(&self) -> Vec<(&'static str, String)> {
        Query::default()
            .set("tripId", &self.trip_id)
            .opt("detailedLegs", self.detailed_legs)
            .opt("joinInterlinedLegs", self.join_interlined_legs)
            .list("language", &self.language)
            .0
    }
}

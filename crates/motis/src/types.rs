//! The response types of the API.
//!
//! They cover the fields a client of the routing and departure endpoints
//! needs, not every field of the specification. Unknown fields are ignored,
//! and missing fields take their default, so a newer server does not break
//! decoding. Times are UTC [`Timestamp`]s; [`Place::tz`] names the time zone
//! of a stop.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::enums::{
    AlertCause, AlertEffect, AlertSeverityLevel, LocationType, Mode, PickupDropoffType,
    TimestampReason, VertexType,
};

/// A geocoding match: a stop, an address or another place.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Match {
    #[serde(rename = "type")]
    pub kind: LocationType,
    /// For a stop, the stop ID to pass to the other endpoints.
    pub id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub level: Option<f64>,
    pub category: Option<String>,
    pub street: Option<String>,
    pub house_number: Option<String>,
    pub country: Option<String>,
    pub zip: Option<String>,
    pub tz: Option<String>,
    /// The administrative areas the match lies in, most specific first.
    pub areas: Vec<Area>,
    /// How well the match fits the text.
    pub score: f64,
    /// For a stop, the modes that serve it.
    pub modes: Vec<Mode>,
    pub importance: Option<f64>,
}

/// An administrative area of a [`Match`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Area {
    pub name: String,
    /// The OpenStreetMap `admin_level`.
    pub admin_level: f64,
    /// Whether the text matched this area.
    pub matched: bool,
    pub unique: Option<bool>,
    /// Whether this is the area to show by default.
    pub default: Option<bool>,
}

/// A stop or another place of a connection, with its times.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub stop_id: Option<String>,
    /// The station this stop (a platform, say) belongs to.
    pub parent_id: Option<String>,
    pub importance: Option<f64>,
    pub lat: f64,
    pub lon: f64,
    pub level: Option<f64>,
    pub tz: Option<String>,
    /// The arrival, with realtime data if there is any.
    pub arrival: Option<Timestamp>,
    /// The departure, with realtime data if there is any.
    pub departure: Option<Timestamp>,
    pub scheduled_arrival: Option<Timestamp>,
    pub scheduled_departure: Option<Timestamp>,
    pub scheduled_track: Option<String>,
    pub track: Option<String>,
    pub stop_code: Option<String>,
    pub description: Option<String>,
    pub vertex_type: Option<VertexType>,
    pub pickup_type: Option<PickupDropoffType>,
    pub dropoff_type: Option<PickupDropoffType>,
    /// Whether the stop is cancelled for this trip.
    pub cancelled: Option<bool>,
    pub arrival_timestamp_reason: Option<TimestampReason>,
    pub departure_timestamp_reason: Option<TimestampReason>,
    pub alerts: Vec<Alert>,
    pub modes: Vec<Mode>,
}

/// One leg of an [`Itinerary`]: a ride on one trip, or a walk.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Leg {
    pub mode: Mode,
    pub from: Place,
    pub to: Place,
    /// In seconds.
    pub duration: i64,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub scheduled_start_time: Timestamp,
    pub scheduled_end_time: Timestamp,
    /// Whether realtime data exists for this leg.
    pub real_time: bool,
    /// Whether the leg follows the schedule, as opposed to on-demand service.
    pub scheduled: bool,
    /// In meters.
    pub distance: Option<f64>,
    /// Whether passengers stay in the vehicle from the previous leg.
    pub interline_with_previous_leg: Option<bool>,
    pub headsign: Option<String>,
    /// The first stop of the trip.
    pub trip_from: Option<Place>,
    /// The last stop of the trip.
    pub trip_to: Option<Place>,
    pub route_id: Option<String>,
    pub route_url: Option<String>,
    pub direction_id: Option<String>,
    pub route_color: Option<String>,
    pub route_text_color: Option<String>,
    pub route_type: Option<i64>,
    pub agency_name: Option<String>,
    pub agency_url: Option<String>,
    pub agency_id: Option<String>,
    pub trip_id: Option<String>,
    pub route_short_name: Option<String>,
    pub route_long_name: Option<String>,
    pub trip_short_name: Option<String>,
    /// The name to show for the line, like `S9`.
    pub display_name: Option<String>,
    /// Whether the trip of this leg is cancelled.
    pub cancelled: Option<bool>,
    /// The timetable source of the trip.
    pub source: Option<String>,
    /// The stops between `from` and `to`.
    pub intermediate_stops: Vec<Place>,
    pub alerts: Vec<Alert>,
    pub bikes_allowed: Option<bool>,
}

/// A connection from one place to another.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Itinerary {
    /// In seconds.
    pub duration: i64,
    pub start_time: Timestamp,
    pub end_time: Timestamp,
    pub transfers: i64,
    pub id: String,
    pub legs: Vec<Leg>,
}

/// The response of [`Plan`](crate::Plan).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlanResponse {
    pub from: Place,
    pub to: Place,
    /// Connections without transit, like walking the whole way.
    pub direct: Vec<Itinerary>,
    pub itineraries: Vec<Itinerary>,
    /// Pass as [`Plan::page_cursor`](crate::Plan::page_cursor) for earlier
    /// connections.
    pub previous_page_cursor: String,
    /// Pass as [`Plan::page_cursor`](crate::Plan::page_cursor) for later
    /// connections.
    pub next_page_cursor: String,
}

/// A departure or arrival at a stop.
#[allow(clippy::struct_excessive_bools, reason = "the fields of the API")]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StopTime {
    /// The stop, with the times of this event.
    pub place: Place,
    pub mode: Mode,
    pub real_time: bool,
    pub headsign: String,
    pub trip_from: Place,
    pub trip_to: Place,
    pub agency_id: String,
    pub agency_name: String,
    pub agency_url: String,
    pub route_id: String,
    pub route_url: Option<String>,
    pub direction_id: String,
    pub route_color: Option<String>,
    pub route_text_color: Option<String>,
    pub trip_id: String,
    pub route_type: Option<i64>,
    pub route_short_name: String,
    pub route_long_name: String,
    pub trip_short_name: String,
    pub display_name: String,
    pub previous_stops: Vec<Place>,
    pub next_stops: Vec<Place>,
    /// [`PickupDropoffType::NotAllowed`] when the stop is skipped or cancelled.
    pub pickup_dropoff_type: PickupDropoffType,
    /// Whether this event is cancelled.
    pub cancelled: bool,
    /// Whether the whole trip is cancelled.
    pub trip_cancelled: bool,
    pub bikes_allowed: bool,
    pub source: String,
}

/// The response of [`Stoptimes`](crate::Stoptimes).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StoptimesResponse {
    pub stop_times: Vec<StopTime>,
    /// The stop that was asked for.
    pub place: Place,
    pub previous_page_cursor: String,
    pub next_page_cursor: String,
}

/// A service alert (GTFS-RT).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Alert {
    pub code: Option<String>,
    pub communication_period: Vec<TimeRange>,
    pub impact_period: Vec<TimeRange>,
    pub cause: Option<AlertCause>,
    pub cause_detail: Option<String>,
    pub effect: Option<AlertEffect>,
    pub effect_detail: Option<String>,
    pub url: Option<String>,
    pub header_text: String,
    pub description_text: String,
    pub severity_level: Option<AlertSeverityLevel>,
}

/// A period of an [`Alert`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TimeRange {
    pub start: Timestamp,
    pub end: Timestamp,
}

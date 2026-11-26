//! Fake MOTIS 2 server on loopback that serves recorded Transitous responses,
//! for tests.
//!
//! The responses come from `fixtures/`, trimmed answers of the live API for
//! Frankfurt (Main) Hauptbahnhof and Hauptwache. [`Injection`]s change them,
//! to produce cancellations, delays and errors on demand. Every request is
//! recorded, so tests can check what was sent, and that nothing was sent.
//!
//! ```no_run
//! # async fn run() {
//! use transitous_sim::{Injection, Sim};
//!
//! let sim = Sim::start().await;
//! sim.inject(Injection::Cancel { trip_id: transitous_sim::DIRECT_TRIP.into() });
//! // point the client under test at sim.base_url()
//! assert_eq!(sim.requests().len(), 0);
//! # }
//! ```

use std::future::IntoFuture;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use jiff::{SignedDuration, Timestamp};
use serde_json::Value;
use tokio::task::JoinHandle;

/// The trip of the first connection in the plan fixture, also the answer of
/// [`Endpoint::Trip`]: the S4 from Hauptbahnhof tief to Hauptwache.
pub const DIRECT_TRIP: &str = "20261126_20:59_de-DELFI_3490143191";

/// A running departure of the stoptimes fixture: the tram 14.
pub const DEPARTURE_TRIP: &str = "20261126_20:33_de-DELFI_3031893234";

/// The first trip of the connection with a transfer in the plan fixture: the
/// U5 at 21:20 from Hauptbahnhof to Willy-Brandt-Platz.
pub const FEEDER_TRIP: &str = "20261126_21:20_de-DELFI_3031889606";

/// The second trip of that connection: the U1 at 21:25 from
/// Willy-Brandt-Platz to Hauptwache.
pub const CONNECTING_TRIP: &str = "20261126_21:25_de-DELFI_3126800675";

/// The endpoints the sim serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Plan,
    Stoptimes,
    Trip,
    Geocode,
    ReverseGeocode,
}

impl Endpoint {
    const ALL: [Self; 5] = [
        Self::Plan,
        Self::Stoptimes,
        Self::Trip,
        Self::Geocode,
        Self::ReverseGeocode,
    ];

    /// The path of the endpoint.
    pub fn path(self) -> &'static str {
        match self {
            Self::Plan => "/api/v6/plan",
            Self::Stoptimes => "/api/v6/stoptimes",
            Self::Trip => "/api/v6/trip",
            Self::Geocode => "/api/v1/geocode",
            Self::ReverseGeocode => "/api/v1/reverse-geocode",
        }
    }

    fn fixture(self) -> &'static str {
        match self {
            Self::Plan => include_str!("../fixtures/plan.json"),
            Self::Stoptimes => include_str!("../fixtures/stoptimes.json"),
            Self::Trip => include_str!("../fixtures/trip.json"),
            Self::Geocode => include_str!("../fixtures/geocode.json"),
            Self::ReverseGeocode => include_str!("../fixtures/reverse-geocode.json"),
        }
    }
}

/// A change to the answers of the sim. Injections stay until
/// [`Sim::clear`].
#[derive(Debug, Clone, PartialEq)]
pub enum Injection {
    /// Cancels a trip: its legs, their stops and its departures.
    Cancel { trip_id: String },
    /// Cancels one stop of a trip, as for a skipped stop.
    CancelStop { trip_id: String, stop_id: String },
    /// Delays a trip: shifts its realtime times and marks it as realtime.
    Delay { trip_id: String, minutes: i64 },
    /// Removes the realtime data: the times are the schedule.
    NoRealtime,
    /// Answers an endpoint with this status and body.
    Respond {
        endpoint: Endpoint,
        status: u16,
        body: String,
    },
    /// Answers every request with 429 and `Retry-After`.
    RateLimited { retry_after_seconds: u64 },
    /// Waits this long before answering.
    Slow(Duration),
    /// Answers every request with a body that is not JSON.
    Malformed,
}

/// A request the sim received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorded {
    pub path: String,
    /// The query parameters, decoded, in the order sent.
    pub query: Vec<(String, String)>,
    pub user_agent: Option<String>,
}

impl Recorded {
    /// The value of a query parameter.
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Default)]
struct Inner {
    injections: Vec<Injection>,
    requests: Vec<Recorded>,
}

type Shared = Arc<Mutex<Inner>>;

fn lock(shared: &Shared) -> MutexGuard<'_, Inner> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A running sim. It stops when dropped.
#[derive(Debug)]
pub struct Sim {
    base_url: String,
    shared: Shared,
    task: JoinHandle<std::io::Result<()>>,
}

impl Sim {
    /// Starts a sim on a free port of 127.0.0.1, on the current Tokio runtime.
    ///
    /// # Panics
    ///
    /// If no loopback port can be bound.
    pub async fn start() -> Self {
        let shared = Shared::default();
        let app = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&shared));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is free");
        let address = listener.local_addr().expect("the listener has an address");
        let task = tokio::spawn(axum::serve(listener, app).into_future());
        Self {
            base_url: format!("http://{address}"),
            shared,
            task,
        }
    }

    /// The base URL to point a client at.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn inject(&self, injection: Injection) {
        lock(&self.shared).injections.push(injection);
    }

    /// Removes all injections.
    pub fn clear(&self) {
        lock(&self.shared).injections.clear();
    }

    /// The requests received so far.
    pub fn requests(&self) -> Vec<Recorded> {
        lock(&self.shared).requests.clone()
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn handle(State(shared): State<Shared>, request: Request) -> Response {
    let uri = request.uri();
    let recorded = Recorded {
        path: uri.path().to_owned(),
        query: form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes())
            .into_owned()
            .collect(),
        user_agent: request
            .headers()
            .get(header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    };
    let endpoint = Endpoint::ALL
        .into_iter()
        .find(|endpoint| endpoint.path() == recorded.path);
    let injections = {
        let mut inner = lock(&shared);
        inner.requests.push(recorded);
        inner.injections.clone()
    };
    for injection in &injections {
        if let Injection::Slow(delay) = injection {
            tokio::time::sleep(*delay).await;
        }
    }
    let Some(endpoint) = endpoint else {
        return json(StatusCode::NOT_FOUND, r#"{"error":"not found"}"#.to_owned());
    };
    answer(endpoint, &injections)
}

fn answer(endpoint: Endpoint, injections: &[Injection]) -> Response {
    let mut body: Value =
        serde_json::from_str(endpoint.fixture()).expect("the fixtures are valid JSON");
    for injection in injections {
        match injection {
            Injection::Respond {
                endpoint: target,
                status,
                body,
            } if *target == endpoint => {
                let status = StatusCode::from_u16(*status).expect("a valid status code");
                return json(status, body.clone());
            }
            Injection::RateLimited {
                retry_after_seconds,
            } => {
                let mut response = json(
                    StatusCode::TOO_MANY_REQUESTS,
                    r#"{"error":"too many requests"}"#.to_owned(),
                );
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from(*retry_after_seconds));
                return response;
            }
            Injection::Malformed => return json(StatusCode::OK, "<html>".to_owned()),
            other => modify(&mut body, other),
        }
    }
    json(StatusCode::OK, body.to_string())
}

fn json(status: StatusCode, body: String) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// Applies an injection that changes the data.
fn modify(body: &mut Value, injection: &Injection) {
    for_each_trip(body, &mut |trip_id, item| match injection {
        Injection::Cancel { trip_id: id } if id == trip_id => cancel(item),
        Injection::CancelStop {
            trip_id: id,
            stop_id,
        } if id == trip_id => cancel_stop(item, stop_id),
        Injection::Delay {
            trip_id: id,
            minutes,
        } if id == trip_id => delay(item, *minutes),
        Injection::NoRealtime => no_realtime(item),
        _ => {}
    });
}

/// Calls `f` with every leg and stop time that has a trip ID.
fn for_each_trip(value: &mut Value, f: &mut impl FnMut(&str, &mut Value)) {
    match value {
        Value::Object(map) => {
            if let Some(trip_id) = map.get("tripId").and_then(Value::as_str) {
                let trip_id = trip_id.to_owned();
                f(&trip_id, value);
                return;
            }
            for child in map.values_mut() {
                for_each_trip(child, f);
            }
        }
        Value::Array(items) => {
            for item in items {
                for_each_trip(item, f);
            }
        }
        _ => {}
    }
}

/// The stops of a leg (`from`, `to`, `intermediateStops`) or of a stop time
/// (`place`).
fn places(item: &mut Value) -> Vec<&mut Value> {
    let mut places = Vec::new();
    for (key, value) in item.as_object_mut().into_iter().flatten() {
        match (key.as_str(), value) {
            ("from" | "to" | "place", value) => places.push(value),
            ("intermediateStops", Value::Array(stops)) => places.extend(stops.iter_mut()),
            _ => {}
        }
    }
    places
}

fn set_cancelled(place: &mut Value) {
    place["cancelled"] = Value::Bool(true);
    place["pickupType"] = "NOT_ALLOWED".into();
    place["dropoffType"] = "NOT_ALLOWED".into();
}

fn cancel(item: &mut Value) {
    item["cancelled"] = Value::Bool(true);
    if item.get("place").is_some() {
        item["tripCancelled"] = Value::Bool(true);
        item["pickupDropoffType"] = "NOT_ALLOWED".into();
    }
    for place in places(item) {
        set_cancelled(place);
    }
}

fn cancel_stop(item: &mut Value, stop_id: &str) {
    let is_stop_time = item.get("place").is_some();
    let mut hit = false;
    for place in places(item) {
        if place.get("stopId").and_then(Value::as_str) == Some(stop_id) {
            set_cancelled(place);
            hit = true;
        }
    }
    if hit && is_stop_time {
        item["cancelled"] = Value::Bool(true);
        item["pickupDropoffType"] = "NOT_ALLOWED".into();
    }
}

/// Shifts a time field by `minutes`.
fn shift(value: &mut Value, key: &str, minutes: i64) {
    let Some(time) = value.get(key).and_then(Value::as_str) else {
        return;
    };
    let time: Timestamp = time.parse().expect("the fixture times are RFC 3339");
    let shifted = time + SignedDuration::from_mins(minutes);
    value[key] = shifted.to_string().into();
}

fn delay(item: &mut Value, minutes: i64) {
    item["realTime"] = Value::Bool(true);
    shift(item, "startTime", minutes);
    shift(item, "endTime", minutes);
    for place in places(item) {
        shift(place, "arrival", minutes);
        shift(place, "departure", minutes);
        for key in ["arrivalTimestampReason", "departureTimestampReason"] {
            if place.get(key).is_some() {
                place[key] = "PREDICTED".into();
            }
        }
    }
}

/// Copies the scheduled time over the realtime one.
fn reset(value: &mut Value, key: &str, scheduled: &str) {
    if let Some(time) = value.get(scheduled).cloned() {
        value[key] = time;
    }
}

fn no_realtime(item: &mut Value) {
    item["realTime"] = Value::Bool(false);
    reset(item, "startTime", "scheduledStartTime");
    reset(item, "endTime", "scheduledEndTime");
    for place in places(item) {
        reset(place, "arrival", "scheduledArrival");
        reset(place, "departure", "scheduledDeparture");
        for key in ["arrivalTimestampReason", "departureTimestampReason"] {
            if place.get(key).is_some() {
                place[key] = "NO_RT_DATA".into();
            }
        }
    }
}

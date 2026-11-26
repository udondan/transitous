//! The operations against the sim: usage rules, stop resolution, statuses
//! and errors.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use transitous_core::model::TripResult;
use transitous_core::status::{Reason, Side};
use transitous_core::{Config, DeparturesQuery, Error, Status, StopsQuery, Transitous, TripQuery};
use transitous_sim::{
    CONNECTING_TRIP, DIRECT_TRIP, Endpoint, FEEDER_TRIP, Injection, Recorded, Sim,
};

const CONTACT: &str = "https://github.com/udondan/transitous";
const HAUPTBAHNHOF: &str = "at-Railway-Current-Reference-Data-2026_de:06412:10:16:16";
const HAUPTWACHE: &str = "de-DELFI_de:06412:1_G";

fn config(sim: &Sim, state: &Path) -> Config {
    Config {
        contact: Some(CONTACT.to_owned()),
        base_url: sim.base_url().to_owned(),
        timeout: Duration::from_secs(5),
        plan_daily_limit: 720,
        delay_threshold_minutes: 2,
        cache_dir: None,
        state_dir: Some(state.to_owned()),
    }
}

fn at(time: &str) -> Timestamp {
    time.parse().unwrap()
}

/// A client at 21:00 in Frankfurt, before the connections of the fixtures.
fn client(config: &Config) -> Transitous {
    client_at(config, "2026-11-26T20:00:00Z")
}

fn client_at(config: &Config, now: &str) -> Transitous {
    let now = at(now);
    Transitous::new(config, "transitous-test/1.0")
        .unwrap()
        .with_clock(Arc::new(move || now))
}

fn trip_query() -> TripQuery {
    TripQuery {
        from: HAUPTBAHNHOF.into(),
        to: HAUPTWACHE.into(),
        count: 4,
        ..TripQuery::default()
    }
}

fn paths(requests: &[Recorded]) -> Vec<&str> {
    requests.iter().map(|r| r.path.as_str()).collect()
}

async fn trip(sim: &Sim, query: &TripQuery) -> TripResult {
    let state = tempfile::tempdir().unwrap();
    client(&config(sim, state.path()))
        .trip(query)
        .await
        .unwrap()
}

fn statuses(result: &TripResult) -> Vec<Status> {
    result.connections.iter().map(|c| c.status).collect()
}

#[tokio::test]
async fn sends_nothing_without_a_contact() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let mut config = config(&sim, state.path());
    config.contact = None;
    let error = Transitous::new(&config, "t/1").unwrap_err();
    assert_eq!(error.status(), "contact_missing");
    assert_eq!(sim.requests(), []);
}

#[tokio::test]
async fn rejects_an_invalid_base_url() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let mut config = config(&sim, state.path());
    config.base_url = "ftp://example.org".into();
    let error = Transitous::new(&config, "t/1").unwrap_err();
    assert_eq!(error.status(), "config_error");
}

#[tokio::test]
async fn every_request_names_the_contact() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path()));
    client
        .departures(&DeparturesQuery {
            stop: "Frankfurt Hauptbahnhof".into(),
            count: 10,
            ..DeparturesQuery::default()
        })
        .await
        .unwrap();
    let requests = sim.requests();
    assert_eq!(paths(&requests), ["/api/v1/geocode", "/api/v6/stoptimes"]);
    for request in requests {
        assert_eq!(
            request.user_agent.as_deref(),
            Some(
                "transitous-test/1.0 (+https://github.com/udondan/transitous; contact: https://github.com/udondan/transitous)"
            )
        );
    }
}

#[tokio::test]
async fn finds_stops() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let result = client(&config(&sim, state.path()))
        .find_stops(&StopsQuery {
            text: "Frankfurt".into(),
            near: Some((50.1, 8.6)),
            count: 50,
        })
        .await
        .unwrap();
    assert_eq!(result.stops.len(), 4);
    assert_eq!(result.stops[0].id, HAUPTWACHE);
    assert_eq!(result.stops[0].area.as_deref(), Some("Frankfurt am Main"));
    let request = &sim.requests()[0];
    assert_eq!(request.param("type"), Some("STOP"));
    assert_eq!(request.param("place"), Some("50.1,8.6"));
    assert_eq!(request.param("numResults"), Some("20"));
}

#[tokio::test]
async fn an_ambiguous_name_lists_the_candidates() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let error = client(&config(&sim, state.path()))
        .departures(&DeparturesQuery {
            stop: "Hauptwache".into(),
            ..DeparturesQuery::default()
        })
        .await
        .unwrap_err();
    let Error::AmbiguousStop { candidates, .. } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(candidates[0].id, HAUPTWACHE);
    assert_eq!(error.status(), "ambiguous_stop");
    assert_eq!(paths(&sim.requests()), ["/api/v1/geocode"]);
}

#[tokio::test]
async fn an_empty_stop_is_not_found() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let mut warnings = Vec::new();
    let error = client(&config(&sim, state.path()))
        .resolve_stop("  ", &mut warnings)
        .await
        .unwrap_err();
    assert_eq!(error.status(), "stop_not_found");
    assert_eq!(sim.requests(), []);
}

#[tokio::test]
async fn lists_departures_with_cancellations() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let result = client(&config(&sim, state.path()))
        .departures(&DeparturesQuery {
            stop: HAUPTBAHNHOF.into(),
            count: 500,
            ..DeparturesQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(result.stop.name, "Frankfurt (Main) Hauptbahnhof");
    assert_eq!(result.departures.len(), 6);
    let tram = &result.departures[0];
    assert_eq!(tram.line.as_deref(), Some("14"));
    assert_eq!(tram.stop.time.scheduled, "2026-11-26T20:55:00+01:00");
    assert_eq!(tram.stop.time.delay_minutes, Some(0));
    assert_eq!(result.departures[1].stop.time.delay_minutes, Some(1));
    let cancelled: Vec<bool> = result.departures.iter().map(|d| d.cancelled).collect();
    assert_eq!(cancelled, [false, false, false, false, true, true]);
    assert!(result.departures[4].trip_cancelled);
    assert_eq!(result.departures[4].stop.time.delay_minutes, None);
    assert!(result.page_cursor.earlier.is_some());
    let request = &sim.requests()[0];
    assert_eq!(request.param("n"), Some("100"));
    assert_eq!(request.param("withAlerts"), Some("true"));
    assert_eq!(request.param("arriveBy"), Some("false"));
}

#[tokio::test]
async fn lists_arrivals() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let result = client(&config(&sim, state.path()))
        .departures(&DeparturesQuery {
            stop: HAUPTBAHNHOF.into(),
            arrivals: true,
            ..DeparturesQuery::default()
        })
        .await
        .unwrap();
    assert!(result.arrivals);
    assert_eq!(sim.requests()[0].param("arriveBy"), Some("true"));
    assert_eq!(sim.requests()[0].param("n"), Some("1"));
}

#[tokio::test]
async fn connections_on_time_are_ok() {
    let sim = Sim::start().await;
    let result = trip(&sim, &trip_query()).await;
    assert_eq!(
        statuses(&result),
        [Status::Ok, Status::Ok, Status::Ok, Status::Delayed]
    );
    assert_eq!(result.from.name, "Frankfurt (Main) Hauptbahnhof");
    assert_eq!(result.to.name, "Frankfurt (Main) Hauptwache");
    let delayed = &result.connections[3];
    assert_eq!(delayed.max_delay_minutes, Some(21));
    assert_eq!(delayed.arrival.delay_minutes, Some(21));
    assert_eq!(delayed.arrival.scheduled, "2026-11-26T21:32:00+01:00");
    assert_eq!(delayed.arrival.actual, "2026-11-26T21:53:00+01:00");
    let feeder = &result.connections[1];
    assert_eq!(feeder.transfers, 1);
    assert_eq!(feeder.legs.len(), 3);
    assert_eq!(feeder.legs[0].line.as_deref(), Some("U5"));
    assert_eq!(feeder.legs[1].mode, "WALK");
    assert!(result.attribution.text.contains("Transitous"));
    let request = &sim.requests()[0];
    assert_eq!(
        request.param("realtimeMode"),
        Some("REALTIME_ANNOTATION_ONLY")
    );
    assert_eq!(request.param("numItineraries"), Some("4"));
    assert_eq!(request.param("arriveBy"), Some("false"));
}

#[tokio::test]
async fn a_cancelled_trip_cancels_its_connection() {
    let sim = Sim::start().await;
    sim.inject(Injection::Cancel {
        trip_id: DIRECT_TRIP.into(),
    });
    let result = trip(&sim, &trip_query()).await;
    assert_eq!(result.connections[0].status, Status::Cancelled);
    assert_eq!(
        result.connections[0].reasons[0],
        Reason::LegCancelled { leg: 1 }
    );
    assert!(result.connections[0].legs[1].cancelled);
    assert_eq!(result.connections[1].status, Status::Ok);
}

#[tokio::test]
async fn a_cancelled_stop_cancels_its_connection() {
    let sim = Sim::start().await;
    sim.inject(Injection::CancelStop {
        trip_id: CONNECTING_TRIP.into(),
        stop_id: "de-DELFI_de:06412:4:21:1".into(),
    });
    let result = trip(&sim, &trip_query()).await;
    let connection = &result.connections[1];
    assert_eq!(connection.status, Status::Cancelled);
    assert_eq!(
        connection.reasons[0],
        Reason::StopCancelled {
            leg: 2,
            side: Side::Departure
        }
    );
    assert!(connection.legs[2].from.cancelled);
}

#[tokio::test]
async fn a_late_feeder_makes_the_transfer_unreachable() {
    let sim = Sim::start().await;
    sim.inject(Injection::Delay {
        trip_id: FEEDER_TRIP.into(),
        minutes: 5,
    });
    let result = trip(&sim, &trip_query()).await;
    let connection = &result.connections[1];
    assert_eq!(connection.status, Status::Unreachable);
    assert_eq!(
        connection.reasons[0],
        Reason::TransferMissed {
            leg: 2,
            margin_minutes: -1
        }
    );
    assert_eq!(connection.departure.delay_minutes, Some(5));
}

#[tokio::test]
async fn a_departed_connection_is_unreachable() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let result = client_at(&config(&sim, state.path()), "2026-11-26T20:21:00Z")
        .trip(&trip_query())
        .await
        .unwrap();
    assert_eq!(result.connections[1].status, Status::Unreachable);
    assert!(matches!(
        result.connections[1].reasons[0],
        Reason::Departed { minutes_ago: 1, .. }
    ));
    assert_eq!(result.connections[0].status, Status::Ok);
}

#[tokio::test]
async fn without_realtime_data_the_status_says_so() {
    let sim = Sim::start().await;
    sim.inject(Injection::NoRealtime);
    let result = trip(&sim, &trip_query()).await;
    assert_eq!(statuses(&result), [Status::NoRealtime; 4]);
    assert_eq!(result.connections[0].departure.delay_minutes, None);
}

#[tokio::test]
async fn realtime_routing_is_an_option() {
    let sim = Sim::start().await;
    trip(
        &sim,
        &TripQuery {
            realtime_routing: true,
            from: "50.1, 8.6".into(),
            ..trip_query()
        },
    )
    .await;
    let requests = sim.requests();
    assert_eq!(paths(&requests), ["/api/v6/plan"]);
    assert_eq!(requests[0].param("realtimeMode"), Some("REALTIME"));
    assert_eq!(requests[0].param("fromPlace"), Some("50.1,8.6"));
}

#[tokio::test]
async fn an_arrive_by_search_adds_the_page_before() {
    let sim = Sim::start().await;
    let result = trip(
        &sim,
        &TripQuery {
            arrive_by: true,
            time: Some(at("2026-11-26T20:35:00Z")),
            count: 6,
            ..trip_query()
        },
    )
    .await;
    let requests = sim.requests();
    assert_eq!(paths(&requests), ["/api/v6/plan", "/api/v6/plan"]);
    assert_eq!(requests[0].param("arriveBy"), Some("true"));
    assert_eq!(requests[0].param("numItineraries"), Some("6"));
    assert_eq!(requests[0].param("pageCursor"), None);
    assert!(requests[1].param("pageCursor").is_some());
    // The sim answers the same page twice: each connection once, in order.
    let departures: Vec<&str> = result
        .connections
        .iter()
        .map(|c| c.departure.scheduled.as_str())
        .collect();
    let mut sorted = departures.clone();
    sorted.sort_unstable();
    assert_eq!(departures, sorted);
    assert!(result.connections.len() <= 6);
    assert!(result.page_cursor.earlier.is_some());
    assert!(result.page_cursor.later.is_some());
}

#[tokio::test]
async fn an_arrive_by_search_keeps_the_latest_connections() {
    let sim = Sim::start().await;
    let result = trip(
        &sim,
        &TripQuery {
            arrive_by: true,
            count: 2,
            ..trip_query()
        },
    )
    .await;
    // At least three are asked for, and all that come are kept, so the
    // page cursors skip nothing. Enough: no second page.
    assert_eq!(paths(&sim.requests()), ["/api/v6/plan"]);
    assert_eq!(sim.requests()[0].param("numItineraries"), Some("3"));
    assert!(sim.requests()[0].param("time").is_some());
    assert_eq!(result.connections.len(), 4);
}

#[tokio::test]
async fn a_walk_faster_than_any_ride_is_a_connection() {
    let sim = Sim::start().await;
    let walk = r#"{"mode":"WALK","from":{"name":"A","lat":50.1,"lon":8.6,"level":0,"vertexType":"NORMAL"},"to":{"name":"B","lat":50.11,"lon":8.61,"level":0,"vertexType":"NORMAL"},"duration":300,"startTime":"2026-11-26T20:05:00Z","endTime":"2026-11-26T20:10:00Z","scheduledStartTime":"2026-11-26T20:05:00Z","scheduledEndTime":"2026-11-26T20:10:00Z","realTime":false,"scheduled":true,"legGeometry":{"points":"","precision":7,"length":0}}"#;
    let body = format!(
        r#"{{"requestParameters":{{}},"debugOutput":{{}},"from":{{"name":"A","lat":50.1,"lon":8.6,"level":0,"vertexType":"NORMAL"}},"to":{{"name":"B","lat":50.11,"lon":8.61,"level":0,"vertexType":"NORMAL"}},"direct":[{{"duration":300,"startTime":"2026-11-26T20:05:00Z","endTime":"2026-11-26T20:10:00Z","transfers":0,"legs":[{walk}]}}],"itineraries":[],"previousPageCursor":"","nextPageCursor":""}}"#
    );
    sim.inject(Injection::Respond {
        endpoint: Endpoint::Plan,
        status: 200,
        body,
    });
    let result = trip(
        &sim,
        &TripQuery {
            from: "50.1,8.6".into(),
            to: "50.11,8.61".into(),
            ..trip_query()
        },
    )
    .await;
    assert_eq!(result.connections.len(), 1);
    let connection = &result.connections[0];
    assert_eq!(connection.legs[0].mode, "WALK");
    assert_eq!(connection.duration_minutes, 5);
    assert_eq!(connection.status, Status::Ok);
    assert_eq!(connection.departure.delay_minutes, None);
}

#[tokio::test]
async fn a_refused_page_before_is_a_warning() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let mut config = config(&sim, state.path());
    config.plan_daily_limit = 1;
    let result = client(&config)
        .trip(&TripQuery {
            arrive_by: true,
            count: 6,
            ..trip_query()
        })
        .await
        .unwrap();
    assert_eq!(result.connections.len(), 4);
    assert_eq!(result.warnings.len(), 2, "{:?}", result.warnings);
    assert_eq!(result.warnings[0], "1 of 1 plan requests in 24 hours used");
    assert!(
        result.warnings[1].starts_with("no earlier connections: 1 plan requests"),
        "{}",
        result.warnings[1]
    );
    assert_eq!(sim.requests().len(), 1);
}

#[tokio::test]
async fn a_given_page_is_not_extended() {
    let sim = Sim::start().await;
    trip(
        &sim,
        &TripQuery {
            arrive_by: true,
            count: 6,
            page_cursor: Some("cursor".into()),
            ..trip_query()
        },
    )
    .await;
    assert_eq!(sim.requests().len(), 1);
    assert_eq!(sim.requests()[0].param("pageCursor"), Some("cursor"));
}

#[tokio::test]
async fn shows_a_trip_with_its_stops() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let result = client(&config(&sim, state.path()))
        .journey(DIRECT_TRIP)
        .await
        .unwrap();
    assert_eq!(result.line.as_deref(), Some("S4"));
    assert_eq!(result.stops.len(), 16);
    assert_eq!(result.stops[0].name, "Kronberg (Taunus) Bahnhof");
    assert!(result.stops[0].arrival.is_none());
    assert!(result.stops[0].departure.is_some());
    assert!(result.stops[15].departure.is_none());
    assert!(result.stops[15].arrival.is_some());
    assert!(result.realtime);
    assert!(!result.cancelled);
    assert_eq!(sim.requests()[0].param("tripId"), Some(DIRECT_TRIP));
}

#[tokio::test]
async fn a_cancelled_trip_shows_as_cancelled() {
    let sim = Sim::start().await;
    sim.inject(Injection::Cancel {
        trip_id: DIRECT_TRIP.into(),
    });
    let state = tempfile::tempdir().unwrap();
    let result = client(&config(&sim, state.path()))
        .journey(DIRECT_TRIP)
        .await
        .unwrap();
    assert!(result.cancelled);
    assert!(result.stops.iter().all(|stop| stop.cancelled));
}

#[tokio::test]
async fn answers_come_from_the_cache() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut config = config(&sim, state.path());
    config.cache_dir = Some(cache.path().to_owned());
    let client = client(&config);
    let first = client.trip(&trip_query()).await.unwrap();
    let second = client.trip(&trip_query()).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(sim.requests().len(), 1);
    let summary = client.usage_summary().unwrap();
    assert_eq!(summary.last_24h[2].requests, 1);
    assert_eq!(summary.plan_remaining, 719);
    assert!(client.usage().path().starts_with(state.path()));
}

#[tokio::test]
async fn keeps_to_twenty_requests_a_minute() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path()));
    for _ in 0..20 {
        client.journey(DIRECT_TRIP).await.unwrap();
    }
    let error = client.journey(DIRECT_TRIP).await.unwrap_err();
    assert_eq!(error.status(), "rate_limited");
    assert_eq!(error.retry_after_seconds(), Some(60));
    assert_eq!(sim.requests().len(), 20);
}

async fn error_for(sim: &Sim) -> Error {
    let state = tempfile::tempdir().unwrap();
    let mut config = config(sim, state.path());
    config.timeout = Duration::from_millis(300);
    let error = client(&config).journey(DIRECT_TRIP).await.unwrap_err();
    // Sent once: no retry.
    assert_eq!(sim.requests().len(), 1, "{error:?}");
    error
}

#[tokio::test]
async fn reports_errors_of_the_server_without_retrying() {
    for (status, expected) in [
        (500, "unavailable"),
        (503, "unavailable"),
        (400, "bad_request"),
        (422, "bad_request"),
        (404, "not_found"),
    ] {
        let sim = Sim::start().await;
        sim.inject(Injection::Respond {
            endpoint: Endpoint::Trip,
            status,
            body: r#"{"error":"broken"}"#.into(),
        });
        let error = error_for(&sim).await;
        assert_eq!(error.status(), expected, "{status}");
        assert!(error.to_string().contains("broken"), "{error}");
    }
}

#[tokio::test]
async fn reports_rate_limits_of_the_server() {
    let sim = Sim::start().await;
    sim.inject(Injection::RateLimited {
        retry_after_seconds: 42,
    });
    let error = error_for(&sim).await;
    assert_eq!(error.status(), "rate_limited");
    assert_eq!(error.retry_after_seconds(), Some(42));
}

#[tokio::test]
async fn reports_a_timeout() {
    let sim = Sim::start().await;
    sim.inject(Injection::Slow(Duration::from_secs(2)));
    assert_eq!(error_for(&sim).await.status(), "timeout");
}

#[tokio::test]
async fn reports_a_malformed_answer() {
    let sim = Sim::start().await;
    sim.inject(Injection::Malformed);
    assert_eq!(error_for(&sim).await.status(), "upstream_error");
}

#[tokio::test]
async fn reports_an_unreachable_server() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let mut config = config(&sim, state.path());
    drop(sim);
    config.timeout = Duration::from_secs(2);
    let error = client(&config).journey(DIRECT_TRIP).await.unwrap_err();
    assert_eq!(error.status(), "unavailable", "{error:?}");
}

#[tokio::test]
async fn an_unwritable_counter_stops_the_request() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let file = state.path().join("file");
    std::fs::write(&file, "").unwrap();
    let client = client(&config(&sim, &file.join("state")));
    assert!(format!("{client:?}").starts_with("Transitous { client:"));
    let error = client.journey(DIRECT_TRIP).await.unwrap_err();
    assert_eq!(error.status(), "usage_error");
    assert_eq!(sim.requests(), []);
}

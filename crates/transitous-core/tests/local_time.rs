//! Local times mean the time at the stop, like the times of the results,
//! whatever the time zone of the process.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use motis_sim::{Recorded, Sim};
use transitous_core::{Config, DeparturesQuery, QueryTime, Transitous, TripQuery};

const CONTACT: &str = "https://github.com/udondan/transitous";
const HAUPTBAHNHOF: &str = "at-Railway-Current-Reference-Data-2026_de:06412:10:16:16";
const HAUPTWACHE: &str = "de-DELFI_de:06412:1_G";

fn config(sim: &Sim, state: &Path, cache: Option<&Path>) -> Config {
    Config {
        contact: Some(CONTACT.to_owned()),
        base_url: sim.base_url().to_owned(),
        timeout: Duration::from_secs(5),
        plan_daily_limit: 720,
        delay_threshold_minutes: 2,
        cache_dir: cache.map(Path::to_owned),
        state_dir: Some(state.to_owned()),
    }
}

/// A client at 21:00 in Frankfurt that guesses `zone` for stops given by ID.
fn client(config: &Config, zone: &str) -> Transitous {
    let now: Timestamp = "2026-11-26T20:00:00Z".parse().unwrap();
    Transitous::new(config, "transitous-test/1.0")
        .unwrap()
        .with_clock(Arc::new(move || now))
        .with_zone(TimeZone::get(zone).unwrap())
}

fn time(input: &str) -> QueryTime {
    input.parse().unwrap()
}

fn departures(stop: &str, at: &str) -> DeparturesQuery {
    DeparturesQuery {
        stop: stop.into(),
        time: Some(time(at)),
        count: 5,
        ..DeparturesQuery::default()
    }
}

/// The path and `time` of each request.
fn sent(requests: &[Recorded]) -> Vec<(&str, Option<&str>)> {
    requests
        .iter()
        .map(|r| (r.path.as_str(), r.param("time")))
        .collect()
}

#[tokio::test]
async fn reads_a_local_time_at_a_stop_given_by_id_in_its_zone() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "UTC");
    client
        .departures(&departures(HAUPTBAHNHOF, "2026-11-26T21:35"))
        .await
        .unwrap();
    // The answer names Europe/Berlin, so the time is asked once more there.
    assert_eq!(
        sent(&sim.requests()),
        [
            ("/api/v6/stoptimes", Some("2026-11-26T21:35:00Z")),
            ("/api/v6/stoptimes", Some("2026-11-26T20:35:00Z")),
        ]
    );
}

#[tokio::test]
async fn asks_once_when_the_guess_is_right() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "Europe/Berlin");
    client
        .departures(&departures(HAUPTBAHNHOF, "21:35"))
        .await
        .unwrap();
    assert_eq!(
        sent(&sim.requests()),
        [("/api/v6/stoptimes", Some("2026-11-26T20:35:00Z"))]
    );
}

#[tokio::test]
async fn asks_once_in_another_zone_with_the_same_time() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "Europe/Paris");
    client
        .departures(&departures(HAUPTBAHNHOF, "21:35"))
        .await
        .unwrap();
    assert_eq!(sim.requests().len(), 1);
}

#[tokio::test]
async fn takes_the_zone_of_a_stop_given_by_name_from_the_search() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "UTC");
    client
        .departures(&departures("Frankfurt Hauptbahnhof", "21:35"))
        .await
        .unwrap();
    assert_eq!(
        sent(&sim.requests()),
        [
            ("/api/v1/geocode", None),
            ("/api/v6/stoptimes", Some("2026-11-26T20:35:00Z")),
        ]
    );
}

#[tokio::test]
async fn keeps_the_zone_of_a_stop_for_the_next_search() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), Some(cache.path())), "UTC");
    client
        .departures(&departures(HAUPTBAHNHOF, "21:35"))
        .await
        .unwrap();
    // Another count, so the answer is not in the cache, but the zone is.
    client
        .departures(&DeparturesQuery {
            count: 6,
            ..departures(HAUPTBAHNHOF, "21:35")
        })
        .await
        .unwrap();
    assert_eq!(
        sent(&sim.requests()),
        [
            ("/api/v6/stoptimes", Some("2026-11-26T21:35:00Z")),
            ("/api/v6/stoptimes", Some("2026-11-26T20:35:00Z")),
            ("/api/v6/stoptimes", Some("2026-11-26T20:35:00Z")),
        ]
    );
}

#[tokio::test]
async fn keeps_instants_and_later_pages() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "UTC");
    client
        .departures(&departures(HAUPTBAHNHOF, "2026-11-26T21:35:00+01:00"))
        .await
        .unwrap();
    client
        .departures(&DeparturesQuery {
            page_cursor: Some("next".into()),
            ..departures(HAUPTBAHNHOF, "21:35")
        })
        .await
        .unwrap();
    assert_eq!(
        sent(&sim.requests()),
        [
            ("/api/v6/stoptimes", Some("2026-11-26T20:35:00Z")),
            ("/api/v6/stoptimes", Some("2026-11-26T21:35:00Z")),
        ]
    );
}

#[tokio::test]
async fn rejects_a_time_beyond_the_last_one() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "Europe/Berlin");
    let error = client
        .departures(&departures(HAUPTBAHNHOF, "9999-12-31T23:59"))
        .await
        .unwrap_err();
    assert_eq!(error.status(), "bad_request");
    assert_eq!(sim.requests(), []);
}

#[tokio::test]
async fn reads_a_departure_time_at_the_origin() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "UTC");
    client
        .trip(&TripQuery {
            from: HAUPTBAHNHOF.into(),
            to: HAUPTWACHE.into(),
            time: Some(time("21:35")),
            count: 4,
            ..TripQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(
        sent(&sim.requests()),
        [
            ("/api/v6/plan", Some("2026-11-26T21:35:00Z")),
            ("/api/v6/plan", Some("2026-11-26T20:35:00Z")),
        ]
    );
}

#[tokio::test]
async fn reads_an_arrival_time_at_the_destination() {
    let sim = Sim::start().await;
    let state = tempfile::tempdir().unwrap();
    let client = client(&config(&sim, state.path(), None), "Europe/Berlin");
    client
        .trip(&TripQuery {
            from: "50.1,8.6".into(),
            to: HAUPTWACHE.into(),
            time: Some(time("21:35")),
            arrive_by: true,
            count: 3,
            ..TripQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(
        sent(&sim.requests())[0],
        ("/api/v6/plan", Some("2026-11-26T20:35:00Z"))
    );
}

//! Runs the `motis` client against the sim.

use std::time::{Duration, Instant};

use motis_dev::{
    Client, Error, Geocode, PickupDropoffType, Plan, ReverseGeocode, Stoptimes, TimestampReason,
    Trip,
};
use motis_sim::{
    CONNECTING_TRIP, DEPARTURE_TRIP, DIRECT_TRIP, Endpoint, FEEDER_TRIP, Injection, Sim,
};

const AGENT: &str = "motis-sim-test";

fn client(sim: &Sim) -> Client {
    Client::builder(sim.base_url())
        .user_agent(AGENT)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

fn plan() -> Plan {
    Plan::new("de-DELFI_de:06412:10", "de-DELFI_de:06412:1_G")
}

#[tokio::test]
async fn serves_every_endpoint_and_records_the_requests() {
    let sim = Sim::start().await;
    let client = client(&sim);
    let plan = client.send(&plan()).await.unwrap();
    assert_eq!(plan.itineraries.len(), 4);
    let departures = client.send(&Stoptimes::new("stop")).await.unwrap();
    assert_eq!(departures.stop_times.len(), 6);
    let trip = client.send(&Trip::new(DIRECT_TRIP)).await.unwrap();
    assert_eq!(trip.legs[0].trip_id.as_deref(), Some(DIRECT_TRIP));
    let matches = client.send(&Geocode::new("Haupt wache")).await.unwrap();
    assert_eq!(matches.len(), 4);
    let nearby = client.send(&ReverseGeocode::new(50.1, 8.6)).await.unwrap();
    assert_eq!(nearby.len(), 3);

    let requests = sim.requests();
    let paths: Vec<_> = requests.iter().map(|r| r.path.as_str()).collect();
    let expected: Vec<_> = [
        Endpoint::Plan,
        Endpoint::Stoptimes,
        Endpoint::Trip,
        Endpoint::Geocode,
        Endpoint::ReverseGeocode,
    ]
    .map(Endpoint::path)
    .into();
    assert_eq!(paths, expected);
    assert!(
        requests
            .iter()
            .all(|r| r.user_agent.as_deref() == Some(AGENT))
    );
    assert_eq!(requests[3].param("text"), Some("Haupt wache"));
    assert_eq!(requests[3].param("missing"), None);
}

#[tokio::test]
async fn records_a_request_without_user_agent() {
    let sim = Sim::start().await;
    let client = Client::builder(sim.base_url()).build().unwrap();
    client.send(&Trip::new(DIRECT_TRIP)).await.unwrap();
    assert_eq!(sim.requests()[0].user_agent, None);
}

#[tokio::test]
async fn answers_an_unknown_path_with_not_found() {
    let sim = Sim::start().await;
    let error = Client::builder(format!("{}/other", sim.base_url()))
        .build()
        .unwrap()
        .send(&Trip::new(DIRECT_TRIP))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Status { status: 404, message, .. } if message == "not found"),
        "{error:?}"
    );
}

#[tokio::test]
async fn cancels_a_trip_in_plans_and_departures() {
    let sim = Sim::start().await;
    sim.inject(Injection::Cancel {
        trip_id: FEEDER_TRIP.to_owned(),
    });
    sim.inject(Injection::Cancel {
        trip_id: DEPARTURE_TRIP.to_owned(),
    });
    let client = client(&sim);
    let plan = client.send(&plan()).await.unwrap();
    let legs = &plan.itineraries[1].legs;
    let feeder = legs
        .iter()
        .find(|leg| leg.trip_id.as_deref() == Some(FEEDER_TRIP))
        .unwrap();
    assert_eq!(feeder.cancelled, Some(true));
    assert_eq!(feeder.from.cancelled, Some(true));
    assert_eq!(feeder.to.pickup_type, Some(PickupDropoffType::NotAllowed));
    assert_eq!(feeder.to.dropoff_type, Some(PickupDropoffType::NotAllowed));
    let connecting = legs
        .iter()
        .find(|leg| leg.trip_id.as_deref() == Some(CONNECTING_TRIP))
        .unwrap();
    assert_ne!(connecting.cancelled, Some(true));

    let departures = client.send(&Stoptimes::new("stop")).await.unwrap();
    let tram = &departures.stop_times[0];
    assert_eq!(tram.trip_id, DEPARTURE_TRIP);
    assert!(tram.cancelled && tram.trip_cancelled);
    assert_eq!(tram.pickup_dropoff_type, PickupDropoffType::NotAllowed);
    assert_eq!(tram.place.cancelled, Some(true));
    assert_eq!(tram.trip_from.cancelled, Some(true));
    assert_eq!(tram.trip_to.cancelled, Some(true));
}

#[tokio::test]
async fn cancels_one_stop() {
    let sim = Sim::start().await;
    let skipped = "de-DELFI_de:06436:2213";
    sim.inject(Injection::CancelStop {
        trip_id: DIRECT_TRIP.to_owned(),
        stop_id: skipped.to_owned(),
    });
    sim.inject(Injection::CancelStop {
        trip_id: DEPARTURE_TRIP.to_owned(),
        stop_id: "de-DELFI_de:06412:8:2:2".to_owned(),
    });
    // the first stop of the next departure's trip, not the stop it departs from
    let fixture =
        Client::decode::<Stoptimes>(include_bytes!("../fixtures/stoptimes.json")).unwrap();
    let next = &fixture.stop_times[1];
    let origin = next.trip_from.stop_id.clone().unwrap();
    assert_ne!(next.place.stop_id.as_ref(), Some(&origin));
    sim.inject(Injection::CancelStop {
        trip_id: next.trip_id.clone(),
        stop_id: origin,
    });
    let client = client(&sim);
    let trip = client.send(&Trip::new(DIRECT_TRIP)).await.unwrap();
    let leg = &trip.legs[0];
    assert_ne!(leg.cancelled, Some(true));
    let cancelled: Vec<_> = leg
        .intermediate_stops
        .iter()
        .filter(|stop| stop.cancelled == Some(true))
        .map(|stop| stop.stop_id.as_deref())
        .collect();
    assert_eq!(cancelled, [Some(skipped)]);

    let departures = client.send(&Stoptimes::new("stop")).await.unwrap();
    let tram = &departures.stop_times[0];
    assert!(tram.cancelled);
    assert!(!tram.trip_cancelled);
    assert_eq!(tram.pickup_dropoff_type, PickupDropoffType::NotAllowed);
    let next = &departures.stop_times[1];
    assert!(!next.cancelled);
    assert_eq!(next.place.cancelled, fixture.stop_times[1].place.cancelled);
    assert_eq!(next.trip_from.cancelled, Some(true));
}

#[tokio::test]
async fn delays_a_trip() {
    let sim = Sim::start().await;
    sim.inject(Injection::Delay {
        trip_id: DIRECT_TRIP.to_owned(),
        minutes: 7,
    });
    sim.inject(Injection::Delay {
        trip_id: DEPARTURE_TRIP.to_owned(),
        minutes: 3,
    });
    let client = client(&sim);
    let before = Client::decode::<Trip>(include_bytes!("../fixtures/trip.json")).unwrap();
    let trip = client.send(&Trip::new(DIRECT_TRIP)).await.unwrap();
    let (old, new) = (&before.legs[0], &trip.legs[0]);
    let seven = jiff::SignedDuration::from_mins(7);
    assert!(new.real_time);
    assert_eq!(new.start_time, old.start_time + seven);
    assert_eq!(new.end_time, old.end_time + seven);
    assert_eq!(new.scheduled_start_time, old.scheduled_start_time);
    assert_eq!(new.from.departure, old.from.departure.map(|t| t + seven));
    let stop = &new.intermediate_stops[1];
    assert_eq!(
        stop.arrival,
        old.intermediate_stops[1].arrival.map(|t| t + seven)
    );
    assert_eq!(
        stop.arrival_timestamp_reason,
        Some(TimestampReason::Predicted)
    );
    // the itinerary keeps its times, as the trip endpoint has none
    assert_eq!(trip.start_time, before.start_time);

    let departures = client.send(&Stoptimes::new("stop")).await.unwrap();
    let tram = &departures.stop_times[0];
    assert!(tram.real_time);
    let scheduled = tram.place.scheduled_departure.unwrap();
    assert_eq!(
        tram.place.departure,
        Some(scheduled + jiff::SignedDuration::from_mins(3))
    );
}

#[tokio::test]
async fn removes_the_realtime_data() {
    let sim = Sim::start().await;
    sim.inject(Injection::NoRealtime);
    let client = client(&sim);
    let plan = client.send(&plan()).await.unwrap();
    for leg in plan.itineraries.iter().flat_map(|i| &i.legs) {
        if leg.trip_id.is_some() {
            assert!(!leg.real_time);
            assert_eq!(leg.start_time, leg.scheduled_start_time);
            assert_eq!(leg.end_time, leg.scheduled_end_time);
            assert_eq!(leg.from.departure, leg.from.scheduled_departure);
            assert_eq!(leg.to.arrival, leg.to.scheduled_arrival);
            let origin = leg.trip_from.as_ref().unwrap();
            assert_eq!(origin.departure, origin.scheduled_departure);
        }
    }
    let departures = client.send(&Stoptimes::new("stop")).await.unwrap();
    for stop_time in &departures.stop_times {
        assert!(!stop_time.real_time);
        assert_eq!(
            stop_time.place.departure,
            stop_time.place.scheduled_departure
        );
        assert_eq!(
            stop_time.place.departure_timestamp_reason,
            Some(TimestampReason::NoRtData)
        );
        let end = &stop_time.trip_to;
        assert_eq!(end.arrival, end.scheduled_arrival);
    }
}

#[tokio::test]
async fn answers_with_an_injected_status() {
    let sim = Sim::start().await;
    sim.inject(Injection::Respond {
        endpoint: Endpoint::Plan,
        status: 500,
        body: r#"{"error":"internal error"}"#.to_owned(),
    });
    let client = client(&sim);
    let error = client.send(&plan()).await.unwrap_err();
    assert!(
        matches!(&error, Error::Status { status: 500, message, .. } if message == "internal error"),
        "{error:?}"
    );
    // other endpoints are not affected
    client.send(&Trip::new(DIRECT_TRIP)).await.unwrap();
    sim.clear();
    client.send(&plan()).await.unwrap();
}

#[tokio::test]
async fn rate_limits() {
    let sim = Sim::start().await;
    sim.inject(Injection::RateLimited {
        retry_after_seconds: 30,
    });
    let error = client(&sim).send(&plan()).await.unwrap_err();
    assert!(
        matches!(
            error,
            Error::Status {
                status: 429,
                retry_after: Some(retry),
                ..
            } if retry == Duration::from_secs(30)
        ),
        "{error:?}"
    );
    assert_eq!(sim.requests().len(), 1);
}

#[tokio::test]
async fn answers_slowly() {
    let sim = Sim::start().await;
    sim.inject(Injection::Slow(Duration::from_millis(300)));
    let started = Instant::now();
    client(&sim).send(&plan()).await.unwrap();
    assert!(started.elapsed() >= Duration::from_millis(300));
    let error = Client::builder(sim.base_url())
        .timeout(Duration::from_millis(50))
        .build()
        .unwrap()
        .send(&plan())
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Timeout(_)), "{error:?}");
}

#[tokio::test]
async fn answers_with_a_malformed_body() {
    let sim = Sim::start().await;
    sim.inject(Injection::Malformed);
    let error = client(&sim).send(&plan()).await.unwrap_err();
    assert!(matches!(error, Error::Decode(_)), "{error:?}");
}

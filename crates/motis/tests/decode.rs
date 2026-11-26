//! Decodes trimmed responses of the live API (`tests/data`).

use motis::{
    AlertCause, AlertEffect, AlertSeverityLevel, Client, Geocode, LocationType, Mode,
    PickupDropoffType, Plan, ReverseGeocode, Stoptimes, TimestampReason, Trip,
};

fn data(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

#[test]
fn decodes_geocode_matches() {
    let matches = Client::decode::<Geocode>(&data("geocode.json")).unwrap();
    let first = &matches[0];
    assert_eq!(first.kind, LocationType::Stop);
    assert_eq!(first.id, "de-DELFI_de:06412:1_G");
    assert_eq!(first.name, "Frankfurt (Main) Hauptwache");
    assert_eq!(first.areas.len(), 6);
    let reverse = Client::decode::<ReverseGeocode>(&data("reverse-geocode.json")).unwrap();
    assert!(reverse.iter().all(|m| m.kind == LocationType::Stop));
}

#[test]
fn keeps_a_cancelled_leg_with_realtime_annotation_only() {
    let plan = Client::decode::<Plan>(&data("plan-annotation-only.json")).unwrap();
    assert_eq!(plan.itineraries.len(), 3);
    let leg = &plan.itineraries[0].legs[1];
    assert_eq!(leg.mode, Mode::Suburban);
    assert!(leg.mode.is_transit());
    assert!(!plan.itineraries[0].legs[0].mode.is_transit());
    assert_eq!(leg.display_name.as_deref(), Some("S9"));
    assert_eq!(leg.cancelled, Some(true));
    assert_eq!(leg.from.cancelled, Some(true));
    assert_eq!(leg.from.pickup_type, Some(PickupDropoffType::NotAllowed));
    assert!(!leg.real_time);
    let running = &plan.itineraries[1].legs[1];
    assert_eq!(running.cancelled, Some(false));
    assert!(running.real_time);
    assert!(running.start_time >= running.scheduled_start_time);
    assert!(plan.next_page_cursor.len() > 8);
    assert!(plan.previous_page_cursor.len() > 8);
}

#[test]
fn decodes_realtime_and_arrive_by_plans() {
    let plan = Client::decode::<Plan>(&data("plan-realtime.json")).unwrap();
    assert!(
        plan.itineraries
            .iter()
            .flat_map(|i| &i.legs)
            .all(|leg| leg.cancelled != Some(true))
    );
    let plan = Client::decode::<Plan>(&data("plan-arrive-by.json")).unwrap();
    let ends: Vec<_> = plan.itineraries.iter().map(|i| i.end_time).collect();
    assert!(ends.is_sorted());
}

#[test]
fn decodes_cancelled_departures() {
    let response = Client::decode::<Stoptimes>(&data("stoptimes.json")).unwrap();
    assert_eq!(response.place.name, "Frankfurt (Main) Hauptbahnhof");
    let cancelled: Vec<_> = response
        .stop_times
        .iter()
        .filter(|st| st.cancelled)
        .collect();
    assert_eq!(cancelled.len(), 2);
    for st in cancelled {
        assert!(st.trip_cancelled);
        assert_eq!(st.pickup_dropoff_type, PickupDropoffType::NotAllowed);
        assert_eq!(
            st.place.departure_timestamp_reason,
            Some(TimestampReason::NoRtData)
        );
    }
}

#[test]
fn decodes_a_trip() {
    let trip = Client::decode::<Trip>(&data("trip.json")).unwrap();
    assert_eq!(trip.legs.len(), 1);
    assert_eq!(trip.legs[0].display_name.as_deref(), Some("S4"));
    assert_eq!(trip.legs[0].intermediate_stops.len(), 14);
}

#[test]
fn decodes_alerts_and_keeps_unknown_values() {
    let body = br#"{
        "stopTimes": [{
            "mode": "HOVERCRAFT",
            "place": {"name": "A", "lat": 1, "lon": 2, "alerts": [{
                "headerText": "Strike",
                "descriptionText": "No service today",
                "cause": "STRIKE",
                "effect": "NO_SERVICE",
                "severityLevel": "SEVERE",
                "impactPeriod": [{"start": "2026-11-26T05:00:00Z", "end": "2026-11-26T23:00:00Z"}],
                "somethingNew": true
            }]}
        }],
        "nextPageCursor": "n"
    }"#;
    let response = Client::decode::<Stoptimes>(body).unwrap();
    let stop_time = &response.stop_times[0];
    assert_eq!(stop_time.mode, Mode::Other("HOVERCRAFT".to_owned()));
    assert!(stop_time.mode.is_transit());
    assert_eq!(stop_time.mode.to_string(), "HOVERCRAFT");
    let alert = &stop_time.place.alerts[0];
    assert_eq!(alert.header_text, "Strike");
    assert_eq!(alert.cause, Some(AlertCause::Strike));
    assert_eq!(alert.effect, Some(AlertEffect::NoService));
    assert_eq!(alert.severity_level, Some(AlertSeverityLevel::Severe));
    assert_eq!(
        alert.impact_period[0].end.to_string(),
        "2026-11-26T23:00:00Z"
    );
}

#[test]
fn enums_round_trip_through_their_api_spelling() {
    assert_eq!(Mode::default(), Mode::Walk);
    assert_eq!(Mode::from("OTHER".to_owned()), Mode::OtherMode);
    assert_eq!(String::from(Mode::RegionalRail), "REGIONAL_RAIL");
    assert_eq!(String::from(Mode::Other("X".to_owned())), "X");
    assert_eq!(serde_json::to_string(&Mode::Rail).unwrap(), r#""RAIL""#);
}

#[test]
fn rejects_a_malformed_response() {
    let error = Client::decode::<Plan>(b"{\"itineraries\": 3}").unwrap_err();
    assert!(matches!(error, motis::Error::Decode(_)), "{error:?}");
    assert_eq!(error.to_string(), "unexpected response");
}

//! Checks the types against the vendored OpenAPI specification, so a typo or
//! a renamed field shows up here and not as a silently missing value.

use std::collections::BTreeSet;

use motis::{
    Alert, AlertCause, AlertEffect, AlertSeverityLevel, Area, Direction, Geocode, Itinerary, Leg,
    LocationType, Match, Mode, OPENAPI_SPEC, PickupDropoffType, Place, Plan, PlanResponse,
    RealtimeMode, Request, ReverseGeocode, StopTime, Stoptimes, StoptimesResponse, TimeRange,
    Timestamp, TimestampReason, Trip, VertexType,
};
use serde::Serialize;
use serde_json::Value;

/// The specification, read with YAML 1.2 booleans: YAML 1.1 would read the
/// parameter name `n` and the enum value `OFF` as booleans.
fn spec() -> Value {
    let options = serde_saphyr::options! { strict_booleans: true };
    serde_saphyr::from_str_with_options(OPENAPI_SPEC, options)
        .expect("the specification is valid YAML")
}

fn schema<'a>(spec: &'a Value, name: &str) -> &'a Value {
    &spec["components"]["schemas"][name]
}

fn response_schema<'a>(spec: &'a Value, path: &str) -> &'a Value {
    &spec["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
}

/// The serialized field names of `T` that are not properties of `schema`.
fn unknown_fields<T: Default + Serialize>(schema: &Value) -> BTreeSet<String> {
    let value = serde_json::to_value(T::default()).unwrap();
    let properties = schema["properties"]
        .as_object()
        .expect("the schema has properties");
    value
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| !properties.contains_key(*key))
        .cloned()
        .collect()
}

#[test]
fn every_field_exists_in_the_specification() {
    let spec = spec();
    let checks = [
        ("Match", unknown_fields::<Match>(schema(&spec, "Match"))),
        ("Area", unknown_fields::<Area>(schema(&spec, "Area"))),
        ("Place", unknown_fields::<Place>(schema(&spec, "Place"))),
        ("Leg", unknown_fields::<Leg>(schema(&spec, "Leg"))),
        (
            "Itinerary",
            unknown_fields::<Itinerary>(schema(&spec, "Itinerary")),
        ),
        (
            "StopTime",
            unknown_fields::<StopTime>(schema(&spec, "StopTime")),
        ),
        ("Alert", unknown_fields::<Alert>(schema(&spec, "Alert"))),
        (
            "TimeRange",
            unknown_fields::<TimeRange>(schema(&spec, "TimeRange")),
        ),
        (
            "PlanResponse",
            unknown_fields::<PlanResponse>(response_schema(&spec, "/api/v6/plan")),
        ),
        (
            "StoptimesResponse",
            unknown_fields::<StoptimesResponse>(response_schema(&spec, "/api/v6/stoptimes")),
        ),
    ];
    for (name, unknown) in checks {
        assert!(
            unknown.is_empty(),
            "{name} has fields the API lacks: {unknown:?}"
        );
    }
}

#[test]
fn the_response_types_match_the_endpoints() {
    let spec = spec();
    let reference = |path: &str| response_schema(&spec, path).to_string();
    assert!(reference("/api/v6/trip").contains("schemas/Itinerary"));
    assert!(reference("/api/v1/geocode").contains("schemas/Match"));
    assert!(reference("/api/v1/reverse-geocode").contains("schemas/Match"));
}

/// The enum values of a schema.
fn enum_values(schema: &Value) -> BTreeSet<String> {
    schema["enum"]
        .as_array()
        .expect("the schema is an enum")
        .iter()
        .map(|value| value.as_str().expect("the values are strings").to_owned())
        .collect()
}

#[test]
fn every_enum_value_exists_in_the_specification() {
    let spec = spec();
    let checks: [(&str, &[&str]); 9] = [
        ("Mode", Mode::KNOWN),
        ("LocationType", LocationType::KNOWN),
        ("PickupDropoffType", PickupDropoffType::KNOWN),
        ("TimestampReason", TimestampReason::KNOWN),
        ("VertexType", VertexType::KNOWN),
        ("AlertEffect", AlertEffect::KNOWN),
        ("AlertCause", AlertCause::KNOWN),
        ("AlertSeverityLevel", AlertSeverityLevel::KNOWN),
        ("RealtimeMode", RealtimeMode::KNOWN),
    ];
    for (name, known) in checks {
        let values = enum_values(schema(&spec, name));
        let known: BTreeSet<String> = known.iter().map(|&value| value.to_owned()).collect();
        assert_eq!(known, values, "{name}");
    }
    let direction = parameter(&spec, "/api/v6/stoptimes", "direction");
    let known: BTreeSet<String> = Direction::KNOWN.iter().map(|&v| v.to_owned()).collect();
    assert_eq!(known, enum_values(&direction["schema"]));
}

fn parameter<'a>(spec: &'a Value, path: &str, name: &str) -> &'a Value {
    spec["paths"][path]["get"]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parameter| parameter["name"] == name)
        .unwrap_or_else(|| panic!("{path} has no parameter {name}"))
}

/// Checks that the endpoint exists and knows every parameter the request
/// sends, and that the request sends every required parameter.
fn check_request<R: Request>(request: &R) {
    let spec = spec();
    let path = format!("/{}", R::PATH);
    let parameters = spec["paths"][&path]["get"]["parameters"]
        .as_array()
        .unwrap_or_else(|| panic!("{path} is not in the specification"));
    let sent: BTreeSet<&str> = request.query().iter().map(|(name, _)| *name).collect();
    for name in &sent {
        parameter(&spec, &path, name);
    }
    for required in parameters.iter().filter(|p| p["required"] == true) {
        let name = required["name"].as_str().unwrap();
        assert!(sent.contains(name), "{path} needs {name}");
    }
}

#[test]
fn every_parameter_exists_in_the_specification() {
    let time: Timestamp = "2026-11-26T06:30:00Z".parse().unwrap();
    check_request(&Geocode {
        language: vec!["de".to_owned()],
        types: vec![LocationType::Stop],
        modes: vec![Mode::Rail],
        place: Some("50.1,8.6".to_owned()),
        place_bias: Some(2.0),
        num_results: Some(5),
        ..Geocode::new("Hauptwache")
    });
    check_request(&ReverseGeocode {
        types: vec![LocationType::Stop],
        num_results: Some(3),
        ..ReverseGeocode::new(50.1, 8.6)
    });
    check_request(&Plan {
        time: Some(time),
        arrive_by: Some(true),
        num_itineraries: Some(3),
        max_itineraries: Some(6),
        page_cursor: Some("cursor".to_owned()),
        search_window: Some(1800),
        timetable_view: Some(true),
        transit_modes: vec![Mode::Rail, Mode::Bus],
        max_transfers: Some(2),
        detailed_transfers: Some(false),
        detailed_legs: Some(false),
        join_interlined_legs: Some(true),
        realtime_mode: Some(RealtimeMode::RealtimeAnnotationOnly),
        language: vec!["de".to_owned()],
        ..Plan::new("a", "b")
    });
    check_request(&Stoptimes {
        time: Some(time),
        arrive_by: Some(false),
        n: Some(10),
        mode: vec![Mode::Tram],
        window: Some(3600),
        page_cursor: Some("cursor".to_owned()),
        with_alerts: Some(true),
        realtime_mode: Some(RealtimeMode::Realtime),
        direction: Some(Direction::Earlier),
        language: vec!["de".to_owned()],
        ..Stoptimes::new("stop")
    });
    check_request(&Trip {
        detailed_legs: Some(false),
        join_interlined_legs: Some(false),
        language: vec!["de".to_owned()],
        ..Trip::new("trip")
    });
}

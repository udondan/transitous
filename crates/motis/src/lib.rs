//! Client for the [MOTIS 2](https://github.com/motis-project/motis) public
//! transport routing API.
//!
//! The types follow the OpenAPI specification of MOTIS [`MOTIS_VERSION`],
//! vendored unchanged in `openapi/openapi.yaml`. They cover geocoding,
//! connections, departures and trips.
//!
//! ```no_run
//! # async fn run() -> Result<(), motis::Error> {
//! use motis::{Client, Geocode, LocationType};
//!
//! let client = Client::builder("https://motis.example.org")
//!     .user_agent("my-app/1.0 (contact: ops@example.org)")
//!     .build()?;
//! let stops = client
//!     .send(&Geocode {
//!         types: vec![LocationType::Stop],
//!         ..Geocode::new("Hauptwache")
//!     })
//!     .await?;
//! for stop in stops {
//!     println!("{} {}", stop.id, stop.name);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! The client sends every request once. It does not retry, and it does not
//! cache: that is up to the caller.

mod client;
mod enums;
mod error;
mod request;
mod types;

pub use client::{Client, ClientBuilder, DEFAULT_TIMEOUT};
pub use enums::{
    AlertCause, AlertEffect, AlertSeverityLevel, Direction, LocationType, Mode, PickupDropoffType,
    RealtimeMode, TimestampReason, VertexType,
};
pub use error::Error;
pub use jiff::Timestamp;
pub use request::{Geocode, Plan, Request, ReverseGeocode, Stoptimes, Trip};
pub use reqwest::Url;
pub use types::{
    Alert, Area, Itinerary, Leg, Match, Place, PlanResponse, StopTime, StoptimesResponse, TimeRange,
};

/// The MOTIS release whose API this crate implements.
pub const MOTIS_VERSION: &str = "v2.11.4";

/// The version of the API in the vendored specification (`info.version`),
/// also the path prefix of the routing endpoints, like `/api/v6/plan`.
pub const API_VERSION: &str = "v6";

/// The vendored OpenAPI specification.
pub const OPENAPI_SPEC: &str = include_str!("../openapi/openapi.yaml");

#[cfg(test)]
mod tests {
    use super::{API_VERSION, OPENAPI_SPEC};

    #[test]
    fn the_api_version_matches_the_vendored_specification() {
        let info = OPENAPI_SPEC
            .split("\ninfo:\n")
            .nth(1)
            .expect("the specification has an info section");
        let version = info
            .lines()
            .take_while(|line| line.is_empty() || line.starts_with(' '))
            .find_map(|line| line.strip_prefix("  version: "))
            .expect("the info section has a version");
        assert_eq!(version, API_VERSION);
    }
}

//! Client for the [MOTIS 2](https://github.com/motis-project/motis) public
//! transport routing API.
//!
//! The types follow the OpenAPI specification of MOTIS [`MOTIS_VERSION`],
//! vendored unchanged in `openapi/openapi.yaml`.

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

# motis

Client for the [MOTIS 2](https://github.com/motis-project/motis) public transport routing API.

Implements the API of MOTIS v2.11.4 (API version `v6`). The OpenAPI specification of that release is vendored unchanged in [`openapi/openapi.yaml`](https://github.com/udondan/transitous/blob/main/crates/motis/openapi/openapi.yaml), © MOTIS Project, under the [MIT license](https://github.com/udondan/transitous/blob/main/crates/motis/openapi/LICENSE).

## Versioning

`motis` follows [Semantic Versioning](https://semver.org) from 1.0.0: a
breaking change of its public API comes with a new major version only.
Support for a newer MOTIS API version comes with a minor version when it
keeps the public API of the client, and with a major version when it does
not.

Part of [transitous](https://github.com/udondan/transitous).

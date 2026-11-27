# motis-sim

A fake [MOTIS 2](https://github.com/motis-project/motis) server for tests.
`motis-sim` serves recorded responses of the MOTIS API on the loopback
interface, so a client can be tested end to end without a MOTIS instance and
without putting load on a public one like the volunteer-run
[Transitous](https://transitous.org).

- Serves `/api/v6/plan`, `/api/v6/stoptimes`, `/api/v6/trip`,
  `/api/v1/geocode` and `/api/v1/reverse-geocode` (API version `v6` of
  MOTIS v2.11.4). Every request gets the recorded response of its endpoint,
  whatever the query; other paths answer 404
- Injects changes on demand: cancelled trips, a cancelled stop of a trip,
  delays, missing realtime data, any status and body for an endpoint, 429
  with `Retry-After`, slow and malformed responses
- Records every request with its path, decoded query and User-Agent, so a
  test can check what was sent, and that nothing was sent

## Fixtures

The responses are trimmed answers of the Transitous instance of MOTIS for
Frankfurt (Main) on 2026-11-26:

| Endpoint | Recorded for |
| --- | --- |
| `plan` | Hauptbahnhof to Hauptwache, four connections: a direct S4 (`DIRECT_TRIP`), U5 then U1 with a transfer (`FEEDER_TRIP`, `CONNECTING_TRIP`), S5 and S9 |
| `stoptimes` | Six departures at Hauptbahnhof/Münchener Straße, among them the tram 14 (`DEPARTURE_TRIP`) |
| `trip` | The S4 of `DIRECT_TRIP`, Kronberg to Südbahnhof, with its stops |
| `geocode`, `reverse-geocode` | Stops for "Hauptwache", and stops near it |

The constants name trips of the fixtures to aim injections at. The timetable
and realtime data come from the
[sources listed by Transitous](https://transitous.org/sources/), in Germany
from DELFI; place data © [OpenStreetMap](https://www.openstreetmap.org/copyright)
contributors.

## In Rust tests

```toml
[dev-dependencies]
motis-sim = "0.1"
```

```rust,no_run
use motis_sim::{DIRECT_TRIP, Injection, Sim};

#[tokio::test]
async fn sees_the_cancellation() {
    let sim = Sim::start().await;
    sim.inject(Injection::Cancel { trip_id: DIRECT_TRIP.into() });
    // Point the client under test at sim.base_url(), like
    // http://127.0.0.1:50123, and query /api/v6/plan.
    let requests = sim.requests();
    assert!(requests.iter().all(|r| r.user_agent.is_some()));
}
```

Injections stay until `Sim::clear()`. The sim stops when it is dropped.

## In other languages

The binary serves the fixtures until it is interrupted, so tests in any
language can start it and point their client at it:

```sh
cargo install motis-sim
motis-sim --port 8080
```

It prints `listening on http://127.0.0.1:8080` once it accepts requests and
ends with status 0 on Ctrl-C (SIGINT). Without `--port` it picks a free
port. Injections are only available from Rust.

## License

Apache-2.0

Part of [transitous](https://github.com/udondan/transitous).

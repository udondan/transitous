# transitous

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/udondan/transitous/main/docs/banner-dark.webp">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/udondan/transitous/main/docs/banner-light.webp">
  <img alt="transitous – Public transport. CLI + MCP." src="https://raw.githubusercontent.com/udondan/transitous/main/docs/banner-light.webp">
</picture>

[![Release](https://img.shields.io/github/v/release/udondan/transitous?filter=transitous-v*&label=release)](https://github.com/udondan/transitous/releases/latest)
[![Docker](https://img.shields.io/docker/v/udondan/transitous?sort=semver&label=docker)](https://hub.docker.com/r/udondan/transitous)
[![CI](https://github.com/udondan/transitous/actions/workflows/test.yml/badge.svg?branch=main)](https://github.com/udondan/transitous/actions/workflows/test.yml)
[![codecov](https://codecov.io/gh/udondan/transitous/graph/badge.svg)](https://codecov.io/gh/udondan/transitous)
[![Maintainability](https://qlty.sh/gh/udondan/projects/transitous/maintainability.svg)](https://qlty.sh/gh/udondan/projects/transitous)
[![License](https://img.shields.io/github/license/udondan/transitous)](https://github.com/udondan/transitous/blob/main/LICENSE)

CLI and MCP server for public transport connections and departures, using the
[Transitous](https://transitous.org) API (MOTIS 2).

It is built for callers that check fixed daily trips, like a script or an
agent asking every few minutes in the morning whether the usual train still
runs. For each connection it reports scheduled and actual times, delays,
cancellations and alerts, and sums them up in one status:

| Status | Meaning |
| --- | --- |
| `ok` | As planned, as far as the realtime data tells |
| `delayed` | The arrival is late, or the departure is off its schedule |
| `cancelled` | A ride, or boarding or alighting at a stop of it, is cancelled |
| `unreachable` | A transfer can no longer be made, or the first ride has left |
| `no_realtime` | No realtime data: the times are the timetable |

A cancelled connection stays in the result, marked as cancelled, instead of
silently dropping out. `reasons` says why, like
`{"kind": "transfer_missed", "leg": 2, "margin_minutes": -3}`.
[docs/status.md](https://github.com/udondan/transitous/blob/main/docs/status.md)
explains the rules.

## Contents

- [Installation](#installation)
- [Set up](#set-up)
- [Usage](#usage)
- [MCP server](#mcp-server)
- [Configuration](#configuration)
- [Usage rules and limits](#usage-rules-and-limits)
- [Crates](#crates)
- [Data and credits](#data-and-credits)

## Installation

```sh
brew install udondan/software/transitous   # macOS and Linux
cargo install transitous                   # from source
docker pull udondan/transitous             # see Docker below
```

The [releases](https://github.com/udondan/transitous/releases) also have
binaries for Linux (x86_64, aarch64, static), macOS and Windows.
`SHA256SUMS.txt` lists their checksums and is signed with the GPG key
`F111A9BDD62F384A`:

```sh
gpg --verify SHA256SUMS.txt.asc SHA256SUMS.txt
sha256sum --check --ignore-missing SHA256SUMS.txt
```

The Docker image is signed with cosign by the release workflow:

```sh
cosign verify udondan/transitous \
  --certificate-identity-regexp '^https://github.com/udondan/transitous/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

## Set up

Transitous is run by volunteers and asks every client to name a contact in
its User-Agent. Set yours once; without it, `transitous` sends no request at
all and answers `contact_missing`:

```sh
transitous config set contact mailto:you@example.org   # or a URL, or an email address
```

`TRANSITOUS_CONTACT` sets it for a single run instead.

Shell completion, loaded when the shell starts (bash shown; `zsh`, `fish`,
`elvish` and `powershell` work the same way):

```sh
source <(transitous completion bash)
```

## Usage

```console
$ transitous stops "Frankfurt Hauptwache"
Frankfurt (Main) Hauptwache, Frankfurt am Main
  de-DELFI_de:06412:1_G
Frankfurt (Main) Freßgass/Hauptwache, Frankfurt am Main
  de-DELFI_de:06412:60854:1:1
…

$ transitous departures de-DELFI_de:06412:1_G --count 3
Departures at Frankfurt (Main) Hauptwache
23:14     U1 → Frankfurt (Main) Ginnheim Mitte
23:13 +1  U6 → Frankfurt (Main) Ostbahnhof
          ! Frankfurt: U3, U4, U6, U8 - Zusatzfahrten, längere Züge
23:13 +2  S8 → Wiesbaden Hauptbahnhof
Data: Transitous (https://transitous.org) and its sources, …
```

| Command | Does |
| --- | --- |
| `stops <name> [--near LAT,LON]` | Finds stops by name, with their IDs |
| `departures <stop> [--time T] [--count N] [--arrivals] [--modes M]` | The next departures or arrivals at a stop |
| `trip <from> <to> [--arrive T \| --depart T] [--count N] [--modes M] [--max-transfers N] [--realtime-routing]` | Connections, each with its status |
| `journey <trip-id>` | A trip with all its stops, by its ID from `departures` or `trip` |
| `config show\|path\|get\|set\|unset` | The configuration file |
| `usage` | The requests of the last 24 hours and the limits |
| `mcp [--http [ADDR]]` | The MCP server |
| `completion <shell>` | The completion script |

- **Stops** are given by ID, or by a name that fits exactly one stop. A name
  that fits several gives `ambiguous_stop` with the candidates and their IDs,
  never a guess. `trip` also takes coordinates `LAT,LON`.
- **Times** are `HH:MM` (always today, in the local time zone, never rolled
  over to tomorrow), a local time like `2026-11-27T07:50`, or RFC 3339.
- **`trip --arrive 07:50`** returns the connections arriving last before
  07:50, together with earlier ones to switch to.
- **Pages**: every result has a `page_cursor` with `earlier` and `later`;
  pass one with `--page-cursor`.

### Output

`-o text|json|yaml` chooses the format. JSON is the default when stdout is
not a terminal, so scripts get JSON without asking. Errors are printed in the
same format on stderr, with a stable `status`:

| `status` | Meaning |
| --- | --- |
| `contact_missing` | No contact configured; nothing was sent |
| `config_error` | The configuration file, a setting or the contact is invalid |
| `invalid_input` | (MCP) A time, mode or place could not be read; the CLI reports these as usage errors |
| `ambiguous_stop` | The name fits several stops; see `candidates` |
| `stop_not_found` | No stop has that name |
| `not_found` | Transitous does not know the stop or trip ID |
| `rate_limited` | Too many requests in the last minute; see `retry_after_seconds` |
| `daily_limit` | The daily limit of `plan` requests is used up; see `retry_after_seconds` |
| `timeout`, `unavailable`, `bad_request`, `upstream_error` | Transitous did not answer, or refused |
| `usage_error` | The usage counter could not be read or written |

Text output always ends with the attribution of the data, which must be
kept.

## MCP server

`transitous mcp` serves four read-only tools to an MCP client on stdin and
stdout. Each has an output schema and returns a one-line summary followed by
the same JSON as the CLI.

| Tool | Returns |
| --- | --- |
| `transitous_find_stop` | Stops by name, with their IDs |
| `transitous_departures` | The next departures or arrivals at a stop |
| `transitous_trip` | Connections between two places, each with its status and reasons |
| `transitous_journey` | A trip with all its stops |

For example, in Claude Code:

```sh
claude mcp add transitous -e TRANSITOUS_CONTACT=mailto:you@example.org -- transitous mcp
```

Or in the configuration of another client:

```json
{
  "mcpServers": {
    "transitous": {
      "command": "transitous",
      "args": ["mcp"],
      "env": { "TRANSITOUS_CONTACT": "mailto:you@example.org" }
    }
  }
}
```

The server tells the model that stop names and alert texts from the API are
data, not instructions, and asks it to poll rarely.

Agents that use the CLI instead can load the skill in
[skills/transitous/SKILL.md](https://github.com/udondan/transitous/blob/main/skills/transitous/SKILL.md).

### HTTP

`transitous mcp --http` serves streamable HTTP at `http://127.0.0.1:8080/mcp`;
`--http 0.0.0.0:9000` takes another address. It needs a bearer token of at
least 16 characters in `TRANSITOUS_MCP_TOKEN`, which every request must send
as `Authorization: Bearer <token>`:

```sh
export TRANSITOUS_MCP_TOKEN=$(openssl rand -hex 32)
transitous mcp --http
claude mcp add --transport http transitous http://127.0.0.1:8080/mcp \
  --header "Authorization: Bearer $TRANSITOUS_MCP_TOKEN"
```

All sessions share one cache and one usage counter. The server shuts down
cleanly on SIGINT and SIGTERM. It serves plain HTTP; put a TLS proxy in front
of it to reach it over a network.

### Docker

The image runs `transitous`; without a command it starts the MCP server on
stdio:

```sh
docker run --rm -i -e TRANSITOUS_CONTACT=mailto:you@example.org udondan/transitous
```

The cache and the usage counter live in `/home/transitous`; mount a volume
there to keep the counter across containers.

`mise run docker-build` builds the image from source. For the Docker MCP
Toolkit, `mise run docker` builds it and adds
[docker-mcp.yml](https://github.com/udondan/transitous/blob/main/docker-mcp.yml)
to the profile `transitous`. Set the secret `transitous.contact` with
`docker mcp secret set`.

## Configuration

`transitous config path` shows the file, by default
`~/.config/transitous/config.toml` (`%APPDATA%` on Windows).
`transitous config show` shows the settings in effect.

| Setting | Default | Meaning |
| --- | --- | --- |
| `contact` | – | Your contact for the User-Agent: a URL, `mailto:` URL or email address |
| `base_url` | `https://api.transitous.org` | Another MOTIS 2 server |
| `timeout_seconds` | 20 | How long to wait for an answer |
| `plan_daily_limit` | 720 | `plan` requests per 24 hours. Raise it only after agreeing on more with Transitous |
| `delay_threshold_minutes` | 2 | From how many minutes late an arrival counts as `delayed` |

| Variable | Does |
| --- | --- |
| `TRANSITOUS_CONTACT` | Overrides `contact` |
| `TRANSITOUS_BASE_URL` | Overrides `base_url` |
| `TRANSITOUS_CONFIG` | Another configuration file |
| `TRANSITOUS_OUTPUT` | The output format, like `-o` |
| `TRANSITOUS_NO_CACHE` | `1`: no cache, like `--no-cache` |
| `TRANSITOUS_LOG`, `TRANSITOUS_LOG_FILE` | Log level, like `debug` or `motis=debug`, and a log file |
| `TRANSITOUS_MCP_TOKEN` | The bearer token of `mcp --http` |
| `TRANSITOUS_NOW` | Evaluate the status as if it were another time, as RFC 3339 like `2026-11-27T07:50:00+01:00`; `HH:MM` times then count from that day |

## Usage rules and limits

`transitous` follows the [usage rules](https://transitous.org/api/) of
Transitous:

- every request carries the User-Agent
  `transitous/<version> (+https://github.com/udondan/transitous; contact: <your contact>)`;
- nothing is retried. The only automatic follow-up request is one earlier
  page for `trip --arrive` when the first has too few connections;
- answers are cached on disk: stop searches for 15 minutes, departures,
  connections and journeys for 60 seconds;
- at most 20 requests per minute go out, counted across all processes and
  MCP sessions; beyond that the answer is `rate_limited` with
  `retry_after_seconds`;
- `plan` requests (one per `trip`) are counted against `plan_daily_limit`.
  A warning comes from 80 %, and at the limit the answer is `daily_limit`.

`transitous usage` shows the counts. The counter lives in
`~/.local/state/transitous/usage.json`.

## Crates

| Crate | crates.io | docs.rs | Purpose |
| --- | --- | --- | --- |
| [`motis`](https://github.com/udondan/transitous/tree/main/crates/motis) | [![motis on crates.io][motis-crate-badge]][motis-crate] | [![motis on docs.rs][motis-docs-badge]][motis-docs] | Neutral client for the MOTIS 2 API (`v2.11.4`) |
| [`motis-sim`](https://github.com/udondan/transitous/tree/main/crates/motis-sim) | [![motis-sim on crates.io][motis-sim-crate-badge]][motis-sim-crate] | [![motis-sim on docs.rs][motis-sim-docs-badge]][motis-sim-docs] | Fake MOTIS server for tests, with recorded responses and injected cancellations, delays and errors |
| [`transitous-core`](https://github.com/udondan/transitous/tree/main/crates/transitous-core) | [![transitous-core on crates.io][transitous-core-crate-badge]][transitous-core-crate] | [![transitous-core on docs.rs][transitous-core-docs-badge]][transitous-core-docs] | Configuration, usage rules, cache, stop resolution, status and the output model |
| [`transitous`](https://github.com/udondan/transitous/tree/main/crates/transitous) | [![transitous on crates.io][transitous-crate-badge]][transitous-crate] | – | The binary: CLI and `transitous mcp` |

[motis-crate-badge]: https://img.shields.io/crates/v/motis?label=
[motis-crate]: https://crates.io/crates/motis
[motis-docs-badge]: https://img.shields.io/docsrs/motis?label=
[motis-docs]: https://docs.rs/motis
[motis-sim-crate-badge]: https://img.shields.io/crates/v/motis-sim?label=
[motis-sim-crate]: https://crates.io/crates/motis-sim
[motis-sim-docs-badge]: https://img.shields.io/docsrs/motis-sim?label=
[motis-sim-docs]: https://docs.rs/motis-sim
[transitous-core-crate-badge]: https://img.shields.io/crates/v/transitous-core?label=
[transitous-core-crate]: https://crates.io/crates/transitous-core
[transitous-core-docs-badge]: https://img.shields.io/docsrs/transitous-core?label=
[transitous-core-docs]: https://docs.rs/transitous-core
[transitous-crate-badge]: https://img.shields.io/crates/v/transitous?label=
[transitous-crate]: https://crates.io/crates/transitous

## Data and credits

Timetable and realtime data come from the
[sources listed by Transitous](https://transitous.org/sources/). Map data ©
[OpenStreetMap](https://www.openstreetmap.org/copyright) contributors.
Transitous is a community project run by volunteers, without any guarantee;
this project is not affiliated with it.

The feature set builds on ideas from
[Movm/transitous-mcp](https://github.com/Movm/transitous-mcp).

## Security and contributing

See [SECURITY.md](https://github.com/udondan/transitous/blob/main/SECURITY.md)
and [CONTRIBUTING.md](https://github.com/udondan/transitous/blob/main/CONTRIBUTING.md).

## License

[Apache-2.0](https://github.com/udondan/transitous/blob/main/LICENSE)

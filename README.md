# transitous

[![Test](https://github.com/udondan/transitous/actions/workflows/test.yml/badge.svg)](https://github.com/udondan/transitous/actions/workflows/test.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](https://github.com/udondan/transitous/blob/main/LICENSE)

CLI and MCP server for public transport connections and departures, using the
[Transitous](https://transitous.org) API (MOTIS 2).

> **Work in progress.** Nothing is released yet.

Built for callers that check fixed daily trips: it reports scheduled and
actual times, delays, cancellations and alerts, and sums each connection up
in one status (`ok`, `delayed`, `cancelled`, `unreachable` or
`no_realtime`).

## Usage rules

Transitous is a free service run by volunteers. `transitous` follows its
[usage rules](https://transitous.org/api/): every request carries a
User-Agent with a contact that you configure, there are no retry loops, and
requests are cached and counted against the daily limit.

## Data

Timetable data comes from the [sources listed by Transitous](https://transitous.org/sources/).
Map data © [OpenStreetMap](https://www.openstreetmap.org/copyright) contributors.

## Credits

The feature set builds on ideas from
[Movm/transitous-mcp](https://github.com/Movm/transitous-mcp).

## License

[Apache-2.0](https://github.com/udondan/transitous/blob/main/LICENSE)

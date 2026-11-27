# Realtime findings

These are observations of how the public Transitous API (MOTIS v2.11.4, API `v6`) reports realtime data. They come from live calls made on 2026-11-26 at about 20:00 UTC in Frankfurt am Main. They are the basis for the status rules of `transitous`. A longer observation over several days is collected on the branch `claude/observe-cancellations`, and its results will be added here.

## Stop IDs

- `geocode?type=STOP` returns matches with a negative `score`, and the higher score is the better match. For "Frankfurt Hauptwache" the station `de-DELFI_de:06412:1_G` scored -29.7, followed by the bus stop "Freßgass/Hauptwache".
- Stop IDs carry the ID of their feed as a prefix, and stops in the same city can come from different feeds. Frankfurt Hbf, for example, is `at-Railway-Current-Reference-Data-2026_de:06412:10:16:16`.
- An unknown stop ID returns HTTP 404 with the body `{"error":"unknown feed id \"\""}`. This is the same for `plan` and `stoptimes`.

## Cancellations in `stoptimes`

- A cancelled departure has `cancelled: true`, `tripCancelled: true` and `place.pickupType: NOT_ALLOWED`.
- On the same departure, `place.departureTimestampReason` is `NO_RT_DATA`, and `realTime` can be `false`.
- **Consequence:** a cancellation must not be dropped because the entry "has no realtime". `cancelled` takes precedence over `no_realtime`.
- At that time Hauptwache had 101 departures (90 with realtime, 1 cancelled) and Hbf had 200 departures (181 with realtime, 2 cancelled). No alerts were present.

## Cancellations in `plan`

For the same query, from Hbf to Hauptwache with `transitModes=SUBURBAN`, there were two cases:

| `realtimeMode` | Cancelled S9 | Delayed trains |
|---|---|---|
| `REALTIME` (default) | **missing** from the results | offered at their actual time (e.g. S2 scheduled 19:39, actual 20:02) |
| `REALTIME_ANNOTATION_ONLY` | **included**, with `leg.cancelled`, `from.cancelled`, `from.pickupType: NOT_ALLOWED`, `realTime: false` | only offered at their scheduled slot |

What this means:

- With the default mode, a planned connection whose trip is cancelled silently disappears. A caller polling "its" connection would not learn about the cancellation. The `trip` command therefore uses `REALTIME_ANNOTATION_ONLY` by default.
- In `REALTIME_ANNOTATION_ONLY`, `itinerary.startTime`/`endTime` are **schedule-based**. In one case a leg actually departed at 20:24 while the itinerary said 20:06. Actual departure and arrival times of a connection must therefore be taken from its legs.
- `REALTIME_ANNOTATION_ONLY` does not show alternatives that exist only because of delays, such as an earlier train running late. `--realtime-routing` switches to `REALTIME` for those cases.
- Missed transfers are not marked in either mode. They have to be computed from the actual times of consecutive legs.

### Confirmed on 2026-11-27

A local observer (stoptimes at Hauptwache, Hbf, Konstablerwache and Römerstadt every 30 minutes, plus a `plan` comparison per new cancelled trip) confirmed the behaviour. Cancelled S1, U8 and U1 trips showed up in `stoptimes`; the `plan` comparison ran for the S1 from Hauptwache (`20261127_06:43_de-DELFI_3490141369`) and the U1 07:29 from Römerstadt towards Ginnheim (`20261127_07:07_de-DELFI_3126800591`), with the same result for both.

- In `stoptimes` every cancelled entry had `cancelled: true`, `tripCancelled: true`, `realTime: false` and `pickupType`/`dropoffType` `NOT_ALLOWED`.
- `plan` from the stop to the next one, two minutes before the scheduled departure:
  - `REALTIME`: the cancelled trip was missing; for the U1 07:29 the four results were the trains before and after it.
  - `REALTIME_ANNOTATION_ONLY`: the trip was included (for the U1 as the second result), with `cancelled: true`, `from.cancelled: true`, `pickupType`/`dropoffType` `NOT_ALLOWED` and `realTime: false`.
- **Consequence:** `realTime: false` on a cancelled ride does not mean that realtime data is missing; the cancellation is realtime data. The status rules therefore neither list `no_realtime` for a cancelled ride nor count it against `realtime`. The trimmed `REALTIME_ANNOTATION_ONLY` response is the fixture `crates/transitous-core/tests/data/plan-cancelled.json`.
- An app showing Transitous departures at Römerstadt listed the same U1 as "fällt aus", so the data matches what users see.

## `arriveBy`

- `arriveBy=true` with `numItineraries=3` returned 7 itineraries arriving before the deadline, sorted by departure, including ones with transfers.
- `previousPageCursor`/`nextPageCursor` have the form `EARLIER|<unix seconds>` / `LATER|<unix seconds>`.
- `requestParameters` was empty (`{}`). `debugOutput` contains only routing statistics.

## `trip` and `reverse-geocode`

- `trip?tripId=...` returns a single itinerary with one leg, which carries all `intermediateStops`.
- `reverse-geocode?place=<lat>,<lon>&type=STOP` returns the same `Match` objects as `geocode`.

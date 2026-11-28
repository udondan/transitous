# RMV API compared with Transitous

The Rhein-Main-Verkehrsverbund (RMV) offers an OpenData API with realtime data for its area. This document records what that API offers and how its connections, delays, cancellations and disruption messages compare with what Transitous reports for the same trips at the same time.

All observations come from manual calls on Saturday, 2026-11-28, in Frankfurt am Main. RMV and Transitous were asked the same questions within the same second. The calls were made once each, with no loops or retries. The raw captures are not part of the repository.

## The RMV API

- It is a HAFAS ReST API, version 2.39.1 (`serverVersion`), at `https://www.rmv.de/hapi/`. Every request needs an `accessId`, which RMV issues on request. Responses come as XML or, with `format=json`, as JSON. The XSD is at `https://www.rmv.de/hapi/xsd`, the OpenAPI description at `https://www.rmv.de/hapi/api-doc`.
- RMV notes that its documentation is a generic HAFAS one and not every function it describes is enabled. With the issued key, all eight listed services answered:

| Service | Purpose |
| --- | --- |
| `location.name` | Stops by name |
| `location.nearbystops` | Stops by coordinate |
| `departureBoard`, `arrivalBoard` | Departures and arrivals at a stop |
| `trip` | Connection search (93 parameters) |
| `journeyDetail` | One trip with all its stops |
| `himsearch` | Disruption messages (HIM) |
| `recon` | Refreshes a found connection from its `ctxRecon` |

- `https://www.rmv.de/hapi/latest/` serves the newest, possibly future version for compatibility tests. It must not be used in production.
- There is no GTFS export, only a stop list on `https://opendata.rmv.de`.
- The access key is not echoed in any response.
- The [terms of use](https://www.rmv.de/s/de/rmv-open-data/nutzungsbedingung) allow at most 600 requests per hour and 5000 per day per user, and require RMV to be named as the data source.

### Identifiers

- Stops have an RMV number (`extId`, e.g. `3000010` for Frankfurt Hbf) and the German stop ID (DHID) in `altId` (`de:06412:10`). The DHID connects RMV stops to Transitous, whose DELFI stop IDs contain the same DHID (`de-DELFI_de:06412:10_G`).
- A `departureBoard` for a station returns all its parts. For Frankfurt Hbf this includes the S-Bahn station (`Hauptbahnhof tief`, `de:06412:7010`) and some tram stops nearby. Transitous lists them as separate stops, so the comparison asked Transitous for all stops within 250 m and matched departures by DHID station, scheduled minute and line.
- Long-distance trains have their train number as `line` at RMV (`277`) and the line as `routeShortName` at Transitous (`12`). They were matched by station and time.
- Lines carry IDs like `de:rmv:00001315:`, the same scheme as the `rmv:` trip IDs in Transitous.

### Realtime fields

- `time`/`date` is the schedule, `rtTime`/`rtDate` the forecast. `prognosisType` is `PROGNOSED` when a forecast exists and missing otherwise.
- A cancelled departure has `cancelled: true`, `reachable: false`, no `rtTime`, and two notes of type `R`: `text.realtime.journey.cancelled` ("Tram 17: Fällt aus") and `text.realtime.stop.cancelled` ("Halt entfällt").
- `journeyDetail` marks a cancelled trip with `cancelled` at the top level and `cancelledDeparture`/`cancelledArrival` at every stop.
- Departures and legs carry the HIM messages that affect them in `Messages`, with a title (`head`) and an HTML text.
- `trip` returns `ctxRecon` for each connection. `recon` with it returns the same connection with current realtime data, which fits checking a connection someone has already chosen.

## Departures

Departures in the next 60 minutes, captured at 07:45 UTC (08:45 local):

| | Frankfurt Hbf | Hauptwache |
| --- | --- | --- |
| RMV departures | 181 | 89 |
| Matched in Transitous | 181 | 89 |
| With realtime: RMV / Transitous | 158 / 163 | 82 / 88 |
| Same delay to the minute | 140 of 153 | 79 of 82 |
| Delay differs by 1 minute | 12 | 2 |
| Delay differs by 2 minutes | 1 | 1 |
| Cancelled: RMV / Transitous | 3 / 1 | 1 / 0 |
| Departures with a disruption message: RMV / Transitous | 68 / 0 | 48 / 0 |

- Every RMV departure was found in Transitous. Transitous had extra departures only at tram and bus stops that RMV does not include in the station's board.
- Delays agree closely. Where they differ, Transitous is mostly one minute lower.
- Transitous marks more departures as realtime. These are mostly entries whose time it propagates (`PROPAGATED`) while RMV gives no forecast.

## Cancellations

| Departure | RMV | Transitous |
| --- | --- | --- |
| RB82 09:24, Hbf | cancelled | cancelled (`tripCancelled`, `NO_RT_DATA`) |
| Tram 17 09:24, Hbf | whole trip cancelled | runs, realtime with 0 minutes delay |
| U2 08:54, Hauptwache | cancelled, with a message about the shortened line | runs, realtime with 0 minutes delay |
| Tram 21 08:58, Hbf | cancelled at 07:45 UTC, running +8 at 07:48 UTC | runs, +3 at 07:45 UTC, +5 at 07:48 UTC |

- The tram 17 case was checked again 33 minutes later, six minutes before departure. RMV still reported it cancelled at all 18 stops. Transitous still had it running, with a forecast (`PREDICTED`) at the first stop and no cancellation. So this was not a delay in passing the cancellation on: the realtime data Transitous receives for this tram did not contain it.
- The tram 21 case shows that an RMV cancellation can be withdrawn within minutes. A single difference therefore needs a second check before it counts.
- The rail cancellation (RB82) was in both. The missing ones were a tram and an underground line (VGF, the Frankfurt city operator).

## Connections

Frankfurt Hbf to Darmstadt Hbf, five connections each:

- The trains and times are the same: RB68 09:06, RE60 09:34 and ICE 1573 (line 26) 09:42, with the same realtime times.
- RMV also offers the slower direct S-Bahn and marks it (`slowDirect`). Transitous instead offers later trains of the same lines.
- Neither had a cancellation on this route at that time.
- In this case `REALTIME` and `REALTIME_ANNOTATION_ONLY` returned the same connections in Transitous.

## Disruption messages

- `himsearch` returned 297 active messages for the whole RMV area: planned construction work, shortened lines and replacement buses, and realtime notes such as missing platform change information.
- RMV attaches them to the departures they affect. At Hbf 68 of 181 departures had one.
- Transitous returned no alerts for any departure at either station (`withAlerts=true`) and none on the legs of the connections.

## Second snapshot

The same questions in the evening, captured at 16:13 UTC (17:13 local):

| | Frankfurt Hbf | Hauptwache |
| --- | --- | --- |
| RMV departures | 199 | 116 |
| Matched in Transitous | 195 | 115 |
| With realtime: RMV / Transitous | 183 / 186 | 108 / 113 |
| Same delay to the minute | 150 of 181 | 83 of 107 |
| Transitous lower / higher | 29 / 2 | 22 / 2 |
| Cancelled: RMV / Transitous | 1 / 1 | 3 / 1 |
| Departures with a disruption message: RMV / Transitous | 83 / 0 | 66 / 0 |

- The five RMV departures without a match are not gaps in Transitous:
  - S5 17:04 and S3 17:09 were scheduled before the capture and running late, at the start of the window.
  - RMV lists RB37 17:33 twice; Transitous has it once.
  - RB10 is scheduled at 17:23 at RMV and at 17:24 at Transitous, so the match by minute failed.
  - U2 17:11 at Hauptwache was also at the start of the window.
- Delays agree to the minute in 83 % and 78 % of cases. Where they differ, Transitous is lower in 51 of 55 cases, by up to four minutes.
- The S2 cancellation (17:39 at Hbf, 17:43 at Hauptwache) was in both.
- Two U6 runs (17:33 and 17:53 at Hauptwache) were cancelled at RMV, with "U6: Fällt aus" and "Halt entfällt". Transitous had both running, with realtime and 0 minutes delay (`PROPAGATED`).
- The U6 at 17:33 was checked again ten minutes later, at 16:23 UTC. RMV now had it running on time in both directions, without the cancellation notes. Transitous was unchanged. Like tram 21 in the morning, the RMV cancellation was withdrawn within minutes. Whether the U6 actually ran is not known; the U6 at 17:53 was not checked again.
- RB82 17:24 was 25 minutes late at RMV. Transitous marked it `INCONSISTENT_RT` with no forecast. Its `REALTIME` connection search still offered two connections to Darmstadt with this RB82 at its scheduled time. RMV did not offer it.
- The connections to Darmstadt were the same trains otherwise: RE60 17:34, ICE 1671 (line 26) 17:41, RB68 18:06. RMV forecast the RE60 to arrive 3 minutes late, Transitous on time.
- `himsearch` returned 261 active messages. Transitous again returned no alerts.

## Freshness

One delayed trip was followed in both sources: tram 14, due at Hbf at 17:31 local, with `journeyDetail` and `api/v6/trip` at the same second, six rounds two minutes apart (16:13 to 16:23 UTC). The table shows the delay at the last stop.

| Round (local) | RMV `planRtTs` | RMV | Transitous |
| --- | --- | --- | --- |
| 17:13:38 | 17:13:06 | +2 | +1 |
| 17:15:38 | 17:15:06 | +2 | +2 |
| 17:17:39 | 17:17:05 | +3 | +2 |
| 17:19:39 | 17:19:05 | +3 | +2 |
| 17:21:39 | 17:21:06 | +3 | +3 |
| 17:23:40 | 17:23:06 | +3 | +3 |

- Both delay increases reached RMV first and Transitous one or two rounds later: +2 at least 2 minutes later, +3 4 minutes later. Two cases, one trip, so this shows a lag of a few minutes, not how large it usually is.
- Along the route the two disagreed more often than at the end: the share of the 24 stops with the same delay varied between 0 and 23 from round to round.
- RMV states the time of its realtime data in every response (`planRtTs`). In this test it was 32 to 34 seconds old in each round. In the morning, `journeyDetail` answered at 08:47:53 local with data from 08:47:35, 18 seconds old. Transitous gives no such timestamp.

## Assumption: RMV is closer to the source

This section is an assumption, not a measurement.

For its own area, RMV is probably closer to the source of the realtime data than Transitous. The operators of the Verbund report their data to the RMV system, which the HAFAS API serves. Transitous gets German data through DELFI, which collects the data of the regional systems for all of Germany. That is one or two steps more.

What the measurements say about it:

- Delays agree to the minute almost everywhere, so both depend on the same origin.
- Where they differ, Transitous is mostly lower: in the morning mostly by one minute, in the evening in 51 of 55 differences. A delay that grew in the meantime would look like that. The freshness test showed exactly this for one tram, where Transitous caught up with RMV two to four minutes later. That is a hint, not proof.
- Tram and underground cancellations and all disruption messages are missing in Transitous, which fits data getting lost on a longer path.

What speaks against reading too much into it:

- The data path is inferred from how German transit data is distributed, not shown.
- For DB trains the source is DB, not RMV. RMV gets that data passed on as well, and for the RB82 both were equally good.
- Closer to the source also means that short-lived states come through, like the tram 21 that was cancelled for two minutes and the U6 whose cancellation was gone ten minutes later.

What has been measured is that RMV is more complete, and in one followed trip a few minutes ahead. Some of its extra cancellations do not last.

## Summary

- For schedules and delays, RMV and Transitous agree: same departures, same trains, and delays within two minutes, mostly to the minute.
- RMV reports cancellations of VGF trams and underground trains that Transitous does not. Rail cancellations were in both. Two of the RMV-only cancellations were withdrawn within minutes, so a single one needs a second look.
- In the one trip followed, RMV had delay changes two to four minutes before Transitous.
- RMV has disruption messages, Transitous has none for Frankfurt.
- RMV offers `recon` to refresh a chosen connection, which matches the `trip` use case of `transitous`.
- RMV covers only its own area. Transitous covers Germany and beyond.

## Requests used

- First snapshot and its checks: about 19 requests to RMV and 11 to Transitous.
- Second snapshot, freshness test and recheck: 11 requests to RMV and 13 to Transitous.

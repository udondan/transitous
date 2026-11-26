# Connection status

Every connection that `trip` (or the MCP tool `transitous_trip`) returns has
a `status`, a list of `reasons`, a `realtime` level and `max_delay_minutes`.
The status answers one question: can the connection still be taken as
planned? It is computed in `crates/transitous-core/src/status.rs` from the
connection, the current time and the delay threshold; no extra request is
made.

The rules rest on how Transitous reports realtime data, see
[realtime-findings.md](https://github.com/udondan/transitous/blob/main/docs/realtime-findings.md).

## Which connections are checked

By default `trip` asks Transitous with `realtimeMode=REALTIME_ANNOTATION_ONLY`:
the connections are found on the timetable, and realtime data is added to
them. A connection whose train is cancelled stays in the result, marked as
cancelled. With the default mode of the API, it would silently drop out,
and a caller polling "its" connection would not learn about the
cancellation.

`--realtime-routing` (`realtime_routing: true`) routes on the realtime
timetable instead. That finds connections that only exist because of delays,
like an earlier train running late. Cancelled trips and missed transfers then
disappear from the result instead of being marked.

Only rides count, which are legs whose mode is transit. Walks have no realtime
data and cannot be cancelled.

## Statuses

From most to least important; a connection has the most important status
any of its reasons gives:

| Status | Meaning |
| --- | --- |
| `cancelled` | A ride, or boarding or alighting at a stop of it, is cancelled. |
| `unreachable` | A transfer can no longer be made, or the first ride has left already. |
| `delayed` | The arrival is late by at least the threshold, or the departure is off by a minute or more. |
| `no_realtime` | No ride has realtime data; the times are the timetable. |
| `ok` | As planned, as far as the realtime data tells. |

## Reasons

`reasons` lists why, most important first. `leg` is the index in `legs`, walks
included.

| `kind` | Gives | When |
| --- | --- | --- |
| `leg_cancelled` | `cancelled` | The trip of the ride is cancelled (`leg.cancelled`). |
| `stop_cancelled` | `cancelled` | Boarding (`side: departure`) or alighting (`side: arrival`) is not possible: the stop is cancelled, or its pickup or drop-off type is `NOT_ALLOWED`. A passenger who stays seated into the next leg (`interlineWithPreviousLeg`) neither alights nor boards there, so those stops don't count. |
| `transfer_missed` | `unreachable` | The ride leaves before the previous ride arrives plus the scheduled time of the walks in between, both by their actual times. `margin_minutes` is negative. Rides that continue as the same vehicle are no transfer. |
| `departed` | `unreachable` | The first ride left `minutes_ago` minutes ago, by its actual time. |
| `delayed` | `delayed` | The first ride departs `minutes` off its schedule (early is negative, any whole minute counts), or the last ride arrives `minutes` late, at least the threshold. |
| `no_realtime` | `no_realtime` | The ride has no realtime data. It makes the status `no_realtime` only if no ride has any; otherwise it is listed but the status comes from the other reasons. |

Delays in the middle of a connection only matter through the transfers: a
late ride that still makes its transfer changes nothing for the caller.

The threshold is `delay_threshold_minutes` in the configuration, 2 minutes by
default.

## Other fields

- `realtime`: `full` if every ride has realtime data, `partial` if some do,
  `none` if none do.
- `max_delay_minutes`: the largest departure or arrival delay of a ride with
  realtime data, early being negative. Absent without realtime data.
- `departure` and `arrival` of the connection, and of each leg: `scheduled`,
  `actual` (the best known time: with realtime data the expected one, else
  the timetable) and `delay_minutes`. The times of the connection come from
  its first and last leg, because the itinerary times of the API follow the
  timetable in this mode.

## Polling a fixed trip

A caller checking the same connection every few minutes in the morning
would:

1. ask `trip FROM TO --arrive HH:MM` (the latest arrival). The connections
   arriving last before that time come together with the earlier ones;
2. find its usual connection and look at `status`;
3. on `cancelled` or `unreachable`, switch to an earlier connection whose
   status is `ok` or `delayed`, and read `reasons` to explain why.

Results are cached for 60 seconds, so polling more often than once a minute
asks Transitous nothing new. Every `plan` request counts against the daily
limit (720 by default).

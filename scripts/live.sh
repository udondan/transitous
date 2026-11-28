#!/usr/bin/env bash
# Smoke test against the real Transitous API, run by hand with `make live`.
# Sends four requests (geocode, stoptimes, plan, trip), each once, with the
# contact from TRANSITOUS_CONTACT. Never run it in CI, in loops or unattended:
# Transitous is run by volunteers.
set -euo pipefail

if [[ -z ${TRANSITOUS_CONTACT:-} ]]; then
  echo "TRANSITOUS_CONTACT is not set: your contact for the User-Agent" >&2
  exit 1
fi

cargo build -q -p transitous
bin=target/debug/transitous

echo "== stops"
stops=$("$bin" --no-cache -o json stops "Frankfurt Hauptwache")
from=$(jq -er '.stops[0].id' <<<"$stops")
jq -r '.stops[] | "\(.id)  \(.name)"' <<<"$stops"

echo "== departures at $from"
departures=$("$bin" --no-cache -o json departures "$from" --count 5)
trip=$(jq -er '.departures[0].trip_id' <<<"$departures")
jq -r '.departures[] | "\(.stop.scheduled)  \(.line // .mode)  \(.headsign // "")  cancelled=\(.cancelled)"' <<<"$departures"

echo "== trip to Frankfurt Hauptbahnhof"
"$bin" --no-cache -o text trip "$from" "Frankfurt (Main) Hauptbahnhof" --count 2

echo "== journey $trip"
"$bin" --no-cache -o text journey "$trip"

# transitous-sim

Fake MOTIS 2 server built from recorded Transitous responses, for tests.

`Sim::start()` serves `fixtures/` (trimmed answers of the live API) on
127.0.0.1. Injections cancel trips or single stops, add delays, remove the
realtime data, or answer with an error status, 429, a slow or a malformed
response. Every request is recorded with its query and User-Agent.

The tests of the workspace use it, so they never call the volunteer-run
Transitous API. Not published.

Part of [transitous](https://github.com/udondan/transitous).

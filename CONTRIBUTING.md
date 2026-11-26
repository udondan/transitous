# Contributing

## Setup

You need a Rust stable toolchain (rustup recommended) and, for the git hooks,
[qlty](https://docs.qlty.sh/cli/quickstart), `jq`, [mise](https://mise.jdx.dev)
and Docker.

```sh
make hooks   # pre-commit: qlty, rustfmt, workflow lints; pre-push: patch coverage
make lint test
```

## Tests

Tests never call the real Transitous API. They run against `transitous-sim`,
a fake MOTIS server on loopback that serves recorded responses and can inject
cancellations, delays and errors.

Every changed line must be covered by tests, as Codecov checks on pull
requests. `make coverage` checks that locally in a Linux container;
`TRANSITOUS_COVERAGE=native make coverage` uses the local `cargo-llvm-cov`.

## The real API

Transitous is a free service run by volunteers. Keep calls against it to what
you need to check a change by hand, always with your own contact in the
User-Agent, and never in loops or scripts that run unattended.

`make live` checks a build against the real API with four requests (a stop
search, departures, a trip and a journey from Frankfurt Hauptwache), with the
contact from `TRANSITOUS_CONTACT` and without the cache:

```sh
TRANSITOUS_CONTACT=mailto:you@example.org make live
```

It is never run by CI or `make test`.

## Commits

Conventional commits, see [AGENTS.md](AGENTS.md). Pull requests are squash
merged with their title, which therefore must be a conventional commit too.

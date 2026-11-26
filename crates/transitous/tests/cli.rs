//! End-to-end tests of the `transitous` binary against the sim.
//!
//! The tests give every run its own home through `HOME` and the XDG
//! variables. Windows takes the directories from the system instead, so
//! there the tests would share the real ones, and they do not run.
#![cfg(not(windows))]

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;
use transitous_sim::{DIRECT_TRIP, Injection, Sim};

const CONTACT: &str = "https://github.com/udondan/transitous";
const HAUPTBAHNHOF: &str = "at-Railway-Current-Reference-Data-2026_de:06412:10:16:16";
const HAUPTWACHE: &str = "de-DELFI_de:06412:1_G";
/// Before the connections of the fixtures.
const NOW: &str = "2026-11-26T20:00:00Z";

/// A home for one test: the configuration, cache and state live below it.
struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    /// `transitous` without the `TRANSITOUS_*` variables of the caller, with
    /// its files below the home.
    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_transitous"));
        for (name, _) in std::env::vars().filter(|(n, _)| n.starts_with("TRANSITOUS_")) {
            command.env_remove(name);
        }
        let home = self.path();
        command
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("TZ", "Europe/Berlin")
            .env_remove("COMPLETE");
        command
    }

    /// Runs `transitous` against the sim, with a contact.
    async fn run(&self, sim: &Sim, args: &[&str]) -> Output {
        let mut command = self.command();
        command
            .env("TRANSITOUS_CONTACT", CONTACT)
            .env("TRANSITOUS_BASE_URL", sim.base_url())
            .env("TRANSITOUS_NOW", NOW)
            .args(args);
        run(command).await
    }

    async fn json(&self, sim: &Sim, args: &[&str]) -> Value {
        let mut all = vec!["-o", "json"];
        all.extend(args);
        let output = self.run(sim, &all).await;
        assert!(output.status.success(), "{}", stderr(&output));
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

/// Runs the command off the runtime, so that the sim can answer.
async fn run(mut command: Command) -> Output {
    tokio::task::spawn_blocking(move || command.output().expect("run transitous"))
        .await
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[tokio::test]
async fn sends_nothing_without_a_contact() {
    let sim = Sim::start().await;
    let home = Home::new();
    let mut command = home.command();
    command.env("TRANSITOUS_BASE_URL", sim.base_url()).args([
        "-o",
        "json",
        "departures",
        HAUPTWACHE,
    ]);
    let output = run(command).await;
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "");
    let error: Value = serde_json::from_str(&stderr(&output)).unwrap();
    assert_eq!(error["status"], "contact_missing");
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("config set contact")
    );
    assert_eq!(sim.requests(), []);
    // As text, too.
    let mut command = home.command();
    command.args(["-o", "text", "trip", HAUPTWACHE, HAUPTBAHNHOF]);
    let output = run(command).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).starts_with("Error: no contact configured"));
}

#[tokio::test]
async fn finds_stops() {
    let sim = Sim::start().await;
    let home = Home::new();
    let result = home
        .json(&sim, &["stops", "Hauptwache", "--near", "50.11,8.68"])
        .await;
    assert_eq!(result["query"], "Hauptwache");
    assert_eq!(result["stops"][0]["id"], HAUPTWACHE);
    assert!(
        result["attribution"]["text"]
            .as_str()
            .unwrap()
            .contains("Transitous")
    );
    let request = &sim.requests()[0];
    assert_eq!(request.param("place"), Some("50.11,8.68"));
    assert_eq!(
        request.user_agent.as_deref(),
        Some(concat!(
            "transitous/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/udondan/transitous; contact: https://github.com/udondan/transitous)"
        ))
    );
    let text = stdout(&home.run(&sim, &["-o", "text", "stops", "Hauptwache"]).await);
    assert!(text.starts_with("Frankfurt (Main) Hauptwache"), "{text}");
    assert!(text.contains(&format!("  {HAUPTWACHE}\n")), "{text}");
    assert!(text.trim_end().ends_with("without guarantee."), "{text}");
}

#[tokio::test]
async fn lists_departures() {
    let sim = Sim::start().await;
    let home = Home::new();
    let result = home
        .json(
            &sim,
            &[
                "departures",
                HAUPTWACHE,
                "--time",
                "21:00",
                "--modes",
                "subway,bus",
                "--count",
                "3",
            ],
        )
        .await;
    assert_eq!(result["arrivals"], false);
    assert_ne!(result["departures"].as_array().unwrap().len(), 0);
    let request = &sim.requests()[0];
    assert_eq!(request.path, "/api/v6/stoptimes");
    assert_eq!(request.param("time"), Some("2026-11-26T20:00:00Z"));
    assert_eq!(request.param("mode"), Some("SUBWAY,BUS"));
    assert_eq!(request.param("n"), Some("3"));
    let output = home
        .run(
            &sim,
            &["-o", "text", "departures", HAUPTWACHE, "--arrivals"],
        )
        .await;
    let text = stdout(&output);
    assert!(text.starts_with("Arrivals at "), "{text}");
    assert!(text.trim_end().ends_with("without guarantee."), "{text}");
}

#[tokio::test]
async fn an_ambiguous_name_lists_the_candidates() {
    let sim = Sim::start().await;
    let home = Home::new();
    let output = home
        .run(&sim, &["-o", "yaml", "departures", "Hauptwache"])
        .await;
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_saphyr::from_str(&stderr(&output)).unwrap();
    assert_eq!(error["status"], "ambiguous_stop");
    assert!(error["candidates"].as_array().unwrap().len() > 1);
    let output = home
        .run(&sim, &["-o", "text", "departures", "Hauptwache"])
        .await;
    let text = stderr(&output);
    assert!(text.contains("Candidates:\n"), "{text}");
    assert!(text.contains(HAUPTWACHE), "{text}");
}

#[tokio::test]
async fn plans_trips_with_their_status() {
    let sim = Sim::start().await;
    sim.inject(Injection::Cancel {
        trip_id: DIRECT_TRIP.to_owned(),
    });
    let home = Home::new();
    let result = home
        .json(
            &sim,
            &[
                "trip",
                HAUPTBAHNHOF,
                HAUPTWACHE,
                "--arrive",
                "2026-11-26T22:00",
                "--count",
                "4",
                "--max-transfers",
                "2",
            ],
        )
        .await;
    let request = &sim.requests()[0];
    assert_eq!(request.path, "/api/v6/plan");
    assert_eq!(request.param("arriveBy"), Some("true"));
    assert_eq!(request.param("time"), Some("2026-11-26T21:00:00Z"));
    assert_eq!(request.param("maxTransfers"), Some("2"));
    assert_eq!(
        request.param("realtimeMode"),
        Some("REALTIME_ANNOTATION_ONLY")
    );
    let connections = result["connections"].as_array().unwrap();
    let statuses: Vec<&str> = connections
        .iter()
        .map(|c| c["status"].as_str().unwrap())
        .collect();
    assert!(statuses.contains(&"cancelled"), "{statuses:?}");
    // The same result as YAML.
    let output = home
        .run(
            &sim,
            &[
                "-o",
                "yaml",
                "trip",
                HAUPTBAHNHOF,
                HAUPTWACHE,
                "--arrive",
                "2026-11-26T22:00",
                "--count",
                "4",
                "--max-transfers",
                "2",
            ],
        )
        .await;
    let yaml: Value = serde_saphyr::from_str(&stdout(&output)).unwrap();
    assert_eq!(yaml, result);
    let output = home
        .run(
            &sim,
            &[
                "-o",
                "text",
                "trip",
                HAUPTBAHNHOF,
                HAUPTWACHE,
                "--depart",
                "21:00",
                "--realtime-routing",
            ],
        )
        .await;
    let text = stdout(&output);
    assert!(text.starts_with("Frankfurt"), "{text}");
    assert!(text.contains("\n1. "), "{text}");
    assert!(text.trim_end().ends_with("without guarantee."), "{text}");
    let last = sim.requests().pop().unwrap();
    assert_eq!(last.param("realtimeMode"), Some("REALTIME"));
    assert_eq!(last.param("arriveBy"), Some("false"));
}

#[tokio::test]
async fn shows_delays_and_journeys() {
    let sim = Sim::start().await;
    let home = Home::new();
    let last_delay = |result: &Value| {
        let stops = result["stops"].as_array().unwrap();
        assert!(stops.len() > 1);
        stops.last().unwrap()["arrival"]["delay_minutes"]
            .as_i64()
            .unwrap_or(0)
    };
    let before = home
        .json(&sim, &["--no-cache", "journey", DIRECT_TRIP])
        .await;
    sim.inject(Injection::Delay {
        trip_id: DIRECT_TRIP.to_owned(),
        minutes: 4,
    });
    let result = home
        .json(&sim, &["--no-cache", "journey", DIRECT_TRIP])
        .await;
    assert_eq!(result["trip_id"], DIRECT_TRIP);
    let delay = last_delay(&result);
    assert_eq!(delay, last_delay(&before) + 4);
    let output = home
        .run(&sim, &["-o", "text", "journey", DIRECT_TRIP])
        .await;
    let text = stdout(&output);
    assert!(text.contains(&format!(" {delay:+}")), "{text}");
    assert!(text.trim_end().ends_with("without guarantee."), "{text}");
    let output = home
        .run(
            &sim,
            &[
                "-o",
                "text",
                "trip",
                HAUPTBAHNHOF,
                HAUPTWACHE,
                "--count",
                "4",
            ],
        )
        .await;
    assert!(output.status.success(), "{}", stderr(&output));
}

#[tokio::test]
async fn reports_server_errors_without_retrying() {
    let sim = Sim::start().await;
    sim.inject(Injection::RateLimited {
        retry_after_seconds: 42,
    });
    let home = Home::new();
    let output = home
        .run(&sim, &["-o", "json", "journey", DIRECT_TRIP])
        .await;
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_str(&stderr(&output)).unwrap();
    assert_eq!(error["status"], "rate_limited");
    assert_eq!(error["retry_after_seconds"], 42);
    assert_eq!(sim.requests().len(), 1);
}

#[tokio::test]
async fn caches_answers_unless_told_not_to() {
    let sim = Sim::start().await;
    let home = Home::new();
    home.json(&sim, &["journey", DIRECT_TRIP]).await;
    home.json(&sim, &["journey", DIRECT_TRIP]).await;
    assert_eq!(sim.requests().len(), 1);
    home.json(&sim, &["--no-cache", "journey", DIRECT_TRIP])
        .await;
    assert_eq!(sim.requests().len(), 2);
    // The usage counter saw both requests.
    let usage = home.json(&sim, &["usage"]).await;
    let trips = usage["last_24h"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["endpoint"] == "trip")
        .unwrap()
        .clone();
    assert_eq!(trips["requests"], 2);
    let text = stdout(&home.run(&sim, &["-o", "text", "usage"]).await);
    assert!(
        text.contains("Plan requests left today: 720 of 720"),
        "{text}"
    );
}

#[tokio::test]
async fn rejects_an_invalid_clock() {
    let sim = Sim::start().await;
    let home = Home::new();
    let mut command = home.command();
    command
        .env("TRANSITOUS_CONTACT", CONTACT)
        .env("TRANSITOUS_BASE_URL", sim.base_url())
        .env("TRANSITOUS_NOW", "soon")
        .args(["-o", "json", "journey", DIRECT_TRIP]);
    let output = run(command).await;
    let error: Value = serde_json::from_str(&stderr(&output)).unwrap();
    assert_eq!(error["status"], "error");
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .starts_with("invalid TRANSITOUS_NOW")
    );
}

#[tokio::test]
async fn changes_the_configuration() {
    let home = Home::new();
    let config = |args: &[&str]| {
        let mut command = home.command();
        command.arg("config").args(args);
        command.output().unwrap()
    };
    let path = stdout(&config(&["path"]));
    assert_eq!(
        path.trim_end(),
        home.path()
            .join("config/transitous/config.toml")
            .display()
            .to_string()
    );
    assert_eq!(stdout(&config(&["get", "contact"])), "");
    let output = config(&["set", "contact", "mailto:someone@example.org"]);
    assert!(
        stdout(&output).starts_with("set contact in "),
        "{}",
        stderr(&output)
    );
    assert_eq!(
        stdout(&config(&["get", "contact"])),
        "mailto:someone@example.org\n"
    );
    let output = config(&["set", "plan_daily_limit", "many"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("plan_daily_limit"),
        "{}",
        stderr(&output)
    );
    let shown: Value = serde_json::from_slice(&config(&["-o", "json", "show"]).stdout).unwrap();
    assert_eq!(shown["contact"], "mailto:someone@example.org");
    assert_eq!(shown["plan_daily_limit"], 720);
    let text = stdout(&config(&["-o", "text", "--no-cache", "show"]));
    assert!(
        text.contains("contact                  mailto:someone@example.org\n"),
        "{text}"
    );
    assert!(text.contains("cache_dir                (off)\n"), "{text}");
    assert!(stdout(&config(&["unset", "contact"])).starts_with("unset contact in "));
    let text = stdout(&config(&["-o", "text", "show"]));
    assert!(
        text.contains("contact                  (unset)\n"),
        "{text}"
    );
    // An unknown key is refused by the command line.
    assert_eq!(config(&["get", "colour"]).status.code(), Some(2));
}

#[test]
fn a_broken_configuration_is_reported() {
    let home = Home::new();
    let file = home.path().join("broken.toml");
    std::fs::write(&file, "contact = [").unwrap();
    let mut command = home.command();
    command
        .env("TRANSITOUS_CONFIG", &file)
        .args(["-o", "text", "usage"]);
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("is not a valid configuration"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn prints_completion_scripts() {
    let home = Home::new();
    let output = home
        .command()
        .args(["completion", "/bin/zsh"])
        .output()
        .unwrap();
    assert!(stdout(&output).contains("_transitous"));
    let mut command = home.command();
    command
        .env("COMPLETE", "fish")
        .args(["--", "transitous", "dep"]);
    let output = command.output().unwrap();
    assert!(
        stdout(&output).contains("departures"),
        "{}",
        stdout(&output)
    );
}

#[tokio::test]
async fn logs_the_requests_to_a_file() {
    let sim = Sim::start().await;
    let home = Home::new();
    let log = home.path().join("transitous.log");
    // Without TRANSITOUS_NOW: the real clock.
    let mut command = home.command();
    command
        .env("TRANSITOUS_CONTACT", CONTACT)
        .env("TRANSITOUS_BASE_URL", sim.base_url())
        .env("TRANSITOUS_LOG_FILE", &log)
        .args(["-vv", "-o", "json", "stops", "Hauptwache"]);
    let output = run(command).await;
    assert!(output.status.success(), "{}", stderr(&output));
    assert_ne!(stderr(&output), "");
    let logged = std::fs::read_to_string(&log).unwrap();
    assert!(logged.contains("geocode"), "{logged}");
    // A directory is no log file.
    let mut command = home.command();
    command
        .env("TRANSITOUS_LOG_FILE", home.path())
        .args(["-v", "-o", "json", "usage"]);
    let output = run(command).await;
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_str(&stderr(&output)).unwrap();
    assert_eq!(error["status"], "error");
}

#[test]
fn rejects_an_invalid_log_filter() {
    let home = Home::new();
    let mut command = home.command();
    command
        .env("TRANSITOUS_LOG", "motis=loud")
        .args(["-o", "json", "usage"]);
    let output = command.output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("invalid TRANSITOUS_LOG"),
        "{}",
        stderr(&output)
    );
}

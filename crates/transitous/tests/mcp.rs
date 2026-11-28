//! End-to-end tests of `transitous mcp`, with a client that starts the
//! binary against the sim.
//!
//! Like the CLI tests, they give every server its own home, which does not
//! isolate the directories on Windows.
#![cfg(not(windows))]

use std::io::{Read as _, Write as _};
use std::process::Stdio;

use motis_sim::{DIRECT_TRIP, Injection, Sim};
use rmcp::model::CallToolRequestParams;
use rmcp::service::RunningService;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::{ConfigureCommandExt as _, StreamableHttpClientTransport, TokioChildProcess};
use rmcp::{RoleClient, ServiceExt as _};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, BufReader};

const CONTACT: &str = "https://github.com/udondan/transitous";
const HAUPTBAHNHOF: &str = "at-Railway-Current-Reference-Data-2026_de:06412:10:16:16";
const HAUPTWACHE: &str = "de-DELFI_de:06412:1_G";
/// Before the connections of the fixtures.
const NOW: &str = "2026-11-26T20:00:00Z";
const TOKEN: &str = "a-token-long-enough-for-tests";

/// The tool list that clients see. `TRANSITOUS_UPDATE_GOLDEN=1` rewrites it.
const GOLDEN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/mcp-tools.json");

/// The most tokens the tool list may cost, counted as four characters each.
const TOKEN_BUDGET: usize = 6000;

type Client = RunningService<RoleClient, ()>;

/// `transitous` without the `TRANSITOUS_*` variables of the caller, with its
/// files below `home`, for `sim`, and with a contact unless `contact` is
/// false.
fn command(home: &tempfile::TempDir, sim: &Sim, contact: bool) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_transitous"));
    for (name, _) in std::env::vars().filter(|(n, _)| n.starts_with("TRANSITOUS_")) {
        command.env_remove(name);
    }
    let home = home.path();
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("TZ", "Europe/Berlin")
        .env("TRANSITOUS_BASE_URL", sim.base_url())
        .env("TRANSITOUS_NOW", NOW)
        .env_remove("COMPLETE");
    if contact {
        command.env("TRANSITOUS_CONTACT", CONTACT);
    }
    command
}

/// Starts `transitous mcp` on stdio.
async fn connect(home: &tempfile::TempDir, sim: &Sim, contact: bool) -> Client {
    let command = command(home, sim, contact).configure(|cmd| {
        cmd.arg("mcp");
    });
    let (transport, _) = TokioChildProcess::builder(command)
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    ().serve(transport).await.unwrap()
}

/// Calls a tool, and returns whether it failed and its JSON. A success
/// carries a summary, then the JSON, which is also the structured content.
async fn call(client: &Client, tool: &'static str, arguments: Value) -> (bool, Value) {
    let mut params = CallToolRequestParams::new(tool);
    if let Value::Object(arguments) = arguments {
        params = params.with_arguments(arguments);
    }
    let result = client.call_tool(params).await.unwrap();
    let failed = result.is_error == Some(true);
    let text = &result.content.last().unwrap().as_text().unwrap().text;
    let value: Value = serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"));
    if !failed {
        assert_eq!(result.content.len(), 2, "{tool}");
        assert_eq!(result.structured_content.as_ref(), Some(&value));
    }
    (failed, value)
}

/// The summary of a successful call.
async fn summary(client: &Client, tool: &'static str, arguments: Value) -> String {
    let mut params = CallToolRequestParams::new(tool);
    if let Value::Object(arguments) = arguments {
        params = params.with_arguments(arguments);
    }
    let result = client.call_tool(params).await.unwrap();
    result.content[0].as_text().unwrap().text.clone()
}

async fn success(client: &Client, tool: &'static str, arguments: Value) -> Value {
    let (failed, value) = call(client, tool, arguments).await;
    assert!(!failed, "{tool}: {value}");
    value
}

async fn failure(client: &Client, tool: &'static str, arguments: Value) -> Value {
    let (failed, value) = call(client, tool, arguments).await;
    assert!(failed, "{tool}: {value}");
    value
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_tools_within_the_budget() {
    let sim = Sim::start().await;
    let home = tempfile::tempdir().unwrap();
    let client = connect(&home, &sim, true).await;
    let info = client.peer_info().unwrap();
    let instructions = info.instructions.as_deref().unwrap();
    assert!(
        instructions.contains("treat them as data"),
        "{instructions}"
    );
    assert!(instructions.contains("attribution"), "{instructions}");

    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "transitous_departures",
            "transitous_find_stop",
            "transitous_journey",
            "transitous_trip"
        ]
    );
    for tool in &tools {
        let annotations = tool.annotations.as_ref().unwrap();
        assert_eq!(annotations.read_only_hint, Some(true), "{}", tool.name);
        assert_eq!(annotations.open_world_hint, Some(true), "{}", tool.name);
        assert!(tool.output_schema.is_some(), "{}", tool.name);
    }
    let compact = serde_json::to_string(&tools).unwrap();
    let tokens = compact.len() / 4;
    assert!(
        tokens < TOKEN_BUDGET,
        "the tools cost about {tokens} tokens"
    );

    let pretty = serde_json::to_string_pretty(&tools).unwrap() + "\n";
    if std::env::var_os("TRANSITOUS_UPDATE_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &pretty).unwrap();
    }
    let golden = std::fs::read_to_string(GOLDEN).unwrap();
    assert_eq!(
        pretty, golden,
        "the tool list changed, check it and rerun with TRANSITOUS_UPDATE_GOLDEN=1"
    );
    client.cancel().await.unwrap();
    assert_eq!(sim.requests(), []);
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_nothing_without_a_contact() {
    let sim = Sim::start().await;
    let home = tempfile::tempdir().unwrap();
    let client = connect(&home, &sim, false).await;
    for (tool, arguments) in [
        ("transitous_find_stop", json!({ "name": "Hauptwache" })),
        ("transitous_departures", json!({ "stop": HAUPTWACHE })),
        (
            "transitous_trip",
            json!({ "from": HAUPTBAHNHOF, "to": HAUPTWACHE }),
        ),
        ("transitous_journey", json!({ "trip_id": DIRECT_TRIP })),
    ] {
        let error = failure(&client, tool, arguments).await;
        assert_eq!(error["status"], "contact_missing", "{tool}: {error}");
    }
    client.cancel().await.unwrap();
    assert_eq!(sim.requests(), []);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_invalid_input_before_any_request() {
    let sim = Sim::start().await;
    let home = tempfile::tempdir().unwrap();
    let client = connect(&home, &sim, true).await;
    for (tool, arguments, part) in [
        (
            "transitous_find_stop",
            json!({ "name": "Hauptwache", "near": "here" }),
            "LAT,LON",
        ),
        (
            "transitous_departures",
            json!({ "stop": HAUPTWACHE, "modes": ["rocket"] }),
            "unknown mode",
        ),
        (
            "transitous_trip",
            json!({ "from": HAUPTBAHNHOF, "to": HAUPTWACHE, "time": "soon" }),
            "invalid time",
        ),
    ] {
        let error = failure(&client, tool, arguments).await;
        assert_eq!(error["status"], "invalid_input", "{tool}: {error}");
        let message = error["error"].as_str().unwrap();
        assert!(message.contains(part), "{tool}: {message}");
    }
    client.cancel().await.unwrap();
    assert_eq!(sim.requests(), []);
}

#[tokio::test(flavor = "multi_thread")]
async fn finds_stops_and_departures() {
    let sim = Sim::start().await;
    let home = tempfile::tempdir().unwrap();
    let client = connect(&home, &sim, true).await;
    let arguments = json!({ "name": "Hauptwache", "near": "50.11,8.68", "count": 3 });
    let stops = success(&client, "transitous_find_stop", arguments.clone()).await;
    assert_eq!(stops["query"], "Hauptwache");
    let line = summary(&client, "transitous_find_stop", arguments).await;
    assert!(line.ends_with("stops for \"Hauptwache\""), "{line}");

    let error = failure(
        &client,
        "transitous_departures",
        json!({ "stop": "Hauptwache" }),
    )
    .await;
    assert_eq!(error["status"], "ambiguous_stop");
    assert!(error["candidates"].as_array().unwrap().len() > 1, "{error}");

    let arguments = json!({
        "stop": HAUPTWACHE,
        "time": "2026-11-26T21:00",
        "count": 5,
        "modes": ["subway"],
    });
    let departures = success(&client, "transitous_departures", arguments.clone()).await;
    assert_eq!(departures["arrivals"], false);
    let request = sim.requests().pop().unwrap();
    assert_eq!(request.path, "/api/v6/stoptimes");
    assert_eq!(request.param("time"), Some("2026-11-26T20:00:00Z"));
    assert_eq!(request.param("mode"), Some("SUBWAY"));
    let line = summary(&client, "transitous_departures", arguments).await;
    assert!(line.contains(" departures at "), "{line}");
    assert!(line.ends_with(" cancelled"), "{line}");

    let arguments = json!({ "stop": HAUPTWACHE, "arrivals": true });
    let line = summary(&client, "transitous_departures", arguments).await;
    assert!(line.contains(" arrivals at "), "{line}");
    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn plans_trips_with_their_status() {
    let sim = Sim::start().await;
    sim.inject(Injection::Cancel {
        trip_id: DIRECT_TRIP.to_owned(),
    });
    let home = tempfile::tempdir().unwrap();
    let client = connect(&home, &sim, true).await;
    let arguments = json!({
        "from": HAUPTBAHNHOF,
        "to": HAUPTWACHE,
        "time": "2026-11-26T22:00",
        "arrive_by": true,
        "count": 4,
        "max_transfers": 2,
    });
    let result = success(&client, "transitous_trip", arguments.clone()).await;
    let request = &sim.requests()[0];
    assert_eq!(request.path, "/api/v6/plan");
    assert_eq!(request.param("arriveBy"), Some("true"));
    assert_eq!(request.param("time"), Some("2026-11-26T21:00:00Z"));
    assert_eq!(
        request.param("realtimeMode"),
        Some("REALTIME_ANNOTATION_ONLY")
    );
    let statuses: Vec<&str> = result["connections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["status"].as_str().unwrap())
        .collect();
    assert!(statuses.contains(&"cancelled"), "{statuses:?}");
    assert!(result["attribution"].is_object(), "{result}");
    let line = summary(&client, "transitous_trip", arguments).await;
    assert!(line.contains(" connections from "), "{line}");
    assert!(line.contains(" cancelled"), "{line}");

    let journey = success(
        &client,
        "transitous_journey",
        json!({ "trip_id": DIRECT_TRIP }),
    )
    .await;
    assert_eq!(journey["cancelled"], true);
    let line = summary(
        &client,
        "transitous_journey",
        json!({ "trip_id": DIRECT_TRIP }),
    )
    .await;
    assert!(line.ends_with(" stops, cancelled"), "{line}");
    client.cancel().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn shows_running_journeys_and_server_errors() {
    let sim = Sim::start().await;
    let home = tempfile::tempdir().unwrap();
    let client = connect(&home, &sim, true).await;
    let arguments = json!({ "trip_id": DIRECT_TRIP });
    let line = summary(&client, "transitous_journey", arguments.clone()).await;
    assert!(line.ends_with(" stops, running"), "{line}");

    sim.inject(Injection::RateLimited {
        retry_after_seconds: 42,
    });
    let error = failure(
        &client,
        "transitous_trip",
        json!({ "from": HAUPTBAHNHOF, "to": HAUPTWACHE, "realtime_routing": true }),
    )
    .await;
    assert_eq!(error["status"], "rate_limited");
    assert_eq!(error["retry_after_seconds"], 42);
    client.cancel().await.unwrap();
}

/// Sends a request without a body to the server at `address`, and returns
/// the response.
fn raw_request(address: &str, authorization: Option<&str>) -> String {
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    let header = authorization
        .map(|value| format!("Authorization: {value}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {address}\r\n{header}Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[tokio::test(flavor = "multi_thread")]
async fn serves_http_with_a_token() {
    let sim = Sim::start().await;
    let home = tempfile::tempdir().unwrap();
    let mut server = command(&home, &sim, true)
        .args(["mcp", "--http", "127.0.0.1:0"])
        .env("TRANSITOUS_MCP_TOKEN", TOKEN)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(server.stderr.take().unwrap()).lines();
    let line = lines.next_line().await.unwrap().unwrap();
    let url = line.strip_prefix("listening on ").unwrap().to_owned();
    let address = url
        .strip_prefix("http://")
        .and_then(|rest| rest.strip_suffix("/mcp"))
        .unwrap()
        .to_owned();

    for authorization in [None, Some("Bearer wrong-token-of-some-length"), Some(TOKEN)] {
        let address = address.clone();
        let response = tokio::task::spawn_blocking(move || raw_request(&address, authorization))
            .await
            .unwrap();
        assert!(response.starts_with("HTTP/1.1 401"), "{response}");
        assert!(
            response.to_lowercase().contains("www-authenticate: bearer"),
            "{response}"
        );
    }

    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = StreamableHttpClientTransportConfig::with_uri(url).auth_header(TOKEN);
    let client = ().serve(StreamableHttpClientTransport::from_config(config)).await.unwrap();
    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 4);
    let journey = success(
        &client,
        "transitous_journey",
        json!({ "trip_id": DIRECT_TRIP }),
    )
    .await;
    assert_eq!(journey["trip_id"], DIRECT_TRIP);
    client.cancel().await.unwrap();

    let pid = server.id().unwrap().to_string();
    let killed = std::process::Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .unwrap();
    assert!(killed.success());
    let status = server.wait().await.unwrap();
    assert!(status.success(), "{status}");
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_a_token_and_a_free_address_for_http() {
    let sim = Sim::start().await;
    let home = tempfile::tempdir().unwrap();
    let output = command(&home, &sim, true)
        .args(["-o", "json", "mcp", "--http"])
        .env("TRANSITOUS_MCP_TOKEN", "short")
        .output()
        .await
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("TRANSITOUS_MCP_TOKEN"),
        "{error}"
    );

    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = taken.local_addr().unwrap().to_string();
    let output = command(&home, &sim, true)
        .args(["-o", "json", "mcp", "--http", &address])
        .env("TRANSITOUS_MCP_TOKEN", TOKEN)
        .output()
        .await
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .starts_with("cannot listen on"),
        "{error}"
    );
}

//! The client against a server on loopback.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use motis::{Client, Error, Geocode, Mode, Plan, RealtimeMode, Stoptimes, Timestamp, Url};

/// A request the server saw.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    query: Vec<(String, String)>,
    user_agent: Option<String>,
}

/// What the server answers to every request.
#[derive(Clone)]
struct Reply {
    status: StatusCode,
    headers: Vec<(&'static str, &'static str)>,
    body: String,
    delay: Duration,
}

impl Reply {
    fn ok(body: impl Into<String>) -> Self {
        Self::status(StatusCode::OK, body)
    }

    fn status(status: StatusCode, body: impl Into<String>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
            delay: Duration::ZERO,
        }
    }
}

/// Serves `reply` on loopback, returns the base URL and the request log.
async fn serve(reply: Reply) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&log);
    let app = Router::new().fallback(move |request: axum::extract::Request| {
        let reply = reply.clone();
        let uri = request.uri().clone();
        let url = Url::parse(&format!("http://loopback{uri}")).unwrap();
        seen.lock().unwrap().push(Seen {
            path: url.path().to_owned(),
            query: url.query_pairs().into_owned().collect(),
            user_agent: request
                .headers()
                .get("user-agent")
                .map(|value| value.to_str().unwrap().to_owned()),
        });
        async move {
            tokio::time::sleep(reply.delay).await;
            let mut headers = HeaderMap::new();
            for (name, value) in reply.headers {
                headers.insert(name, value.parse().unwrap());
            }
            (reply.status, headers, reply.body)
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), log)
}

fn data(name: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/data/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[tokio::test]
async fn sends_the_user_agent_and_the_query_below_the_base_path() {
    let (base, log) = serve(Reply::ok(data("plan-realtime.json"))).await;
    let client = Client::builder(format!("{base}/motis"))
        .user_agent("test/1.0 (contact: test@example.org)")
        .build()
        .unwrap();
    let time: Timestamp = "2026-11-26T06:30:00Z".parse().unwrap();
    let plan = client
        .send(&Plan {
            time: Some(time),
            arrive_by: Some(true),
            transit_modes: vec![Mode::Rail, Mode::Bus],
            realtime_mode: Some(RealtimeMode::RealtimeAnnotationOnly),
            ..Plan::new("de:06412:1_G", "50.1,8.6")
        })
        .await
        .unwrap();
    assert_eq!(plan.itineraries.len(), 3);

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1);
    let seen = &log[0];
    assert_eq!(seen.path, "/motis/api/v6/plan");
    assert_eq!(
        seen.user_agent.as_deref(),
        Some("test/1.0 (contact: test@example.org)")
    );
    let pairs: Vec<(&str, &str)> = seen
        .query
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("fromPlace", "de:06412:1_G"),
            ("toPlace", "50.1,8.6"),
            ("time", "2026-11-26T06:30:00Z"),
            ("arriveBy", "true"),
            ("transitModes", "RAIL,BUS"),
            ("realtimeMode", "REALTIME_ANNOTATION_ONLY"),
        ]
    );
}

#[tokio::test]
async fn sends_no_user_agent_unless_told() {
    let (base, log) = serve(Reply::ok(data("geocode.json"))).await;
    let client = Client::builder(base).build().unwrap();
    let matches = client.send(&Geocode::new("Hauptwache")).await.unwrap();
    assert_eq!(matches.len(), 4);
    let log = log.lock().unwrap();
    assert_eq!(log[0].path, "/api/v1/geocode");
    assert_eq!(log[0].user_agent, None);
}

#[tokio::test]
async fn reports_the_error_of_the_api_without_retrying() {
    let reply = Reply::status(StatusCode::NOT_FOUND, data("error-unknown-stop.json"));
    let (base, log) = serve(reply).await;
    let client = Client::builder(base).build().unwrap();
    let error = client.send(&Stoptimes::new("nope")).await.unwrap_err();
    let Error::Status {
        status,
        ref message,
        retry_after,
    } = error
    else {
        panic!("{error:?}");
    };
    assert_eq!(status, 404);
    assert_eq!(message, r#"unknown feed id """#);
    assert_eq!(retry_after, None);
    assert_eq!(
        error.to_string(),
        r#"the server answered 404: unknown feed id """#
    );
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn reports_retry_after_without_retrying() {
    let mut reply = Reply::status(StatusCode::TOO_MANY_REQUESTS, "");
    reply.headers.push(("retry-after", "30"));
    let (base, log) = serve(reply).await;
    let client = Client::builder(base).build().unwrap();
    let error = client.fetch(&Geocode::new("x")).await.unwrap_err();
    let Error::Status {
        status,
        message,
        retry_after,
    } = error
    else {
        panic!("{error:?}");
    };
    assert_eq!(status, 429);
    assert_eq!(message, "Too Many Requests");
    assert_eq!(retry_after, Some(Duration::from_secs(30)));
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn times_out_once() {
    let mut reply = Reply::ok("[]");
    reply.delay = Duration::from_secs(5);
    let (base, log) = serve(reply).await;
    let client = Client::builder(base)
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();
    let error = client.fetch(&Geocode::new("x")).await.unwrap_err();
    assert!(matches!(error, Error::Timeout(_)), "{error:?}");
    assert_eq!(error.to_string(), "the server did not answer in time");
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn reports_a_server_that_is_not_there() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let client = Client::builder(format!("http://{address}"))
        .build()
        .unwrap();
    let error = client.fetch(&Geocode::new("x")).await.unwrap_err();
    assert!(matches!(error, Error::Http(_)), "{error:?}");
    assert_eq!(error.to_string(), "the request failed");
}

#[tokio::test]
async fn reports_a_malformed_response() {
    let (base, _) = serve(Reply::ok("<html>")).await;
    let client = Client::builder(base).build().unwrap();
    let error = client.send(&Geocode::new("x")).await.unwrap_err();
    assert!(matches!(error, Error::Decode(_)), "{error:?}");
}

#[test]
fn takes_only_http_base_urls() {
    for base in ["not a url", "mailto:ops@example.org", "ftp://example.org"] {
        let error = Client::builder(base).build().unwrap_err();
        assert!(matches!(error, Error::BaseUrl(_)), "{base}: {error:?}");
    }
    assert_eq!(
        Client::builder("ftp://example.org")
            .build()
            .unwrap_err()
            .to_string(),
        r#"invalid base URL "ftp://example.org": expected an absolute http or https URL"#
    );
}

#[test]
fn builds_urls_below_the_base() {
    let client = Client::builder("https://example.org/motis?x=1#top")
        .build()
        .unwrap();
    assert_eq!(client.base_url().as_str(), "https://example.org/motis/");
    let url = client.url(&Geocode::new("Frankfurt (Main) Hbf"));
    assert_eq!(
        url.as_str(),
        "https://example.org/motis/api/v1/geocode?text=Frankfurt+%28Main%29+Hbf"
    );
    let client = Client::builder("https://example.org/").build().unwrap();
    assert_eq!(client.base_url().as_str(), "https://example.org/");
}

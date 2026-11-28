use std::time::Duration;

use jiff::Timestamp;

/// What can go wrong talking to the API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The base URL is not an absolute `http` or `https` URL.
    #[error("invalid base URL {0:?}: expected an absolute http or https URL")]
    BaseUrl(String),
    /// The HTTP client could not be set up.
    #[error("could not set up the HTTP client")]
    Client(#[source] reqwest::Error),
    /// The server did not answer in time.
    #[error("the server did not answer in time")]
    Timeout(#[source] reqwest::Error),
    /// The request could not be sent or the response not be read.
    #[error("the request failed")]
    Http(#[source] reqwest::Error),
    /// The server answered with an error status.
    #[error("the server answered {status}: {message}")]
    Status {
        status: u16,
        /// The `error` text of the response, or the status reason.
        message: String,
        /// How long to wait before asking again, from `Retry-After`.
        retry_after: Option<Duration>,
    },
    /// The response does not have the expected form.
    #[error("unexpected response")]
    Decode(#[source] serde_json::Error),
}

impl Error {
    pub(crate) fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout(error)
        } else {
            Self::Http(error)
        }
    }
}

/// The longest message taken from an error response that is not JSON.
const MAX_MESSAGE: usize = 200;

/// The message of an error response: the `error` field the API sends, or the
/// start of the body, or else the reason phrase of the status.
pub(crate) fn error_message(status: reqwest::StatusCode, body: &[u8]) -> String {
    #[derive(serde::Deserialize)]
    struct Body {
        error: String,
    }
    if let Ok(Body { error }) = serde_json::from_slice::<Body>(body) {
        return error;
    }
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    if text.is_empty() {
        return status.canonical_reason().unwrap_or("error").to_owned();
    }
    text.chars().take(MAX_MESSAGE).collect()
}

/// Parses `Retry-After`: seconds, or an HTTP date relative to `now`.
pub(crate) fn retry_after(value: &str, now: Timestamp) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let at = jiff::fmt::rfc2822::DateTimeParser::new()
        .parse_timestamp(value)
        .ok()?;
    let seconds = at.as_second().saturating_sub(now.as_second());
    Some(Duration::from_secs(u64::try_from(seconds).unwrap_or(0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::StatusCode;

    #[test]
    fn takes_the_error_field_of_the_api() {
        let body = br#"{"error":"unknown feed id \"\""}"#;
        assert_eq!(
            error_message(StatusCode::NOT_FOUND, body),
            r#"unknown feed id """#
        );
    }

    #[test]
    fn takes_the_start_of_another_body() {
        let body = format!("  {}  ", "x".repeat(500));
        let message = error_message(StatusCode::BAD_GATEWAY, body.as_bytes());
        assert_eq!(message, "x".repeat(MAX_MESSAGE));
    }

    #[test]
    fn takes_the_reason_of_an_empty_body() {
        assert_eq!(
            error_message(StatusCode::SERVICE_UNAVAILABLE, b" \n"),
            "Service Unavailable"
        );
        assert_eq!(
            error_message(StatusCode::from_u16(599).unwrap(), b""),
            "error"
        );
    }

    #[test]
    fn parses_retry_after() {
        let now: Timestamp = "2026-11-26T20:00:00Z".parse().unwrap();
        assert_eq!(retry_after(" 120 ", now), Some(Duration::from_secs(120)));
        assert_eq!(
            retry_after("Thu, 26 Nov 2026 20:01:30 GMT", now),
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            retry_after("Thu, 26 Nov 2026 19:00:00 GMT", now),
            Some(Duration::ZERO)
        );
        assert_eq!(retry_after("soon", now), None);
    }
}

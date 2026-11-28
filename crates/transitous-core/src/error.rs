//! The errors, each with a stable [`Error::status`] for scripts and agents.

use crate::model::{ErrorBody, StopMatch};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No contact is configured, so no request is sent.
    #[error(
        "no contact configured: Transitous asks for one in every request. \
         Set it with `transitous config set contact <url or email>` or TRANSITOUS_CONTACT"
    )]
    ContactMissing,
    #[error("the contact is not valid: {0}")]
    InvalidContact(String),
    /// The name fits several stops.
    #[error("{query:?} does not name one stop, pass the ID of one of the {} candidates", candidates.len())]
    AmbiguousStop {
        query: String,
        candidates: Vec<StopMatch>,
    },
    #[error("no stop matches {query:?}")]
    StopNotFound { query: String },
    /// Too many requests in the last minute, by this client or the server.
    #[error("too many requests, try again in {retry_after_seconds} s")]
    RateLimited { retry_after_seconds: u64 },
    /// The `plan` requests of the last 24 hours reached the configured
    /// limit.
    #[error(
        "{limit} plan requests in 24 hours reached, try again in {retry_after_seconds} s. \
         Raise plan_daily_limit only after agreeing on more with Transitous"
    )]
    DailyLimit {
        limit: u32,
        retry_after_seconds: u64,
    },
    /// The server cannot be reached or has a problem.
    #[error("Transitous is unavailable: {message}")]
    Unavailable { message: String },
    #[error("Transitous did not answer in time")]
    Timeout,
    /// The server rejected the request.
    #[error("bad request: {message}")]
    BadRequest { message: String },
    /// The server does not know the stop or trip.
    #[error("not found: {message}")]
    NotFound { message: String },
    /// The server answered something unexpected.
    #[error("unexpected answer from Transitous: {message}")]
    Upstream { message: String },
    #[error(transparent)]
    Config(#[from] crate::config::Error),
    /// The usage counter cannot be read or written.
    #[error("cannot update the usage counter at {path}: {source}")]
    Usage {
        path: String,
        source: std::io::Error,
    },
}

impl Error {
    /// A stable name of the kind of error, like `ambiguous_stop`.
    pub fn status(&self) -> &'static str {
        match self {
            Self::ContactMissing => "contact_missing",
            Self::InvalidContact(_) | Self::Config(_) => "config_error",
            Self::AmbiguousStop { .. } => "ambiguous_stop",
            Self::StopNotFound { .. } => "stop_not_found",
            Self::RateLimited { .. } => "rate_limited",
            Self::DailyLimit { .. } => "daily_limit",
            Self::Unavailable { .. } => "unavailable",
            Self::Timeout => "timeout",
            Self::BadRequest { .. } => "bad_request",
            Self::NotFound { .. } => "not_found",
            Self::Upstream { .. } => "upstream_error",
            Self::Usage { .. } => "usage_error",
        }
    }

    /// When to try again, in seconds.
    pub fn retry_after_seconds(&self) -> Option<u64> {
        match self {
            Self::RateLimited {
                retry_after_seconds,
            }
            | Self::DailyLimit {
                retry_after_seconds,
                ..
            } => Some(*retry_after_seconds),
            _ => None,
        }
    }

    /// The error as output.
    pub fn body(&self) -> ErrorBody {
        ErrorBody {
            error: self.to_string(),
            status: self.status().to_owned(),
            retry_after_seconds: self.retry_after_seconds(),
            candidates: match self {
                Self::AmbiguousStop { candidates, .. } => candidates.clone(),
                _ => Vec::new(),
            },
        }
    }
}

impl From<motis::Error> for Error {
    fn from(error: motis::Error) -> Self {
        match error {
            motis::Error::Status {
                status: 429,
                retry_after,
                ..
            } => Self::RateLimited {
                retry_after_seconds: retry_after.map_or(60, |d| d.as_secs()),
            },
            motis::Error::Status {
                status: 400 | 422,
                message,
                ..
            } => Self::BadRequest { message },
            motis::Error::Status {
                status: 404,
                message,
                ..
            } => Self::NotFound { message },
            motis::Error::Status {
                status, message, ..
            } if status >= 500 => Self::Unavailable {
                message: format!("{status}: {message}"),
            },
            motis::Error::Timeout(_) => Self::Timeout,
            error @ (motis::Error::Http(_) | motis::Error::Client(_)) => Self::Unavailable {
                message: chain(&error),
            },
            error @ motis::Error::BaseUrl(_) => Self::Config(crate::config::Error::InvalidValue {
                key: "base_url".to_owned(),
                reason: error.to_string(),
            }),
            error => Self::Upstream {
                message: chain(&error),
            },
        }
    }
}

/// The message of an error and its sources.
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn status(status: u16, retry_after: Option<Duration>) -> Error {
        motis::Error::Status {
            status,
            message: "text".into(),
            retry_after,
        }
        .into()
    }

    #[test]
    fn maps_the_statuses_of_the_server() {
        let error = status(429, Some(Duration::from_secs(7)));
        assert_eq!(error.status(), "rate_limited");
        assert_eq!(error.retry_after_seconds(), Some(7));
        assert_eq!(status(429, None).retry_after_seconds(), Some(60));
        assert_eq!(status(400, None).status(), "bad_request");
        assert_eq!(status(422, None).status(), "bad_request");
        assert_eq!(status(404, None).status(), "not_found");
        let error = status(503, None);
        assert_eq!(error.status(), "unavailable");
        assert_eq!(error.to_string(), "Transitous is unavailable: 503: text");
        assert_eq!(status(418, None).status(), "upstream_error");
    }

    #[test]
    fn maps_other_client_errors() {
        let decode = serde_json::from_str::<u8>("x").unwrap_err();
        let error = Error::from(motis::Error::Decode(decode));
        assert_eq!(error.status(), "upstream_error");
        assert!(
            error
                .to_string()
                .starts_with("unexpected answer from Transitous: unexpected response: "),
            "{error}"
        );
        let error = Error::from(motis::Error::BaseUrl("x".into()));
        assert_eq!(error.status(), "config_error");
        assert_eq!(error.retry_after_seconds(), None);
    }

    #[test]
    fn names_every_status() {
        let errors = [
            Error::ContactMissing,
            Error::InvalidContact("x".into()),
            Error::StopNotFound { query: "x".into() },
            Error::DailyLimit {
                limit: 720,
                retry_after_seconds: 5,
            },
            Error::Timeout,
        ];
        let statuses: Vec<&str> = errors.iter().map(Error::status).collect();
        assert_eq!(
            statuses,
            [
                "contact_missing",
                "config_error",
                "stop_not_found",
                "daily_limit",
                "timeout"
            ]
        );
        assert_eq!(errors[3].retry_after_seconds(), Some(5));
    }

    #[test]
    fn the_body_lists_the_candidates() {
        let candidate = StopMatch {
            id: "1".into(),
            name: "Bahnhof".into(),
            area: None,
            lat: 0.0,
            lon: 0.0,
            modes: Vec::new(),
        };
        let body = Error::AmbiguousStop {
            query: "Bahnhof".into(),
            candidates: vec![candidate.clone()],
        }
        .body();
        assert_eq!(body.status, "ambiguous_stop");
        assert_eq!(
            body.error,
            "\"Bahnhof\" does not name one stop, pass the ID of one of the 1 candidates"
        );
        assert_eq!(body.candidates, [candidate]);
        assert_eq!(Error::Timeout.body().candidates, Vec::<StopMatch>::new());
    }
}

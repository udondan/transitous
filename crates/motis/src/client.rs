use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use reqwest::header::{RETRY_AFTER, USER_AGENT};
use reqwest::{StatusCode, Url};
use rustls::{ClientConfig, RootCertStore};

use crate::error::{Error, error_message, retry_after};
use crate::request::Request;

/// The default time to wait for a response.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Configures a [`Client`].
#[derive(Debug, Clone)]
pub struct ClientBuilder {
    base: String,
    user_agent: Option<String>,
    timeout: Duration,
}

impl ClientBuilder {
    /// The `User-Agent` header of every request. Public instances usually
    /// ask for one that names the application and a contact.
    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// How long to wait for a response, [`DEFAULT_TIMEOUT`] if not set.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Builds the client.
    ///
    /// # Panics
    ///
    /// Never: the TLS provider supports the protocol versions it asks for.
    pub fn build(self) -> Result<Client, Error> {
        let base = parse_base(&self.base)?;
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let tls =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .expect("ring supports the default protocol versions")
                .with_root_certificates(roots)
                .with_no_client_auth();
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .timeout(self.timeout)
            // Not even the protocol-level retries reqwest makes by default.
            .retry(reqwest::retry::never())
            .build()
            .map_err(Error::Client)?;
        Ok(Client {
            http,
            base,
            user_agent: self.user_agent,
        })
    }
}

/// Makes the base URL end in a slash, so the endpoint paths go below it.
fn parse_base(base: &str) -> Result<Url, Error> {
    let invalid = || Error::BaseUrl(base.to_owned());
    let mut url = Url::parse(base).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(invalid());
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// A client for one MOTIS server.
///
/// It sends every request once: it never retries.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    user_agent: Option<String>,
}

impl Client {
    /// Starts configuring a client for the server at `base`, like
    /// `https://api.transitous.org`. The endpoint paths go below it.
    pub fn builder(base: impl Into<String>) -> ClientBuilder {
        ClientBuilder {
            base: base.into(),
            user_agent: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// The base URL, ending in a slash.
    pub fn base_url(&self) -> &Url {
        &self.base
    }

    /// The URL a request goes to, with its query.
    ///
    /// # Panics
    ///
    /// Never: the endpoint paths are relative URLs.
    pub fn url<R: Request>(&self, request: &R) -> Url {
        let mut url = self
            .base
            .join(R::PATH)
            .expect("the endpoint paths are relative URLs");
        url.query_pairs_mut().extend_pairs(request.query());
        url
    }

    /// Sends a request and decodes the response.
    pub async fn send<R: Request>(&self, request: &R) -> Result<R::Response, Error> {
        let body = self.fetch(request).await?;
        Self::decode::<R>(&body)
    }

    /// Sends a request and returns the body of a successful response, for
    /// callers that keep responses, like a cache. [`Client::decode`] turns it
    /// into the response type.
    pub async fn fetch<R: Request>(&self, request: &R) -> Result<Vec<u8>, Error> {
        let url = self.url(request);
        tracing::debug!(%url, "request");
        let mut builder = self.http.get(url);
        if let Some(user_agent) = &self.user_agent {
            builder = builder.header(USER_AGENT, user_agent);
        }
        let response = builder.send().await.map_err(Error::from_reqwest)?;
        let status = response.status();
        tracing::debug!(%status, "response");
        if status.is_success() {
            let body = response.bytes().await.map_err(Error::from_reqwest)?;
            return Ok(body.to_vec());
        }
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| retry_after(value, Timestamp::now()));
        let body = response.bytes().await.unwrap_or_default();
        Err(status_error(status, &body, retry_after))
    }

    /// Decodes the body of a successful response.
    pub fn decode<R: Request>(body: &[u8]) -> Result<R::Response, Error> {
        serde_json::from_slice(body).map_err(Error::Decode)
    }
}

fn status_error(status: StatusCode, body: &[u8], retry_after: Option<Duration>) -> Error {
    Error::Status {
        status: status.as_u16(),
        message: error_message(status, body),
        retry_after,
    }
}

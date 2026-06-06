//! Backend-agnostic request builder.
//!
//! Wraps the *small* surface area that quota / OAuth fetchers actually need
//! (GET / POST, header, bearer-auth, JSON body, form body, parsed response)
//! so each fetcher writes its logic once instead of branching on
//! [`FingerprintAwareClient`] every time.
//!
//! ## Why not a trait?
//!
//! `reqwest::RequestBuilder` and `wreq::RequestBuilder` are *not* traits and
//! don't share a common API surface; a struct holding both as an enum lets
//! us expose a tiny stable API to fetchers without async-trait overhead.
//!
//! ## Example
//!
//! ```no_run
//! use skillstar_fingerprint::{build_client, DeviceFingerprint, request::Req};
//!
//! # #[derive(serde::Deserialize)] struct Balance { total: f64 }
//! # async fn run() -> anyhow::Result<()> {
//! let fp = DeviceFingerprint::generate_chrome();
//! let client = build_client(&fp)?;
//! let body: Balance = Req::get(&client, "https://api.deepseek.com/user/balance")
//!     .bearer("sk-xxx")
//!     .header("Accept", "application/json")
//!     .send_json()
//!     .await?;
//! println!("{}", body.total);
//! # Ok(())
//! # }
//! ```

use crate::client::FingerprintAwareClient;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::time::Duration;
use thiserror::Error;

/// HTTP verbs the fetchers actually use.
#[derive(Debug, Clone, Copy)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, Clone)]
enum Body {
    None,
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
    Raw(String, &'static str), // (body, content_type)
}

/// Fluent request builder backed by either reqwest or wreq.
#[derive(Debug, Clone)]
pub struct Req<'c> {
    client: &'c FingerprintAwareClient,
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    bearer: Option<String>,
    body: Body,
    timeout: Option<Duration>,
}

impl<'c> Req<'c> {
    /// Start a new request. Most fetchers use [`Req::get`] / [`Req::post`].
    pub fn new(client: &'c FingerprintAwareClient, method: Method, url: impl Into<String>) -> Self {
        Self {
            client,
            method,
            url: url.into(),
            headers: Vec::new(),
            bearer: None,
            body: Body::None,
            timeout: None,
        }
    }

    pub fn get(client: &'c FingerprintAwareClient, url: impl Into<String>) -> Self {
        Self::new(client, Method::Get, url)
    }

    pub fn post(client: &'c FingerprintAwareClient, url: impl Into<String>) -> Self {
        Self::new(client, Method::Post, url)
    }

    pub fn put(client: &'c FingerprintAwareClient, url: impl Into<String>) -> Self {
        Self::new(client, Method::Put, url)
    }

    pub fn delete(client: &'c FingerprintAwareClient, url: impl Into<String>) -> Self {
        Self::new(client, Method::Delete, url)
    }

    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }

    pub fn bearer(mut self, token: impl Into<String>) -> Self {
        self.bearer = Some(token.into());
        self
    }

    pub fn json<T: Serialize>(mut self, body: &T) -> Result<Self, RequestError> {
        self.body = Body::Json(serde_json::to_value(body).map_err(RequestError::JsonEncode)?);
        Ok(self)
    }

    pub fn form(mut self, pairs: &[(&str, &str)]) -> Self {
        self.body = Body::Form(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
        self
    }

    pub fn raw(mut self, body: impl Into<String>, content_type: &'static str) -> Self {
        self.body = Body::Raw(body.into(), content_type);
        self
    }

    pub fn timeout(mut self, dur: Duration) -> Self {
        self.timeout = Some(dur);
        self
    }

    /// Send the request and return the raw response (status + text body).
    pub async fn send(self) -> Result<Resp, RequestError> {
        match self.client {
            FingerprintAwareClient::Reqwest(c) => self.send_reqwest(c).await,
            #[cfg(feature = "impersonate")]
            FingerprintAwareClient::Wreq(c) => self.send_wreq(c).await,
        }
    }

    /// Convenience: send and deserialize the JSON body. Returns
    /// [`RequestError::HttpStatus`] for non-success responses (includes the
    /// status code so the fetcher can distinguish 401 from other errors).
    pub async fn send_json<T: DeserializeOwned>(self) -> Result<T, RequestError> {
        let resp = self.send().await?;
        if !resp.is_success() {
            return Err(RequestError::HttpStatus {
                status: resp.status,
                body: resp.body,
            });
        }
        serde_json::from_str(&resp.body).map_err(|e| RequestError::JsonDecode {
            source: e,
            body: resp.body,
        })
    }

    async fn send_reqwest(self, client: &reqwest::Client) -> Result<Resp, RequestError> {
        use reqwest::Method as M;
        let m = match self.method {
            Method::Get => M::GET,
            Method::Post => M::POST,
            Method::Put => M::PUT,
            Method::Delete => M::DELETE,
        };
        let mut req = client.request(m, &self.url);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(b) = &self.bearer {
            req = req.bearer_auth(b);
        }
        match &self.body {
            Body::None => {}
            Body::Json(v) => req = req.json(v),
            Body::Form(p) => req = req.form(p),
            Body::Raw(b, ct) => {
                req = req.header("Content-Type", *ct).body(b.clone());
            }
        }
        if let Some(t) = self.timeout {
            req = req.timeout(t);
        }
        let resp = req.send().await.map_err(RequestError::Transport)?;
        let status = resp.status().as_u16();
        let body = resp.text().await.map_err(RequestError::Transport)?;
        Ok(Resp { status, body })
    }

    #[cfg(feature = "impersonate")]
    async fn send_wreq(self, client: &wreq::Client) -> Result<Resp, RequestError> {
        use wreq::Method as M;
        let m = match self.method {
            Method::Get => M::GET,
            Method::Post => M::POST,
            Method::Put => M::PUT,
            Method::Delete => M::DELETE,
        };
        let mut req = client.request(m, &self.url);
        for (k, v) in &self.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(b) = &self.bearer {
            req = req.header("Authorization", format!("Bearer {b}"));
        }
        match &self.body {
            Body::None => {}
            Body::Json(v) => req = req.json(v),
            Body::Form(p) => req = req.form(p),
            Body::Raw(b, ct) => {
                req = req.header("Content-Type", *ct).body(b.clone());
            }
        }
        if let Some(t) = self.timeout {
            req = req.timeout(t);
        }
        let resp = req.send().await.map_err(RequestError::transport_wreq)?;
        let status = resp.status().as_u16();
        let body = resp.text().await.map_err(RequestError::transport_wreq)?;
        Ok(Resp { status, body })
    }
}

/// Lightweight response: status code + decoded UTF-8 body.
#[derive(Debug, Clone)]
pub struct Resp {
    pub status: u16,
    pub body: String,
}

impl Resp {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_auth_error(&self) -> bool {
        self.status == 401 || self.status == 403
    }

    pub fn parsed_json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.body)
    }
}

/// Errors from [`Req::send`] and [`Req::send_json`].
#[derive(Debug, Error)]
pub enum RequestError {
    /// Network / TLS / DNS / timeout — any transport-level failure.
    #[error("transport: {0}")]
    Transport(reqwest::Error),

    /// wreq transport error, kept separate so callers can match if needed.
    #[cfg(feature = "impersonate")]
    #[error("transport (wreq): {0}")]
    TransportWreq(String),

    /// HTTP status is non-2xx. `body` is the raw response text.
    #[error("http {status}: {body}")]
    HttpStatus { status: u16, body: String },

    /// JSON encoding of the request body failed.
    #[error("json encode: {0}")]
    JsonEncode(serde_json::Error),

    /// JSON decoding of the response body failed.
    #[error("json decode: {source}; raw body: {body}")]
    JsonDecode {
        source: serde_json::Error,
        body: String,
    },
}

impl RequestError {
    #[cfg(feature = "impersonate")]
    fn transport_wreq(e: wreq::Error) -> Self {
        Self::TransportWreq(e.to_string())
    }

    /// Quick check: did the upstream return 401/403?
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::HttpStatus { status, .. } if *status == 401 || *status == 403)
    }

    /// If this is an HTTP status error, return the status code.
    pub fn status_code(&self) -> Option<u16> {
        if let Self::HttpStatus { status, .. } = self {
            Some(*status)
        } else {
            None
        }
    }
}

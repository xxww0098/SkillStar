//! Small request builder shared by the quota / OAuth fetchers.
//!
//! Wraps the *small* surface area that fetchers actually need (GET / POST,
//! header, bearer-auth, JSON body, form body, parsed response) so each
//! fetcher writes its logic once and gets uniform error mapping —
//! [`RequestError::HttpStatus`] carries the status code, so a fetcher can
//! tell 401 from any other failure without touching `reqwest` directly.
//!
//! ## Example
//!
//! ```no_run
//! use skillstar_usage::request::Req;
//!
//! # #[derive(serde::Deserialize)] struct Balance { total: f64 }
//! # async fn run() -> anyhow::Result<()> {
//! let client = skillstar_usage::http_client::usage_http_client()?;
//! let body: Balance = Req::get(&client, "https://api.deepseek.com/user/balance")
//!     .bearer("sk-xxx")
//!     .header("Accept", "application/json")
//!     .send_json()
//!     .await?;
//! println!("{}", body.total);
//! # Ok(())
//! # }
//! ```

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;
use thiserror::Error;

/// HTTP verbs the fetchers actually use.
#[derive(Debug, Clone, Copy)]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Clone)]
enum Body {
    None,
    Json(serde_json::Value),
    Form(Vec<(String, String)>),
    Raw(String, &'static str), // (body, content_type)
}

/// Fluent request builder over the shared proxy-aware `reqwest::Client`.
#[derive(Debug, Clone)]
pub struct Req<'c> {
    client: &'c reqwest::Client,
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    bearer: Option<String>,
    body: Body,
    timeout: Option<Duration>,
}

impl<'c> Req<'c> {
    /// Start a new request. Most fetchers use [`Req::get`] / [`Req::post`].
    pub fn new(client: &'c reqwest::Client, method: Method, url: impl Into<String>) -> Self {
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

    pub fn get(client: &'c reqwest::Client, url: impl Into<String>) -> Self {
        Self::new(client, Method::Get, url)
    }

    pub fn post(client: &'c reqwest::Client, url: impl Into<String>) -> Self {
        Self::new(client, Method::Post, url)
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
        use reqwest::Method as M;
        let m = match self.method {
            Method::Get => M::GET,
            Method::Post => M::POST,
        };
        let mut req = self.client.request(m, &self.url);
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

    /// 401 only. **403 is deliberately excluded**: Cloudflare and regional
    /// blocks answer 403 with a perfectly valid credential, and calling that
    /// "login expired" latches `requires_reauth` on accounts that have nothing
    /// to re-authorize (API-key providers cannot re-authorize at all).
    pub fn is_auth_error(&self) -> bool {
        self.status == 401
    }

    /// 429 / 5xx — the provider is busy or broken, the credential is fine.
    pub fn is_transient(&self) -> bool {
        self.status == 429 || (500..600).contains(&self.status)
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
    /// Quick check: did the upstream return 401?
    ///
    /// **403 is deliberately not an auth error here.** Cloudflare / WAF /
    /// regional blocks all answer 403 to a perfectly valid credential, and the
    /// four API-key fetchers that consume this used to turn that into "登录已
    /// 失效，请重新授权" — advice they cannot even act on, since an API key has
    /// no re-authorization flow.
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Self::HttpStatus { status, .. } if *status == 401)
    }

    /// 429 / 5xx / transport failure: retrying later is plausible.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Transport(_) => true,
            Self::HttpStatus { status, .. } => *status == 429 || (500..600).contains(status),
            _ => false,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn status(code: u16) -> RequestError {
        RequestError::HttpStatus {
            status: code,
            body: "{}".into(),
        }
    }

    #[test]
    fn only_401_counts_as_an_auth_failure() {
        assert!(status(401).is_auth_error());
        for code in [403, 404, 429, 500, 503] {
            assert!(
                !status(code).is_auth_error(),
                "{code} must not latch requires_reauth"
            );
        }
    }

    #[test]
    fn rate_limit_and_server_errors_are_transient() {
        for code in [429, 500, 502, 503] {
            assert!(status(code).is_transient(), "{code}");
        }
        for code in [400, 401, 403, 404] {
            assert!(!status(code).is_transient(), "{code}");
        }
    }

    #[test]
    fn resp_helpers_agree_with_the_error_classification() {
        let forbidden = Resp {
            status: 403,
            body: String::new(),
        };
        assert!(!forbidden.is_auth_error());
        assert!(!forbidden.is_transient());

        let throttled = Resp {
            status: 429,
            body: String::new(),
        };
        assert!(!throttled.is_auth_error());
        assert!(throttled.is_transient());
    }
}

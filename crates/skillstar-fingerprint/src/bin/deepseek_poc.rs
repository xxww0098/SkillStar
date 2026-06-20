//! PoC: query DeepSeek `/user/balance` through multiple fingerprints.
//!
//! Demonstrates the end-to-end pattern an existing fetcher follows to
//! become fingerprint-aware:
//!
//! 1. Build a [`FingerprintAwareClient`] from a [`DeviceFingerprint`].
//! 2. Dispatch the request through the right backend (reqwest vs wreq).
//! 3. Parse the response identically regardless of backend.
//!
//! Run with:
//!
//! ```bash
//! DEEPSEEK_API_KEY=sk-... cargo run -p skillstar-fingerprint \
//!     --features impersonate --bin deepseek_poc
//! ```
//!
//! Without a key the PoC still demonstrates the HTTP plumbing — you'll
//! just see HTTP 401 across every fingerprint.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use skillstar_fingerprint::{DeviceFingerprint, FingerprintAwareClient, build_client};

const ENDPOINT: &str = "https://api.deepseek.com/user/balance";

#[derive(Debug, Deserialize)]
struct BalanceResponse {
    #[serde(default)]
    is_available: bool,
    #[serde(default)]
    balance_infos: Vec<BalanceInfo>,
}

#[derive(Debug, Deserialize)]
struct BalanceInfo {
    #[serde(default)]
    currency: String,
    #[serde(default)]
    total_balance: String,
    #[serde(default)]
    granted_balance: String,
    #[serde(default)]
    topped_up_balance: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .without_time()
        .init();

    let api_key = std::env::var("DEEPSEEK_API_KEY").unwrap_or_default();
    let key_redacted = if api_key.is_empty() {
        "<unset, expect 401>".to_string()
    } else if api_key.len() > 8 {
        format!("{}…(redacted)", &api_key[..6])
    } else {
        "<short>".to_string()
    };

    println!("== DeepSeek Balance · multi-fingerprint PoC ==");
    println!("Endpoint : {ENDPOINT}");
    println!("API key  : {key_redacted}");
    println!();

    let fingerprints = vec![
        DeviceFingerprint::original(),
        DeviceFingerprint::generate_chrome(),
        DeviceFingerprint::generate_safari(),
    ];

    for fp in &fingerprints {
        probe(fp, &api_key).await;
        println!();
    }

    Ok(())
}

async fn probe(fp: &DeviceFingerprint, api_key: &str) {
    println!("── {} [{}] ──", fp.name, fp.tls.label());
    let client = match build_client(fp) {
        Ok(c) => c,
        Err(e) => {
            println!("  build_client error: {e:#}");
            return;
        }
    };
    println!("  backend       : {}", client.backend());

    let result = call_deepseek(&client, api_key).await;
    match result {
        Ok(Outcome::Ok(body)) => {
            let total: f64 = body
                .balance_infos
                .iter()
                .filter_map(|b| parse_decimal(&b.total_balance))
                .sum();
            println!(
                "  ✅ available={} entries={} total≈{:.4}",
                body.is_available,
                body.balance_infos.len(),
                total
            );
            for entry in &body.balance_infos {
                println!(
                    "     · {:<5} total={:>10} granted={:>10} topped_up={:>10}",
                    entry.currency,
                    entry.total_balance,
                    entry.granted_balance,
                    entry.topped_up_balance,
                );
            }
        }
        Ok(Outcome::AuthRequired(body)) => {
            println!("  🔒 HTTP 401 (auth required)");
            if !body.is_empty() {
                println!("     body: {}", truncate(&body, 200));
            }
        }
        Ok(Outcome::HttpError { status, body }) => {
            println!("  ⚠️  HTTP {status}: {}", truncate(&body, 200));
        }
        Err(e) => {
            println!("  ❌ transport error: {e:#}");
        }
    }
}

enum Outcome {
    Ok(BalanceResponse),
    AuthRequired(String),
    HttpError { status: u16, body: String },
}

async fn call_deepseek(client: &FingerprintAwareClient, api_key: &str) -> Result<Outcome> {
    match client {
        FingerprintAwareClient::Reqwest(c) => {
            let req = c
                .get(ENDPOINT)
                .header(reqwest::header::ACCEPT, "application/json");
            let req = if api_key.is_empty() {
                req
            } else {
                req.bearer_auth(api_key)
            };
            let resp = req.send().await.context("reqwest send")?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Ok(Outcome::AuthRequired(resp.text().await.unwrap_or_default()));
            }
            if !status.is_success() {
                return Ok(Outcome::HttpError {
                    status: status.as_u16(),
                    body: resp.text().await.unwrap_or_default(),
                });
            }
            let text = resp.text().await.context("reqwest body")?;
            let parsed: BalanceResponse =
                serde_json::from_str(&text).context("reqwest parse json")?;
            Ok(Outcome::Ok(parsed))
        }
        #[cfg(feature = "impersonate")]
        FingerprintAwareClient::Wreq(c) => {
            let mut req = c.get(ENDPOINT).header("Accept", "application/json");
            if !api_key.is_empty() {
                req = req.header("Authorization", format!("Bearer {api_key}"));
            }
            let resp = req.send().await.context("wreq send")?;
            let status = resp.status();
            if status == wreq::StatusCode::UNAUTHORIZED {
                return Ok(Outcome::AuthRequired(resp.text().await.unwrap_or_default()));
            }
            if !status.is_success() {
                return Ok(Outcome::HttpError {
                    status: status.as_u16(),
                    body: resp.text().await.unwrap_or_default(),
                });
            }
            let text = resp.text().await.context("wreq body")?;
            // DeepSeek sometimes wraps in a status envelope; tolerate both.
            let parsed: BalanceResponse = match serde_json::from_str::<BalanceResponse>(&text) {
                Ok(v) => v,
                Err(_) => {
                    // Try generic Value and re-deserialize from the inner shape.
                    let raw: Value = serde_json::from_str(&text).context("wreq parse json")?;
                    serde_json::from_value(raw).context("wreq normalize json")?
                }
            };
            Ok(Outcome::Ok(parsed))
        }
    }
}

fn parse_decimal(s: &str) -> Option<f64> {
    s.parse().ok()
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

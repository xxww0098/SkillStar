//! PoC: probe `https://tls.peet.ws/api/all` with several fingerprints
//! and dump the JA3/JA4/H2 it reports back.
//!
//! Run with:
//!
//! ```bash
//! cargo run -p skillstar-fingerprint --bin tls_poc
//! ```
//!
//! Optional env vars:
//!   SKILLSTAR_FP_TARGET   — override probe URL (default: tls.peet.ws)
//!   SKILLSTAR_FP_PROFILES — comma-separated list of profiles to test
//!                          (default: default,chrome,safari,firefox,edge)

use anyhow::{Context, Result};
use serde_json::Value;
use skillstar_fingerprint::{build_client, DeviceFingerprint, FingerprintAwareClient, TlsProfile};
use std::time::Duration;

const DEFAULT_TARGET: &str = "https://tls.peet.ws/api/all";

#[derive(Debug)]
struct ProbeResult {
    label: String,
    backend: &'static str,
    ja3: Option<String>,
    ja3_hash: Option<String>,
    ja4: Option<String>,
    h2_fingerprint: Option<String>,
    user_agent: Option<String>,
    ip: Option<String>,
    error: Option<String>,
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

    let target = std::env::var("SKILLSTAR_FP_TARGET").unwrap_or_else(|_| DEFAULT_TARGET.to_string());
    let requested = std::env::var("SKILLSTAR_FP_PROFILES").ok();

    let profiles = build_profile_set(requested.as_deref());

    println!("== SkillStar TLS Fingerprint PoC ==");
    println!("Target  : {target}");
    println!("Profiles: {}", profiles.len());
    println!();

    let mut results = Vec::new();
    for fp in &profiles {
        let res = probe_one(fp, &target).await;
        print_summary(&res);
        println!();
        results.push(res);
    }

    print_table(&results);
    Ok(())
}

type NamedProfileFactory = (&'static str, fn() -> DeviceFingerprint);

fn build_profile_set(filter: Option<&str>) -> Vec<DeviceFingerprint> {
    let all: Vec<NamedProfileFactory> = vec![
        ("default", || DeviceFingerprint::original()),
        ("chrome", || DeviceFingerprint::generate_chrome()),
        ("safari", || DeviceFingerprint::generate_safari()),
        ("firefox", || {
            let mut fp = DeviceFingerprint::new("Firefox 133 (probe)");
            fp.tls = TlsProfile::firefox_latest();
            fp.http.user_agent = concat!(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) ",
                "Gecko/20100101 Firefox/133.0"
            )
            .to_string();
            fp
        }),
        ("edge", || {
            let mut fp = DeviceFingerprint::new("Edge 134 (probe)");
            fp.tls = TlsProfile::edge_latest();
            fp.http.user_agent = concat!(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) ",
                "AppleWebKit/537.36 (KHTML, like Gecko) ",
                "Chrome/134.0.0.0 Safari/537.36 Edg/134.0.0.0"
            )
            .to_string();
            fp
        }),
    ];

    match filter {
        Some(csv) => {
            let want: Vec<&str> = csv.split(',').map(|s| s.trim()).collect();
            all.into_iter()
                .filter(|(k, _)| want.contains(k))
                .map(|(_, f)| f())
                .collect()
        }
        None => all.into_iter().map(|(_, f)| f()).collect(),
    }
}

async fn probe_one(fp: &DeviceFingerprint, target: &str) -> ProbeResult {
    let label = format!("{} [{}]", fp.name, fp.tls.label());
    let client = match build_client(fp) {
        Ok(c) => c,
        Err(e) => {
            return ProbeResult {
                label,
                backend: "?",
                ja3: None,
                ja3_hash: None,
                ja4: None,
                h2_fingerprint: None,
                user_agent: None,
                ip: None,
                error: Some(format!("build_client: {e:#}")),
            }
        }
    };
    let backend = client.backend();

    let body = match send_probe(&client, target).await {
        Ok(b) => b,
        Err(e) => {
            return ProbeResult {
                label,
                backend,
                ja3: None,
                ja3_hash: None,
                ja4: None,
                h2_fingerprint: None,
                user_agent: None,
                ip: None,
                error: Some(format!("{e:#}")),
            }
        }
    };

    parse_peet_response(label, backend, &body)
}

async fn send_probe(client: &FingerprintAwareClient, target: &str) -> Result<String> {
    match client {
        FingerprintAwareClient::Reqwest(c) => {
            let resp = c
                .get(target)
                .timeout(Duration::from_secs(20))
                .send()
                .await
                .context("reqwest send failed")?
                .error_for_status()
                .context("non-success status")?;
            Ok(resp.text().await.context("reqwest body read failed")?)
        }
        #[cfg(feature = "impersonate")]
        FingerprintAwareClient::Wreq(c) => {
            let resp = c
                .get(target)
                .send()
                .await
                .context("wreq send failed")?
                .error_for_status()
                .context("non-success status")?;
            Ok(resp.text().await.context("wreq body read failed")?)
        }
    }
}

fn parse_peet_response(label: String, backend: &'static str, body: &str) -> ProbeResult {
    let parsed: Result<Value, _> = serde_json::from_str(body);
    let json = match parsed {
        Ok(v) => v,
        Err(e) => {
            return ProbeResult {
                label,
                backend,
                ja3: None,
                ja3_hash: None,
                ja4: None,
                h2_fingerprint: None,
                user_agent: None,
                ip: None,
                error: Some(format!("not JSON: {e}; first 200 bytes: {}", &body[..body.len().min(200)])),
            }
        }
    };

    let tls = json.get("tls");
    let http2 = json.get("http2");
    let donate = json.get("donate");

    ProbeResult {
        label,
        backend,
        ja3: tls
            .and_then(|t| t.get("ja3"))
            .and_then(|v| v.as_str())
            .map(String::from),
        ja3_hash: tls
            .and_then(|t| t.get("ja3_hash"))
            .and_then(|v| v.as_str())
            .map(String::from),
        ja4: tls
            .and_then(|t| t.get("ja4"))
            .and_then(|v| v.as_str())
            .map(String::from),
        h2_fingerprint: http2
            .and_then(|h| h.get("akamai_fingerprint"))
            .and_then(|v| v.as_str())
            .map(String::from),
        user_agent: json
            .get("user_agent")
            .and_then(|v| v.as_str())
            .map(String::from),
        ip: donate
            .and_then(|d| d.get("ip"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                json.get("ip")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            }),
        error: None,
    }
}

fn print_summary(r: &ProbeResult) {
    println!("── {} ──", r.label);
    println!("  backend       : {}", r.backend);
    if let Some(e) = &r.error {
        println!("  ❌ error      : {e}");
        return;
    }
    if let Some(v) = &r.ja3_hash {
        println!("  ja3 hash      : {v}");
    }
    if let Some(v) = &r.ja4 {
        println!("  ja4           : {v}");
    }
    if let Some(v) = &r.h2_fingerprint {
        println!("  h2 fingerprint: {v}");
    }
    if let Some(v) = &r.user_agent {
        println!("  reported UA   : {v}");
    }
    if let Some(v) = &r.ip {
        println!("  egress IP     : {v}");
    }
    if let Some(v) = &r.ja3 {
        println!("  ja3 (raw)     : {}", truncate(v, 80));
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

fn print_table(rs: &[ProbeResult]) {
    println!("\n== JA4 comparison ==");
    for r in rs {
        let ja4 = r.ja4.as_deref().unwrap_or("-");
        println!("  {:<32} {}", r.label, ja4);
    }
    println!("\n== Akamai H2 fingerprint comparison ==");
    for r in rs {
        let h2 = r.h2_fingerprint.as_deref().unwrap_or("-");
        println!("  {:<32} {}", r.label, h2);
    }
}

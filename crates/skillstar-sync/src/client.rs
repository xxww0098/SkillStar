//! S3 client construction + connection testing.
//!
//! Builds an `aws-sdk-s3::Client` from a [`S3TargetDef`] + secret, supporting
//! custom endpoints (Cloudflare R2, MinIO, 七牛云, OSS, COS, …). Connection
//! tests use HeadBucket and report latency.

use std::time::Instant;

use aws_sdk_s3::config::Region;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::Config;
use aws_sdk_s3::Client;

use crate::progress::{NoopSink, Phase, ProgressSink, Status, event};
use crate::store::SecretStore;
use crate::types::{ConnectionTestResult, S3TargetDef};

/// Build an S3 client for the given target + resolved secret.
pub fn build_client(target: &S3TargetDef, secret: Option<String>) -> Client {
    let creds = secret.map(|s| {
        Credentials::new(
            &target.access_key_id,
            s,
            None,
            None,
            "skillstar-sync",
        )
    });

    let mut cfg = Config::builder()
        .behavior_version_latest()
        .region(Region::new(target.region.clone()))
        .force_path_style(target.force_path_style);

    if let Some(endpoint) = optional_endpoint(&target.endpoint_url) {
        cfg = cfg.endpoint_url(endpoint);
    }
    if let Some(c) = creds {
        cfg = cfg.credentials_provider(c);
    }

    Client::from_conf(cfg.build())
}

fn optional_endpoint(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Probe a target with HeadBucket. Returns latency in ms.
pub async fn test_connection<S: SecretStore>(
    target: &S3TargetDef,
    secrets: &S,
    session_id: &str,
    sink: &impl ProgressSink,
) -> Result<ConnectionTestResult, anyhow::Error> {
    sink.emit(event(session_id, Phase::Resolve, Status::Start, format!(
        "Resolving bucket '{}' at {}…",
        target.bucket,
        if target.endpoint_url.is_empty() { target.region.clone() } else { target.endpoint_url.clone() }
    )));

    let secret = secrets.get_secret(&target.id)?;
    let client = build_client(target, secret);
    let started = Instant::now();
    match client.head_bucket().bucket(&target.bucket).send().await {
        Ok(_) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            sink.emit(event(session_id, Phase::Resolve, Status::Ok, format!(
                "Bucket reachable ({latency_ms} ms)"
            )));
            Ok(ConnectionTestResult { latency_ms })
        }
        Err(err) => {
            let msg = err.to_string();
            sink.emit(event(session_id, Phase::Resolve, Status::Fail, msg.clone()));
            Err(anyhow::anyhow!(msg))
        }
    }
}

/// Convenience wrapper that uses a noop sink (for non-UI callers / tests).
pub async fn test_connection_quiet<S: SecretStore>(
    target: &S3TargetDef,
    secrets: &S,
) -> Result<ConnectionTestResult, anyhow::Error> {
    test_connection(target, secrets, "test", &NoopSink).await
}

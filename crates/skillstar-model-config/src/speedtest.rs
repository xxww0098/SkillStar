//! Endpoint speed test — concurrent HTTP HEAD requests with latency measurement.

use anyhow::Result;
use serde::Serialize;
use std::time::{Duration, Instant};

/// Result of a single endpoint latency test.
#[derive(Debug, Clone, Serialize)]
pub struct EndpointLatency {
    pub url: String,
    /// Latency in milliseconds. None if the request failed.
    pub latency: Option<u64>,
    /// HTTP status code, if available.
    pub status: Option<u16>,
    /// Error message, if the request failed.
    pub error: Option<String>,
}

/// Test multiple endpoints concurrently and return latency results.
///
/// Sends an HTTP HEAD request to each URL with the given timeout.
/// Falls back to GET if HEAD fails with 405.
pub async fn test_endpoints(
    urls: Vec<String>,
    timeout_secs: Option<u64>,
) -> Result<Vec<EndpointLatency>> {
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(8));

    let client = reqwest::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    let mut handles = Vec::with_capacity(urls.len());

    for url in urls {
        let client = client.clone();
        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let result = client.head(&url).send().await;

            match result {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    // Retry with GET if HEAD returns 405
                    if status == 405 {
                        let start2 = Instant::now();
                        match client.get(&url).send().await {
                            Ok(resp2) => EndpointLatency {
                                url,
                                latency: Some(start2.elapsed().as_millis() as u64),
                                status: Some(resp2.status().as_u16()),
                                error: None,
                            },
                            Err(e) => EndpointLatency {
                                url,
                                latency: None,
                                status: None,
                                error: Some(e.to_string()),
                            },
                        }
                    } else {
                        EndpointLatency {
                            url,
                            latency: Some(start.elapsed().as_millis() as u64),
                            status: Some(status),
                            error: None,
                        }
                    }
                }
                Err(e) => EndpointLatency {
                    url,
                    latency: None,
                    status: None,
                    error: Some(e.to_string()),
                },
            }
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(result) => results.push(result),
            Err(e) => {
                tracing::warn!("Speedtest task join error: {e}");
            }
        }
    }

    Ok(results)
}

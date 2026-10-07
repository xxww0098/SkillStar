//! JSON transport and scalar decoding shared by account endpoints.
use crate::{UsageError, UsageResult};
use serde_json::Value;

pub(super) async fn read(request: reqwest::RequestBuilder) -> UsageResult<Value> {
    let value = read_response(request).await?;
    if value.get("success") == Some(&Value::Bool(false)) {
        return Err(UsageError::Fetcher("账号接口拒绝请求".into()));
    }
    Ok(value)
}

pub(super) async fn read_response(request: reqwest::RequestBuilder) -> UsageResult<Value> {
    let response = request
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| UsageError::transport("账号", e))?;
    let status = response.status();
    if status.as_u16() == 401 {
        return Err(UsageError::AuthRequired);
    }
    if !status.is_success() {
        // Response bodies can echo credentials; never place them on account cards.
        return Err(UsageError::http_status("账号", status.as_u16(), ""));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| UsageError::Fetcher("账号响应不是有效 JSON".into()))?;
    Ok(value)
}

pub(super) fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .or_else(|| value.get("val").or(value.get("value")).and_then(number))
        .filter(|n| n.is_finite())
}

pub(super) fn stamp(value: &Value) -> Option<i64> {
    if let Some(n) = number(value).filter(|n| *n > 0.0) {
        return Some((if n > 1e12 { n / 1000.0 } else { n }) as i64);
    }
    chrono::DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|t| t.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transport_distinguishes_revocation_outage_and_business_rejection() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        let worker = std::thread::spawn(move || {
            for (status, body) in [
                (401, "secret"),
                (503, "secret"),
                (200, r#"{"success":false}"#),
                (200, r#"{"data":{}}"#),
            ] {
                let request = server
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap()
                    .unwrap();
                request
                    .respond(tiny_http::Response::from_string(body).with_status_code(status))
                    .unwrap();
            }
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        assert!(matches!(
            read(client.get(&url)).await,
            Err(UsageError::AuthRequired)
        ));
        let error = read(client.get(&url)).await.unwrap_err();
        assert!(error.is_transient());
        assert!(!error.to_string().contains("secret"));
        assert!(matches!(
            read(client.get(&url)).await,
            Err(UsageError::Fetcher(_))
        ));
        assert!(read(client.get(&url)).await.unwrap()["data"].is_object());
        worker.join().unwrap();
    }
}

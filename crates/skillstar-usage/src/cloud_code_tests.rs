use super::*;

#[test]
fn parses_antigravity_model_quota_groups() {
    let payload = json!({
        "models": {
            "claude-sonnet-4-6": {
                "displayName": "Claude Sonnet 4.6",
                "quotaInfo": { "remainingFraction": 0.25 }
            },
            "gemini-3.1-pro-high": {
                "quotaInfo": { "remainingFraction": "75%" }
            },
            "gemini-2.5-flash": {
                "quota_info": { "remaining_fraction": 1.0 }
            },
            "gemini-3.1-flash-image": {
                "displayName": "Gemini 3.1 Flash Image",
                "quotaInfo": { "remainingFraction": 0.5 }
            }
        }
    });

    let windows = parse_model_windows(&payload);

    assert_eq!(windows.len(), 4);
    assert_eq!(windows[0].label, "Claude/GPT");
    assert_eq!(windows[0].used, 75);
    assert_eq!(windows[1].label, "Gemini 3.1 Pro Series");
    assert_eq!(windows[1].used, 25);
    assert_eq!(windows[2].label, "Gemini 2.5 Flash");
    assert_eq!(windows[2].used, 0);
    assert_eq!(windows[3].label, "Gemini 3.1 Flash Image");
    assert_eq!(windows[3].used, 50);
}

#[test]
fn extracts_project_id_when_load_code_assist_returns_an_object() {
    let project = json!({ "id": "projects/alpha" });

    assert_eq!(
        extract_project_id(&project).as_deref(),
        Some("projects/alpha")
    );
}

#[test]
fn keeps_new_model_ids_instead_of_dropping_their_quota() {
    let payload = json!({
        "models": {
            "gemini-pro-experimental": {
                "displayName": "Gemini Pro Experimental",
                "quotaInfo": { "remainingFraction": 0.3 }
            }
        }
    });

    let windows = parse_model_windows(&payload);

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].label, "Gemini Pro Experimental");
    assert_eq!(windows[0].used, 70);
}

#[test]
fn does_not_treat_reset_only_model_metadata_as_exhausted_quota() {
    let payload = json!({
        "models": {
            "gemini-3-pro-high": {
                "quotaInfo": { "resetTime": "2026-08-17T12:34:56Z" }
            }
        }
    });

    assert!(parse_model_windows(&payload).is_empty());
}

#[test]
fn parses_user_quota_summary_buckets_with_reset_time() {
    let payload = json!({
        "groups": [{
            "displayName": "Gemini Models",
            "buckets": [{
                "window": "5h",
                "remainingFraction": 0.4,
                "resetTime": "2026-08-17T12:34:56Z"
            }]
        }]
    });

    let windows = parse_quota_summary_windows(&payload).expect("summary groups");

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].label, "Gemini Models · 5h");
    assert_eq!(windows[0].used, 60);
    assert_eq!(windows[0].reset_at, Some(1_786_970_096));
}

#[test]
fn parses_nested_quota_summary_and_keeps_group_and_bucket_labels() {
    let payload = json!({
        "response": {
            "groups": [{
                "displayName": "Claude and GPT models",
                "buckets": [{
                    "displayName": "Five Hour Limit Remaining",
                    "window": "5h",
                    "remaining": { "remainingFraction": 0.05 }
                }]
            }]
        }
    });

    let windows = parse_quota_summary_windows(&payload).expect("nested summary groups");

    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].label, "Claude and GPT models · Five Hour Limit");
    assert_eq!(windows[0].used, 95);
    assert_eq!(windows[0].percent, Some(95));
}

#[tokio::test]
async fn supported_quota_summary_does_not_call_model_fallback() {
    let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let base = format!("http://{}", server.server_addr());
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&requests);
    let responder_server = std::sync::Arc::clone(&server);
    let responder = std::thread::spawn(move || {
        while let Ok(request) = responder_server.recv() {
            let url = request.url().to_string();
            seen.lock().unwrap().push(url.clone());
            let response = if url.ends_with(SUMMARY_PATH) {
                tiny_http::Response::from_string(
                    r#"{"groups":[{"displayName":"Five hour","buckets":[{"window":"5h","remainingFraction":0.5}]}]}"#,
                )
            } else {
                tiny_http::Response::from_string("unexpected fallback").with_status_code(500)
            };
            request.respond(response).unwrap();
        }
    });

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = fetch_model_quotas_from_bases(
        &client,
        "access-token",
        "test-agent",
        &json!({}),
        &[base.as_str()],
    )
    .await;
    server.unblock();
    responder.join().unwrap();
    let windows = result.unwrap();

    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "summary success must avoid model fallback"
    );
    assert!(requests[0].ends_with(SUMMARY_PATH));
    assert_eq!(windows.len(), 1);
}

#[tokio::test]
async fn model_fallback_runs_only_for_unsupported_summary() {
    let server = std::sync::Arc::new(tiny_http::Server::http("127.0.0.1:0").unwrap());
    let base = format!("http://{}", server.server_addr());
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&requests);
    let responder_server = std::sync::Arc::clone(&server);
    let responder = std::thread::spawn(move || {
        while let Ok(request) = responder_server.recv() {
            let url = request.url().to_string();
            seen.lock().unwrap().push(url.clone());
            let response = if url.ends_with(SUMMARY_PATH) {
                tiny_http::Response::from_string("unsupported").with_status_code(404)
            } else if url.ends_with(MODELS_PATH) {
                tiny_http::Response::from_string(
                    r#"{"models":{"gemini-pro":{"displayName":"Gemini Pro","quotaInfo":{"remainingFraction":0.25}}}}"#,
                )
            } else {
                tiny_http::Response::from_string("unexpected request").with_status_code(500)
            };
            request.respond(response).unwrap();
        }
    });

    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = fetch_model_quotas_from_bases(
        &client,
        "access-token",
        "test-agent",
        &json!({}),
        &[base.as_str()],
    )
    .await;
    server.unblock();
    responder.join().unwrap();
    let windows = result.unwrap();

    let requests = requests.lock().unwrap();
    assert_eq!(
        requests.len(),
        2,
        "unsupported summary should use one model fallback"
    );
    assert!(requests[0].ends_with(SUMMARY_PATH));
    assert!(requests[1].ends_with(MODELS_PATH));
    assert_eq!(windows.len(), 1);
}

use super::*;
use crate::token_import::ImportedToken;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client")
}

fn row(host: &Host, enterprise: Option<&str>) -> Subscription {
    let imported = ImportedToken {
        display_name: host.display_name.to_string(),
        access_token: "old-access".into(),
        refresh_token: Some("old-refresh".into()),
        expires_at: None,
        oauth_account_id: Some("uid-1".into()),
        provider_state: enterprise.map(str::to_string),
        currency: None,
        oauth_region: Some(host.oauth_region.to_string()),
    };
    super::super::import::oauth_row(host, imported).expect("row")
}

#[test]
fn parser_covers_account_resource_and_enterprise_shapes() {
    let accounts = json!({
        "code": 0,
        "data": { "Response": { "Data": { "Accounts": [
            {"PackageCode": PRO[0], "PackageName": "专业版", "Status": 0,
             "CycleCapacitySizePrecise": "100", "CycleCapacityRemainPrecise": "40",
             "CycleCapacityUsedPrecise": "60", "CycleEndTime": "2026-09-01 00:00:00"},
            {"PackageCode": ADDON, "PackageName": "加量", "Status": "0",
             "CycleCapacitySizePrecise": "10", "CycleCapacityUsedPrecise": "0",
             "CycleCapacityRemainPrecise": "10"},
            {"PackageCode": "gone", "Status": 2, "CycleCapacitySizePrecise": "5",
             "CycleCapacityUsedPrecise": "1"},
            {"PackageName": "empty"}
        ]}}}
    });
    let usage = usage_from_bodies("sub", &json!({}), &json!({"data": {}}), &accounts);
    assert_eq!(usage.plan_name.as_deref(), Some("Pro"));
    let monthly = usage.monthly.expect("window");
    assert_eq!(monthly.label, "Pro");
    assert_eq!(monthly.used, 60);
    assert_eq!(monthly.total, Some(100));
    assert_eq!(monthly.percent, Some(60));
    assert!(monthly.reset_at.is_some());
    assert_eq!(monthly.breakdown.len(), 1);
    assert_eq!(monthly.breakdown[0].label, "Add-on");
    assert_eq!(monthly.breakdown[0].used, 0);
    assert_eq!(monthly.breakdown[0].total, Some(10));

    let resources = json!({"data": {"resources": [
        {"packageName": "Boost", "used": 2, "total": 8}
    ]}});
    let usage = usage_from_bodies("sub", &json!({}), &json!({}), &resources);
    assert_eq!(
        usage.monthly.as_ref().map(|window| window.label.as_str()),
        Some("Boost")
    );
    assert_eq!(usage.monthly.as_ref().map(|window| window.used), Some(2));

    let nested = json!({"data": {"data": {"Response": {"Data": {"Accounts": [
        {"PackageCode": BASE[0], "CycleCapacityRemainPrecise": "3", "CycleCapacitySizePrecise": "5"}
    ]}}}}});
    let usage = usage_from_bodies("sub", &json!({}), &json!({}), &nested);
    assert_eq!(usage.plan_name.as_deref(), Some("Basic"));
    assert_eq!(usage.monthly.as_ref().map(|window| window.used), Some(2));

    let snake = json!({"code": 0, "data": {"data": {
        "limit_num": 1000, "used_num": 250, "cycle_reset_time": "2026-09-01 00:00:00"
    }}});
    let usage = usage_from_bodies("sub", &json!({}), &json!({}), &snake);
    let monthly = usage.monthly.expect("enterprise");
    assert_eq!(monthly.label, "Enterprise");
    assert_eq!(monthly.used, 250);
    assert_eq!(monthly.total, Some(1000));
    assert!(monthly.reset_at.is_some());

    let strings = json!({"limitNum": "100", "credit": "25", "cycleResetTime": "1800000000"});
    let usage = usage_from_bodies("sub", &json!({}), &json!({}), &strings);
    let monthly = usage.monthly.expect("strings");
    assert_eq!(monthly.used, 25);
    assert_eq!(monthly.total, Some(100));
    assert_eq!(monthly.reset_at, Some(1_800_000_000));

    let unlimited = json!({"data": {"data": {"limitNum": -1, "credit": 42}}});
    let usage = usage_from_bodies("sub", &json!({}), &json!({}), &unlimited);
    let monthly = usage.monthly.expect("unlimited");
    assert_eq!(monthly.used, 42);
    assert_eq!(monthly.total, None);
    assert_eq!(monthly.percent, None);

    let payment = usage_from_bodies(
        "sub",
        &json!({"data": {"dosageNotifyZh": "额度充足"}}),
        &json!({"code": 0, "data": "Pro Plan"}),
        &accounts,
    );
    assert_eq!(payment.plan_name.as_deref(), Some("Pro Plan"));
    let object_payment = usage_from_bodies(
        "sub",
        &json!({"data": {"dosageNotifyEn": "ok"}}),
        &json!({"data": {"paymentType": "Team"}}),
        &json!({}),
    );
    assert_eq!(object_payment.plan_name.as_deref(), Some("Team"));
    assert!(object_payment.monthly.is_none());
    let dosage_only = usage_from_bodies(
        "sub",
        &json!({"data": {"dosageNotifyZh": "额度充足"}}),
        &json!({}),
        &json!({}),
    );
    assert_eq!(dosage_only.plan_name.as_deref(), Some("额度充足"));
    assert!(dosage_only.monthly.is_none());
    assert!(dosage_only.weekly.is_none());
    assert!(dosage_only.hourly.is_none());

    let root = json!({"Response": {"Data": {"Accounts": [
        {"PackageName": "Root", "used": 1, "total": 2}
    ]}}});
    assert_eq!(
        usage_from_bodies("sub", &json!({}), &json!({}), &root)
            .monthly
            .unwrap()
            .label,
        "Root"
    );

    let body = user_resource_body(Local::now());
    assert_eq!(body["PageNumber"], 1);
    assert_eq!(body["PageSize"], 100);
    assert_eq!(body["ProductCode"], "p_tcaca");
    assert_eq!(body["Status"], json!([0, 3]));
    assert!(body["PackageEndTimeRangeBegin"].as_str().is_some());
    assert!(body.get("OnlyValidPeriod").is_none());
}

#[tokio::test]
async fn quota_refresh_sends_bearer_identity_headers_and_rotates_tokens() {
    let server = super::super::http::scripted::ScriptedHttp::start(vec![
        (
            200,
            r#"{"code":0,"data":{"accessToken":"new-access","refreshToken":"new-refresh","expiresAt":1793368047,"domain":"rotated.example"}}"#.into(),
        ),
        (200, r#"{"code":0,"data":{"dosageNotifyZh":"ok"}}"#.into()),
        (200, r#"{"code":0,"data":{"paymentType":"Pro Plan"}}"#.into()),
        (
            200,
            r#"{"code":0,"data":{"Response":{"Data":{"Accounts":[
                {"PackageCode":"TCACA_code_002_AkiJS3ZHF5","Status":0,"CycleCapacitySizePrecise":"100","CycleCapacityUsedPrecise":"60","CycleCapacityRemainPrecise":"40"},
                {"PackageCode":"TCACA_code_009_0XmEQc2xOf","Status":0,"CycleCapacitySizePrecise":"10","CycleCapacityUsedPrecise":"1","CycleCapacityRemainPrecise":"9"}
            ]}}}}"#.into(),
        ),
    ]);
    let mut sub = row(&super::super::GLOBAL, Some(r#"{"domain":"team.example"}"#));
    let usage = fetch_with_client(
        &client(),
        &super::super::GLOBAL,
        &server.base,
        &mut sub,
        true,
    )
    .await
    .expect("quota");
    assert_eq!(usage.plan_name.as_deref(), Some("Pro Plan"));
    assert_eq!(usage.monthly.as_ref().map(|window| window.used), Some(60));
    assert_eq!(usage.monthly.as_ref().unwrap().breakdown.len(), 1);
    assert_eq!(
        crate::crypto::decrypt(sub.access_token_encrypted.as_deref().unwrap()),
        "new-access"
    );
    assert_eq!(
        crate::crypto::decrypt(sub.refresh_token_encrypted.as_deref().unwrap()),
        "new-refresh"
    );
    assert_eq!(sub.access_token_expires_at, Some(1_793_368_047));
    let state = Enterprise::parse(&crate::crypto::decrypt(
        sub.provider_state_encrypted.as_deref().unwrap(),
    ));
    assert_eq!(state.domain.as_deref(), Some("rotated.example"));

    let seen = server.seen();
    assert_eq!(seen.len(), 4);
    assert_eq!(seen[0].path, super::super::AUTH_REFRESH_PATH);
    assert_eq!(
        super::super::http::scripted::header(&seen[0], "x-refresh-token"),
        Some("old-refresh")
    );
    assert_eq!(
        super::super::http::scripted::header(&seen[0], "x-auth-refresh-source"),
        Some("ide-main")
    );
    assert!(seen[0].body.is_empty(), "{}", seen[0].body);
    assert_eq!(seen[1].path, super::super::DOSAGE_PATH);
    assert_eq!(seen[2].path, super::super::PAYMENT_PATH);
    assert_eq!(seen[3].path, super::super::USER_RESOURCE_PATH);
    assert!(seen[3].body.contains("p_tcaca"), "{}", seen[3].body);
    assert!(
        !seen
            .iter()
            .any(|request| request.path.contains("get-enterprise"))
    );
    for request in &seen[1..] {
        assert_eq!(request.method, "POST");
        assert_eq!(
            super::super::http::scripted::header(request, "authorization"),
            Some("Bearer new-access")
        );
        assert_eq!(
            super::super::http::scripted::header(request, "x-user-id"),
            Some("uid-1")
        );
        assert_eq!(
            super::super::http::scripted::header(request, "x-domain"),
            Some("rotated.example")
        );
        assert!(super::super::http::scripted::header(request, "x-enterprise-id").is_none());
        assert!(super::super::http::scripted::header(request, "x-tenant-id").is_none());
        assert_eq!(
            super::super::http::scripted::header(request, "user-agent"),
            Some(super::super::USER_AGENT)
        );
    }
}

#[tokio::test]
async fn enterprise_quota_uses_the_enterprise_endpoint_and_omits_blank_headers() {
    let server = super::super::http::scripted::ScriptedHttp::start(vec![
        (200, r#"{"code":0,"data":{}}"#.into()),
        (200, r#"{"code":0,"data":"Enterprise"}"#.into()),
        (
            200,
            r#"{"code":0,"data":{"data":{"limit_num":"200","credit":"50"}}}"#.into(),
        ),
    ]);
    let mut sub = row(
        &super::super::CN,
        Some(r#"{"enterpriseId":"ent-9","domain":"cn.example"}"#),
    );
    sub.oauth_account_id = None;
    let usage = fetch_with_client(&client(), &super::super::CN, &server.base, &mut sub, false)
        .await
        .expect("enterprise");
    assert_eq!(usage.plan_name.as_deref(), Some("Enterprise"));
    assert_eq!(
        usage.monthly.as_ref().map(|window| window.label.as_str()),
        Some("Enterprise")
    );
    assert_eq!(usage.monthly.as_ref().map(|window| window.used), Some(50));
    assert_eq!(
        usage.monthly.as_ref().and_then(|window| window.total),
        Some(200)
    );
    let seen = server.seen();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[2].path, super::super::ENTERPRISE_USAGE_PATH);
    assert!(
        !seen
            .iter()
            .any(|request| request.path == super::super::USER_RESOURCE_PATH)
    );
    assert_eq!(
        super::super::http::scripted::header(&seen[2], "x-enterprise-id"),
        Some("ent-9")
    );
    assert_eq!(
        super::super::http::scripted::header(&seen[2], "x-tenant-id"),
        Some("ent-9")
    );
    assert_eq!(
        super::super::http::scripted::header(&seen[2], "x-domain"),
        Some("cn.example")
    );
    assert!(super::super::http::scripted::header(&seen[2], "x-user-id").is_none());
    assert_eq!(
        super::super::http::scripted::header(&seen[0], "user-agent"),
        Some(super::super::USER_AGENT)
    );
    assert!(seen[2].body.contains("{}") || seen[2].body.contains('{'));
}

#[tokio::test]
async fn quota_classifies_auth_forbidden_and_business_code() {
    let unauthorized =
        super::super::http::scripted::ScriptedHttp::start(vec![(401, "nope".into())]);
    let mut sub = row(&super::super::GLOBAL, None);
    let auth = fetch_with_client(
        &client(),
        &super::super::GLOBAL,
        &unauthorized.base,
        &mut sub,
        false,
    )
    .await
    .expect_err("401");
    assert!(matches!(auth, UsageError::AuthRequired), "{auth:?}");

    let forbidden = super::super::http::scripted::ScriptedHttp::start(vec![(
        403,
        r#"{"code":10085,"message":"ua"}"#.into(),
    )]);
    let err = fetch_with_client(
        &client(),
        &super::super::GLOBAL,
        &forbidden.base,
        &mut sub,
        false,
    )
    .await
    .expect_err("403");
    assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
    assert!(!err.is_transient());

    let business = super::super::http::scripted::ScriptedHttp::start(vec![(
        200,
        r#"{"code":1001,"message":"denied"}"#.into(),
    )]);
    let err = fetch_with_client(
        &client(),
        &super::super::GLOBAL,
        &business.base,
        &mut sub,
        false,
    )
    .await
    .expect_err("code");
    assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
    assert!(!err.is_transient());
    assert!(err.to_string().contains("1001"), "{err}");

    let limited = super::super::http::scripted::ScriptedHttp::start(vec![(429, "slow".into())]);
    let err = fetch_with_client(
        &client(),
        &super::super::GLOBAL,
        &limited.base,
        &mut sub,
        false,
    )
    .await
    .expect_err("429");
    assert!(matches!(err, UsageError::Transient(_)), "{err:?}");
}

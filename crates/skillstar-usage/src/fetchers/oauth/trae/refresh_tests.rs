use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};
use serde_json::{Value, json};

use super::exchange::EXCHANGE_PATH;
use super::http::scripted::{self, ScriptedHttp};
use super::quota::USER_INFO_PATH;
use super::{TraeAuthState, decrypt_optional, fetch_with};
use crate::UsageError;
use crate::fetchers::oauth::common::SubscriptionBuilder;
use crate::fetchers::trae::device::{self, device_proof_message};
use crate::subscription::Subscription;
use crate::trae_platform::TraePlatformKind;

const SPKI_PREFIX: &[u8] = &[
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client")
}

fn state_json(kind: TraePlatformKind, private_pem: &str, public_pem: &str) -> String {
    TraeAuthState {
        private_pem: private_pem.to_string(),
        public_pem: public_pem.to_string(),
        client_id: kind.auth_client_id().to_string(),
        login_host: kind.default_login_host().to_string(),
        auth_domain: kind.auth_domain().to_string(),
    }
    .to_json()
}

fn row(kind: TraePlatformKind, refresh: &str, private_pem: &str, public_pem: &str) -> Subscription {
    SubscriptionBuilder::new(
        kind.catalog_id(),
        kind.display_name(),
        "USD",
        "old-access",
        None,
    )
    .refresh_token(Some(refresh.to_string()))
    .provider_state(state_json(kind, private_pem, public_pem))
    .build()
}

fn exchange_count(seen: &[scripted::Seen]) -> usize {
    seen.iter()
        .filter(|item| item.path.contains("ExchangeToken"))
        .count()
}

#[tokio::test]
async fn exchange_is_one_post_and_the_device_proof_verifies() {
    let pair = device::generate_device_keypair().expect("key");
    let server = ScriptedHttp::start(vec![
        (
            200,
            json!({
                "Result": {
                    "Token": "access-new",
                    "RefreshToken": "refresh-new",
                    "TokenExpireAt": 1_800_000_000,
                    "UserID": "user-9"
                }
            })
            .to_string(),
        ),
        (
            200,
            json!({
                "Result": {
                    "UserID": "user-9",
                    "Email": "a@example.com",
                    "loginRegion": "singapore-central"
                }
            })
            .to_string(),
        ),
        (
            200,
            json!({"code": 0, "user_pay_identity_str": "pro_plus_raw"}).to_string(),
        ),
        (
            200,
            json!({
                "code": 0,
                "user_entitlement_pack_list": [
                    {"entitlement_base_info": {"product_type": 3}, "usage": {"basic_usage_amount": 99, "basic_usage_limit": 100}},
                    {
                        "entitlement_base_info": {
                            "product_type": 6,
                            "end_time": 1_800_000_000,
                            "quota": {"basic_usage_limit": 10, "bonus_usage_limit": 2}
                        },
                        "usage": {"basic_usage_amount": 1.25, "bonus_usage_amount": 0.5, "identity_str": "ignored-when-pay-has-raw"}
                    },
                    {
                        "entitlement_base_info": {"product_type": 1, "quota": {"basic_usage_limit": 1}},
                        "usage": {"basic_usage_amount": 1}
                    }
                ]
            })
            .to_string(),
        ),
    ]);
    let mut sub = row(
        TraePlatformKind::Trae,
        "refresh-old",
        &pair.private_pem,
        &pair.public_pem,
    );
    let usage = fetch_with(
        &client(),
        TraePlatformKind::Trae,
        &mut sub,
        Some(&server.base),
    )
    .await
    .expect("refresh");
    let seen = server.seen();
    assert_eq!(exchange_count(&seen), 1, "{seen:?}");
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, EXCHANGE_PATH);
    let body: Value = serde_json::from_str(&seen[0].body).unwrap();
    assert_eq!(body["ClientID"], TraePlatformKind::Trae.auth_client_id());
    assert_eq!(body["ClientSecret"], "");
    assert_eq!(body["RefreshToken"], "refresh-old");
    assert_eq!(
        body["DeviceInfo"]["DevicePublicKey"]
            .as_str()
            .unwrap()
            .trim(),
        pair.public_pem.trim()
    );
    assert_eq!(body["DeviceInfo"]["PlatformCode"], "IDE_PC");
    assert_eq!(body["IDEVersion"], "3.5.66");
    let ts = body["DeviceProof"]["Timestamp"].as_i64().unwrap();
    let nonce = body["DeviceProof"]["Nonce"].as_str().unwrap();
    let message = device_proof_message(
        "POST",
        EXCHANGE_PATH,
        TraePlatformKind::Trae.auth_client_id(),
        "refresh-old",
        ts,
        nonce,
    );
    let signature = BASE64
        .decode(body["DeviceProof"]["Signature"].as_str().unwrap())
        .unwrap();
    let der = pem_der(&pair.public_pem);
    let point = der.strip_prefix(SPKI_PREFIX).expect("spki");
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, point)
        .verify(message.as_bytes(), &signature)
        .expect("device proof");
    assert_eq!(
        scripted::header(&seen[0], "authorization"),
        Some("Bearer old-access")
    );
    assert!(seen.iter().any(|item| item.path == USER_INFO_PATH));
    assert!(
        seen.iter()
            .any(|item| item.path == "/trae/api/v1/pay/ide_user_pay_status")
    );
    assert_eq!(
        scripted::header(
            seen.iter()
                .find(|item| item.path.contains("ide_user_pay_status"))
                .unwrap(),
            "authorization"
        ),
        Some("Cloud-IDE-JWT access-new")
    );
    assert_eq!(
        decrypt_optional(&sub.access_token_encrypted).as_deref(),
        Some("access-new")
    );
    assert_eq!(
        decrypt_optional(&sub.refresh_token_encrypted).as_deref(),
        Some("refresh-new")
    );
    assert!(sub.platform_token_encrypted.is_none());
    assert_eq!(sub.oauth_account_id.as_deref(), Some("user-9"));
    assert_eq!(sub.oauth_region.as_deref(), Some("sg"));
    assert_eq!(sub.display_name, "a@example.com");
    assert_eq!(usage.plan_name.as_deref(), Some("pro_plus_raw"));
    let monthly = usage.monthly.expect("dollars");
    assert_eq!(monthly.label, "Monthly credits");
    assert_eq!(monthly.used, 125);
    assert_eq!(monthly.total, Some(1000));
    assert_eq!(monthly.reset_at, Some(1_800_000_001));
    assert_eq!(usage.credits.len(), 1);
    assert_eq!(
        usage.credits[0].credit_amount.as_deref(),
        Some("$0.50 / $2.00")
    );
}

#[tokio::test]
async fn missing_dollar_total_omits_the_window_and_a_failed_exchange_is_not_retried() {
    let pair = device::generate_device_keypair().expect("key");
    let partial = ScriptedHttp::start(vec![
        (
            200,
            json!({"Token": "access-new", "RefreshToken": "refresh-new"}).to_string(),
        ),
        (
            200,
            json!({"Result": {"Email": "a@example.com"}}).to_string(),
        ),
        (
            200,
            json!({"code": 0, "user_pay_identity_str": "raw-plan"}).to_string(),
        ),
        (
            200,
            json!({
                "code": 0,
                "user_entitlement_pack_list": [{
                    "entitlement_base_info": {"product_type": 1},
                    "usage": {"basic_usage_amount": 1.5}
                }]
            })
            .to_string(),
        ),
    ]);
    let mut sub = row(
        TraePlatformKind::Trae,
        "refresh-old",
        &pair.private_pem,
        &pair.public_pem,
    );
    let usage = fetch_with(
        &client(),
        TraePlatformKind::Trae,
        &mut sub,
        Some(&partial.base),
    )
    .await
    .expect("partial");
    assert!(usage.monthly.is_none(), "{usage:?}");
    assert!(usage.hourly.is_none());
    assert!(usage.weekly.is_none());
    assert_eq!(usage.plan_name.as_deref(), Some("raw-plan"));
    assert_eq!(exchange_count(&partial.seen()), 1);

    let denied = ScriptedHttp::start(vec![(401, "no".into()), (200, "{}".into())]);
    let mut sub = row(
        TraePlatformKind::TraeSolo,
        "refresh-old",
        &pair.private_pem,
        &pair.public_pem,
    );
    let err = fetch_with(
        &client(),
        TraePlatformKind::TraeSolo,
        &mut sub,
        Some(&denied.base),
    )
    .await
    .expect_err("401");
    assert!(matches!(err, UsageError::AuthRequired), "{err:?}");
    assert_eq!(exchange_count(&denied.seen()), 1);
    assert_eq!(
        decrypt_optional(&sub.refresh_token_encrypted).as_deref(),
        Some("refresh-old")
    );
    assert_eq!(sub.catalog_id, "trae-solo");

    let forbidden = ScriptedHttp::start(vec![(403, r#"{"error":"invalid_grant"}"#.into())]);
    let mut sub = row(
        TraePlatformKind::TraeCn,
        "refresh-old",
        &pair.private_pem,
        &pair.public_pem,
    );
    let err = fetch_with(
        &client(),
        TraePlatformKind::TraeCn,
        &mut sub,
        Some(&forbidden.base),
    )
    .await
    .expect_err("403");
    assert!(matches!(err, UsageError::Fetcher(_)), "{err:?}");
    assert!(!err.is_transient());
    assert_eq!(exchange_count(&forbidden.seen()), 1);

    let down = ScriptedHttp::start(vec![(500, "down".into()), (200, "{}".into())]);
    let mut sub = row(
        TraePlatformKind::Trae,
        "refresh-old",
        &pair.private_pem,
        &pair.public_pem,
    );
    let err = fetch_with(
        &client(),
        TraePlatformKind::Trae,
        &mut sub,
        Some(&down.base),
    )
    .await
    .expect_err("500");
    assert!(err.is_transient(), "{err:?}");
    assert_eq!(exchange_count(&down.seen()), 1);
}

#[tokio::test]
async fn quota_failure_after_exchange_keeps_the_new_refresh_token() {
    let pair = device::generate_device_keypair().expect("key");
    let server = ScriptedHttp::start(vec![
        (
            200,
            json!({"Token": "access-new", "RefreshToken": "refresh-rotated"}).to_string(),
        ),
        (500, "user".into()),
        (500, "pay-v2".into()),
        (500, "pay-v1".into()),
        (500, "usage-v2".into()),
        (500, "usage-v1".into()),
        (500, "list".into()),
    ]);
    let mut sub = row(
        TraePlatformKind::TraeCn,
        "refresh-old",
        &pair.private_pem,
        &pair.public_pem,
    );
    let usage = fetch_with(
        &client(),
        TraePlatformKind::TraeCn,
        &mut sub,
        Some(&server.base),
    )
    .await
    .expect("tokens kept");
    assert!(usage.error.is_some(), "{usage:?}");
    assert!(usage.monthly.is_none());
    assert_eq!(exchange_count(&server.seen()), 1);
    let paths: Vec<_> = server.seen().into_iter().map(|item| item.path).collect();
    assert!(
        paths
            .iter()
            .any(|path| path == "/trae/api/v2/pay/ide_user_pay_status")
    );
    assert!(
        paths
            .iter()
            .any(|path| path == "/trae/api/v1/pay/ide_user_pay_status")
    );
    assert_eq!(
        decrypt_optional(&sub.refresh_token_encrypted).as_deref(),
        Some("refresh-rotated")
    );
    assert_eq!(
        decrypt_optional(&sub.access_token_encrypted).as_deref(),
        Some("access-new")
    );
}

#[tokio::test]
async fn access_token_without_refresh_does_not_call_exchange() {
    let server = ScriptedHttp::start(vec![
        (200, json!({"Result": {"UserID": "u"}}).to_string()),
        (200, json!({"code": 0}).to_string()),
        (
            200,
            json!({"code": 0, "user_entitlement_pack_list": []}).to_string(),
        ),
    ]);
    let mut sub =
        SubscriptionBuilder::new("trae-solo-cn", "TRAE SOLO CN", "USD", "access-only", None)
            .build();
    let usage = fetch_with(
        &client(),
        TraePlatformKind::TraeSoloCn,
        &mut sub,
        Some(&server.base),
    )
    .await
    .expect("quota");
    assert_eq!(exchange_count(&server.seen()), 0);
    assert_eq!(usage.plan_name, None);
    assert!(usage.monthly.is_none());
    assert_eq!(sub.oauth_account_id.as_deref(), Some("u"));
    assert!(
        server
            .seen()
            .iter()
            .any(|item| item.path.contains("/trae/api/v2/"))
    );
}

fn pem_der(pem: &str) -> Vec<u8> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("-----"))
        .collect();
    BASE64.decode(body).expect("pem")
}

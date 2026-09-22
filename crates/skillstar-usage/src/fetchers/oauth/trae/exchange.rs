//! One `ExchangeToken` call. The refresh token is single-use: this function
//! posts exactly once and does not fall back to the legacy body.

use serde_json::{Value, json};

use super::http::{self, exchange_headers};
use super::{TraeAuthState, epoch_seconds, pick_i64, pick_string};
use crate::fetchers::trae::device::{self, DeviceProofError};
use crate::{UsageError, UsageResult};

/// Official device-proof exchange. Not the legacy `/cloudide/.../ExchangeToken`.
pub(super) const EXCHANGE_PATH: &str = "/trae/api/v3/oauth/ExchangeToken";
/// src-tauri official refresh sends an empty secret. The legacy non-proof
/// call used `"-"`; that call is not made here.
pub(super) const CLIENT_SECRET: &str = "";
pub(super) const IDE_VERSION: &str = "3.5.66";

pub(super) struct Exchanged {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub user_id: Option<String>,
    pub login_host: Option<String>,
    pub login_region: Option<String>,
}

pub(super) async fn exchange_refresh(
    client: &reqwest::Client,
    origin: &str,
    state: &TraeAuthState,
    refresh_token: &str,
    access_token: Option<&str>,
) -> UsageResult<Exchanged> {
    if !state.has_key() {
        return Err(UsageError::Fetcher(
            "Trae 设备密钥缺失，无法刷新。请用本机导入，不要只粘贴 refresh token。".into(),
        ));
    }
    let refresh_token = refresh_token.trim();
    if refresh_token.is_empty() {
        return Err(UsageError::AuthRequired);
    }
    let nonce = nonce_hex();
    let ts = chrono::Utc::now().timestamp();
    let signature = device::sign_device_proof(
        &state.private_pem,
        "POST",
        EXCHANGE_PATH,
        &state.client_id,
        refresh_token,
        ts,
        &nonce,
    )
    .map_err(proof_error)?;
    let body = json!({
        "ClientID": state.client_id,
        "ClientSecret": CLIENT_SECRET,
        "RefreshToken": refresh_token,
        "DeviceInfo": {
            "DevicePublicKey": state.public_pem,
            "PlatformCode": "IDE_PC",
            "DeviceType": "PC",
            "ClientVersion": IDE_VERSION,
        },
        "DeviceProof": {
            "Signature": signature,
            "Timestamp": ts,
            "Nonce": nonce,
        },
        "IDEVersion": IDE_VERSION,
    });
    let url = format!("{}{EXCHANGE_PATH}", origin.trim_end_matches('/'));
    let response = http::post_json(
        client,
        &url,
        &exchange_headers(access_token),
        &body,
        "Trae ExchangeToken",
    )
    .await?;
    issued(&response)
}

fn issued(response: &Value) -> UsageResult<Exchanged> {
    let access_token = pick_string(
        response,
        &[&["Token"], &["accessToken"], &["access_token"], &["token"]],
    )
    .ok_or_else(|| UsageError::Fetcher("Trae ExchangeToken 响应缺少 access token".into()))?;
    let refresh_token = pick_string(
        response,
        &[&["RefreshToken"], &["refreshToken"], &["refresh_token"]],
    );
    let expires_at = pick_i64(
        response,
        &[
            &["TokenExpireAt"],
            &["expiresAt"],
            &["expires_at"],
            &["expiredAt"],
        ],
    )
    .and_then(epoch_seconds);
    Ok(Exchanged {
        access_token,
        refresh_token,
        expires_at,
        user_id: pick_string(
            response,
            &[&["UserID"], &["userId"], &["user_id"], &["uid"]],
        ),
        login_host: pick_string(response, &[&["loginHost"], &["host"], &["Result", "Host"]]),
        login_region: pick_string(response, &[&["loginRegion"], &["userRegion"]]),
    })
}

fn nonce_hex() -> String {
    let mut bytes = [0u8; 16];
    for byte in &mut bytes {
        *byte = rand::random::<u8>();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn proof_error(err: DeviceProofError) -> UsageError {
    UsageError::Fetcher(err.to_string())
}

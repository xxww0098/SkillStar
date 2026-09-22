//! Native-app sign-in.
//!
//! One RSA-2048 keypair per attempt. The browser opens
//! `zed.dev/native_app_signin` with the loopback port and the PKCS#1 public
//! key. The callback carries `user_id` and an encrypted `access_token`.
//! [`super::super::zed_token`] decrypts it. The private key is not stored.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rsa::RsaPrivateKey;
use rsa::RsaPublicKey;
use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};

use super::quota;
use super::{CATALOG_ID, PLACEHOLDER_NAME, SIGNIN_URL};
use crate::fetchers::oauth::common::{carry_over_user_metadata, reauth_target};
use crate::oauth::local_server::{self, CallbackParams, CallbackSession};
use crate::subscription::Subscription;
use crate::token_import::ImportedToken;
use crate::{UsageError, UsageResult};

pub(crate) async fn start_login(
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let session = local_server::start_session(0, None)?;
    let port = session.port;
    let (private_der, public_key) = tokio::task::spawn_blocking(generate_keypair)
        .await
        .map_err(|error| UsageError::Other(format!("生成 Zed RSA 密钥失败：{error}")))??;
    let auth_url = build_signin_url(port, &public_key);

    let pending_id = crate::oauth::pending_state::register_with_callback_port(
        CATALOG_ID,
        None,
        auth_url.clone(),
        Some(port),
    );
    crate::oauth::pending_state::set_target_subscription_id(
        &pending_id,
        target_subscription_id.map(str::to_string),
    );

    let pid = pending_id.clone();
    tokio::spawn(async move {
        let target = crate::oauth::pending_state::target_subscription_id(&pid);
        let result = drive_login(session, private_der, target).await;
        if let Some(tx) = crate::oauth::pending_state::take_sender(&pid) {
            let _ = tx.send(result);
        }
    });

    Ok(super::super::OAuthStartInfo::browser(auth_url, pending_id))
}

pub(super) fn build_signin_url(port: u16, public_key_b64: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("native_app_port", &port.to_string());
    serializer.append_pair("native_app_public_key", public_key_b64);
    format!("{SIGNIN_URL}?{}", serializer.finish())
}

fn generate_keypair() -> UsageResult<(Vec<u8>, String)> {
    let mut rng = rand::rng();
    let private_key = RsaPrivateKey::new(&mut rng, 2048)
        .map_err(|error| UsageError::Other(format!("生成 Zed RSA 私钥失败：{error}")))?;
    let public_key = RsaPublicKey::from(&private_key);
    let private_der = private_key
        .to_pkcs1_der()
        .map_err(|error| UsageError::Other(format!("编码 Zed RSA 私钥失败：{error}")))?;
    let public_der = public_key
        .to_pkcs1_der()
        .map_err(|error| UsageError::Other(format!("编码 Zed RSA 公钥失败：{error}")))?;
    Ok((
        private_der.as_bytes().to_vec(),
        URL_SAFE_NO_PAD.encode(public_der.as_bytes()),
    ))
}

pub(super) fn credentials_from_callback(
    private_der: &[u8],
    params: &CallbackParams,
) -> UsageResult<(String, String)> {
    let user_id = required_param(params, "user_id")?;
    let ciphertext = required_param(params, "access_token")?;
    let access_token =
        crate::fetchers::oauth::zed_token::decrypt_zed_token(private_der, &ciphertext)
            .map_err(|error| UsageError::Other(format!("解密 Zed access_token 失败：{error}")))?;
    if access_token.trim().is_empty() {
        return Err(UsageError::Other("Zed access_token 为空".into()));
    }
    Ok((user_id, access_token))
}

fn required_param(params: &CallbackParams, key: &str) -> UsageResult<String> {
    params
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UsageError::Other(format!("Zed 回调缺少 {key}")))
}

async fn drive_login(
    session: CallbackSession,
    private_der: Vec<u8>,
    target_subscription_id: Option<String>,
) -> UsageResult<Subscription> {
    let params = local_server::wait_for_keys(session, &["user_id", "access_token"], None).await?;
    let (user_id, access_token) = credentials_from_callback(&private_der, &params)?;
    crate::refresh_guard::with_catalog_lock(CATALOG_ID, || async {
        finalize(user_id, access_token, target_subscription_id.as_deref()).await
    })
    .await?
}

async fn finalize(
    user_id: String,
    access_token: String,
    target_subscription_id: Option<&str>,
) -> UsageResult<Subscription> {
    let imported = ImportedToken {
        display_name: user_id.clone(),
        access_token,
        refresh_token: None,
        expires_at: None,
        oauth_account_id: Some(user_id),
        provider_state: None,
        currency: None,
        oauth_region: None,
    };
    let mut sub = super::import::oauth_row_from_imported(imported)?;
    if let Some(existing) = reauth_target(CATALOG_ID, target_subscription_id) {
        carry_over_user_metadata(&mut sub, &existing, &[PLACEHOLDER_NAME]);
    }
    if let Some(user_id) = sub.oauth_account_id.clone() {
        let plain = crate::crypto::decrypt(sub.access_token_encrypted.as_deref().unwrap_or(""));
        if !plain.is_empty()
            && let Ok(client) = crate::fetchers::http_client()
            && let Ok(body) = quota::fetch_me(&client, super::CLOUD_BASE, &user_id, &plain).await
        {
            quota::apply_profile(&mut sub, &body);
            let usage = quota::usage_from_body(&sub.id, &body);
            crate::storage::save_usage_snapshot(usage).ok();
        }
    }
    crate::storage::upsert_subscription(sub)
        .map_err(|error| UsageError::Other(format!("Zed 订阅保存失败：{error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::fetchers::oauth::OAuthFlow;
    use rsa::Oaep;
    use sha2::Sha256;

    #[test]
    fn signin_url_carries_the_port_and_public_key() {
        let url = build_signin_url(1455, "abc-DEF_09");
        assert!(url.starts_with("https://zed.dev/native_app_signin?"));
        assert!(url.contains("native_app_port=1455"), "{url}");
        assert!(url.contains("native_app_public_key=abc-DEF_09"), "{url}");
        assert!(!url.contains("billing"), "{url}");
        assert!(!url.contains("frontend"), "{url}");
    }

    #[test]
    fn callback_map_decrypts_with_the_login_private_key() {
        let mut rng = rand::rng();
        let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa");
        let public_key = RsaPublicKey::from(&private_key);
        let der = private_key.to_pkcs1_der().expect("der");
        let token = "zed-access-token";
        let encrypted = public_key
            .encrypt(&mut rng, Oaep::<Sha256>::new(), token.as_bytes())
            .expect("encrypt");
        let mut params = CallbackParams::new();
        params.insert("user_id".into(), "user-7".into());
        params.insert("access_token".into(), URL_SAFE_NO_PAD.encode(encrypted));
        let (user_id, access) =
            credentials_from_callback(der.as_bytes(), &params).expect("decrypt");
        assert_eq!(user_id, "user-7");
        assert_eq!(access, token);
    }

    #[tokio::test]
    async fn start_login_is_a_local_callback_to_native_app_signin() {
        let info = start_login(None, None).await.expect("start");
        let pending = info.pending_id.clone();
        struct Cancel(String);
        impl Drop for Cancel {
            fn drop(&mut self) {
                let _ = crate::oauth::pending_state::cancel(&self.0);
            }
        }
        let _cancel = Cancel(pending);
        assert_eq!(info.flow, OAuthFlow::LocalCallback);
        assert!(
            info.auth_url
                .starts_with("https://zed.dev/native_app_signin?"),
            "{}",
            info.auth_url
        );
        assert!(
            info.auth_url.contains("native_app_port="),
            "{}",
            info.auth_url
        );
        assert!(
            !info.auth_url.contains("native_app_port=0&")
                && !info.auth_url.ends_with("native_app_port=0"),
            "{}",
            info.auth_url
        );
        assert!(
            info.auth_url.contains("native_app_public_key="),
            "{}",
            info.auth_url
        );
        assert!(!info.auth_url.contains("billing"), "{}", info.auth_url);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

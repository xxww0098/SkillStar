//! No browser leg. "登录" adopts the local `storage.json` session in place.
//! It does not call ExchangeToken, so it does not spend the IDE's refresh token.

use super::{import, oauth_flow_home};
use crate::trae_platform::TraePlatformKind;
use crate::{UsageError, UsageResult};

pub(crate) async fn start_login(
    catalog_id: &str,
    _region: Option<&str>,
    target_subscription_id: Option<&str>,
) -> UsageResult<super::super::OAuthStartInfo> {
    let kind = TraePlatformKind::from_catalog_id(catalog_id)
        .ok_or_else(|| crate::fetchers::unsupported(catalog_id))?;
    let target = target_subscription_id.map(str::to_string);
    let subscription = crate::refresh_guard::with_catalog_lock(kind.catalog_id(), || async move {
        adopt_local(kind, target.as_deref()).await
    })
    .await??;
    let auth_url = oauth_flow_home(kind).to_string();
    let pending_id = crate::oauth::pending_state::register_with_flow(
        kind.catalog_id(),
        None,
        auth_url.clone(),
        super::super::OAuthFlow::Immediate,
    );
    if let Some(tx) = crate::oauth::pending_state::take_sender(&pending_id) {
        let _ = tx.send(Ok(subscription));
    }
    Ok(super::super::OAuthStartInfo::immediate(
        auth_url, pending_id,
    ))
}

async fn adopt_local(
    kind: TraePlatformKind,
    target_subscription_id: Option<&str>,
) -> UsageResult<crate::subscription::Subscription> {
    let imported = import::read_local(kind).map_err(|err| {
        UsageError::Other(format!(
            "{err} Trae 没有浏览器登录，请用本机导入或粘贴带 deviceKeyPair 的凭据 JSON。"
        ))
    })?;
    let mut sub = import::oauth_row(kind, imported)?;
    if let Some(existing) =
        super::super::common::reauth_target(kind.catalog_id(), target_subscription_id)
    {
        super::super::common::carry_over_user_metadata(
            &mut sub,
            &existing,
            &["Trae", "TRAE SOLO", "Trae CN", "TRAE SOLO CN"],
        );
    }
    crate::storage::upsert_subscription(sub)
        .map_err(|err| UsageError::Other(format!("{} 订阅保存失败：{err}", kind.display_name())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetchers::oauth::OAuthFlow;

    struct Env {
        _lock: std::sync::MutexGuard<'static, ()>,
        tool: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
    }

    impl Env {
        fn sandbox(path: &std::path::Path) -> Self {
            let lock = crate::test_env_lock()
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let tool = std::env::var_os("SKILLSTAR_TOOL_SYNC_HOME");
            let data = std::env::var_os("SKILLSTAR_DATA_DIR");
            unsafe {
                std::env::set_var("SKILLSTAR_TOOL_SYNC_HOME", path);
                std::env::set_var("SKILLSTAR_DATA_DIR", path.join("data"));
            }
            Self {
                _lock: lock,
                tool,
                data,
            }
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            restore("SKILLSTAR_TOOL_SYNC_HOME", self.tool.as_deref());
            restore("SKILLSTAR_DATA_DIR", self.data.as_deref());
        }
    }

    fn restore(key: &str, prev: Option<&std::ffi::OsStr>) {
        unsafe {
            match prev {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[tokio::test]
    async fn start_login_is_immediate_local_import_and_does_not_rotate_the_refresh_token() {
        let dir = tempfile::tempdir().unwrap();
        let _guard = Env::sandbox(dir.path());
        let missing = start_login("trae", None, None).await.unwrap_err();
        let text = missing.to_string();
        assert!(text.contains("storage.json"), "{text}");
        assert!(text.contains("本机导入"), "{text}");
        assert!(!text.contains("GetLoginGuidance"), "{text}");

        let pair = crate::fetchers::trae::device::generate_device_keypair().unwrap();
        let auth = serde_json::json!({
            "accessToken": "access-local",
            "refreshToken": "refresh-local",
            "userId": "user-local",
            "email": "local@example.com",
            "loginHost": "https://api.trae.cn",
            "loginRegion": "cn",
            "authClientId": "ono9krqynydwx5",
            "authDomain": "www.trae.cn"
        });
        let device = serde_json::json!({
            "privateKeyPEM": pair.private_pem,
            "publicKeyPEM": pair.public_pem
        });
        let path = crate::tool_paths::trae_storage_path_for(TraePlatformKind::TraeCn).unwrap();
        assert!(path.starts_with(dir.path()), "{path:?}");
        super::import::write_storage(&path, &super::import::cockpit_storage(&auth, Some(&device)));
        let info = start_login("trae-cn", None, None).await.expect("login");
        assert_eq!(info.flow, OAuthFlow::Immediate);
        assert_eq!(info.auth_url, "https://www.trae.cn");
        assert!(info.user_code.is_none());
        let saved = crate::oauth::pending_state::take_receiver(&info.pending_id)
            .expect("receiver")
            .await
            .expect("send")
            .expect("subscription");
        assert_eq!(saved.catalog_id, "trae-cn");
        assert!(saved.platform_token_encrypted.is_none());
        assert_eq!(
            crate::crypto::decrypt(saved.refresh_token_encrypted.as_deref().unwrap()),
            "refresh-local"
        );
        let state: serde_json::Value = serde_json::from_str(&crate::crypto::decrypt(
            saved.provider_state_encrypted.as_deref().unwrap(),
        ))
        .unwrap();
        assert_eq!(
            state["deviceKeyPair"]["privateKeyPEM"]
                .as_str()
                .unwrap()
                .trim(),
            pair.private_pem.trim()
        );
        assert_eq!(state["loginHost"], "https://api.trae.cn");
    }
}

//! Gemini CLI 的 Google OAuth 客户端凭据解析。
//!
//! 与 Antigravity 同属 Google 安装型 client，但**不是同一个 client**：Gemini CLI
//! 用的是它自己开源仓库里那一个，本仓库不发布它的 secret。来源优先级与
//! [super::antigravity_config] 完全一致，理由也一致：
//!
//! 1. env `SKILLSTAR_GEMINI_CLI_CLIENT_ID` / `_SECRET`
//! 2. 编译期 `option_env!`（发布 CI 或本地构建注入）
//! 3. `~/.skillstar/config/gemini_cli_oauth.json`（`{client_id, client_secret}`）
//! 4. 内置公开 client id + `GOCSPX-` 占位 secret
//!
//! 占位串永不当成 secret 发出去：缺失时登录在打开浏览器之前就失败，而不是让
//! 用户走完授权才发现换不到票。**不写死 secret 是刻意的**：把它硬编码进仓库，
//! 等于把一份别人配额上的共享凭据变成公共端点。

use std::path::PathBuf;

use serde::Deserialize;

use crate::oauth::error::{OAuthError, OAuthResult};

/// Gemini CLI 自己开源仓库里的公开安装型 client id。
const BUILTIN_CLIENT_ID: &str = concat!(
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j",
    ".apps.googleusercontent.com",
);

/// 内置 client 的 secret 占位：真实值只经 env / 编译期 / 配置文件注入。
const BUILTIN_CLIENT_SECRET_PLACEHOLDER: &str = "GOCSPX-";

const CLIENT_ID_ENV: &str = "SKILLSTAR_GEMINI_CLI_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "SKILLSTAR_GEMINI_CLI_CLIENT_SECRET";
const CONFIG_FILE_NAME: &str = "gemini_cli_oauth.json";

/// 解析出来的 client 配置。
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ClientConfig {
    pub(crate) client_id: String,
    /// 没有可用的 secret 时是 None（占位串不算数）。
    client_secret: Option<String>,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ClientConfig")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl ClientConfig {
    /// 换票用的 secret；缺失时给出照着做就能修好的错误。
    pub(crate) fn require_secret(&self) -> OAuthResult<&str> {
        self.client_secret.as_deref().ok_or_else(|| {
            OAuthError::Other(format!(
                "Gemini CLI 没有可用的 OAuth client secret：请设置 {CLIENT_SECRET_ENV}，或把 client_secret 写入 {}（client_id 可同时覆盖 {CLIENT_ID_ENV}）",
                config_path().display()
            ))
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(client_id: &str, client_secret: Option<&str>) -> Self {
        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.map(str::to_string),
        }
    }
}

/// `~/.skillstar/config/gemini_cli_oauth.json`。
pub(crate) fn config_path() -> PathBuf {
    ss_core::infra::paths::config_dir().join(CONFIG_FILE_NAME)
}

/// 用户写的覆盖文件；读不到或格式不对都当作"没有覆盖"。
#[derive(Debug, Deserialize)]
struct ConfigFile {
    client_id: Option<String>,
    client_secret: Option<String>,
}

/// 这个进程该用哪个 client。每次调用都重新解析，所以运行期加进配置文件也生效。
pub(crate) fn client_config() -> ClientConfig {
    let file = read_config_file(&config_path());
    resolve(
        clean(std::env::var(CLIENT_ID_ENV).ok()),
        clean(std::env::var(CLIENT_SECRET_ENV).ok()),
        compile_time(CLIENT_ID_ENV),
        compile_time(CLIENT_SECRET_ENV),
        file,
    )
}

fn compile_time(name: &str) -> Option<String> {
    let value = match name {
        CLIENT_ID_ENV => option_env!("SKILLSTAR_GEMINI_CLI_CLIENT_ID"),
        CLIENT_SECRET_ENV => option_env!("SKILLSTAR_GEMINI_CLI_CLIENT_SECRET"),
        _ => None,
    };
    clean(value.map(str::to_string))
}

fn read_config_file(path: &std::path::Path) -> Option<ConfigFile> {
    let body = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&body).ok()
}

/// 优先级：env → 编译期 → 文件 → 内置。id 总有值，secret 可能没有。
fn resolve(
    env_id: Option<String>,
    env_secret: Option<String>,
    built_id: Option<String>,
    built_secret: Option<String>,
    file: Option<ConfigFile>,
) -> ClientConfig {
    let from_file_id = file.as_ref().and_then(|file| clean(file.client_id.clone()));
    let from_file_secret = file.and_then(|file| clean(file.client_secret));
    let client_id = clean(env_id)
        .or_else(|| clean(built_id))
        .or(from_file_id)
        .unwrap_or_else(|| BUILTIN_CLIENT_ID.to_string());
    let client_secret = clean(env_secret)
        .or_else(|| clean(built_secret))
        .or(from_file_secret)
        .filter(|secret| !is_placeholder(secret));
    ClientConfig {
        client_id,
        client_secret,
    }
}

/// 占位串（只有前缀、没有真实值）不是 secret。
fn is_placeholder(secret: &str) -> bool {
    secret.eq_ignore_ascii_case(BUILTIN_CLIENT_SECRET_PLACEHOLDER)
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::Sandbox;

    fn file(id: Option<&str>, secret: Option<&str>) -> Option<ConfigFile> {
        Some(ConfigFile {
            client_id: id.map(str::to_string),
            client_secret: secret.map(str::to_string),
        })
    }

    #[test]
    fn env_wins_over_every_other_source() {
        let config = resolve(
            Some("env-id".to_string()),
            Some("env-secret".to_string()),
            Some("built-id".to_string()),
            Some("built-secret".to_string()),
            file(Some("file-id"), Some("file-secret")),
        );
        assert_eq!(config.client_id, "env-id");
        assert_eq!(config.require_secret().expect("secret"), "env-secret");
    }

    #[test]
    fn the_config_file_is_used_before_the_builtin_client() {
        let config = resolve(
            None,
            None,
            None,
            None,
            file(Some("file-id"), Some("file-secret")),
        );
        assert_eq!(config.client_id, "file-id");
        assert_eq!(config.require_secret().expect("secret"), "file-secret");
    }

    #[test]
    fn the_builtin_client_has_an_id_but_never_a_secret() {
        let config = resolve(None, None, None, None, None);
        assert_eq!(config.client_id, BUILTIN_CLIENT_ID);
        assert!(config.client_id.ends_with(".apps.googleusercontent.com"));
        let error = config.require_secret().expect_err("no secret");
        let message = error.to_string();
        assert!(message.contains("SKILLSTAR_GEMINI_CLI_CLIENT_SECRET"), "{message}");
        assert!(message.contains("gemini_cli_oauth.json"), "{message}");
    }

    #[test]
    fn the_secret_placeholder_is_never_sent() {
        for placeholder in ["GOCSPX-", " gocspx- "] {
            let config = resolve(None, Some(placeholder.to_string()), None, None, None);
            assert_eq!(config.client_secret, None, "{placeholder:?} is not a secret");
            assert!(config.require_secret().is_err());
        }
    }

    #[test]
    fn the_debug_form_never_prints_the_secret() {
        let config = resolve(
            None,
            Some("client-secret-value".to_string()),
            None,
            None,
            None,
        );
        let printed = format!("{config:?}");
        assert!(!printed.contains("client-secret-value"), "{printed}");
        assert!(printed.contains("redacted"), "{printed}");
    }

    #[test]
    fn the_override_file_is_read_from_the_data_root() {
        let sandbox = Sandbox::new();
        let path = config_path();
        assert!(path.starts_with(sandbox.root()), "{} escaped", path.display());
        sandbox.write_file(
            &path,
            r#"{"client_id":"file-id","client_secret":"file-secret"}"#,
        );
        let parsed = read_config_file(&path).expect("config file");
        assert_eq!(parsed.client_id.as_deref(), Some("file-id"));
        assert_eq!(parsed.client_secret.as_deref(), Some("file-secret"));

        sandbox.write_file(&path, "not json");
        assert!(
            read_config_file(&path).is_none(),
            "a broken override is not an override"
        );
    }
}

//! ZCode account switching through the encrypted credential file.
//!
//! OAuth rows are written to `{zcode_home()}/v2/credentials.json`. Values use
//! `enc:v1` and `zcode_credential_key`. The key home is the OS home, or
//! `SKILLSTAR_TOOL_SYNC_HOME` when that sandbox is set — never `dataBaseDir`.
//! `zcode_home()` reads `setting.json` (not `settings.json`) for `dataBaseDir`
//! before the credentials path is chosen.
//!
//! API-key rows (`provider_state.kind = api_key`) go to `config.json`
//! `providers.builtin:{zai|bigmodel}.options.apiKey` in plaintext. That is the
//! provider file ZCode reads; it is not enc:v1. OAuth activate does not rewrite
//! `config.json`. API-key activate does not rewrite `credentials.json`.
//!
//! `setting.json` `modelProviderFamilyModes.{family}` is merged to `oauth` or
//! `apiKey` so reconcile knows which file is live. `dataBaseDir` and every
//! other settings key stay. `telemetry-state.json` / `deviceMid` is not
//! created, rotated, or deleted — a device id is not an account.
//!
//! Activate backs up each file it replaces, writes it atomically, and checks
//! the decrypt (or the plaintext API key) before the pin moves. A failed
//! read-back restores those backups and leaves the pin. Restarting ZCode is
//! not verified here.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, crypto, storage};

#[path = "zcode_store.rs"]
mod store;

use store::{
    access_key, apply_api, apply_oauth, builtin_id, commit, config_path, credential_key,
    credentials_path, display_path, is_api_key, nonempty, plain, provider_of, read_api_keys,
    read_modes, read_oauth_state, read_object, readback_error, remove_verified, settings_path,
    strip_api, strip_oauth, to_bytes, user_info_json, verify_api, verify_mode, verify_oauth,
    with_mode,
};

#[cfg(test)]
use store::os_username;

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "zcode";

const ACTIVE: &str = "oauth:active_provider";
const JWT: &str = "zcodejwttoken";
const FAMILY_MODES: &str = "modelProviderFamilyModes";
const MODE_OAUTH: &str = "oauth";
const MODE_API: &str = "apiKey";
const BUILTIN_ZAI: &str = "builtin:zai";
const BUILTIN_BIGMODEL: &str = "builtin:bigmodel";

#[cfg(test)]
const READBACK_FAIL_ENV: &str = "SKILLSTAR_ZCODE_READBACK_FAIL";

pub(super) struct Adapter;

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        CATALOG_ID
    }

    fn available(&self) -> bool {
        true
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        activate(sub_id)
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        sync(sub)
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        reconcile().map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        adopt(sub)
    }

    fn forget(&self, sub_id: &str) -> UsageResult<()> {
        forget(sub_id)
    }
}

#[derive(Clone)]
struct OauthLive {
    provider: String,
    access: String,
    refresh: Option<String>,
    jwt: String,
    user_id: Option<String>,
}

#[derive(Clone)]
struct OauthMaterial {
    provider: String,
    access: String,
    refresh: Option<String>,
    jwt: String,
    user_info: String,
}

#[derive(Clone)]
struct ApiLive {
    provider: String,
    api_key: String,
}

enum OauthState {
    Absent,
    Incomplete { provider: String },
    Session(OauthLive),
}

enum Serving {
    Absent,
    Incomplete,
    Oauth(OauthLive),
    Api(Vec<ApiLive>),
}

struct Write {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn activate(subscription_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    let path = display_path(&subscription);
    match write_subscription(&subscription) {
        Ok(outcome) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, outcome))
        }
        Err(error) => Ok((subscription, failed(&path, error))),
    }
}

fn sync(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    let path = display_path(subscription);
    write_subscription(subscription).or_else(|error| Ok(failed(&path, error)))
}

fn reconcile() -> UsageResult<CliAccountState> {
    match serving()? {
        Serving::Absent => Ok(CliAccountState::Missing),
        Serving::Incomplete => Ok(CliAccountState::Diverged),
        Serving::Oauth(live) => {
            let Some(subscription) = find_oauth(&live)? else {
                return Ok(CliAccountState::Diverged);
            };
            absorb_oauth(&subscription, &live)?;
            Ok(CliAccountState::LinkedTo {
                subscription_id: subscription.id,
            })
        }
        Serving::Api(candidates) => choose_api(&candidates),
    }
}

fn adopt(subscription: &mut Subscription) -> UsageResult<()> {
    if subscription.catalog_id != CATALOG_ID || is_api_key(subscription) {
        return Ok(());
    }
    let OauthState::Session(live) = read_oauth_state()? else {
        return Ok(());
    };
    let Ok(provider) = provider_of(subscription) else {
        return Ok(());
    };
    if provider != live.provider {
        return Ok(());
    }
    let Some(account) = nonempty(subscription.oauth_account_id.as_deref()) else {
        return Ok(());
    };
    if live.user_id.as_deref() != Some(account.as_str()) {
        return Ok(());
    }
    if tokens_match(subscription, &live) {
        return Ok(());
    }
    let updated = patched_oauth(subscription, &live);
    *subscription = storage::patch_oauth_credentials(&updated)?;
    Ok(())
}

fn forget(subscription_id: &str) -> UsageResult<()> {
    let subscription = match storage::get_subscription(subscription_id) {
        Ok(subscription) => subscription,
        Err(UsageError::NotFound(_)) => return Ok(()),
        Err(error) => return Err(error),
    };
    if subscription.catalog_id != CATALOG_ID {
        return Ok(());
    }
    if is_api_key(&subscription) {
        forget_api(&subscription)
    } else {
        forget_oauth(&subscription)
    }
}

fn write_subscription(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    if subscription.catalog_id != CATALOG_ID {
        return Err(UsageError::Other(
            "ZCode 切换收到了其它 catalog 的订阅".into(),
        ));
    }
    if is_api_key(subscription) {
        write_api_key(subscription)
    } else {
        write_oauth(subscription)
    }
}

fn write_oauth(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    let material = oauth_material(subscription)?;
    let key = credential_key();
    let credentials = credentials_path();
    let settings = settings_path();
    let mut cred_map = if credentials.is_file() {
        read_object(&credentials)?
    } else {
        Map::new()
    };
    apply_oauth(&mut cred_map, &material, &key)?;
    let settings_map = with_mode(
        if settings.is_file() {
            read_object(&settings)?
        } else {
            Map::new()
        },
        &material.provider,
        MODE_OAUTH,
    )?;
    let backup = commit(
        &[
            Write {
                path: credentials.clone(),
                bytes: to_bytes(&Value::Object(cred_map))?,
            },
            Write {
                path: settings,
                bytes: to_bytes(&Value::Object(settings_map))?,
            },
        ],
        || {
            verify_oauth(&credentials, &material, &key)?;
            verify_mode(&material.provider, MODE_OAUTH)
        },
    )?;
    Ok(succeeded(&credentials, backup.as_deref()))
}

fn write_api_key(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    let provider = provider_of(subscription)?;
    let api_key = require_api_key(subscription)?;
    let config = config_path();
    let settings = settings_path();
    let mut config_map = if config.is_file() {
        read_object(&config)?
    } else {
        Map::new()
    };
    apply_api(&mut config_map, &provider, &api_key)?;
    let settings_map = with_mode(
        if settings.is_file() {
            read_object(&settings)?
        } else {
            Map::new()
        },
        &provider,
        MODE_API,
    )?;
    let backup = commit(
        &[
            Write {
                path: config.clone(),
                bytes: to_bytes(&Value::Object(config_map))?,
            },
            Write {
                path: settings,
                bytes: to_bytes(&Value::Object(settings_map))?,
            },
        ],
        || {
            verify_api(&config, &provider, &api_key)?;
            verify_mode(&provider, MODE_API)
        },
    )?;
    Ok(succeeded(&config, backup.as_deref()))
}

fn forget_oauth(subscription: &Subscription) -> UsageResult<()> {
    let path = credentials_path();
    if !path.is_file() {
        return Ok(());
    }
    let mut map = read_object(&path)?;
    if !strip_oauth(&mut map, subscription, &credential_key())? {
        return Ok(());
    }
    if map.is_empty() {
        return remove_verified(&path);
    }
    let provider = provider_of(subscription)?;
    let bytes = to_bytes(&Value::Object(map))?;
    commit(
        &[Write {
            path: path.clone(),
            bytes,
        }],
        || {
            let map = read_object(&path)?;
            if map.contains_key(&access_key(&provider)) {
                Err(readback_error())
            } else {
                Ok(())
            }
        },
    )?;
    Ok(())
}

fn forget_api(subscription: &Subscription) -> UsageResult<()> {
    let path = config_path();
    if !path.is_file() {
        return Ok(());
    }
    let mut map = read_object(&path)?;
    if !strip_api(&mut map, subscription)? {
        return Ok(());
    }
    if map.is_empty() {
        return remove_verified(&path);
    }
    let provider = provider_of(subscription)?;
    let bytes = to_bytes(&Value::Object(map))?;
    commit(
        &[Write {
            path: path.clone(),
            bytes,
        }],
        || {
            let map = read_object(&path)?;
            let still = map
                .get("providers")
                .and_then(|value| value.get(builtin_id(&provider)))
                .and_then(|value| value.get("options"))
                .and_then(|value| value.get("apiKey"))
                .is_some();
            if still { Err(readback_error()) } else { Ok(()) }
        },
    )?;
    Ok(())
}

fn serving() -> UsageResult<Serving> {
    let modes = read_modes()?;
    let oauth = read_oauth_state()?;
    let apis = read_api_keys()?;
    let forced: Vec<ApiLive> = apis
        .iter()
        .filter(|api| modes.get(&api.provider).map(String::as_str) == Some(MODE_API))
        .cloned()
        .collect();
    match oauth {
        OauthState::Session(live) => {
            if modes.get(&live.provider).map(String::as_str) == Some(MODE_API) {
                let mine: Vec<ApiLive> = forced
                    .into_iter()
                    .filter(|api| api.provider == live.provider)
                    .collect();
                if mine.is_empty() {
                    Ok(Serving::Absent)
                } else {
                    Ok(Serving::Api(mine))
                }
            } else {
                Ok(Serving::Oauth(live))
            }
        }
        OauthState::Incomplete { provider } => {
            if modes.get(&provider).map(String::as_str) == Some(MODE_API) && !forced.is_empty() {
                Ok(Serving::Api(forced))
            } else if modes.get(&provider).map(String::as_str) != Some(MODE_API) {
                Ok(Serving::Incomplete)
            } else {
                Ok(Serving::Absent)
            }
        }
        OauthState::Absent if !forced.is_empty() => Ok(Serving::Api(forced)),
        OauthState::Absent if modes.is_empty() && !apis.is_empty() => Ok(Serving::Api(apis)),
        OauthState::Absent => Ok(Serving::Absent),
    }
}

fn choose_api(candidates: &[ApiLive]) -> UsageResult<CliAccountState> {
    let mut matches: Vec<Subscription> = storage::list_subscriptions()?
        .into_iter()
        .filter(|subscription| api_matches(subscription, candidates))
        .collect();
    if matches.is_empty() {
        return Ok(CliAccountState::Diverged);
    }
    if let Some(pin) = pinned_id()?
        && let Some(subscription) = matches.iter().find(|subscription| subscription.id == pin)
    {
        return Ok(linked(&subscription.id));
    }
    if candidates.len() == 1 || matches.len() == 1 {
        return Ok(linked(&matches.remove(0).id));
    }
    Ok(CliAccountState::Diverged)
}

fn find_oauth(live: &OauthLive) -> UsageResult<Option<Subscription>> {
    let matches: Vec<Subscription> = storage::list_subscriptions()?
        .into_iter()
        .filter(|subscription| oauth_matches(subscription, live))
        .collect();
    if matches.is_empty() {
        return Ok(None);
    }
    if let Some(pin) = pinned_id()?
        && let Some(subscription) = matches.iter().find(|subscription| subscription.id == pin)
    {
        return Ok(Some(subscription.clone()));
    }
    Ok(matches.into_iter().next())
}

fn absorb_oauth(subscription: &Subscription, live: &OauthLive) -> UsageResult<()> {
    if tokens_match(subscription, live) {
        return Ok(());
    }
    storage::patch_oauth_credentials(&patched_oauth(subscription, live))?;
    Ok(())
}

fn patched_oauth(subscription: &Subscription, live: &OauthLive) -> Subscription {
    let mut updated = subscription.clone();
    updated.access_token_encrypted = Some(crypto::encrypt(&live.access));
    updated.refresh_token_encrypted = live.refresh.as_ref().map(|value| crypto::encrypt(value));
    updated.id_token_encrypted = if live.jwt.is_empty() {
        None
    } else {
        Some(crypto::encrypt(&live.jwt))
    };
    updated
}

fn tokens_match(subscription: &Subscription, live: &OauthLive) -> bool {
    plain(&subscription.access_token_encrypted).as_deref() == Some(live.access.as_str())
        && plain(&subscription.refresh_token_encrypted) == live.refresh
        && plain(&subscription.id_token_encrypted).unwrap_or_default() == live.jwt
}

fn oauth_matches(subscription: &Subscription, live: &OauthLive) -> bool {
    if subscription.catalog_id != CATALOG_ID || is_api_key(subscription) {
        return false;
    }
    let Ok(provider) = provider_of(subscription) else {
        return false;
    };
    provider == live.provider
        && plain(&subscription.access_token_encrypted).as_deref() == Some(live.access.as_str())
        && plain(&subscription.id_token_encrypted).unwrap_or_default() == live.jwt
}

fn api_matches(subscription: &Subscription, candidates: &[ApiLive]) -> bool {
    if subscription.catalog_id != CATALOG_ID || !is_api_key(subscription) {
        return false;
    }
    let Ok(provider) = provider_of(subscription) else {
        return false;
    };
    let Some(api_key) = plain(&subscription.api_key_encrypted) else {
        return false;
    };
    candidates
        .iter()
        .any(|candidate| candidate.provider == provider && candidate.api_key == api_key)
}

fn oauth_material(subscription: &Subscription) -> UsageResult<OauthMaterial> {
    let provider = provider_of(subscription)?;
    let access = plain(&subscription.access_token_encrypted)
        .ok_or_else(|| UsageError::Other("ZCode 账号缺少 access_token，切换未生效".into()))?;
    let jwt = plain(&subscription.id_token_encrypted)
        .ok_or_else(|| UsageError::Other("ZCode 账号缺少 zcode JWT，切换未生效".into()))?;
    Ok(OauthMaterial {
        provider,
        access,
        refresh: plain(&subscription.refresh_token_encrypted),
        jwt,
        user_info: user_info_json(subscription),
    })
}

fn require_api_key(subscription: &Subscription) -> UsageResult<String> {
    let api_key = plain(&subscription.api_key_encrypted)
        .ok_or_else(|| UsageError::Other("ZCode 账号缺少 API Key，切换未生效".into()))?;
    if api_key.chars().any(char::is_whitespace) {
        return Err(UsageError::Other("ZCode API Key 不能包含空白字符".into()));
    }
    Ok(api_key)
}

fn pinned_id() -> UsageResult<Option<String>> {
    storage::get_active_subscription(CATALOG_ID)
}

fn linked(subscription_id: &str) -> CliAccountState {
    CliAccountState::LinkedTo {
        subscription_id: subscription_id.to_string(),
    }
}

fn succeeded(path: &Path, backup: Option<&Path>) -> SwitchOutcome {
    SwitchOutcome {
        tool_id: CATALOG_ID.to_string(),
        config_path: path.display().to_string(),
        backup_path: backup.map(|path| path.display().to_string()),
        keychain_updated: false,
        link_mode: None,
        success: true,
        error: None,
    }
}

fn failed(path: &Path, error: UsageError) -> SwitchOutcome {
    SwitchOutcome::fail(CATALOG_ID, path, error.to_string())
}

#[cfg(test)]
#[path = "zcode_tests.rs"]
mod tests;

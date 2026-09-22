//! Kiro account switching for the one live store.
//!
//! `~/.aws/sso/cache/kiro-auth-token.json` is a single global file, shared
//! with anything else that reads that cache. This adapter switches that file,
//! the IdC registration cockpit names with SHA-1 of the start URL, Kiro
//! `profile.json`, and the `kiro.kiroAgent` row when `state.vscdb` already
//! exists. It does not register instances. Client secrets are written only
//! to the registration file. Unrelated cache entries stay. Restarting the
//! official Kiro app is not verified here.

use std::path::{Path, PathBuf};

use serde_json::Value;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, UsageResult, storage, tool_paths, vscdb};

use super::ide::IdeCredentialAdapter;
use super::{CliAccountState, SwitchOutcome};

pub(super) const CATALOG_ID: &str = "kiro";

const PRODUCT: &str = "Kiro";
const AUTH_FILE: &str = "kiro-auth-token.json";
const USAGE_DB_KEY: &str = "kiro.kiroAgent";
const BUILDER_ID_START_URL: &str = "https://view.awsapps.com/start";
const HASH_LEN: usize = 40;

#[cfg(test)]
const READBACK_FAIL_ENV: &str = "SKILLSTAR_KIRO_READBACK_FAIL";

#[path = "kiro_store.rs"]
mod store;

use store::{
    LiveAuth, Material, absorb, auth_token_path, cache_dir, capture_paths, capture_plan,
    client_id_hash, credentials_changed, eq_opt, field, hashed_registration_path, is_client_hash,
    login_labels, matching_subscription, material_of, profile_document, profile_path, read_live,
    read_object, read_value, readback_error, readback_forced_failure, registration_document,
    registration_path, restore_all, same_account, state_db_path, success_outcome, token_document,
    usage_document, write_json,
};

pub(super) struct Adapter;

impl IdeCredentialAdapter for Adapter {
    fn catalog_id(&self) -> &'static str {
        CATALOG_ID
    }

    fn available(&self) -> bool {
        // Addressable store, not "a login file is present". A missing login
        // stays in the reconcile map as `Missing`. On a desktop host the Kiro
        // user-data dir and the AWS cache dir both resolve.
        tool_paths::kiro_data_dir().is_some()
    }

    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
        activate(sub_id)
    }

    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome> {
        sync(sub)
    }

    fn reconcile(&self) -> UsageResult<Option<CliAccountState>> {
        if !self.available() {
            return Ok(None);
        }
        reconcile().map(Some)
    }

    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()> {
        adopt_active_session(sub)
    }

    fn forget(&self, sub_id: &str) -> UsageResult<()> {
        forget_account(sub_id)
    }
}

pub(super) fn activate(subscription_id: &str) -> UsageResult<(Subscription, SwitchOutcome)> {
    let subscription = storage::get_subscription(subscription_id)?;
    match write_subscription(&subscription) {
        Ok(outcome) => {
            storage::set_active_subscription(&subscription.catalog_id, &subscription.id)?;
            Ok((subscription, outcome))
        }
        Err(error) => Ok((
            subscription,
            SwitchOutcome::fail(CATALOG_ID, &auth_token_path(), error.to_string()),
        )),
    }
}

pub(super) fn sync(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    write_subscription(subscription).or_else(|error| {
        Ok(SwitchOutcome::fail(
            CATALOG_ID,
            &auth_token_path(),
            error.to_string(),
        ))
    })
}

pub(super) fn reconcile() -> UsageResult<CliAccountState> {
    let Some(live) = read_live()? else {
        return Ok(CliAccountState::Missing);
    };
    let Some(subscription) = matching_subscription(&live)? else {
        return Ok(CliAccountState::Diverged);
    };
    let updated = absorb(&subscription, &live);
    if credentials_changed(&updated, &subscription) {
        storage::patch_oauth_credentials(&updated)?;
    }
    Ok(CliAccountState::LinkedTo {
        subscription_id: subscription.id,
    })
}

pub(super) fn adopt_active_session(subscription: &mut Subscription) -> UsageResult<()> {
    if subscription.catalog_id != CATALOG_ID {
        return Ok(());
    }
    let Some(live) = read_live()? else {
        return Ok(());
    };
    if !same_account(subscription, &live) {
        return Ok(());
    }
    let updated = absorb(subscription, &live);
    if credentials_changed(&updated, subscription) {
        *subscription = storage::patch_oauth_credentials(&updated)?;
    }
    Ok(())
}

fn forget_account(subscription_id: &str) -> UsageResult<()> {
    let Ok(subscription) = storage::get_subscription(subscription_id) else {
        return Ok(());
    };
    if subscription.catalog_id != CATALOG_ID {
        return Ok(());
    }
    let Some(live) = read_live()? else {
        return Ok(());
    };
    if !same_account(&subscription, &live) {
        return Ok(());
    }
    let material = material_of(&subscription);
    let mut steps = vec![ForgetStep::Remove(auth_token_path())];
    if let Some(path) = registration_owned_by(&material, &live) {
        steps.push(ForgetStep::Remove(path));
    }
    if let Some(path) = profile_path()
        && profile_matches(&path, &material)
    {
        steps.push(ForgetStep::Remove(path));
    }
    if let Some(path) = state_db_path().filter(|path| path.is_file()) {
        steps.push(ForgetStep::ClearUsage(path));
    }
    let backups = capture_paths(steps.iter().map(ForgetStep::path))?;
    if let Err(error) = apply_forget(&steps) {
        return Err(restore_all(&backups, error));
    }
    Ok(())
}

fn write_subscription(subscription: &Subscription) -> UsageResult<SwitchOutcome> {
    if subscription.catalog_id != CATALOG_ID {
        return Err(UsageError::Other(
            "Kiro 切换收到了其它 catalog 的订阅".into(),
        ));
    }
    let plan = plan(subscription)?;
    let backups = capture_plan(&plan)?;
    if let Err(error) = apply_and_verify(&plan) {
        return Err(restore_all(&backups, error));
    }
    Ok(success_outcome(&plan, &backups))
}

struct Plan {
    token_path: PathBuf,
    token: Value,
    registration: Option<(PathBuf, Value)>,
    profile: Option<(PathBuf, Value)>,
    database: Option<(PathBuf, String)>,
}

fn plan(subscription: &Subscription) -> UsageResult<Plan> {
    let material = material_of(subscription);
    if material.access.is_none() && material.refresh.is_none() {
        return Err(UsageError::Other(
            "Kiro 账号缺少 access_token 或 refresh_token，切换未生效".into(),
        ));
    }
    let has_client = material.client_id.is_some() && material.client_secret.is_some();
    if has_client && material.start_url.is_none() {
        return Err(UsageError::Other(
            "Kiro IdC 缺少 startUrl，无法写入客户端注册文件".into(),
        ));
    }
    let (provider, auth_method) = login_labels(&material, has_client);
    let registration = if has_client {
        let start = material.start_url.as_deref().ok_or_else(|| {
            UsageError::Other("Kiro IdC 缺少 startUrl，无法写入客户端注册文件".into())
        })?;
        let client_id = material.client_id.as_deref().ok_or_else(|| {
            UsageError::Other("Kiro IdC 缺少 clientId，无法写入客户端注册文件".into())
        })?;
        let secret = material.client_secret.as_deref().ok_or_else(|| {
            UsageError::Other("Kiro IdC 缺少 clientSecret，无法写入客户端注册文件".into())
        })?;
        let path = hashed_registration_path(start);
        Some((
            path.clone(),
            registration_document(&path, client_id, secret),
        ))
    } else {
        None
    };
    let profile = profile_path().zip(profile_document(&material, provider.as_deref()));
    let database = state_db_path().and_then(|path| {
        if !path.is_file() {
            return None;
        }
        usage_document(&material).map(|value| (path, value))
    });
    Ok(Plan {
        token_path: auth_token_path(),
        token: token_document(
            &material,
            has_client,
            provider.as_deref(),
            auth_method.as_deref(),
        ),
        registration,
        profile,
        database,
    })
}

fn apply_and_verify(plan: &Plan) -> UsageResult<()> {
    if let Some((path, value)) = &plan.registration {
        write_json(path, value)?;
    }
    write_json(&plan.token_path, &plan.token)?;
    if let Some((path, value)) = &plan.profile {
        write_json(path, value)?;
    }
    if let Some((path, value)) = &plan.database {
        vscdb::mutate_labeled_items(path, PRODUCT, &[(USAGE_DB_KEY, value.as_str())], &[])?;
    }
    verify(plan)
}

fn verify(plan: &Plan) -> UsageResult<()> {
    if readback_forced_failure() {
        return Err(readback_error());
    }
    let token = read_value(&plan.token_path)?;
    if token != plan.token {
        return Err(readback_error());
    }
    if plan.registration.is_some() {
        reject_token_secret(&token)?;
    }
    if let Some((path, expected)) = &plan.registration
        && read_value(path)? != *expected
    {
        return Err(readback_error());
    }
    if let Some((path, expected)) = &plan.profile
        && read_value(path)? != *expected
    {
        return Err(readback_error());
    }
    if let Some((path, expected)) = &plan.database
        && vscdb::read_item_string(path, USAGE_DB_KEY)?.as_deref() != Some(expected.as_str())
    {
        return Err(readback_error());
    }
    Ok(())
}

fn reject_token_secret(token: &Value) -> UsageResult<()> {
    let Some(map) = token.as_object() else {
        return Err(readback_error());
    };
    if map.contains_key("clientSecret") || map.contains_key("client_secret") {
        return Err(readback_error());
    }
    Ok(())
}

enum ForgetStep {
    Remove(PathBuf),
    ClearUsage(PathBuf),
}

impl ForgetStep {
    fn path(&self) -> &Path {
        match self {
            Self::Remove(path) | Self::ClearUsage(path) => path,
        }
    }
}

fn apply_forget(steps: &[ForgetStep]) -> UsageResult<()> {
    for step in steps {
        match step {
            ForgetStep::Remove(path) => {
                if path.exists() {
                    std::fs::remove_file(path).map_err(|err| {
                        UsageError::Other(format!("删除 {} 失败：{err}", path.display()))
                    })?;
                }
            }
            ForgetStep::ClearUsage(path) => {
                vscdb::mutate_labeled_items(path, PRODUCT, &[], &[USAGE_DB_KEY])?;
            }
        }
    }
    if auth_token_path().is_file() {
        return Err(UsageError::Other("Kiro 授权文件仍在，登出未生效".into()));
    }
    for step in steps {
        match step {
            ForgetStep::Remove(path) if path.exists() => {
                return Err(UsageError::Other(format!(
                    "Kiro 文件仍在，登出未生效：{}",
                    path.display()
                )));
            }
            ForgetStep::ClearUsage(path) => {
                if vscdb::read_item_string(path, USAGE_DB_KEY)?.is_some() {
                    return Err(UsageError::Other(
                        "Kiro state.vscdb 的 kiro.kiroAgent 仍在，登出未生效".into(),
                    ));
                }
            }
            ForgetStep::Remove(_) => {}
        }
    }
    Ok(())
}

fn registration_owned_by(material: &Material, live: &LiveAuth) -> Option<PathBuf> {
    let hash = live
        .client_id_hash
        .clone()
        .filter(|value| is_client_hash(value))
        .or_else(|| material.start_url.as_deref().map(client_id_hash))?;
    let path = registration_path(&cache_dir(), &hash)?;
    if !path.is_file() {
        return None;
    }
    let map = read_object(&path)?;
    let file_id = field(&map, &["clientId", "client_id"]);
    let file_secret = field(&map, &["clientSecret", "client_secret"]);
    let id_match = material
        .client_id
        .as_ref()
        .is_some_and(|id| file_id.as_ref() == Some(id));
    let secret_match = material
        .client_secret
        .as_ref()
        .is_some_and(|secret| file_secret.as_ref() == Some(secret));
    if id_match || secret_match {
        Some(path)
    } else {
        None
    }
}

fn profile_matches(path: &Path, material: &Material) -> bool {
    let Some(map) = read_object(path) else {
        return false;
    };
    eq_opt(
        field(&map, &["email"]).as_deref(),
        material.email.as_deref(),
    ) || eq_opt(
        field(&map, &["userId", "user_id"]).as_deref(),
        material.user_id.as_deref(),
    ) || eq_opt(
        field(&map, &["arn", "profileArn"]).as_deref(),
        material.profile_arn.as_deref(),
    ) || eq_opt(
        field(&map, &["arn"]).as_deref(),
        material.user_id.as_deref(),
    ) || eq_opt(field(&map, &["arn"]).as_deref(), material.email.as_deref())
}

#[cfg(test)]
#[path = "kiro_tests.rs"]
mod tests;

//! OAuth login flows (start/await/submit/cancel) and local credential import.

use skillstar_core::infra::error::AppError;
use skillstar_usage::fetchers;
use skillstar_usage::storage;

use super::helpers::{fill_active, map_err};
use crate::usage::dto::{OAuthStartDto, SubscriptionDto, SwitchOutcomeDto};

// ── OAuth (Phase 6 wires the real flows; v1 phase 2 returns clear errors) ─

pub async fn start_oauth_login(
    catalog_id: String,
    region: Option<String>,
    subscription_id: Option<String>,
) -> Result<OAuthStartDto, AppError> {
    let target_subscription_id = subscription_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    let info = fetchers::oauth::start_login(&catalog_id, region.as_deref(), target_subscription_id)
        .await
        .map_err(map_err)?;
    Ok(OAuthStartDto {
        pending_id: info.pending_id,
        auth_url: info.auth_url,
        flow: info.flow,
        user_code: info.user_code,
        verification_uri: info.verification_uri,
        interval_secs: info.interval_secs,
    })
}

pub async fn import_subscription_from_local(
    catalog_id: String,
) -> Result<SubscriptionDto, AppError> {
    if !skillstar_usage::local_import::local_import_supported(&catalog_id) {
        return Err(AppError::Other(format!(
            "Usage: 不支持从本地导入 {}",
            catalog_id
        )));
    }
    let sub = skillstar_usage::local_import::import_subscription_from_local(&catalog_id)
        .await
        .map_err(map_err)?;
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(fill_active(
        SubscriptionDto::from_parts(sub, usage),
        &active,
    ))
}

pub async fn await_oauth_completion(pending_id: String) -> Result<SubscriptionDto, AppError> {
    use skillstar_usage::oauth::pending_state;
    let rx = pending_state::take_receiver(&pending_id)
        .ok_or_else(|| AppError::Other("Usage: pending_id 不存在或已被取走".into()))?;
    let result = rx
        .await
        .map_err(|_| AppError::Other("Usage: OAuth 等待中断".into()))?;
    pending_state::remove(&pending_id);
    let mut sub = result.map_err(map_err)?;
    let mut switch_result = None;
    if skillstar_usage::usage_switch::oauth_completion_rewrites_live_store(&sub.catalog_id)
        && storage::get_active_subscription(&sub.catalog_id)
            .map_err(map_err)?
            .as_deref()
            == Some(sub.id.as_str())
    {
        // OAuth can rotate the pinned row. IDE adapters and xAI rewrite the
        // live store so the UI cannot say "active" over the previous token.
        let activation = skillstar_usage::usage_switch::activate_subscription(&sub.id)
            .await
            .map_err(map_err)?;
        sub = activation.subscription;
        switch_result = Some(activation.switch_result);
    }
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    let mut dto = fill_active(SubscriptionDto::from_parts(sub, usage), &active);
    dto.switch_result = switch_result.map(SwitchOutcomeDto::from);
    Ok(dto)
}

pub async fn submit_oauth_callback(
    pending_id: String,
    callback_input: String,
) -> Result<(), AppError> {
    fetchers::oauth::submit_callback(&pending_id, &callback_input)
        .await
        .map_err(map_err)
}

pub fn cancel_oauth_login(pending_id: String) -> Result<(), AppError> {
    use skillstar_usage::oauth::pending_state;
    pending_state::cancel(&pending_id).map_err(map_err)
}

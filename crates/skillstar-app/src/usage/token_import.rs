//! `import_subscription_token` — paste a credential without create/update,
//! which reject `token-import` rows.

use skillstar_core::infra::error::AppError;
use skillstar_usage::storage;

use super::dto::SubscriptionDto;
use super::service::{fill_active, map_err};

pub async fn import_subscription_token(
    catalog_id: String,
    payload: String,
    target_subscription_id: Option<String>,
) -> Result<SubscriptionDto, AppError> {
    let sub = skillstar_usage::token_import::import_subscription_from_token(
        &catalog_id,
        payload,
        target_subscription_id.as_deref(),
    )
    .await
    .map_err(map_err)?;
    let usage = storage::get_usage_snapshot(&sub.id).map_err(map_err)?;
    let active = storage::list_active_per_catalog().map_err(map_err)?;
    Ok(fill_active(
        SubscriptionDto::from_parts(sub, usage),
        &active,
    ))
}

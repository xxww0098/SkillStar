//! API-key fetchers (DeepSeek, GLM, MiniMax, Kimi).

pub mod deepseek;
pub mod glm;
pub mod kimi;
pub mod minimax;

use crate::crypto;
use crate::subscription::{Subscription, SubscriptionUsage};
use crate::{UsageError, UsageResult};

/// Dispatch an API-key refresh based on `subscription.catalog_id`.
pub async fn dispatch(subscription: &mut Subscription) -> UsageResult<SubscriptionUsage> {
    let key_cipher = subscription
        .api_key_encrypted
        .as_deref()
        .ok_or_else(|| UsageError::Other("订阅缺少 API Key".into()))?;
    let api_key = crypto::decrypt(key_cipher);
    if api_key.is_empty() {
        return Err(UsageError::Other("API Key 解密失败（已损坏或机器变化）".into()));
    }

    match subscription.catalog_id.as_str() {
        "deepseek" => deepseek::fetch(&subscription.id, &api_key).await,
        "glm" => glm::fetch(&subscription.id, &api_key).await,
        "minimax" => minimax::fetch(&subscription.id, &api_key).await,
        "kimi" => kimi::fetch(&subscription.id, &api_key).await,
        other => Err(super::unsupported(other)),
    }
}

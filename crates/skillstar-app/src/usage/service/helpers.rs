//! Helpers shared by every usage service use case: usage-error mapping with
//! proxy guidance, credential-rotation bookkeeping, catalog lookup, and the
//! active-flag DTO stamp.

use crate::usage::dto::SubscriptionDto;
use skillstar_core::config::proxy;
use skillstar_core::infra::error::AppError;
use skillstar_usage::subscription::Subscription;
use skillstar_usage::{UsageError, catalog};

// `pub(in crate::usage)`: consumed across the service submodules and by
// `usage::token_import` via the `service::{..}` re-export in `mod.rs`.
pub(in crate::usage) fn map_err(e: UsageError) -> AppError {
    let message = append_network_hint(e.to_string());
    AppError::Other(format!("Usage: {}", message))
}

/// Rotating stored credentials should drop any prior auth-expired latch so the
/// UI can refresh again instead of staying stuck on the re-auth affordance.
pub(super) fn mark_credentials_rotated(sub: &mut Subscription) {
    sub.requires_reauth = false;
    sub.cookie_session_expires_at = None;
}

pub(super) fn append_network_hint(message: String) -> String {
    if !looks_like_network_transport_error(&message) || message.contains("网络代理") {
        return message;
    }

    format!("{}。{}", message, usage_network_hint(&message))
}

fn looks_like_network_transport_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "error sending request",
        "operation timed out",
        "timed out",
        "connection refused",
        "connection reset",
        "dns",
        "failed to lookup address",
        "tcp connect error",
        "network is unreachable",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn usage_network_hint(message: &str) -> String {
    let targets = network_hint_targets(message);
    match proxy::load_config() {
        Ok(config) if config.enabled && !config.host.trim().is_empty() => format!(
            "请检查 SkillStar 网络代理（{}://{}:{}）能访问 {}，或切换到可访问这些服务的节点后重试",
            config.proxy_type.as_scheme(),
            config.host.trim(),
            config.port,
            targets
        ),
        _ => format!(
            "当前 SkillStar 网络代理未启用；如果所在网络无法直连 {}，请在设置 > 网络代理启用代理后重试",
            targets
        ),
    }
}

pub(super) fn network_hint_targets(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("auth.x.ai")
        || lower.contains("grok.com")
        || lower.contains("grok oauth")
        || lower.contains("grok token")
        || lower.contains("grok refresh")
        || lower.contains("grok billing")
    {
        return "x.ai / Grok";
    }
    "Google / GitHub 等海外服务"
}

pub(super) fn ensure_catalog(id: &str) -> Result<catalog::CatalogEntry, AppError> {
    catalog::find(id).ok_or_else(|| AppError::Other(format!("Usage: unknown catalog id `{}`", id)))
}

/// Stamp `is_active` on a DTO based on the active-per-catalog map.
pub(in crate::usage) fn fill_active(
    mut dto: SubscriptionDto,
    active: &std::collections::HashMap<String, String>,
) -> SubscriptionDto {
    dto.is_active = active
        .get(&dto.catalog_id)
        .is_some_and(|sub_id| sub_id == &dto.id);
    dto
}

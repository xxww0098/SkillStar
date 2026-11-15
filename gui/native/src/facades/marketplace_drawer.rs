//! Market detail-drawer facade — the `marketplace/detail_drawer/` slice
//! (`mod.rs` + `column.rs` + `translate.rs`) owns this file. Pre-registered
//! by the market base wave so the slice never touches a shared registry;
//! the one core read ships here so the slice starts from a live seam.
//!
//! The slice's remaining domain seams, all already covered elsewhere:
//! - `sync_marketplace_scope("skill_detail:<key>")` — the base's
//!   `Marketplace.sync_scope` (parse_scope understands the prefix);
//! - install/uninstall rides `Marketplace.install_skill` /
//!   `uninstall_skill` (the column's button reuses the page's set-installed
//!   path — `note_installed` on the GPUI controller);
//! - the translate button's stored choice and the surface switch read
//!   through `MarketplaceTranslation` (translation.rs); writing the choice
//!   (`set_description_choice`) and the live `fill` engine call belong to
//!   this slice's translate.rs.
//!
//! Contract for late responses (detail_drawer/mod.rs:5-7): a read that
//! answers after the open skill changed is dropped — keep an epoch per
//! open, the same pattern the base page's read side uses.

use nsh_core::prelude::*;
use nsh_core::JsonValue;
use ss_marketplace::snapshot::{LocalFirstResult, SnapshotStatus, get_skill_detail_local};
use ss_marketplace::MarketplaceSkillDetails;

use nsh_core::element::__private::JsonMap;

/// The market detail-drawer seam (`MarketplaceDrawer` in TSX).
#[module]
pub struct MarketplaceDrawer;

fn opt_str(value: &Option<String>) -> JsonValue {
    match value {
        Some(text) => JsonValue::String(text.clone()),
        None => JsonValue::Null,
    }
}

fn details_result(result: LocalFirstResult<MarketplaceSkillDetails>) -> JsonValue {
    let mut map = JsonMap::new();
    let mut data = JsonMap::new();
    data.insert("summary".to_owned(), opt_str(&result.data.summary));
    data.insert("readme".to_owned(), opt_str(&result.data.readme));
    data.insert(
        "weeklyInstalls".to_owned(),
        opt_str(&result.data.weekly_installs),
    );
    data.insert(
        "githubStars".to_owned(),
        match result.data.github_stars {
            Some(count) => JsonValue::from(count as u64),
            None => JsonValue::Null,
        },
    );
    data.insert("firstSeen".to_owned(), opt_str(&result.data.first_seen));
    data.insert(
        "securityAudits".to_owned(),
        JsonValue::Array(
            result
                .data
                .security_audits
                .iter()
                .map(|audit| {
                    let mut entry = JsonMap::new();
                    entry.insert("name".to_owned(), JsonValue::String(audit.name.clone()));
                    entry.insert("result".to_owned(), JsonValue::String(audit.result.clone()));
                    JsonValue::Object(entry)
                })
                .collect(),
        ),
    );
    map.insert("data".to_owned(), JsonValue::Object(data));
    map.insert(
        "snapshotStatus".to_owned(),
        JsonValue::String(
            match result.snapshot_status {
                SnapshotStatus::Fresh => "fresh",
                SnapshotStatus::Stale => "stale",
                SnapshotStatus::Seeding => "seeding",
                SnapshotStatus::Miss => "miss",
                SnapshotStatus::ErrorFallback => "error_fallback",
                SnapshotStatus::RemoteError => "remote_error",
            }
            .to_owned(),
        ),
    );
    map.insert(
        "snapshotUpdatedAt".to_owned(),
        opt_str(&result.snapshot_updated_at),
    );
    map.insert(
        "error".to_owned(),
        opt_str(&result.error),
    );
    JsonValue::Object(map)
}

#[module]
impl MarketplaceDrawer {
    /// The open column's opening read — the local skill detail for one
    /// source + name (`get_skill_detail_local`, detail_drawer/mod.rs
    /// `MarketDetailController`). Read-only local-first. Resolves
    /// `{data: {summary, readme, weeklyInstalls, githubStars, firstSeen,
    /// securityAudits: [{name, result}]}, snapshotStatus,
    /// snapshotUpdatedAt, error}`; drop the answer when the
    /// open skill changed while it was in flight.
    pub async fn skill_detail(source: String, name: String) -> Result<JsonValue, String> {
        get_skill_detail_local(&source, &name)
            .await
            .map(details_result)
            .map_err(|err| format!("{err:#}"))
    }
}

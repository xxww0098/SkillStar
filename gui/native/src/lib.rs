//! The SkillStar nsh bridge crate: what `App.tsx` imports from
//! `skillstar-gui/native`. TypeScript declarations for everything registered
//! here are generated into `.nsh/types/skillstar-gui/native.d.ts`.
//!
//! Domain facades are registered into `skillstar-gui/native` below, one
//! module per domain/slice inside `facades/`.

mod agent_icons;
mod facades;
mod popover_dismiss;
mod repo_avatars;

use nsh_core::prelude::*;

/// The crate's native module (`nsh.json` `"rust": "native"`).
#[native_module("skillstar-gui/native")]
fn native(registry: &mut Registry) {
    registry.module::<facades::marketplace::Marketplace>();
    registry.module::<facades::marketplace_drawer::MarketplaceDrawer>();
    registry.module::<facades::marketplace_publisher::MarketplacePublisher>();
    // Wave 5 (accounts): the subscriptions workbench (quota cards, handoff,
    // reconcile, add-account flows; the reset-bank seam rides accounts.rs).
    registry.module::<facades::accounts::Accounts>();
    registry.module::<facades::translation::MarketplaceTranslation>();
    // Wave 3 (my-skills base): the page's own facade plus pre-registered
    // empty slots for the detail-drawer, selection-bar and reader slices.
    registry.module::<facades::my_skills::MySkills>();
    registry.module::<facades::my_skills_drawer::MySkillsDrawer>();
    registry.module::<facades::my_skills_selection::MySkillsSelection>();
    registry.module::<facades::my_skills_reader::MySkillsReader>();
    registry.module::<facades::skill_cards::SkillCards>();
    registry.module::<facades::skill_cards_create_group::SkillCardsCreateGroup>();
    registry.module::<facades::skill_cards_import::SkillCardsImport>();
    registry.module::<facades::skill_cards_share::SkillCardsShare>();
    registry.module::<facades::skill_cards_drawer::SkillCardsDrawer>();
    // Wave 4 (async bridge foundation): `DomainJob` is exported alongside
    // `MySkillsImport` / `MySkillsGitHub` (their signatures name `DomainJob`),
    // plus the two slots of the import-modal and GitHub sign-in slices.
    // Projects page: the whole area in one facade (projects.rs).
    registry.module::<facades::projects::Projects>();
    registry.module::<facades::my_skills_import::MySkillsImport>();
    registry.module::<facades::my_skills_github::MySkillsGitHub>();
    registry.module::<facades::my_skills_channels::MySkillsChannels>();
    // Settings base wave: the settings page's own seam (prefs + proxy
    // read/write through ss-core) plus pre-registered empty slots for the
    // two parallel section slices (skill-updates/translation and
    // gh-mirror/network-doctor).
    registry.module::<facades::settings::Settings>();
    registry.module::<facades::settings_updates::SettingsUpdates>();
    registry.module::<facades::settings_network::SettingsNetwork>();
    // Wave 7 (settings sections 2): pre-registered empty slots for the
    // agent-connections/managed-skills and storage/skill-repair/about
    // section slices.
    registry.module::<facades::mcp_hub::McpHub>();
    registry.module::<facades::settings_agent_connections::SettingsAgentConnections>();
    registry.module::<facades::settings_storage::SettingsStorage>();
}

/// Product version of the repository root package (`Cargo.toml`
/// `[package].version`), parsed by `build.rs`.
pub const ROOT_VERSION: &str = env!("SKILLSTAR_ROOT_VERSION");

/// Canonical 512×512 RGBA app icon (`gui/assets/icon.png`).
/// Shared with `ss-gui-host` (macOS Dock icon) and the sidebar brand mark.
pub const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

/// Monochrome vector template icon (`gui/assets/tray-icon.svg`) for the macOS
/// menu bar and Linux status notifier item.
pub const TRAY_ICON_SVG: &[u8] = include_bytes!("../../assets/tray-icon.svg");

/// `data:image/png;base64,…` URI of [`APP_ICON_PNG`], encoded once per process
/// so `<Image>` renders in `nsh run`, `nsh test`, and the single-file binary
/// without an external `assets/` directory on disk.
pub fn brand_icon_data_uri() -> &'static str {
    static URI: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URI.get_or_init(|| encode_data_uri("data:image/png;base64,", APP_ICON_PNG))
        .as_str()
}

/// `data:image/svg+xml;base64,…` URI of [`TRAY_ICON_SVG`], rendered as an
/// automatic monochrome template image in the macOS menu bar.
pub fn tray_icon_data_uri() -> &'static str {
    static URI: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URI.get_or_init(|| encode_data_uri("data:image/svg+xml;base64,", TRAY_ICON_SVG))
        .as_str()
}

pub(crate) fn encode_data_uri(prefix: &str, bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(prefix.len() + bytes.len().div_ceil(3) * 4);
    out.push_str(prefix);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 { TABLE[((n >> 6) & 0x3f) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[(n & 0x3f) as usize] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        APP_ICON_PNG, ROOT_VERSION, TRAY_ICON_SVG, brand_icon_data_uri, tray_icon_data_uri,
    };

    /// Compile-level proof that the bridge links `ss-core`, the infra base
    /// crate. `canonical_target` is pure: a table lookup with a default, no
    /// IO, no side effects.
    #[test]
    fn links_ss_core_with_a_pure_lookup() {
        assert_eq!(
            ss_core::translation::canonical_target("not-a-language"),
            "zh-CN"
        );
    }

    /// `build.rs` parsed the repository root manifest, not this crate's own
    /// placeholder version.
    #[test]
    fn root_version_came_from_the_repository_root() {
        assert_eq!(ROOT_VERSION, "0.0.1");
    }

    /// Compile-level proof that the pre-registered deck-slice dependency
    /// links: `AgentLinkReport::served` is a pure lookup over the report
    /// the deck drawer's batch toggle will return (no IO, no side effects).
    #[test]
    fn links_ss_skills_with_a_pure_lookup() {
        let report = ss_skills::workflows::agent_links::AgentLinkReport::default();
        assert!(!report.served("claude"));
    }

    #[test]
    fn app_icon_png_round_trips_through_nsh_data_uri_decoder() {
        assert!(APP_ICON_PNG.starts_with(b"\x89PNG\r\n\x1a\n"));
        let (mime, decoded) =
            nsh_core::image::decode_data_uri(brand_icon_data_uri()).expect("valid data URI");
        assert_eq!(mime, "image/png");
        assert_eq!(decoded.as_slice(), APP_ICON_PNG);
    }

    #[test]
    fn tray_icon_svg_round_trips_through_nsh_data_uri_decoder() {
        assert!(TRAY_ICON_SVG.starts_with(b"<svg "));
        let (mime, decoded) =
            nsh_core::image::decode_data_uri(tray_icon_data_uri()).expect("valid SVG data URI");
        assert_eq!(mime, "image/svg+xml");
        assert_eq!(decoded.as_slice(), TRAY_ICON_SVG);
    }
}

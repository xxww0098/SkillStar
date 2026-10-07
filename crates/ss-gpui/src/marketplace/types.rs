//! Shared types, constants, and helper utilities for Marketplace.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::*;
use ss_core::types::skill::Skill;

pub fn icon(name: IconName, size: f32, color: u32) -> Icon {
    Icon::new(name).with_size(px(size)).text_color(rgb(color))
}

pub fn format_installs(count: u32) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else {
        format!("{count}")
    }
}

/// Description shown on a market tile. Empty and whitespace-only copy is
/// omitted — the card must not invent a placeholder paragraph.
pub fn skill_blurb(skill: &Skill) -> Option<&str> {
    if let Some(text) = skill
        .localized_description
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(text);
    }
    let text = skill.description.trim();
    if text.is_empty() { None } else { Some(text) }
}

pub fn avatar_palette(seed: &str) -> (u32, u32) {
    let mut h: u32 = 0;
    for b in seed.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as u32);
    }
    const PALETTES: &[(u32, u32, u32, u32)] = &[
        // (dark bg, dark fg, light bg, light fg)
        (0x1e3a5f, 0x60a5fa, 0xdbe7fb, 0x1d4ed8), // Blue
        (0x312e81, 0x818cf8, 0xe6e6fa, 0x4338ca), // Indigo
        (0x4c1d95, 0xa78bfa, 0xede5fb, 0x6d28d9), // Violet
        (0x581c87, 0xc084fc, 0xf4e3fb, 0x86198f), // Purple
        (0x064e3b, 0x34d399, 0xd8f0e3, 0x047857), // Emerald
        (0x134e4a, 0x2dd4bf, 0xd7efec, 0x0f766e), // Teal
        (0x155e75, 0x38bdf8, 0xd9ecf8, 0x0369a1), // Cyan
        (0x713f12, 0xfbbf24, 0xfbeecf, 0xa16207), // Amber
        (0x881337, 0xfb7185, 0xfbdfe4, 0xbe123c), // Rose
    ];
    let (dbg, dfg, lbg, lfg) = PALETTES[(h as usize) % PALETTES.len()];
    if crate::theme::is_light() {
        (lbg, lfg)
    } else {
        (dbg, dfg)
    }
}

/// Tabs match the React Marketplace (`src/pages/Marketplace.tsx`) —
/// "all" / "trending" / "hot" hit `get_leaderboard_local`, "official"
/// swaps to the publishers list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarketplaceTab {
    All,
    Trending,
    Hot,
    Official,
}

impl MarketplaceTab {
    pub const ALL: [MarketplaceTab; 4] = [
        MarketplaceTab::All,
        MarketplaceTab::Trending,
        MarketplaceTab::Hot,
        MarketplaceTab::Official,
    ];

    pub fn label(self) -> gpui_kit::SharedString {
        crate::i18n::t(match self {
            MarketplaceTab::All => "toolbar.all",
            MarketplaceTab::Trending => "marketplace.trending",
            MarketplaceTab::Hot => "marketplace.hot",
            MarketplaceTab::Official => "marketplace.official",
        })
    }

    pub fn leaderboard_category(self) -> &'static str {
        match self {
            MarketplaceTab::All => "all",
            MarketplaceTab::Trending => "trending",
            MarketplaceTab::Hot => "hot",
            MarketplaceTab::Official => "all",
        }
    }

    pub fn subtitle(self) -> gpui_kit::SharedString {
        crate::i18n::t(match self {
            MarketplaceTab::All => "marketplace.subtitleAll",
            MarketplaceTab::Trending => "marketplace.subtitleTrending",
            MarketplaceTab::Hot => "marketplace.subtitleHot",
            MarketplaceTab::Official => "marketplace.subtitleOfficial",
        })
    }
}

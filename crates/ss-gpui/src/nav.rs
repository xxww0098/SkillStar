//! Route ids and the events a capability emits when it wants the shell
//! to go somewhere else. Capabilities do not import each other's views.

use gpui_kit::*;
use ss_marketplace::OfficialPublisher;

/// Top-level navigation targets. Mirrors `NavPage` in
/// `src/types/marketplace.ts` — skills mode pages plus settings.
/// `accounts` mode is a separate App-mode switch, deferred until the
/// basic skills-mode flow exists here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NavPage {
    #[default]
    MySkills,
    Marketplace,
    SkillCards,
    Projects,
    Accounts,
    Settings,
    /// Detail page pushed by Marketplace → click a publisher. Not in the
    /// sidebar; reached only via `Shell`'s publisher subscription.
    PublisherDetail,
}

impl NavPage {
    /// Sidebar entries. `PublisherDetail` is excluded — it's a pushed
    /// detail view, not a top-level destination.
    pub const ALL: [NavPage; 6] = [
        NavPage::MySkills,
        NavPage::Marketplace,
        NavPage::SkillCards,
        NavPage::Projects,
        NavPage::Accounts,
        NavPage::Settings,
    ];

    /// Sidebar page entries for Skills mode (React `SkillsNav`). Accounts
    /// mode fills that slot with the provider menu. Settings is footer-only.
    pub const SKILLS_NAV: [NavPage; 4] = [
        NavPage::MySkills,
        NavPage::Marketplace,
        NavPage::SkillCards,
        NavPage::Projects,
    ];

    pub fn dom_id(self) -> &'static str {
        match self {
            NavPage::MySkills => "nav-skills",
            NavPage::Marketplace => "nav-market",
            NavPage::SkillCards => "nav-decks",
            NavPage::Projects => "nav-projects",
            NavPage::Accounts => "nav-accounts",
            NavPage::Settings => "nav-settings",
            NavPage::PublisherDetail => "nav-publisher",
        }
    }

    /// Rail label from `src/i18n` (`sidebar.*`).
    pub fn label(self) -> SharedString {
        crate::i18n::t(match self {
            NavPage::MySkills => "sidebar.skills",
            NavPage::Marketplace => "sidebar.market",
            NavPage::SkillCards => "sidebar.groups",
            NavPage::Projects => "sidebar.projects",
            NavPage::Accounts => "sidebar.accounts",
            NavPage::Settings => "sidebar.settings",
            NavPage::PublisherDetail => "sharedChannels.rolePublisher",
        })
    }

    /// Lucide glyph for the rail. Skills-mode picks name the object:
    /// one skill, a storefront, a deck stacked upward, a project.
    /// Collapsed mode is icon-only, so each silhouette stays distinct at 16px.
    /// `PublisherDetail` has no rail entry.
    pub fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName as I;
        match self {
            NavPage::MySkills => I::Sparkle,
            NavPage::Marketplace => I::Store,
            NavPage::SkillCards => I::GalleryVerticalEnd,
            NavPage::Projects => I::Briefcase,
            NavPage::Accounts => I::Users,
            NavPage::Settings => I::Settings,
            NavPage::PublisherDetail => I::Package,
        }
    }
}

/// Top-level app mode — mirrors `AppMode` in `src/types/marketplace.ts`.
/// Drives which nav list the rail shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AppMode {
    #[default]
    Skills,
    Accounts,
}

/// Event fired by `MarketplacePage` when a publisher row is clicked.
/// `Shell` subscribes and opens that publisher on `NavPage::PublisherDetail`.
pub struct SelectPublisher {
    pub publisher: OfficialPublisher,
}

/// Page-internal "go elsewhere" event — emitted by a capability when it
/// wants `Shell` to switch `NavPage` (empty-state CTAs, cross-page links).
/// `Shell` subscribes on each page entity and calls `set_page`.
pub struct SelectPage(pub NavPage);

/// Emitted on `MySkillsPage` when a flow there writes a deck
/// (`skill_group::create_group`): Quick-Pack and `agd-` share-code installs
/// both land here. KeepAlive pages never re-run `new`, so `Shell` forwards
/// it as a `refresh` on `SkillCardsPage` — React's react-query invalidation
/// counterpart.
pub struct GroupsChanged;

/// Emitted on `SettingsPage` after a manual agent enable or disable lands.
/// KeepAlive skill, deck, and project pages snapshot `list_profiles()` at
/// construction, so `Shell` reloads those snapshots in the same turn.
/// Waiting for an unrelated refresh leaves the skill-card carousel without
/// the new brand SVG.
pub struct AgentsChanged;

/// Emitted on `SettingsPage` after translation preferences are saved.
/// KeepAlive skill and market pages snapshot nothing from the file, so
/// `Shell` bumps their translation generation and they repaint from cache.
pub struct TranslationPrefsChanged;

//! Marketplace — 4-tab listing over `ss_marketplace::snapshot`.
//! React source: `src/pages/Marketplace.tsx` + `features/marketplace/`.
//!
//! The leaderboard is a few hundred cards. Rows go through `uniform_list`
//! so scroll and hover rebuild only the visible ones.
//!
//! Emits `SelectPublisher` when a publisher row is clicked on the
//! Official tab; `Shell` translates that into `NavPage::PublisherDetail`.

mod board;
mod card;
mod detail_drawer;
mod market_card;
mod publisher;
mod spotlight;
pub mod types;
mod view;

pub use publisher::PublisherDetailPage;

pub(crate) use card::{card_grid, market_tile, scroll_pane};
pub(crate) use market_card::render_market_card;

pub use types::*;

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_marketplace::OfficialPublisher;
use ss_marketplace::snapshot::{
    LocalFirstResult, SnapshotStatus, get_leaderboard_local, get_publishers_local, search_local,
    sync_marketplace_scope,
};

use crate::chrome::{bar_refresh_button, bar_text_chip, page_chrome, page_toolbar};
use crate::nav::SelectPublisher;
use crate::spawn_domain;
use crate::theme::palette;
pub use types::icon;

/// Per-tab fetch shape — different `LocalFirstResult` payloads per tab so
/// one `spawn_domain` call can route the result back to the right field.
enum MarketplaceData {
    Skills(anyhow::Result<LocalFirstResult<Vec<Skill>>>),
    Publishers(anyhow::Result<LocalFirstResult<Vec<OfficialPublisher>>>),
}

impl crate::DomainOutput for MarketplaceData {
    fn from_panic(panic: crate::DomainPanic) -> Self {
        Self::Skills(Err(anyhow::Error::new(panic)))
    }
}

/// Toolbar sort. `Stars` keeps leaderboard order until a search is active,
/// matching `computeDisplaySkills`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MarketSort {
    Stars,
    Updated,
}

pub struct MarketplacePage {
    tab: MarketplaceTab,
    search: Option<Entity<InputState>>,
    query: String,
    /// Unsorted fetch. `skills` is the sorted view `uniform_list` reads.
    loaded_skills: Vec<Skill>,
    skills: Vec<Skill>,
    sort: MarketSort,
    view_list: bool,
    publishers: Vec<OfficialPublisher>,
    snapshot_status: SnapshotStatus,
    snapshot_updated_at: Option<String>,
    snapshot_error: Option<String>,
    status_message: Option<String>,
    loading: bool,
    refreshing: bool,
    busy: Option<String>,
    /// Bumped on every load. A slower response from an older load is dropped.
    load_epoch: u64,
    /// Column count from the last layout width. Resize redraws only when it changes.
    columns: usize,
    /// Scroll offsets of the two virtual lists. Replaced whenever the list
    /// identity changes (tab switch, view toggle, cleared search) so the new
    /// dataset opens at the top; a tracked handle keeps its offset across
    /// re-renders, unlike the per-id element state of an untracked list.
    skills_scroll: UniformListScrollHandle,
    publishers_scroll: UniformListScrollHandle,
    detail: detail_drawer::MarketDetailController,
    _subscription: Option<Subscription>,
    _bounds_watch: Option<Subscription>,
    /// List data. Refresh-icon frames notify the page without bumping this.
    list_epoch: u64,
    translation_epoch: u64,
    board: Option<Entity<board::MarketBoard>>,
    detail_view: Option<Entity<board::MarketDetail>>,
}

const SEARCH_DEBOUNCE: Duration = Duration::from_millis(200);
/// A leaderboard sync also kicks off the description backfill in the
/// background (`snapshot::backfill_missing_descriptions`); one delayed re-read
/// after that round has had time to land picks up the new descriptions without
/// the user doing anything.
const SYNC_BACKFILL_RELOAD: Duration = Duration::from_secs(8);

impl MarketplacePage {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let this = Self {
            tab: MarketplaceTab::All,
            search: None,
            query: String::new(),
            loaded_skills: Vec::new(),
            skills: Vec::new(),
            sort: MarketSort::Stars,
            view_list: false,
            publishers: Vec::new(),
            snapshot_status: SnapshotStatus::Fresh,
            snapshot_updated_at: None,
            snapshot_error: None,
            status_message: None,
            loading: true,
            refreshing: false,
            busy: None,
            load_epoch: 0,
            columns: 1,
            skills_scroll: UniformListScrollHandle::new(),
            publishers_scroll: UniformListScrollHandle::new(),
            detail: detail_drawer::MarketDetailController::new(),
            _subscription: None,
            _bounds_watch: None,
            list_epoch: 0,
            translation_epoch: 0,
            board: None,
            detail_view: None,
        };
        Self::refresh(cx, MarketplaceTab::All, "", 0);
        this
    }

    fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            return;
        }
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("marketplace.searchPlaceholder"))
        });
        self._subscription = Some(cx.subscribe_in(
            &search,
            window,
            |this, _state, event: &InputEvent, _window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some(search) = &this.search else { return };
                let query = search.read(cx).value().to_string();
                if query == this.query {
                    return;
                }
                this.query = query;
                this.load_debounced(cx);
            },
        ));
        self.search = Some(search);
    }

    pub fn sync_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = &self.search {
            crate::i18n::sync_placeholder(
                search,
                crate::i18n::t("marketplace.searchPlaceholder"),
                window,
                cx,
            );
        }
        self.revise(cx);
    }

    pub(super) fn replay_epochs(&self) -> (u64, u64) {
        (self.list_epoch, self.translation_epoch)
    }

    /// List data changed. The toolbar spin notifies this page and must not
    /// come through here.
    pub(crate) fn revise(&mut self, cx: &mut Context<Self>) {
        self.list_epoch = self.list_epoch.wrapping_add(1);
        cx.notify();
    }

    fn load_now(&mut self, cx: &mut Context<Self>, skeleton: bool) {
        self.load_epoch = self.load_epoch.wrapping_add(1);
        if skeleton {
            self.loading = true;
            // A skeleton reload is a user navigation (tab switch, cleared
            // search): the incoming dataset opens at the top. In-place
            // reloads (sync, spotlight pick) keep the current position.
            self.skills_scroll = UniformListScrollHandle::new();
            self.publishers_scroll = UniformListScrollHandle::new();
        }
        let epoch = self.load_epoch;
        let tab = self.tab;
        let query = self.query.clone();
        Self::refresh(cx, tab, &query, epoch);
        self.revise(cx);
    }

    /// Keystrokes notify on every edit. Wait until typing pauses before
    /// hitting the snapshot, and keep the current rows on screen.
    fn load_debounced(&mut self, cx: &mut Context<Self>) {
        self.load_epoch = self.load_epoch.wrapping_add(1);
        let epoch = self.load_epoch;
        let tab = self.tab;
        let query = self.query.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                if this.load_epoch != epoch {
                    return;
                }
                if this.skills.is_empty() && this.publishers.is_empty() {
                    this.loading = true;
                }
                Self::refresh(cx, tab, &query, epoch);
                this.revise(cx);
            });
        })
        .detach();
    }

    fn refresh(cx: &mut Context<Self>, tab: MarketplaceTab, query: &str, epoch: u64) {
        let view = cx.entity();
        let query = query.trim().to_string();
        let fut = async move {
            if !query.is_empty() {
                MarketplaceData::Skills(search_local(&query, Some(50)).await)
            } else if tab == MarketplaceTab::Official {
                MarketplaceData::Publishers(get_publishers_local().await)
            } else {
                MarketplaceData::Skills(get_leaderboard_local(tab.leaderboard_category()).await)
            }
        };
        spawn_domain(&view, cx, fut, move |this, cx, result| {
            if this.load_epoch != epoch {
                return;
            }
            this.loading = false;
            match result {
                MarketplaceData::Skills(Ok(LocalFirstResult {
                    data,
                    snapshot_status,
                    snapshot_updated_at,
                    error,
                })) => {
                    this.loaded_skills = data;
                    this.apply_sort();
                    this.detail.retain(&this.skills);
                    this.publishers.clear();
                    this.snapshot_status = snapshot_status;
                    this.snapshot_updated_at = snapshot_updated_at;
                    this.snapshot_error = error;
                }
                MarketplaceData::Skills(Err(err)) => {
                    this.snapshot_error = Some(err.to_string());
                }
                MarketplaceData::Publishers(Ok(LocalFirstResult {
                    data,
                    snapshot_status,
                    snapshot_updated_at,
                    error,
                })) => {
                    this.publishers = data;
                    this.skills.clear();
                    this.loaded_skills.clear();
                    this.detail.clear();
                    this.snapshot_status = snapshot_status;
                    this.snapshot_updated_at = snapshot_updated_at;
                    this.snapshot_error = error;
                }
                MarketplaceData::Publishers(Err(err)) => {
                    this.snapshot_error = Some(err.to_string());
                }
            }
            this.revise(cx);
        });
    }

    fn sync_snapshot(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.revise(cx);
        let tab = self.tab;
        let query = self.query.clone();
        let scope = if !query.is_empty() {
            format!("search_seed:{}", query.to_ascii_lowercase())
        } else if tab == MarketplaceTab::Official {
            "official_publishers".to_string()
        } else {
            format!("leaderboard_{}", tab.leaderboard_category())
        };

        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move { sync_marketplace_scope(&scope).await },
            move |this, cx, res| {
                this.refreshing = false;
                match res {
                    Ok(_) => {
                        this.status_message = Some("Snapshot synced".to_string());
                        this.load_now(cx, false);
                        // The backfill round the sync spawned is still running;
                        // re-read once more when it has had time to write.
                        let epoch = this.load_epoch;
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(SYNC_BACKFILL_RELOAD).await;
                            let _ = this.update(cx, |this, cx| {
                                if this.load_epoch != epoch {
                                    return;
                                }
                                this.load_now(cx, false);
                            });
                        })
                        .detach();
                    }
                    Err(err) => {
                        this.snapshot_error = Some(format!("Sync failed: {err}"));
                    }
                }
                this.revise(cx);
            },
        );
        self.revise(cx);
    }

    pub(super) fn set_installed(
        &mut self,
        url: String,
        name: String,
        install: bool,
        cx: &mut Context<Self>,
    ) {
        self.busy = Some(name.clone());
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move {
                let fut = tokio::task::spawn_blocking(move || {
                    if install {
                        ss_skills::git_skill::GitSkillFacade::from_file_store()
                            .install_skill(url, Some(name))
                            .map(|s| s.name)
                    } else {
                        ss_skills::skill_install::uninstall_skill(&name).map(|()| name)
                    }
                });
                fut.await.map_err(|e| e.to_string()).and_then(|r| r)
            },
            move |this, cx, res: Result<String, String>| {
                this.busy = None;
                match res {
                    Ok(name) => {
                        crate::notify::toast(
                            Notification::success(if install {
                                format!("Installed {name}")
                            } else {
                                format!("Uninstalled {name}")
                            }),
                            cx,
                        );
                        if let Some(s) = this.skills.iter_mut().find(|s| s.name == name) {
                            s.installed = install;
                        }
                        if let Some(s) = this.loaded_skills.iter_mut().find(|s| s.name == name) {
                            s.installed = install;
                        }
                        this.detail.note_installed(&name, install);
                    }
                    Err(err) => {
                        crate::notify::toast(
                            Notification::error(format!(
                                "{} failed: {err}",
                                if install { "Install" } else { "Uninstall" }
                            )),
                            cx,
                        );
                    }
                }
                this.revise(cx);
            },
        );
        self.revise(cx);
    }

    fn apply_sort(&mut self) {
        let mut skills = self.loaded_skills.clone();
        let searching = !self.query.trim().is_empty();
        match self.sort {
            MarketSort::Updated => {
                skills.sort_by(|a, b| {
                    b.last_updated
                        .cmp(&a.last_updated)
                        .then_with(|| a.name.cmp(&b.name))
                });
            }
            MarketSort::Stars if searching => {
                skills.sort_by(|a, b| b.stars.cmp(&a.stars).then_with(|| a.name.cmp(&b.name)));
            }
            MarketSort::Stars => {}
        }
        if self.sort == MarketSort::Stars {
            for (index, skill) in skills.iter_mut().enumerate() {
                let rank = if searching {
                    (index + 1) as u32
                } else {
                    skill.rank.unwrap_or((index + 1) as u32)
                };
                skill.rank = Some(rank);
            }
        }
        self.skills = skills;
    }

    /// Fresh snapshots stay unlabeled. A sync in flight replaces the status.
    fn snapshot_label(&self) -> Option<SharedString> {
        if self.refreshing {
            return Some(crate::i18n::t("marketplace.refreshingSnapshot"));
        }
        let key = match self.snapshot_status {
            SnapshotStatus::Fresh => return None,
            SnapshotStatus::Stale => "marketplace.snapshotStale",
            SnapshotStatus::Seeding => "marketplace.seedingSnapshot",
            SnapshotStatus::Miss => "marketplace.snapshotMiss",
            SnapshotStatus::ErrorFallback => "marketplace.snapshotErrorFallback",
            SnapshotStatus::RemoteError => "marketplace.snapshotRemoteError",
        };
        Some(crate::i18n::t(key))
    }

    /// Grid and list mode are different `uniform_list` ids; the tracked
    /// handles must not carry an offset across the switch.
    fn reset_list_scroll(&mut self) {
        self.skills_scroll = UniformListScrollHandle::new();
        self.publishers_scroll = UniformListScrollHandle::new();
    }

    // ── Sub-components ───────────────────────────────────────────────────────
}

impl MarketplacePage {
    /// Settings changed a translation switch. The board watches this epoch
    /// and resyncs, so the next paint reads the cache or queues new lines.
    pub(crate) fn note_translation_prefs(&mut self, cx: &mut Context<Self>) {
        self.translation_epoch = self.translation_epoch.wrapping_add(1);
        cx.notify();
    }
}

impl crate::translation::TranslationHost for MarketplacePage {
    fn translations_arrived(&mut self) {
        self.translation_epoch = self.translation_epoch.wrapping_add(1);
    }
}

impl Render for MarketplacePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        let view = cx.entity().downgrade();

        // ── Top Toolbar Chrome ───────────────────────────────────────────────
        let v = view.clone();
        let mut toolbar =
            page_toolbar(crate::i18n::t("sidebar.market")).drag_id("market-toolbar-drag");

        if let Some(search) = &self.search {
            toolbar = toolbar.search(self.render_spotlight(search, view.clone(), cx));
        }

        toolbar = toolbar.filter(self.render_tabs(view.clone()));
        if self.tab != MarketplaceTab::Official {
            let count = self.skills.len().to_string();
            toolbar = toolbar.filter(bar_text_chip(crate::i18n::tf(
                "marketplace.skillsCount",
                &[("count", &count)],
            )));
        }

        if let Some(msg) = &self.status_message {
            toolbar = toolbar.action(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().ok))
                    .child(msg.clone()),
            );
        }
        if let Some(label) = self.snapshot_label() {
            toolbar = toolbar.action(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(palette().fg_muted))
                    .flex_shrink_0()
                    .child(label),
            );
        }
        toolbar = toolbar
            .action(
                bar_refresh_button("mk-refresh-btn", self.refreshing).on_click(move |_, _, cx| {
                    let _ = v.update(cx, |this, cx| this.sync_snapshot(cx));
                }),
            )
            .action(self.render_sort(view.clone()))
            .action(self.render_view_toggle(view.clone()));

        let toolbar = toolbar.build();

        // Banner stays on the page: its retry spin must not rebuild the list.
        let board = self.ensure_board(cx);
        let mut col = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .flex_1()
            .min_h_0()
            .min_w_0();
        if let Some(banner) = self.render_snapshot_banner(view) {
            col = col.child(banner);
        }
        col = col.child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .size_full()
                .child(crate::chrome::replay_view(board.into())),
        );

        let mut page = div()
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .w_full()
            .child(col);
        if self.detail.is_open() {
            let detail = self.ensure_detail(cx);
            page = page.child(
                detail.cached(
                    StyleRefinement::default()
                        .w(px(detail_drawer::DRAWER_W))
                        .h_full()
                        .flex_shrink_0(),
                ),
            );
        }
        page_chrome(toolbar, page)
    }
}

impl EventEmitter<SelectPublisher> for MarketplacePage {}

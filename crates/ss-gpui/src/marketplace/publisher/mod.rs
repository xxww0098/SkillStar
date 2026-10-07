//! Publisher Detail — repos rail + skills in selected repo. React source:
//! `src/pages/PublisherDetail.tsx`. Reached via `NavPage::PublisherDetail`
//! — pushed by `Shell` on `SelectPublisher`, not in the sidebar.
//! Emits `SelectPage(NavPage::Marketplace)` on back button click.

mod board;
mod card;
mod hero;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use ss_core::types::skill::Skill;
use ss_marketplace::snapshot::{
    LocalFirstResult, SnapshotStatus, get_publisher_repos_local, get_repo_skills_local,
    sync_marketplace_scope,
};
use ss_marketplace::{OfficialPublisher, PublisherRepo};

use super::icon;
use crate::chrome::{
    InteractionSpring, MotionPaint, PageBar, bar_refresh_button, page_chrome, pulse, toolbar_search,
};
use crate::nav::{NavPage, SelectPage};
use crate::skill_card::{CARD_GAP, CardFace, card_placeholder, card_rows};
use crate::spawn_domain;
use crate::theme::palette;

/// Per-publisher drill-down: repos rail + skills in the selected repo.
/// React source: `src/pages/PublisherDetail.tsx`.
pub struct PublisherDetailPage {
    publisher: Option<OfficialPublisher>,
    repos: Vec<PublisherRepo>,
    skills: Vec<Skill>,
    active_repo: Option<String>,
    search: Option<Entity<InputState>>,
    query: String,
    loading_repos: bool,
    loading_skills: bool,
    refreshing: bool,
    status_message: Option<String>,
    snapshot_status: SnapshotStatus,
    snapshot_error: Option<String>,
    busy: Option<String>,
    detail: super::detail_drawer::MarketDetailController,
    _subscription: Option<Subscription>,
    /// Repo and skill rows. The toolbar refresh spin notifies this page
    /// without bumping the epoch, so the cached list stays put.
    body_epoch: u64,
    board: Option<Entity<board::PublisherBoard>>,
    detail_view: Option<Entity<board::PublisherDetailColumn>>,
}

impl PublisherDetailPage {
    pub fn new() -> Self {
        Self {
            publisher: None,
            repos: Vec::new(),
            skills: Vec::new(),
            active_repo: None,
            search: None,
            query: String::new(),
            loading_repos: false,
            loading_skills: false,
            refreshing: false,
            status_message: None,
            snapshot_status: SnapshotStatus::Fresh,
            snapshot_error: None,
            busy: None,
            detail: super::detail_drawer::MarketDetailController::new(),
            _subscription: None,
            body_epoch: 0,
            board: None,
            detail_view: None,
        }
    }

    pub fn set_publisher(&mut self, publisher: OfficialPublisher, cx: &mut Context<Self>) {
        self.publisher = Some(publisher);
        self.repos.clear();
        self.skills.clear();
        self.detail.clear();
        self.active_repo = None;
        self.query.clear();
        self.snapshot_error = None;
        self.status_message = None;
        self.refresh_repos(cx);
    }

    fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            return;
        }
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder(self.search_placeholder()));
        self._subscription = Some(cx.subscribe_in(
            &search,
            window,
            |this, _state, event: &InputEvent, _window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some(search) = &this.search else { return };
                this.query = search.read(cx).value().trim().to_string();
                this.revise(cx);
            },
        ));
        self.search = Some(search);
    }

    pub(super) fn body_epoch(&self) -> u64 {
        self.body_epoch
    }

    /// List data changed. The refresh spin notifies this page and must not
    /// come through here.
    pub(crate) fn revise(&mut self, cx: &mut Context<Self>) {
        self.body_epoch = self.body_epoch.wrapping_add(1);
        cx.notify();
    }

    fn search_placeholder(&self) -> SharedString {
        let name = self
            .publisher
            .as_ref()
            .map(|publisher| publisher.name.as_str())
            .unwrap_or("");
        crate::i18n::tf("publisherDetail.searchPlaceholder", &[("name", name)])
    }

    pub fn sync_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = &self.search {
            crate::i18n::sync_placeholder(search, self.search_placeholder(), window, cx);
        }
        self.revise(cx);
    }

    fn refresh_repos(&mut self, cx: &mut Context<Self>) {
        let Some(publisher) = &self.publisher else {
            return;
        };
        let name = publisher.name.clone();
        self.loading_repos = true;
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move { get_publisher_repos_local(&name).await },
            |this, cx, result| {
                this.loading_repos = false;
                match result {
                    Ok(LocalFirstResult {
                        data,
                        snapshot_status,
                        error,
                        ..
                    }) => {
                        this.repos = data;
                        this.snapshot_status = snapshot_status;
                        this.snapshot_error = error;
                    }
                    Err(err) => this.snapshot_error = Some(err.to_string()),
                }
                this.revise(cx);
            },
        );
    }

    fn load_repo_skills(&mut self, cx: &mut Context<Self>, source: &str) {
        let source = source.to_string();
        if self.detail.selected_source() != Some(source.as_str()) {
            self.detail.clear();
        }
        self.active_repo = Some(source.clone());
        self.loading_skills = true;
        self.query.clear();
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move { get_repo_skills_local(&source).await },
            |this, cx, result| {
                this.loading_skills = false;
                match result {
                    Ok(LocalFirstResult {
                        data,
                        snapshot_status,
                        error,
                        ..
                    }) => {
                        this.skills = data;
                        this.detail.retain(&this.skills);
                        this.snapshot_status = snapshot_status;
                        if this.snapshot_error.is_none() {
                            this.snapshot_error = error;
                        }
                    }
                    Err(err) => this.snapshot_error = Some(err.to_string()),
                }
                this.revise(cx);
            },
        );
    }

    fn sync_current_scope(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.publisher.as_ref().map(|pub_| pub_.name.clone()) else {
            return;
        };
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.revise(cx);
        let scope = if let Some(repo) = &self.active_repo {
            format!("repo_skills:{repo}")
        } else {
            format!("publisher_repos:{}", name.to_ascii_lowercase())
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
                        this.status_message = Some("Snapshot refreshed".to_string());
                        if let Some(active) = this.active_repo.clone() {
                            this.load_repo_skills(cx, &active);
                        } else {
                            this.refresh_repos(cx);
                        }
                    }
                    Err(err) => {
                        this.snapshot_error = Some(format!("Sync error: {err}"));
                    }
                }
                this.revise(cx);
            },
        );
        self.revise(cx);
    }

    fn set_installed(&mut self, url: String, name: String, install: bool, cx: &mut Context<Self>) {
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

    /// Placeholder tiles on the same tracks the loaded skill grid uses.
    fn render_skeletons(&self, columns: usize) -> impl IntoElement {
        let mut tiles = Vec::with_capacity(6);
        for index in 0..6 {
            tiles.push(
                card_placeholder(
                    ElementId::Name(format!("pd-skel-{index}").into()),
                    CardFace::Market,
                )
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(pulse(format!("pd-skel-{index}-title")).h_4().w(px(160.0)))
                .child(
                    pulse(format!("pd-skel-{index}-line"))
                        .h_3()
                        .w(px(180.0))
                        .secondary(),
                )
                .into_any_element(),
            );
        }
        card_rows(columns, CARD_GAP, tiles)
    }
}

impl Render for PublisherDetailPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        let view = cx.entity().downgrade();

        let publisher_name = self
            .publisher
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Publisher".to_string());
        let active_repo = self.active_repo.clone();
        let refreshing = self.refreshing;
        let status_message = self.status_message.clone();

        let v_back = view.clone();
        let mut title_row = div().flex().items_center().gap_2().min_w_0().child(
            div()
                .id("pd-back-btn")
                .flex()
                .items_center()
                .gap(px(6.0))
                .px_2()
                .h(px(32.0))
                .rounded_md()
                .cursor_pointer()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(palette().fg_muted))
                .occlude()
                .child(icon(IconName::ArrowLeft, 16.0, palette().fg_muted))
                .child(crate::i18n::t("publisherDetail.back"))
                .on_click(move |_, _, cx| {
                    let _ = v_back.update(cx, |this, cx| {
                        if this.active_repo.is_some() {
                            this.active_repo = None;
                            this.query.clear();
                            this.detail.clear();
                            this.revise(cx);
                        } else {
                            cx.emit(SelectPage(NavPage::Marketplace));
                        }
                    });
                })
                .interaction_spring(
                    "pd-back-btn",
                    true,
                    MotionPaint::new().fg(rgb(palette().fg_muted)),
                    MotionPaint::new()
                        .bg(rgb(palette().card_hover))
                        .fg(rgb(palette().fg)),
                ),
        );
        title_row = title_row
            .child(
                div()
                    .w(px(1.0))
                    .h(px(20.0))
                    .mx(px(4.0))
                    .bg(rgb(palette().border)),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(palette().fg))
                    .overflow_hidden()
                    .child(publisher_name),
            );
        if let Some(repo) = active_repo {
            title_row = title_row
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(palette().fg_muted))
                        .child("/"),
                )
                .child(
                    div()
                        .text_sm()
                        .font_family("monospace")
                        .text_color(rgb(palette().fg))
                        .overflow_hidden()
                        .child(repo),
                );
        }

        let mut toolbar = PageBar::title_row(title_row).drag_id("publisher-toolbar-drag");
        if let Some(search) = &self.search {
            toolbar = toolbar.action(toolbar_search(search, 224.0));
        }
        if let Some(msg) = status_message {
            toolbar = toolbar.action(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().ok))
                    .child(msg),
            );
        }
        if refreshing {
            toolbar = toolbar.action(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t("marketplace.refreshingSnapshot")),
            );
        }
        let v_sync = view.clone();
        toolbar = toolbar.action(bar_refresh_button("pd-sync-btn", refreshing).on_click(
            move |_, _, cx| {
                let _ = v_sync.update(cx, |this, cx| this.sync_current_scope(cx));
            },
        ));
        let toolbar = toolbar.build();

        // ── Main Page Content ────────────────────────────────────────────────
        let mut col = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .flex_1()
            .min_h_0()
            .min_w_0();

        if self.publisher.is_none() {
            return page_chrome(
                toolbar,
                col.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .py_20()
                        .child(icon(IconName::Package, 32.0, palette().fg_muted))
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(palette().fg_muted))
                                .child(crate::i18n::t("publisherDetail.noneSelected")),
                        ),
                ),
            );
        }

        let board = self.ensure_board(cx);
        col = col.child(self.render_hero_banner(self.publisher.as_ref().unwrap()));
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
                        .w(px(super::detail_drawer::DRAWER_W))
                        .h_full()
                        .flex_shrink_0(),
                ),
            );
        }
        page_chrome(toolbar, page)
    }
}

impl PublisherDetailPage {
    pub(super) fn open_market_skill(&mut self, skill: &Skill, cx: &mut Context<Self>) {
        let intent = self.detail.toggle(skill);
        self.follow_market_detail(intent, cx);
        self.revise(cx);
    }

    fn close_market_detail(&mut self, cx: &mut Context<Self>) {
        self.detail.clear();
        self.revise(cx);
    }

    fn retry_market_detail(&mut self, cx: &mut Context<Self>) {
        let intent = self.detail.retry();
        self.follow_market_detail(intent, cx);
        self.revise(cx);
    }

    fn follow_market_detail(
        &mut self,
        intent: super::detail_drawer::DetailIntent,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity();
        super::detail_drawer::spawn_market_detail(
            &view,
            cx,
            intent,
            |this, cx, epoch, id, refresh, result| {
                let next = this.detail.apply(epoch, id, refresh, result);
                this.follow_market_detail(next, cx);
            },
        );
    }

    fn render_market_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(skill) = self.detail.skill().cloned() else {
            return div().into_any_element();
        };
        let Some(phase) = self.detail.phase().cloned() else {
            return div().into_any_element();
        };
        if let super::detail_drawer::DetailPhase::Ready(details) = &phase
            && let Some(summary) = details
                .summary
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
        {
            crate::translation::schedule(
                &cx.entity(),
                [summary.to_string()],
                crate::translation::Surface::Description,
                cx,
            );
        }
        let busy = self.busy.as_deref() == Some(skill.name.as_str());
        let install = !skill.installed;
        let url = skill.git_url.clone();
        let name = skill.name.clone();
        let view = cx.entity().downgrade();
        let retry_view = view.clone();
        let close_view = view.clone();
        super::detail_drawer::market_detail_column(
            "pd",
            &skill,
            &phase,
            busy,
            move |_, _, cx| {
                let _ = close_view.update(cx, |this, cx| this.close_market_detail(cx));
            },
            move |_, _, cx| {
                let url = url.clone();
                let name = name.clone();
                let _ = view.update(cx, |this, cx| {
                    this.set_installed(url, name, install, cx);
                });
            },
            move |_, _, cx| {
                let _ = retry_view.update(cx, |this, cx| this.retry_market_detail(cx));
            },
        )
    }
}

fn empty_block(icon_name: IconName, title: String) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .py_16()
        .child(icon(icon_name, 32.0, palette().fg_muted))
        .child(
            div()
                .text_base()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(palette().fg))
                .child(title),
        )
}

impl EventEmitter<SelectPage> for PublisherDetailPage {}

impl crate::translation::TranslationHost for PublisherDetailPage {
    fn translations_arrived(&mut self) {
        self.body_epoch = self.body_epoch.wrapping_add(1);
    }
}

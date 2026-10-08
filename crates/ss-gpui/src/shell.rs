//! `Shell` owns the active page, the sidebar, and the keep-alive page
//! table. Mirrors `App.tsx` in the React SPA: nav state lives here,
//! capabilities are child entities.

use std::rc::Rc;

use gpui_kit::component::Sizable;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_marketplace::OfficialPublisher;

use crate::accounts::AccountsPage;
use crate::chrome::{
    InteractionSpring, MotionPaint, SliderGeometry, SliderSegment, motion_spring, replay_view,
    slider_segmented_at, window_drag,
};
use crate::layout::{
    RAIL_COLLAPSED_W as RAIL_COLLAPSED_PX, RAIL_W as RAIL_EXPANDED_PX, SHELL_GAP as SHELL_GAP_PX,
};
use crate::marketplace::{MarketplacePage, PublisherDetailPage};
use crate::my_skills::MySkillsPage;
use crate::nav::{
    AgentsChanged, AppMode, GroupsChanged, NavPage, SelectPage, SelectPublisher,
    TranslationPrefsChanged,
};
use crate::projects::ProjectsPage;
use crate::settings::SettingsPage;
use crate::skill_cards::SkillCardsPage;
use crate::theme::{self, palette};

mod nav;

#[cfg(test)]
mod dialog_motion;

/// SkillStar `--radius-xl` is 16px. GPUI `rounded_xl` is Tailwind's 12px.
const PANEL_RADIUS_PX: f32 = 16.0;

pub struct Shell {
    page: NavPage,
    mode: AppMode,
    collapsed: bool,
    /// Last skills-nav row. Settings and publisher detail leave it put so the
    /// selection box fades out where it is.
    nav_slot: u8,
    pages: Pages,
    /// Bottom alert band fed by `notify::toast`.
    notices: Entity<crate::notify::NoticeBoard>,
    /// Keeps the `Marketplace → SelectPublisher` subscription alive.
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pages = Pages::new(window, cx);
        // Marketplace → PublisherDetail: the marketplace view emits
        // `SelectPublisher`; Shell translates it into a nav hop +
        // detail-page payload. Neither side knows the other exists.
        let subscription = cx.subscribe(
            &pages.marketplace,
            |this, _emitter, event: &SelectPublisher, cx| {
                this.pages.open_publisher(event.publisher.clone(), cx);
                this.set_page(NavPage::PublisherDetail, cx);
            },
        );
        // Page-internal nav hops: a page emits `SelectPage` to ask
        // `Shell` to switch tabs (empty-state CTAs, cross-page links).
        // Typed subscribe needs the concrete `Entity<T>` — only
        // `MySkillsPage` emits today; extend when other pages do.
        let mut subs = vec![subscription];
        subs.push(
            cx.subscribe(&pages.my_skills, |this, _, ev: &SelectPage, cx| {
                this.set_page(ev.0, cx);
            }),
        );
        // MySkills flows that write decks (Quick Pack, `agd-` share codes)
        // ask the KeepAlive cards page to reload its group list.
        subs.push(
            cx.subscribe(&pages.my_skills, |this, _, _: &GroupsChanged, cx| {
                let _ = this
                    .pages
                    .skill_cards
                    .update(cx, |page, cx| page.refresh(cx));
            }),
        );
        // Agent enable/disable only mutates Settings. Reload the KeepAlive
        // snapshots now, or the skill-card carousel keeps the old SVGs.
        subs.push(
            cx.subscribe(&pages.settings, |this, _, _: &AgentsChanged, cx| {
                let _ = this
                    .pages
                    .my_skills
                    .update(cx, |page, cx| page.reload_agent_profiles(cx));
                // Deck refresh already reloads the targetable profile
                // snapshot and has no dirty buffer.
                let _ = this
                    .pages
                    .skill_cards
                    .update(cx, |page, cx| page.refresh(cx));
                let _ = this
                    .pages
                    .projects
                    .update(cx, |page, cx| page.reload_agent_profiles(cx));
            }),
        );
        subs.push(cx.subscribe(
            &pages.settings,
            |this, _, _: &TranslationPrefsChanged, cx| {
                let _ = this
                    .pages
                    .my_skills
                    .update(cx, |page, cx| page.note_translation_prefs(cx));
                let _ = this
                    .pages
                    .marketplace
                    .update(cx, |page, cx| page.note_translation_prefs(cx));
                let _ = this
                    .pages
                    .publisher_detail
                    .update(cx, |page, cx| page.revise(cx));
            },
        ));
        subs.push(
            cx.subscribe(&pages.publisher_detail, |this, _, ev: &SelectPage, cx| {
                this.set_page(ev.0, cx);
            }),
        );
        // The accounts-mode rail lists providers from this page. A filter
        // click or a subscription reload has to repaint the shell, not only
        // the card pane.
        subs.push(cx.observe(&pages.accounts, |this, _, cx| {
            if this.mode == AppMode::Accounts {
                cx.notify();
            }
        }));
        subs.push(cx.observe_global::<crate::i18n::UiLang>(|this, cx| {
            let my_skills = this.pages.my_skills.clone();
            let marketplace = this.pages.marketplace.clone();
            let skill_cards = this.pages.skill_cards.clone();
            let projects = this.pages.projects.clone();
            let accounts = this.pages.accounts.clone();
            let publisher = this.pages.publisher_detail.clone();
            let settings = this.pages.settings.clone();
            let synced = if let Some(win) = cx.active_window() {
                let my_skills = my_skills.clone();
                let marketplace = marketplace.clone();
                let skill_cards = skill_cards.clone();
                let projects = projects.clone();
                let accounts = accounts.clone();
                let publisher = publisher.clone();
                let settings = settings.clone();
                win.update(cx, move |_root, window, cx| {
                    let _ = my_skills.update(cx, |page, cx| page.sync_language(window, cx));
                    let _ = marketplace.update(cx, |page, cx| page.sync_language(window, cx));
                    let _ = skill_cards.update(cx, |page, cx| page.sync_language(window, cx));
                    let _ = projects.update(cx, |page, cx| page.sync_language(window, cx));
                    let _ = accounts.update(cx, |page, cx| page.sync_language(window, cx));
                    let _ = publisher.update(cx, |page, cx| page.sync_language(window, cx));
                    let _ = settings.update(cx, |page, cx| page.sync_language(window, cx));
                })
                .is_ok()
            } else {
                false
            };
            // Window updates fail while that window is already on the stack.
            // Pages still have to redraw so `t()` picks up the new catalog.
            if !synced {
                let _ = my_skills.update(cx, |page, cx| page.revise(cx));
                let _ = marketplace.update(cx, |page, cx| page.revise(cx));
                let _ = skill_cards.update(cx, |_, cx| cx.notify());
                let _ = projects.update(cx, |_, cx| cx.notify());
                let _ = accounts.update(cx, |page, cx| page.revise(cx));
                let _ = publisher.update(cx, |page, cx| page.revise(cx));
                let _ = settings.update(cx, |_, cx| cx.notify());
            }
            cx.notify();
        }));
        let (initial_page, initial_mode) = match std::env::var("SKILLSTAR_START_PAGE").as_deref() {
            Ok("marketplace") => (NavPage::Marketplace, AppMode::Skills),
            Ok("accounts") => (NavPage::Accounts, AppMode::Accounts),
            Ok("skill_cards") | Ok("cards") => (NavPage::SkillCards, AppMode::Skills),
            Ok("projects") => (NavPage::Projects, AppMode::Skills),
            Ok("settings") => (NavPage::Settings, AppMode::Skills),
            _ => (NavPage::default(), AppMode::default()),
        };
        // The alert band: pages push through `notify::toast`, which finds
        // this board via a global. Board changes repaint the shell because
        // the band is part of the shell's own scene.
        let notices = cx.new(|_| crate::notify::NoticeBoard::default());
        crate::notify::NoticeBoard::register(&notices, cx);
        subs.push(cx.observe(&notices, |_, _, cx| {
            cx.notify();
        }));
        Self {
            page: initial_page,
            mode: initial_mode,
            collapsed: false,
            nav_slot: nav::skills_nav_slot(initial_page).unwrap_or(0),
            pages,
            notices,
            _subscriptions: subs,
        }
    }

    pub fn set_page(&mut self, page: NavPage, cx: &mut Context<Self>) {
        // Accounts is a mode-gated destination: selecting it flips the
        // rail context (mirrors `useNavigation`).
        match page {
            NavPage::Accounts => self.mode = AppMode::Accounts,
            NavPage::Settings | NavPage::PublisherDetail => {}
            _ => self.mode = AppMode::Skills,
        }
        self.page = page;
        self.remember_nav_slot();
        // Re-list on entry: background automatic updates and channel flows
        // write the shared update projection while this page is kept alive, so
        // its cards must not show the snapshot from the previous visit.
        // Deferred so an event emitted by the page itself can never re-enter
        // its own update.
        if page == NavPage::MySkills {
            let my_skills = self.pages.my_skills.clone();
            cx.defer(move |cx| {
                let _ = my_skills.update(cx, |page, cx| page.refresh(cx));
            });
        }
        cx.notify();
    }

    fn set_mode(&mut self, mode: AppMode, cx: &mut Context<Self>) {
        self.mode = mode;
        // Land on the mode's first page when the current page isn't in
        // that rail.
        let ok = match mode {
            AppMode::Skills => !matches!(self.page, NavPage::Accounts),
            AppMode::Accounts => self.page == NavPage::Accounts,
        };
        if !ok {
            self.page = match mode {
                AppMode::Skills => NavPage::MySkills,
                AppMode::Accounts => NavPage::Accounts,
            };
        }
        self.remember_nav_slot();
        cx.notify();
    }

    fn remember_nav_slot(&mut self) {
        if let Some(slot) = nav::skills_nav_slot(self.page) {
            self.nav_slot = slot;
        }
    }

    fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        theme::toggle(cx);
    }

    fn toggle_collapsed(&mut self, cx: &mut Context<Self>) {
        self.collapsed = !self.collapsed;
        cx.notify();
    }
}

/// Mount a keep-alive page in the main panel.
///
/// Shell springs (the mode capsule, the nav selection) notify this shell.
/// The page is a sibling of those springs, so replaying it keeps the card
/// grid out of those frames.
fn mount_page(page: AnyView) -> impl IntoElement {
    replay_view(page)
}

/// Root child. Dialogs, sheets, and toasts notify the window layer, which
/// dirties [`gpui_kit::component::Root`] but not this shell. Replaying the
/// shell copies its last scene instead of laying the sidebar and the page
/// out on every entrance frame.
pub struct WindowSurface {
    shell: Entity<Shell>,
}

impl WindowSurface {
    pub fn new(shell: Entity<Shell>) -> Self {
        Self { shell }
    }
}

impl Render for WindowSurface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .child(replay_view(self.shell.clone().into()))
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.pages.view_for(self.page);
        let shell = cx.entity().downgrade();
        let rail_w = if self.collapsed {
            RAIL_COLLAPSED_PX
        } else {
            RAIL_EXPANDED_PX
        };
        // Canvas is palette().bg. Both panels float on it: 8px inset, 16px radius,
        // 1px border, opaque palette().panel. Main's left edge is gap + rail + gap.
        div()
            .relative()
            .size_full()
            .bg(rgb(palette().bg))
            .text_color(rgb(palette().fg))
            .child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .bottom_2()
                    .left(px(SHELL_GAP_PX + rail_w + SHELL_GAP_PX))
                    .flex()
                    .flex_col()
                    .rounded(px(PANEL_RADIUS_PX))
                    .border_1()
                    .border_color(rgb(palette().border))
                    .bg(rgb(palette().panel))
                    .overflow_hidden()
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .size_full()
                            .child(mount_page(body)),
                    )
                    // The alert band docks below the page content: docking at
                    // the top would push the toolbar band that carries the
                    // window's traffic-light clearance out of alignment.
                    .child(self.notices.read(cx).render()),
            )
            .child(render_sidebar(
                self.page,
                self.mode,
                self.collapsed,
                self.nav_slot,
                shell,
                &self.pages.accounts,
                cx,
            ))
    }
}

fn icon(name: assets::IconName, size: f32, color: u32) -> component::Icon {
    component::Icon::new(name)
        .with_size(px(size))
        .text_color(rgb(color))
}

/// Left nav rail — mirrors React `Sidebar.tsx`.
fn render_sidebar(
    active: NavPage,
    mode: AppMode,
    collapsed: bool,
    nav_slot: u8,
    shell: WeakEntity<Shell>,
    accounts: &Entity<AccountsPage>,
    cx: &mut Context<Shell>,
) -> Div {
    // React sidebar is `fixed top-0 left-2 bottom-2` and is not a card.
    // The lane under the traffic lights is only a drag strip. The main
    // panel starts at `top-2`, so its toolbar fills the same band.
    let mut rail = div()
        .absolute()
        .top_0()
        .left(px(SHELL_GAP_PX))
        .bottom(px(SHELL_GAP_PX))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .pb_2()
        .w(px(if collapsed {
            RAIL_COLLAPSED_PX
        } else {
            RAIL_EXPANDED_PX
        }))
        .when(collapsed, |d| d.overflow_hidden());

    // `collapsed ? "h-7" : isMacDesktop ? "h-9" : "h-7"`.
    let lane_h = if collapsed || !cfg!(target_os = "macos") {
        28.0
    } else {
        36.0
    };
    rail = rail.child(window_drag(
        "sidebar-traffic-drag",
        div().w_full().h(px(lane_h)).flex_shrink_0(),
    ));
    let brand = div()
        .flex()
        .items_center()
        .justify_center()
        .pb(px(10.0))
        .when(!collapsed, |d| d.gap(px(10.0)).px_3())
        .child(
            div()
                .w(px(32.0))
                .h(px(32.0))
                .rounded_lg()
                .overflow_hidden()
                .bg(rgb(0xffffff))
                .child(gpui_kit::img("icon.png").size_full()),
        )
        .when(!collapsed, |d| {
            d.child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_color(rgb(palette().fg))
                            .font_weight(FontWeight::BOLD)
                            .text_sm()
                            .child("SkillStar"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("sidebar.tagline")),
                    ),
            )
        });
    // Expanded brand row is a drag region. The collapsed logo is not.
    rail = if collapsed {
        rail.child(brand)
    } else {
        rail.child(window_drag("sidebar-brand-drag", brand))
    };

    rail = rail.child(render_mode_switcher(mode, collapsed, shell.clone()));

    // Skills mode lists pages. Accounts mode lists providers in this same
    // slot; the card pane does not draw a second column.
    let mut nav = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .gap(px(2.0))
        .py_2()
        .when(collapsed, |d| d.px(px(6.0)))
        .when(!collapsed, |d| d.px_2());
    if mode == AppMode::Accounts {
        let menu = accounts
            .read(cx)
            .render_provider_menu(collapsed, accounts.downgrade());
        // `overflow_y_scroll` (not `overflow_y_scrollbar`): the rail list
        // stays wheel-scrollable but draws no scrollbar.
        nav = nav.child(
            div()
                .id("accounts-provider-menu")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(menu),
        );
    } else {
        nav = nav.overflow_y_hidden().child(nav::render_skills_nav(
            active,
            collapsed,
            nav_slot,
            shell.clone(),
        ));
    }
    rail = rail.child(nav);

    // Footer matches React: expanded is collapse | theme | settings;
    // collapsed stacks settings, theme, then collapse. Collapse must
    // notify — flipping the flag alone does not re-render the rail.
    let paper = theme::is_light();
    let settings_active = active == NavPage::Settings;
    let collapse_btn = footer_btn(
        "collapse",
        if collapsed {
            assets::IconName::PanelLeftOpen
        } else {
            assets::IconName::PanelLeftClose
        },
        false,
        shell.clone(),
        |this, cx| this.toggle_collapsed(cx),
    );
    let theme_btn = footer_btn(
        "theme",
        if paper {
            assets::IconName::Sun
        } else {
            assets::IconName::Moon
        },
        false,
        shell.clone(),
        |this, cx| this.toggle_theme(cx),
    );
    let settings_btn = footer_btn(
        "settings",
        assets::IconName::Settings,
        settings_active,
        shell,
        |this, cx| this.set_page(NavPage::Settings, cx),
    );
    let mut footer = div()
        .flex()
        .items_center()
        .px_1()
        .py_2()
        .border_t_1()
        .border_color(rgb(palette().border));
    footer = if collapsed {
        footer
            .flex_col()
            .gap(px(2.0))
            .child(settings_btn)
            .child(theme_btn)
            .child(collapse_btn)
    } else {
        footer
            .justify_between()
            .gap_1()
            .child(collapse_btn)
            .child(theme_btn)
            .child(settings_btn)
    };
    rail = rail.child(footer);
    rail
}

/// 28×28 icon button for the footer row.
fn footer_btn(
    id: &'static str,
    icon_name: assets::IconName,
    active: bool,
    shell: WeakEntity<Shell>,
    act: fn(&mut Shell, &mut Context<Shell>),
) -> impl IntoElement {
    let mut rest = MotionPaint::new();
    let hover = MotionPaint::new().bg(rgb(palette().panel_hover));
    if active {
        rest = rest.bg(rgb(palette().panel_active));
    }
    div()
        .id(ElementId::Name(id.into()))
        .flex()
        .items_center()
        .justify_center()
        .size(px(28.0))
        .rounded_md()
        .cursor_pointer()
        .child(icon(
            icon_name,
            15.0,
            if active {
                palette().accent_fg
            } else {
                palette().fg_muted
            },
        ))
        .when(active, |d| d.bg(rgb(palette().panel_active)))
        .on_click(move |_, _, cx| {
            let _ = shell.update(cx, |this, cx| act(this, cx));
        })
        .interaction_spring(id, true, rest, hover)
}

/// Sidebar inset around the mode switcher. Kept beside the rail widths so
/// the switcher geometry below derives from one set of constants.
const MODE_SWITCHER_INSET_PX: f32 = 12.0;
const MODE_SWITCHER_INSET_COLLAPSED_PX: f32 = 6.0;

/// Skills/Accounts segmented pills — mirrors `ModeSwitcher`. Icon-only
/// stacked buttons when the rail is collapsed.
///
/// The selected capsule is the shared sliding thumb (`chrome::segmented`),
/// so it glides when the mode changes and the scope switch below the toolbar
/// behaves the same way. `reduce_motion` snaps it (`with_spring`).
fn render_mode_switcher(
    mode: AppMode,
    collapsed: bool,
    shell: WeakEntity<Shell>,
) -> impl IntoElement {
    let target = if mode == AppMode::Accounts { 1.0 } else { 0.0 };
    div().with_spring(
        ElementId::Name("mode-switch-motion".into()),
        motion_spring(target),
        move |_, t| {
            div()
                .when(collapsed, |d| d.px(px(MODE_SWITCHER_INSET_COLLAPSED_PX)))
                .when(!collapsed, |d| d.px(px(MODE_SWITCHER_INSET_PX)))
                .child(mode_switcher_track(mode, t, collapsed, &shell))
        },
    )
}

/// Slot geometry derives from the rail width minus the wrapper inset and the
/// track's padding (4) plus border (1) per side; the thumb mirrors one pill.
fn mode_switcher_track(mode: AppMode, t: f32, collapsed: bool, shell: &WeakEntity<Shell>) -> Div {
    const TRACK_PAD_PX: f32 = 4.0;
    const GAP_PX: f32 = 2.0;
    const PILL_H_PX: f32 = 32.0;
    let rail_w = if collapsed {
        RAIL_COLLAPSED_PX
    } else {
        RAIL_EXPANDED_PX
    };
    let inset = if collapsed {
        MODE_SWITCHER_INSET_COLLAPSED_PX
    } else {
        MODE_SWITCHER_INSET_PX
    };
    let chrome = TRACK_PAD_PX + 1.0;
    let inner_w = rail_w - 2.0 * inset - 2.0 * chrome;
    let pill_w = if collapsed {
        inner_w
    } else {
        (inner_w - GAP_PX) / 2.0
    };

    let segments = vec![
        SliderSegment {
            id: "mode-skills",
            icon: Some(assets::IconName::LayoutGrid),
            label: (!collapsed).then(|| crate::i18n::t("sidebar.modeSkills")),
        },
        SliderSegment {
            id: "mode-accounts",
            icon: Some(assets::IconName::Users),
            label: (!collapsed).then(|| crate::i18n::t("sidebar.modeAccounts")),
        },
    ];
    let selected = if mode == AppMode::Accounts { 1 } else { 0 };
    slider_segmented_at(
        t,
        &segments,
        selected,
        SliderGeometry {
            slot_w: pill_w,
            slot_h: PILL_H_PX,
            pad: TRACK_PAD_PX,
            icon_size: if collapsed { 16.0 } else { 14.0 },
        },
        collapsed,
        &mode_switch_on_click(shell),
    )
    .w_full()
}

fn mode_switch_on_click(shell: &WeakEntity<Shell>) -> Rc<dyn Fn(usize, &mut Window, &mut App)> {
    let shell = shell.clone();
    Rc::new(move |ix, _, cx| {
        let m = if ix == 1 {
            AppMode::Accounts
        } else {
            AppMode::Skills
        };
        let _ = shell.update(cx, |this, cx| this.set_mode(m, cx));
    })
}

/// Per-capability entities kept alive across navigation.
struct Pages {
    my_skills: Entity<MySkillsPage>,
    marketplace: Entity<MarketplacePage>,
    skill_cards: Entity<SkillCardsPage>,
    projects: Entity<ProjectsPage>,
    accounts: Entity<AccountsPage>,
    publisher_detail: Entity<PublisherDetailPage>,
    settings: Entity<SettingsPage>,
}

impl Pages {
    fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        Self {
            my_skills: cx.new(|cx| MySkillsPage::new(cx)),
            marketplace: cx.new(|cx| MarketplacePage::new(cx)),
            skill_cards: cx.new(|cx| SkillCardsPage::new(cx)),
            projects: cx.new(|cx| ProjectsPage::new(cx)),
            accounts: cx.new(|cx| AccountsPage::new(cx)),
            publisher_detail: cx.new(|_| PublisherDetailPage::new()),
            settings: cx.new(|cx| SettingsPage::new(window, cx)),
        }
    }

    fn view_for(&self, page: NavPage) -> AnyView {
        match page {
            NavPage::MySkills => self.my_skills.clone().into(),
            NavPage::Marketplace => self.marketplace.clone().into(),
            NavPage::SkillCards => self.skill_cards.clone().into(),
            NavPage::Projects => self.projects.clone().into(),
            NavPage::Accounts => self.accounts.clone().into(),
            NavPage::PublisherDetail => self.publisher_detail.clone().into(),
            NavPage::Settings => self.settings.clone().into(),
        }
    }

    /// Feed the detail page a publisher — called by `Shell` when it
    /// receives a `SelectPublisher` event.
    fn open_publisher(&self, publisher: OfficialPublisher, cx: &mut App) {
        let _ = self.publisher_detail.update(cx, |this, cx| {
            this.set_publisher(publisher, cx);
        });
    }
}

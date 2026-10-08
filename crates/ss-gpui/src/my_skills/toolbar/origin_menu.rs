//! Compact source filter and repository actions for My Skills.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Selectable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::types::skill::Skill;

use super::super::MySkillsPage;
use super::super::types::SourceFilter;
use crate::chrome::{InteractionSpring, MotionPaint, icon, icon_spin};
use crate::theme::palette;

/// Repo rows (the all-repos entry included) that fit before the origin menu
/// has to scroll. A `max_h` on the scrollable does not create wheel overflow.
const ORIGIN_REPOS_VISIBLE: usize = 8;
/// Repo-list viewport height, same `max_h` caveat as [`ORIGIN_REPOS_VISIBLE`].
const ORIGIN_REPOS_VIEW_H: f32 = 280.0;

pub(super) fn origin_menu_button(
    view: WeakEntity<MySkillsPage>,
    page: &MySkillsPage,
    count: usize,
    repos: Vec<String>,
) -> impl IntoElement {
    let active = page.source_filter != SourceFilter::All || page.repo_filter.is_some();
    let mut tip = format!(
        "{}: {}",
        crate::i18n::t("toolbar.source"),
        page.source_filter.label(),
    );
    if let Some(repo) = &page.repo_filter {
        tip.push_str(&format!(" · {repo}"));
    }
    let source_filter = page.source_filter;
    let repo_filter = page.repo_filter.clone();
    let reinstalling = page.reinstalling_repo.clone();
    // `Button::tooltip` builds an unstyled kit tip. The hover tip hangs here.
    let hover = tip.clone();
    div()
        .id("my-skills-origin-hover")
        .flex_shrink_0()
        .occlude()
        .tooltip(move |window, cx| crate::chrome::tooltip(hover.clone()).build(window, cx))
        .child(
            Popover::new("my-skills-origin")
                .anchor(Anchor::TopLeft)
                .offset(px(6.0))
                .appearance(false)
                .trigger(
                    Button::new("my-skills-origin-trigger")
                        // `small` is 24px. The agent-filter track beside this
                        // button is the toolbar's 32px control (`segment_track`).
                        .small()
                        .h(px(32.0))
                        .icon(IconName::Layers)
                        .label(count.to_string())
                        .dropdown_caret(true)
                        .selected(active)
                        .accessibility_label(tip),
                )
                .content(move |_, _, cx| {
                    origin_menu(
                        cx,
                        view.clone(),
                        source_filter,
                        repo_filter.clone(),
                        repos.clone(),
                        reinstalling.clone(),
                    )
                }),
        )
}

fn origin_menu(
    cx: &mut Context<gpui_kit::component::popover::PopoverState>,
    view: WeakEntity<MySkillsPage>,
    source_filter: SourceFilter,
    repo_filter: Option<String>,
    repos: Vec<String>,
    reinstalling: Option<String>,
) -> impl IntoElement + use<> {
    let dismiss_popover = cx.entity().downgrade();
    let show_repos =
        source_filter != SourceFilter::Local && !(repos.is_empty() && repo_filter.is_none());
    // One width so the source track and the repo column share an edge.
    // Without repos the track only has to hold three short labels.
    let mut panel = div()
        .w(px(if show_repos { 320.0 } else { 220.0 }))
        .rounded_xl()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .p_2()
        .shadow_lg()
        .flex()
        .flex_col()
        .gap_2()
        .child(source_segment(
            view.clone(),
            dismiss_popover.clone(),
            source_filter,
        ));
    if !show_repos {
        return panel;
    }
    // More rows than the viewport holds scroll inside a fixed height; fewer
    // render at their natural height. A `max_h` on the scrollable does not
    // create wheel overflow — the wrapper leaves it on the content.
    let repo_rows = repos.len() + 1;
    let mut list = div()
        .id("my-skills-origin-repos")
        .flex()
        .flex_col()
        .gap(px(2.0));
    let v = view.clone();
    list = list.child(repo_row(
        "origin-repo-all",
        crate::i18n::t("toolbar.allRepos"),
        repo_filter.is_none(),
        {
            let v = v.clone();
            let dismiss_popover = dismiss_popover.clone();
            move |_, window, app| {
                app.stop_propagation();
                let _ = v.update(app, |this, cx| {
                    this.repo_filter = None;
                    this.revise(cx);
                });
                let _ = dismiss_popover.update(app, |state, cx| state.dismiss(window, cx));
            }
        },
    ));
    for repo in repos {
        let active = repo_filter.as_deref() == Some(repo.as_str());
        let spinning = reinstalling.as_deref() == Some(repo.as_str());
        let locked = reinstalling.is_some();
        let v = view.clone();
        let dismiss_popover = dismiss_popover.clone();
        let pick = repo.clone();
        let reinstall_view = view.clone();
        let remove_view = view.clone();
        let remove_dismiss = dismiss_popover.clone();
        let remove_source = repo.clone();
        list = list.child(repo_source_row(
            &repo,
            active,
            spinning,
            locked,
            {
                let pick = pick.clone();
                move |_, window, app| {
                    app.stop_propagation();
                    let pick = pick.clone();
                    let _ = v.update(app, |this, cx| {
                        this.repo_filter = if this.repo_filter.as_deref() == Some(pick.as_str()) {
                            None
                        } else {
                            Some(pick)
                        };
                        this.revise(cx);
                    });
                    let _ = dismiss_popover.update(app, |state, cx| state.dismiss(window, cx));
                }
            },
            move |_, _, app| {
                app.stop_propagation();
                let pick = pick.clone();
                let _ = reinstall_view.update(app, |this, cx| {
                    this.reinstall_repo_source(&pick, cx);
                });
            },
            move |_, window, app| {
                app.stop_propagation();
                let source = remove_source.clone();
                let count = remove_view
                    .update(app, |this, _| {
                        this.skills
                            .iter()
                            .filter(|skill| skill.source.as_deref() == Some(source.as_str()))
                            .count()
                    })
                    .unwrap_or(0);
                if count == 0 {
                    return;
                }
                let _ = remove_dismiss.update(app, |state, cx| state.dismiss(window, cx));
                let view = remove_view.clone();
                let source = source.clone();
                crate::chrome::open_confirm(
                    window,
                    app,
                    crate::i18n::tf(
                        if count == 1 {
                            "uninstallDialog.title_one"
                        } else {
                            "uninstallDialog.title_other"
                        },
                        &[("count", &count.to_string())],
                    ),
                    crate::i18n::t("uninstallDialog.description"),
                    crate::i18n::t("uninstallDialog.confirmUninstall"),
                    true,
                    move |_, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.uninstall_repo_source(&source, cx);
                        });
                        true
                    },
                );
            },
        ));
    }
    panel = panel.child(
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .child(div().h(px(1.0)).bg(rgb(palette().border_soft)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pt_2()
                    .child(
                        div()
                            .px_2()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("toolbar.repo")),
                    )
                    .child(if repo_rows > ORIGIN_REPOS_VISIBLE {
                        div()
                            .h(px(ORIGIN_REPOS_VIEW_H))
                            .w_full()
                            .min_w_0()
                            .flex_shrink_0()
                            .child(
                                div()
                                    .id("origin-repo-scroll")
                                    .overflow_y_scrollbar()
                                    .child(list),
                            )
                            .into_any_element()
                    } else {
                        list.into_any_element()
                    }),
            ),
    );
    panel
}

fn source_segment(
    view: WeakEntity<MySkillsPage>,
    dismiss_popover: WeakEntity<gpui_kit::component::popover::PopoverState>,
    source_filter: SourceFilter,
) -> TabBar {
    let selected = SourceFilter::ALL
        .iter()
        .position(|kind| *kind == source_filter)
        .unwrap_or(0);
    let mut bar = TabBar::new("source-filter")
        .segmented()
        .w_full()
        .selected_index(selected)
        .on_click(move |ix, window, app| {
            let kind = SourceFilter::ALL[*ix];
            let _ = view.update(app, |this, cx| {
                this.source_filter = kind;
                if kind == SourceFilter::Local {
                    this.repo_filter = None;
                }
                this.revise(cx);
            });
            let _ = dismiss_popover.update(app, |state, cx| state.dismiss(window, cx));
        });
    for kind in SourceFilter::ALL {
        bar = bar.child(Tab::new().label(kind.label()).flex_1());
    }
    bar
}

fn repo_row(
    id: &'static str,
    label: SharedString,
    active: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::chrome::MotionDiv {
    let ink = rgb(if active {
        palette().accent
    } else {
        palette().fg
    });
    menu_row(id, active).child(
        row_hit(id, on_click)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(ink)
                    .child(label),
            )
            .when(active, |hit| hit.child(selection_mark())),
    )
}

/// One repository row: the name filters, then reinstall and remove.
/// Same order as the React origin menu (`RefreshCw`, then `Trash2`).
fn repo_source_row(
    repo: &str,
    active: bool,
    spinning: bool,
    locked: bool,
    on_pick: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_reinstall: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::chrome::MotionDiv {
    let reinstall_tip = crate::i18n::t("toolbar.reinstallSource");
    let remove_tip = crate::i18n::t("toolbar.removeSource");
    let row_id = format!("origin-repo-row-{repo}");
    let mut name =
        repo_name(repo, active).id(ElementId::Name(format!("origin-repo-{repo}").into()));
    if repo.chars().count() > 28 {
        let tip: SharedString = repo.to_string().into();
        name =
            name.tooltip(move |window, cx| crate::chrome::tooltip(tip.clone()).build(window, cx));
    }
    menu_row(ElementId::Name(row_id.clone().into()), active)
        .child(
            row_hit(row_id, on_pick)
                .child(name)
                .when(active, |hit| hit.child(selection_mark())),
        )
        .child(repo_icon_button(
            &format!("origin-repo-reinstall-{repo}"),
            IconName::RefreshCw,
            palette().fg_muted,
            palette().panel_hover,
            reinstall_tip,
            locked,
            spinning,
            on_reinstall,
        ))
        .child(repo_icon_button(
            &format!("origin-repo-remove-{repo}"),
            IconName::Trash,
            palette().fg_muted,
            palette().danger_bg,
            remove_tip,
            false,
            false,
            on_remove,
        ))
}

/// Shared row pitch with the source segment: 32px and one leading edge.
/// The name is a separate hit target so the trailing icon buttons do not
/// also apply the repository filter.
fn menu_row(id: impl Into<ElementId>, active: bool) -> crate::chrome::MotionDiv {
    let id = id.into();
    let key: SharedString = id.to_string().into();
    let rest = if active {
        MotionPaint::new().bg(rgb(palette().accent_soft))
    } else {
        MotionPaint::new()
    };
    let hover = if active {
        MotionPaint::new().bg(rgb(palette().accent_soft))
    } else {
        MotionPaint::new().bg(rgb(palette().panel_hover))
    };
    div()
        .id(id)
        .h(px(32.0))
        .px_2()
        .flex()
        .items_center()
        .gap_1()
        .rounded_md()
        .cursor_pointer()
        .when(active, |row| row.bg(rgb(palette().accent_soft)))
        .interaction_spring(key, true, rest, hover)
}

fn row_hit(
    id: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    div()
        .id(ElementId::Name(format!("{id}-hit").into()))
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .gap_1()
        .on_click(on_click)
}

/// Check at the end of the label. The action icons stay on the trailing edge.
fn selection_mark() -> Div {
    div()
        .w(px(16.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(icon(IconName::Check, 14.0, palette().accent))
}

/// `owner/name` on one line. The owner stays muted; the name takes the
/// remaining width and ellipsizes. Names without a slash are one run of text.
fn repo_name(repo: &str, active: bool) -> Div {
    let name_ink = rgb(if active {
        palette().accent
    } else {
        palette().fg
    });
    let owner_ink = rgb(palette().fg_muted);
    let Some((owner, name)) = repo.split_once('/') else {
        return div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(name_ink)
            .child(repo.to_string());
    };
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap_1()
        .text_xs()
        .child(
            div()
                .max_w(px(104.0))
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(owner_ink)
                .child(owner.to_string()),
        )
        .child(div().flex_shrink_0().text_color(owner_ink).child("/"))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::MEDIUM)
                .text_color(name_ink)
                .child(name.to_string()),
        )
}

fn repo_icon_button(
    id: &str,
    glyph: IconName,
    color: u32,
    hover_bg: u32,
    tip: SharedString,
    dimmed: bool,
    spin: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> crate::chrome::MotionDiv {
    let color = if spin { palette().accent } else { color };
    let live = !dimmed;
    div()
        .id(ElementId::Name(id.to_string().into()))
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .flex_shrink_0()
        .when(dimmed, |d| d.opacity(0.5))
        .when(live, |d| d.cursor_pointer())
        .child(icon_spin(glyph, 14.0, color, spin))
        .tooltip({
            let tip = tip.clone();
            move |window, cx| crate::chrome::tooltip(tip.clone()).build(window, cx)
        })
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            if dimmed {
                return;
            }
            on_click(event, window, cx);
        })
        .interaction_spring(
            id.to_string(),
            live,
            MotionPaint::new(),
            MotionPaint::new().bg(rgb(hover_bg)),
        )
}

pub(super) fn repo_sources(skills: &[Skill]) -> Vec<String> {
    let mut sources: Vec<String> = skills
        .iter()
        .filter_map(|skill| skill.source.clone())
        .filter(|source| !source.is_empty())
        .collect();
    sources.sort();
    sources.dedup();
    sources
}

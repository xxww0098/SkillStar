//! `CreateGroupDialog` — React `CreateGroupModal.tsx`, the deck editor with
//! the member picker: emoji + name + description, pill strip of picked
//! members, filter + checkbox list over installed skills, Cancel/Create bar.
//!
//! Two mounts share this one entity: the Skill Cards page opens it directly
//! (`open_create_group`), and the import dialog's Quick Pack step embeds it
//! in its `Phase::Pack` body. Either way the dialog layer supplies overlay
//! and close chrome; the embedder paints the header row.

use std::collections::HashSet;

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::chrome::{InteractionSpring, MotionPaint, ghost_button, icon, primary_button};
use crate::spawn_domain;
use crate::theme::palette;

/// `EMOJI_OPTIONS` from `CreateGroupModal.tsx`.
const EMOJI_OPTIONS: &[&str] = &[
    "💻", "🚀", "🎨", "🔧", "📦", "🧪", "📊", "🔐", "🌐", "📝", "⚡", "🤖", "🛠️", "📱", "🎯", "🧩",
];

/// Deck-write notification — the opener refreshes whatever page it owns
/// (Skill Cards reloads itself; Quick Pack emits `GroupsChanged`).
type OnCreated = Box<dyn Fn(&mut Window, &mut App)>;

pub(crate) struct CreateGroupDialog {
    name: Entity<InputState>,
    desc: Entity<InputState>,
    filter: Entity<InputState>,
    query: String,
    icon: String,
    emoji_open: bool,
    members: HashSet<String>,
    /// (name, description) for every installed skill — member picker rows.
    all: Vec<(String, String)>,
    /// Existing group names, for the duplicate-name guard.
    names: Vec<String>,
    /// Default name handed to the name field on the first render —
    /// `set_value` needs a `Window`, which only render supplies.
    seed: Option<String>,
    /// Dialog open focuses the shell, not the name field. The first render
    /// moves the caret there once the shell is in the tree.
    focus_name: bool,
    on_created: OnCreated,
    _subs: Vec<Subscription>,
}

impl CreateGroupDialog {
    /// `seed` pre-fills the name (Quick Pack names the deck after the repo);
    /// `members` pre-picks just-installed skills. The picker rows and the
    /// duplicate-name guard load in the background right away.
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        seed: Option<String>,
        members: Vec<String>,
        on_created: OnCreated,
    ) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("createGroupModal.groupName"))
        });
        let desc = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("createGroupModal.description"))
        });
        let filter = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("createGroupModal.searchSkills"))
        });
        // Typing the deck name must re-render: the duplicate-name guard and
        // the Create button state are computed in `render`.
        let subs = vec![
            cx.subscribe_in(&name, window, Self::on_name_event),
            cx.subscribe_in(&filter, window, Self::on_filter_event),
        ];
        let this = Self {
            name,
            desc,
            filter,
            query: String::new(),
            icon: "💻".to_string(),
            emoji_open: false,
            members: members.into_iter().collect(),
            all: Vec::new(),
            names: Vec::new(),
            seed,
            focus_name: true,
            on_created,
            _subs: subs,
        };
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async {
                let skills = ss_skills::installed_skill::list_installed_skills().await;
                let names = tokio::task::spawn_blocking(|| {
                    ss_skills::skill_group::list_groups()
                        .into_iter()
                        .map(|g| g.name)
                        .collect::<Vec<String>>()
                })
                .await;
                (skills, names)
            },
            |this, _cx, (skills, names)| {
                this.all = skills
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| (s.name, s.description))
                    .collect();
                this.names = names.unwrap_or_default();
            },
        );
        this
    }

    /// `handleSave` — React's `createGroup` call passes no `skillSources`,
    /// so the map stays empty here too.
    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        let desc = self.desc.read(cx).value().trim().to_string();
        let mut members: Vec<String> = self.members.iter().cloned().collect();
        members.sort();
        let dup = self.names.iter().any(|n| n == &name);
        if name.is_empty() || members.is_empty() || dup {
            return;
        }
        match ss_skills::skill_group::create_group(
            name,
            desc,
            self.icon.clone(),
            members,
            std::collections::HashMap::new(),
        ) {
            Ok(_) => {
                (self.on_created)(window, cx);
                window.close_dialog(cx);
            }
            Err(err) => crate::notify::toast(Notification::error(format!("{err:#}")), cx),
        }
    }

    /// Name keystrokes only re-render — `render` reads the live value for
    /// the duplicate-name guard and Create enablement.
    fn on_name_event(
        &mut self,
        _state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            cx.notify();
        }
    }

    fn on_filter_event(
        &mut self,
        _state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.query = self.filter.read(cx).value().to_string();
            cx.notify();
        }
    }

    /// `CreateGroupModal` body: emoji well + name, description, pill strip
    /// for picked members, filter + checkbox list of installed skills, and
    /// the Cancel/Create bar. The emoji grid renders as the column's last
    /// absolute child — GPUI paints in child order, so this keeps the
    /// overlay above the inputs it overlaps.
    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if let Some(seed) = self.seed.take() {
            let _ = self
                .name
                .update(cx, |state, cx| state.set_value(seed, window, cx));
        }
        if self.focus_name {
            self.focus_name = false;
            let handle = self.name.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
        let view = cx.entity().downgrade();
        let name = self.name.read(cx).value().trim().to_string();
        let dup = !name.is_empty() && self.names.iter().any(|n| n == &name);
        let can_create = !name.is_empty() && !self.members.is_empty() && !dup;
        let query = self.query.to_lowercase();
        let filtered: Vec<(String, String)> = self
            .all
            .iter()
            .filter(|(n, d)| {
                query.is_empty()
                    || n.to_lowercase().contains(&query)
                    || d.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        let all_marked =
            !filtered.is_empty() && filtered.iter().all(|(n, _)| self.members.contains(n));

        // ── Icon button + name ────────────────────────────────────
        let emoji_view = view.clone();
        let icon_row = div()
            .flex()
            .items_end()
            .gap_3()
            .child(
                div()
                    .id("pack-icon")
                    .size(px(44.0))
                    .rounded_xl()
                    .border_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(20.0))
                    .flex_shrink_0()
                    .cursor_pointer()
                    .border_color(rgb(if self.emoji_open {
                        palette().accent
                    } else if dup {
                        palette().danger
                    } else {
                        palette().border
                    }))
                    .when(self.emoji_open, |d| {
                        d.bg(rgb(palette().accent).alpha(0.05))
                    })
                    .child(self.icon.clone())
                    .on_click(move |_, _window, cx| {
                        let _ = emoji_view.update(cx, |this, cx| {
                            this.emoji_open = !this.emoji_open;
                            cx.notify();
                        });
                    })
                    .interaction_spring(
                        "pack-icon",
                        true,
                        if self.emoji_open {
                            MotionPaint::new().bg(rgb(palette().accent).alpha(0.05))
                        } else {
                            MotionPaint::new()
                        },
                        if self.emoji_open {
                            MotionPaint::new().bg(rgb(palette().accent).alpha(0.05))
                        } else {
                            MotionPaint::new().bg(rgb(palette().panel_hover))
                        },
                    ),
            )
            .child(div().flex_1().min_w_0().child(Input::new(&self.name)));

        let mut body = div()
            .flex()
            .flex_col()
            .px(px(24.0))
            .py(px(16.0))
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(icon_row)
                    .when(dup, |d| {
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .pl(px(56.0))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(palette().danger))
                                .child(icon(IconName::CircleAlert, 14.0, palette().danger))
                                .child(crate::i18n::t("createGroupModal.nameExists")),
                        )
                    }),
            )
            .child(Input::new(&self.desc));

        // ── Selected member pills ─────────────────────────────────
        if !self.members.is_empty() {
            let installed: HashSet<&String> = self.all.iter().map(|(n, _)| n).collect();
            let mut pills = div().flex().flex_wrap().gap(px(6.0)).pr_1();
            // Installed members first, orphans after — React's pill sort.
            let mut members: Vec<String> = self.members.iter().cloned().collect();
            members.sort_by_key(|n| !installed.contains(n));
            for member in members {
                let orphan = !installed.contains(&member);
                let pill_key = format!("pack-pill-{member}");
                let name = member.clone();
                let remove = view.clone();
                let mut pill = div()
                    .id(ElementId::Name(pill_key.clone().into()))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py(px(2.0))
                    .rounded_md()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .bg(rgb(if orphan {
                        palette().danger
                    } else {
                        palette().accent
                    })
                    .alpha(0.10))
                    .text_color(rgb(if orphan {
                        palette().danger
                    } else {
                        palette().accent
                    }))
                    .child(member);
                if orphan {
                    pill = pill.line_through();
                }
                let pill = pill
                    .child(icon(
                        IconName::X,
                        10.0,
                        if orphan {
                            palette().danger
                        } else {
                            palette().accent
                        },
                    ))
                    .on_click(move |_, _window, cx| {
                        let name = name.clone();
                        let _ = remove.update(cx, |this, cx| {
                            this.members.remove(&name);
                            cx.notify();
                        });
                    })
                    .interaction_spring(
                        pill_key,
                        true,
                        MotionPaint::new().bg(rgb(if orphan {
                            palette().danger
                        } else {
                            palette().accent
                        })
                        .alpha(0.10)),
                        MotionPaint::new().bg(rgb(if orphan {
                            palette().danger
                        } else {
                            palette().accent
                        })
                        .alpha(0.20)),
                    );
                pills = pills.child(pill);
            }
            body = body.child(div().max_h(px(140.0)).overflow_y_scrollbar().child(pills));
        }

        // ── Search + select-all ───────────────────────────────────
        let all_view = view.clone();
        let filtered_ids: Vec<String> = filtered.iter().map(|(n, _)| n.clone()).collect();
        body = body.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&self.filter).prefix(
                            div().pl(px(4.0)).flex().items_center().child(icon(
                                IconName::Search,
                                14.0,
                                palette().fg_muted,
                            )),
                        )),
                )
                .when(!filtered.is_empty(), |d| {
                    d.child(
                        div()
                            .id("pack-select-all")
                            .h(px(32.0))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded_md()
                            .bg(rgb(palette().well))
                            .cursor_pointer()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .flex_shrink_0()
                            .child(if all_marked {
                                crate::i18n::t("common.deselectAll")
                            } else {
                                crate::i18n::t("common.selectAll")
                            })
                            .on_click(move |_, _window, cx| {
                                let ids = filtered_ids.clone();
                                let _ = all_view.update(cx, |this, cx| {
                                    if all_marked {
                                        for n in ids {
                                            this.members.remove(&n);
                                        }
                                    } else {
                                        for n in ids {
                                            if !this.members.contains(&n) {
                                                this.members.insert(n);
                                            }
                                        }
                                    }
                                    cx.notify();
                                });
                            })
                            .interaction_spring(
                                "pack-select-all",
                                true,
                                MotionPaint::new().bg(rgb(palette().well)),
                                MotionPaint::new().bg(rgb(palette().panel_hover)),
                            ),
                    )
                }),
        );

        // ── Member list ───────────────────────────────────────────
        let mut rows = div().flex().flex_col().gap(px(2.0));
        if self.all.is_empty() {
            rows = rows.child(
                div()
                    .py(px(24.0))
                    .text_center()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t("skillCards.noSkillsInstalled")),
            );
        } else if filtered.is_empty() {
            rows = rows.child(
                div()
                    .py(px(24.0))
                    .text_center()
                    .text_sm()
                    .text_color(rgb(palette().fg_muted))
                    .child(crate::i18n::t("createGroupModal.noSkillsFound")),
            );
        }
        for (ix, (skill_name, _desc)) in filtered.iter().enumerate() {
            let picked = self.members.contains(skill_name);
            let name = skill_name.clone();
            let toggle = view.clone();
            rows = rows.child(
                div()
                    .id(ElementId::named_usize("pack-row", ix))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded_lg()
                    .cursor_pointer()
                    .when(picked, |d| d.bg(rgb(palette().accent).alpha(0.05)))
                    .child(
                        div()
                            .size(px(16.0))
                            .flex_shrink_0()
                            .rounded(px(4.0))
                            .border(px(1.5))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(picked, |d| {
                                d.bg(rgb(palette().accent))
                                    .border_color(rgb(palette().accent))
                            })
                            .when(!picked, |d| {
                                d.border_color(rgb(palette().fg_muted).alpha(0.30))
                            })
                            .when(picked, |d| {
                                d.child(icon(IconName::Check, 10.0, palette().on_accent))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.0))
                            .font_weight(if picked {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(rgb(if picked {
                                palette().accent
                            } else {
                                palette().fg
                            }))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(skill_name.clone()),
                    )
                    .on_click(move |_, _window, cx| {
                        let name = name.clone();
                        let _ = toggle.update(cx, |this, cx| {
                            if !this.members.remove(&name) {
                                this.members.insert(name);
                            }
                            cx.notify();
                        });
                    })
                    .interaction_spring(
                        format!("pack-row-{ix}"),
                        true,
                        if picked {
                            MotionPaint::new().bg(rgb(palette().accent).alpha(0.05))
                        } else {
                            MotionPaint::new()
                        },
                        if picked {
                            MotionPaint::new().bg(rgb(palette().accent).alpha(0.05))
                        } else {
                            MotionPaint::new().bg(rgb(palette().panel_hover))
                        },
                    ),
            );
        }
        body = body.child(
            div()
                .max_h(px(160.0))
                .overflow_y_scrollbar()
                .rounded_lg()
                .child(rows),
        );

        // ── Footer + emoji overlay ────────────────────────────────
        let cancel = view.clone();
        let create = view.clone();
        let mut col = div().flex().flex_col().relative().child(body).child(
            div()
                .px(px(24.0))
                .py(px(14.0))
                .border_t_1()
                .border_color(rgb(palette().border_soft))
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .child(ghost_button(
                    "pack-cancel",
                    crate::i18n::t("createGroupModal.cancel"),
                    cancel,
                    |_this, window, cx| window.close_dialog(cx),
                ))
                .child(
                    primary_button(
                        "pack-create",
                        crate::i18n::t("createGroupModal.create"),
                        create,
                        |this, window, cx| this.create(window, cx),
                    )
                    .when(!can_create, |d| d.opacity(0.5)),
                ),
        );

        if self.emoji_open {
            // The grid floats over the name field. Occlude so that field
            // does not take the hover face while the pointer is on an emoji.
            let mut grid = div()
                .occlude()
                .flex()
                .flex_wrap()
                .gap(px(2.0))
                .w(px(196.0))
                .p_2()
                .rounded_xl()
                .border_1()
                .border_color(rgb(palette().border))
                .bg(rgb(palette().card))
                .shadow_lg();
            for (ix, emoji) in EMOJI_OPTIONS.iter().enumerate() {
                let pick = emoji.to_string();
                let active = self.icon == *emoji;
                let pick_view = view.clone();
                grid = grid.child(
                    div()
                        .id(ElementId::named_usize("pack-emoji", ix))
                        .size(px(36.0))
                        .rounded_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_lg()
                        .cursor_pointer()
                        .when(active, |d| d.bg(rgb(palette().accent).alpha(0.10)))
                        .child(*emoji)
                        .on_click(move |_, _window, cx| {
                            let pick = pick.clone();
                            let _ = pick_view.update(cx, |this, cx| {
                                this.icon = pick;
                                this.emoji_open = false;
                                cx.notify();
                            });
                        })
                        .interaction_spring(
                            format!("pack-emoji-{ix}"),
                            true,
                            if active {
                                MotionPaint::new().bg(rgb(palette().accent).alpha(0.10))
                            } else {
                                MotionPaint::new()
                            },
                            if active {
                                MotionPaint::new().bg(rgb(palette().accent).alpha(0.10))
                            } else {
                                MotionPaint::new().bg(rgb(palette().panel_hover))
                            },
                        ),
                );
            }
            col = col.child(
                div()
                    .absolute()
                    // body py(16) + 44px icon button + 8px gap, minus the
                    // px(24) horizontal padding → React's `top-full mt-2`.
                    .top(px(64.0))
                    .left(px(24.0))
                    .child(grid),
            );
        }
        col
    }
}

impl Render for CreateGroupDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_body(window, cx)
    }
}

/// Open the deck editor as its own centered dialog — the Skill Cards page's
/// "New Deck" entry. Same surface chrome as the import modal: `p_0` dialog,
/// modal-surface fill, and the plain `CreateGroupModal` header (no icon).
pub(crate) fn open_create_group(
    window: &mut Window,
    cx: &mut App,
    on_created: impl Fn(&mut Window, &mut App) + 'static,
) {
    let entity = cx.new(|cx| {
        CreateGroupDialog::new(window, cx, None, Vec::new(), Box::new(on_created))
    });
    crate::chrome::open_centered(
        window,
        cx,
        480.0,
        crate::chrome::DialogChrome::Flush,
        move |dialog, frame, _, _| {
            // `.modal-surface`: rounded-xl, sidebar fill (card on paper).
            let surface = if crate::theme::is_light() {
                palette().card
            } else {
                palette().panel
            };
            let column = div()
                .flex()
                .flex_col()
                .w_full()
                .child(header())
                .child(entity.clone());
            dialog
                .w(px(512.0))
                .p_0()
                .rounded(px(12.0))
                .bg(rgb(surface))
                .border_color(rgb(palette().border))
                .child(frame.measure(column))
        },
    )
}

/// The `CreateGroupModal` header — `px-6 pt-4 pb-3` + border-b, no icon well.
fn header() -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .w_full()
        .px(px(24.0))
        .pt(px(16.0))
        .pb(px(12.0))
        .flex_shrink_0()
        .border_b_1()
        .border_color(rgb(palette().border_soft))
        .child(
            div()
                .text_size(px(16.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(palette().fg))
                .child(crate::i18n::t("createGroupModal.newGroup")),
        )
}

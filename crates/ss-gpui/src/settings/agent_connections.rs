//! Agent connections. React source:
//! `src/features/settings/sections/AgentConnectionsSection.tsx`.
//! Not ported: custom-agent dialog, batch link.

use std::path::Path;

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_skills::agents::AgentProfile;

use crate::chrome::{InteractionSpring, MotionPaint};
use crate::i18n::{t, tf};
use crate::theme::palette;

use super::managed_skills::global_skills_target_key;
use super::{AgentFilter, SettingsPage, SettingsSection, card, section_shell};

const VISIBLE_AGENTS: usize = 10;

impl SettingsPage {
    pub(crate) fn render_agents(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let enabled = self
            .profiles
            .iter()
            .filter(|profile| profile.enabled)
            .count();
        let summary = div()
            .px_4()
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .border_b_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().well))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(div().size(px(8.0)).rounded_full().bg(rgb(if enabled > 0 {
                        palette().ok
                    } else {
                        palette().fg_muted
                    })))
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(palette().fg))
                            .child(t("settings.manualAgentActivation")),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(tf(
                        "settings.activeCount",
                        &[
                            ("enabled", &enabled.to_string()),
                            ("total", &self.profiles.len().to_string()),
                        ],
                    )),
            );

        let mut body = card().child(summary);
        if self.profiles.is_empty() {
            body = body.child(
                div()
                    .px_5()
                    .py_8()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(t("settings.noAgentsRegistered")),
            );
        } else {
            body = body.child(self.agent_filter_bar(view.clone()));
            let matched = self.matched_agents();
            let filter_active = self.agent_filter_active();
            let hidden = if filter_active {
                0
            } else {
                matched.len().saturating_sub(VISIBLE_AGENTS)
            };
            let shown = if filter_active || self.show_all_agents {
                matched.len()
            } else {
                VISIBLE_AGENTS.min(matched.len())
            };
            if matched.is_empty() {
                body = body.child(self.agent_empty_filters(view.clone()));
            } else {
                for (index, profile) in matched.iter().take(shown).enumerate() {
                    let last = index + 1 == shown && hidden == 0;
                    body = body.child(self.agent_row(profile, last, view.clone()));
                }
            }
            if hidden > 0 {
                body = body.child(self.agent_show_more(hidden, view.clone()));
            }
        }
        section_shell(SettingsSection::AgentConnections, None, None, body)
    }

    fn agent_filter_active(&self) -> bool {
        !self.agent_query.trim().is_empty() || self.agent_filter != AgentFilter::All
    }

    fn matched_agents(&self) -> Vec<&AgentProfile> {
        let mut ordered: Vec<&AgentProfile> = self.profiles.iter().collect();
        ordered.sort_by_key(|profile| !profile.enabled);
        let query = self.agent_query.trim().to_lowercase();
        let searched: Vec<&AgentProfile> = if query.is_empty() {
            ordered
        } else {
            ordered
                .into_iter()
                .filter(|profile| {
                    profile.display_name.to_lowercase().contains(&query)
                        || profile.id.to_lowercase().contains(&query)
                })
                .collect()
        };
        searched
            .into_iter()
            .filter(|profile| match self.agent_filter {
                AgentFilter::All => true,
                AgentFilter::Enabled => profile.enabled,
                AgentFilter::Disabled => !profile.enabled,
            })
            .collect()
    }

    fn status_counts(&self) -> [usize; 3] {
        let mut ordered: Vec<&AgentProfile> = self.profiles.iter().collect();
        ordered.sort_by_key(|profile| !profile.enabled);
        let query = self.agent_query.trim().to_lowercase();
        let searched: Vec<&AgentProfile> = ordered
            .into_iter()
            .filter(|profile| {
                query.is_empty()
                    || profile.display_name.to_lowercase().contains(&query)
                    || profile.id.to_lowercase().contains(&query)
            })
            .collect();
        let enabled = searched.iter().filter(|profile| profile.enabled).count();
        [searched.len(), enabled, searched.len() - enabled]
    }

    fn agent_filter_bar(&self, view: WeakEntity<Self>) -> Div {
        let counts = self.status_counts();
        let filters = [
            (AgentFilter::All, "settings.filterAgentsAll", "all"),
            (
                AgentFilter::Enabled,
                "settings.filterAgentsEnabled",
                "enabled",
            ),
            (
                AgentFilter::Disabled,
                "settings.filterAgentsDisabled",
                "disabled",
            ),
        ];
        let query = self.agent_query.clone();
        let v = view.clone();
        let bar = div()
            .px_4()
            .py(px(10.0))
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(palette().border))
            .child(
                div()
                    .flex_1()
                    .min_w(px(140.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.agent_search)),
                    )
                    .when(!query.is_empty(), |row| {
                        let v = v.clone();
                        row.child(
                            div()
                                .id("agent-search-clear")
                                .size(px(24.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_md()
                                .cursor_pointer()
                                .interaction_spring(
                                    "agent-search-clear",
                                    true,
                                    MotionPaint::new(),
                                    MotionPaint::new().bg(rgb(palette().card_hover)),
                                )
                                .child(
                                    Icon::new(IconName::X)
                                        .size(px(12.0))
                                        .text_color(rgb(palette().fg_muted)),
                                )
                                .on_click(move |_, window, cx| {
                                    let _ = v.update(cx, |this, cx| {
                                        this.agent_query.clear();
                                        this.agent_search.update(cx, |input, cx| {
                                            input.set_value("", window, cx);
                                        });
                                        cx.notify();
                                    });
                                }),
                        )
                    }),
            );

        let mut pills = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.0))
            .h(px(32.0))
            .p(px(2.0))
            .rounded_lg()
            .border_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().panel));
        for (index, (filter, label, slug)) in filters.into_iter().enumerate() {
            let active = self.agent_filter == filter;
            let v = view.clone();
            pills = pills.child(
                div()
                    .id(ElementId::Name(format!("agent-filter-{slug}").into()))
                    .h_full()
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if active {
                        palette().on_accent
                    } else {
                        palette().fg_muted
                    }))
                    .when(active, |d| d.bg(rgb(palette().accent)))
                    .interaction_spring(
                        format!("agent-filter-{slug}"),
                        true,
                        if active {
                            MotionPaint::new()
                                .bg(rgb(palette().accent))
                                .fg(rgb(palette().on_accent))
                        } else {
                            MotionPaint::new().fg(rgb(palette().fg_muted))
                        },
                        if active {
                            MotionPaint::new()
                                .bg(rgb(palette().accent))
                                .fg(rgb(palette().on_accent))
                        } else {
                            MotionPaint::new()
                                .bg(rgb(palette().panel_hover))
                                .fg(rgb(palette().fg))
                        },
                    )
                    .child(t(label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(if active {
                                palette().on_accent
                            } else {
                                palette().fg_faint
                            }))
                            .child(counts[index].to_string()),
                    )
                    .on_click(move |_, _, cx| {
                        let _ = v.update(cx, |this, cx| {
                            this.agent_filter = filter;
                            cx.notify();
                        });
                    }),
            );
        }
        bar.child(pills)
    }

    fn agent_empty_filters(&self, view: WeakEntity<Self>) -> Div {
        div()
            .px_5()
            .py_8()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(t("settings.noAgentsMatchFilter")),
            )
            .child(
                div()
                    .id("agent-clear-filters")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(palette().accent))
                    .interaction_spring(
                        "agent-clear-filters",
                        true,
                        MotionPaint::new(),
                        MotionPaint::new().bg(rgb(palette().accent_soft)),
                    )
                    .child(t("settings.clearAgentFilters"))
                    .on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.agent_query.clear();
                            this.agent_filter = AgentFilter::All;
                            this.agent_search.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                            cx.notify();
                        });
                    }),
            )
    }

    fn agent_show_more(&self, hidden: usize, view: WeakEntity<Self>) -> impl IntoElement {
        let open = self.show_all_agents;
        div()
            .id("agent-show-more")
            .w_full()
            .px_5()
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.0))
            .border_t_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().well))
            .cursor_pointer()
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(palette().fg_muted))
            .interaction_spring(
                "agent-show-more",
                true,
                MotionPaint::new()
                    .bg(rgb(palette().well))
                    .fg(rgb(palette().fg_muted)),
                MotionPaint::new()
                    .bg(rgb(palette().card_hover))
                    .fg(rgb(palette().fg)),
            )
            .child(if open {
                t("settings.collapseAgentList")
            } else {
                tf(
                    "settings.showRemainingAgents",
                    &[("count", &hidden.to_string())],
                )
            })
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(px(14.0))
                .text_color(rgb(palette().fg_muted)),
            )
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.show_all_agents = !this.show_all_agents;
                    cx.notify();
                });
            })
    }

    fn agent_row(&self, profile: &AgentProfile, last: bool, view: WeakEntity<Self>) -> Div {
        let expanded = self.expanded_agent.as_deref() == Some(profile.id.as_str());
        let linked = self
            .linked_skills
            .get(&profile.id)
            .map(|names| names.len() as u32)
            .unwrap_or(profile.synced_count);
        let id = profile.id.clone();
        let enabled = profile.enabled;
        let mut head = div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py(px(10.0))
            .when(!last, |row| {
                row.border_b_1().border_color(rgb(palette().border))
            })
            .child(agent_glyph(&profile.id, enabled))
            .child(self.agent_identity(profile, view.clone()));

        if linked > 0 || expanded {
            let expand_id = id.clone();
            let v = view.clone();
            head = head.child(
                div()
                    .id(ElementId::Name(format!("agent-links-{id}").into()))
                    .flex_shrink_0()
                    .px_2()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_xs()
                    .when(expanded, |d| {
                        d.bg(rgb(palette().accent_soft))
                            .text_color(rgb(palette().accent))
                    })
                    .when(!expanded, |d| d.text_color(rgb(palette().fg_muted)))
                    .child(format!("{linked} {}", t("settings.linked")))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(px(12.0))
                        .text_color(rgb(if expanded {
                            palette().accent
                        } else {
                            palette().fg_muted
                        })),
                    )
                    .on_click(move |_, _, cx| {
                        let expand_id = expand_id.clone();
                        let _ = v.update(cx, |this, cx| this.toggle_agent_expand(&expand_id, cx));
                    })
                    .interaction_spring(
                        format!("agent-links-{id}"),
                        true,
                        if expanded {
                            MotionPaint::new()
                                .bg(rgb(palette().accent_soft))
                                .fg(rgb(palette().accent))
                        } else {
                            MotionPaint::new().fg(rgb(palette().fg_muted))
                        },
                        if expanded {
                            MotionPaint::new()
                                .bg(rgb(palette().accent_soft))
                                .fg(rgb(palette().accent))
                        } else {
                            MotionPaint::new()
                                .bg(rgb(palette().well))
                                .fg(rgb(palette().fg))
                        },
                    ),
            );
        }

        let toggle_id = id.clone();
        head = head.child(Self::toggle(
            &format!("agent-toggle-{id}"),
            enabled,
            view.clone(),
            move |this, cx| this.toggle_agent_enabled(&toggle_id, cx),
        ));

        let mut row = div().flex().flex_col().w_full().child(head);
        if expanded {
            row = row.child(self.linked_panel(&id, view));
        }
        row
    }

    fn agent_identity(&self, profile: &AgentProfile, view: WeakEntity<Self>) -> Div {
        let enabled = profile.enabled;
        let mut block = div().flex_1().min_w_0().flex().flex_col().gap_1().child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(palette().fg))
                        .child(profile.display_name.clone()),
                )
                .child(status_badge(enabled)),
        );
        let paths = display_paths(profile);
        if !paths.is_empty() {
            let mut chips = div().flex().flex_row().flex_wrap().gap_1();
            for path in paths {
                chips = chips.child(
                    div()
                        .max_w_full()
                        .px_1()
                        .py(px(1.0))
                        .rounded_md()
                        .bg(rgb(palette().well))
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .truncate()
                        .child(path),
                );
            }
            block = block.child(chips);
        }
        if profile.enabled && profile.has_global_skills() {
            block = block.child(self.managed_skills_row(profile, view));
        }
        let shared = shared_names(&self.profiles, profile);
        if !shared.is_empty() {
            block = block.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .truncate()
                    .child(tf(
                        "settings.sharedSkillsTarget",
                        &[("names", &shared.join(", "))],
                    )),
            );
        }
        block
    }

    fn linked_panel(&self, agent_id: &str, view: WeakEntity<Self>) -> Div {
        let skills = self
            .linked_skills
            .get(agent_id)
            .cloned()
            .unwrap_or_default();
        let profile = self.profiles.iter().find(|profile| profile.id == agent_id);
        let pending = profile.is_some_and(|profile| self.managed_target_pending(profile));
        let clearable = profile.is_some_and(|profile| profile.has_global_skills());
        let mut panel = div()
            .px_4()
            .py(px(10.0))
            .border_t_1()
            .border_color(rgb(palette().border))
            .bg(rgb(palette().well));
        if skills.is_empty() {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .child(t("settings.noSkillsLinked")),
            );
        } else {
            if clearable {
                panel = panel.child(linked_panel_actions(agent_id, pending, view.clone()));
            }
            let mut chips = div().flex().flex_row().flex_wrap().gap(px(6.0));
            for name in skills {
                let skill = name.clone();
                let aid = agent_id.to_string();
                let unlink_key = format!("unlink-{aid}-{skill}");
                let v = view.clone();
                chips = chips.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py(px(2.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(palette().border))
                        .bg(rgb(palette().card))
                        .text_xs()
                        .text_color(rgb(palette().fg))
                        .child(name)
                        .child({
                            let mut button = div().p(px(2.0)).rounded_md().child(
                                Icon::new(IconName::X)
                                    .size(px(10.0))
                                    .text_color(rgb(palette().fg_muted)),
                            );
                            if pending {
                                button = button.opacity(0.5);
                            } else {
                                button = button.cursor_pointer();
                            }
                            button
                                .id(ElementId::Name(unlink_key.clone().into()))
                                .on_click(move |_, _, cx| {
                                    if pending {
                                        return;
                                    }
                                    let skill = skill.clone();
                                    let aid = aid.clone();
                                    let _ = v.update(cx, |this, cx| {
                                        this.unlink_linked_skill(&aid, &skill, cx);
                                    });
                                })
                                .interaction_spring(
                                    unlink_key,
                                    !pending,
                                    MotionPaint::new(),
                                    MotionPaint::new().bg(rgb(palette().danger_bg)),
                                )
                        }),
                );
            }
            panel = panel.child(chips);
        }
        panel
    }
}

/// Header of the expanded linked panel: one click sweeps every managed card
/// out of this Agent's physical Global skills directory. The count lives on
/// the row's own toggle, so the button stays the only action here.
fn linked_panel_actions(agent_id: &str, pending: bool, view: WeakEntity<SettingsPage>) -> Div {
    let id = agent_id.to_string();
    let unlink_all_key = format!("unlink-all-{id}");
    let mut button = div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1()
        .px_2()
        .py(px(2.0))
        .rounded_md()
        .border_1()
        .border_color(rgb(palette().border))
        .text_xs()
        .font_weight(FontWeight::MEDIUM);
    if pending {
        button = button.opacity(0.5).text_color(rgb(palette().fg_muted));
    } else {
        button = button.cursor_pointer().text_color(rgb(palette().danger));
    }
    div()
        .mb(px(8.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_end()
        .child(
            button
                .id(ElementId::Name(unlink_all_key.clone().into()))
                .child(Icon::new(IconName::Link2Off).size(px(12.0)))
                .child(t("settings.unlinkAllFromAgent"))
                .on_click(move |_, _, cx| {
                    if pending {
                        return;
                    }
                    let id = id.clone();
                    let _ = view.update(cx, |this, cx| {
                        this.unlink_all_linked_skills(&id, cx);
                    });
                })
                .interaction_spring(
                    unlink_all_key,
                    !pending,
                    MotionPaint::new().border(rgb(palette().border)),
                    MotionPaint::new()
                        .bg(rgb(palette().danger_bg))
                        .border(rgb(if pending {
                            palette().border
                        } else {
                            palette().danger_border
                        })),
                ),
        )
}

fn status_badge(enabled: bool) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .px(px(6.0))
        .py(px(1.0))
        .rounded_md()
        .bg(rgb(if enabled {
            palette().ok_bg
        } else {
            palette().well
        }))
        .child(div().size(px(4.0)).rounded_full().bg(rgb(if enabled {
            palette().ok
        } else {
            palette().fg_muted
        })))
        .child(
            div()
                .text_xs()
                .text_color(rgb(if enabled {
                    palette().ok
                } else {
                    palette().fg_muted
                }))
                .child(t(if enabled {
                    "settings.agentEnabled"
                } else {
                    "settings.agentDisabled"
                })),
        )
}

fn agent_glyph(id: &str, enabled: bool) -> Div {
    let mut glyph = img(crate::agent_icons::agent_icon_path(id))
        .w(px(16.0))
        .h(px(16.0));
    if !enabled {
        glyph = glyph.grayscale(true).opacity(0.65);
    }
    div()
        .size(px(32.0))
        .flex_shrink_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(if enabled {
            palette().accent_soft_edge
        } else {
            palette().border
        }))
        .bg(rgb(palette().card))
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
}

fn display_paths(profile: &AgentProfile) -> Vec<String> {
    let primary = home_prefix(&profile.global_skills_dir);
    if primary.is_empty() {
        return Vec::new();
    }
    if profile.id != "codex" {
        return vec![primary];
    }
    let legacy = std::env::var("HOME")
        .map(|home| format!("{home}/.agents/skills"))
        .unwrap_or_else(|_| "~/.agents/skills".into());
    let legacy_shown = home_prefix(Path::new(&legacy));
    if primary.eq_ignore_ascii_case(&legacy_shown) {
        vec![primary]
    } else {
        vec![primary, legacy_shown]
    }
}

fn home_prefix(path: &Path) -> String {
    let text = path.display().to_string();
    if let Ok(home) = std::env::var("HOME") {
        if let Some(rest) = text.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    text
}

fn shared_names(profiles: &[AgentProfile], profile: &AgentProfile) -> Vec<String> {
    if !profile.has_global_skills() {
        return Vec::new();
    }
    let key = global_skills_target_key(&profile.global_skills_dir);
    profiles
        .iter()
        .filter(|candidate| {
            candidate.id != profile.id
                && candidate.has_global_skills()
                && global_skills_target_key(&candidate.global_skills_dir) == key
        })
        .map(|candidate| candidate.display_name.clone())
        .collect()
}

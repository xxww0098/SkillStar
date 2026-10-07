//! Projects — registered projects + per-agent skill configuration.
//! React source: `src/pages/Projects.tsx` + `features/projects/`.

mod agent_item;
mod detail;

pub(crate) use agent_item::render_agent_item;

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;
use ss_skills::agents::{AgentProfile, list_profiles};
use ss_skills::projects::{
    ProjectDeployMode, ProjectEntry, list_projects, load_skills_list, register_project,
    save_and_sync,
};

use crate::chrome::{
    InteractionSpring, MotionPaint, bar_count, bar_primary, icon, page_chrome, page_toolbar,
    toolbar_search,
};
use crate::nav::NavPage;
use crate::theme::palette;
use detail::{DetectedRule, render_project_detail, scan_project_rules};

fn load_hub_skill_names() -> Vec<String> {
    let mut names = ss_skills::installer::installed_names();
    names.sort();
    names
}

/// Registry of registered projects + per-project skill assignments.
/// Two-column master-detail UI replicating React `Projects.tsx`.
pub struct ProjectsPage {
    pub projects: Vec<ProjectEntry>,
    pub selected_project: Option<ProjectEntry>,
    pub search: Option<Entity<InputState>>,
    pub search_query: String,
    pub _subscription: Option<Subscription>,
    // Working state for the currently selected project
    pub agent_skills: HashMap<String, Vec<String>>,
    pub deploy_modes: HashMap<String, ProjectDeployMode>,
    pub expanded_agent: Option<String>,
    pub dirty: bool,
    pub status_message: Option<String>,
    pub hub_skills: Vec<String>,
    pub profiles: Vec<AgentProfile>,
    pub detected_rules: Vec<DetectedRule>,
}

impl ProjectsPage {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        let projects = list_projects();
        let first = projects.first().cloned();
        let profiles = list_profiles();
        let hub_skills = load_hub_skill_names();

        let mut this = Self {
            projects,
            selected_project: None,
            search: None,
            search_query: String::new(),
            _subscription: None,
            agent_skills: HashMap::new(),
            deploy_modes: HashMap::new(),
            expanded_agent: None,
            dirty: false,
            status_message: None,
            hub_skills,
            profiles,
            detected_rules: Vec::new(),
        };

        if let Some(proj) = first {
            this.set_active_project(proj);
        }

        this
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.projects = list_projects();
        self.profiles = list_profiles();
        self.hub_skills = load_hub_skill_names();

        let prev_name = self.selected_project.as_ref().map(|p| p.name.clone());
        if let Some(name) = prev_name
            && let Some(current) = self.projects.iter().find(|p| p.name == name).cloned()
        {
            self.set_active_project(current);
        } else if let Some(first) = self.projects.first().cloned() {
            self.set_active_project(first);
        } else {
            self.selected_project = None;
            self.agent_skills.clear();
            self.deploy_modes.clear();
            self.detected_rules.clear();
        }

        cx.notify();
    }

    /// Settings changed which agents are enabled. Do not call `refresh`:
    /// that reloads the selected project and would drop unsaved edits.
    pub(crate) fn reload_agent_profiles(&mut self, cx: &mut Context<Self>) {
        self.profiles = list_profiles();
        cx.notify();
    }

    fn set_active_project(&mut self, project: ProjectEntry) {
        let list = load_skills_list(&project.name).unwrap_or_default();
        self.agent_skills = list.agents;
        self.deploy_modes = list.deploy_modes;
        self.detected_rules = scan_project_rules(&project.path);
        self.expanded_agent = self.agent_skills.keys().next().cloned();
        self.dirty = false;
        self.status_message = None;
        self.selected_project = Some(project);
    }

    pub fn select_project(&mut self, project: ProjectEntry, cx: &mut Context<Self>) {
        self.set_active_project(project);
        cx.notify();
    }

    pub fn toggle_agent(&mut self, agent_id: &str, cx: &mut Context<Self>) {
        let is_enabled = self.agent_skills.contains_key(agent_id);
        if is_enabled {
            self.agent_skills.remove(agent_id);
            if self.expanded_agent.as_deref() == Some(agent_id) {
                self.expanded_agent = None;
            }
        } else {
            self.agent_skills.insert(agent_id.to_string(), Vec::new());
            self.expanded_agent = Some(agent_id.to_string());
        }
        self.dirty = true;
        cx.notify();
    }

    pub fn toggle_deploy_mode(&mut self, rel_path: &str, cx: &mut Context<Self>) {
        let current_mode = self
            .deploy_modes
            .get(rel_path)
            .copied()
            .unwrap_or(ProjectDeployMode::Symlink);
        let next_mode = match current_mode {
            ProjectDeployMode::Symlink => ProjectDeployMode::Copy,
            ProjectDeployMode::Copy => ProjectDeployMode::Symlink,
        };
        self.deploy_modes.insert(rel_path.to_string(), next_mode);
        self.dirty = true;
        cx.notify();
    }

    pub fn add_skill_to_agent(&mut self, agent_id: &str, skill: String, cx: &mut Context<Self>) {
        if let Some(skills) = self.agent_skills.get_mut(agent_id) {
            if !skills.contains(&skill) {
                skills.push(skill);
                self.dirty = true;
                cx.notify();
            }
        }
    }

    pub fn remove_skill_from_agent(&mut self, agent_id: &str, skill: &str, cx: &mut Context<Self>) {
        if let Some(skills) = self.agent_skills.get_mut(agent_id) {
            skills.retain(|s| s != skill);
            self.dirty = true;
            cx.notify();
        }
    }

    pub fn save_and_sync_current(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project.clone() else {
            return;
        };

        match save_and_sync(
            &project.path,
            self.agent_skills.clone(),
            self.deploy_modes.clone(),
        ) {
            Ok((_, count)) => {
                self.dirty = false;
                self.status_message =
                    Some(format!("Successfully deployed {count} link(s) to project"));
                self.refresh(cx);
            }
            Err(err) => {
                self.status_message = Some(format!("Deploy failed: {err}"));
                cx.notify();
            }
        }
    }

    fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() {
            return;
        }
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("projects.searchPlaceholder"))
        });
        self._subscription = Some(cx.subscribe_in(
            &search,
            window,
            |this, _state, event: &InputEvent, _window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let Some(search) = &this.search else { return };
                this.search_query = search.read(cx).value().to_string();
                cx.notify();
            },
        ));
        self.search = Some(search);
    }

    pub fn sync_language(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(search) = &self.search {
            crate::i18n::sync_placeholder(
                search,
                crate::i18n::t("projects.searchPlaceholder"),
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn render_project_list(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let query = self.search_query.trim().to_lowercase();
        let filtered: Vec<&ProjectEntry> = self
            .projects
            .iter()
            .filter(|p| {
                if query.is_empty() {
                    return true;
                }
                p.name.to_lowercase().contains(&query) || p.path.to_lowercase().contains(&query)
            })
            .collect();

        // Fixed rail (~w-72). Height comes from the split row; the empty
        // state is the flex_1 child so it centers in this pane only.
        let mut list_col = div()
            .w(px(288.0))
            .min_w(px(288.0))
            .max_w(px(320.0))
            .flex_shrink_0()
            .flex_grow_0()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(palette().border));

        if self.projects.is_empty() {
            list_col = list_col.child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .gap_2()
                    .child(icon(IconName::Folder, 28.0, palette().fg_muted))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_align(TextAlign::Center)
                            .text_color(rgb(palette().fg))
                            .child(crate::i18n::t("projects.emptyTitle")),
                    )
                    .child(
                        div()
                            .w_full()
                            .text_xs()
                            .text_align(TextAlign::Center)
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("projects.emptyDesc")),
                    ),
            );
        } else if filtered.is_empty() {
            list_col = list_col.child(
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px_3()
                    .child(
                        div()
                            .text_xs()
                            .text_align(TextAlign::Center)
                            .text_color(rgb(palette().fg_muted))
                            .child(crate::i18n::t("projects.noMatching")),
                    ),
            );
        } else {
            let mut rows = div()
                .w_full()
                .flex_grow_0()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .flex_nowrap()
                .justify_start()
                .items_stretch()
                .gap_1()
                .p_2();
            for project in filtered {
                let name = project.name.clone();
                let path = project.path.clone();
                let is_selected =
                    self.selected_project.as_ref().map(|p| p.name.as_str()) == Some(name.as_str());

                let card_view = view.clone();
                let proj_clone = project.clone();

                let card = div()
                    .id(ElementId::Name(format!("proj-card-{name}").into()))
                    .w_full()
                    .flex_grow_0()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2_5()
                    .rounded_lg()
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(if is_selected {
                        palette().accent
                    } else {
                        palette().border
                    }))
                    .bg(rgb(if is_selected {
                        palette().accent_soft
                    } else {
                        palette().card
                    }))
                    .interaction_spring(
                        format!("proj-card-{name}"),
                        true,
                        MotionPaint::new().bg(rgb(if is_selected {
                            palette().accent_soft
                        } else {
                            palette().card
                        })),
                        MotionPaint::new().bg(rgb(if is_selected {
                            palette().accent_soft
                        } else {
                            palette().card_hover
                        })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2_5()
                            .flex_1()
                            .min_w_0()
                            .child(icon(
                                if is_selected {
                                    IconName::FolderOpen
                                } else {
                                    IconName::Folder
                                },
                                16.0,
                                if is_selected {
                                    palette().accent_fg
                                } else {
                                    palette().fg_muted
                                },
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(palette().fg))
                                            .truncate()
                                            .child(name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(palette().fg_muted))
                                            .font_family("JetBrains Mono")
                                            .truncate()
                                            .child(path),
                                    ),
                            ),
                    )
                    .on_click(move |_, _, cx| {
                        let proj = proj_clone.clone();
                        let _ = card_view.update(cx, |this, cx| {
                            this.select_project(proj, cx);
                        });
                    });

                rows = rows.child(card);
            }
            list_col = list_col.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .justify_start()
                    .overflow_y_scrollbar()
                    .child(rows),
            );
        }

        list_col
    }
}

impl Render for ProjectsPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_search(window, cx);
        let view = cx.entity().downgrade();

        let mut toolbar =
            page_toolbar(crate::i18n::t("sidebar.projects")).drag_id("projects-toolbar-drag");

        if let Some(search) = &self.search {
            toolbar = toolbar.search(toolbar_search(search, 224.0));
        }

        let query = self.search_query.trim().to_lowercase();
        let filtered_count = self
            .projects
            .iter()
            .filter(|p| {
                if query.is_empty() {
                    return true;
                }
                p.name.to_lowercase().contains(&query) || p.path.to_lowercase().contains(&query)
            })
            .count();

        toolbar = toolbar.filter(bar_count(IconName::Layers, filtered_count.to_string()));

        let reg_view = view.clone();
        toolbar = toolbar.action(
            bar_primary(
                "projects-register-btn",
                IconName::Plus,
                crate::i18n::t("projects.registerProject"),
            )
            .on_click(move |_, window, cx| {
                let view = reg_view.clone();
                let path_input = cx.new(|cx| {
                    InputState::new(window, cx).placeholder("/path/to/project/workspace")
                });
                let submitted = path_input.clone();
                crate::chrome::open_form_dialog(
                    window,
                    cx,
                    "Register Local Project",
                    crate::i18n::t("projects.registerProject"),
                    false,
                    160.0,
                    move |_, _| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(Input::new(&path_input))
                            .into_any_element()
                    },
                    move |_, cx| {
                        let path = submitted.read(cx).value().trim().to_string();
                        if path.is_empty() {
                            return false;
                        }
                        let _ = view.update(cx, |this, cx| match register_project(&path) {
                            Ok(entry) => {
                                this.refresh(cx);
                                this.select_project(entry, cx);
                            }
                            Err(err) => {
                                tracing::warn!("register_project failed: {err}")
                            }
                        });
                        true
                    },
                );
            }),
        );
        let toolbar = toolbar.build();

        let mut layout = div()
            .flex()
            .flex_row()
            .items_stretch()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .child(self.render_project_list(view.clone()));

        if let Some(selected) = &self.selected_project {
            layout = layout.child(render_project_detail(selected, self, view.clone()));
        } else {
            layout = layout.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_3()
                            .px_6()
                            .child(
                                div()
                                    .size(px(56.0))
                                    .rounded_2xl()
                                    .bg(rgb(palette().accent_soft))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(
                                        NavPage::Projects.icon(),
                                        28.0,
                                        palette().accent_fg,
                                    )),
                            )
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(palette().fg))
                                    .child(crate::i18n::t("projects.selectProjectTitle")),
                            )
                            .child(
                                div()
                                    .max_w(px(280.0))
                                    .text_sm()
                                    .text_align(TextAlign::Center)
                                    .text_color(rgb(palette().fg_muted))
                                    .child(crate::i18n::t("projects.selectProjectDesc")),
                            ),
                    ),
            );
        }

        page_chrome(toolbar, layout)
    }
}

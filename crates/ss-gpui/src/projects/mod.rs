//! Projects — registered projects + per-agent skill configuration.
//! React source: `src/pages/Projects.tsx` + `features/projects/`.

mod agent_item;
mod detail;

pub(crate) use agent_item::render_agent_item;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle};
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
                let count = count.to_string();
                self.status_message = Some(
                    crate::i18n::tf("projects.deploySuccess", &[("count", &count)]).to_string(),
                );
                self.refresh(cx);
            }
            Err(err) => {
                let err = err.to_string();
                self.status_message =
                    Some(crate::i18n::tf("projects.deployFailed", &[("err", &err)]).to_string());
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
                Empty::new().header(
                    EmptyHeader::new()
                        .media(EmptyMedia::new().child(icon(
                            IconName::Folder,
                            28.0,
                            palette().fg_muted,
                        )))
                        .title(EmptyTitle::new().child(crate::i18n::t("projects.emptyTitle")))
                        .description(
                            EmptyDescription::new()
                                .text_xs()
                                .child(crate::i18n::t("projects.emptyDesc")),
                        ),
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
                    .debug_selector(|| format!("proj-card-{name}"))
                    .w_full()
                    .flex_grow_0()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
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
                            .gap_2()
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
            .debug_selector(|| "projects-register-btn".into())
            .on_click(move |_, window, cx| {
                let view = reg_view.clone();
                let path_input = cx.new(|cx| {
                    InputState::new(window, cx).placeholder("/path/to/project/workspace")
                });
                let submitted = path_input.clone();
                // Shared between the dialog body, the browse picker and the
                // commit callback, which run on different paints.
                let error = Rc::new(RefCell::new(None::<SharedString>));
                let body_error = error.clone();
                let browse_error = error.clone();
                let browse_input = path_input.clone();
                crate::chrome::open_form_dialog(
                    window,
                    cx,
                    crate::i18n::t("projects.registerDialogTitle"),
                    crate::i18n::t("projects.registerProject"),
                    false,
                    move |_, _| {
                        // The wrapper only carries the test selector; it adds
                        // no styling, so its bounds stay the button's bounds.
                        let browse = div().debug_selector(|| "projects-browse".into()).child({
                            let path_input = browse_input.clone();
                            let error = browse_error.clone();
                            Button::new("projects-browse")
                                .label(crate::i18n::t("projects.browseFolder"))
                                .on_click(move |_, window, cx| {
                                    let receiver = cx.prompt_for_paths(PathPromptOptions {
                                        files: false,
                                        directories: true,
                                        multiple: false,
                                        prompt: Some(crate::i18n::t("projects.chooseDir")),
                                    });
                                    let path_input = path_input.clone();
                                    let error = error.clone();
                                    window
                                        .spawn(cx, async move |cx| {
                                            let picked = receiver
                                                .await
                                                .ok()
                                                .and_then(|r| r.ok())
                                                .flatten()
                                                .and_then(|p| p.into_iter().next());
                                            if let Some(path) = picked {
                                                let _ = cx.update(|window, cx| {
                                                    let text = path.to_string_lossy().to_string();
                                                    path_input.update(cx, |state, cx| {
                                                        state.set_value(text, window, cx)
                                                    });
                                                    *error.borrow_mut() = None;
                                                    window.refresh();
                                                });
                                            }
                                        })
                                        .detach();
                                })
                        });
                        let mut body = div().flex().flex_col().gap_2().w_full().child(
                            div()
                                .flex()
                                .flex_row()
                                .gap_2()
                                .w_full()
                                .child(div().flex_1().min_w_0().child(Input::new(&path_input)))
                                .child(browse),
                        );
                        if let Some(message) = body_error.borrow().clone() {
                            body = body.child(
                                div()
                                    .debug_selector(|| "projects-register-error".into())
                                    .w_full()
                                    .text_sm()
                                    .text_color(rgb(palette().danger))
                                    .child(message),
                            );
                        }
                        body.into_any_element()
                    },
                    move |window, cx| {
                        let path = submitted.read(cx).value().trim().to_string();
                        if path.is_empty() {
                            *error.borrow_mut() =
                                Some(crate::i18n::t("projects.registerEmptyPath"));
                            window.refresh();
                            return false;
                        }
                        match register_project(&path) {
                            Ok(entry) => {
                                let _ = view.update(cx, |this, cx| {
                                    this.refresh(cx);
                                    this.select_project(entry, cx);
                                });
                                true
                            }
                            Err(err) => {
                                tracing::warn!("register_project failed: {err}");
                                *error.borrow_mut() =
                                    Some(crate::i18n::t("projects.registerFailed"));
                                window.refresh();
                                false
                            }
                        }
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

#[cfg(test)]
mod tests {
    use gpui_kit::component::Root;
    use gpui_kit::{
        AppContext, Context, IntoElement, Modifiers, ParentElement, Render, Styled, Window, div,
        point, px, size,
    };

    use super::ProjectsPage;
    use crate::test_support::IsolatedDataDir;

    struct PageHost {
        page: gpui_kit::Entity<ProjectsPage>,
    }

    impl Render for PageHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.page.clone())
        }
    }

    fn paint(cx: &mut gpui_kit::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn click_center(cx: &mut gpui_kit::VisualTestContext, selector: &'static str) {
        paint(cx);
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is not on screen"));
        cx.simulate_click(
            point(
                bounds.origin.x + bounds.size.width / 2.0,
                bounds.origin.y + bounds.size.height / 2.0,
            ),
            Modifiers::default(),
        );
        paint(cx);
    }

    /// Commits the open dialog the way Enter does: the kit's Confirm
    /// action on the focused node. The AlertDialog footer has no
    /// test selector of its own.
    fn commit_dialog(cx: &mut gpui_kit::VisualTestContext) {
        cx.dispatch_action(gpui_kit::component::dialog::Confirm { secondary: false });
        paint(cx);
    }

    fn open_register_dialog(cx: &mut gpui_kit::TestAppContext) -> &mut gpui_kit::VisualTestContext {
        crate::init_test(cx);
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let page = cx.new(ProjectsPage::new);
            let host = cx.new(|_| PageHost { page });
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        click_center(cx, "projects-register-btn");
        cx
    }

    /// Regression: the project card must stay content-sized. A half-step
    /// spacing helper (`py_2_5` etc.) balloons to ~100px per side in this
    /// gpui, which stretched the card to fill the rail; see docs/errors.md.
    #[gpui_kit::test]
    fn project_cards_stay_compact(cx: &mut gpui_kit::TestAppContext) {
        let _dir = IsolatedDataDir::new();
        crate::init_test(cx);
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let page = cx.new(ProjectsPage::new);
            page.update(cx, |this, cx| {
                this.projects.push(super::ProjectEntry {
                    name: "Compact".into(),
                    path: "/tmp/compact".into(),
                    created_at: "2026-01-01T00:00:00Z".into(),
                });
                let proj = this.projects[0].clone();
                this.set_active_project(proj);
                cx.notify();
            });
            let host = cx.new(|_| PageHost { page });
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        paint(cx);
        let bounds = cx
            .debug_bounds("proj-card-Compact")
            .expect("the project card is not on screen");
        assert!(
            bounds.size.height < px(80.0),
            "the project card grew to {:?}; a half-step spacing helper is probably back",
            bounds.size
        );
    }

    /// The detail scroll area must start below the header, not overlap it.
    #[gpui_kit::test]
    fn detail_body_starts_below_header(cx: &mut gpui_kit::TestAppContext) {
        let _dir = IsolatedDataDir::new();
        crate::init_test(cx);
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let page = cx.new(ProjectsPage::new);
            page.update(cx, |this, cx| {
                this.projects.push(super::ProjectEntry {
                    name: "Overlap".into(),
                    path: "/tmp/overlap".into(),
                    created_at: "2026-01-01T00:00:00Z".into(),
                });
                let proj = this.projects[0].clone();
                this.set_active_project(proj);
                cx.notify();
            });
            let host = cx.new(|_| PageHost { page });
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        paint(cx);
        let card = cx
            .debug_bounds("agents-card")
            .expect("the agents card is not on screen");
        let rail = cx.debug_bounds("proj-card-Overlap").expect("rail card");
        assert!(
            card.origin.y >= rail.origin.y,
            "the detail card top {:?} sits above the header bottom (rail top {:?})",
            card.origin,
            rail.origin
        );
    }

    #[gpui_kit::test]
    fn browse_fills_the_path_and_commit_registers_the_project(cx: &mut gpui_kit::TestAppContext) {
        let _dir = IsolatedDataDir::new();
        let picked = std::env::temp_dir().join(format!(
            "skillstar-register-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&picked).unwrap();

        let cx = open_register_dialog(cx);
        click_center(cx, "projects-browse");
        assert!(
            cx.did_prompt_for_paths(),
            "the browse button did not open the system picker"
        );
        let chosen = picked.clone();
        cx.simulate_path_prompt_response(move |_| Some(vec![chosen]));
        cx.run_until_parked();
        paint(cx);

        commit_dialog(cx);
        let registered: Vec<String> = super::list_projects().into_iter().map(|p| p.path).collect();
        assert!(
            registered.contains(&picked.to_string_lossy().to_string()),
            "the picked folder was not registered: {registered:?}"
        );
        let _ = std::fs::remove_dir_all(&picked);
    }

    #[gpui_kit::test]
    fn an_empty_commit_keeps_the_dialog_open_with_the_reason(cx: &mut gpui_kit::TestAppContext) {
        let _dir = IsolatedDataDir::new();
        let cx = open_register_dialog(cx);

        commit_dialog(cx);
        assert!(
            cx.debug_bounds("projects-register-error").is_some(),
            "the empty-path reason is not shown"
        );
        assert!(
            cx.debug_bounds("dialog-0").is_some(),
            "the dialog closed on a failed commit"
        );
        assert!(super::list_projects().is_empty());
    }

    /// Regression: the deploy-mode capsule sits inside the collapsible row,
    /// so its click must not also toggle the row's expand state.
    #[gpui_kit::test]
    fn clicking_the_deploy_mode_capsule_keeps_the_row_expanded(cx: &mut gpui_kit::TestAppContext) {
        let _dir = IsolatedDataDir::new();
        crate::init_test(cx);
        let page_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot = page_slot.clone();
        let (_root, cx) = cx.add_window_view(|window, cx| {
            let page = cx.new(ProjectsPage::new);
            page.update(cx, |this, cx| {
                this.projects.push(super::ProjectEntry {
                    name: "Mode".into(),
                    path: "/tmp/mode".into(),
                    created_at: "2026-01-01T00:00:00Z".into(),
                });
                let proj = this.projects[0].clone();
                this.set_active_project(proj);
                this.profiles = vec![ss_skills::agents::AgentProfile {
                    id: "cursor".into(),
                    display_name: "Cursor".into(),
                    icon: String::new(),
                    global_skills_dir: std::path::PathBuf::new(),
                    project_skills_rel: ".cursor/skills".into(),
                    installed: true,
                    enabled: true,
                    synced_count: 0,
                }];
                this.agent_skills
                    .insert("cursor".into(), vec!["demo".into()]);
                this.expanded_agent = Some("cursor".into());
                cx.notify();
            });
            *slot.borrow_mut() = Some(page.clone());
            let host = cx.new(|_| PageHost { page });
            Root::new(host, window, cx)
        });
        cx.simulate_resize(size(px(1200.), px(800.)));
        paint(cx);
        assert!(
            cx.debug_bounds("agent-top-cursor").is_some(),
            "the agent row is not on screen"
        );

        click_center(cx, "mode-toggle-cursor");

        let page = page_slot.borrow().clone().unwrap();
        let (mode, expanded) = page.read_with(cx, |this, _| {
            (
                this.deploy_modes.get(".cursor/skills").copied(),
                this.expanded_agent.clone(),
            )
        });
        assert_eq!(
            mode,
            Some(super::ProjectDeployMode::Copy),
            "the capsule did not flip the deploy mode"
        );
        assert_eq!(
            expanded.as_deref(),
            Some("cursor"),
            "the row collapsed when the deploy mode was toggled"
        );
    }
}

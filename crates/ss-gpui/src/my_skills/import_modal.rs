//! Import dialog — the multi-phase counterpart of React `ImportModal.tsx`:
//! URL input → repo scan → skill picker → install → done, plus the
//! `ags-`/`agd-` share-code preview path. The toolbar's old single-line
//! prompt collapsed all of that into one field; this entity keeps each
//! phase's own chrome, like `ModalShell` + the `import-modal/*` phases.
//!
//! Renderers live in `phases`/`select`/`pack`/`share` — the per-1000-line
//! file budget is the only reason; `ImportDialog` owns all state here.

mod helpers;
mod pack;
mod phases;
mod progress;
mod select;
mod share;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::anyhow;
use gpui_kit::component::WindowExt;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use ss_skills::git::repo_history::{self, RepoHistoryEntry};
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::repo_scanner::{ScanResult, SkillInstallTarget};
use ss_skills::share_install::{self, ParsedShareCode, ShareCodeInstallSummary, ShareCodeKind};
use ss_skills::skill_bundle::AnyBundleImport;

use super::MySkillsPage;
use crate::spawn_domain;
use helpers::{deck_name_from_source, git_error_message, looks_like_share_code};

/// React `Phase` union: `inputURL | scanning | selectSkills | installing |
/// completed | error | shareCodePreview | shareCodeInstalling`, plus `Pack`
/// — React closes this modal and opens `CreateGroupModal` on quick-pack;
/// keeping it as a phase avoids juggling a second dialog for one field set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Phase {
    Input,
    Scanning,
    Select,
    Installing,
    Completed,
    Failed,
    SharePreview,
    ShareInstalling,
    Pack,
}

/// Dialog content view. Opened as a child of `window.open_dialog`, so the
/// surrounding overlay/close chrome comes from gpui-component while this
/// entity owns phase state and re-renders on `cx.notify()`.
pub(crate) struct ImportDialog {
    page: WeakEntity<MySkillsPage>,
    url: Entity<InputState>,
    /// Dialog open focuses the shell, not this field. The first render
    /// moves the caret here, once, after the shell is in the tree.
    focus_url: bool,
    filter: Entity<InputState>,
    filter_query: String,
    phase: Phase,
    history: Vec<RepoHistoryEntry>,
    scan: Option<ScanResult>,
    scan_input: String,
    selected: HashSet<String>,
    full_depth: bool,
    progress: Option<SharedString>,
    error: Option<String>,
    installed: usize,
    share: Option<ParsedShareCode>,
    share_existing: Vec<String>,
    summary: Option<ShareCodeInstallSummary>,
    /// Git session backing the in-flight scan/install; `cancel()` aborts it.
    facade: Option<GitSkillFacade>,
    // ── Pack step (React `CreateGroupModal`, opened by Quick Pack) ───
    pack_name: Entity<InputState>,
    pack_desc: Entity<InputState>,
    pack_filter: Entity<InputState>,
    pack_query: String,
    pack_icon: String,
    pack_emoji_open: bool,
    pack_members: HashSet<String>,
    /// (name, description) for every installed skill — member picker rows.
    pack_all: Vec<(String, String)>,
    /// Existing group names, for the duplicate-name guard.
    pack_names: Vec<String>,
    /// Default name handed to the name field on the first Pack render —
    /// `set_value` needs a `Window`, which only render supplies.
    pack_seed: Option<String>,
    _subs: Vec<Subscription>,
}

impl ImportDialog {
    pub(crate) fn new(
        page: WeakEntity<MySkillsPage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let url = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("githubImportModal.placeholder"))
        });
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::t("common.search")));
        let pack_name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("createGroupModal.groupName"))
        });
        let pack_desc = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("createGroupModal.description"))
        });
        let pack_filter = cx.new(|cx| {
            InputState::new(window, cx).placeholder(crate::i18n::t("createGroupModal.searchSkills"))
        });
        let subs = vec![
            cx.subscribe_in(&url, window, Self::on_url_event),
            cx.subscribe_in(&filter, window, Self::on_filter_event),
            // Typing the deck name must re-render: the duplicate-name guard
            // and Create button state are computed in `render_pack`.
            cx.subscribe_in(&pack_name, window, Self::on_pack_name_event),
            cx.subscribe_in(&pack_filter, window, Self::on_pack_filter_event),
        ];
        let this = Self {
            page,
            url,
            focus_url: true,
            filter,
            filter_query: String::new(),
            phase: Phase::Input,
            history: Vec::new(),
            scan: None,
            scan_input: String::new(),
            selected: HashSet::new(),
            full_depth: false,
            progress: None,
            error: None,
            installed: 0,
            share: None,
            share_existing: Vec::new(),
            summary: None,
            facade: None,
            pack_name,
            pack_desc,
            pack_filter,
            pack_query: String::new(),
            pack_icon: "💻".to_string(),
            pack_emoji_open: false,
            pack_members: HashSet::new(),
            pack_all: Vec::new(),
            pack_names: Vec::new(),
            pack_seed: None,
            _subs: subs,
        };
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async { tokio::task::spawn_blocking(repo_history::list_entries).await },
            |this, _cx, history| this.history = history.unwrap_or_default(),
        );
        this
    }

    /// Cancel the running git op; `Dialog::on_close` calls this so X/Escape/
    /// backdrop all abort in-flight work before the dialog tears down.
    pub(crate) fn cancel_active(&mut self) {
        if let Some(facade) = &self.facade {
            facade.cancel();
        }
    }

    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_active();
        window.close_dialog(cx);
    }

    fn on_url_event(
        &mut self,
        _state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let text = self.url.read(cx).value().to_string();
                if looks_like_share_code(&text) {
                    self.parse_share(text, cx);
                }
            }
            InputEvent::PressEnter { .. } => {
                let text = self.url.read(cx).value().trim().to_string();
                self.scan(text, self.full_depth, cx);
            }
            _ => {}
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
            self.filter_query = self.filter.read(cx).value().to_string();
            cx.notify();
        }
    }

    /// Name keystrokes only re-render — `render_pack` reads the live value for
    /// the duplicate-name guard and Create enablement.
    fn on_pack_name_event(
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

    fn on_pack_filter_event(
        &mut self,
        _state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            self.pack_query = self.pack_filter.read(cx).value().to_string();
            cx.notify();
        }
    }

    // ── Domain actions ─────────────────────────────────────────────

    /// `handleScan`: shallow or full-depth repo scan. Keeps the facade around
    /// so the Cancel button can abort mid-fetch.
    fn scan(&mut self, input: String, full_depth: bool, cx: &mut Context<Self>) {
        self.scan_with_refresh(input, full_depth, false, cx);
    }

    fn scan_with_refresh(
        &mut self,
        input: String,
        full_depth: bool,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if input.is_empty() {
            return;
        }
        self.full_depth = full_depth;
        self.scan_input = input.clone();
        let facade = progress::tracked_facade(cx);
        self.facade = Some(facade.clone());
        self.phase = Phase::Scanning;
        self.progress = Some(crate::i18n::t("githubImportModal.cloning"));
        self.error = None;
        cx.notify();
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    let scan = facade.scan_repo_with_refresh(&input, full_depth, refresh);
                    if let Ok(scan) = &scan {
                        // Same as `scan_github_repo`: history writes never
                        // fail the scan itself.
                        let _ = repo_history::upsert_entry(&scan.spec.short, &scan.spec.repo_url);
                    }
                    scan.map_err(|err| anyhow!("{err:#}"))
                })
                .await
                .unwrap_or_else(|err| Err(anyhow!("{err}")))
            },
            |this, _cx, result: anyhow::Result<ScanResult>| {
                this.facade = None;
                match result {
                    Ok(scan) => {
                        this.selected = scan
                            .skills
                            .iter()
                            .filter(|s| s.installable && !s.already_installed)
                            .map(|s| s.id.clone())
                            .collect();
                        this.scan = Some(scan);
                        this.phase = Phase::Select;
                    }
                    Err(err) => {
                        this.error = Some(git_error_message(&format!("{err:#}")));
                        this.phase = Phase::Failed;
                    }
                }
            },
        );
    }

    /// `handleDeepScan`: re-scan the same repo at full depth.
    fn deep_scan(&mut self, cx: &mut Context<Self>) {
        if self.scan.is_none() {
            return;
        }
        self.full_depth = true;
        let url = self.scan_input.clone();
        self.scan(url, true, cx);
    }

    /// `handleInstall`: install the checked skills; `pack` mirrors Quick Pack
    /// (install, then name a deck for the result).
    fn install_selected(&mut self, pack: bool, cx: &mut Context<Self>) {
        let Some(scan) = self.scan.clone() else {
            return;
        };
        if self.selected.is_empty() {
            return;
        }
        let targets: Vec<SkillInstallTarget> = scan
            .skills
            .iter()
            .filter(|s| s.installable && self.selected.contains(&s.id))
            .map(|s| SkillInstallTarget {
                id: s.id.clone(),
                folder_path: s.folder_path.clone(),
                pinned: false,
            })
            .collect();
        let facade = progress::tracked_facade(cx);
        self.facade = Some(facade.clone());
        self.summary = None;
        self.phase = Phase::Installing;
        self.progress = Some(crate::i18n::tf(
            "githubImportModal.installing",
            &[("count", &targets.len().to_string())],
        ));
        cx.notify();

        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    facade
                        .install_from_scan(&scan, &targets)
                        .map(|installed| (installed, scan.spec.clone()))
                        .map_err(|err| anyhow!("{err:#}"))
                })
                .await
                .unwrap_or_else(|err| Err(anyhow!("{err}")))
            },
            move |this,
                  cx,
                  result: anyhow::Result<(Vec<String>, ss_skills::source_resolver::Source)>| {
                this.facade = None;
                match result {
                    Ok((installed, spec)) => {
                        this.installed = installed.len();
                        this.error = None;
                        this.refresh_page(cx);
                        if pack {
                            this.open_pack(&installed, &spec, cx);
                        } else {
                            this.phase = Phase::Completed;
                        }
                    }
                    Err(err) => {
                        this.error = Some(git_error_message(&format!("{err:#}")));
                        this.phase = Phase::Failed;
                    }
                }
            },
        );
    }

    /// `handleParseShareCode`: decode the `ags-`/`agd-` payload, then mark the
    /// skills that are already installed. Parse failures land on the preview
    /// phase's error block — the same surface React uses for `data === null`
    /// (its password retry branch stays unreachable: the Rust decoder has no
    /// password parameter).
    fn parse_share(&mut self, text: String, cx: &mut Context<Self>) {
        self.summary = None;
        match share_install::parse_share_code(&text) {
            Ok(parsed) => {
                let names: Vec<String> = parsed.payload.s.iter().map(|s| s.n.clone()).collect();
                self.share = Some(parsed);
                self.share_existing = Vec::new();
                self.error = None;
                self.phase = Phase::SharePreview;
                let view = cx.entity();
                spawn_domain(
                    &view,
                    cx,
                    async move {
                        let installed: HashSet<String> =
                            ss_skills::installed_skill::list_installed_skills()
                                .await
                                .unwrap_or_default()
                                .iter()
                                .map(|s| s.name.trim().to_lowercase())
                                .collect();
                        let mut seen = HashSet::new();
                        names
                            .into_iter()
                            .map(|n| n.trim().to_string())
                            .filter(|n| !n.is_empty() && seen.insert(n.to_lowercase()))
                            .filter(|n| installed.contains(&n.to_lowercase()))
                            .collect::<Vec<String>>()
                    },
                    |this, _cx, existing| this.share_existing = existing,
                );
            }
            Err(err) => {
                self.share = None;
                self.share_existing = Vec::new();
                self.error = Some(err);
                self.phase = Phase::SharePreview;
            }
        }
        cx.notify();
    }

    /// `handleShareCodeInstall`: install the payload, summarize, and create a
    /// local deck for `agd-` codes (React's auto-create for convenience).
    fn install_share(&mut self, cx: &mut Context<Self>) {
        let Some(share) = self.share.clone() else {
            return;
        };
        self.phase = Phase::ShareInstalling;
        self.progress = Some(crate::i18n::t("shareCodeImport.installing"));
        cx.notify();

        let deck = share.kind == ShareCodeKind::Deck;
        let (name, desc, icon_text) = (
            share.payload.n.clone(),
            share.payload.d.clone(),
            share.payload.i.clone(),
        );
        let skills = share.payload.s;
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    let summary = share_install::install_from_share_code(skills.clone());
                    let mut deck_created = false;
                    if deck && !name.trim().is_empty() {
                        let sources: HashMap<String, String> = skills
                            .iter()
                            .filter(|s| s.remote().is_ok())
                            .map(|s| (s.n.clone(), s.u.clone()))
                            .collect();
                        let names: Vec<String> = skills
                            .iter()
                            .map(|s| s.n.clone())
                            .filter(|n| !n.is_empty())
                            .collect();
                        let icon = if icon_text.trim().is_empty() {
                            "📦".to_string()
                        } else {
                            icon_text
                        };
                        deck_created =
                            ss_skills::skill_group::create_group(name, desc, icon, names, sources)
                                .is_ok();
                    }
                    Ok::<_, anyhow::Error>((summary, deck_created))
                })
                .await
                .unwrap_or_else(|err| Err(anyhow!("{err}")))
            },
            |this, cx, result: anyhow::Result<(ShareCodeInstallSummary, bool)>| match result {
                Ok((summary, deck_created)) => {
                    this.error = None;
                    this.installed = summary.installed_names.len() + summary.embedded_names.len();
                    this.share_existing = summary.existing_names.clone();
                    this.summary = Some(summary);
                    this.phase = Phase::Completed;
                    this.refresh_page(cx);
                    if deck_created {
                        this.notify_groups_changed(cx);
                    }
                }
                Err(err) => {
                    this.error = Some(format!("{err:#}"));
                    this.phase = Phase::Failed;
                }
            },
        );
    }

    /// `handlePickLocalFolder`: native directory picker → `adopt_folder`.
    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(crate::i18n::t("importModal.adoptFolderTitle")),
        });
        let view = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let path = receiver
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
                .and_then(|p| p.into_iter().next());
            if let Some(path) = path {
                let _ = view.update(cx, |this, cx| this.adopt_folder(path, cx));
            }
        })
        .detach();
    }

    fn adopt_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.phase = Phase::Installing;
        self.progress = Some(crate::i18n::t("importModal.adoptingFolder"));
        cx.notify();
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    ss_skills::local_skill::adopt_folder(&path.to_string_lossy(), None)
                        .map(|r| {
                            r.adopted
                                .into_iter()
                                .map(|s| s.name)
                                .collect::<Vec<String>>()
                        })
                        .map_err(|err| anyhow!("{err:#}"))
                })
                .await
                .unwrap_or_else(|err| Err(anyhow!("{err}")))
            },
            |this, cx, result: anyhow::Result<Vec<String>>| match result {
                Ok(names) => {
                    this.installed = names.len();
                    this.phase = Phase::Completed;
                    this.refresh_page(cx);
                }
                Err(err) => {
                    this.error = Some(format!("{err:#}"));
                    this.phase = Phase::Failed;
                }
            },
        );
    }

    /// `onPickLocalFile` counterpart — React hops to `ImportBundleModal` /
    /// `ImportDeckBundleModal`; the GPUI shell runs the bundle import inline,
    /// same as `adopt_folder`.
    fn pick_bundle(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(crate::i18n::t("importBundleModal.pickFile")),
        });
        let view = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let path = receiver
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
                .and_then(|p| p.into_iter().next());
            if let Some(path) = path {
                let _ = view.update(cx, |this, cx| this.import_bundle(path, cx));
            }
        })
        .detach();
    }

    /// The picker accepts both `.ags` and `.agd`, so the domain dispatches on
    /// the archive itself — a deck bundle must not land in the single-skill
    /// importer.
    fn import_bundle(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.phase = Phase::Installing;
        self.progress = Some(crate::i18n::t("importBundleModal.importing"));
        cx.notify();
        let deck_path = path.clone();
        let view = cx.entity();
        spawn_domain(
            &view,
            cx,
            async move {
                tokio::task::spawn_blocking(move || {
                    ss_skills::skill_bundle::import_any_bundle(&path.to_string_lossy(), false)
                        .map_err(|err| anyhow!("{err:#}"))
                })
                .await
                .unwrap_or_else(|err| Err(anyhow!("{err}")))
            },
            move |this, cx, result: anyhow::Result<AnyBundleImport>| {
                if result.is_ok() {
                    this.error = None;
                }
                match result {
                    Ok(AnyBundleImport::Single(_)) => {
                        this.installed = 1;
                        this.phase = Phase::Completed;
                        this.refresh_page(cx);
                    }
                    Ok(AnyBundleImport::Multi(result)) => {
                        this.installed = result.skill_names.len();
                        // `onDeckImported`: a deck bundle also creates its group,
                        // named after the file. A failed create (duplicate name)
                        // keeps the success phase — the skills are installed.
                        let deck = ss_skills::skill_bundle::deck_name_from_bundle_path(
                            &deck_path.to_string_lossy(),
                        );
                        let deck = if deck.is_empty() {
                            crate::i18n::t("importDeckBundleModal.deckBundle").to_string()
                        } else {
                            deck
                        };
                        let desc = crate::i18n::tf(
                            "importDeckBundleModal.skillsCount",
                            &[("count", &result.skill_names.len().to_string())],
                        )
                        .to_string();
                        let deck_created = ss_skills::skill_group::create_group(
                            deck,
                            desc,
                            "📦".to_string(),
                            result.skill_names.clone(),
                            HashMap::new(),
                        )
                        .is_ok();
                        this.phase = Phase::Completed;
                        this.refresh_page(cx);
                        if deck_created {
                            this.notify_groups_changed(cx);
                        }
                    }
                    Err(err) => {
                        this.error = Some(format!("{err:#}"));
                        this.phase = Phase::Failed;
                    }
                }
            },
        );
    }

    /// `reset` — back to the URL field with everything cleared.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.phase = Phase::Input;
        self.scan = None;
        self.selected.clear();
        self.full_depth = false;
        self.progress = None;
        self.error = None;
        self.installed = 0;
        self.share = None;
        self.share_existing.clear();
        self.summary = None;
        // `set_value` suppresses the Change event, so no share re-parse loop.
        let _ = self
            .url
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn refresh_page(&self, cx: &mut Context<Self>) {
        let _ = self.page.update(cx, |page, cx| {
            page.loading = true;
            page.refresh(cx);
        });
    }

    /// Deck write → tell `SkillCardsPage` to reload (`GroupsChanged` rides
    /// the page entity because `Shell` holds the card page, not us).
    fn notify_groups_changed(&self, cx: &mut Context<Self>) {
        let _ = self
            .page
            .update(cx, |_, cx| cx.emit(crate::nav::GroupsChanged));
    }
}

/// Any close path that skips `Dialog::on_close` (dialog dropped with its
/// entity) still cancels the in-flight git session.
impl Drop for ImportDialog {
    fn drop(&mut self) {
        self.cancel_active();
    }
}

impl Render for ImportDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // `open_dialog` focuses the dialog shell before this view is painted.
        // Focusing from `new` loses: the handle is not in the tree yet, and
        // the shell focus replaces it. Do it once on the first real render.
        if self.focus_url {
            self.focus_url = false;
            let handle = self.url.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
        let body = match self.phase {
            Phase::Input => self.render_input(cx).into_any_element(),
            Phase::Scanning | Phase::Installing | Phase::ShareInstalling => {
                self.render_loading(cx).into_any_element()
            }
            Phase::Select => self.render_select(window, cx).into_any_element(),
            Phase::Completed => self.render_completed(cx).into_any_element(),
            Phase::Failed => self.render_failed(cx).into_any_element(),
            Phase::SharePreview => self.render_share_preview(cx).into_any_element(),
            Phase::Pack => self.render_pack(window, cx).into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(self.render_header())
            .child(body)
    }
}

// ── Free helpers ───────────────────────────────────────────────────
// Pure helpers live in `helpers`; renderers live in the phase modules.

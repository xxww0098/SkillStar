//! Settings → translation switches, engine, and translation style.
//!
//! Machine translation needs no key. LLM translation uses an OpenCode Go,
//! Ollama, or Command Code key already saved in Accounts. The user picks
//! one account as the default. The model starts as that service's
//! recommendation (OpenCode Go: `deepseek-v4.1-flash`). Fetch only fills the
//! model combobox, which stays closed until opened; picking one keeps it
//! until the account changes. The combobox is searchable and caps its menu
//! height, scrolling longer catalogs; the footer states the catalog size.
//! Themes only change how the SKILL.md reader paints a cached translation;
//! descriptions always stay plain.

use std::rc::Rc;

use gpui_kit::App;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::Button;
use gpui_kit::component::combobox::Combobox;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::translation::{self, Engine};
use ss_usage::accounts::{SubscriptionDto, list_subscriptions};

use crate::chrome::{SliderGeometry, SliderSegment, slider_segmented};
use crate::spawn_domain;

use super::{SettingsPage, SettingsSection, card, choice_pills, field_label, section_shell};
use crate::theme::palette;

impl SettingsPage {
    pub(crate) fn render_translation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        view: WeakEntity<Self>,
    ) -> impl IntoElement {
        let engine = match self.translation.engine {
            Engine::Machine => "machine",
            Engine::Llm => "llm",
        };
        let engines = [
            ("machine", crate::i18n::t("settings.tranEngineMachine")),
            ("llm", crate::i18n::t("settings.tranEngineLlm")),
        ];
        let targets: Vec<SliderSegment> = translation::TRANSLATION_LANGUAGES
            .iter()
            .map(|language| SliderSegment {
                id: language.code,
                icon: None,
                label: Some(SharedString::from(language.label)),
            })
            .collect();
        // `starts_with` keeps the old `choice_pills` fallback: a stored
        // dialect code still lands on its base language's slot.
        let target_ix = translation::TRANSLATION_LANGUAGES
            .iter()
            .position(|language| {
                self.translation.target_lang == language.code
                    || self.translation.target_lang.starts_with(language.code)
            })
            .unwrap_or(0);
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(auto_row(
                "tran-descriptions",
                crate::i18n::t("settings.tranDescriptions"),
                crate::i18n::t("settings.tranDescriptionsHint"),
                self.translation.translate_descriptions,
                view.clone(),
                |this, cx| {
                    this.translation.translate_descriptions =
                        !this.translation.translate_descriptions;
                    this.save_translation(cx);
                },
            ))
            .child(auto_row(
                "tran-skill-md",
                crate::i18n::t("settings.tranSkillMd"),
                crate::i18n::t("settings.tranSkillMdHint"),
                self.translation.translate_skill_md,
                view.clone(),
                |this, cx| {
                    this.translation.translate_skill_md = !this.translation.translate_skill_md;
                    this.save_translation(cx);
                },
            ))
            .child(field_label(crate::i18n::t("settings.tranTarget"), {
                // 72px slots hold the widest endonym ("简体中文" at
                // text_xs) with centered padding; all five fit the
                // settings column without wrapping.
                let pick = view.clone();
                slider_segmented(
                    "tran-target-motion",
                    &targets,
                    target_ix,
                    SliderGeometry {
                        slot_w: 72.0,
                        slot_h: 32.0,
                        pad: 2.0,
                        icon_size: 14.0,
                    },
                    false,
                    Rc::new(move |ix, _, cx| {
                        let _ = pick.update(cx, |this, cx| {
                            this.translation.target_lang =
                                translation::TRANSLATION_LANGUAGES[ix].code.to_string();
                            this.save_translation(cx);
                        });
                    }),
                )
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .whitespace_normal()
                    .child(crate::i18n::t("settings.tranTargetHint")),
            )
            .child(field_label(
                crate::i18n::t("settings.tranEngine"),
                choice_pills(
                    "tran-engine",
                    &engines,
                    engine,
                    view.clone(),
                    |this, id, cx| {
                        this.translation.engine = if id == "llm" {
                            Engine::Llm
                        } else {
                            Engine::Machine
                        };
                        if this.translation.engine == Engine::Llm {
                            ensure_llm_default(&mut this.translation);
                        }
                        this.save_translation(cx);
                    },
                ),
            ));
        body = body.child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(crate::i18n::t(if self.translation.engine == Engine::Llm {
                    "settings.tranEngineLlmHint"
                } else {
                    "settings.tranEngineMachineHint"
                })),
        );
        if self.translation.engine == Engine::Llm {
            let accounts = translation_accounts();
            if accounts.is_empty() {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .whitespace_normal()
                        .child(crate::i18n::t("settings.tranLlmAccountEmpty")),
                );
            } else {
                body = body
                    .child(field_label(
                        crate::i18n::t("settings.tranLlmDefault"),
                        credential_pills(&accounts, &self.translation.llm_account_id, view.clone()),
                    ))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(palette().fg_muted))
                            .whitespace_normal()
                            .child(crate::i18n::t("settings.tranLlmAccountHint")),
                    );
                if translation::llm_account(&self.translation.llm_account_catalog).is_some() {
                    body = body.child(self.model_field(window, cx, view.clone()));
                }
            }
        }
        body = body.child(self.style_gallery(
            "tran-reader",
            crate::i18n::t("settings.tranReaderTheme"),
            view,
        ));
        section_shell(
            SettingsSection::Translation,
            None,
            None,
            card().px_4().py_4().child(body),
        )
    }

    fn style_gallery(
        &self,
        prefix: &'static str,
        title: SharedString,
        view: WeakEntity<Self>,
    ) -> Div {
        let themes: Vec<&'static translation::Theme> = translation::reader_themes().collect();
        let current = self.translation.reader_theme.as_str();
        let collapsible = themes.len() > 9;
        let open = !collapsible || self.reader_styles_open;
        let shown: Vec<&'static translation::Theme> = if open {
            themes.clone()
        } else {
            collapsed_themes(&themes, current)
        };
        let mut grid = div().grid().grid_cols(3).gap_2();
        for theme in &shown {
            grid = grid.child(style_card(prefix, theme, current == theme.id, view.clone()));
        }
        if collapsible {
            let remainder = shown.len() % 3;
            let span = if remainder == 0 {
                3
            } else {
                (3 - remainder) as u16
            };
            grid = grid.child(fold_button(span, open, view));
        }
        field_label(title, grid)
    }

    pub(crate) fn save_translation(&mut self, cx: &mut Context<Self>) {
        // This page never edits the per-skill description choices; take what
        // is on disk so a save cannot wipe a choice the drawer made while
        // this page held its older copy.
        if let Ok(disk) = translation::load_config() {
            self.translation.description_choices = disk.description_choices;
        }
        if let Err(error) = translation::save_config(&self.translation) {
            tracing::warn!("failed to save translation settings: {error}");
        }
        self.translation = translation::load_config().unwrap_or_default();
        crate::translation::settings_changed();
        cx.emit(crate::nav::TranslationPrefsChanged);
        cx.notify();
    }

    fn select_llm_account(&mut self, id: String, catalog: String, cx: &mut Context<Self>) {
        let switched = self.translation.llm_account_id != id;
        self.translation.llm_account_id = id;
        self.translation.llm_account_catalog = catalog;
        if switched {
            self.translation.llm_model_pinned = false;
            if let Some(provider) = translation::llm_account(&self.translation.llm_account_catalog)
            {
                self.translation.llm_model = provider.model_hint.to_string();
            }
            self.llm_models.clear();
            self.llm_models_for.clear();
            self.llm_models_error = None;
            self.llm_models_loading = false;
        }
        self.save_translation(cx);
    }

    pub(crate) fn select_llm_model(&mut self, model: String, cx: &mut Context<Self>) {
        let model = model.trim().to_string();
        if model.is_empty()
            || (self.translation.llm_model_pinned && self.translation.llm_model == model)
        {
            return;
        }
        self.translation.llm_model = model;
        self.translation.llm_model_pinned = true;
        self.save_translation(cx);
    }

    fn pull_llm_models(&mut self, cx: &mut Context<Self>) {
        if self.llm_models_loading {
            return;
        }
        let account_id = self.translation.llm_account_id.clone();
        let Some(provider) = translation::llm_account(&self.translation.llm_account_catalog) else {
            return;
        };
        let base = provider.base_url.to_string();
        let key = ss_usage::accounts::get_subscription_api_key(account_id.clone())
            .ok()
            .flatten()
            .unwrap_or_default();
        self.llm_models_loading = true;
        self.llm_models_error = None;
        let entity = cx.entity();
        spawn_domain(
            &entity,
            cx,
            async move { translation::list_models(&base, &key).await },
            move |this, cx, result| {
                if this.translation.llm_account_id != account_id {
                    return;
                }
                this.llm_models_loading = false;
                match result {
                    Ok(models) => {
                        this.llm_models = models;
                        this.llm_models_for = account_id;
                        this.llm_models_error = None;
                        this.llm_fetch_seq += 1;
                    }
                    Err(error) => this.llm_models_error = Some(model_list_message(&error)),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    pub(crate) fn model_field(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        view: WeakEntity<Self>,
    ) -> Div {
        self.sync_llm_model_state(window, cx);
        let loading = self.llm_models_loading;
        let pull_view = view.clone();
        // Fetch fills the combobox data only. The menu stays closed so a full
        // model list never pushes the settings card open on its own.
        let fetched =
            self.llm_models_for == self.translation.llm_account_id && !self.llm_models.is_empty();
        let total = self.llm_models.len().to_string();
        let combobox = Combobox::new(&self.llm_model_state)
            .placeholder("—")
            .search_placeholder(crate::i18n::t("settings.tranLlmModelSearch"))
            .w_full()
            .small()
            .font_family("monospace")
            .menu_max_h(px(280.0))
            .empty(|_, _| {
                div()
                    .p_2()
                    .text_xs()
                    .text_color(rgb(palette().fg_muted))
                    .whitespace_normal()
                    .child(crate::i18n::t("settings.tranLlmModelEmpty"))
            })
            .when(fetched, |this| {
                let total = total.clone();
                this.footer(move |_, _| {
                    div()
                        .debug_selector(|| "tran-llm-model-total".into())
                        .flex()
                        .justify_center()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .child(crate::i18n::tf(
                            "settings.tranLlmModelTotal",
                            &[("count", &total)],
                        ))
                })
            });
        let mut field = field_label(
            crate::i18n::t("settings.tranLlmModel"),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .debug_selector(|| "tran-llm-model-field".into())
                        .child(combobox),
                )
                .child(
                    Button::new("tran-llm-pull")
                        .outline()
                        .small()
                        .flex_shrink_0()
                        .icon(Icon::new(IconName::RefreshCw))
                        .label(crate::i18n::t(if loading {
                            "settings.tranLlmModelPulling"
                        } else {
                            "settings.tranLlmModelPull"
                        }))
                        .loading(loading)
                        .on_click(move |_, _, cx| {
                            let _ = pull_view.update(cx, |this, cx| this.pull_llm_models(cx));
                        }),
                ),
        );
        if let Some(error) = &self.llm_models_error {
            field = field.child(
                div()
                    .text_xs()
                    .text_color(rgb(palette().danger))
                    .whitespace_normal()
                    .child(error.clone()),
            );
        }
        // After a fetch the hint states the catalog size, so a scrolled
        // menu never reads as the full list.
        let hint = if fetched {
            let count = self.llm_models.len().to_string();
            crate::i18n::tf("settings.tranLlmModelCount", &[("count", &count)])
        } else {
            crate::i18n::t("settings.tranLlmModelHint")
        };
        field = field.child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(hint),
        );
        field
    }

    /// Push the fetched catalog and the current model into the combobox state.
    ///
    /// Fetch callbacks carry no `Window`, so this runs from `model_field`
    /// during render; `llm_models_synced` keeps it to one pass per change
    /// (account, fetch generation, or current model). The current model is
    /// kept as an explicit first row when the catalog does not list it, so
    /// the trigger always shows what is configured.
    fn sync_llm_model_state(&mut self, window: &mut Window, cx: &mut App) {
        let current = self.translation.llm_model.trim().to_string();
        let key = format!(
            "{}|{}|{}",
            self.translation.llm_account_id, self.llm_fetch_seq, current
        );
        if self.llm_models_synced.as_deref() == Some(key.as_str()) {
            return;
        }
        let fetched =
            self.llm_models_for == self.translation.llm_account_id && !self.llm_models.is_empty();
        let mut rows = if fetched {
            self.llm_models.clone()
        } else {
            Vec::new()
        };
        if !current.is_empty() && !rows.iter().any(|id| id == &current) {
            rows.insert(0, current.clone());
        }
        let selected: Vec<String> = if current.is_empty() {
            Vec::new()
        } else {
            vec![current]
        };
        self.llm_models_synced = Some(key);
        _ = self.llm_model_state.update(cx, move |state, cx| {
            state.set_items(SearchableVec::new(rows), window, cx);
            state.set_selected_values(&selected, window, cx);
        });
    }
}

fn model_list_message(error: &anyhow::Error) -> String {
    let text = if let Some(kind) = error.downcast_ref::<translation::ModelListError>() {
        match kind {
            translation::ModelListError::MissingKey => crate::i18n::t("settings.tranLlmModelNoKey"),
            translation::ModelListError::Empty => crate::i18n::t("settings.tranLlmModelNone"),
            translation::ModelListError::Status(code) => crate::i18n::tf(
                "settings.tranLlmModelStatus",
                &[("status", &code.to_string())],
            ),
            translation::ModelListError::Request => crate::i18n::t("settings.tranLlmModelFailed"),
        }
    } else {
        crate::i18n::t("settings.tranLlmModelFailed")
    };
    text.to_string()
}

fn auto_row(
    id: &'static str,
    title: SharedString,
    hint: SharedString,
    enabled: bool,
    view: WeakEntity<SettingsPage>,
    apply: impl Fn(&mut SettingsPage, &mut Context<SettingsPage>) + 'static,
) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .min_w_0()
                .max_w(px(520.0))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(palette().fg))
                        .child(title),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(palette().fg_muted))
                        .whitespace_normal()
                        .child(hint),
                ),
        )
        .child(SettingsPage::toggle(id, enabled, view, apply))
}

fn collapsed_themes<'a>(
    themes: &[&'a translation::Theme],
    current: &str,
) -> Vec<&'a translation::Theme> {
    let mut shown: Vec<&translation::Theme> = themes.iter().copied().take(3).collect();
    if shown.iter().any(|theme| theme.id == current) {
        return shown;
    }
    if let Some(selected) = themes.iter().copied().find(|theme| theme.id == current)
        && let Some(last) = shown.last_mut()
    {
        *last = selected;
    }
    shown
}

fn style_card(
    prefix: &'static str,
    theme: &translation::Theme,
    selected: bool,
    view: WeakEntity<SettingsPage>,
) -> impl IntoElement {
    let id = theme.id;
    div()
        .id(ElementId::Name(format!("{prefix}-{id}").into()))
        .min_w_0()
        .flex()
        .flex_col()
        .justify_center()
        .gap_1()
        .p_2()
        .rounded_xl()
        .border_1()
        .border_color(rgb(if selected {
            palette().accent
        } else {
            palette().border
        }))
        .bg(rgb(if selected {
            palette().accent_soft
        } else {
            palette().card
        }))
        .when(!selected, |card| {
            card.hover(|style| {
                style
                    .bg(rgb(palette().panel_hover))
                    .border_color(rgb(palette().edge))
            })
        })
        .child(
            div()
                .self_start()
                .max_w_full()
                .overflow_hidden()
                .child(crate::translation::style_sample(id)),
        )
        .child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(crate::i18n::t(theme.label_key)),
        )
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| {
                this.translation.reader_theme = id.to_string();
                this.save_translation(cx);
            });
        })
}

fn fold_button(span: u16, open: bool, view: WeakEntity<SettingsPage>) -> impl IntoElement {
    div()
        .id("tran-reader-fold")
        .col_span(span)
        .py_2()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_1()
        .rounded_xl()
        .bg(rgb(palette().accent_soft))
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(palette().accent))
        .hover(|style| style.bg(rgb(palette().panel_active)))
        .child(crate::i18n::t(if open {
            "settings.tranStylesCollapse"
        } else {
            "settings.tranStylesExpand"
        }))
        .child(
            Icon::new(if open {
                IconName::ChevronUp
            } else {
                IconName::ChevronDown
            })
            .size(px(16.0))
            .text_color(rgb(palette().accent)),
        )
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |this, cx| {
                this.reader_styles_open = !this.reader_styles_open;
                cx.notify();
            });
        })
}

/// Pick a saved OpenCode Go, Ollama, or Command Code key when none is chosen.
/// Returns whether `config` changed.
pub(crate) fn ensure_llm_default(config: &mut translation::TranslationConfig) -> bool {
    if !llm_default_needs_apply(config) {
        return false;
    }
    let accounts = translation_accounts();
    if accounts.is_empty() {
        config.llm_account_id.clear();
        config.llm_account_catalog.clear();
        config.llm_model_pinned = false;
        return true;
    }
    let chosen = accounts
        .iter()
        .find(|account| account.id == config.llm_account_id);
    let Some(account) = chosen.or_else(|| preferred_account(&accounts)) else {
        return false;
    };
    let switched = config.llm_account_id != account.id;
    config.llm_account_id = account.id.clone();
    config.llm_account_catalog = account.catalog_id.clone();
    if switched {
        config.llm_model_pinned = false;
    }
    if !config.llm_model_pinned
        && let Some(provider) = translation::llm_account(&account.catalog_id)
    {
        config.llm_model = provider.model_hint.to_string();
    }
    true
}

pub(crate) fn llm_default_needs_apply(config: &translation::TranslationConfig) -> bool {
    let accounts = translation_accounts();
    if accounts.is_empty() {
        return !config.llm_account_id.is_empty();
    }
    let chosen = accounts
        .iter()
        .any(|account| account.id == config.llm_account_id);
    if !chosen || config.llm_model.trim().is_empty() {
        return true;
    }
    !config.llm_model_pinned
        && translation::llm_account(&config.llm_account_catalog)
            .is_some_and(|provider| config.llm_model.trim() != provider.model_hint)
}

fn translation_accounts() -> Vec<SubscriptionDto> {
    let mut accounts: Vec<SubscriptionDto> = list_subscriptions()
        .unwrap_or_default()
        .into_iter()
        .filter(|account| {
            translation::llm_account(&account.catalog_id).is_some() && account.has_credential
        })
        .collect();
    accounts.sort_by_key(|account| {
        translation::LLM_ACCOUNT_PROVIDERS
            .iter()
            .position(|provider| provider.catalog_id == account.catalog_id)
            .unwrap_or(usize::MAX)
    });
    accounts
}

fn preferred_account(accounts: &[SubscriptionDto]) -> Option<&SubscriptionDto> {
    for provider in translation::LLM_ACCOUNT_PROVIDERS {
        let mut found = None;
        for account in accounts {
            if account.catalog_id != provider.catalog_id {
                continue;
            }
            if account.is_active {
                return Some(account);
            }
            if found.is_none() {
                found = Some(account);
            }
        }
        if let Some(account) = found {
            return Some(account);
        }
    }
    None
}

fn account_label(account: &SubscriptionDto) -> String {
    let catalog = match account.catalog_id.as_str() {
        "opencode-go" => "OpenCode Go",
        "ollama" => "Ollama",
        "command-code" => "Command Code",
        other => other,
    };
    if account.display_name.is_empty() || account.display_name.eq_ignore_ascii_case(catalog) {
        catalog.to_string()
    } else {
        format!("{catalog} · {}", account.display_name)
    }
}

fn credential_pills(
    accounts: &[SubscriptionDto],
    current: &str,
    view: WeakEntity<SettingsPage>,
) -> Div {
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.0))
        .p_1()
        .rounded_lg()
        .bg(rgb(palette().well));
    for account in accounts {
        row = row.child(credential_pill(
            &account.id,
            account_label(account).into(),
            current == account.id,
            view.clone(),
            account.id.clone(),
            account.catalog_id.clone(),
        ));
    }
    row
}

fn credential_pill(
    id: &str,
    label: SharedString,
    selected: bool,
    view: WeakEntity<SettingsPage>,
    account_id: String,
    catalog: String,
) -> impl IntoElement {
    div()
        .id(ElementId::Name(format!("tran-credential-{id}").into()))
        .h(px(32.0))
        .px_4()
        .flex()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(if selected {
            palette().fg
        } else {
            palette().fg_muted
        }))
        .when(selected, |pill| pill.bg(rgb(palette().bg)))
        .child(label)
        .on_click(move |_, _, cx| {
            let account_id = account_id.clone();
            let catalog = catalog.clone();
            let _ = view.update(cx, |this, cx| {
                this.select_llm_account(account_id, catalog, cx);
            });
        })
}

#[cfg(test)]
mod target_slider_tests {
    use gpui_kit::{AppContext, Context, IntoElement, Window, point, px, size};

    use super::SettingsPage;
    use crate::test_support::IsolatedDataDir;

    /// `SettingsPage::new` needs a window, so the page is built inside the
    /// window closure and kept in a thin host the test can read back.
    struct TranslationHost(gpui_kit::Entity<SettingsPage>);

    impl gpui_kit::Render for TranslationHost {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            self.0.clone()
        }
    }

    fn paint(cx: &mut gpui_kit::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn click(cx: &mut gpui_kit::VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} missing"));
        cx.simulate_click(
            point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            ),
            Default::default(),
        );
    }

    /// The translation-language row rides the shared sliding thumb
    /// (`chrome::segmented`): the five endonym pills mount and clicking one
    /// persists the new target language.
    #[gpui_kit::test]
    fn target_slider_clicks_switch_language(cx: &mut gpui_kit::TestAppContext) {
        let _dir = IsolatedDataDir::new();
        crate::init_test(cx);
        let (host, cx) = cx.add_window_view(|window, cx| {
            let page = cx.new(|cx| SettingsPage::new(window, cx));
            TranslationHost(page)
        });
        let page = cx.update(|_, cx| host.read(cx).0.clone());
        // A tall window keeps every settings section inside the viewport, so
        // the translation card needs no scrolling to be clickable.
        cx.simulate_resize(size(px(1200.), px(2400.)));
        paint(cx);
        paint(cx);

        let en = cx
            .debug_bounds("en")
            .expect("language pills missing from the translation section");
        let ja = cx.debug_bounds("ja").expect("ja pill mounts beside en");
        // Equal 72px slots with the 2px gap: enough centered padding for the
        // widest endonym without the track outgrowing the settings column.
        assert_eq!(en.size.width, px(72.0));
        assert_eq!(ja.origin.x - en.origin.x, px(74.0));
        assert!(
            en.size.height >= px(30.0) && en.size.height <= px(34.0),
            "pill height stays at the settings row density, got {}",
            en.size.height
        );

        click(cx, "en");
        paint(cx);
        paint(cx);
        cx.update(|_, cx| {
            assert_eq!(page.read(cx).translation.target_lang, "en");
        });

        click(cx, "ja");
        paint(cx);
        paint(cx);
        cx.update(|_, cx| {
            assert_eq!(page.read(cx).translation.target_lang, "ja");
        });
    }
}

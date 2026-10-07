//! Settings → translation switches, engine, and translation style.
//!
//! Machine translation needs no key. LLM translation uses an OpenCode Go,
//! Ollama, or Command Code key already saved in Accounts. The user picks
//! one account as the default. The model starts as that service's
//! recommendation (OpenCode Go: `deepseek-v4.1-flash`). Fetch only fills the
//! model dropdown, which stays closed until opened; picking one keeps it
//! until the account changes. Themes only change how a cached translation
//! is painted.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::Selectable;
use gpui_kit::component::Sizable;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use ss_core::translation::{self, Engine};
use ss_usage::accounts::{SubscriptionDto, list_subscriptions};

use crate::spawn_domain;

use super::{SettingsPage, SettingsSection, card, choice_pills, field_label, section_shell};
use crate::theme::palette;

impl SettingsPage {
    pub(crate) fn render_translation(&self, view: WeakEntity<Self>) -> impl IntoElement {
        let engine = match self.translation.engine {
            Engine::Machine => "machine",
            Engine::Llm => "llm",
        };
        let engines = [
            ("machine", crate::i18n::t("settings.tranEngineMachine")),
            ("llm", crate::i18n::t("settings.tranEngineLlm")),
        ];
        let targets: Vec<(&'static str, SharedString)> = translation::TRANSLATION_LANGUAGES
            .iter()
            .map(|language| (language.code, SharedString::from(language.label)))
            .collect();
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
            .child(field_label(
                crate::i18n::t("settings.tranTarget"),
                choice_pills(
                    "tran-target",
                    &targets,
                    self.translation.target_lang.as_str(),
                    view.clone(),
                    |this, id, cx| {
                        this.translation.target_lang = id.to_string();
                        this.save_translation(cx);
                    },
                ),
            ))
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
                    body = body.child(self.model_field(view.clone()));
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

    fn select_llm_model(&mut self, model: String, cx: &mut Context<Self>) {
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
                    }
                    Err(error) => this.llm_models_error = Some(model_list_message(&error)),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    fn model_field(&self, view: WeakEntity<Self>) -> Div {
        let model = self.translation.llm_model.clone();
        let loading = self.llm_models_loading;
        let pull_view = view.clone();
        // Fetch fills the dropdown data only. The popover stays closed so a
        // full model list never pushes the settings card open on its own.
        let fetched = self.llm_models_for == self.translation.llm_account_id
            && !self.llm_models.is_empty();
        let models = if fetched {
            self.llm_models.clone()
        } else {
            Vec::new()
        };
        let trigger_label: SharedString = if model.is_empty() {
            "—".into()
        } else {
            model.clone().into()
        };
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
                        .child(
                            Popover::new("tran-llm-models")
                                .anchor(Anchor::TopLeft)
                                .offset(px(6.0))
                                .appearance(false)
                                .trigger(
                                    Button::new("tran-llm-model-trigger")
                                        .outline()
                                        .small()
                                        .w_full()
                                        .font_family("monospace")
                                        .label(trigger_label)
                                        .dropdown_caret(true),
                                )
                                .content(move |_, _, cx| {
                                    model_menu(cx, view.clone(), models.clone(), model.clone())
                                }),
                        ),
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
        field = field.child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(crate::i18n::t("settings.tranLlmModelHint")),
        );
        field
    }
}

fn model_menu(
    cx: &mut Context<gpui_kit::component::popover::PopoverState>,
    view: WeakEntity<SettingsPage>,
    models: Vec<String>,
    current: String,
) -> impl IntoElement + use<> {
    let dismiss_popover = cx.entity().downgrade();
    let panel = div()
        .w(px(280.0))
        .rounded_xl()
        .border_1()
        .border_color(rgb(palette().border))
        .bg(rgb(palette().card))
        .p_2()
        .shadow_lg();
    if models.is_empty() {
        return panel.child(
            div()
                .text_xs()
                .text_color(rgb(palette().fg_muted))
                .whitespace_normal()
                .child(crate::i18n::t("settings.tranLlmModelEmpty")),
        );
    }
    let mut rows = Vec::with_capacity(models.len() + 1);
    if !current.is_empty() && !models.iter().any(|id| id == &current) {
        rows.push(current.to_string());
    }
    rows.extend(models.iter().cloned());
    // The catalog scrolls inside the popover so long model lists stay in the
    // dropdown instead of stretching the settings column.
    let mut list = div()
        .id("tran-llm-models")
        .flex()
        .flex_col()
        .gap(px(2.0))
        .max_h(px(280.0))
        .overflow_y_scrollbar();
    for id in rows {
        let selected = id == current;
        let model = id.clone();
        let row_view = view.clone();
        let row_dismiss = dismiss_popover.clone();
        list = list.child(
            Button::new(ElementId::Name(format!("tran-model-{id}").into()))
                .ghost()
                .small()
                .w_full()
                .font_family("monospace")
                .label(id)
                .selected(selected)
                .on_click(move |_, window, app| {
                    app.stop_propagation();
                    let model = model.clone();
                    let _ = row_view.update(app, |this, cx| this.select_llm_model(model, cx));
                    let _ =
                        row_dismiss.update(app, |state, cx| state.dismiss(window, cx));
                }),
        );
    }
    panel.child(list)
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
    if let Some(selected) = themes.iter().copied().find(|theme| theme.id == current) {
        if let Some(last) = shown.last_mut() {
            *last = selected;
        }
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

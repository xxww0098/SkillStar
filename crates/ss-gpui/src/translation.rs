//! Shared translation lookup for cards, the detail column, and the reader.
//!
//! Render reads the local cache when that surface's switch is on, or when
//! the reader button has asked for this opening. Missing English is queued
//! on the domain runtime. A failure is not cached, so the next paint asks
//! again and the reader spinner stays up. Off means the original text, even
//! when a cached line exists.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use gpui_kit::*;
use ss_core::translation::{self, Translation};

use crate::theme::palette;

struct Fill(Vec<Translation>);

impl Default for Fill {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl crate::DomainOutput for Fill {
    fn from_panic(_: crate::DomainPanic) -> Self {
        Self::default()
    }
}

static INFLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static FAILED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

fn target() -> &'static str {
    translation::active_target()
}

/// Drop failed keys so the next paint can ask again. Call after settings save.
pub fn settings_changed() {
    if let Ok(mut failed) = FAILED.lock() {
        failed.clear();
    }
}

/// Which surface a switch in Settings owns.
#[derive(Clone, Copy)]
pub enum Surface {
    /// Skill cards, the detail column, and the market.
    Description,
    /// The SKILL.md reader.
    SkillMd,
}

/// The user turned this surface on.
pub fn enabled(surface: Surface) -> bool {
    let Ok(config) = translation::load_config() else {
        return false;
    };
    match surface {
        Surface::Description => config.translate_descriptions,
        Surface::SkillMd => config.translate_skill_md,
    }
}

/// Cached translation when this surface is on, otherwise the original string.
pub fn display(source: &str, surface: Surface) -> String {
    display_when(source, enabled(surface))
}

/// Cached translation when `on` is true. The reader passes its own button,
/// which does not write the Settings switch.
pub fn display_when(source: &str, on: bool) -> String {
    if !on {
        return source.to_string();
    }
    let target = target();
    translation::lookup(source, target).unwrap_or_else(|| source.to_string())
}

/// Cached, or not something this target would translate. A miss stays pending.
pub fn resolved(source: &str) -> bool {
    let target = target();
    let source = source.trim();
    if source.is_empty() || !translation::needs_translation(source, target) {
        return true;
    }
    if translation::lookup(source, target).is_some() {
        return true;
    }
    let config = translation::load_config().unwrap_or_default();
    let key = translation::cache_key(source, target, &config);
    FAILED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&key)
}

/// A view that shows translated copy. The default does nothing; pages that
/// replay a cached body bump a generation so the body paints the new text
/// once, without treating every animation frame as new copy.
pub trait TranslationHost {
    fn translations_arrived(&mut self) {}
}

/// Queue English strings that are not cached, when this surface's switch is on.
pub fn schedule<V, I, S>(view: &Entity<V>, sources: I, surface: Surface, cx: &mut Context<V>)
where
    V: TranslationHost + 'static,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    schedule_when(view, sources, enabled(surface), cx);
}

/// Queue English strings when `on` is true, even if the Settings switch is off.
/// At most 24 new ones per call. The reader uses this for its translate button.
pub fn schedule_when<V, I, S>(view: &Entity<V>, sources: I, on: bool, cx: &mut Context<V>)
where
    V: TranslationHost + 'static,
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if !on {
        return;
    }
    if translation::load_config()
        .ok()
        .is_some_and(|config| blocked(&config))
    {
        return;
    }
    let target = target();
    let config = translation::load_config().unwrap_or_default();
    let mut batch = Vec::new();
    let mut inflight = INFLIGHT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let failed = FAILED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for source in sources {
        let source = source.as_ref().trim();
        if source.is_empty() || !translation::needs_translation(source, target) {
            continue;
        }
        let key = translation::cache_key(source, target, &config);
        if translation::lookup(source, target).is_some()
            || failed.contains(&key)
            || inflight.contains(&key)
        {
            continue;
        }
        inflight.insert(key);
        batch.push(source.to_string());
        if batch.len() == 24 {
            break;
        }
    }
    drop(failed);
    drop(inflight);
    if batch.is_empty() {
        return;
    }
    let keys: Vec<String> = batch
        .iter()
        .map(|source| translation::cache_key(source, target, &config))
        .collect();
    let target_owned = target.to_string();
    let fill_target = target_owned.clone();
    let auth = llm_auth(&config);
    crate::spawn_domain(
        view,
        cx,
        async move { Fill(translation::fill(batch, &fill_target, auth).await) },
        move |this, _, Fill(done)| {
            this.translations_arrived();
            let config = translation::load_config().unwrap_or_default();
            let target = target_owned.as_str();
            let finished: HashSet<String> = done
                .iter()
                .map(|item| translation::cache_key(&item.source, target, &config))
                .collect();
            let mut inflight = INFLIGHT
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut failed = FAILED
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for key in keys {
                inflight.remove(&key);
                if finished.contains(&key) {
                    failed.remove(&key);
                }
            }
        },
    );
}

fn uses_account(config: &translation::TranslationConfig) -> bool {
    !config.llm_account_id.is_empty()
        && translation::llm_account(&config.llm_account_catalog).is_some()
}

fn llm_auth(config: &translation::TranslationConfig) -> translation::LlmAuth {
    if uses_account(config)
        && let Some(provider) = translation::llm_account(&config.llm_account_catalog)
    {
        let api_key = ss_usage::accounts::get_subscription_api_key(config.llm_account_id.clone())
            .ok()
            .flatten()
            .unwrap_or_default();
        return translation::LlmAuth {
            base_url: provider.base_url.to_string(),
            api_key,
        };
    }
    translation::LlmAuth::default()
}

fn blocked(config: &translation::TranslationConfig) -> bool {
    if config.engine != translation::Engine::Llm {
        return false;
    }
    config.llm_model.trim().is_empty() || llm_auth(config).api_key.trim().is_empty()
}

/// Hint when LLM translation cannot run. Machine translation has no hint.
pub fn blocked_hint() -> Option<SharedString> {
    let config = translation::load_config().ok()?;
    if !blocked(&config) {
        return None;
    }
    Some(crate::i18n::t("detailPanel.translationNeedsAccount"))
}

/// Translation style from Settings, for a string that already has a translation.
pub fn paint_card(row: Div) -> AnyElement {
    paint_reader(row)
}

/// Same style, for the line under an English paragraph.
pub fn paint_reader(row: Div) -> AnyElement {
    paint(
        row,
        translation::load_config()
            .ok()
            .map(|config| config.reader_theme),
    )
}

/// Sample line used by the settings gallery. The same painter styles a real
/// translation, except the two fills that have to be drawn per character.
pub(crate) fn style_sample(theme_id: &str) -> AnyElement {
    let text = crate::i18n::t("settings.tranStyleSample");
    match theme_id {
        "grad" => gradient_sample(text).into_any_element(),
        "color" => colorful_sample(text).into_any_element(),
        "dot" => dotted(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(text),
            false,
        ),
        _ => paint(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(text),
            Some(theme_id.to_string()),
        ),
    }
}

fn paint(row: Div, theme: Option<String>) -> AnyElement {
    let theme = theme.unwrap_or_else(|| "none".to_string());
    let accent = rgb(palette().accent);
    match theme.as_str() {
        "line" => row
            .underline()
            .text_decoration_color(accent)
            .text_decoration_1()
            .into_any_element(),
        "dot" => dotted(row, true),
        "dash" => row
            .border_b_1()
            .border_dashed()
            .border_color(accent)
            .into_any_element(),
        "dbold" => row
            .border_b_2()
            .border_dashed()
            .border_color(accent)
            .into_any_element(),
        "wavy" => row
            .underline()
            .text_decoration_wavy()
            .text_decoration_color(accent)
            .text_decoration_1()
            .into_any_element(),
        "wbold" => row
            .underline()
            .text_decoration_wavy()
            .text_decoration_color(accent)
            .text_decoration_2()
            .into_any_element(),
        "box" => row
            .border_1()
            .border_dashed()
            .border_color(accent)
            .rounded_md()
            .px_1()
            .into_any_element(),
        "xbold" => row
            .border_2()
            .border_dashed()
            .border_color(accent)
            .rounded_md()
            .px_1()
            .into_any_element(),
        "mark" => row
            .text_bg(accent.alpha(0.45))
            .rounded_sm()
            .into_any_element(),
        "gmark" => row
            .text_bg(rgb(palette().violet).alpha(0.4))
            .rounded_sm()
            .into_any_element(),
        "fuzzy" => row
            .text_color(rgb(palette().fg_faint))
            .opacity(0.55)
            .into_any_element(),
        "hi" => row
            .bg(accent)
            .text_color(rgb(palette().on_accent))
            .rounded_sm()
            .px_1()
            .into_any_element(),
        "quote" => row
            .border_l_4()
            .border_color(accent)
            .bg(rgb(palette().accent_soft))
            .pl_2()
            .pr_1()
            .into_any_element(),
        "grad" => row.text_color(rgb(palette().violet)).into_any_element(),
        "blink" => row.opacity(0.35).into_any_element(),
        "glow" => row
            .bg(rgb(palette().danger).alpha(0.16))
            .rounded_md()
            .px_1()
            .into_any_element(),
        "color" => row
            .bg(linear_gradient(
                45.,
                linear_color_stop(rgb(palette().ok).alpha(0.55), 0.),
                linear_color_stop(rgb(palette().accent).alpha(0.55), 1.),
            ))
            .rounded_sm()
            .px_1()
            .into_any_element(),
        _ => row.into_any_element(),
    }
}

fn dotted(row: Div, wide: bool) -> AnyElement {
    let count = if wide { 80 } else { 10 };
    let mut rule = div()
        .flex()
        .flex_row()
        .gap(px(3.0))
        .h(px(3.0))
        .overflow_hidden();
    if wide {
        rule = rule.w_full().flex_wrap();
    }
    for _ in 0..count {
        rule = rule.child(
            div()
                .size(px(2.0))
                .flex_shrink_0()
                .rounded_full()
                .bg(rgb(palette().accent)),
        );
    }
    let mut column = div().flex().flex_col().gap(px(2.0));
    column = if wide {
        column.w_full()
    } else {
        column.items_start()
    };
    column.child(row).child(rule).into_any_element()
}

fn gradient_sample(text: SharedString) -> Div {
    let chars: Vec<char> = text.chars().collect();
    let last = chars.len().saturating_sub(1).max(1);
    let stops = [palette().accent, palette().violet, palette().danger];
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD);
    for (index, ch) in chars.into_iter().enumerate() {
        let slot = index * (stops.len() - 1) / last;
        row = row.child(div().text_color(rgb(stops[slot])).child(ch.to_string()));
    }
    row
}

fn colorful_sample(text: SharedString) -> Div {
    let fills = [
        rgb(palette().ok).alpha(0.55),
        rgb(palette().warn).alpha(0.55),
        rgb(palette().danger).alpha(0.4),
        rgb(palette().accent).alpha(0.4),
    ];
    let mut row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD);
    for (index, ch) in text.chars().enumerate() {
        row = row.child(
            div()
                .px(px(1.0))
                .bg(fills[index % fills.len()])
                .text_color(rgb(palette().fg))
                .child(ch.to_string()),
        );
    }
    row
}

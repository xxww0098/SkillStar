//! Interface language for the GPUI shell.
//!
//! App copy lives in `assets/locales/{en,zh-CN}.json`. Copy owned by the kit's
//! components lives in `locales/ui.yml` and resolves through `rust-i18n`; see
//! [`publish`] and [`install`]. [`set_language`] updates a process-wide code, a
//! GPUI global, and the kit's component locale; `Shell` observes that global,
//! re-renders the pages, and they pick up [`t`] on the next frame.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Once, OnceLock, RwLock};

use gpui_kit::component::input::InputState;
use gpui_kit::*;

static CURRENT: RwLock<String> = RwLock::new(String::new());

// Tests on one worker must not flip the language other workers are rendering.
thread_local! {
    static OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

struct Catalogs {
    en: HashMap<String, String>,
    zh: HashMap<String, String>,
}

static CATALOGS: OnceLock<Catalogs> = OnceLock::new();

/// Observed by `Shell`. Mutating it (via [`set_language`]) makes the shell
/// re-render its pages, which then read fresh copy from [`t`].
pub struct UiLang {
    code: String,
}

impl Global for UiLang {}

/// Namespace `locales/ui.yml` onto the kit's component copy. Call once per
/// process, before the first component is created: `rust-i18n` panics when a
/// backend is extended twice.
pub(crate) fn extend_component_copy() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        use gpui_kit::component as gpui_component;

        rust_i18n::extend!(gpui_component);
    });
}

/// Load `gui_prefs.json` and publish the language before the first frame.
pub fn install(cx: &mut App) {
    let code = normalize(&crate::prefs::load().language);
    publish(&code);
    cx.set_global(UiLang { code });
}

/// Persist happens in the caller. This applies `code` immediately.
///
/// `set_locale` sits outside GPUI's change tracking, so the windows that
/// show component strings are refreshed here. `UiLang` still wakes `Shell`,
/// which rebuilds cached placeholders the refresh would otherwise keep.
pub fn set_language(cx: &mut App, code: &str) {
    let code = normalize(code);
    publish(&code);
    cx.global_mut::<UiLang>().code = code;
    cx.refresh_windows();
}

/// Dotted key, e.g. `sidebar.skills`. Missing keys fall back to the other
/// catalog, then to the key itself.
pub fn t(key: &str) -> SharedString {
    lookup(&current_code(), key).into()
}

/// `{{name}}` placeholders, same shape as the React catalogs.
pub fn tf(key: &str, vars: &[(&str, &str)]) -> SharedString {
    let mut text = lookup(&current_code(), key);
    for (name, value) in vars {
        let token = format!("{{{{{name}}}}}");
        text = text.replace(&token, value);
    }
    text.into()
}

/// `"en"` or `"zh-CN"`. Empty until [`install`] or [`set_language`].
pub fn language() -> String {
    current_code()
}

/// Tests only. Overrides this thread until the guard drops, so parallel
/// tests keep the process language.
#[cfg(test)]
pub(crate) fn set_language_for_test(code: &str) -> LanguageGuard {
    LanguageGuard::apply(code);
    LanguageGuard
}

#[cfg(test)]
pub(crate) struct LanguageGuard;

#[cfg(test)]
impl LanguageGuard {
    pub(crate) fn set(&self, code: &str) {
        Self::apply(code);
    }

    fn apply(code: &str) {
        let code = normalize(code);
        OVERRIDE.with(|slot| *slot.borrow_mut() = Some(code));
    }
}

#[cfg(test)]
impl Drop for LanguageGuard {
    fn drop(&mut self) {
        OVERRIDE.with(|slot| *slot.borrow_mut() = None);
    }
}

/// `"en"` or `"zh-CN"`. Anything else follows the React fallback, `zh-CN`.
pub fn normalize(code: &str) -> String {
    let code = code.trim();
    if code.eq_ignore_ascii_case("en") || code.to_ascii_lowercase().starts_with("en-") {
        "en".into()
    } else {
        "zh-CN".into()
    }
}

pub fn sync_placeholder(
    input: &Entity<InputState>,
    text: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    if input.read(cx).presentation().placeholder() == &text {
        return;
    }
    let _ = input.update(cx, |state, cx| state.set_placeholder(text, window, cx));
}

/// Publish `code` to both copy layers: the app catalogs read by [`t`] and
/// [`tf`], and the kit's component copy, whose keys resolve through
/// `rust-i18n` (see `locales/ui.yml`).
fn publish(code: &str) {
    set_current(code);
    gpui_kit::component::set_locale(code);
}

fn set_current(code: &str) {
    if let Ok(mut guard) = CURRENT.write() {
        *guard = code.to_string();
    }
}

fn current_code() -> String {
    if let Some(code) = OVERRIDE.with(|slot| slot.borrow().clone()) {
        return code;
    }
    CURRENT
        .read()
        .ok()
        .filter(|code| !code.is_empty())
        .map(|code| code.clone())
        .unwrap_or_else(|| "zh-CN".into())
}

fn catalogs() -> &'static Catalogs {
    CATALOGS.get_or_init(|| Catalogs {
        en: flatten(include_str!("../assets/locales/en.json")),
        zh: flatten(include_str!("../assets/locales/zh-CN.json")),
    })
}

fn flatten(raw: &str) -> HashMap<String, String> {
    let value: serde_json::Value = serde_json::from_str(raw).expect("locale json");
    let mut out = HashMap::new();
    walk(&value, "", &mut out);
    out
}

fn walk(value: &serde_json::Value, prefix: &str, out: &mut HashMap<String, String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let next = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                walk(child, &next, out);
            }
        }
        serde_json::Value::String(text) => {
            out.insert(prefix.to_string(), text.clone());
        }
        _ => {}
    }
}

fn lookup(lang: &str, key: &str) -> String {
    let cats = catalogs();
    let (primary, fallback) = if lang == "en" {
        (&cats.en, &cats.zh)
    } else {
        (&cats.zh, &cats.en)
    };
    primary
        .get(key)
        .or_else(|| fallback.get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize, set_language_for_test, t, tf};

    #[test]
    fn normalize_matches_react_codes() {
        assert_eq!(normalize("en"), "en");
        assert_eq!(normalize("en-US"), "en");
        assert_eq!(normalize("zh-CN"), "zh-CN");
        assert_eq!(normalize("zh"), "zh-CN");
        assert_eq!(normalize(""), "zh-CN");
    }

    #[test]
    fn catalogs_translate_sidebar_and_interpolate() {
        let lang = set_language_for_test("zh-CN");
        assert_eq!(t("sidebar.skills").as_ref(), "技能");
        assert_eq!(
            tf("settings.storageHubCount", &[("count", "3")]).as_ref(),
            "3 个技能"
        );
        lang.set("en");
        assert_eq!(t("sidebar.skills").as_ref(), "Cards");
        assert_eq!(
            tf("settings.activeCount", &[("enabled", "1"), ("total", "4")]).as_ref(),
            "1 / 4 active"
        );
        assert_eq!(t("missing.key").as_ref(), "missing.key");
    }

    /// Resolve `key` the way the kit's own `t!` does: through the component
    /// backend, where the extension installed by [`super::extend_component_copy`]
    /// lives. App-side `rust_i18n::t!` in this crate compiles against a static
    /// table of the app's own keys, so it never reaches component copy.
    fn kit_copy(key: &str, locale: &str) -> Option<String> {
        gpui_kit::component::_rust_i18n_try_translate(locale, key).map(|v| v.into_owned())
    }

    #[test]
    fn kit_copy_overrides_resolve_through_the_component_namespace() {
        super::extend_component_copy();

        // The kit calls both of these keys but defines neither, so its built-ins
        // cannot answer them: only `locales/ui.yml` can. That is what proves the
        // extension was installed under the `gpui_component` namespace.
        assert_eq!(
            kit_copy("Combobox.placeholder", "en").as_deref(),
            Some("Please select")
        );
        assert_eq!(
            kit_copy("Combobox.placeholder", "zh-CN").as_deref(),
            Some("请选择")
        );
        assert_eq!(kit_copy("Copy", "zh-CN").as_deref(), Some("复制"));
    }

    #[gpui_kit::test]
    fn set_language_publishes_to_the_kit_component_locale(cx: &mut gpui_kit::TestAppContext) {
        // `set_locale` is process-global. Put the previous code back even if
        // an assertion fails, so other tests keep the locale they started with.
        struct Restore(String);
        impl Drop for Restore {
            fn drop(&mut self) {
                gpui_kit::component::set_locale(&self.0);
            }
        }
        let _restore = Restore(gpui_kit::component::locale().to_string());

        crate::init_test(cx);
        cx.update(|cx| {
            crate::i18n::install(cx);
            crate::i18n::set_language(cx, "en");
        });
        assert_eq!(&*gpui_kit::component::locale(), "en");
        assert_eq!(kit_copy("Copy", "en").as_deref(), Some("Copy"));

        cx.update(|cx| crate::i18n::set_language(cx, "zh-HK"));
        let locale = &*gpui_kit::component::locale();
        assert_eq!(locale, "zh-CN");
        assert_eq!(kit_copy("Copy", locale).as_deref(), Some("复制"));
    }

    #[test]
    fn usage_card_copy_switches() {
        let lang = set_language_for_test("zh-CN");
        assert_eq!(t("usage.meterResetSoon").as_ref(), "即将重置");
        assert_eq!(t("usage.resetCards").as_ref(), "重置卡");
        assert_eq!(
            tf("usage.remainingPercent", &[("percent", "86")]).as_ref(),
            "剩余 86%"
        );
        lang.set("en");
        assert_eq!(t("usage.meterResetSoon").as_ref(), "Resetting soon");
        assert_eq!(t("usage.resetCards").as_ref(), "Reset cards");
        assert_eq!(
            tf("usage.remainingPercent", &[("percent", "86")]).as_ref(),
            "86% left"
        );
        assert_eq!(tf("usage.meterDays", &[("n", "6")]).as_ref(), "6d");
    }
}

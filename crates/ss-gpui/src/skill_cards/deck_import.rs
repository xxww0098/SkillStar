//! Share-code and bundle dialogs for the skill-card toolbar.
//! The React page puts both next to New group; neither is a refresh button.

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::*;
use ss_skills::share_install::{ShareCodeKind, parse_share_code};
use ss_skills::skill_bundle::AnyBundleImport;

use super::SkillCardsPage;

pub(crate) fn open_share_import(
    view: WeakEntity<SkillCardsPage>,
    window: &mut Window,
    cx: &mut App,
) {
    // Builder runs every paint; an input created inside it is wiped each frame.
    let input = cx.new(|cx| {
        InputState::new(window, cx).placeholder(crate::i18n::t("toolbar.searchPlaceholder"))
    });
    let submitted = input.clone();
    crate::chrome::open_form_dialog(
        window,
        cx,
        crate::i18n::t("common.import"),
        crate::i18n::t("common.import"),
        false,
        move |_, _| Input::new(&input).into_any_element(),
        move |_, cx| {
            let text = submitted.read(cx).value().trim().to_string();
            if text.is_empty() {
                return false;
            }
            let _ = view.update(cx, |this, cx| this.import_share_code(text, cx));
            true
        },
    );
}

pub(crate) fn open_bundle_import(
    view: WeakEntity<SkillCardsPage>,
    window: &mut Window,
    cx: &mut App,
) {
    let input = cx.new(|cx| {
        InputState::new(window, cx).placeholder(crate::i18n::t("skillCards.bundlePathPlaceholder"))
    });
    let submitted = input.clone();
    crate::chrome::open_form_dialog(
        window,
        cx,
        crate::i18n::t("toolbar.importFile"),
        crate::i18n::t("toolbar.importFile"),
        false,
        move |_, _| Input::new(&input).into_any_element(),
        move |_, cx| {
            let path = submitted.read(cx).value().trim().to_string();
            if path.is_empty() {
                return false;
            }
            let _ = view.update(cx, |this, cx| this.import_bundle_path(path, cx));
            true
        },
    );
}

/// Install a share code, and for a deck code create the deck. Skills stay in
/// the hub until the user links an Agent. Returns a notice when a Skill was skipped.
pub(crate) fn apply_share_code(text: &str) -> anyhow::Result<Option<String>> {
    let parsed = parse_share_code(text).map_err(anyhow::Error::msg)?;
    let outcome = ss_skills::share_install::install_share_and_deck(
        parsed.kind == ShareCodeKind::Deck,
        &parsed.payload.n,
        parsed.payload.d,
        parsed.payload.i,
        parsed.payload.s,
    )?;
    Ok(Some(skipped_notice(&outcome.summary.skipped)).filter(|notice| !notice.is_empty()))
}

pub(crate) fn apply_bundle(path: &str) -> anyhow::Result<Option<String>> {
    let fallback = crate::i18n::t("importDeckBundleModal.deckBundle").to_string();
    let outcome = ss_skills::skill_bundle::import_bundle_and_deck(path, &fallback, |count| {
        crate::i18n::tf(
            "importDeckBundleModal.skillsCount",
            &[("count", &count.to_string())],
        )
        .to_string()
    })?;
    match outcome.import {
        AnyBundleImport::Single(_) => Ok(None),
        AnyBundleImport::Multi(result) if !result.skipped.is_empty() => {
            Ok(Some(skipped_notice(&result.skipped)))
        }
        AnyBundleImport::Multi(_) => Ok(None),
    }
}

fn skipped_notice(skipped: &[ss_skills::share_install::SkippedSkill]) -> String {
    if skipped.is_empty() {
        return String::new();
    }
    let names = skipped
        .iter()
        .map(|skip| match &skip.detail {
            Some(detail) => format!("{} ({detail})", skip.name),
            None => format!("{} ({})", skip.name, skip.reason),
        })
        .collect::<Vec<_>>()
        .join("; ");
    crate::i18n::tf("skillCards.installAllFailedDetailed", &[("names", &names)]).to_string()
}

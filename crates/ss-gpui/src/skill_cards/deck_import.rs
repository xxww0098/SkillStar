//! Share-code and bundle dialogs for the skill-card toolbar.
//! The React page puts both next to New group; neither is a refresh button.

use std::collections::HashMap;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::*;
use ss_skills::share_install::{ShareCodeInstallSummary, ShareCodeKind, parse_share_code};
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
        140.0,
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
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("/path/to/deck.ags|.agd"));
    let submitted = input.clone();
    crate::chrome::open_form_dialog(
        window,
        cx,
        crate::i18n::t("toolbar.importFile"),
        crate::i18n::t("toolbar.importFile"),
        false,
        140.0,
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
    let summary = ss_skills::share_install::install_from_share_code(parsed.payload.s.clone());
    if parsed.kind == ShareCodeKind::Deck {
        let skills: Vec<String> = parsed
            .payload
            .s
            .iter()
            .map(|skill| skill.n.clone())
            .filter(|name| !name.is_empty())
            .collect();
        let sources = parsed
            .payload
            .s
            .iter()
            .filter(|skill| skill.remote().is_ok())
            .map(|skill| (skill.n.clone(), skill.u.clone()))
            .collect::<HashMap<_, _>>();
        let name = if parsed.payload.n.trim().is_empty() {
            "Imported deck".to_string()
        } else {
            parsed.payload.n.clone()
        };
        let icon = if parsed.payload.i.trim().is_empty() {
            "📦".to_string()
        } else {
            parsed.payload.i.clone()
        };
        ss_skills::skill_group::create_group(name, parsed.payload.d, icon, skills, sources)?;
    }
    Ok(skipped_notice(&summary))
}

pub(crate) fn apply_bundle(path: &str) -> anyhow::Result<Option<String>> {
    match ss_skills::skill_bundle::import_any_bundle(path, false)? {
        AnyBundleImport::Single(_) => {}
        AnyBundleImport::Multi(result) => {
            // Deck import mirrors React's `onDeckImported`: the skills are
            // the payload, the auto-created group is a convenience — a
            // duplicate group name must not turn a finished install into an
            // error notice.
            let skills = result.skill_names;
            if !skills.is_empty() {
                let deck = ss_skills::skill_bundle::deck_name_from_bundle_path(path);
                let deck = if deck.is_empty() {
                    crate::i18n::t("importDeckBundleModal.deckBundle").to_string()
                } else {
                    deck
                };
                let desc = crate::i18n::tf(
                    "importDeckBundleModal.skillsCount",
                    &[("count", &skills.len().to_string())],
                )
                .to_string();
                let _ = ss_skills::skill_group::create_group(
                    deck,
                    desc,
                    "📦".to_string(),
                    skills.clone(),
                    HashMap::new(),
                );
            }
        }
    }
    Ok(None)
}

fn skipped_notice(summary: &ShareCodeInstallSummary) -> Option<String> {
    if summary.skipped.is_empty() {
        return None;
    }
    let names = summary
        .skipped
        .iter()
        .map(|skip| match &skip.detail {
            Some(detail) => format!("{} ({detail})", skip.name),
            None => format!("{} ({})", skip.name, skip.reason),
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(crate::i18n::tf("skillCards.installAllFailedDetailed", &[("names", &names)]).to_string())
}

//! Deck-import facade — the `skill_cards/deck_import.rs` slice owns this
//! file (the smart-field share-code gate, the two install flavors, the
//! native `.ags`/`.agd` picker).
//!
//! Domain seams (`crates/ss-skills`), kept as thin wraps:
//! - `share_install::{looks_like_share_code, parse_share_code,
//!   install_share_and_deck}` — paste-side routing and install;
//! - `skill_bundle::import_bundle_and_deck` — file-side install;
//! - `skill_group::DeckAttach` — whether the deck itself was created.
//!
//! The heavy installs run on `blocking` (tokio spawn_blocking), the same
//! off-thread shape the GPUI dialog's `spawn_domain` uses, so the engine
//! thread never waits on git or disk.

use nsh_core::file_dialog::OpenDialog;
use nsh_core::prelude::*;
use ss_skills::share_install::{
    self, ShareCodeInstallSummary, ShareCodeKind, ShareCodeSkill, SkippedSkill,
};
use ss_skills::skill_bundle::{AnyBundleImport, import_bundle_and_deck};
use ss_skills::skill_group::DeckAttach;

/// The deck-import seam (`SkillCardsImport` in TSX).
#[module]
pub struct SkillCardsImport;

/// One member of a parsed share code, in the domain's wire field names
/// (`n` name, `u` git URL, `c` inline base64 SKILL.md, `p` needs-auth) —
/// kept verbatim so the payload round-trips through TSX unchanged.
#[derive(Debug, JsObject)]
pub struct ShareSkillInfo {
    /// Skill name (human-readable id).
    pub n: String,
    /// Git URL when the skill is git-backed; may be empty.
    pub u: String,
    /// Inline SKILL.md content (Base64 UTF-8), when present.
    pub c: Option<String>,
    /// `true` when the repo requires auth (private).
    pub p: Option<bool>,
}

/// What `parseShareCode` hands the renderer: the code's flavour plus the
/// decoded payload, ready to feed back into `installShareCode`.
#[derive(Debug, JsObject)]
pub struct ParsedShareCodeInfo {
    /// `"skills"` (`ags-`) or `"deck"` (`agd-`).
    pub kind: String,
    /// Deck (or pack) name from the payload.
    pub name: String,
    /// Deck description.
    pub description: String,
    /// Deck icon (often an emoji).
    pub icon: String,
    /// The skill entries to install.
    pub skills: Vec<ShareSkillInfo>,
}

/// A skill that did not install, with the domain's reason slug
/// (`no_source`, `install_failed`, …) — the renderer maps it to copy.
#[derive(JsObject)]
pub struct SkippedSkillInfo {
    /// Skill name.
    pub name: String,
    /// Reason slug; unknown slugs render as `install_failed`.
    pub reason: String,
}

/// Completed-phase payload, already reduced per source so the renderer
/// never re-derives counts (deck_import.rs `ImportDone` parity): merged
/// installed names (repo + embedded), the existing ones, the skipped rows,
/// and whether the deck name was already taken.
#[derive(JsObject)]
pub struct DeckImportOutcome {
    /// Skills the share code requested (bundle flavor: the imported count).
    pub requested: usize,
    /// Skills already present before this run.
    pub existing: Vec<String>,
    /// Skills this run installed (repo installs and embedded decodes).
    pub installed: Vec<String>,
    /// Skills that did not install, with their reason slugs.
    pub skipped: Vec<SkippedSkillInfo>,
    /// A deck with this name already existed: skills imported, deck not.
    pub name_taken: bool,
}

#[module]
impl SkillCardsImport {
    /// Cheap input sniff for share codes: a bare `ags-`/`agd-` token of
    /// real code length, or one embedded in a longer pasted message. Drives
    /// the live detected banner; the authoritative check stays
    /// `parseShareCode`. Pure, no IO, no side effects.
    pub fn looks_like_share_code(text: String) -> bool {
        share_install::looks_like_share_code(&text)
    }

    /// Cheap input sniff for GitHub/Git repository URLs, `owner/repo`
    /// shorthands, and pasted install commands (excluding share codes and
    /// local `.ags`/`.agd` bundle paths). Drives the live banner and handoff
    /// to the repository import modal. Pure, no IO.
    pub fn looks_like_repo_source(text: String) -> bool {
        ss_skills::source_resolver::looks_like_remote_source(&text)
    }

    /// Decode an `ags-`/`agd-` share code (7-day TTL, 1 MB cap) into its
    /// flavour and payload. Pure; an invalid, corrupted or expired code
    /// rejects with the domain's reason.
    pub fn parse_share_code(text: String) -> Result<ParsedShareCodeInfo, String> {
        let parsed = share_install::parse_share_code(&text)?;
        let kind = match parsed.kind {
            ShareCodeKind::Skills => "skills",
            ShareCodeKind::Deck => "deck",
        };
        Ok(ParsedShareCodeInfo {
            kind: kind.to_string(),
            name: parsed.payload.n,
            description: parsed.payload.d,
            icon: parsed.payload.i,
            skills: parsed
                .payload
                .s
                .into_iter()
                .map(|skill| ShareSkillInfo {
                    n: skill.n,
                    u: skill.u,
                    c: skill.c,
                    p: skill.p,
                })
                .collect(),
        })
    }

    /// Install a parsed share code's members and create the deck when the
    /// flavour is `deck` (`share_install::install_share_and_deck`). Runs on
    /// the blocking pool; resolves with the reduced outcome, rejects with
    /// the install error.
    pub async fn install_share_code(
        kind: String,
        name: String,
        description: String,
        icon: String,
        skills: Vec<ShareSkillInfo>,
    ) -> Result<DeckImportOutcome, String> {
        let deck = kind == "deck";
        let skills: Vec<ShareCodeSkill> = skills
            .into_iter()
            .map(|skill| ShareCodeSkill {
                n: skill.n,
                u: skill.u,
                c: skill.c,
                p: skill.p,
            })
            .collect();
        let joined = blocking(move || {
            share_install::install_share_and_deck(deck, &name, description, icon, skills)
                .map(|outcome| (outcome.summary, outcome.deck))
        })
        .await
        .map_err(|err| err.to_string())?;
        let (summary, deck) = joined.map_err(|err| format!("{err:#}"))?;
        Ok(outcome_from_summary(summary, deck == DeckAttach::NameTaken))
    }

    /// Import a local `.ags`/`.agd` bundle path and attach the deck for an
    /// `.agd` (`skill_bundle::import_bundle_and_deck`). `fallbackName` is
    /// the deck name when the file name carries none; `countLabel` is a
    /// `{{count}}` template for the deck description. Runs on the blocking
    /// pool; resolves with the reduced outcome, rejects with the import
    /// error.
    pub async fn install_bundle(
        path: String,
        fallback_name: String,
        count_label: String,
    ) -> Result<DeckImportOutcome, String> {
        let joined = blocking(move || {
            import_bundle_and_deck(&path, &fallback_name, |count| {
                count_label.replace("{{count}}", &count.to_string())
            })
            .map(|outcome| (outcome.import, outcome.deck))
        })
        .await
        .map_err(|err| err.to_string())?;
        let (import, deck) = joined.map_err(|err| format!("{err:#}"))?;
        let (names, skipped) = match import {
            AnyBundleImport::Single(result) => (vec![result.name], Vec::new()),
            AnyBundleImport::Multi(result) => (result.skill_names, result.skipped),
        };
        Ok(DeckImportOutcome {
            requested: names.len(),
            existing: Vec::new(),
            installed: names,
            skipped: skipped_from(skipped),
            name_taken: deck == DeckAttach::NameTaken,
        })
    }

    /// Show the system open dialog for one `.ags`/`.agd` file. Resolves
    /// with the picked path, or `null` when the user cancelled or the
    /// platform has no file dialog — both leave the dialog where it is
    /// (deck_import.rs `pick_file` treats them alike).
    pub async fn pick_deck_file(prompt: String) -> Option<String> {
        let dialog = OpenDialog::files().title(prompt);
        match pick_paths(dialog).await {
            Ok(Some(paths)) => paths
                .into_iter()
                .next()
                .map(|path| path.to_string_lossy().into_owned()),
            _ => None,
        }
    }
}

fn skipped_from(skipped: Vec<SkippedSkill>) -> Vec<SkippedSkillInfo> {
    skipped
        .into_iter()
        .map(|entry| SkippedSkillInfo {
            name: entry.name,
            reason: entry.reason,
        })
        .collect()
}

/// Merge repo installs and embedded decodes into one `installed` list and
/// drop the fields the renderer never reads (`ImportDone::from_summary`
/// parity).
fn outcome_from_summary(
    summary: ShareCodeInstallSummary,
    name_taken: bool,
) -> DeckImportOutcome {
    let installed: Vec<String> = summary
        .installed_names
        .iter()
        .chain(summary.embedded_names.iter())
        .cloned()
        .collect();
    DeckImportOutcome {
        requested: summary.requested_count,
        existing: summary.existing_names,
        installed,
        skipped: skipped_from(summary.skipped),
        name_taken,
    }
}

#[cfg(test)]
mod tests {
    use super::SkillCardsImport;
    use ss_skills::share_install::{
        ShareCodePayload, ShareCodeSkill, encode_share_code, parse_share_code,
    };

    fn payload() -> ShareCodePayload {
        ShareCodePayload {
            n: "Pack".into(),
            d: "Tools".into(),
            i: "📦".into(),
            s: vec![ShareCodeSkill {
                n: "demo".into(),
                u: "https://github.com/o/demo".into(),
                c: None,
                p: None,
            }],
        }
    }

    #[test]
    fn sniffs_share_codes_the_way_the_banner_needs() {
        let code = format!("agd-{}", "A".repeat(64));
        assert!(SkillCardsImport::looks_like_share_code(code.clone()));
        assert!(SkillCardsImport::looks_like_share_code(format!(
            "Deck: pack\nsee {code} thanks"
        )));
        assert!(!SkillCardsImport::looks_like_share_code("ags-short".into()));
        assert!(!SkillCardsImport::looks_like_share_code("".into()));
    }

    #[test]
    fn sniffs_repo_sources_without_catching_bundles_or_share_codes() {
        assert!(SkillCardsImport::looks_like_repo_source(
            "https://github.com/vuejs-ai/skills".into()
        ));
        assert!(SkillCardsImport::looks_like_repo_source(
            "vuejs-ai/skills".into()
        ));
        assert!(SkillCardsImport::looks_like_repo_source(
            "npx skills add vuejs-ai/skills".into()
        ));
        assert!(!SkillCardsImport::looks_like_repo_source(
            "/tmp/pack.agd".into()
        ));
        assert!(!SkillCardsImport::looks_like_repo_source(format!(
            "agd-{}/{}",
            "A".repeat(20),
            "B".repeat(20)
        )));
    }

    #[test]
    fn parse_reports_the_flavour_and_the_members() {
        let info = SkillCardsImport::parse_share_code(encode_share_code(
            ss_skills::share_install::ShareCodeKind::Deck,
            &payload(),
        ))
        .expect("round trip parses");
        assert_eq!(info.kind, "deck");
        assert_eq!(info.name, "Pack");
        assert_eq!(info.icon, "📦");
        assert_eq!(info.skills.len(), 1);
        assert_eq!(info.skills[0].n, "demo");
        assert_eq!(info.skills[0].u, "https://github.com/o/demo");
        assert_eq!(info.skills[0].c, None);
        assert_eq!(info.skills[0].p, None);
    }

    #[test]
    fn parse_rejects_garbage_with_the_domain_reason() {
        let err = SkillCardsImport::parse_share_code("not a code".into())
            .expect_err("plain text is not a share code");
        assert!(err.contains("prefix"));
    }

    #[test]
    fn the_skills_prefix_maps_to_the_skills_flavour() {
        let info = SkillCardsImport::parse_share_code(encode_share_code(
            ss_skills::share_install::ShareCodeKind::Skills,
            &payload(),
        ))
        .expect("round trip parses");
        assert_eq!(info.kind, "skills");
        // The domain decode agrees on the flavour split.
        let parsed = parse_share_code(&encode_share_code(
            ss_skills::share_install::ShareCodeKind::Skills,
            &payload(),
        ))
        .unwrap();
        assert_eq!(parsed.payload.s.len(), info.skills.len());
    }
}

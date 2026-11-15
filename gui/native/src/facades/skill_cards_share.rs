//! Deck-share facade — the `skill_cards/share_sheet.rs` slice owns this
//! file (the share dialog's two ways out: the 7-day `agd-` code and the
//! `.agd` bundle export).
//!
//! Thin wrappers over `ss_skills` mirroring the GPUI sheet's `spawn_domain`
//! calls (`share_sheet.rs:38-138`): one call builds the code (hub git
//! remotes stay off the UI thread), one runs the bundle export through
//! `blocking` (the GPUI side uses `tokio::task::spawn_blocking`), and one
//! reveals the exported file in the platform file manager (the GPUI side
//! shells out in `os_open.rs`; here `reveal_path` is the core's own
//! request, resolved by the host). Clipboard writes stay in TSX (`nsh`'s
//! `Clipboard.writeText`) — the test host keeps its own clipboard.

use std::collections::BTreeMap;
use std::path::PathBuf;

use nsh_core::prelude::*;
use nsh_core::JsonValue;
use ss_skills::share_install::{ShareCodeKind, ShareCodePayload, ShareCodeSkill, encode_share_code};

/// The deck-share seam (`SkillCardsShare` in TSX).
#[module]
pub struct SkillCardsShare;

#[module]
impl SkillCardsShare {
    /// Builds the deck's 7-day `agd-` share code. A code carries repo URLs
    /// only: each member resolves through the deck's `skillSources` first
    /// and the hub's git remotes as fallback (`list_installed_skills`, read
    /// off the UI thread); members with neither stay out of the code and are
    /// counted in `localCount` — the bundle is their way out.
    /// Resolves `{ code: string, remoteCount: number, localCount: number }`.
    pub async fn build_deck_share_code(
        deck_name: String,
        description: String,
        icon: String,
        members: Vec<String>,
        skill_sources: BTreeMap<String, String>,
    ) -> Result<JsonValue, String> {
        let hub = ss_skills::installed_skill::list_installed_skills()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|skill| (skill.name, skill.git_url))
            .collect::<Vec<(String, String)>>();
        let (entries, local) = split_share_members(&members, &skill_sources, &hub);
        let remote = entries.len();
        let code = encode_share_code(
            ShareCodeKind::Deck,
            &ShareCodePayload {
                n: deck_name,
                d: description,
                i: icon,
                s: entries,
            },
        );
        Ok(outcome(code, remote, local))
    }

    /// Exports the deck as a `.agd` bundle (remote members ship as their
    /// source link, local-only members ship content; missing members are
    /// skipped) and resolves with the written path. `outDir` unset writes
    /// the product location — the download directory, same as the GPUI
    /// sheet; set, it overrides the directory only (the headless tests'
    /// injection point, so a test never writes a real `$HOME`).
    pub async fn export_deck_bundle(
        skill_names: Vec<String>,
        deck_name: String,
        out_dir: Option<String>,
    ) -> Result<String, String> {
        blocking(move || match out_dir {
            Some(dir) => export_to_dir(&skill_names, &deck_name, PathBuf::from(dir)),
            None => ss_skills::skill_bundle::export_deck_bundle(&skill_names, &deck_name)
                .map_err(|error| format!("{error:#}")),
        })
        .await
        .map_err(|join| format!("export task failed: {join}"))
        .and_then(|path| path.map(|path| path.to_string_lossy().to_string()))
    }

    /// Reveals `path` (the exported bundle) in the platform file manager —
    /// Finder / File Explorer, selected, like `os_open::open_folder`.
    /// Resolves once the manager answered; a missing file rejects.
    pub async fn reveal_in_file_manager(path: String) -> Result<(), String> {
        reveal_path(path).await
    }
}

/// The GPUI sheet's member-split decision (`share_sheet.rs:53-88`), pure so
/// the ordering rules are testable without a hub: `skill_sources` wins when
/// non-empty, the hub's git remote is the fallback, and a member with
/// neither is local-only (bundle, not code).
fn split_share_members(
    members: &[String],
    skill_sources: &BTreeMap<String, String>,
    hub: &[(String, String)],
) -> (Vec<ShareCodeSkill>, usize) {
    let mut entries = Vec::new();
    let mut local = 0usize;
    for name in members {
        let source = skill_sources
            .get(name)
            .filter(|url| !url.is_empty())
            .cloned()
            .or_else(|| {
                hub.iter()
                    .find(|(hub_name, url)| hub_name == name && !url.is_empty())
                    .map(|(_, url)| url.clone())
            });
        match source {
            Some(url) => entries.push(ShareCodeSkill {
                n: name.clone(),
                u: url,
                c: None,
                p: None,
            }),
            None => local += 1,
        }
    }
    (entries, local)
}

/// The JSON the TSX sheet renders from, without naming `serde_json` (the
/// bridge manifest stays closed to this slice).
fn outcome(code: String, remote: usize, local: usize) -> JsonValue {
    JsonValue::Object(
        vec![
            ("code".to_owned(), JsonValue::String(code)),
            ("remoteCount".to_owned(), JsonValue::from(remote as u64)),
            ("localCount".to_owned(), JsonValue::from(local as u64)),
        ]
        .into_iter()
        .collect(),
    )
}

/// The explicit-directory branch of the export: the same
/// `<deck>-bundle-<stamp>.agd` name shape as the domain's download-directory
/// writer, with a Unix-millisecond stamp (the bridge keeps off `chrono`).
fn export_to_dir(
    skill_names: &[String],
    deck_name: &str,
    dir: PathBuf,
) -> Result<PathBuf, String> {
    let safe: String = deck_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis())
        .unwrap_or_default();
    let out = dir.join(format!("{safe}-bundle-{stamp}.agd"));
    ss_skills::skill_bundle::export_multi_bundle(skill_names, &out.to_string_lossy())
        .map_err(|error| format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss_skills::share_install::parse_share_code;
    use std::collections::BTreeMap;

    /// The split follows the GPUI sheet's rules exactly: a non-empty
    /// `skill_sources` URL wins, the hub remote is the fallback (an empty
    /// hub URL never carries), and a member with neither is local-only.
    #[test]
    fn the_split_prefers_deck_sources_then_hub_remotes() {
        let members = ["alpha", "beta", "gamma", "delta"].map(String::from);
        let mut sources = BTreeMap::new();
        sources.insert("alpha".to_owned(), "https://github.com/owner/alpha".to_owned());
        // An empty deck source is no source: beta falls through to the hub.
        sources.insert("beta".to_owned(), String::new());
        // gamma's deck source wins over a different hub URL.
        sources.insert(
            "gamma".to_owned(),
            "https://github.com/deck/gamma".to_owned(),
        );
        let hub = vec![
            ("beta".to_owned(), "https://github.com/hub/beta".to_owned()),
            ("gamma".to_owned(), "https://github.com/hub/gamma".to_owned()),
            // delta is in the hub but with an empty git URL: local-only.
            ("delta".to_owned(), String::new()),
        ];
        let (entries, local) = split_share_members(&members, &sources, &hub);
        let urls = entries
            .iter()
            .map(|e| (e.n.as_str(), e.u.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            urls,
            vec![
                ("alpha", "https://github.com/owner/alpha"),
                ("beta", "https://github.com/hub/beta"),
                ("gamma", "https://github.com/deck/gamma"),
            ]
        );
        assert_eq!(local, 1, "delta stays local-only");
    }

    /// The code the facade hands TSX round-trips through the paste-side
    /// parser and carries the deck identity — the same wire the GPUI sheet
    /// emits (`ags-`/`agd-` + base64, 7-day validity enforced by the
    /// reader).
    #[test]
    fn the_deck_code_round_trips_through_the_parser() {
        let (entries, local) = split_share_members(
            &["alpha".to_owned()],
            &{
                let mut map = BTreeMap::new();
                map.insert(
                    "alpha".to_owned(),
                    "https://github.com/owner/alpha".to_owned(),
                );
                map
            },
            &[],
        );
        assert_eq!(local, 0);
        let payload = ShareCodePayload {
            n: "我的卡组".to_owned(),
            d: "deck description".to_owned(),
            i: "sparkle".to_owned(),
            s: entries,
        };
        let code = encode_share_code(ShareCodeKind::Deck, &payload);
        assert!(code.starts_with("agd-"), "a deck code rides the agd- prefix");
        let parsed = parse_share_code(&code).expect("the paste side reads it back");
        assert_eq!(parsed.payload.n, "我的卡组");
        assert_eq!(parsed.payload.s.len(), 1);
        assert_eq!(parsed.payload.s[0].u, "https://github.com/owner/alpha");

        // The outcome shape TSX renders from.
        let value = outcome(code.clone(), 1, 0);
        let JsonValue::Object(fields) = value else {
            panic!("the outcome is an object");
        };
        assert_eq!(fields.get("remoteCount"), Some(&JsonValue::from(1u64)));
        assert!(matches!(fields.get("code"), Some(JsonValue::String(hit)) if hit == &code));
    }

    /// The explicit-directory export writes a real `.agd` under that
    /// directory (missing hub members are skipped, not an error). The lock
    /// read inside `export_multi_bundle` is read-only for the ambient data
    /// dir and every write stays inside the temp directory — this test
    /// never writes a real `$HOME`, and never touches the process
    /// environment (parallel slice tests own the env-var dance).
    #[test]
    fn the_explicit_dir_export_lands_the_agd_there() {
        let base = std::env::temp_dir().join(format!(
            "skillstar-nsh-share-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&base).expect("temp base");
        let out = base.join("out");
        let path = export_to_dir(
            &["no-such-hub-skill".to_owned()],
            "My 测试 Deck",
            out.clone(),
        )
        .expect("missing members are skipped, not fatal");
        let shown = path.to_string_lossy().to_string();
        assert!(shown.starts_with(out.to_string_lossy().as_ref()));
        // The domain's fold: spaces become `-` (letters, digits, `-`, `_`
        // and CJK letters stay), and the `-bundle-<stamp>.agd` tail rides
        // the same shape as the download-directory writer.
        assert!(shown.contains("My-测试-Deck-bundle-"), "name folds to {shown}");
        assert!(shown.ends_with(".agd"));
        assert!(path.exists(), "the bundle is really on disk");
        assert!(path.is_file());
        std::fs::remove_dir_all(&base).ok();
    }
}

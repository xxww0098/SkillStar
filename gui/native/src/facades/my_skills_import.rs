//! Import-dialog slice facade (wave 4) — the `my_skills/import_modal.rs`
//! (+ `import_modal/{phases,select,share,progress,helpers}.rs`) seam: repo
//! scan → skill picker → install → done, the `ags-`/`agd-` share-code
//! preview path, folder adoption and bundle import. The slice worker owns
//! this file and adds `MySkillsImport` methods only — registration is
//! already in `lib.rs`.
//!
//! Bridge contract the slice must follow (see `bridge_task.rs` for the
//! `DomainJob` + `pump_progress` primitives):
//!
//! * **Every git-touching call rides the blocking pool** exactly like the
//!   GPUI `spawn_domain` jobs (import_modal.rs:223-238, 302-313, 420-434,
//!   484-499, 548-568: `tokio::task::spawn_blocking` around
//!   `scan_repo_with_refresh` / `install_from_scan` /
//!   `install_share_and_deck` / `adopt_folder` / `import_bundle_and_deck`)
//!   — here that is `blocking(move || body())` with the body a plain `fn`
//!   so the crate's tests can drive it without a runtime (the
//!   `my_skills_selection.rs` precedent).
//! * **Progress**: build the facade with
//!   `GitSkillFacade::from_file_store_with_progress(sink)` where the sink
//!   sends `ss-git`-shaped progress over a `std::sync::mpsc::Sender`
//!   (the `DialogProgressSink` shape, progress.rs:38-46; the sink is
//!   `Send + Sync`, the sender side of the channel). The engine-thread
//!   task then drains at most one newest event per 120ms tick — the
//!   throttle lives in THIS crate (bridge layer), not in TSX and not in
//!   the domain. The phase/stage → label mapping (`progress_key`,
//!   progress.rs:23-36) belongs to the TSX side (it owns i18n), so the
//!   raw `{phase, stage, skill}` posts and the dialog maps it.
//! * **Cancel**: the task takes `job: Instance<DomainJob>`; the pump
//!   calls `facade.cancel()` on the held clone the moment the shared flag
//!   flips (the git session's own `AtomicBool`, transport.rs
//!   `GitOperationSession::cancel`) — the `cancel_active` counterpart
//!   (import_modal.rs:145-151). TSX cancels by `DomainJob.cancel()`/
//!   `dispose()` on dialog close; the promise may settle later, which the
//!   TS-side epoch gate ignores.
//! * **Cheap local reads** (`repo_history::list_entries`,
//!   `looks_like_share_code`, `parse_share_code`, installed-name diffing)
//!   stay inline — no pool, mirroring which GPUI jobs took
//!   `spawn_blocking` and which did not (history list did, import_modal.rs
//!   :136-141 — here it rides the pool with a plain body for the same
//!   reason: the engine thread never blocks on fs).
//!
//! The scan the picker edits is cached in the facade (the last successful
//! `scan_repo`), exactly the way the GPUI dialog holds `self.scan` in the
//! entity (import_modal.rs:68); `install_skills` re-reads it by the
//! `short` id the TSX hands back, so the wire never round-trips the whole
//! scan.

use std::sync::{Arc, Mutex, Weak, mpsc};
use std::time::Duration;

use nsh_core::JsonValue;
use nsh_core::element::__private::JsonMap;
use nsh_core::file_dialog::OpenDialog;
use nsh_core::prelude::*;
use ss_skills::git::repo_history;
use ss_skills::git::transport::{
    GitOperationPhase, GitOperationProgress, GitProgressSink, InstallStage,
};
use ss_skills::git_skill::GitSkillFacade;
use ss_skills::installed_skill;
use ss_skills::repo_scanner::{DiscoveredSkill, ScanResult, SkillInstallTarget};
use ss_skills::share_install::{self, ShareCodeKind};
use ss_skills::skill_bundle::{self, AnyBundleImport};

use super::bridge_task::{DomainJob, PROGRESS_TICK_MS, drain_latest, job_flag, JobFlag};

// ---- Wire shapes (JsObject) -------------------------------------------------

/// One `DiscoveredSkill` for the picker rows (`repo_scanner::DiscoveredSkill`
/// verbatim; `included_in_bulk` is derived TS-side as `installable && !internal`).
#[derive(Debug, Clone, JsObject)]
pub struct DiscoveredSkillInfo {
    /// Human-readable skill id (the row label and the selection key).
    pub id: String,
    /// Repo-relative folder path (the install target).
    pub folder_path: String,
    /// One-line description under the name.
    pub description: String,
    /// Already present in the hub (the "Installed" badge).
    pub already_installed: bool,
    /// Frontmatter passed the install gate (false blocks the checkbox).
    pub installable: bool,
    /// `metadata.internal` was true — excluded from bulk selection.
    pub internal: bool,
    /// Frontmatter issue slugs (advisory vs blocking, select.rs:559-566).
    pub frontmatter_issues: Vec<String>,
}

/// A finished scan for the picker phase (`ScanResult` minus what only the
/// facade needs). `short` is the id `installSkills` hands back.
#[derive(Debug, Clone, JsObject)]
pub struct ScanInfo {
    /// `owner/repo` (or `local/name`) — the header chip and the install key.
    pub short: String,
    /// Full clone URL (audit-side metadata; unused by the TSX today).
    pub repo_url: String,
    /// The discovered skills, folder-path order.
    pub skills: Vec<DiscoveredSkillInfo>,
    /// The repo declares a Claude Code plugin (the hint banner).
    pub plugin: bool,
    /// The scan came from the cache (the "cached source" row).
    pub cache_hit: bool,
    /// RFC3339 when the cached copy was fetched, if known.
    pub cached_at: Option<String>,
    /// Skill ids preselected when scanning a pasted CLI command (`--skill`, `--all`).
    pub preselected_skills: Vec<String>,
}

/// One install target (the picker's checked rows).
#[derive(Debug, Clone, JsObject)]
pub struct InstallTargetInfo {
    /// Skill id (`DiscoveredSkillInfo.id`).
    pub id: String,
    /// Repo-relative folder path.
    pub folder_path: String,
}

/// The install outcome: what landed, plus the source the Quick Pack step
/// names the deck after (`deck_name_from_source`, helpers.rs:7-16).
#[derive(Debug, Clone, JsObject)]
pub struct InstallOutcome {
    /// Names of the freshly installed skills, install order.
    pub installed: Vec<String>,
    /// The scan's `short` id (Quick Pack's deck-name seed).
    pub short: String,
}

/// One share-code member in the domain's wire field names (`n` name, `u`
/// git URL, `c` inline base64 SKILL.md, `p` needs-auth).
#[derive(Debug, Clone, JsObject)]
pub struct ShareSkillInfo {
    /// Skill name.
    pub n: String,
    /// Git URL when git-backed; may be empty.
    pub u: String,
    /// Inline SKILL.md (Base64 UTF-8), when present.
    pub c: Option<String>,
    /// True when the repo requires auth (private).
    pub p: Option<bool>,
}

/// `parseShareCode`'s payload: flavour plus decoded members.
#[derive(Debug, Clone, JsObject)]
pub struct SharePreviewInfo {
    /// `"skills"` (`ags-`) or `"deck"` (`agd-`).
    pub kind: String,
    /// Deck (or pack) name.
    pub name: String,
    /// Deck description.
    pub description: String,
    /// Deck icon (often an emoji).
    pub icon: String,
    /// The skill entries.
    pub skills: Vec<ShareSkillInfo>,
}

/// A share member that did not install, with the domain's reason slug.
#[derive(Debug, Clone, JsObject)]
pub struct SkippedSkillInfo {
    /// Skill name.
    pub name: String,
    /// Reason slug; unknown slugs render as `install_failed`.
    pub reason: String,
}

/// `install_share`'s reduced outcome (`ImportDone::from_summary` parity).
#[derive(Debug, Clone, JsObject)]
pub struct ShareOutcomeInfo {
    /// Skills the code requested.
    pub requested: usize,
    /// Already present before this run.
    pub existing: Vec<String>,
    /// Newly installed (repo installs and embedded decodes).
    pub installed: Vec<String>,
    /// Skipped rows with reason slugs.
    pub skipped: Vec<SkippedSkillInfo>,
    /// An `agd-` run created the local deck.
    pub deck_created: bool,
}

/// `adopt_folder`'s outcome.
#[derive(Debug, Clone, JsObject)]
pub struct AdoptOutcomeInfo {
    /// Names of the adopted skills.
    pub names: Vec<String>,
}

/// `import_bundle`'s outcome.
#[derive(Debug, Clone, JsObject)]
pub struct BundleOutcomeInfo {
    /// Skills this bundle installed.
    pub installed: Vec<String>,
    /// The deck attach result created a new deck.
    pub deck_created: bool,
}

/// One partner-audit row (`ss_marketplace::SecurityAudit`).
#[derive(Debug, Clone, JsObject)]
pub struct AuditRowInfo {
    /// The audit programme's name.
    pub name: String,
    /// Its verdict string ("critical"/"high"/… — attention gating is TS-side).
    pub result: String,
}

/// Partner audits for one skill (`ss_marketplace::SkillSecurityAudit`) —
/// the select phase's advisory well (import_modal.rs `start_install_audits`).
#[derive(Debug, Clone, JsObject)]
pub struct AuditInfo {
    /// Skill id the rows belong to.
    pub skill: String,
    /// The rows, display order.
    pub audits: Vec<AuditRowInfo>,
}

// ---- Progress plumbing (progress.rs parity) --------------------------------

/// The dialog's sink sends progress while the worker owns the channel.
struct DialogProgressSink {
    tx: Weak<mpsc::Sender<GitOperationProgress>>,
}

impl GitProgressSink for DialogProgressSink {
    fn emit(&self, progress: GitOperationProgress) {
        if let Some(tx) = self.tx.upgrade() {
            let _ = tx.send(progress);
        }
    }
}

/// One git progress event as the TSX maps it (`progress_key`, progress.rs
/// :23-36 — the label choice is the TSX's; the payload stays raw).
pub(super) fn progress_json(progress: &GitOperationProgress) -> JsonValue {
    let phase = match progress.phase {
        GitOperationPhase::Preparing => "preparing",
        GitOperationPhase::Running => "running",
        GitOperationPhase::Completed => "completed",
        GitOperationPhase::Failed => "failed",
        GitOperationPhase::Cancelled => "cancelled",
    };
    let mut pairs = vec![
        ("phase".to_owned(), JsonValue::String(phase.to_owned())),
        (
            "repository".to_owned(),
            JsonValue::String(progress.repository.clone()),
        ),
    ];
    if let Some(stage) = progress.stage {
        let stage = match stage {
            InstallStage::Resolving => "resolving",
            InstallStage::Fetching => "fetching",
            InstallStage::Discovering => "discovering",
            InstallStage::Materializing => "materializing",
            InstallStage::Deploying => "deploying",
        };
        pairs.push(("stage".to_owned(), JsonValue::String(stage.to_owned())));
    }
    if let Some(skill) = &progress.skill {
        pairs.push(("skill".to_owned(), JsonValue::String(skill.clone())));
    }
    JsonValue::Object(pairs.into_iter().collect::<JsonMap<String, JsonValue>>())
}

/// The watcher progress.rs:57-96 is on the GPUI thread; here the same loop
/// runs on the engine thread with one addition: a cancelled flag also fires
/// `facade.cancel()` (the `cancel_active` counterpart, import_modal.rs
/// :145-151) so the blocking git session stops at its next transport
/// callback. A cancelled job consumes nothing further and posts nothing
/// more — events already queued in JS are the TS epoch's to drop.
async fn pump_with_cancel(
    rx: mpsc::Receiver<GitOperationProgress>,
    on_event: Callback<JsonValue>,
    flag: JobFlag,
    facade: GitSkillFacade,
) -> bool {
    loop {
        timer(Duration::from_millis(PROGRESS_TICK_MS)).await;
        if flag.cancelled() {
            facade.cancel();
            return true;
        }
        let drained = drain_latest(&rx);
        if let Some(event) = drained.latest {
            on_event.post(progress_json(&event));
        }
        if drained.disconnected {
            return false;
        }
    }
}

// ---- The scan cache (the dialog's `self.scan`, import_modal.rs:68) --------

/// The last successful scan, held for the install step the same way the
/// GPUI entity holds `self.scan`. One dialog drives one flow at a time.
static LAST_SCAN: Mutex<Option<(String, ScanResult)>> = Mutex::new(None);

fn cache_scan(scan: &ScanResult) {
    match LAST_SCAN.lock() {
        Ok(mut slot) => *slot = Some((scan.spec.short.clone(), scan.clone())),
        Err(poisoned) => *poisoned.into_inner() = Some((scan.spec.short.clone(), scan.clone())),
    }
}

fn take_cached_scan(short: &str) -> Option<ScanResult> {
    fn take(slot: &mut Option<(String, ScanResult)>, short: &str) -> Option<ScanResult> {
        let hit = slot
            .as_ref()
            .and_then(|(cached, scan)| (cached == short).then(|| scan.clone()));
        // The install consumes the scan (import_modal.rs:275-290 reads
        // self.scan once); drop it either way so a second Install cannot
        // reuse a stale tree.
        *slot = None;
        hit
    }
    match LAST_SCAN.lock() {
        Ok(mut slot) => take(&mut slot, short),
        Err(poisoned) => take(&mut poisoned.into_inner(), short),
    }
}

fn skill_info(skill: &DiscoveredSkill) -> DiscoveredSkillInfo {
    DiscoveredSkillInfo {
        id: skill.id.clone(),
        folder_path: skill.folder_path.clone(),
        description: skill.description.clone(),
        already_installed: skill.already_installed,
        installable: skill.installable,
        internal: skill.internal,
        frontmatter_issues: skill.frontmatter_issues.clone(),
    }
}

fn scan_info(scan: &ScanResult, preselected_skills: Vec<String>) -> ScanInfo {
    ScanInfo {
        short: scan.spec.short.clone(),
        repo_url: scan.spec.repo_url.clone(),
        skills: scan.skills.iter().map(skill_info).collect(),
        plugin: scan.plugin.is_some(),
        cache_hit: scan.cache_hit,
        cached_at: scan.cached_at.clone(),
        preselected_skills,
    }
}

// ---- Blocking bodies (plain fns — crate tests drive these) ------------------

/// Resolve preselected skill ids from a pasted install command (`--skill` or `--all`).
fn resolve_preselected_skills(
    parsed: &ss_skills::source_resolver::ParsedInstallCommand,
    discovered: &[DiscoveredSkill],
) -> Vec<String> {
    if parsed.all {
        return discovered
            .iter()
            .filter(|skill| skill.installable && !skill.internal)
            .map(|skill| skill.id.clone())
            .collect();
    }
    let mut selected = Vec::new();
    for requested in &parsed.skills {
        if let Some(found) = discovered
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(requested) && skill.installable)
            && !selected.contains(&found.id)
        {
            selected.push(found.id.clone());
        }
    }
    selected
}

/// `scan_repo_with_refresh` + pasted command extraction + history upsert.
pub(super) fn scan_with_command(
    facade: &GitSkillFacade,
    input: &str,
    full_depth: bool,
    refresh: bool,
) -> Result<(ScanResult, Vec<String>), String> {
    let parsed =
        ss_skills::source_resolver::parse_install_input(input).map_err(|err| format!("{err:#}"))?;
    let scan = facade
        .scan_repo_with_refresh(&parsed.source, full_depth, refresh)
        .map_err(|err| format!("{err:#}"))?;
    // History writes never fail the scan itself (import_modal.rs:230-233).
    let _ = repo_history::upsert_entry(&scan.spec.short, &scan.spec.repo_url);
    let preselected = resolve_preselected_skills(&parsed, &scan.skills);
    Ok((scan, preselected))
}

/// `install_from_scan` (import_modal.rs:306-313).
pub(super) fn install_body(
    facade: &GitSkillFacade,
    scan: &ScanResult,
    targets: &[SkillInstallTarget],
) -> Result<Vec<String>, String> {
    facade
        .install_from_scan(scan, targets)
        .map_err(|err| format!("{err:#}"))
}

/// `install_share_and_deck` (import_modal.rs:424-434).
fn install_share_body(
    deck: bool,
    name: &str,
    description: String,
    icon: String,
    skills: Vec<ss_skills::share_install::ShareCodeSkill>,
) -> Result<(share_install::ShareCodeInstallSummary, bool), String> {
    let outcome =
        share_install::install_share_and_deck(deck, name, description, icon, skills)
            .map_err(|err| format!("{err:#}"))?;
    Ok((
        outcome.summary,
        outcome.deck == ss_skills::skill_group::DeckAttach::Created,
    ))
}

/// `adopt_folder` (import_modal.rs:488-499).
fn adopt_folder_body(path: &str) -> Result<Vec<String>, String> {
    ss_skills::local_skill::adopt_folder(path, None)
        .map(|result| result.adopted.into_iter().map(|skill| skill.name).collect())
        .map_err(|err| format!("{err:#}"))
}

/// `import_bundle_and_deck` (import_modal.rs:552-568). The count label is
/// pre-rendered TS copy with a `{{count}}` hole (import_modal.rs:557-563).
fn import_bundle_body(
    path: &str,
    fallback_name: &str,
    count_label: &str,
) -> Result<(Vec<String>, bool), String> {
    let outcome = skill_bundle::import_bundle_and_deck(path, fallback_name, |count| {
        count_label.replace("{{count}}", &count.to_string())
    })
    .map_err(|err| format!("{err:#}"))?;
    let names = match outcome.import {
        AnyBundleImport::Single(result) => vec![result.name],
        AnyBundleImport::Multi(result) => result.skill_names,
    };
    Ok((
        names,
        outcome.deck == ss_skills::skill_group::DeckAttach::Created,
    ))
}

// ---- The module --------------------------------------------------------------

/// The import-dialog seam (`MySkillsImport` in TSX): repo scan → picker →
/// install → done, `ags-`/`agd-` share codes, folder adoption and bundle
/// import.
#[module]
pub struct MySkillsImport;

#[module]
impl MySkillsImport {
    /// Cheap input sniff for share codes: a bare `ags-`/`agd-` token of
    /// real code length, or one embedded in a longer pasted message
    /// (`looks_like_share_code`). Drives the live detected banner; the
    /// authoritative check stays `parseShareCode`. Pure, no IO.
    pub fn looks_like_share_code(text: String) -> bool {
        share_install::looks_like_share_code(&text)
    }

    /// Decode an `ags-`/`agd-` share code into its flavour and payload
    /// (import_modal.rs:360-400's `parse_share`). Pure; an invalid,
    /// corrupted or expired code rejects with the domain's reason — the
    /// preview phase's error block renders it verbatim.
    pub fn parse_share_code(text: String) -> Result<SharePreviewInfo, String> {
        let parsed = share_install::parse_share_code(&text)?;
        let kind = match parsed.kind {
            ShareCodeKind::Skills => "skills",
            ShareCodeKind::Deck => "deck",
        };
        Ok(SharePreviewInfo {
            kind: kind.to_owned(),
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

    /// Lowercased names of the installed skills (the share preview's
    /// "already had" diff, import_modal.rs:370-390). The installed listing
    /// is the domain's cached async read — awaited inline, no pool
    /// (`list_installed_skills` is not a blocking-pool job on the GPUI
    /// side either). A listing failure reads as no skills.
    pub async fn installed_names() -> Vec<String> {
        installed_skill::list_installed_skills()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|skill| skill.name.trim().to_lowercase())
            .collect()
    }

    /// Recent repo sources, newest first (`repo_history::list_entries`,
    /// import_modal.rs:136-141 rode `spawn_blocking`; same pool here).
    pub async fn repo_history() -> Vec<String> {
        blocking(repo_history::list_entries)
            .await
            .map(|entries| entries.into_iter().map(|entry| entry.source).collect())
            .unwrap_or_default()
    }

    /// Scan a repo URL / `owner/repo` / local path (`handleScan` +
    /// `handleDeepScan` + `refreshUpstream` share this). The git work runs
    /// on the blocking pool; progress events post through the 120ms
    /// newest-only pump; a cancelled job also fires `facade.cancel()` so
    /// the transport stops mid-fetch. Resolves with the picker's scan;
    /// rejects with the raw domain chain (`{err:#}`) for the TSX's
    /// `git_error_message` mapping.
    pub async fn scan_repo(
        input: String,
        full_depth: bool,
        refresh: bool,
        job: Instance<DomainJob>,
        #[args(event)] on_event: Callback<JsonValue>,
    ) -> Result<ScanInfo, String> {
        let flag = job_flag(&job);
        let (tx, rx) = mpsc::channel::<GitOperationProgress>();
        let tx = Arc::new(tx);
        let facade = GitSkillFacade::from_file_store_with_progress(Arc::new(DialogProgressSink {
            tx: Arc::downgrade(&tx),
        }));
        let worker_facade = facade.clone();
        let worker_input = input.clone();
        let handle = blocking(move || {
            // Only the worker keeps the channel open; the pump retains the facade for cancellation.
            let _progress_sender = tx;
            scan_with_command(&worker_facade, &worker_input, full_depth, refresh)
        });
        let cancelled = pump_with_cancel(rx, on_event.clone(), flag.clone(), facade.clone()).await;
        let (scan, preselected) = handle
            .await
            .map_err(|join| format!("scan job failed: {join}"))??;
        if cancelled || flag.cancelled() {
            // The dialog is gone; the epoch gate drops this settle. Cancelling
            // mid-fetch leaves the checkout partial, so do not cache it.
            facade.cancel();
            return Err("cancelled:".to_owned());
        }
        cache_scan(&scan);
        Ok(scan_info(&scan, preselected))
    }

    /// Install the picker's checked rows from the cached scan
    /// (`handleInstall`; the Quick Pack continuation reads
    /// `outcome.short` for the deck name). Blocking pool + progress pump +
    /// cancel, exactly `scan_repo`.
    pub async fn install_skills(
        short: String,
        targets: Vec<InstallTargetInfo>,
        job: Instance<DomainJob>,
        #[args(event)] on_event: Callback<JsonValue>,
    ) -> Result<InstallOutcome, String> {
        let Some(scan) = take_cached_scan(&short) else {
            return Err("no-scan:".to_owned());
        };
        let flag = job_flag(&job);
        let (tx, rx) = mpsc::channel::<GitOperationProgress>();
        let tx = Arc::new(tx);
        let facade = GitSkillFacade::from_file_store_with_progress(Arc::new(DialogProgressSink {
            tx: Arc::downgrade(&tx),
        }));
        let worker_facade = facade.clone();
        let install_targets: Vec<SkillInstallTarget> = targets
            .into_iter()
            .map(|target| SkillInstallTarget {
                id: target.id,
                folder_path: target.folder_path,
                pinned: false,
            })
            .collect();
        let handle = blocking(move || {
            let _progress_sender = tx;
            install_body(&worker_facade, &scan, &install_targets)
        });
        let cancelled = pump_with_cancel(rx, on_event.clone(), flag.clone(), facade.clone()).await;
        let installed = handle
            .await
            .map_err(|join| format!("install job failed: {join}"))??;
        if cancelled || flag.cancelled() {
            return Err("cancelled:".to_owned());
        }
        Ok(InstallOutcome {
            installed,
            short,
        })
    }

    /// Install a parsed share code's members and create the local deck for
    /// `agd-` flavours (`handleShareCodeInstall`, import_modal.rs:404-454).
    /// Runs on the blocking pool; the domain install loop itself is not
    /// interruptible mid-skill, so the job flag gates the outcome instead
    /// (a cancelled dialog drops the settle via the epoch gate).
    pub async fn install_share(
        preview: SharePreviewInfo,
        job: Instance<DomainJob>,
    ) -> Result<ShareOutcomeInfo, String> {
        let flag = job_flag(&job);
        let deck = preview.kind == "deck";
        let name = preview.name;
        let description = preview.description;
        let icon = preview.icon;
        let skills: Vec<ss_skills::share_install::ShareCodeSkill> = preview
            .skills
            .into_iter()
            .map(|skill| ss_skills::share_install::ShareCodeSkill {
                n: skill.n,
                u: skill.u,
                c: skill.c,
                p: skill.p,
            })
            .collect();
        let joined = blocking(move || install_share_body(deck, &name, description, icon, skills))
            .await
            .map_err(|join| format!("share install job failed: {join}"))??;
        if flag.cancelled() {
            return Err("cancelled:".to_owned());
        }
        let (summary, deck_created) = joined;
        let installed: Vec<String> = summary
            .installed_names
            .iter()
            .chain(summary.embedded_names.iter())
            .cloned()
            .collect();
        Ok(ShareOutcomeInfo {
            requested: summary.requested_count,
            existing: summary.existing_names,
            installed,
            skipped: summary
                .skipped
                .into_iter()
                .map(|entry| SkippedSkillInfo {
                    name: entry.name,
                    reason: entry.reason,
                })
                .collect(),
            deck_created,
        })
    }

    /// Adopt a local folder of skills (`handlePickLocalFolder` →
    /// `adopt_folder`, import_modal.rs:479-513). Blocking pool.
    pub async fn adopt_folder(
        path: String,
        job: Instance<DomainJob>,
    ) -> Result<AdoptOutcomeInfo, String> {
        let flag = job_flag(&job);
        let names = blocking(move || adopt_folder_body(&path))
            .await
            .map_err(|join| format!("adopt job failed: {join}"))??;
        if flag.cancelled() {
            return Err("cancelled:".to_owned());
        }
        Ok(AdoptOutcomeInfo { names })
    }

    /// Import a local `.ags`/`.agd` bundle (`onPickLocalFile` →
    /// `import_bundle_and_deck`, import_modal.rs:543-595). `fallback_name`
    /// is the deck name when the file carries none; `count_label` is a
    /// `{{count}}` template for the deck description. Blocking pool.
    pub async fn import_bundle(
        path: String,
        fallback_name: String,
        count_label: String,
        job: Instance<DomainJob>,
    ) -> Result<BundleOutcomeInfo, String> {
        let flag = job_flag(&job);
        let (installed, deck_created) = blocking(move || {
            import_bundle_body(&path, &fallback_name, &count_label)
        })
        .await
        .map_err(|join| format!("bundle job failed: {join}"))??;
        if flag.cancelled() {
            return Err("cancelled:".to_owned());
        }
        Ok(BundleOutcomeInfo {
            installed,
            deck_created,
        })
    }

    /// Partner security audits for the select phase's advisory well
    /// (import_modal.rs `start_install_audits`:597-636 — rows only for a
    /// confirmed-public github.com repo, empty for every other source;
    /// absence is not a pass, the TSX well hides on empty). Async HTTP —
    /// awaited inline, no pool (the GPUI job was a plain `spawn_domain`
    /// future, import_modal.rs:622-635). A failed lookup is an empty list.
    pub async fn install_audits(
        repo_url: String,
        skill_names: Vec<String>,
    ) -> Vec<AuditInfo> {
        if !ss_marketplace::may_query_install_audits(&repo_url) {
            return Vec::new();
        }
        ss_marketplace::fetch_public_github_skill_audits(&repo_url, &skill_names)
            .await
            .iter()
            .map(|audit| AuditInfo {
                skill: audit.skill.clone(),
                audits: audit
                    .audits
                    .iter()
                    .map(|row| AuditRowInfo {
                        name: row.name.clone(),
                        result: row.result.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    /// System open dialog for one `.ags`/`.agd` file (`pick_bundle`,
    /// import_modal.rs:518-538). Resolves with the picked path, or `null`
    /// on cancel / no platform dialog — both leave the dialog where it is.
    pub async fn pick_bundle_file(prompt: String) -> Option<String> {
        let dialog = OpenDialog::files().title(prompt);
        match pick_paths(dialog).await {
            Ok(Some(paths)) => paths
                .into_iter()
                .next()
                .map(|path| path.to_string_lossy().into_owned()),
            _ => None,
        }
    }

    /// System dialog for one directory of skills (`pick_folder` →
    /// `adopt_folder`, import_modal.rs:457-477). Same null semantics.
    pub async fn pick_folder(prompt: String) -> Option<String> {
        let dialog = OpenDialog::folders().title(prompt);
        match pick_paths(dialog).await {
            Ok(Some(paths)) => paths
                .into_iter()
                .next()
                .map(|path| path.to_string_lossy().into_owned()),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "my_skills_import_tests.rs"]
mod tests;

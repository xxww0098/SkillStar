//! Skill-shipped agent hooks — the second payload a skill can carry.
//!
//! Everything else this crate deploys is inert Markdown: it sits in a skills
//! directory until a model chooses to read it. A hook is the opposite — it is a
//! command the agent runs on its own schedule, on every matching turn. The two
//! travel through the same install pipe and have nothing else in common, which
//! is why hooks get their own module instead of another branch inside
//! [`crate::deployment`].
//!
//! ## One payload, no translation
//!
//! A skill ships `hooks/hooks.json` in the Claude Code hook format. That format
//! is written to every target verbatim, and that is a measured fact rather than
//! a shortcut: Claude Code's `~/.claude/settings.json` `hooks` object and
//! Codex's `~/.codex/hooks.json` `hooks` object use the same PascalCase event
//! names (`PreToolUse`, `SessionStart`, …) and the same entry shape
//! (`{matcher?, hooks: [{type, command, timeout?}]}`). Only the file differs, so
//! the file *is* the registry — see [`HOOK_TARGETS`].
//!
//! The snake_case spellings (`pre_tool_use`) that also appear in Codex's
//! `config.toml` are trust-ledger keys, not event names, and must not be
//! confused for a second dialect worth translating to.
//!
//! ## Writing offers a hook, it never arms one
//!
//! Codex keys a `trusted_hash` per entry under `[hooks.state]` in
//! `~/.codex/config.toml` and refuses to run an entry it has not been told to
//! trust; bypassing that needs an explicit `--dangerously-bypass-hook-trust`.
//! So a write here is a proposal the user still has to accept inside the agent.
//! Callers must keep it that way — a hook must never ride along silently with a
//! skill install the way a Markdown file can.
//!
//! ## Ownership is derived, never tracked
//!
//! No sidecar ledger records what was written. A command is required to name
//! the skill's own directory (via the `${SKILL_DIR}` placeholder), so
//! "the command mentions this skill's path" is the whole ownership rule — the
//! same zero-state derivation `decisions.md` D-024 chose for deployed skills,
//! for the same reason: a second record of what is on disk can drift from the
//! disk.

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// The key both agents nest their event map under.
const HOOKS_KEY: &str = "hooks";

/// Where a skill declares its hooks, relative to the skill directory.
const SKILL_HOOK_DOC: [&str; 2] = ["hooks", "hooks.json"];

/// Placeholders expanded to the skill's absolute directory.
///
/// `${CLAUDE_PLUGIN_ROOT}` is the Claude Code plugin ecosystem's own spelling
/// and is accepted so a repo that already ships plugin hooks needs no edit;
/// `${SKILL_DIR}` is the neutral name for everything else.
const SKILL_DIR_PLACEHOLDERS: [&str; 2] = ["${SKILL_DIR}", "${CLAUDE_PLUGIN_ROOT}"];

/// One agent that can run skill-shipped hooks.
///
/// This table is deliberately short. An agent earns a row only once its hook
/// file has been read on disk and its event map confirmed to use the format
/// above — a row added on the strength of a vendor doc would write hooks that
/// never fire, which is worse than not writing them.
struct HookTarget {
    /// `AgentProfile.id` from `skillstar-agents` (`claude`, not `claude-code`).
    agent_id: &'static str,
    /// Home-relative config file holding the event map. Claude's file also
    /// holds unrelated settings, so the document is always merged, never
    /// replaced; Codex's file happens to need the same treatment, which is why
    /// one code path serves both.
    rel_path: &'static [&'static str],
}

static HOOK_TARGETS: &[HookTarget] = &[
    HookTarget {
        agent_id: "claude",
        rel_path: &[".claude", "settings.json"],
    },
    HookTarget {
        agent_id: "codex",
        rel_path: &[".codex", "hooks.json"],
    },
];

impl HookTarget {
    fn path(&self, home: &Path) -> PathBuf {
        self.rel_path.iter().fold(home.to_path_buf(), |acc, s| acc.join(s))
    }
}

fn hook_target(agent_id: &str) -> Option<&'static HookTarget> {
    HOOK_TARGETS.iter().find(|target| target.agent_id == agent_id)
}

/// Agents this module can write hooks for, in table order.
pub fn hook_capable_agents() -> Vec<&'static str> {
    HOOK_TARGETS.iter().map(|target| target.agent_id).collect()
}

/// What one sync wrote, for the caller to show the user.
///
/// `events` is what actually reached the file. It is reported rather than
/// silently assumed because an agent quietly ignores an event key it does not
/// know, and a hook that never fires must not look like a hook that installed.
///
/// The event sets are *not* identical across targets — Claude Code dispatches
/// `PostToolUseFailure`, `StopFailure` and `TeammateIdle`, and none of the
/// three appears in Codex's hook-migration event list. There is deliberately no
/// per-target allowlist to filter against: Codex's published list also omits
/// `Stop`, which its own trust ledger proves it runs, so an allowlist built
/// from it would reject working hooks. Absence of evidence is not evidence of
/// absence, and a false rejection is worse than a reported no-op — hence the
/// honest report instead of a guessed filter.
#[derive(Debug, Clone, Default)]
pub struct HookSyncReport {
    pub agent_id: String,
    pub file: PathBuf,
    pub events: Vec<String>,
    pub entries: usize,
}

/// The hook document a skill ships, or `None` when it ships none.
pub fn skill_hook_doc(skill_dir: &Path) -> Result<Option<Map<String, Value>>> {
    let path = SKILL_HOOK_DOC
        .iter()
        .fold(skill_dir.to_path_buf(), |acc, s| acc.join(s));
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let doc: Value = serde_json::from_str(&raw)
        .with_context(|| format!("invalid hook document: {}", path.display()))?;
    let events = doc
        .get(HOOKS_KEY)
        .and_then(Value::as_object)
        .cloned()
        .with_context(|| format!("{} has no `{HOOKS_KEY}` object", path.display()))?;
    Ok(Some(events))
}

/// Whether `command` names `skill_dir` as a whole path rather than a prefix.
///
/// A bare substring test is wrong: sibling skills share a parent, so
/// `…/skills/foo` occurs inside `…/skills/foobar/run.sh` and would let one
/// skill claim — and on uninstall delete — its neighbour's hooks. The match
/// therefore only counts when a path separator (or the end of the command)
/// follows it.
///
/// The test is deliberately "is a separator" rather than "cannot continue a
/// file name": almost every punctuation mark is legal in a POSIX file name, so
/// an exclusion list is a list of the characters someone happened to think of.
/// `…/skills/foo+bar/run` is a different directory and a `+` is not special —
/// only a separator ends a path component.
///
/// One case this cannot separate: a skill nested *inside* another
/// (`…/skills/foo` and `…/skills/foo/bar`), where the outer path is a genuine
/// path prefix of the inner one and the outer skill would claim the inner
/// skill's entries. Telling them apart needs the set of every other skill
/// directory, which is exactly the second source of truth this module refuses
/// to keep. It stays safe because the hub is flat — `hub_skills_dir()/<name>`,
/// one level, no skill inside a skill — so callers must pass a hub-rooted
/// skill directory rather than an arbitrary nested path.
///
/// Note that a command which *fails* this test is rejected by
/// [`resolve_entries`] before it is ever written, and both sites call this same
/// function: anything on disk was recognisable when written and stays
/// recognisable now, so no entry can become an unremovable orphan.
fn command_names_dir(command: &str, skill_dir: &str) -> bool {
    command.match_indices(skill_dir).any(|(index, _)| {
        matches!(
            command[index + skill_dir.len()..].chars().next(),
            None | Some('/') | Some('\\')
        )
    })
}

/// The skill directory as it is matched inside a command.
///
/// Trailing separators are stripped so a caller that says `…/skills/foo` on
/// sync and `…/skills/foo/` on removal still recognises its own entries;
/// without this the second spelling matches nothing and every entry the first
/// one wrote becomes unremovable.
fn match_key(skill_dir: &Path) -> String {
    skill_dir
        .to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .to_string()
}

/// Whether `entry` was written for the skill living at `skill_dir`.
fn entry_owned_by(entry: &Value, skill_dir: &str) -> bool {
    entry
        .get(HOOKS_KEY)
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|command| command_names_dir(command, skill_dir))
            })
        })
}

/// Expand `${SKILL_DIR}` in every command, and reject entries that end up not
/// naming the skill directory.
///
/// The rejection is the price of holding no ownership state: an entry whose
/// commands never mention the skill could be written but never found again, so
/// a later uninstall would leave a live hook behind pointing at deleted files.
/// Failing at write time is the recoverable end of that trade.
fn resolve_entries(entries: &[Value], skill_dir: &str) -> Result<Vec<Value>> {
    let mut resolved = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut entry = entry.clone();
        let hooks = entry
            .get_mut(HOOKS_KEY)
            .and_then(Value::as_array_mut)
            .with_context(|| format!("hook entry has no `{HOOKS_KEY}` array"))?;
        let mut owning_commands = 0usize;
        for hook in hooks.iter_mut() {
            let Some(command) = hook.get("command").and_then(Value::as_str) else {
                continue;
            };
            let expanded = SKILL_DIR_PLACEHOLDERS
                .iter()
                .fold(command.to_string(), |acc, ph| acc.replace(ph, skill_dir));
            if !command_names_dir(&expanded, skill_dir) {
                bail!(
                    "hook command does not reference the skill directory, so it could \
                     never be removed again; write it as ${{SKILL_DIR}}/... (a separator \
                     must follow the placeholder) instead: {command}"
                );
            }
            hook["command"] = Value::String(expanded);
            owning_commands += 1;
        }
        // An entry whose handlers carry no command — an empty `hooks` array, or
        // only non-command handler types — passes the loop above without ever
        // naming the skill, so it would be written and then never recognised
        // again. Refusing it keeps "written implies removable" true.
        if owning_commands == 0 {
            bail!(
                "hook entry declares no command naming the skill directory, so it could \
                 never be removed again"
            );
        }
        resolved.push(entry);
    }
    Ok(resolved)
}

fn read_doc(path: &Path) -> Result<Map<String, Value>> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Ok(Map::new());
    };
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    serde_json::from_str(&raw).with_context(|| format!("invalid JSON: {}", path.display()))
}

fn write_doc(path: &Path, doc: &Map<String, Value>) -> Result<()> {
    let mut content = serde_json::to_string_pretty(doc)?;
    content.push('\n');
    skillstar_core::infra::fs_ops::atomic_write(path, content.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Splice this skill's entries into `slot`, keeping foreign entries put.
///
/// Codex keys hook trust by `<file>:<event>:<index>:<index>`, so moving a
/// neighbouring entry silently invalidates its `trusted_hash` and re-prompts
/// the user for a hook they already vetted. Overwriting in place avoids that
/// for the common re-sync, where the skill declares the same number of entries
/// it did last time.
///
/// ponytail: only the equal-count case is shift-free — adding or removing an
/// entry still renumbers every foreign entry after it. Codex exposes no way to
/// rekey its ledger, so the fix would have to come from upstream; revisit if
/// users report re-trust prompts.
fn splice_entries(slot: &mut Vec<Value>, resolved: Vec<Value>, skill_dir: &str) {
    let owned: Vec<usize> = slot
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry_owned_by(entry, skill_dir))
        .map(|(index, _)| index)
        .collect();

    let mut resolved = resolved.into_iter();
    let mut reused = 0usize;
    for &index in &owned {
        let Some(entry) = resolved.next() else { break };
        slot[index] = entry;
        reused += 1;
    }
    slot.extend(resolved);

    // Stale entries from a shrunken declaration; removing them is the one case
    // that renumbers, and leaving them would keep dead hooks running.
    for &index in owned[reused..].iter().rev() {
        slot.remove(index);
    }
}

/// Write `skill_dir`'s hooks into `agent_id`'s config under `home`.
///
/// Idempotent: re-syncing the same skill replaces its own entries and leaves
/// every other entry in the file untouched.
pub fn sync_skill_hooks(home: &Path, skill_dir: &Path, agent_id: &str) -> Result<HookSyncReport> {
    let target = hook_target(agent_id)
        .with_context(|| format!("agent '{agent_id}' has no known hook file"))?;
    let path = target.path(home);
    let mut report = HookSyncReport {
        agent_id: agent_id.to_string(),
        file: path.clone(),
        ..Default::default()
    };

    let Some(declared) = skill_hook_doc(skill_dir)? else {
        return Ok(report);
    };

    // Fail closed rather than provision an agent the user does not have: the
    // parent directory is the agent's own, and creating it would make SkillStar
    // look like an installed Codex or Claude to everything that probes for one.
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    if !parent.is_dir() {
        bail!(
            "agent '{agent_id}' is not set up on this machine ({} does not exist)",
            parent.display()
        );
    }

    let skill_dir_str = match_key(skill_dir);
    let mut doc = read_doc(&path)?;
    let events = doc
        .entry(HOOKS_KEY.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .with_context(|| format!("`{HOOKS_KEY}` in {} is not an object", path.display()))?;

    for (event, declared_entries) in &declared {
        let declared_entries = declared_entries
            .as_array()
            .with_context(|| format!("event `{event}` is not an array"))?;
        // An event declared with no entries must not materialise the key:
        // nothing would own it, so removal could never prune it again.
        if declared_entries.is_empty() {
            continue;
        }
        let resolved = resolve_entries(declared_entries, &skill_dir_str)?;
        report.entries += resolved.len();

        let slot = events
            .entry(event.clone())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .with_context(|| format!("event `{event}` in {} is not an array", path.display()))?;
        splice_entries(slot, resolved, &skill_dir_str);
        report.events.push(event.clone());
    }

    write_doc(&path, &doc)?;
    Ok(report)
}

/// Remove every entry belonging to `skill_dir` from `agent_id`'s config.
///
/// Returns how many entries were removed. A missing file is not an error —
/// uninstall must stay callable for an agent that was never synced.
pub fn remove_skill_hooks(home: &Path, skill_dir: &Path, agent_id: &str) -> Result<usize> {
    let target = hook_target(agent_id)
        .with_context(|| format!("agent '{agent_id}' has no known hook file"))?;
    let path = target.path(home);
    if !path.exists() {
        return Ok(0);
    }

    let skill_dir_str = match_key(skill_dir);
    let mut doc = read_doc(&path)?;
    let Some(events) = doc.get_mut(HOOKS_KEY).and_then(Value::as_object_mut) else {
        return Ok(0);
    };

    let mut removed = 0usize;
    // Only keys this removal emptied are pruned. An event key that was already
    // empty belongs to whoever wrote it, and deleting it because it "looks
    // unused" is the same guess D-024 removed from the deployment paths.
    let mut emptied = Vec::new();
    for (event, entries) in events.iter_mut() {
        let Some(slot) = entries.as_array_mut() else {
            continue;
        };
        let before = slot.len();
        slot.retain(|entry| !entry_owned_by(entry, &skill_dir_str));
        removed += before - slot.len();
        if before > slot.len() && slot.is_empty() {
            emptied.push(event.clone());
        }
    }
    for event in emptied {
        events.remove(&event);
    }

    if removed > 0 {
        write_doc(&path, &doc)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    /// A skill directory shipping `hooks/hooks.json` with `commands`.
    fn skill_with_hooks(root: &Path, name: &str, doc: Value) -> PathBuf {
        let skill_dir = root.join(name);
        std::fs::create_dir_all(skill_dir.join("hooks")).unwrap();
        std::fs::write(
            skill_dir.join("hooks").join("hooks.json"),
            serde_json::to_string(&doc).unwrap(),
        )
        .unwrap();
        skill_dir
    }

    fn one_hook(event: &str, command: &str) -> Value {
        json!({ "hooks": { event: [ { "hooks": [ { "type": "command", "command": command } ] } ] } })
    }

    /// `~/.codex` present so the fail-closed provisioning guard passes.
    fn home_with_codex(root: &Path) -> PathBuf {
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        home
    }

    fn events_of(home: &Path, agent: &str) -> Map<String, Value> {
        let path = hook_target(agent).unwrap().path(home);
        read_doc(&path)
            .unwrap()
            .get(HOOKS_KEY)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    #[test]
    fn writes_the_skill_hook_and_expands_the_placeholder() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            one_hook("PreToolUse", "${SKILL_DIR}/scripts/check.sh"),
        );

        let report = sync_skill_hooks(&home, &skill, "codex").unwrap();
        assert_eq!(report.events, ["PreToolUse"]);
        assert_eq!(report.entries, 1);

        let command = events_of(&home, "codex")["PreToolUse"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            command,
            format!("{}/scripts/check.sh", skill.to_string_lossy()),
            "the placeholder must resolve to the installed skill directory"
        );
    }

    #[test]
    fn rejects_a_command_that_cannot_be_traced_back() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let skill = skill_with_hooks(temp.path(), "demo", one_hook("PreToolUse", "npx impeccable"));

        let err = sync_skill_hooks(&home, &skill, "codex").unwrap_err();
        assert!(
            err.to_string().contains("${SKILL_DIR}"),
            "the error must name the fix, got: {err}"
        );
        assert!(
            !hook_target("codex").unwrap().path(&home).exists(),
            "a rejected sync must not leave a half-written hook file"
        );
    }

    #[test]
    fn resync_is_idempotent_and_leaves_foreign_entries_in_place() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            one_hook("PreToolUse", "${SKILL_DIR}/check.sh"),
        );

        // A hook the user (or another tool) already owns, written first so ours
        // lands after it.
        let path = hook_target("codex").unwrap().path(&home);
        write_doc(
            &path,
            json!({ "hooks": { "PreToolUse": [ { "hooks": [ { "type": "command", "command": "mine" } ] } ] } })
                .as_object()
                .unwrap(),
        )
        .unwrap();

        sync_skill_hooks(&home, &skill, "codex").unwrap();
        let after_first = events_of(&home, "codex");
        sync_skill_hooks(&home, &skill, "codex").unwrap();
        let after_second = events_of(&home, "codex");

        assert_eq!(after_first, after_second, "re-sync must not accumulate");
        let entries = after_second["PreToolUse"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            entries[0]["hooks"][0]["command"], "mine",
            "the foreign entry must keep index 0 so its trusted_hash stays valid"
        );
    }

    #[test]
    fn removal_takes_only_this_skill() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let mine = skill_with_hooks(
            temp.path(),
            "mine",
            one_hook("PreToolUse", "${SKILL_DIR}/a.sh"),
        );
        let other = skill_with_hooks(
            temp.path(),
            "other",
            one_hook("PreToolUse", "${SKILL_DIR}/b.sh"),
        );
        sync_skill_hooks(&home, &mine, "codex").unwrap();
        sync_skill_hooks(&home, &other, "codex").unwrap();

        assert_eq!(remove_skill_hooks(&home, &mine, "codex").unwrap(), 1);

        let entries = events_of(&home, "codex")["PreToolUse"].as_array().unwrap().clone();
        assert_eq!(entries.len(), 1);
        assert!(
            entries[0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("other"),
            "the other skill's hook must survive"
        );
    }

    #[test]
    fn a_sibling_skill_sharing_a_name_prefix_is_not_claimed() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        // `foo` is a prefix of `foobar`; a substring test would let `foo` own —
        // and on uninstall delete — `foobar`'s hook.
        let foo = skill_with_hooks(temp.path(), "foo", one_hook("PreToolUse", "${SKILL_DIR}/a.sh"));
        let foobar = skill_with_hooks(
            temp.path(),
            "foobar",
            one_hook("PreToolUse", "${SKILL_DIR}/b.sh"),
        );
        sync_skill_hooks(&home, &foobar, "codex").unwrap();
        sync_skill_hooks(&home, &foo, "codex").unwrap();

        let entries = events_of(&home, "codex")["PreToolUse"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(entries.len(), 2, "foo must not overwrite foobar's entry");

        assert_eq!(remove_skill_hooks(&home, &foo, "codex").unwrap(), 1);
        let left = events_of(&home, "codex")["PreToolUse"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(left.len(), 1);
        assert!(
            left[0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("foobar"),
            "removing foo must leave foobar's hook alone"
        );
    }

    #[test]
    fn a_neighbour_whose_name_merely_starts_the_same_is_not_claimed() {
        // `/s/foo` occurs in `/s/foo+bar/run`, and `+` is a perfectly legal
        // file-name character — only a separator ends a path component.
        assert!(!command_names_dir("/s/foo+bar/run", "/s/foo"));
        assert!(!command_names_dir("/s/foobar/run", "/s/foo"));
        assert!(command_names_dir("/s/foo/run", "/s/foo"));
        assert!(command_names_dir("/s/foo", "/s/foo"));
        assert!(command_names_dir("cmd /s/foo\\run.cmd", "/s/foo"));
    }

    #[test]
    fn a_trailing_separator_still_finds_the_same_entries() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            one_hook("PreToolUse", "${SKILL_DIR}/a.sh"),
        );
        sync_skill_hooks(&home, &skill, "codex").unwrap();

        // The same directory, spelled with a trailing separator.
        let with_slash = PathBuf::from(format!("{}/", skill.to_string_lossy()));
        assert_eq!(
            remove_skill_hooks(&home, &with_slash, "codex").unwrap(),
            1,
            "a trailing separator must not orphan the entry"
        );
    }

    #[test]
    fn an_entry_with_no_owning_command_is_refused() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        // Handlers with no `command` never name the skill, so the entry could
        // be written and then never recognised again.
        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            json!({ "hooks": { "PreToolUse": [ { "hooks": [] } ] } }),
        );

        assert!(sync_skill_hooks(&home, &skill, "codex").is_err());
        assert!(!hook_target("codex").unwrap().path(&home).exists());
    }

    #[test]
    fn an_event_declared_with_no_entries_creates_no_key() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            json!({ "hooks": { "PreToolUse": [], "Stop": [ { "hooks": [ { "type": "command", "command": "${SKILL_DIR}/s.sh" } ] } ] } }),
        );

        let report = sync_skill_hooks(&home, &skill, "codex").unwrap();
        assert_eq!(report.events, ["Stop"], "the empty event is not reported");

        let events = events_of(&home, "codex");
        assert!(
            !events.contains_key("PreToolUse"),
            "an unownable empty key must never be created"
        );

        remove_skill_hooks(&home, &skill, "codex").unwrap();
        assert!(
            events_of(&home, "codex").is_empty(),
            "removal must leave nothing of this skill behind"
        );
    }

    #[test]
    fn removal_leaves_foreign_empty_event_keys_alone() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            one_hook("PreToolUse", "${SKILL_DIR}/a.sh"),
        );
        sync_skill_hooks(&home, &skill, "codex").unwrap();

        // Somebody else's empty placeholder, added after the sync.
        let path = hook_target("codex").unwrap().path(&home);
        let mut doc = read_doc(&path).unwrap();
        doc[HOOKS_KEY]["Stop"] = json!([]);
        write_doc(&path, &doc).unwrap();

        remove_skill_hooks(&home, &skill, "codex").unwrap();
        let events = events_of(&home, "codex");
        assert!(
            events.contains_key("Stop"),
            "an already-empty foreign key is not ours to delete"
        );
        assert!(
            !events.contains_key("PreToolUse"),
            "the key this removal emptied is pruned"
        );
    }

    #[test]
    fn merging_preserves_unrelated_settings() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let path = hook_target("claude").unwrap().path(&home);
        write_doc(
            &path,
            json!({ "model": "opus", "statusLine": { "type": "command" } })
                .as_object()
                .unwrap(),
        )
        .unwrap();

        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            one_hook("SessionStart", "${SKILL_DIR}/s.sh"),
        );
        sync_skill_hooks(&home, &skill, "claude").unwrap();

        let doc = read_doc(&path).unwrap();
        assert_eq!(doc["model"], "opus", "unrelated settings must survive");
        assert!(doc["statusLine"].is_object());
        assert!(doc[HOOKS_KEY]["SessionStart"].is_array());
    }

    #[test]
    fn refuses_to_provision_an_agent_that_is_not_installed() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let skill = skill_with_hooks(
            temp.path(),
            "demo",
            one_hook("PreToolUse", "${SKILL_DIR}/c.sh"),
        );

        assert!(sync_skill_hooks(&home, &skill, "codex").is_err());
        assert!(
            !home.join(".codex").exists(),
            "a missing agent must not be conjured into existence"
        );
    }

    #[test]
    fn a_skill_without_hooks_is_a_no_op() {
        let temp = TempDir::new().unwrap();
        let home = home_with_codex(temp.path());
        let skill = temp.path().join("plain");
        std::fs::create_dir_all(&skill).unwrap();

        let report = sync_skill_hooks(&home, &skill, "codex").unwrap();
        assert!(report.events.is_empty());
        assert_eq!(report.entries, 0);
        assert!(!hook_target("codex").unwrap().path(&home).exists());
    }

    #[test]
    fn target_ids_exist_in_the_agent_registry() {
        // A typo here would silently make hooks unreachable for that agent.
        let profiles = skillstar_agents::list_profiles();
        for agent_id in hook_capable_agents() {
            assert!(
                profiles.iter().any(|profile| profile.id == agent_id),
                "hook target '{agent_id}' is not an AgentProfile id"
            );
        }
    }
}

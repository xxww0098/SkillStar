//! Pure shell-script builders + remote-path helpers for the hub operations.
//!
//! **Why `"$HOME"` instead of `~`:** a `~` inside a single-quoted shell word is
//! never expanded, and SFTP servers (OpenSSH included) treat `~` as a literal
//! path component too. Early versions quoted `~/.skillstar/...`, which silently
//! created a directory literally named `~` under `$HOME` and dangling agent
//! symlinks. Every script here therefore references the remote `$HOME` as a
//! double-quoted shell variable (expanded server-side at run time), and every
//! SFTP path is made absolute via [`expand_remote_home`] with the home dir
//! resolved once per session (`sftp.canonicalize(".")`).
//!
//! All builders are pure `&str -> String` so they unit-test without a live
//! connection. Skill names are validated ([`validate_skill_name`]) and every
//! embedded value goes through [`shell_quote`].

use anyhow::Result;

/// Hub content root, relative to the remote `$HOME`.
pub const REMOTE_HUB_REL: &str = ".skillstar/hub/content";

/// Legacy (buggy) hub content root: a literal `~` directory under `$HOME`,
/// produced by older SkillStar versions that quoted `~` paths. Kept only so
/// discovery still recognises un-healed layouts and the heal script can move
/// them into place.
pub const LEGACY_HUB_PREFIX: &str = "~/.skillstar/hub/content";

/// Shell-safe single-quoted string.
pub fn shell_quote(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

/// Reject skill names that would escape the hub content dir or break scripts.
pub fn validate_skill_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name == "~"
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\n')
    {
        anyhow::bail!("invalid skill name: {name:?}");
    }
    Ok(())
}

/// Shell expression for a possibly `~`-prefixed path: `~/x` becomes
/// `"$HOME"/'x'` (expanded remotely at run time), everything else is quoted
/// verbatim. POSIX concatenates adjacent quoted words, so the result is safe
/// to splice into scripts.
pub fn shell_path_expr(path: &str) -> String {
    if path == "~" {
        return "\"$HOME\"".to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("\"$HOME\"/{}", shell_quote(rest));
    }
    shell_quote(path)
}

/// Shell expression for the hub content root: `"$HOME/.skillstar/hub/content"`.
pub fn hub_root_expr() -> String {
    format!("\"$HOME/{REMOTE_HUB_REL}\"")
}

/// Shell expression for one skill's hub content dir.
pub fn hub_skill_expr(skill_name: &str) -> String {
    format!("{}/{}", hub_root_expr(), shell_quote(skill_name))
}

/// Expand a possibly `~`-prefixed remote path to an absolute one using the
/// already-resolved remote home. Used for SFTP paths (SFTP has no `$HOME`).
pub fn expand_remote_home(home: &str, path: &str) -> String {
    let home = home.trim_end_matches('/');
    if path == "~" {
        return home.to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return format!("{home}/{rest}");
    }
    path.to_string()
}

/// Absolute hub content dir for one skill, given the resolved remote home.
pub fn hub_skill_abs(home: &str, skill_name: &str) -> String {
    format!(
        "{}/{REMOTE_HUB_REL}/{skill_name}",
        home.trim_end_matches('/')
    )
}

// ── operation scripts ───────────────────────────────────────────────

/// `mkdir -p` the agent dir and (re)point `<agent>/<name>` at the hub content.
pub fn link_skill_script(agent_skills_dir: &str, skill_name: &str) -> String {
    let dir = shell_path_expr(agent_skills_dir.trim_end_matches('/'));
    let link = format!("{dir}/{}", shell_quote(skill_name));
    let target = hub_skill_expr(skill_name);
    format!("set -e\nmkdir -p {dir}\nln -sfn {target} {link}\necho LINKED\n")
}

/// Move a standalone skill tree into the hub and replace it with a symlink.
pub fn migrate_script(skill_name: &str, agent_skills_dir: &str, standalone_path: &str) -> String {
    let hub_root = hub_root_expr();
    let content = hub_skill_expr(skill_name);
    let standalone = shell_path_expr(standalone_path);
    let dir = shell_path_expr(agent_skills_dir.trim_end_matches('/'));
    let link = format!("{dir}/{}", shell_quote(skill_name));
    format!(
        r#"set -e
mkdir -p {hub_root}
if [ -e {content} ]; then
  echo "HUB_EXISTS"
  exit 1
fi
if [ -L {standalone} ]; then
  rm -f {standalone}
elif [ -d {standalone} ]; then
  mv {standalone} {content}
else
  echo "MISSING_STANDALONE"
  exit 1
fi
mkdir -p {dir}
ln -sfn {content} {link}
echo OK
"#
    )
}

/// One-shot, idempotent repair of layouts produced by the old literal-`~` bug:
/// 1. move `$HOME/~/.skillstar/hub/content/*` into the real hub root, and
/// 2. re-point any agent symlink whose target still starts with a literal `~/`.
///
/// Safe to run on every discovery: with nothing to heal it only reads. Prints
/// `HEALED moved=<n> relinked=<n>` so the caller can surface non-zero repairs.
pub fn heal_legacy_layout_script() -> String {
    r#"LEGACY="$HOME/~/.skillstar/hub/content"
HUB="$HOME/.skillstar/hub/content"
moved=0
relinked=0
if [ -d "$LEGACY" ]; then
  mkdir -p "$HUB"
  for d in "$LEGACY"/*; do
    [ -e "$d" ] || continue
    n=$(basename "$d")
    if [ ! -e "$HUB/$n" ]; then
      mv "$d" "$HUB/$n" && moved=$((moved+1))
    fi
  done
  rmdir "$HOME/~/.skillstar/hub/content" "$HOME/~/.skillstar/hub" "$HOME/~/.skillstar" "$HOME/~" 2>/dev/null
fi
for a in "$HOME"/.*; do
  case "$a" in */.|*/..) continue ;; esac
  [ -d "$a/skills" ] || continue
  for l in "$a/skills"/*; do
    [ -L "$l" ] || continue
    t=$(readlink "$l") || continue
    case "$t" in
      "~/"*) ln -sfn "$HOME/${t#??}" "$l" && relinked=$((relinked+1)) ;;
    esac
  done
done
echo "HEALED moved=$moved relinked=$relinked"
"#
    .to_string()
}

/// Parse the `HEALED moved=<n> relinked=<n>` line; `None` when absent.
pub fn parse_heal_output(out: &str) -> Option<(u32, u32)> {
    let line = out.lines().find(|l| l.starts_with("HEALED"))?;
    let mut moved = 0u32;
    let mut relinked = 0u32;
    for part in line.split_whitespace() {
        if let Some(v) = part.strip_prefix("moved=") {
            moved = v.parse().ok()?;
        } else if let Some(v) = part.strip_prefix("relinked=") {
            relinked = v.parse().ok()?;
        }
    }
    Some((moved, relinked))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No script may quote a `~` — that was the original bug.
    fn assert_no_quoted_tilde(script: &str) {
        assert!(
            !script.contains("'~") && !script.contains("~'"),
            "script contains a quoted tilde that the remote shell will never expand:\n{script}"
        );
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("a"), "'a'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
    }

    #[test]
    fn validate_skill_name_rejects_traversal() {
        assert!(validate_skill_name("good-skill").is_ok());
        assert!(validate_skill_name("").is_err());
        assert!(validate_skill_name("..").is_err());
        assert!(validate_skill_name("a/b").is_err());
        assert!(validate_skill_name("~").is_err());
    }

    #[test]
    fn shell_path_expr_expands_tilde_via_home_var() {
        assert_eq!(
            shell_path_expr("~/.claude/skills"),
            "\"$HOME\"/'.claude/skills'"
        );
        assert_eq!(shell_path_expr("~"), "\"$HOME\"");
        assert_eq!(
            shell_path_expr("/root/.codex/skills"),
            "'/root/.codex/skills'"
        );
    }

    #[test]
    fn expand_remote_home_makes_absolute() {
        assert_eq!(
            expand_remote_home("/root", "~/.claude/skills"),
            "/root/.claude/skills"
        );
        assert_eq!(expand_remote_home("/root/", "~"), "/root");
        assert_eq!(expand_remote_home("/root", "/abs/path"), "/abs/path");
    }

    #[test]
    fn hub_skill_abs_builds_absolute_content_dir() {
        assert_eq!(
            hub_skill_abs("/root", "my-skill"),
            "/root/.skillstar/hub/content/my-skill"
        );
    }

    #[test]
    fn link_script_uses_home_var_and_quotes_name() {
        let s = link_skill_script("~/.claude/skills", "my-skill");
        assert_no_quoted_tilde(&s);
        assert!(s.contains("\"$HOME/.skillstar/hub/content\"/'my-skill'"));
        assert!(s.contains("\"$HOME\"/'.claude/skills'/'my-skill'"));
        assert!(s.starts_with("set -e"));
    }

    #[test]
    fn migrate_script_moves_then_links() {
        let s = migrate_script("sk", "/root/.codex/skills", "/root/.codex/skills/sk");
        assert_no_quoted_tilde(&s);
        assert!(s.contains("HUB_EXISTS"));
        assert!(s.contains("MISSING_STANDALONE"));
        assert!(s.contains("mv '/root/.codex/skills/sk'"));
        // Symlink creation must come after the mv (mv failure aborts via set -e).
        let mv_pos = s.find("mv ").unwrap();
        let ln_pos = s.find("ln -sfn").unwrap();
        assert!(mv_pos < ln_pos);
    }

    #[test]
    fn heal_script_moves_legacy_dir_and_relinks() {
        let s = heal_legacy_layout_script();
        assert!(s.contains(r#""$HOME/~/.skillstar/hub/content""#));
        assert!(s.contains("readlink"));
        assert!(s.contains("HEALED"));
    }

    #[test]
    fn parse_heal_output_extracts_counts() {
        assert_eq!(
            parse_heal_output("HEALED moved=2 relinked=5\n"),
            Some((2, 5))
        );
        assert_eq!(parse_heal_output("HEALED moved=0 relinked=0"), Some((0, 0)));
        assert_eq!(parse_heal_output("standalone"), None);
    }
}

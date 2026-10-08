//! Which facts the skill column may show.
//!
//! A fact appears once. A repository URL already contains its `owner/repo`
//! and that repository's owner, so those are not second rows. The install
//! path is not a fact: it is the skill name under a fixed directory.

use ss_core::types::skill::{Skill, UpstreamChange};

/// One source line. `Link` opens in the browser; `Text` is the address as
/// written (SSH URLs are not browser links).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceValue<'a> {
    Link(&'a str),
    Text(&'a str),
}

impl<'a> SourceValue<'a> {
    pub(crate) fn as_str(self) -> &'a str {
        match self {
            SourceValue::Link(value) | SourceValue::Text(value) => value,
        }
    }
}

/// Something the tracked source did that the column should explain in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UpstreamNote {
    Removed { source: String, folder: String },
    Renamed { from: String, to: String },
    LocalEdits { baseline_missing: bool },
}

/// HTTP(S) address first, then any other source the address does not already
/// say. Empty input yields nothing.
pub(crate) fn source_values<'a>(
    git_url: &'a str,
    source: Option<&'a str>,
    name: &str,
) -> Vec<SourceValue<'a>> {
    let mut out = Vec::new();
    if let Some(git) = nonempty(git_url) {
        push_source(&mut out, git);
    }
    if let Some(extra) = source.and_then(nonempty).filter(|value| *value != name) {
        let covered = out.iter().any(|line| covers(line.as_str(), extra));
        if !covered {
            push_source(&mut out, extra);
        }
    }
    out
}

fn push_source<'a>(out: &mut Vec<SourceValue<'a>>, value: &'a str) {
    if is_http(value) {
        out.push(SourceValue::Link(value));
    } else {
        out.push(SourceValue::Text(value));
    }
}

/// Author handle, when it is not already the owner inside a shown source.
pub(crate) fn shown_author<'a>(
    author: Option<&'a str>,
    lines: &[SourceValue<'a>],
) -> Option<&'a str> {
    let author = nonempty(author?)?;
    let handle = author.trim_start_matches('@');
    if handle.is_empty() {
        return None;
    }
    let covered = lines.iter().any(|line| {
        let raw = line.as_str();
        covers(raw, author) || covers(raw, handle)
    });
    if covered { None } else { Some(author) }
}

/// Local minute precision. Unparseable text is returned whole so a value is
/// never replaced with a shorter guess. Empty input is absent.
pub(crate) fn format_updated(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(trimmed) else {
        return Some(trimmed.to_string());
    };
    Some(
        parsed
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
    )
}

pub(crate) fn upstream_note(skill: &Skill) -> Option<UpstreamNote> {
    match &skill.upstream_change {
        Some(UpstreamChange::Removed { .. }) => Some(UpstreamNote::Removed {
            source: display_source(skill),
            folder: skill.name.clone(),
        }),
        Some(UpstreamChange::IdentityChanged { upstream_name }) => Some(UpstreamNote::Renamed {
            from: skill.name.clone(),
            to: upstream_name.clone(),
        }),
        Some(UpstreamChange::LocalChanges { baseline_missing }) => Some(UpstreamNote::LocalEdits {
            baseline_missing: *baseline_missing,
        }),
        None => None,
    }
}

fn display_source(skill: &Skill) -> String {
    skill
        .source
        .as_deref()
        .and_then(nonempty)
        .filter(|value| *value != skill.name)
        .or_else(|| nonempty(&skill.git_url))
        .unwrap_or("")
        .to_string()
}

/// Break opportunities after URL punctuation so the narrow column can show
/// the whole address. The stored href is unchanged.
pub(crate) fn wrap_long_token(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + value.len() / 8);
    for ch in value.chars() {
        out.push(ch);
        if matches!(ch, '/' | '.' | '-' | '_') {
            out.push('\u{200b}');
        }
    }
    out
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() { None } else { Some(value) }
}

fn is_http(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

fn covers(url: &str, source: &str) -> bool {
    let url = normalize_source(url);
    let source = normalize_source(source);
    if source.is_empty() {
        return false;
    }
    url == source || url.ends_with(&format!("/{source}")) || url.contains(&format!("/{source}/"))
}

fn normalize_source(value: &str) -> String {
    let value = value.trim().trim_end_matches('/').trim_end_matches(".git");
    let value = value
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://")
        .trim_start_matches("git@");
    value.replace(':', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_url_hides_the_owner_repo_it_already_contains() {
        let lines = source_values(
            "https://github.com/nextlevelbuilder/ui-ux-pro-max.git",
            Some("nextlevelbuilder/ui-ux-pro-max"),
            "ui-ux-pro-max",
        );
        assert_eq!(
            lines,
            vec![SourceValue::Link(
                "https://github.com/nextlevelbuilder/ui-ux-pro-max.git"
            )]
        );
    }

    #[test]
    fn ssh_url_covers_owner_repo_and_stays_text() {
        let lines = source_values(
            "git@github.com:nextlevelbuilder/ui-ux-pro-max.git",
            Some("nextlevelbuilder/ui-ux-pro-max"),
            "ui-ux-pro-max",
        );
        assert_eq!(
            lines,
            vec![SourceValue::Text(
                "git@github.com:nextlevelbuilder/ui-ux-pro-max.git"
            )]
        );
    }

    #[test]
    fn local_source_shows_when_there_is_no_url() {
        let lines = source_values("", Some("local/cursor"), "brand");
        assert_eq!(lines, vec![SourceValue::Text("local/cursor")]);
    }

    #[test]
    fn source_equal_to_the_skill_name_is_not_a_second_fact() {
        assert!(source_values("", Some("brand"), "brand").is_empty());
    }

    #[test]
    fn a_different_source_stays_beside_an_unrelated_url() {
        let lines = source_values(
            "https://gitlab.com/other/repo",
            Some("local/cursor"),
            "brand",
        );
        assert_eq!(
            lines,
            vec![
                SourceValue::Link("https://gitlab.com/other/repo"),
                SourceValue::Text("local/cursor"),
            ]
        );
    }

    #[test]
    fn author_hidden_when_it_is_the_github_owner() {
        let lines = source_values(
            "https://github.com/nextlevelbuilder/ui-ux-pro-max",
            Some("nextlevelbuilder/ui-ux-pro-max"),
            "ui-ux-pro-max",
        );
        assert!(shown_author(Some("nextlevelbuilder"), &lines).is_none());
        assert_eq!(
            shown_author(Some("someone-else"), &lines),
            Some("someone-else")
        );
    }

    #[test]
    fn keeps_an_unparseable_timestamp_whole() {
        assert_eq!(format_updated("yesterday").as_deref(), Some("yesterday"));
        assert!(format_updated("  ").is_none());
    }

    #[test]
    fn wrap_inserts_breaks_without_dropping_characters() {
        let wrapped = wrap_long_token("https://github.com/a/b");
        assert!(wrapped.contains('\u{200b}'));
        assert_eq!(wrapped.replace('\u{200b}', ""), "https://github.com/a/b");
    }
}

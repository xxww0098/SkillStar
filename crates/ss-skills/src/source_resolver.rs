//! URL normalization for repository sources.
//!
//! Converts user-provided repository identifiers into normalized clone URLs
//! and short display identifiers.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ── Source type ─────────────────────────────────────────────────────

/// Normalized source identifier for a skill repository.
///
/// `Source::parse` is the primary entry point for parsing user-provided
/// repository inputs (owner/repo, HTTPS URLs, .git-suffixed URLs).
///
/// Serializes under the field names `ScanResult` already exposed to the
/// frontend (`source`, `source_url`), so flattening it into `ScanResult`
/// is backward compatible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// Full clone URL, e.g. `https://github.com/owner/repo.git`
    #[serde(rename = "source_url")]
    pub repo_url: String,
    /// Short identifier, e.g. `owner/repo`
    #[serde(rename = "source")]
    pub short: String,
    /// Optional Git ref selected by a tree URL or `#ref` suffix.
    pub git_ref: Option<String>,
    /// Optional repository-relative directory to scan.
    pub subpath: Option<String>,
    /// Optional skill selected by `owner/repo@skill` or `#ref@skill`.
    pub skill_filter: Option<String>,
    /// True when this scan came from well-known discovery, not a Git checkout.
    /// Missing JSON stays false so older previews still install as Git.
    #[serde(default, skip_serializing_if = "is_false")]
    pub well_known: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Source {
    /// Parse any accepted input into a normalized `Source`.
    ///
    /// Accepted Git forms:
    /// - `owner/repo`, `owner/repo/path`, `owner/repo@skill`
    /// - GitHub / GitLab repository and tree URLs
    /// - HTTPS, SCP-style SSH, and `ssh://` Git URLs
    /// - an optional `#ref` or `#ref@skill` suffix
    ///
    /// Errors on empty or whitespace input.
    pub fn parse(input: &str) -> Result<Source> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("Repository URL cannot be empty"));
        }
        if ss_git::transport::is_unsafe_git_transport(trimmed) {
            return Err(anyhow!(
                "Unsupported Git transport. Use https, ssh, git, file, or a local path."
            ));
        }

        let (source_without_fragment, fragment_ref, fragment_skill) = split_fragment(trimmed)?;
        let source_without_fragment = source_without_fragment
            .strip_prefix("github:")
            .map(str::to_string)
            .unwrap_or_else(|| source_without_fragment.to_string());

        if let Some(gitlab) = source_without_fragment.strip_prefix("gitlab:") {
            return parse_http_source(
                &format!("https://gitlab.com/{gitlab}"),
                fragment_ref,
                fragment_skill,
            );
        }

        if source_without_fragment.starts_with("git@")
            || source_without_fragment.starts_with("ssh://")
        {
            let short = short_from_ssh(&source_without_fragment)?;
            let repo_url = if ssh_host(&source_without_fragment)
                .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
            {
                format!("https://github.com/{short}.git")
            } else {
                source_without_fragment
            };
            return Ok(Source {
                repo_url,
                short,
                git_ref: fragment_ref,
                subpath: None,
                skill_filter: fragment_skill,
                well_known: false,
            });
        }

        if source_without_fragment.starts_with("http://")
            || source_without_fragment.starts_with("https://")
        {
            return parse_http_source(&source_without_fragment, fragment_ref, fragment_skill);
        }

        if looks_like_local_source(&source_without_fragment) {
            return parse_local_source(&source_without_fragment, fragment_ref, fragment_skill);
        }

        parse_github_shorthand(&source_without_fragment, fragment_ref, fragment_skill)
    }
}

fn looks_like_local_source(input: &str) -> bool {
    input.starts_with("file://")
        || Path::new(input).is_absolute()
        || input.starts_with('.')
        || input.starts_with('~')
}

/// Undo the form [`ss_git::ops::local_file_url`] writes: drop the scheme and,
/// for a drive-letter path, the slash that precedes the drive. That slash is
/// part of the URL, not of the path — kept, it turns `C:/…` into a path that
/// Windows cannot resolve. Every reader of a `file://` source goes through
/// here, so the two sides cannot drift apart again.
pub(crate) fn file_url_to_path_input(repo_url: &str) -> Option<&str> {
    let rest = repo_url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    match rest.strip_prefix('/') {
        Some(after) if cfg!(windows) && is_windows_drive_path(after) => Some(after),
        _ => Some(rest),
    }
}

fn parse_local_source(
    input: &str,
    git_ref: Option<String>,
    skill_filter: Option<String>,
) -> Result<Source> {
    let path = match file_url_to_path_input(input) {
        Some(rest) => PathBuf::from(
            if rest.len() >= 2 && rest.as_bytes()[1] == b':' || rest.starts_with('/') {
                rest.to_string()
            } else {
                format!("/{rest}")
            },
        ),
        None => PathBuf::from(input),
    };
    if !path.exists() {
        return Err(anyhow!(
            "Local path does not exist. Use 'owner/repo', a Git URL, or a local path."
        ));
    }
    let canonical = std::fs::canonicalize(&path)
        .map_err(|error| anyhow!("Invalid local path '{input}': {error}"))?;
    if !canonical.is_dir() {
        return Err(anyhow!("Local source must be a directory: {input}"));
    }
    let short = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("local-skill");
    Ok(Source {
        repo_url: crate::git::ops::local_file_url(&canonical),
        short: format!("local/{short}"),
        git_ref,
        subpath: None,
        skill_filter,
        well_known: false,
    })
}

fn split_fragment(input: &str) -> Result<(String, Option<String>, Option<String>)> {
    let Some((source, fragment)) = input.split_once('#') else {
        return Ok((input.to_string(), None, None));
    };
    if fragment.trim().is_empty() {
        return Err(anyhow!("Git ref after '#' cannot be empty"));
    }
    let (git_ref, skill_filter) = match fragment.split_once('@') {
        Some((git_ref, skill)) => (git_ref, Some(skill)),
        None => (fragment, None),
    };
    if git_ref.trim().is_empty() {
        return Err(anyhow!("Git ref after '#' cannot be empty"));
    }
    let skill_filter = skill_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok((
        source.to_string(),
        Some(git_ref.trim().to_string()),
        skill_filter,
    ))
}

fn parse_github_shorthand(
    input: &str,
    git_ref: Option<String>,
    fragment_skill: Option<String>,
) -> Result<Source> {
    if input.contains(':') || input.starts_with('.') || input.starts_with('/') {
        return Err(anyhow!(
            "Invalid repository format. Use 'owner/repo', a Git URL, or a local path."
        ));
    }

    let (path, inline_skill) = match input.rsplit_once('@') {
        Some((path, skill)) if path.split('/').count() == 2 => {
            (path, Some(skill.trim().to_string()))
        }
        _ => (input, None),
    };
    let parts = path
        .trim_end_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(anyhow!(
            "Invalid repository format. Use 'owner/repo' or a full Git URL."
        ));
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return Err(anyhow!("Repository owner and name cannot be empty"));
    }

    let subpath = if parts.len() > 2 {
        Some(sanitize_subpath(&parts[2..].join("/"))?)
    } else {
        None
    };
    let short = format!("{owner}/{repo}");
    Ok(Source {
        repo_url: format!("https://github.com/{short}.git"),
        short,
        git_ref,
        subpath,
        skill_filter: fragment_skill.or(inline_skill),
        well_known: false,
    })
}

fn parse_http_source(
    input: &str,
    fragment_ref: Option<String>,
    fragment_skill: Option<String>,
) -> Result<Source> {
    let (scheme, remainder) = input
        .split_once("://")
        .ok_or_else(|| anyhow!("Invalid Git URL: {input}"))?;
    let (authority, raw_path) = remainder
        .split_once('/')
        .ok_or_else(|| anyhow!("Git URL must contain a repository path: {input}"))?;
    let path = raw_path.trim_end_matches('/');
    if authority.is_empty() || path.is_empty() {
        return Err(anyhow!("Git URL must contain a host and repository path"));
    }

    if is_github_authority(authority) {
        let parts = path.split('/').collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(anyhow!("GitHub URL must contain owner/repo"));
        }
        let owner = parts[0];
        let repo = parts[1].trim_end_matches(".git");
        let mut git_ref = fragment_ref;
        let mut subpath = None;
        if parts.get(2).is_some_and(|part| *part == "tree") {
            let tree_ref = parts
                .get(3)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow!("GitHub tree URL must include a ref"))?;
            git_ref = Some((*tree_ref).to_string());
            if parts.len() > 4 {
                subpath = Some(sanitize_subpath(&parts[4..].join("/"))?);
            }
        }
        let short = format!("{owner}/{repo}");
        return Ok(Source {
            repo_url: format!("https://github.com/{short}.git"),
            short,
            git_ref,
            subpath,
            skill_filter: fragment_skill,
            well_known: false,
        });
    }

    if let Some((repo_path, tree_path)) = path.split_once("/-/tree/") {
        let mut tree_parts = tree_path.split('/');
        let tree_ref = tree_parts
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("GitLab tree URL must include a ref"))?;
        let remaining = tree_parts.collect::<Vec<_>>().join("/");
        let clean_repo_path = repo_path.trim_end_matches(".git");
        return Ok(Source {
            repo_url: format!("{scheme}://{authority}/{clean_repo_path}.git"),
            short: clean_repo_path.to_string(),
            git_ref: Some(tree_ref.to_string()).or(fragment_ref),
            subpath: if remaining.is_empty() {
                None
            } else {
                Some(sanitize_subpath(&remaining)?)
            },
            skill_filter: fragment_skill,
            well_known: false,
        });
    }

    let clean_path = path.trim_end_matches(".git");
    let repo_url = if path.ends_with(".git") {
        input.to_string()
    } else {
        format!("{scheme}://{authority}/{clean_path}.git")
    };
    Ok(Source {
        repo_url,
        short: format!("{authority}/{clean_path}"),
        git_ref: fragment_ref,
        subpath: None,
        skill_filter: fragment_skill,
        well_known: false,
    })
}

fn is_github_authority(authority: &str) -> bool {
    let authority = authority.to_ascii_lowercase();
    let authority = authority
        .rsplit_once('@')
        .map(|(_, authority)| authority)
        .unwrap_or(authority.as_str());
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, None), |(host, port)| (host, Some(port)));
    matches!(host, "github.com" | "www.github.com")
        && port.is_none_or(|port| matches!(port, "80" | "443"))
}

fn short_from_ssh(input: &str) -> Result<String> {
    let path = if let Some(rest) = input.strip_prefix("ssh://") {
        rest.split_once('/')
            .map(|(_, path)| path)
            .ok_or_else(|| anyhow!("SSH URL must contain a repository path"))?
    } else {
        input
            .split_once(':')
            .map(|(_, path)| path)
            .ok_or_else(|| anyhow!("SSH URL must use git@host:path syntax"))?
    };
    let short = path.trim_matches('/').trim_end_matches(".git");
    if !short.contains('/') {
        return Err(anyhow!("SSH URL must contain owner/repo"));
    }
    Ok(short.to_string())
}

fn ssh_host(input: &str) -> Option<String> {
    let authority = input
        .strip_prefix("ssh://")
        .and_then(|rest| rest.split_once('/').map(|(authority, _)| authority))
        .or_else(|| input.split_once(':').map(|(authority, _)| authority))?;
    let host = authority
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(authority);
    let host = host
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(host, _)| host)
        .unwrap_or(host);
    Some(host.to_string())
}

fn sanitize_subpath(input: &str) -> Result<String> {
    let normalized = input.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized
            .split('/')
            .any(|segment| segment == ".." || segment == ".")
    {
        return Err(anyhow!(
            "Unsafe repository subpath '{input}': relative traversal is not allowed"
        ));
    }
    let normalized = normalized.trim_matches('/');
    if normalized.is_empty() {
        return Err(anyhow!("Repository subpath cannot be empty"));
    }
    Ok(normalized.to_string())
}

// ── URL Normalization ───────────────────────────────────────────────

/// Derive the install name hint for a source: explicit name, then `@skill`
/// filter, then subpath tail, then repo short tail, then the raw URL tail.
///
/// Single implementation shared by the domain install path and the CLI, so a
/// preview and a real install can never derive different hints.
pub fn derive_skill_name_hint(input: &str, explicit_name: Option<&str>) -> String {
    explicit_name.map(str::to_string).unwrap_or_else(|| {
        if let Ok(source) = Source::parse(input) {
            if let Some(skill) = source.skill_filter {
                return skill;
            }
            if let Some(subpath) = source.subpath
                && let Some(name) = subpath.rsplit('/').next()
            {
                return name.to_string();
            }
            if let Some(name) = source.short.rsplit('/').next() {
                return name.to_string();
            }
        }
        input
            .rsplit('/')
            .next()
            .unwrap_or("skill")
            .trim_end_matches(".git")
            .to_string()
    })
}

pub fn cache_dir_name(source: &str) -> String {
    source
        .chars()
        .flat_map(|ch| {
            if ch == '/' || ch == '\\' {
                "--".chars().collect::<Vec<_>>()
            } else if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                vec![ch]
            } else {
                vec!['-']
            }
        })
        .collect()
}

pub fn same_remote_url(left: &str, right: &str) -> bool {
    let left = Source::parse(left)
        .map(|source| source.repo_url)
        .unwrap_or_else(|_| left.to_string());
    let right = Source::parse(right)
        .map(|source| source.repo_url)
        .unwrap_or_else(|_| right.to_string());
    normalize_remote_url(&left) == normalize_remote_url(&right)
}

pub fn normalize_remote_url(url: &str) -> String {
    url.trim()
        .trim_end_matches(".git")
        .trim_end_matches('/')
        .to_lowercase()
}

/// Result of parsing either a raw source string or a pasted CLI command
/// (`npx skills add ...`, `bunx skills add ...`, `pnpm dlx skills add ...`,
/// `yarn dlx skills add ...`, `skills add ...`, `skillstar install ...`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParsedInstallCommand {
    pub source: String,
    pub skills: Vec<String>,
    pub agents: Vec<String>,
    pub global: bool,
    pub copy: bool,
    pub all: bool,
    pub yes: bool,
    pub was_command: bool,
}

fn tokenize_shell_words(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    // A word that opens with a drive letter (`C:`) is a Windows path: outside
    // quotes its backslashes stay literal, the way PowerShell and cmd read them.
    let mut windows_path = false;
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_single {
            if ch == '\'' {
                in_single = false;
            } else {
                current.push(ch);
            }
            continue;
        }
        if in_double {
            // POSIX shells escape only these four here; before anything else a
            // backslash is literal, which is what a quoted Windows path needs.
            if ch == '\\' && matches!(chars.peek(), Some('"' | '\\' | '`' | '$')) {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            } else if ch == '"' {
                in_double = false;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' => in_single = true,
            '"' => in_double = true,
            '\\' if windows_path => current.push(ch),
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                    windows_path = false;
                }
            }
            _ => {
                current.push(ch);
                windows_path |= is_windows_drive_path(&current);
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn command_tail_index(tokens: &[String]) -> Option<usize> {
    if tokens.is_empty() {
        return None;
    }
    let first = tokens[0].to_ascii_lowercase();
    let is_add = |s: &str| matches!(s, "add" | "install" | "i");
    let skip_runner_flags = |mut idx: usize| -> usize {
        while idx < tokens.len()
            && matches!(tokens[idx].as_str(), "-y" | "--yes" | "--quiet" | "-q")
        {
            idx += 1;
        }
        idx
    };
    match first.as_str() {
        "npx" | "bunx" => {
            let idx = skip_runner_flags(1);
            if idx + 1 < tokens.len()
                && tokens[idx].eq_ignore_ascii_case("skills")
                && is_add(&tokens[idx + 1].to_ascii_lowercase())
            {
                Some(idx + 2)
            } else {
                None
            }
        }
        "pnpm" | "yarn" => {
            if tokens.len() > 2 && tokens[1].eq_ignore_ascii_case("dlx") {
                let idx = skip_runner_flags(2);
                if idx + 1 < tokens.len()
                    && tokens[idx].eq_ignore_ascii_case("skills")
                    && is_add(&tokens[idx + 1].to_ascii_lowercase())
                {
                    return Some(idx + 2);
                }
            }
            None
        }
        "skills" => {
            if tokens.len() >= 2 && is_add(&tokens[1].to_ascii_lowercase()) {
                Some(2)
            } else {
                None
            }
        }
        "skillstar" => {
            if tokens.len() >= 2
                && matches!(tokens[1].to_ascii_lowercase().as_str(), "install" | "add")
            {
                Some(2)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse user input that may be either a plain repository/URL/path or a pasted
/// installation command (`npx skills add owner/repo --skill foo -g -y`).
pub fn parse_install_input(raw: &str) -> Result<ParsedInstallCommand> {
    let mut trimmed = raw.trim();
    while let Some(rest) = trimmed.strip_prefix('$').or_else(|| trimmed.strip_prefix('>')) {
        if trimmed == rest {
            break;
        }
        let next = rest.trim_start();
        if next.is_empty() || (!trimmed.starts_with("$ ") && !trimmed.starts_with("> ")) {
            break;
        }
        trimmed = next;
    }
    if trimmed.is_empty() {
        return Err(anyhow!("Install source cannot be empty"));
    }

    let tokens = tokenize_shell_words(trimmed);
    let Some(start) = command_tail_index(&tokens) else {
        let single = trimmed
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| trimmed.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(trimmed);
        return Ok(ParsedInstallCommand {
            source: single.to_string(),
            skills: Vec::new(),
            agents: Vec::new(),
            global: false,
            copy: false,
            all: false,
            yes: false,
            was_command: false,
        });
    };

    let mut parsed = ParsedInstallCommand {
        source: String::new(),
        skills: Vec::new(),
        agents: Vec::new(),
        global: false,
        copy: false,
        all: false,
        yes: false,
        was_command: true,
    };

    let push_csv = |val: &str, out: &mut Vec<String>| {
        for part in val.split(',') {
            let clean = part.trim();
            if !clean.is_empty() {
                out.push(clean.to_string());
            }
        }
    };

    let mut i = start;
    while i < tokens.len() {
        let tok = &tokens[i];
        if let Some(val) = tok
            .strip_prefix("--skill=")
            .or_else(|| tok.strip_prefix("--name="))
        {
            push_csv(val, &mut parsed.skills);
        } else if tok == "--skill" || tok == "-s" || tok == "--name" {
            if i + 1 < tokens.len() {
                i += 1;
                push_csv(&tokens[i], &mut parsed.skills);
            }
        } else if let Some(val) = tok.strip_prefix("--agent=") {
            push_csv(val, &mut parsed.agents);
        } else if tok == "--agent" || tok == "-a" {
            if i + 1 < tokens.len() {
                i += 1;
                push_csv(&tokens[i], &mut parsed.agents);
            }
        } else if tok == "--project" || tok == "-p" {
            if i + 1 < tokens.len() && !tokens[i + 1].starts_with('-') {
                i += 1;
            }
        } else if tok == "-g" || tok == "--global" {
            parsed.global = true;
        } else if tok == "--copy" {
            parsed.copy = true;
        } else if tok == "--all" {
            parsed.all = true;
        } else if tok == "-y" || tok == "--yes" {
            parsed.yes = true;
        } else if matches!(
            tok.as_str(),
            "--refresh" | "--list" | "-l" | "--preview" | "--json"
        ) || tok.starts_with("--")
        {
            // Ignore benign CLI flags.
        } else if tok.starts_with('-') && tok.len() > 1 && !tok.starts_with("./") && !tok.starts_with("../")
        {
            for flag in tok[1..].chars() {
                match flag {
                    'g' => parsed.global = true,
                    'y' => parsed.yes = true,
                    _ => {}
                }
            }
        } else if parsed.source.is_empty() {
            parsed.source = tok.clone();
        }
        i += 1;
    }

    if parsed.source.is_empty() {
        return Err(anyhow!(
            "Install command does not include a repository, URL, or path source"
        ));
    }
    Ok(parsed)
}

fn is_valid_repo_segment(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub(crate) fn is_windows_drive_path(input: &str) -> bool {
    let bytes = input.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn has_bundle_extension(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.ends_with(".ags") || lower.ends_with(".agd")
}

/// Return `true` when `raw` looks like a remote skill repository source
/// (GitHub/Git URL, `owner/repo` shorthand, or pasted install command),
/// excluding share codes and local `.ags`/`.agd` bundle paths.
pub fn looks_like_remote_source(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || crate::share_install::looks_like_share_code(trimmed) {
        return false;
    }
    let Ok(parsed) = parse_install_input(trimmed) else {
        return false;
    };
    if parsed.was_command {
        return true;
    }
    let source = parsed.source.trim();
    if source.is_empty()
        || source.chars().any(char::is_whitespace)
        || ss_git::transport::is_unsafe_git_transport(source)
    {
        return false;
    }
    let lower = source.to_ascii_lowercase();
    if lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("ssh://")
        || lower.starts_with("github:")
        || lower.starts_with("gitlab:")
        || source.starts_with("git@")
    {
        return true;
    }
    if looks_like_local_source(source)
        || source.contains('\\')
        || is_windows_drive_path(source)
        || has_bundle_extension(source)
    {
        return false;
    }
    let Ok(resolved) = Source::parse(source) else {
        return false;
    };
    let mut parts = resolved.short.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(owner), Some(repo), None)
            if is_valid_repo_segment(owner) && is_valid_repo_segment(repo)
    )
}

/// Return `true` when `raw` looks like a direct skill install target (a pasted
/// `npx/bunx/pnpm/yarn/skillstar` install command, a Git/HTTPS/SSH/file URL, a
/// local path, or a `owner/repo` shorthand) rather than a marketplace keyword query.
pub fn looks_like_direct_source(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    if let Ok(cmd) = parse_install_input(trimmed)
        && cmd.was_command
    {
        return true;
    }
    if trimmed.chars().any(char::is_whitespace) {
        return false;
    }
    if trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("git@")
        || trimmed.starts_with("ssh://")
        || trimmed.starts_with("file://")
        || trimmed.starts_with('/')
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with("~/")
    {
        return true;
    }
    let clean = trimmed
        .split('#')
        .next()
        .unwrap_or(trimmed);
    let clean = clean
        .rsplit_once('@')
        .map(|(lhs, _)| lhs)
        .unwrap_or(clean);
    let parts: Vec<&str> = clean.split('/').collect();
    if parts.len() == 2 {
        return is_valid_repo_segment(parts[0]) && is_valid_repo_segment(parts[1]);
    }
    false
}

#[cfg(test)]
#[path = "source_resolver_tests.rs"]
mod tests;

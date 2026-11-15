//! Preview of a project's package.json skills field.
//!
//! Grammar: https://github.com/antfu/skills-npm/blob/main/SPEC.md.
//! This module only reads. It does not install, uninstall, or copy anything
//! into node_modules. Uninstall is a plan: empty provenance refuses, and a
//! recorded name still requires confirmation. Neither branch deletes files.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::source_resolver::Source;
use crate::validation::{FrontmatterIssue, inspect_skill_frontmatter};

const MAX_PACKAGE_JSON_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SHIPPED_SKILLS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageSkillsPreview {
    pub project: String,
    pub remote: Vec<RemoteSkillsRequest>,
    pub npm: Vec<NpmSkillsRequest>,
    pub errors: Vec<String>,
    pub already_installed: Vec<String>,
    pub missing_named: Vec<String>,
    pub uninstall: UninstallPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteSkillsRequest {
    pub source: String,
    pub repo_url: String,
    pub git_ref: Option<String>,
    pub subpath: Option<String>,
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NpmSkillsRequest {
    pub package: String,
    pub skills: Vec<String>,
    pub resolved: Option<String>,
    pub shipped: Vec<String>,
}

/// What a confirmed uninstall would be allowed to name. This is not an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UninstallPlan {
    NothingRecorded,
    NeedsConfirmation { names: Vec<String> },
    Confirmed { names: Vec<String> },
}

impl UninstallPlan {
    pub fn summary(&self) -> String {
        match self {
            Self::NothingRecorded => {
                "Refusing to uninstall: SkillStar has no record that package.json installed these skills.".to_string()
            }
            Self::NeedsConfirmation { names } => format!(
                "Uninstall requires confirmation before removing: {}.",
                names.join(", ")
            ),
            Self::Confirmed { names } => format!(
                "Uninstall confirmed for {}; this preview does not remove files.",
                names.join(", ")
            ),
        }
    }
}

/// Plan only. An empty record refuses even when confirm is true. A recorded
/// name without confirm stays unconfirmed. Nothing is deleted.
pub fn plan_package_skills_uninstall(confirm: bool, recorded: &[String]) -> UninstallPlan {
    if recorded.is_empty() {
        return UninstallPlan::NothingRecorded;
    }
    let names = recorded.to_vec();
    if confirm {
        UninstallPlan::Confirmed { names }
    } else {
        UninstallPlan::NeedsConfirmation { names }
    }
}

/// Read the project package.json and describe the skills field. Does not fetch
/// git sources and does not write, including under node_modules.
pub fn preview_package_skills(
    project_dir: &Path,
    installed: &[String],
    recorded_by_field: &[String],
    confirm_uninstall: bool,
) -> PackageSkillsPreview {
    let mut preview = PackageSkillsPreview {
        project: project_dir.display().to_string(),
        remote: Vec::new(),
        npm: Vec::new(),
        errors: Vec::new(),
        already_installed: Vec::new(),
        missing_named: Vec::new(),
        uninstall: plan_package_skills_uninstall(confirm_uninstall, recorded_by_field),
    };
    let value = match read_package_json(&project_dir.join("package.json")) {
        Ok(value) => value,
        Err(error) => {
            preview.errors.push(error);
            return preview;
        }
    };
    let Some(field) = value.get("skills") else {
        return preview;
    };
    let Some(entries) = field.as_array() else {
        preview
            .errors
            .push("package.json: \"skills\" must be an array".to_string());
        return preview;
    };

    for entry in entries {
        match entry_parts(entry) {
            None => preview.errors.push(format!(
                ".: invalid \"skills\" entry {}",
                serde_json::to_string(entry).unwrap_or_else(|_| entry.to_string())
            )),
            Some((source, skills, git_ref)) => {
                classify_entry(&mut preview, &source, skills, git_ref.as_deref());
            }
        }
    }
    resolve_npm(&mut preview, project_dir);
    compare_installed(&mut preview, installed);
    preview
}

fn classify_entry(
    preview: &mut PackageSkillsPreview,
    source: &str,
    skills: Vec<String>,
    git_ref: Option<&str>,
) {
    let source = source.trim();
    if source.is_empty() {
        preview
            .errors
            .push(".: invalid \"skills\" entry \"\"".to_string());
        return;
    }
    if let Some(package) = source.strip_prefix("npm:") {
        if git_ref.is_some() {
            preview
                .errors
                .push(format!(".: \"ref\" cannot be used with \"{source}\""));
            return;
        }
        let package = package.trim();
        if !valid_npm_package(package) {
            preview
                .errors
                .push(format!(".: invalid npm package name \"{package}\""));
            return;
        }
        merge_npm(preview, package, skills);
        return;
    }
    if is_local_source(source) || is_non_git_http(source) {
        preview
            .errors
            .push(format!(".: \"{source}\" is not a git source"));
        return;
    }
    let parsed = match Source::parse(source) {
        Ok(parsed) if is_accepted_git(&parsed) => parsed,
        Ok(_) => {
            preview
                .errors
                .push(format!(".: \"{source}\" is not a git source"));
            return;
        }
        Err(error) => {
            preview
                .errors
                .push(format!(".: \"{source}\" is not a git source ({error:#})"));
            return;
        }
    };
    if git_ref.is_some() && parsed.git_ref.is_some() {
        preview
            .errors
            .push(format!(".: \"{source}\" already has a ref; remove \"ref\""));
        return;
    }
    let mut skills = skills;
    if let Some(filter) = parsed.skill_filter {
        if !skills.iter().any(|name| name == &filter) {
            skills.push(filter);
        }
    }
    let request = RemoteSkillsRequest {
        source: source.to_string(),
        repo_url: parsed.repo_url,
        git_ref: git_ref.map(str::to_string).or(parsed.git_ref),
        subpath: parsed.subpath,
        skills,
    };
    if !preview
        .remote
        .iter()
        .any(|existing| same_remote(existing, &request))
    {
        preview.remote.push(request);
    }
}

fn same_remote(left: &RemoteSkillsRequest, right: &RemoteSkillsRequest) -> bool {
    left.repo_url == right.repo_url
        && left.git_ref == right.git_ref
        && left.subpath == right.subpath
        && left.skills == right.skills
}

fn merge_npm(preview: &mut PackageSkillsPreview, package: &str, skills: Vec<String>) {
    if let Some(existing) = preview.npm.iter_mut().find(|item| item.package == package) {
        if existing.skills.is_empty() || skills.is_empty() {
            existing.skills.clear();
        } else {
            for skill in skills {
                if !existing.skills.contains(&skill) {
                    existing.skills.push(skill);
                }
            }
        }
        return;
    }
    preview.npm.push(NpmSkillsRequest {
        package: package.to_string(),
        skills,
        resolved: None,
        shipped: Vec::new(),
    });
}

fn resolve_npm(preview: &mut PackageSkillsPreview, project_dir: &Path) {
    for request in &mut preview.npm {
        let Some(dir) = find_installed_package(project_dir, &request.package) else {
            preview.errors.push(format!(
                ".: cannot resolve \"npm:{}\"; is it a dependency?",
                request.package
            ));
            continue;
        };
        let shipped = shipped_skills(&dir);
        if shipped.len() > MAX_SHIPPED_SKILLS {
            preview.errors.push(format!(
                ".: \"npm:{}\" contains too many skills",
                request.package
            ));
            continue;
        }
        let wanted = request.skills.clone();
        request.resolved = Some(dir.display().to_string());
        request.shipped = shipped
            .iter()
            .filter(|skill| skill_selected(&wanted, skill))
            .map(|skill| skill.name.clone())
            .collect();
        for wanted_name in &wanted {
            if !shipped
                .iter()
                .any(|skill| &skill.folder == wanted_name || &skill.name == wanted_name)
            {
                preview.errors.push(format!(
                    ".: \"npm:{}\" has no skill \"{wanted_name}\"",
                    request.package
                ));
            }
        }
    }
}

fn compare_installed(preview: &mut PackageSkillsPreview, installed: &[String]) {
    let mut named = Vec::new();
    for request in &preview.remote {
        named.extend(request.skills.iter().cloned());
    }
    for request in &preview.npm {
        if request.skills.is_empty() {
            named.extend(request.shipped.iter().cloned());
        } else {
            named.extend(request.skills.iter().cloned());
        }
    }
    named.sort();
    named.dedup();
    for name in named {
        if installed.iter().any(|installed| installed == &name) {
            preview.already_installed.push(name);
        } else {
            preview.missing_named.push(name);
        }
    }
}

struct FoundSkill {
    folder: String,
    name: String,
}

fn skill_selected(wanted: &[String], skill: &FoundSkill) -> bool {
    wanted.is_empty()
        || wanted
            .iter()
            .any(|name| name == &skill.folder || name == &skill.name)
}

fn shipped_skills(package_dir: &Path) -> Vec<FoundSkill> {
    let Ok(root) = package_dir.canonicalize() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let folder = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("package");
    if let Some(skill) = read_skill(&root, folder) {
        found.push(skill);
        return found;
    }
    for relative in ["skills", "dist/skills"] {
        let dir = root.join(relative);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= MAX_SHIPPED_SKILLS {
                return found;
            }
            let Ok(canon) = entry.path().canonicalize() else {
                continue;
            };
            if !canon.starts_with(&root) {
                continue;
            }
            let Some(folder) = entry.file_name().into_string().ok() else {
                continue;
            };
            if let Some(skill) = read_skill(&canon, &folder) {
                found.push(skill);
            }
        }
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    found
}

fn read_skill(dir: &Path, folder: &str) -> Option<FoundSkill> {
    let report = inspect_skill_frontmatter(dir);
    if report
        .issues
        .iter()
        .any(|issue| *issue == FrontmatterIssue::UnreadableManifest)
    {
        return None;
    }
    Some(FoundSkill {
        folder: folder.to_string(),
        name: report.name.unwrap_or_else(|| folder.to_string()),
    })
}

fn find_installed_package(from: &Path, name: &str) -> Option<PathBuf> {
    if !valid_npm_package(name) {
        return None;
    }
    let mut dir = from.to_path_buf();
    loop {
        let candidate = match name.split_once('/') {
            Some((scope, package)) => dir.join("node_modules").join(scope).join(package),
            None => dir.join("node_modules").join(name),
        };
        if candidate.join("package.json").is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn valid_npm_package(name: &str) -> bool {
    if name.is_empty() || name.contains('\\') || name.contains(':') || name.contains('\0') {
        return false;
    }
    let (scope, package) = if let Some(rest) = name.strip_prefix('@') {
        let Some((scope, package)) = rest.split_once('/') else {
            return false;
        };
        if scope.is_empty() || package.contains('/') {
            return false;
        }
        (Some(scope), package)
    } else if name.contains('/') {
        return false;
    } else {
        (None, name)
    };
    scope.is_none_or(valid_npm_part) && valid_npm_part(package)
}

fn valid_npm_part(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && part
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~'))
}

fn is_local_source(source: &str) -> bool {
    let source = source.trim();
    source.starts_with('.')
        || source.starts_with('/')
        || source.starts_with('~')
        || source.to_ascii_lowercase().starts_with("file://")
        || looks_like_windows_path(source)
}

fn looks_like_windows_path(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn is_non_git_http(source: &str) -> bool {
    let lower = source.trim().to_ascii_lowercase();
    let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    else {
        return false;
    };
    if rest.contains(".git") {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map(|(_, host)| host).unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    !matches!(
        host,
        "github.com" | "www.github.com" | "gitlab.com" | "www.gitlab.com"
    )
}

fn is_accepted_git(parsed: &Source) -> bool {
    !parsed.well_known
        && !parsed.repo_url.to_ascii_lowercase().starts_with("file:")
        && !is_local_source(&parsed.repo_url)
}

fn entry_parts(value: &Value) -> Option<(String, Vec<String>, Option<String>)> {
    match value {
        Value::String(source) => Some((source.clone(), Vec::new(), None)),
        Value::Object(map) => {
            let source = map.get("source")?.as_str()?.to_string();
            let skills = match map.get("skills") {
                None => Vec::new(),
                Some(Value::Array(items)) => items
                    .iter()
                    .map(|item| item.as_str().map(str::to_string))
                    .collect::<Option<_>>()?,
                Some(_) => return None,
            };
            let git_ref = match map.get("ref") {
                None => None,
                Some(Value::String(value)) => Some(value.clone()),
                Some(_) => return None,
            };
            Some((source, skills, git_ref))
        }
        _ => None,
    }
}

fn read_package_json(path: &Path) -> Result<Value, String> {
    let metadata = std::fs::metadata(path).map_err(|_| "package.json: not found".to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_PACKAGE_JSON_BYTES {
        return Err("package.json: unreadable or exceeds 2 MiB".to_string());
    }
    let text = std::fs::read_to_string(path).map_err(|error| format!("package.json: {error}"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    serde_json::from_str(text).map_err(|_| "package.json: invalid JSON".to_string())
}

#[cfg(test)]
#[path = "package_skills_tests.rs"]
mod tests;

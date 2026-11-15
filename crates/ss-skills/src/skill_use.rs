//! Use one skill without installing it.
//!
//! This reads a local directory, a well-known checkout, or the existing import
//! cache. It does not call the installer, write the skill lock, or deploy to
//! an Agent directory. A remote skill's files are copied to a temporary
//! directory so the prompt can name them; that copy is not a canonical install.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::discovery::DiscoveredSkill;
use crate::source_resolver::Source;

const MAX_USE_FILES: usize = 1000;
const MAX_USE_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedUse {
    pub skill: String,
    pub prompt: String,
    pub support_dir: Option<PathBuf>,
}

pub fn resolve_selector(
    source_selector: Option<&str>,
    option_selector: Option<&str>,
) -> Result<Option<String>> {
    match (source_selector, option_selector) {
        (Some(source), Some(option)) if !source.eq_ignore_ascii_case(option) => {
            bail!(
                "Conflicting skill selectors: source selects \"{source}\" but --skill selects \"{option}\". Provide one selector."
            );
        }
        (Some(_), Some(option)) => Ok(Some(option.to_string())),
        (Some(source), None) => Ok(Some(source.to_string())),
        (None, Some(option)) => Ok(Some(option.to_string())),
        (None, None) => Ok(None),
    }
}

/// Unnamed use skips internal and un-installable skills. A selector includes
/// an internal skill and still rejects one the frontmatter gate blocks.
pub fn select_use_skill<'a>(
    skills: &'a [DiscoveredSkill],
    selector: Option<&str>,
) -> Result<&'a DiscoveredSkill> {
    if skills.is_empty() {
        bail!("No valid skills found. Skills require a SKILL.md with name and description.");
    }
    if let Some(selector) = selector {
        let Some(skill) = skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(selector))
        else {
            bail!("Skill '{selector}' not found. Available: {}", names(skills));
        };
        if !skill.installable {
            bail!(
                "Skill '{}' is not usable: {}",
                skill.id,
                skill.frontmatter_issues.join(", ")
            );
        }
        return Ok(skill);
    }
    let usable = skills
        .iter()
        .filter(|skill| skill.included_in_bulk())
        .collect::<Vec<_>>();
    match usable.as_slice() {
        [only] => Ok(*only),
        [] => {
            bail!("No usable skills found. Internal skills are skipped unless named with --skill.")
        }
        many => bail!(
            "Multiple skills found. Pass --skill <name>. Available: {}",
            many.iter()
                .map(|skill| skill.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub fn build_use_prompt(
    skill_md: &str,
    support_dir: Option<&Path>,
    has_supporting_files: bool,
) -> String {
    let mut sections = vec![
        "You are being given a Skill to execute for the user's next request.".to_string(),
        "Use the following SKILL.md as your instructions:".to_string(),
        format!("<SKILL.md>\n{skill_md}\n</SKILL.md>"),
    ];
    if has_supporting_files && let Some(dir) = support_dir {
        sections.push(format!(
            "Supporting files for this skill were downloaded to:\n{}\n\nWhen the SKILL.md references relative paths, read them from that directory.",
            dir.display()
        ));
    }
    let mut prompt = sections.join("\n\n");
    prompt.push('\n');
    prompt
}

/// Read one skill and build its prompt. Does not install.
pub fn prepare_use(input: &str, skill: Option<&str>, full_depth: bool) -> Result<PreparedUse> {
    if crate::well_known::is_candidate(input) {
        if let Some(acquired) = crate::well_known::acquire(input)? {
            let selector = resolve_selector(acquired.spec.skill_filter.as_deref(), skill)?;
            let (_, _, root, skills) = crate::skill_install::scan_parsed_checkout(
                &acquired.spec,
                acquired.dir().to_path_buf(),
                full_depth,
            );
            return prepare_from_root(&root, &skills, selector.as_deref(), true);
        }
    }
    let parsed = Source::parse(input).context("Invalid source")?;
    let selector = resolve_selector(parsed.skill_filter.as_deref(), skill)?;
    if let Some(root) = file_url_path(&parsed.repo_url) {
        let (_, _, root, skills) =
            crate::skill_install::scan_parsed_checkout(&parsed, root, full_depth);
        return prepare_from_root(&root, &skills, selector.as_deref(), false);
    }
    prepare_remote(input, selector.as_deref(), full_depth)
}

fn prepare_remote(input: &str, selector: Option<&str>, full_depth: bool) -> Result<PreparedUse> {
    let git = crate::git_skill::GitSkillFacade::from_file_store();
    let scan = git.scan_repo(input, full_depth)?;
    let selected = select_use_skill(&scan.skills, selector)?;
    // Import-cache materialization only. Do not call install_from_scan.
    let checkout = crate::fetch::fetch_for_install(
        &scan.spec,
        &[selected.folder_path.as_str()],
        git.session(),
    )?;
    let skill_dir = contained_skill_dir(checkout.dir(), &selected.folder_path)?;
    prepare_from_skill_dir(&skill_dir, &selected.id, true)
}

fn prepare_from_root(
    root: &Path,
    skills: &[DiscoveredSkill],
    selector: Option<&str>,
    copy: bool,
) -> Result<PreparedUse> {
    let selected = select_use_skill(skills, selector)?;
    let skill_dir = contained_skill_dir(root, &selected.folder_path)?;
    prepare_from_skill_dir(&skill_dir, &selected.id, copy)
}

fn prepare_from_skill_dir(skill_dir: &Path, name: &str, copy: bool) -> Result<PreparedUse> {
    let skill_md_path = skill_dir.join("SKILL.md");
    let skill_md = std::fs::read_to_string(&skill_md_path)
        .with_context(|| format!("Failed to read {}", skill_md_path.display()))?;
    let readable = if copy {
        copy_skill_for_use(skill_dir, name)?
    } else {
        skill_dir.to_path_buf()
    };
    let has_supporting_files = has_supporting_files(&readable)?;
    let support_dir = has_supporting_files.then(|| readable.clone());
    Ok(PreparedUse {
        skill: name.to_string(),
        prompt: build_use_prompt(&skill_md, support_dir.as_deref(), has_supporting_files),
        support_dir,
    })
}

fn contained_skill_dir(root: &Path, folder: &str) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .context("Skill checkout is unavailable")?;
    if folder.is_empty() {
        return Ok(root);
    }
    if folder
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        bail!("Unsafe skill path '{folder}'");
    }
    let skill_dir = root.join(folder);
    let skill_dir = skill_dir
        .canonicalize()
        .with_context(|| format!("Skill folder '{folder}' is not in the checkout"))?;
    if !skill_dir.starts_with(&root) {
        bail!("Skill folder '{folder}' escapes the checkout");
    }
    Ok(skill_dir)
}

fn copy_skill_for_use(skill_dir: &Path, name: &str) -> Result<PathBuf> {
    let safe_name = safe_skill_dir_name(name)?;
    let temp_root = std::env::temp_dir().join(format!("skillstar-use-{}", uuid::Uuid::new_v4()));
    let destination = temp_root.join(&safe_name);
    std::fs::create_dir_all(&destination)?;
    match copy_tree(skill_dir, &destination) {
        Ok(()) => Ok(destination),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&temp_root);
            Err(error)
        }
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    let mut files = 0usize;
    let mut bytes = 0u64;
    copy_tree_inner(source, destination, &mut files, &mut bytes)
}

fn copy_tree_inner(
    source: &Path,
    destination: &Path,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<()> {
    for entry in
        std::fs::read_dir(source).with_context(|| format!("Failed to read {}", source.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name == ".git"
            || name == "__pycache__"
            || name == "__pypackages__"
            || name == "metadata.json"
        {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let target = destination.join(name);
        if file_type.is_dir() {
            std::fs::create_dir_all(&target)?;
            copy_tree_inner(&entry.path(), &target, files, bytes)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        *files += 1;
        if *files > MAX_USE_FILES {
            bail!("Skill has too many files to use without installing");
        }
        let len = entry.metadata()?.len();
        *bytes = bytes.saturating_add(len);
        if *bytes > MAX_USE_BYTES {
            bail!("Skill is too large to use without installing");
        }
        std::fs::copy(entry.path(), &target)
            .with_context(|| format!("Failed to copy {}", entry.path().display()))?;
    }
    Ok(())
}

fn has_supporting_files(skill_dir: &Path) -> Result<bool> {
    fn walk(dir: &Path, root: &Path) -> Result<bool> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name == ".git" || name == "SKILL.md" && dir == root {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_file() {
                return Ok(true);
            }
            if file_type.is_dir() && walk(&entry.path(), root)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    walk(skill_dir, skill_dir)
}

fn safe_skill_dir_name(name: &str) -> Result<String> {
    let mut cleaned = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            cleaned.push(ch);
        } else {
            cleaned.push('-');
        }
    }
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." || cleaned.contains("..") {
        bail!("Invalid skill name");
    }
    Ok(cleaned)
}

fn file_url_path(repo_url: &str) -> Option<PathBuf> {
    let rest = repo_url.strip_prefix("file://")?;
    let path = if rest.len() >= 3 && rest.as_bytes()[0] == b'/' && rest.as_bytes()[2] == b':' {
        PathBuf::from(&rest[1..])
    } else {
        PathBuf::from(rest)
    };
    path.is_dir().then_some(path)
}

fn names(skills: &[DiscoveredSkill]) -> String {
    skills
        .iter()
        .map(|skill| skill.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "skill_use_tests.rs"]
mod tests;

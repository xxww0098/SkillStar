//! SKILL.md frontmatter quality validation.
//!
//! Aligned with the open Agent Skills ecosystem: the [agentskills.io
//! specification](https://agentskills.io) (name + description required,
//! description drives agent triggering), the `npx skills` installer (skills
//! without `name`/`description` are not valid), Anthropic's
//! `skill-creator/scripts/quick_validate.py` (length caps, no angle brackets
//! in description), and SkillStar's own install contract.
//!
//! The split is deliberately two-tier, mirroring the ecosystem:
//! - **Blocking** issues make a skill un-installable through the repo install
//!   path and un-exportable through bundles. These are the fields an agent
//!   actually consumes (`description`) or names that would break the hub
//!   (`name` beyond the spec cap).
//! - **Advisory** issues (`name` missing — SkillStar falls back to the
//!   directory name — or not kebab-case) are surfaced to the UI but never
//!   block. Existing documented fallbacks keep working.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Spec cap for `name` (agentskills.io: 1–64 chars).
pub const MAX_NAME_CHARS: usize = 64;
/// Spec cap for `description` (agentskills.io: 1–1024 chars).
pub const MAX_DESCRIPTION_CHARS: usize = 1024;
/// `description` must not contain angle brackets (quick_validate.py ban,
/// prompt-injection guard).
const FORBIDDEN_DESCRIPTION_CHARS: [char; 2] = ['<', '>'];
/// Manifest size guard, shared with discovery (single source of truth).
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Stable snake_case issue code, serialized to the frontend so the UI can
/// map codes to localized labels without mirroring the enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontmatterIssue {
    /// SKILL.md missing, unreadable, or larger than the manifest cap.
    UnreadableManifest,
    /// Content does not start with a `---` frontmatter block.
    MissingFrontmatter,
    /// Frontmatter YAML could not be parsed.
    MalformedFrontmatter,
    /// `name` absent (advisory — SkillStar falls back to the directory name).
    MissingName,
    /// `name` longer than [`MAX_NAME_CHARS`].
    NameTooLong,
    /// `name` is not kebab-case (advisory — many real skills use other forms).
    NameNotKebabCase,
    /// `description` absent or empty — required by the Agent Skills spec.
    MissingDescription,
    /// `description` parsed as a non-string YAML value.
    DescriptionNotString,
    /// `description` longer than [`MAX_DESCRIPTION_CHARS`].
    DescriptionTooLong,
    /// `description` contains `<` or `>`.
    DescriptionHasAngleBrackets,
}

impl FrontmatterIssue {
    /// Machine-stable code for the frontend (snake_case).
    pub fn as_code(self) -> &'static str {
        match self {
            Self::UnreadableManifest => "unreadable_manifest",
            Self::MissingFrontmatter => "missing_frontmatter",
            Self::MalformedFrontmatter => "malformed_frontmatter",
            Self::MissingName => "missing_name",
            Self::NameTooLong => "name_too_long",
            Self::NameNotKebabCase => "name_not_kebab_case",
            Self::MissingDescription => "missing_description",
            Self::DescriptionNotString => "description_not_string",
            Self::DescriptionTooLong => "description_too_long",
            Self::DescriptionHasAngleBrackets => "description_has_angle_brackets",
        }
    }

    /// Issues that make a skill un-installable / un-exportable.
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            Self::UnreadableManifest
                | Self::MalformedFrontmatter
                | Self::NameTooLong
                | Self::MissingDescription
                | Self::DescriptionNotString
                | Self::DescriptionTooLong
                | Self::DescriptionHasAngleBrackets
        )
    }

    /// Human-readable reason for CLI/error surfacing.
    pub fn describe(self) -> &'static str {
        match self {
            Self::UnreadableManifest => "SKILL.md is missing, unreadable, or exceeds 1 MiB",
            Self::MissingFrontmatter => "SKILL.md has no YAML frontmatter block",
            Self::MalformedFrontmatter => "SKILL.md frontmatter is not valid YAML",
            Self::MissingName => "frontmatter has no `name` (falls back to the directory name)",
            Self::NameTooLong => {
                "frontmatter `name` exceeds 64 characters (Agent Skills spec cap)"
            }
            Self::NameNotKebabCase => {
                "frontmatter `name` is not kebab-case (lowercase letters, digits, hyphens)"
            }
            Self::MissingDescription => {
                "frontmatter `description` is missing — agents use it to decide when to trigger the skill"
            }
            Self::DescriptionNotString => "frontmatter `description` must be a string",
            Self::DescriptionTooLong => {
                "frontmatter `description` exceeds 1024 characters (Agent Skills spec cap)"
            }
            Self::DescriptionHasAngleBrackets => {
                "frontmatter `description` must not contain `<` or `>`"
            }
        }
    }
}

/// Parsed frontmatter plus the issues found in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontmatterReport {
    pub name: Option<String>,
    pub description: Option<String>,
    pub issues: Vec<FrontmatterIssue>,
}

impl FrontmatterReport {
    /// No blocking issues — the skill may be installed/exported.
    pub fn is_installable(&self) -> bool {
        !self.issues.iter().any(|issue| issue.is_blocking())
    }

    /// The first blocking issue, if any (stable order: description issues are
    /// the actionable ones, so they surface first).
    pub fn first_blocking(&self) -> Option<FrontmatterIssue> {
        [
            FrontmatterIssue::MissingDescription,
            FrontmatterIssue::DescriptionNotString,
            FrontmatterIssue::DescriptionTooLong,
            FrontmatterIssue::DescriptionHasAngleBrackets,
            FrontmatterIssue::NameTooLong,
            FrontmatterIssue::MalformedFrontmatter,
            FrontmatterIssue::UnreadableManifest,
        ]
        .into_iter()
        .find(|candidate| self.issues.contains(candidate))
    }
}

/// Raw frontmatter values before rule evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    /// The frontmatter declared a `description` key whose value is not a
    /// string (YAML number/boolean/etc.). Distinct from "absent".
    pub description_not_string: bool,
    /// The raw YAML parse error, if the frontmatter could not be parsed.
    pub yaml_error: Option<String>,
    /// Whether the file even had a frontmatter block.
    pub has_frontmatter: bool,
}

/// Parse the `---`-delimited frontmatter of a SKILL.md file, tolerantly.
///
/// This is the single parsing implementation used by both discovery and
/// validation, so the id derivation and the quality report never disagree.
pub(crate) fn parse_frontmatter(content: &str) -> ParsedFrontmatter {
    #[derive(Deserialize)]
    struct Frontmatter {
        name: Option<String>,
        #[serde(default)]
        description: Option<serde_yaml::Value>,
    }

    if !content.starts_with("---") {
        return ParsedFrontmatter {
            name: None,
            description: None,
            description_not_string: false,
            yaml_error: None,
            has_frontmatter: false,
        };
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return ParsedFrontmatter {
            name: None,
            description: None,
            description_not_string: false,
            yaml_error: Some("unterminated frontmatter block".to_string()),
            has_frontmatter: true,
        };
    }

    match serde_yaml::from_str::<Frontmatter>(parts[1]) {
        Ok(fm) => {
            let (description, description_not_string) = match fm.description {
                Some(serde_yaml::Value::String(value)) => (Some(value), false),
                Some(_) => (None, true),
                None => (None, false),
            };
            ParsedFrontmatter {
                name: fm.name.map(|name| name.trim().to_string()),
                description: description.map(|desc| desc.trim().to_string()),
                description_not_string,
                yaml_error: None,
                has_frontmatter: true,
            }
        }
        Err(error) => ParsedFrontmatter {
            name: None,
            description: None,
            description_not_string: false,
            yaml_error: Some(error.to_string()),
            has_frontmatter: true,
        },
    }
}

/// Inspect a skill directory's `SKILL.md` and report frontmatter issues.
///
/// Reads at most [`MAX_MANIFEST_BYTES`]; an oversized/unreadable file yields
/// [`FrontmatterIssue::UnreadableManifest`] rather than failing hard.
pub fn inspect_skill_frontmatter(skill_dir: &Path) -> FrontmatterReport {
    let skill_md = skill_dir.join("SKILL.md");
    let Ok(metadata) = std::fs::symlink_metadata(&skill_md) else {
        return FrontmatterReport {
            name: None,
            description: None,
            issues: vec![FrontmatterIssue::UnreadableManifest],
        };
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return FrontmatterReport {
            name: None,
            description: None,
            issues: vec![FrontmatterIssue::UnreadableManifest],
        };
    }
    let Ok(content) = std::fs::read_to_string(&skill_md) else {
        return FrontmatterReport {
            name: None,
            description: None,
            issues: vec![FrontmatterIssue::UnreadableManifest],
        };
    };

    let parsed = parse_frontmatter(&content);
    let mut issues = Vec::new();

    if !parsed.has_frontmatter {
        issues.push(FrontmatterIssue::MissingFrontmatter);
    } else if parsed.yaml_error.is_some() {
        issues.push(FrontmatterIssue::MalformedFrontmatter);
    }

    match &parsed.name {
        None => issues.push(FrontmatterIssue::MissingName),
        Some(name) => {
            if name.chars().count() > MAX_NAME_CHARS {
                issues.push(FrontmatterIssue::NameTooLong);
            } else if !is_kebab_case(name) {
                issues.push(FrontmatterIssue::NameNotKebabCase);
            }
        }
    }

    match &parsed.description {
        None if parsed.description_not_string => {
            issues.push(FrontmatterIssue::DescriptionNotString);
        }
        None => issues.push(FrontmatterIssue::MissingDescription),
        Some(description) if description.is_empty() => {
            issues.push(FrontmatterIssue::MissingDescription);
        }
        Some(description) => {
            if description.chars().count() > MAX_DESCRIPTION_CHARS {
                issues.push(FrontmatterIssue::DescriptionTooLong);
            }
            if description
                .chars()
                .any(|c| FORBIDDEN_DESCRIPTION_CHARS.contains(&c))
            {
                issues.push(FrontmatterIssue::DescriptionHasAngleBrackets);
            }
        }
    }

    FrontmatterReport {
        name: parsed.name,
        description: parsed.description,
        issues,
    }
}

/// Blocking check used by the repo-install path and bundle export/import.
///
/// Returns a single actionable reason naming the skill directory.
pub fn ensure_installable(skill_dir: &Path) -> Result<(), String> {
    let report = inspect_skill_frontmatter(skill_dir);
    match report.first_blocking() {
        Some(issue) => Err(format!(
            "SKILL.md is not a valid skill: {}",
            issue.describe()
        )),
        None => Ok(()),
    }
}

fn is_kebab_case(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    let mut previous = first;
    for c in chars {
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return false;
        }
        if c == '-' && previous == '-' {
            return false;
        }
        previous = c;
    }
    !name.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_skill(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join("SKILL.md");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn valid_skill_has_no_issues() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(
            dir.path(),
            "---\nname: my-skill\ndescription: Does a useful thing when asked\n---\n\n# Body\n",
        );
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert!(report.is_installable());
    }

    #[test]
    fn missing_description_blocks_install() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(dir.path(), "---\nname: my-skill\n---\n\n# Body\n");
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report.issues.contains(&FrontmatterIssue::MissingDescription));
        assert!(!report.is_installable());
        let error = ensure_installable(dir.path()).unwrap_err();
        assert!(error.contains("description"), "{error}");
    }

    #[test]
    fn bare_skill_md_blocks_and_reports_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(dir.path(), "# No frontmatter at all\n");
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report.issues.contains(&FrontmatterIssue::MissingFrontmatter));
        assert!(report.issues.contains(&FrontmatterIssue::MissingName));
        assert!(report.issues.contains(&FrontmatterIssue::MissingDescription));
        assert!(!report.is_installable());
    }

    #[test]
    fn missing_name_is_advisory_only() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(
            dir.path(),
            "---\ndescription: A valid description\n---\n\n# Body\n",
        );
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report.issues.contains(&FrontmatterIssue::MissingName));
        // Documented fallback: id derives from the directory name, so this
        // stays advisory.
        assert!(report.is_installable());
    }

    #[test]
    fn non_string_description_blocks() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(
            dir.path(),
            "---\nname: my-skill\ndescription: 12345\n---\n",
        );
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report.issues.contains(&FrontmatterIssue::DescriptionNotString));
        assert!(!report.is_installable());
    }

    #[test]
    fn oversized_description_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let long = "x".repeat(MAX_DESCRIPTION_CHARS + 1);
        write_skill(
            dir.path(),
            &format!("---\nname: my-skill\ndescription: {long}\n---\n"),
        );
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report.issues.contains(&FrontmatterIssue::DescriptionTooLong));
        assert!(!report.is_installable());
    }

    #[test]
    fn angle_brackets_in_description_block() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(
            dir.path(),
            "---\nname: my-skill\ndescription: Runs <script>alert(1)</script>\n---\n",
        );
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report
            .issues
            .contains(&FrontmatterIssue::DescriptionHasAngleBrackets));
        assert!(!report.is_installable());
    }

    #[test]
    fn oversize_name_blocks_but_non_kebab_is_advisory() {
        let dir = tempfile::tempdir().unwrap();
        let long_name = "a".repeat(MAX_NAME_CHARS + 1);
        write_skill(
            dir.path(),
            &format!("---\nname: {long_name}\ndescription: ok\n---\n"),
        );
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report.issues.contains(&FrontmatterIssue::NameTooLong));
        assert!(!report.is_installable());

        let dir2 = tempfile::tempdir().unwrap();
        write_skill(
            dir2.path(),
            "---\nname: MySkill_Name\ndescription: ok\n---\n",
        );
        let report2 = inspect_skill_frontmatter(dir2.path());
        assert!(report2.issues.contains(&FrontmatterIssue::NameNotKebabCase));
        assert!(report2.is_installable());
    }

    #[test]
    fn malformed_yaml_blocks() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(
            dir.path(),
            "---\nname: [unclosed\n---\n",
        );
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report
            .issues
            .contains(&FrontmatterIssue::MalformedFrontmatter));
        assert!(!report.is_installable());
    }

    #[test]
    fn oversized_or_missing_manifest_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let report = inspect_skill_frontmatter(dir.path());
        assert!(report
            .issues
            .contains(&FrontmatterIssue::UnreadableManifest));
        assert!(!report.is_installable());
    }

    #[test]
    fn kebab_case_rules() {
        for (name, expected) in [
            ("my-skill", true),
            ("my_skill", false),
            ("MySkill", false),
            ("my skill", false),
            ("-leading", false),
            ("trailing-", false),
            ("double--dash", false),
            ("123", true),
            ("a1-b2", true),
        ] {
            assert_eq!(is_kebab_case(name), expected, "name {name:?}");
        }
    }
}

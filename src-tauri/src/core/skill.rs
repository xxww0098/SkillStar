use serde::{Deserialize, Serialize};
use std::path::Path;

pub use skillstar_marketplace_core::{
    OfficialPublisher, Skill, SkillCategory, SkillType, extract_github_source_from_url,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct SkillContent {
    pub name: String,
    pub description: Option<String>,
    pub triggers: Vec<String>,
    pub scopes: Vec<String>,
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Vec<String>,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct YamlFrontmatter {
    description: Option<String>,
    triggers: Option<Vec<String>>,
    scopes: Option<Vec<String>>,
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<Vec<String>>,
}

pub fn parse_skill_content(name: String, full_content: String) -> SkillContent {
    let (yaml_str, _markdown) = if full_content.starts_with("---") {
        let parts: Vec<&str> = full_content.splitn(3, "---").collect();
        if parts.len() >= 3 {
            (parts[1], parts[2])
        } else {
            ("", full_content.as_str())
        }
    } else {
        ("", full_content.as_str())
    };

    let frontmatter: YamlFrontmatter = if yaml_str.trim().is_empty() {
        YamlFrontmatter {
            description: None,
            triggers: None,
            scopes: None,
            allowed_tools: None,
        }
    } else {
        serde_yaml::from_str(yaml_str).unwrap_or(YamlFrontmatter {
            description: None,
            triggers: None,
            scopes: None,
            allowed_tools: None,
        })
    };

    SkillContent {
        name,
        description: frontmatter.description,
        triggers: frontmatter.triggers.unwrap_or_default(),
        scopes: frontmatter.scopes.unwrap_or_default(),
        allowed_tools: frontmatter.allowed_tools.unwrap_or_default(),
        content: full_content,
    }
}

/// Extract a skill description from SKILL.md frontmatter or README fallback.
pub fn extract_skill_description(skill_dir: &Path) -> String {
    let skill_md = skill_dir.join("SKILL.md");
    let full_content = match std::fs::read_to_string(&skill_md) {
        Ok(content) => content,
        Err(_) => return extract_description_from_readme(skill_dir).unwrap_or_default(),
    };

    let mut lines = full_content.lines();
    if matches!(lines.next().map(str::trim), Some("---")) {
        let mut in_description_block = false;
        let mut description_lines = Vec::new();

        for line in lines.by_ref() {
            let trimmed = line.trim();
            if trimmed == "---" {
                break;
            }

            if in_description_block {
                if line.starts_with(' ') || line.starts_with('\t') {
                    if !trimmed.is_empty() {
                        description_lines.push(trimmed.to_string());
                    }
                    continue;
                }

                if !description_lines.is_empty() {
                    return description_lines.join(" ").trim().to_string();
                }
                in_description_block = false;
            }

            if let Some(rest) = trimmed.strip_prefix("description:") {
                let value = rest.trim().trim_matches('"').trim_matches('\'');
                if value.is_empty() || value.starts_with('>') || value.starts_with('|') {
                    in_description_block = true;
                    continue;
                }

                return value.to_string();
            }
        }

        if !description_lines.is_empty() {
            return description_lines.join(" ").trim().to_string();
        }
    }

    for line in full_content.lines().take(120) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("description:") {
            let description = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !description.is_empty() {
                return description;
            }
        }
    }

    extract_description_from_readme(skill_dir).unwrap_or_default()
}

fn extract_description_from_readme(skill_dir: &Path) -> Option<String> {
    const README_NAMES: &[&str] = &["README.md", "readme.md", "README.MD", "Readme.md"];

    for readme_name in README_NAMES {
        let readme_path = skill_dir.join(readme_name);
        let content = match std::fs::read_to_string(&readme_path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let mut in_code_block = false;
        let mut paragraph: Vec<String> = Vec::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();

            if line.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block {
                continue;
            }

            if line.is_empty() {
                if !paragraph.is_empty() {
                    break;
                }
                continue;
            }

            if line.starts_with('#')
                || line.starts_with("![")
                || line.starts_with("[![")
                || line.contains("shields.io")
                || line.starts_with("<img")
            {
                continue;
            }

            paragraph.push(line.to_string());
            if paragraph.len() >= 3 {
                break;
            }
        }

        if !paragraph.is_empty() {
            return Some(paragraph.join(" "));
        }
    }

    None
}

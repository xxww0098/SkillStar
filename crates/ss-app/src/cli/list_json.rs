//! JSON rows for skillstar list --json.
//!
//! Field names match the skills CLI list output. scope "local" is SkillStar's
//! authored-skill scope; hub skills use "global". agents lists display names
//! of Agents where SkillStar owns a deployment. Agents that read the canonical
//! root directly are omitted because they have no per-skill deployment.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListedSkillJson {
    pub name: String,
    pub path: String,
    pub scope: String,
    pub agents: Vec<String>,
    pub source: Option<String>,
    #[serde(rename = "sourceUrl")]
    pub source_url: Option<String>,
    #[serde(rename = "sourceType")]
    pub source_type: Option<String>,
    pub deploy: String,
}

pub fn listed_skill_json(
    name: &str,
    path: &str,
    scope: &str,
    agents: &[String],
    source: Option<&str>,
    source_url: Option<&str>,
    source_type: Option<&str>,
    deploy: &str,
) -> ListedSkillJson {
    let mut agents = agents.to_vec();
    agents.sort();
    agents.dedup();
    ListedSkillJson {
        name: name.to_string(),
        path: path.to_string(),
        scope: scope.to_string(),
        agents,
        source: nonempty(source),
        source_url: nonempty(source_url),
        source_type: nonempty(source_type),
        deploy: deploy.to_string(),
    }
}

fn nonempty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::listed_skill_json;

    #[test]
    fn list_json_keeps_upstream_field_names() {
        let row = listed_skill_json(
            "demo",
            "/skills/demo",
            "global",
            &[
                "Codex".to_string(),
                "Claude Code".to_string(),
                "Codex".to_string(),
            ],
            Some("owner/repo"),
            Some("https://github.com/owner/repo.git"),
            Some("github"),
            "dir",
        );
        let json = serde_json::to_value(&row).unwrap();
        assert_eq!(json["name"], "demo");
        assert_eq!(json["sourceUrl"], "https://github.com/owner/repo.git");
        assert_eq!(json["sourceType"], "github");
        assert_eq!(json["agents"], serde_json::json!(["Claude Code", "Codex"]));
        assert!(json.get("source_url").is_none());
    }
}

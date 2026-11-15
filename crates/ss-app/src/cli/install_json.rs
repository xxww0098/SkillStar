//! skillstar add --json. The install itself is unchanged; this only
//! suppresses prompts and prints one result object per skill.
//!
//! --json requires --yes or --all so it never asks. It cannot combine with
//! --list or --preview. Security rows are included only for a confirmed-public
//! github.com source; an empty audit is null, not a pass.

use std::path::Path;

use serde::Serialize;
use serde_json::{Value, json};

use super::install::{
    InstallDestination, InstallScope, bulk_selection, deploy_installed_skills, git_skill_facade,
    resolve_install_destination,
};
use super::{AddKind, InstallOpts, classify_add_input};
use ss_skills::repo_scanner;

#[derive(Debug, Serialize)]
struct AddJsonResult {
    name: Option<String>,
    status: &'static str,
    source: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    hash: Option<String>,
    path: Option<String>,
    scope: Option<String>,
    agents: Option<Vec<String>>,
    mode: Option<String>,
    security: Option<Value>,
    reason: Option<String>,
    error: Option<String>,
}

pub fn cmd_install_json(opts: InstallOpts<'_>) {
    if opts.list || opts.preview {
        fail("add --json cannot be combined with --list or --preview");
    }
    if !opts.yes && !opts.all {
        fail("--json requires --yes or --all so it never prompts");
    }
    let destination = match resolve_install_destination(&opts) {
        Ok(destination) => destination,
        Err(error) => fail(&error),
    };
    match classify_add_input(opts.url) {
        AddKind::Bundle(_) => fail("add --json does not install bundles; use the text command"),
        AddKind::LocalDir(path) => install_local_json(&path, &opts, &destination),
        AddKind::Repo => install_repo_json(&opts, &destination),
    }
}

fn install_repo_json(opts: &InstallOpts<'_>, destination: &InstallDestination) {
    let git = match git_skill_facade() {
        Ok(git) => git,
        Err(error) => fail(&error),
    };
    let fetched = match git.scan_repo_with_refresh(opts.url, false, opts.refresh) {
        Ok(fetched) => fetched,
        Err(error) => fail(&error.to_string()),
    };
    if fetched.skills.is_empty() {
        fail("No valid SKILL.md found in the selected source");
    }
    let selected = match select_names(&fetched.skills, opts) {
        Ok(selected) => selected,
        Err(error) => fail(&error),
    };
    let blocked = fetched
        .skills
        .iter()
        .filter(|skill| selected.iter().any(|name| name == &skill.id) && !skill.installable)
        .map(|skill| {
            format!(
                "Skill '{}' is not installable: {}",
                skill.id,
                skill.frontmatter_issues.join(", ")
            )
        })
        .collect::<Vec<_>>();
    if let Some(error) = blocked.first() {
        fail(error);
    }
    let targets = fetched
        .skills
        .iter()
        .filter(|skill| selected.iter().any(|name| name == &skill.id))
        .map(|skill| repo_scanner::SkillInstallTarget {
            id: skill.id.clone(),
            folder_path: skill.folder_path.clone(),
            pinned: false,
        })
        .collect::<Vec<_>>();
    let security = security_for(&fetched.spec.repo_url, &fetched.spec.short, &selected);
    let installed = match git.install_from_scan(&fetched, &targets) {
        Ok(installed) => installed,
        Err(error) => fail(&format!("{error:#}")),
    };
    if installed.is_empty() {
        fail("Nothing was installed");
    }
    if let Err(error) = deploy_installed_skills(&installed, destination) {
        fail(&format!("Installed to hub but failed to deploy: {error}"));
    }
    let lock = ss_skills::skill_lock::load();
    let mut rows = Vec::new();
    for name in &installed {
        let entry = lock.skills.get(name);
        rows.push(result(
            Some(name.clone()),
            "installed",
            Some(opts.url.to_string()),
            fetched.spec.git_ref.clone(),
            entry.and_then(|entry| entry.skill_folder_hash.clone()),
            Some(
                ss_core::infra::paths::hub_skills_dir()
                    .join(name)
                    .display()
                    .to_string(),
            ),
            destination,
            security.get(name).cloned(),
            None,
            None,
        ));
    }
    if opts.all || (opts.yes && opts.name.is_none() && opts.skill.is_empty()) {
        for skill in &fetched.skills {
            if installed.iter().any(|name| name == &skill.id)
                || selected.iter().any(|name| name == &skill.id)
            {
                continue;
            }
            rows.push(result(
                Some(skill.id.clone()),
                "skipped",
                Some(opts.url.to_string()),
                fetched.spec.git_ref.clone(),
                None,
                None,
                destination,
                None,
                Some(if skill.internal {
                    "internal".to_string()
                } else {
                    skill.frontmatter_issues.join(", ")
                }),
                None,
            ));
        }
    }
    emit(&rows, 0);
}

fn install_local_json(path: &Path, opts: &InstallOpts<'_>, destination: &InstallDestination) {
    let names = if opts.all
        || opts.skill.iter().any(|name| name == "*")
        || (opts.yes && opts.name.is_none() && opts.skill.is_empty())
    {
        None
    } else if !opts.skill.is_empty() {
        Some(opts.skill.to_vec())
    } else {
        opts.name.map(|name| vec![name.to_string()])
    };
    let adopted = match ss_skills::local_skill::adopt_folder(&path.to_string_lossy(), names) {
        Ok(adopted) => adopted,
        Err(error) => fail(&error.to_string()),
    };
    if adopted.adopted.is_empty() {
        fail("Nothing was adopted");
    }
    let installed = adopted
        .adopted
        .iter()
        .map(|skill| skill.name.clone())
        .collect::<Vec<_>>();
    if let Err(error) = deploy_installed_skills(&installed, destination) {
        fail(&format!(
            "Adopted into the local store but failed to deploy: {error}"
        ));
    }
    let mut rows = adopted
        .adopted
        .into_iter()
        .map(|skill| {
            result(
                Some(skill.name.clone()),
                "installed",
                Some(path.display().to_string()),
                None,
                None,
                Some(
                    ss_core::infra::paths::local_skills_dir()
                        .join(&skill.name)
                        .display()
                        .to_string(),
                ),
                destination,
                None,
                None,
                None,
            )
        })
        .collect::<Vec<_>>();
    for skipped in adopted.skipped {
        rows.push(result(
            Some(skipped.name),
            "skipped",
            Some(path.display().to_string()),
            None,
            None,
            None,
            destination,
            None,
            Some(skipped.reason),
            None,
        ));
    }
    emit(&rows, 0);
}

fn select_names(
    skills: &[repo_scanner::DiscoveredSkill],
    opts: &InstallOpts<'_>,
) -> Result<Vec<String>, String> {
    if opts.name.is_some() && !opts.skill.is_empty() {
        return Err("--name cannot be combined with --skill".to_string());
    }
    let install_all = opts.all || opts.skill.iter().any(|name| name == "*");
    if install_all || (opts.yes && opts.name.is_none() && opts.skill.is_empty()) {
        let installable = bulk_selection(skills);
        if installable.is_empty() {
            return Err("No installable SKILL.md found in the selected source".to_string());
        }
        return Ok(installable);
    }
    if !opts.skill.is_empty() {
        let mut selected = Vec::new();
        let mut missing = Vec::new();
        for requested in opts.skill {
            match skills
                .iter()
                .find(|skill| skill.id.eq_ignore_ascii_case(requested))
            {
                Some(skill) if !selected.contains(&skill.id) => selected.push(skill.id.clone()),
                Some(_) => {}
                None => missing.push(requested.clone()),
            }
        }
        if !missing.is_empty() {
            return Err(format!(
                "Requested skills not found: {}",
                missing.join(", ")
            ));
        }
        return Ok(selected);
    }
    if let Some(name) = opts.name {
        let skill = skills
            .iter()
            .find(|skill| skill.id.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("Skill '{name}' not found"))?;
        return Ok(vec![skill.id.clone()]);
    }
    if skills.len() == 1 {
        return Ok(vec![skills[0].id.clone()]);
    }
    Err("Multiple skills found. Pass --skill <name> or --all.".to_string())
}

fn security_for(
    repo_url: &str,
    short: &str,
    names: &[String],
) -> std::collections::HashMap<String, Value> {
    if names.is_empty() || !crate::install_audit::may_query_install_audits(repo_url) {
        return std::collections::HashMap::new();
    }
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return std::collections::HashMap::new();
    };
    let audits = runtime.block_on(crate::install_audit::fetch_public_github_skill_audits(
        repo_url, names,
    ));
    security_json(short, &audits)
}

pub(crate) fn security_json(
    short: &str,
    audits: &[ss_marketplace::SkillSecurityAudit],
) -> std::collections::HashMap<String, Value> {
    let mut by_skill = std::collections::HashMap::new();
    if audits.is_empty() {
        return by_skill;
    }
    for audit in audits {
        let mut row = serde_json::Map::new();
        for item in &audit.audits {
            let key = match item.name.as_str() {
                "Gen" => "gen",
                "Socket" => "socket",
                "Snyk" => "snyk",
                _ => continue,
            };
            row.insert(key.to_string(), json!(risk_value(&item.result)));
        }
        if row.is_empty() {
            continue;
        }
        if looks_like_owner_repo(short) {
            row.insert(
                "details".to_string(),
                json!(format!("https://skills.sh/{short}")),
            );
        }
        by_skill.insert(audit.skill.clone(), Value::Object(row));
    }
    by_skill
}

fn risk_value(label: &str) -> String {
    match label {
        "Safe" => "safe".to_string(),
        "Low Risk" => "low".to_string(),
        "Medium Risk" => "medium".to_string(),
        "High Risk" => "high".to_string(),
        "Critical" => "critical".to_string(),
        "Unknown" => "unknown".to_string(),
        other => other.to_string(),
    }
}

fn looks_like_owner_repo(short: &str) -> bool {
    let mut parts = short.split('/');
    let owner = parts.next().unwrap_or("");
    let repo = parts.next().unwrap_or("");
    parts.next().is_none() && !owner.is_empty() && !repo.is_empty() && !short.starts_with("local/")
}

fn result(
    name: Option<String>,
    status: &'static str,
    source: Option<String>,
    git_ref: Option<String>,
    hash: Option<String>,
    path: Option<String>,
    destination: &InstallDestination,
    security: Option<Value>,
    reason: Option<String>,
    error: Option<String>,
) -> AddJsonResult {
    AddJsonResult {
        name,
        status,
        source,
        git_ref,
        hash,
        path,
        scope: Some(
            match destination.scope {
                InstallScope::Global => "global",
                InstallScope::Project(_) => "project",
            }
            .to_string(),
        ),
        agents: Some(destination.agent_ids.clone()),
        mode: Some(
            match destination.mode {
                ss_skills::projects::ProjectDeployMode::Symlink => "symlink",
                ss_skills::projects::ProjectDeployMode::Copy => "copy",
            }
            .to_string(),
        ),
        security,
        reason,
        error,
    }
}

fn emit(rows: &[AddJsonResult], code: i32) -> ! {
    match serde_json::to_string_pretty(rows) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("add --json: {error}"),
    }
    std::process::exit(code);
}

fn fail(error: &str) -> ! {
    let rows = vec![AddJsonResult {
        name: None,
        status: "failed",
        source: None,
        git_ref: None,
        hash: None,
        path: None,
        scope: None,
        agents: None,
        mode: None,
        security: None,
        reason: None,
        error: Some(error.to_string()),
    }];
    emit(&rows, 1);
}

#[cfg(test)]
mod tests {
    use super::security_json;
    use ss_marketplace::{SecurityAudit, SkillSecurityAudit};

    #[test]
    fn empty_audit_is_not_a_pass() {
        assert!(security_json("owner/repo", &[]).is_empty());
    }

    #[test]
    fn audit_json_uses_upstream_risk_words() {
        let audits = vec![SkillSecurityAudit {
            skill: "demo".to_string(),
            audits: vec![
                SecurityAudit {
                    name: "Gen".into(),
                    result: "Low Risk".into(),
                },
                SecurityAudit {
                    name: "Socket".into(),
                    result: "0 alerts".into(),
                },
            ],
        }];
        let json = security_json("owner/repo", &audits);
        let demo = json.get("demo").unwrap();
        assert_eq!(demo["gen"], "low");
        assert_eq!(demo["socket"], "0 alerts");
        assert_eq!(demo["details"], "https://skills.sh/owner/repo");
        assert!(json.get("other").is_none());
    }
}

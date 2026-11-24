use nsh_core::JsonValue;
use nsh_core::element::__private::JsonMap;
use ss_skills::git::gh_manager::{self, GhStatus, GitStatus};
use ss_skills::github_auth::{CredentialStore, FileCredentialStore};

fn obj(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
    let mut map = JsonMap::<String, JsonValue>::new();
    for (key, value) in pairs {
        map.insert(key.to_owned(), value);
    }
    JsonValue::Object(map)
}

fn str(value: &str) -> JsonValue {
    JsonValue::String(value.to_owned())
}

fn sanitize_repo_name(skill_name: &str) -> String {
    let sanitized: String = skill_name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches(['.', '-']).to_owned();
    if trimmed.is_empty() {
        "my-skills".to_owned()
    } else {
        trimmed.chars().take(100).collect()
    }
}

fn validate_repo_name(repo_name: &str) -> Result<String, String> {
    let trimmed = repo_name.trim();
    if trimmed.is_empty() || trimmed.len() > 100 || trimmed.starts_with('.') {
        return Err("Repository name must be 1–100 characters and cannot start with '.'".to_owned());
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(
            "Repository name may only contain letters, digits, '-', '_', and '.'".to_owned(),
        );
    }
    Ok(trimmed.to_owned())
}

fn read_local_skill_meta(skill_name: &str) -> (bool, String) {
    if ss_skills::content::validate_skill_name(skill_name).is_err() {
        return (false, String::new());
    }
    let skill_path = ss_core::infra::paths::hub_skills_dir().join(skill_name);
    if !skill_path.exists() {
        return (false, String::new());
    }
    let resolved = if ss_core::infra::fs_ops::is_link(&skill_path) {
        ss_core::infra::fs_ops::read_link_resolved(&skill_path).unwrap_or(skill_path)
    } else {
        skill_path
    };
    let report = ss_skills::validation::inspect_skill_frontmatter(&resolved);
    (true, report.description.unwrap_or_default())
}

pub(super) fn publish_preflight_sync(skill_name: &str) -> Result<JsonValue, String> {
    let trimmed = skill_name.trim();
    ss_skills::content::validate_skill_name(trimmed).map_err(|err| format!("{err:#}"))?;
    let gh_status = gh_manager::check_status();
    let git_status = gh_manager::check_git_status();
    let (skill_exists, suggested_description) = read_local_skill_meta(trimmed);
    let has_stored_credential = matches!(FileCredentialStore::default().load(), Ok(Some(_)));
    Ok(publish_preflight_json(
        trimmed,
        gh_status,
        git_status,
        skill_exists,
        &suggested_description,
        has_stored_credential,
    ))
}

fn publish_preflight_json(
    skill_name: &str,
    gh_status: GhStatus,
    git_status: GitStatus,
    skill_exists: bool,
    suggested_description: &str,
    has_stored_credential: bool,
) -> JsonValue {
    let (status, username, git_installed, authenticated, auth_state) = match &gh_status {
        GhStatus::NotInstalled => (
            "not_installed",
            None,
            false,
            false,
            if has_stored_credential {
                "unknown"
            } else {
                "signed_out"
            },
        ),
        GhStatus::NotAuthenticated => (
            "not_authenticated",
            None,
            true,
            false,
            if has_stored_credential {
                "expired"
            } else {
                "signed_out"
            },
        ),
        GhStatus::Ready { username } => ("ready", Some(username.clone()), true, true, "connected"),
    };
    let (git_download_url, git_instructions) = match git_status {
        GitStatus::Installed { .. } => (JsonValue::Null, Vec::new()),
        GitStatus::NotInstalled {
            download_url,
            install_instructions,
            ..
        } => (
            str(&download_url),
            install_instructions
                .into_iter()
                .map(|item| obj(vec![("label", str(&item.label)), ("command", str(&item.command))]))
                .collect(),
        ),
    };
    obj(vec![
        ("status", str(status)),
        (
            "username",
            match username {
                Some(name) => str(&name),
                None => JsonValue::Null,
            },
        ),
        ("authState", str(auth_state)),
        ("gitInstalled", JsonValue::Bool(git_installed)),
        ("authenticated", JsonValue::Bool(authenticated)),
        ("gitDownloadUrl", git_download_url),
        ("gitInstallInstructions", JsonValue::Array(git_instructions)),
        ("skillName", str(skill_name)),
        ("skillExists", JsonValue::Bool(skill_exists)),
        ("suggestedRepo", str(&sanitize_repo_name(skill_name))),
        ("suggestedDescription", str(suggested_description)),
    ])
}

pub(super) fn publish_skill_sync(
    skill_name: &str,
    repo_name: &str,
    is_private: bool,
    description: &str,
) -> Result<JsonValue, String> {
    let skill = skill_name.trim();
    ss_skills::content::validate_skill_name(skill).map_err(|err| format!("{err:#}"))?;
    let repo = validate_repo_name(repo_name)?;
    let desc = description.trim();
    let result = gh_manager::publish_skill(
        skill,
        &repo,
        desc,
        !is_private,
        None,
        skill,
        &ss_skills::skill_lock::lock_path(),
    )
    .map_err(|err| format!("{err:#}"))?;
    Ok(obj(vec![
        ("url", str(&result.url)),
        ("gitUrl", str(&result.git_url)),
        ("sourceFolder", str(&result.source_folder)),
        ("isPrivate", JsonValue::Bool(is_private)),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_preflight_json_surfaces_all_three_gh_status_states() {
        let missing_git = publish_preflight_json(
            "demo-skill",
            GhStatus::NotInstalled,
            GitStatus::NotInstalled {
                os: "macOS".into(),
                install_instructions: vec![gh_manager::GitInstallInstruction {
                    label: "Homebrew".into(),
                    command: "brew install git".into(),
                }],
                download_url: "https://git-scm.com/downloads/mac".into(),
            },
            true,
            "Demo desc",
            false,
        );
        assert_eq!(missing_git["status"].as_str(), Some("not_installed"));
        assert_eq!(missing_git["gitInstalled"].as_bool(), Some(false));
        assert_eq!(
            missing_git["gitInstallInstructions"][0]["command"].as_str(),
            Some("brew install git")
        );

        let signed_out = publish_preflight_json(
            "demo skill",
            GhStatus::NotAuthenticated,
            GitStatus::Installed {
                version: "2.44.0".into(),
            },
            true,
            "",
            true,
        );
        assert_eq!(signed_out["status"].as_str(), Some("not_authenticated"));
        assert_eq!(signed_out["authState"].as_str(), Some("expired"));
        assert_eq!(signed_out["suggestedRepo"].as_str(), Some("demo-skill"));

        let ready = publish_preflight_json(
            "demo-skill",
            GhStatus::Ready {
                username: "octocat".into(),
            },
            GitStatus::Installed {
                version: "2.44.0".into(),
            },
            true,
            "Hello world",
            true,
        );
        assert_eq!(ready["status"].as_str(), Some("ready"));
        assert_eq!(ready["username"].as_str(), Some("octocat"));
        assert_eq!(ready["suggestedDescription"].as_str(), Some("Hello world"));
    }
}

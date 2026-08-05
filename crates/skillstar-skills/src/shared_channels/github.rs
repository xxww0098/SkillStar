use super::{
    GitHubOrganization, RemoteRepository, RepositoryPermissions, SharedChannelError,
    SharedChannelErrorCode, SharedChannelGateway,
};
use crate::github_auth::GitHubApiCredential;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

const API_ROOT: &str = "https://api.github.com";

pub struct ProductionSharedChannelGateway {
    credential: GitHubApiCredential,
}

impl ProductionSharedChannelGateway {
    pub fn new(credential: GitHubApiCredential) -> Self {
        Self { credential }
    }

    fn client(&self) -> Result<reqwest::Client, SharedChannelError> {
        skillstar_core::infra::http_client::probe_http_client(Duration::from_secs(30)).map_err(
            |_| {
                SharedChannelError::new(
                    SharedChannelErrorCode::Network,
                    "Unable to create the GitHub client; check SkillStar proxy settings",
                )
            },
        )
    }

    fn request(
        &self,
        method: reqwest::Method,
        url: &str,
    ) -> Result<reqwest::RequestBuilder, SharedChannelError> {
        Ok(self
            .client()?
            .request(method, url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header(reqwest::header::USER_AGENT, "SkillStar")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .bearer_auth(self.credential.expose_secret()))
    }

    async fn response(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<(u16, String), SharedChannelError> {
        let response = request.send().await.map_err(|_| network_error())?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(|_| network_error())?;
        Ok((status, body))
    }

    async fn organization_installation(
        &self,
        organization_id: u64,
    ) -> Result<UserInstallation, SharedChannelError> {
        let mut page = 1usize;
        loop {
            let (status, body) = self
                .response(self.request(reqwest::Method::GET, &user_installations_url(page))?)
                .await?;
            ensure_status(status, &[200])?;
            let response: InstallationsResponse = parse_json(&body)?;
            let count = response.installations.len();
            if let Some(installation) = response.installations.into_iter().find(|installation| {
                installation.account.id == organization_id
                    && installation
                        .account
                        .kind
                        .eq_ignore_ascii_case("organization")
            }) {
                return Ok(installation);
            }
            if page.saturating_mul(100) >= response.total_count || count == 0 {
                break;
            }
            page += 1;
        }
        Err(SharedChannelError::new(
            SharedChannelErrorCode::AppNotInstalled,
            "Install the SkillStar GitHub App for the selected organization, then retry",
        ))
    }

    async fn selected_repositories(
        &self,
        installation_id: u64,
    ) -> Result<Vec<RemoteRepository>, SharedChannelError> {
        let mut repositories = Vec::new();
        let mut page = 1usize;
        loop {
            let (status, body) = self
                .response(self.request(
                    reqwest::Method::GET,
                    &installation_repositories_url(installation_id, page),
                )?)
                .await?;
            if status == 404 {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::AppNotInstalled,
                    "The SkillStar GitHub App installation is no longer accessible; reinstall it for the organization, then retry",
                ));
            }
            ensure_status(status, &[200])?;
            let response: AccessibleRepositoriesResponse = parse_json(&body)?;
            let count = response.repositories.len();
            repositories.extend(
                response
                    .repositories
                    .into_iter()
                    .map(map_repository)
                    .collect::<Result<Vec<_>, _>>()?,
            );
            if page.saturating_mul(100) >= response.total_count || count == 0 {
                return Ok(repositories);
            }
            page += 1;
        }
    }
}

#[derive(Deserialize)]
struct OrganizationMembership {
    role: String,
    state: String,
    organization: OrganizationAccount,
}

#[derive(Deserialize)]
struct OrganizationAccount {
    id: u64,
    login: String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct RepositoryResponse {
    id: u64,
    name: String,
    html_url: String,
    clone_url: String,
    private: bool,
    owner: RepositoryOwner,
    #[serde(default)]
    permissions: RepositoryPermissionResponse,
}

#[derive(Deserialize)]
struct RepositoryOwner {
    id: u64,
    login: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Default, Deserialize)]
struct RepositoryPermissionResponse {
    #[serde(default)]
    admin: bool,
    #[serde(default)]
    maintain: bool,
    #[serde(default)]
    push: bool,
    #[serde(default)]
    pull: bool,
}

#[derive(Deserialize)]
struct InstallationsResponse {
    total_count: usize,
    installations: Vec<UserInstallation>,
}

#[derive(Deserialize)]
struct UserInstallation {
    id: u64,
    repository_selection: String,
    account: InstallationAccount,
    #[serde(default)]
    permissions: InstallationPermissions,
}

#[derive(Default, Deserialize)]
struct InstallationPermissions {
    administration: Option<String>,
    contents: Option<String>,
}

#[derive(Deserialize)]
struct InstallationAccount {
    id: u64,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct AccessibleRepositoriesResponse {
    total_count: usize,
    repositories: Vec<RepositoryResponse>,
}

#[async_trait]
impl SharedChannelGateway for ProductionSharedChannelGateway {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        let mut organizations = Vec::new();
        let mut page = 1usize;
        loop {
            let (status, body) = self
                .response(self.request(
                    reqwest::Method::GET,
                    &format!(
                        "{API_ROOT}/user/memberships/orgs?state=active&per_page=100&page={page}"
                    ),
                )?)
                .await?;
            ensure_status(status, &[200])?;
            let memberships: Vec<OrganizationMembership> = parse_json(&body)?;
            let count = memberships.len();
            organizations.extend(
                memberships
                    .into_iter()
                    .filter(|membership| membership.state == "active")
                    .map(|membership| GitHubOrganization {
                        id: membership.organization.id,
                        login: membership.organization.login,
                        avatar_url: membership.organization.avatar_url,
                        viewer_is_admin: membership.role == "admin",
                    }),
            );
            if count < 100 {
                return Ok(organizations);
            }
            page += 1;
        }
    }

    async fn list_selected_repositories(
        &self,
        organization_id: u64,
    ) -> Result<Vec<RemoteRepository>, SharedChannelError> {
        let installation = self.organization_installation(organization_id).await?;
        validate_installation_contract(&installation)?;
        self.selected_repositories(installation.id).await
    }

    async fn create_private_repository(
        &self,
        organization: &str,
        name: &str,
        description: &str,
    ) -> Result<RemoteRepository, SharedChannelError> {
        let organization = encode_path_segment(organization)?;
        let body = serde_json::json!({
            "name": name,
            "description": description,
            "private": true,
            "auto_init": true,
        });
        let request = self
            .request(
                reqwest::Method::POST,
                &format!("{API_ROOT}/orgs/{organization}/repos"),
            )?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string());
        let (status, body) = self.response(request).await?;
        match status {
            201 => map_repository(parse_json(&body)?),
            401 => Err(not_authenticated()),
            403 => Err(permission_denied()),
            422 => Err(SharedChannelError::new(
                SharedChannelErrorCode::RepositoryConflict,
                "GitHub could not create the repository; the name may already exist",
            )),
            _ => Err(protocol_status(status)),
        }
    }

    async fn validate_selected_installation(
        &self,
        organization_id: u64,
    ) -> Result<(), SharedChannelError> {
        let installation = self.organization_installation(organization_id).await?;
        validate_installation_contract(&installation)
    }

    async fn get_selected_repository(
        &self,
        organization_id: u64,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        let installation = self.organization_installation(organization_id).await?;
        validate_installation_contract(&installation)?;
        self.selected_repositories(installation.id)
            .await?
            .into_iter()
            .find(|repository| repository.id == repository_id)
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::AppRepositoryAccessRequired,
                    "Select this repository in the SkillStar GitHub App installation, then retry",
                )
            })
    }
}

fn validate_installation_contract(
    installation: &UserInstallation,
) -> Result<(), SharedChannelError> {
    if installation.repository_selection != "selected" {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::AppRepositorySelectionRequired,
            "Configure the SkillStar GitHub App for selected repositories, then retry",
        ));
    }
    if installation.permissions.administration.as_deref() != Some("write")
        || installation.permissions.contents.as_deref() != Some("write")
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::PermissionDenied,
            "The SkillStar GitHub App installation requires Administration write and Contents write",
        ));
    }
    Ok(())
}

fn user_installations_url(page: usize) -> String {
    format!("{API_ROOT}/user/installations?per_page=100&page={page}")
}

fn installation_repositories_url(installation_id: u64, page: usize) -> String {
    format!("{API_ROOT}/user/installations/{installation_id}/repositories?per_page=100&page={page}")
}

fn map_repository(response: RepositoryResponse) -> Result<RemoteRepository, SharedChannelError> {
    Ok(RemoteRepository {
        id: response.id,
        owner_id: response.owner.id,
        owner_login: response.owner.login,
        owner_type: response.owner.kind,
        name: response.name,
        html_url: response.html_url,
        clone_url: response.clone_url,
        private: response.private,
        permissions: RepositoryPermissions {
            admin: response.permissions.admin,
            maintain: response.permissions.maintain,
            push: response.permissions.push,
            pull: response.permissions.pull,
        },
    })
}

fn parse_json<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, SharedChannelError> {
    serde_json::from_str(body).map_err(|_| {
        SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "GitHub returned an invalid shared-channel response",
        )
    })
}

fn ensure_status(status: u16, expected: &[u16]) -> Result<(), SharedChannelError> {
    if expected.contains(&status) {
        Ok(())
    } else {
        match status {
            401 => Err(not_authenticated()),
            403 => Err(permission_denied()),
            _ => Err(protocol_status(status)),
        }
    }
}

fn encode_path_segment(value: &str) -> Result<String, SharedChannelError> {
    if value.is_empty()
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "GitHub organization login contains unsupported characters",
        ));
    }
    Ok(value.to_string())
}

fn network_error() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Network,
        "Unable to reach GitHub; check the network and SkillStar proxy settings",
    )
}

fn not_authenticated() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::NotAuthenticated,
        "GitHub rejected the current session; refresh it or sign in again",
    )
}

fn permission_denied() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::PermissionDenied,
        "GitHub requires organization owner and repository Administration write access",
    )
}

fn protocol_status(status: u16) -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Protocol,
        format!("GitHub shared-channel request failed with status {status}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installation(
        repository_selection: &str,
        administration: Option<&str>,
        contents: Option<&str>,
    ) -> UserInstallation {
        UserInstallation {
            id: 9,
            repository_selection: repository_selection.into(),
            account: InstallationAccount {
                id: 7,
                kind: "Organization".into(),
            },
            permissions: InstallationPermissions {
                administration: administration.map(str::to_owned),
                contents: contents.map(str::to_owned),
            },
        }
    }

    #[test]
    fn selected_installation_requires_both_write_permissions() {
        assert!(
            validate_installation_contract(&installation("selected", Some("write"), Some("write")))
                .is_ok()
        );

        for candidate in [
            installation("all", Some("write"), Some("write")),
            installation("selected", Some("read"), Some("write")),
            installation("selected", Some("write"), None),
        ] {
            assert!(validate_installation_contract(&candidate).is_err());
        }
    }

    #[test]
    fn repository_verification_uses_the_supported_paginated_list_endpoint() {
        assert_eq!(
            installation_repositories_url(9, 2),
            "https://api.github.com/user/installations/9/repositories?per_page=100&page=2"
        );
        assert!(!installation_repositories_url(9, 2).contains("repositories/"));
    }
}

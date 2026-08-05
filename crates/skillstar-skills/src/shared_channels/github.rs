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
    installations: Vec<UserInstallation>,
}

#[derive(Deserialize)]
struct UserInstallation {
    id: u64,
    repository_selection: String,
    account: InstallationAccount,
}

#[derive(Deserialize)]
struct InstallationAccount {
    id: u64,
    #[serde(rename = "type")]
    kind: String,
}

#[async_trait]
impl SharedChannelGateway for ProductionSharedChannelGateway {
    async fn list_organizations(&self) -> Result<Vec<GitHubOrganization>, SharedChannelError> {
        let (status, body) = self
            .response(self.request(
                reqwest::Method::GET,
                &format!("{API_ROOT}/user/memberships/orgs?state=active&per_page=100"),
            )?)
            .await?;
        ensure_status(status, &[200])?;
        let memberships: Vec<OrganizationMembership> = parse_json(&body)?;
        Ok(memberships
            .into_iter()
            .filter(|membership| membership.state == "active")
            .map(|membership| GitHubOrganization {
                id: membership.organization.id,
                login: membership.organization.login,
                avatar_url: membership.organization.avatar_url,
                viewer_is_admin: membership.role == "admin",
            })
            .collect())
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

    async fn get_repository(
        &self,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        let (status, body) = self
            .response(self.request(
                reqwest::Method::GET,
                &format!("{API_ROOT}/repositories/{repository_id}"),
            )?)
            .await?;
        match status {
            200 => map_repository(parse_json(&body)?),
            401 => Err(not_authenticated()),
            403 => Err(permission_denied()),
            404 => Err(SharedChannelError::new(
                SharedChannelErrorCode::RepositoryNotFound,
                "The shared channel repository no longer exists or is inaccessible",
            )),
            _ => Err(protocol_status(status)),
        }
    }

    async fn authorize_selected_repository(
        &self,
        organization_id: u64,
        repository_id: u64,
    ) -> Result<(), SharedChannelError> {
        let (status, body) = self
            .response(self.request(
                reqwest::Method::GET,
                &format!("{API_ROOT}/user/installations?per_page=100"),
            )?)
            .await?;
        ensure_status(status, &[200])?;
        let installations: InstallationsResponse = parse_json(&body)?;
        let installation = installations
            .installations
            .into_iter()
            .find(|installation| {
                installation.account.id == organization_id
                    && installation
                        .account
                        .kind
                        .eq_ignore_ascii_case("organization")
            })
            .ok_or_else(|| {
                SharedChannelError::new(
                    SharedChannelErrorCode::AppNotInstalled,
                    "Install the SkillStar GitHub App for the selected organization, then retry",
                )
            })?;
        if installation.repository_selection != "selected" {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::AppRepositorySelectionRequired,
                "Configure the SkillStar GitHub App for selected repositories, then retry",
            ));
        }

        let endpoint = format!(
            "{API_ROOT}/user/installations/{}/repositories/{repository_id}",
            installation.id
        );
        let (status, _) = self
            .response(self.request(reqwest::Method::PUT, &endpoint)?)
            .await?;
        match status {
            204 => {}
            401 => return Err(not_authenticated()),
            403 => return Err(permission_denied()),
            404 => {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::AppNotInstalled,
                    "Authorize the SkillStar GitHub App for this repository, then retry",
                ));
            }
            _ => return Err(protocol_status(status)),
        }

        let (status, _) = self
            .response(self.request(reqwest::Method::GET, &endpoint)?)
            .await?;
        ensure_status(status, &[200])
    }
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

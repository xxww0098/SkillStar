use super::release::parse_revision_tag;
use super::{
    ChannelPublicationGateway, ChannelPublisherIdentity, ChannelReleaseManifest,
    GitHubOrganization, RemoteChannelRelease, RemoteRepository, RepositoryPermissions,
    SharedChannelError, SharedChannelErrorCode, SharedChannelGateway, validate_manifest,
};
use crate::github_auth::GitHubApiCredential;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

pub(super) const API_ROOT: &str = "https://api.github.com";
const MAX_GITHUB_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PUBLICATION_PAGES: usize = 50;
const MAX_CHANNEL_RELEASES: usize = 4_096;

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

    pub(super) fn request(
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

    pub(super) async fn response(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<(u16, String), SharedChannelError> {
        let mut response = request.send().await.map_err(|_| network_error())?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|length| length > MAX_GITHUB_RESPONSE_BYTES as u64)
        {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Protocol,
                "GitHub returned a shared-channel response above the supported size limit",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| network_error())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_GITHUB_RESPONSE_BYTES {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::Protocol,
                    "GitHub returned a shared-channel response above the supported size limit",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(bytes).map_err(|_| {
            SharedChannelError::new(
                SharedChannelErrorCode::Protocol,
                "GitHub returned a non-UTF-8 shared-channel response",
            )
        })?;
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
pub(super) struct RepositoryResponse {
    id: u64,
    name: String,
    default_branch: String,
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

#[derive(Deserialize)]
struct GitHubUserResponse {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct GitObjectReference {
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct GitReferenceResponse {
    #[serde(rename = "ref")]
    reference: String,
    object: GitObjectReference,
}

#[derive(Deserialize)]
struct GitTagResponse {
    tag: String,
    message: String,
    object: GitObjectReference,
}

#[derive(Deserialize)]
struct CreatedGitTagResponse {
    sha: String,
}

#[derive(Deserialize)]
struct GitHubReleaseResponse {
    id: u64,
    html_url: String,
    target_commitish: String,
}

#[derive(Deserialize)]
struct GitHubReleaseListItem {
    id: u64,
    html_url: String,
    tag_name: String,
    target_commitish: String,
    name: Option<String>,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
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

#[async_trait]
impl ChannelPublicationGateway for ProductionSharedChannelGateway {
    async fn publisher_identity(&self) -> Result<ChannelPublisherIdentity, SharedChannelError> {
        let (status, body) = self
            .response(self.request(reqwest::Method::GET, &format!("{API_ROOT}/user"))?)
            .await?;
        ensure_status(status, &[200])?;
        let user: GitHubUserResponse = parse_json(&body)?;
        Ok(ChannelPublisherIdentity {
            id: user.id,
            login: user.login,
        })
    }

    async fn head_commit(
        &self,
        repository: &RemoteRepository,
    ) -> Result<String, SharedChannelError> {
        let url = format!(
            "{}/git/ref/heads/{}",
            repository_api_base(repository)?,
            encode_ref_segment(&repository.default_branch)?
        );
        let (status, body) = self
            .response(self.request(reqwest::Method::GET, &url)?)
            .await?;
        ensure_status(status, &[200])?;
        let reference: GitReferenceResponse = parse_json(&body)?;
        if reference.object.kind != "commit" {
            return Err(publication_integrity_error(
                "GitHub default branch did not resolve to a commit",
            ));
        }
        Ok(reference.object.sha)
    }

    async fn published_manifests(
        &self,
        repository: &RemoteRepository,
    ) -> Result<Vec<ChannelReleaseManifest>, SharedChannelError> {
        let api_base = repository_api_base(repository)?;
        let mut manifests = Vec::new();
        let mut page = 1usize;
        loop {
            if page > MAX_PUBLICATION_PAGES {
                return Err(publication_integrity_error(
                    "The channel release history exceeds the supported page limit",
                ));
            }
            let url = format!("{api_base}/releases?per_page=100&page={page}");
            let (status, body) = self
                .response(self.request(reqwest::Method::GET, &url)?)
                .await?;
            ensure_status(status, &[200])?;
            let releases: Vec<GitHubReleaseListItem> = parse_json(&body)?;
            let count = releases.len();
            for release in releases {
                if !release.tag_name.starts_with("channel-v") {
                    continue;
                }
                if release.draft
                    || release.prerelease
                    || parse_revision_tag(&release.tag_name).is_none()
                {
                    return Err(publication_integrity_error(
                        "A channel Release has an invalid immutable publication identity",
                    ));
                }
                let ref_url = format!(
                    "{api_base}/git/ref/tags/{}",
                    encode_ref_segment(&release.tag_name)?
                );
                let (status, body) = self
                    .response(self.request(reqwest::Method::GET, &ref_url)?)
                    .await?;
                ensure_publication_object_status(status)?;
                let reference: GitReferenceResponse = parse_json(&body)?;
                if reference.object.kind != "tag" {
                    return Err(publication_integrity_error(
                        "A channel revision tag is not an immutable annotated tag",
                    ));
                }
                let tag_url = format!("{api_base}/git/tags/{}", reference.object.sha);
                let (status, body) = self
                    .response(self.request(reqwest::Method::GET, &tag_url)?)
                    .await?;
                ensure_publication_object_status(status)?;
                let tag: GitTagResponse = parse_json(&body)?;
                if tag.object.kind != "commit" {
                    return Err(publication_integrity_error(
                        "A channel revision tag does not target a commit",
                    ));
                }
                let manifest: ChannelReleaseManifest =
                    serde_json::from_str(&tag.message).map_err(|_| {
                        publication_integrity_error(
                            "A channel revision tag contains an invalid release manifest",
                        )
                    })?;
                validate_manifest(&manifest, repository.id, repository.owner_id)?;
                if tag.tag != manifest.tag_name
                    || reference.reference != format!("refs/tags/{}", manifest.tag_name)
                    || tag.object.sha != manifest.commit_sha
                    || release.tag_name != manifest.tag_name
                    || release.name.as_deref() != Some(manifest.title.as_str())
                    || release.body.as_deref().unwrap_or_default() != manifest.notes
                    || release.target_commitish != manifest.commit_sha
                    || release.id == 0
                    || release.html_url
                        != format!("{}/releases/tag/{}", repository.html_url, manifest.tag_name)
                {
                    return Err(publication_integrity_error(
                        "A channel Release does not match its immutable release manifest",
                    ));
                }
                manifests.push(manifest);
                if manifests.len() > MAX_CHANNEL_RELEASES {
                    return Err(publication_integrity_error(
                        "The channel release history exceeds the supported revision limit",
                    ));
                }
            }
            if count < 100 {
                manifests.sort_by_key(|manifest| manifest.revision);
                return Ok(manifests);
            }
            page += 1;
        }
    }

    async fn highest_reserved_revision(
        &self,
        repository: &RemoteRepository,
    ) -> Result<u64, SharedChannelError> {
        let api_base = repository_api_base(repository)?;
        let mut highest = 0;
        let mut seen = 0usize;
        let mut page = 1usize;
        loop {
            if page > MAX_PUBLICATION_PAGES {
                return Err(publication_integrity_error(
                    "The reserved channel revision history exceeds the supported page limit",
                ));
            }
            let url =
                format!("{api_base}/git/matching-refs/tags/channel-v?per_page=100&page={page}");
            let (status, body) = self
                .response(self.request(reqwest::Method::GET, &url)?)
                .await?;
            ensure_status(status, &[200])?;
            let references: Vec<GitReferenceResponse> = parse_json(&body)?;
            let count = references.len();
            for reference in references {
                seen = seen.saturating_add(1);
                if seen > MAX_CHANNEL_RELEASES {
                    return Err(publication_integrity_error(
                        "The reserved channel revision history exceeds the supported limit",
                    ));
                }
                let tag = reference
                    .reference
                    .strip_prefix("refs/tags/")
                    .and_then(parse_revision_tag)
                    .ok_or_else(|| {
                        publication_integrity_error(
                            "A reserved channel revision tag has an invalid identity",
                        )
                    })?;
                highest = highest.max(tag);
            }
            if count < 100 {
                return Ok(highest);
            }
            page += 1;
        }
    }

    async fn publish_immutable(
        &self,
        repository: &RemoteRepository,
        manifest: &ChannelReleaseManifest,
    ) -> Result<RemoteChannelRelease, SharedChannelError> {
        validate_manifest(manifest, repository.id, repository.owner_id)?;
        let api_base = repository_api_base(repository)?;
        let tag_request = self
            .request(reqwest::Method::POST, &format!("{api_base}/git/tags"))?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(create_tag_body(manifest)?.to_string());
        let (status, body) = self.response(tag_request).await?;
        if status != 201 {
            return Err(publication_status_error(status, &body));
        }
        let tag: CreatedGitTagResponse = parse_json(&body)?;

        let ref_request = self
            .request(reqwest::Method::POST, &format!("{api_base}/git/refs"))?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(create_ref_body(manifest, &tag.sha).to_string());
        let (status, body) = self.response(ref_request).await?;
        if status != 201 {
            return Err(publication_status_error(status, &body));
        }

        let release_request = self
            .request(reqwest::Method::POST, &format!("{api_base}/releases"))?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(create_release_body(manifest).to_string());
        let (status, body) = self.response(release_request).await?;
        if status != 201 {
            // Keep the tag as a reserved revision. A concurrent publisher may
            // have created a Release from this exact ref before GitHub returned
            // a conflict, so deleting it here could corrupt a valid release.
            return Err(publication_status_error(status, &body));
        }
        let release: GitHubReleaseResponse = parse_json(&body)?;
        let expected_url = format!("{}/releases/tag/{}", repository.html_url, manifest.tag_name);
        if release.id == 0
            || release.html_url != expected_url
            || release.target_commitish != manifest.commit_sha
        {
            return Err(publication_integrity_error(
                "GitHub returned an unexpected channel Release identity",
            ));
        }
        Ok(RemoteChannelRelease {
            id: release.id,
            html_url: release.html_url,
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

pub(super) fn map_repository(
    response: RepositoryResponse,
) -> Result<RemoteRepository, SharedChannelError> {
    Ok(RemoteRepository {
        id: response.id,
        owner_id: response.owner.id,
        owner_login: response.owner.login,
        owner_type: response.owner.kind,
        name: response.name,
        default_branch: response.default_branch,
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

pub(super) fn parse_json<T: serde::de::DeserializeOwned>(
    body: &str,
) -> Result<T, SharedChannelError> {
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

pub(super) fn repository_api_base(
    repository: &RemoteRepository,
) -> Result<String, SharedChannelError> {
    Ok(format!(
        "{API_ROOT}/repos/{}/{}",
        encode_path_segment(&repository.owner_login)?,
        encode_repository_segment(&repository.name)?
    ))
}

fn encode_repository_segment(value: &str) -> Result<String, SharedChannelError> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "GitHub repository name contains unsupported characters",
        ));
    }
    Ok(value.to_string())
}

fn encode_ref_segment(value: &str) -> Result<String, SharedChannelError> {
    if value.is_empty()
        || value.starts_with('-')
        || value.contains("..")
        || value.chars().any(char::is_control)
    {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "GitHub ref contains unsupported characters",
        ));
    }
    Ok(url::form_urlencoded::byte_serialize(value.as_bytes()).collect())
}

fn create_tag_body(
    manifest: &ChannelReleaseManifest,
) -> Result<serde_json::Value, SharedChannelError> {
    let message = serde_json::to_string(manifest).map_err(|_| {
        SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "Unable to serialize the channel release manifest",
        )
    })?;
    Ok(serde_json::json!({
        "tag": manifest.tag_name,
        "message": message,
        "object": manifest.commit_sha,
        "type": "commit",
    }))
}

fn create_ref_body(manifest: &ChannelReleaseManifest, tag_object_sha: &str) -> serde_json::Value {
    serde_json::json!({
        "ref": format!("refs/tags/{}", manifest.tag_name),
        "sha": tag_object_sha,
    })
}

fn create_release_body(manifest: &ChannelReleaseManifest) -> serde_json::Value {
    serde_json::json!({
        "tag_name": manifest.tag_name,
        "target_commitish": manifest.commit_sha,
        "name": manifest.title,
        "body": manifest.notes,
        "draft": false,
        "prerelease": false,
    })
}

fn publication_status_error(status: u16, body: &str) -> SharedChannelError {
    let lower = body.to_ascii_lowercase();
    if matches!(status, 403 | 422) && lower.contains("workflow") {
        return SharedChannelError::new(
            SharedChannelErrorCode::WorkflowPermissionRequired,
            "GitHub refused the channel release because workflow authorization is not granted; SkillStar does not request Workflows write",
        );
    }
    match status {
        401 => not_authenticated(),
        403 => SharedChannelError::new(
            SharedChannelErrorCode::PermissionDenied,
            "GitHub refused channel publishing for the current repository permission",
        ),
        404 => SharedChannelError::new(
            SharedChannelErrorCode::RepositoryNotFound,
            "GitHub could not find the channel repository while publishing",
        ),
        409 | 422 => SharedChannelError::new(
            SharedChannelErrorCode::ReleaseConflict,
            "The channel revision already exists or conflicts with another publisher; scan again",
        ),
        _ => protocol_status(status),
    }
}

fn publication_integrity_error(message: &str) -> SharedChannelError {
    SharedChannelError::new(SharedChannelErrorCode::Integrity, message)
}

fn ensure_publication_object_status(status: u16) -> Result<(), SharedChannelError> {
    if status == 404 {
        Err(publication_integrity_error(
            "A published channel Release references a deleted tag or tag object",
        ))
    } else {
        ensure_status(status, &[200])
    }
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
    use crate::shared_channels::{
        ChannelPublisherIdentity, ChannelReleaseSkill, ChannelSkillReleaseStatus,
    };

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

    #[test]
    fn publication_request_contracts_bind_ref_release_and_manifest_without_workflows() {
        let manifest = fixture_manifest();
        let tag = create_tag_body(&manifest).unwrap();
        assert_eq!(tag["tag"], "channel-v000001");
        assert_eq!(tag["object"], manifest.commit_sha);
        assert_eq!(tag["type"], "commit");
        let decoded: ChannelReleaseManifest =
            serde_json::from_str(tag["message"].as_str().unwrap()).unwrap();
        assert_eq!(decoded, manifest);

        let reference = create_ref_body(&manifest, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        assert_eq!(reference["ref"], "refs/tags/channel-v000001");
        assert_eq!(reference["sha"], "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");

        let release = create_release_body(&manifest);
        assert_eq!(release["tag_name"], "channel-v000001");
        assert_eq!(release["target_commitish"], manifest.commit_sha);
        assert_eq!(release["name"], "Writing tools");
        assert_eq!(release["body"], "Initial release");
        assert!(release.get("workflows").is_none());
        assert_eq!(
            encode_ref_segment("feature/draft").unwrap(),
            "feature%2Fdraft"
        );
    }

    #[test]
    fn workflow_rejection_has_a_dedicated_error_contract() {
        let error = publication_status_error(
            422,
            r#"{"message":"refusing to update workflow without workflows permission"}"#,
        );
        assert_eq!(
            error.code,
            SharedChannelErrorCode::WorkflowPermissionRequired
        );
        assert!(error.message.contains("does not request Workflows write"));
    }

    #[test]
    fn deleted_release_tag_is_classified_as_integrity_failure() {
        let error = ensure_publication_object_status(404).unwrap_err();
        assert_eq!(error.code, SharedChannelErrorCode::Integrity);
        assert!(error.message.contains("deleted tag"));
    }

    #[test]
    fn release_response_contract_preserves_the_exact_target_commit() {
        let response: GitHubReleaseResponse = parse_json(
            r#"{"id":501,"html_url":"https://github.com/acme/shared/releases/tag/channel-v000001","target_commitish":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
        )
        .unwrap();
        assert_eq!(
            response.target_commitish,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    fn fixture_manifest() -> ChannelReleaseManifest {
        ChannelReleaseManifest {
            schema_version: 1,
            repository_id: 42,
            organization_id: 7,
            revision: 1,
            tag_name: "channel-v000001".into(),
            commit_sha: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            publisher: ChannelPublisherIdentity {
                id: 99,
                login: "alice".into(),
            },
            published_at: "2026-08-05T00:00:00Z".into(),
            title: "Writing tools".into(),
            notes: "Initial release".into(),
            skills: vec![ChannelReleaseSkill {
                id: "writer".into(),
                content_root: "skills/writer".into(),
                content_hash:
                    "sha256:6e8b30c29c269c5375c2149f4834f8f6d289e5842b6d75f0f912749605a537f7".into(),
                content_hash_version: 2,
                status: ChannelSkillReleaseStatus::Added,
            }],
        }
    }
}

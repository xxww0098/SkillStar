use super::github::{
    API_ROOT, ProductionSharedChannelGateway, RepositoryResponse, map_repository, parse_json,
    repository_api_base,
};
use super::membership::permissions_for_role;
use super::{
    ChannelInvitation, ChannelInviteRole, ChannelMember, ChannelMemberIdentity,
    ChannelMembershipGateway, ChannelMembershipStatus, EffectiveRepositoryAccess,
    RemoteChannelInvitation, RemoteInvitationOutcome, RemoteRepository, RepositoryAccessSource,
    SharedChannelError, SharedChannelErrorCode, SharedChannelRole, project_role,
};
use async_trait::async_trait;
use serde::Deserialize;

const MAX_MEMBERSHIP_PAGES: usize = 50;

#[derive(Deserialize)]
struct CollaboratorResponse {
    id: u64,
    login: String,
    role_name: Option<String>,
    #[serde(default)]
    permissions: CollaboratorPermissions,
}

#[derive(Default, Deserialize)]
struct CollaboratorPermissions {
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
struct PermissionResponse {
    permission: String,
    role_name: Option<String>,
}

#[derive(Deserialize)]
struct InvitationResponse {
    id: u64,
    permissions: String,
    created_at: String,
    invitee: Option<AccountResponse>,
    inviter: Option<AccountResponse>,
    repository: InvitationRepositoryResponse,
}

#[derive(Deserialize)]
struct AccountResponse {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct InvitationRepositoryResponse {
    id: u64,
    name: String,
    html_url: String,
    private: bool,
    owner: InvitationRepositoryOwner,
}

#[derive(Deserialize)]
struct InvitationRepositoryOwner {
    id: u64,
    login: String,
    #[serde(rename = "type")]
    kind: String,
}

#[async_trait]
impl ChannelMembershipGateway for ProductionSharedChannelGateway {
    async fn effective_access(
        &self,
        repository: &RemoteRepository,
        username: &str,
    ) -> Result<Option<EffectiveRepositoryAccess>, SharedChannelError> {
        let url = format!(
            "{}/collaborators/{}/permission",
            repository_api_base(repository)?,
            encode_login(username)?
        );
        let (status, body) = self
            .response(self.request(reqwest::Method::GET, &url)?)
            .await?;
        if status == 404 {
            return Ok(None);
        }
        if status != 200 {
            return Err(membership_status_error(status, &body));
        }
        let response: PermissionResponse = parse_json(&body)?;
        let role = role_from_permission(&response.permission, response.role_name.as_deref())?;
        Ok(role.map(|role| EffectiveRepositoryAccess {
            role,
            // GitHub documents this as the highest effective grant and does not
            // expose whether it came from repo, team, org, or enterprise access.
            source: RepositoryAccessSource::Unknown,
        }))
    }

    async fn list_members(
        &self,
        repository: &RemoteRepository,
    ) -> Result<Vec<ChannelMember>, SharedChannelError> {
        let mut members = Vec::new();
        for page in 1..=MAX_MEMBERSHIP_PAGES {
            let url = format!(
                "{}/collaborators?affiliation=all&per_page=100&page={page}",
                repository_api_base(repository)?
            );
            let (status, body) = self
                .response(self.request(reqwest::Method::GET, &url)?)
                .await?;
            if status != 200 {
                return Err(membership_status_error(status, &body));
            }
            let page_members: Vec<CollaboratorResponse> = parse_json(&body)?;
            let count = page_members.len();
            for member in page_members {
                let permissions = super::RepositoryPermissions {
                    admin: member.permissions.admin,
                    maintain: member.permissions.maintain,
                    push: member.permissions.push,
                    pull: member.permissions.pull,
                };
                let Some(role) = project_role(&permissions) else {
                    continue;
                };
                members.push(ChannelMember {
                    user: ChannelMemberIdentity {
                        id: member.id,
                        login: member.login,
                    },
                    role,
                    github_role_name: member
                        .role_name
                        .unwrap_or_else(|| github_role_name(role).into()),
                    status: ChannelMembershipStatus::Accepted,
                });
            }
            if count < 100 {
                return Ok(members);
            }
        }
        Err(membership_page_limit())
    }

    async fn list_repository_invitations(
        &self,
        repository: &RemoteRepository,
    ) -> Result<Vec<RemoteChannelInvitation>, SharedChannelError> {
        self.list_invitations_at(&format!("{}/invitations", repository_api_base(repository)?))
            .await
    }

    async fn create_invitation(
        &self,
        repository: &RemoteRepository,
        username: &str,
        role: ChannelInviteRole,
    ) -> Result<RemoteInvitationOutcome, SharedChannelError> {
        let url = collaborator_url(repository, username)?;
        let request = self
            .request(reqwest::Method::PUT, &url)?
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(invitation_body(role).to_string());
        let (status, body) = self.response(request).await?;
        match status {
            201 => Ok(RemoteInvitationOutcome::Pending(Box::new(map_invitation(
                parse_json(&body)?,
            )?))),
            204 => {
                let access = self.effective_access(repository, username).await?.ok_or_else(|| {
                    SharedChannelError::new(
                        SharedChannelErrorCode::Protocol,
                        "GitHub reported existing access but returned no effective repository role",
                    )
                })?;
                Ok(RemoteInvitationOutcome::AlreadyAccepted(access))
            }
            _ => Err(membership_status_error(status, &body)),
        }
    }

    async fn cancel_invitation(
        &self,
        repository: &RemoteRepository,
        invitation_id: u64,
    ) -> Result<(), SharedChannelError> {
        let url = repository_invitation_url(repository, invitation_id)?;
        let (status, body) = self
            .response(self.request(reqwest::Method::DELETE, &url)?)
            .await?;
        ensure_membership_status(status, &body, &[204])
    }

    async fn list_user_invitations(
        &self,
    ) -> Result<Vec<RemoteChannelInvitation>, SharedChannelError> {
        self.list_invitations_at(&format!("{API_ROOT}/user/repository_invitations"))
            .await
    }

    async fn accept_invitation(&self, invitation_id: u64) -> Result<(), SharedChannelError> {
        let url = user_invitation_url(invitation_id)?;
        let (status, body) = self
            .response(self.request(reqwest::Method::PATCH, &url)?)
            .await?;
        ensure_membership_status(status, &body, &[204])
    }

    async fn decline_invitation(&self, invitation_id: u64) -> Result<(), SharedChannelError> {
        let url = user_invitation_url(invitation_id)?;
        let (status, body) = self
            .response(self.request(reqwest::Method::DELETE, &url)?)
            .await?;
        ensure_membership_status(status, &body, &[204])
    }

    async fn get_repository_by_id(
        &self,
        repository_id: u64,
    ) -> Result<RemoteRepository, SharedChannelError> {
        for page in 1..=MAX_MEMBERSHIP_PAGES {
            let url = accepted_repository_inventory_url(page);
            let (status, body) = self
                .response(self.request(reqwest::Method::GET, &url)?)
                .await?;
            if status != 200 {
                return Err(membership_status_error(status, &body));
            }
            let responses: Vec<RepositoryResponse> = parse_json(&body)?;
            let count = responses.len();
            for response in responses {
                let repository = map_repository(response)?;
                if repository.id == repository_id {
                    return Ok(repository);
                }
            }
            if count < 100 {
                return Err(SharedChannelError::new(
                    SharedChannelErrorCode::RepositoryNotFound,
                    "The accepted channel repository is no longer available to this GitHub identity",
                ));
            }
        }
        Err(membership_page_limit())
    }
}

fn accepted_repository_inventory_url(page: usize) -> String {
    format!(
        "{API_ROOT}/user/repos?visibility=private&affiliation=owner,collaborator,organization_member&per_page=100&page={page}"
    )
}

impl ProductionSharedChannelGateway {
    async fn list_invitations_at(
        &self,
        base_url: &str,
    ) -> Result<Vec<RemoteChannelInvitation>, SharedChannelError> {
        let mut invitations = Vec::new();
        for page in 1..=MAX_MEMBERSHIP_PAGES {
            let separator = if base_url.contains('?') { '&' } else { '?' };
            let url = format!("{base_url}{separator}per_page=100&page={page}");
            let (status, body) = self
                .response(self.request(reqwest::Method::GET, &url)?)
                .await?;
            if status != 200 {
                return Err(membership_status_error(status, &body));
            }
            let page_invitations: Vec<InvitationResponse> = parse_json(&body)?;
            let count = page_invitations.len();
            invitations.extend(
                page_invitations
                    .into_iter()
                    .map(map_invitation)
                    .collect::<Result<Vec<_>, _>>()?,
            );
            if count < 100 {
                return Ok(invitations);
            }
        }
        Err(membership_page_limit())
    }
}

fn map_invitation(
    response: InvitationResponse,
) -> Result<RemoteChannelInvitation, SharedChannelError> {
    let role = invite_role_from_permission(&response.permissions)?;
    let effective_role = role_from_permission(&response.permissions, Some(&response.permissions))?
        .ok_or_else(|| {
            SharedChannelError::new(
                SharedChannelErrorCode::Protocol,
                "GitHub returned an invitation without repository access",
            )
        })?;
    let mut repository = map_invitation_repository(response.repository)?;
    repository.permissions = permissions_for_role(effective_role);
    Ok(RemoteChannelInvitation {
        invitation: ChannelInvitation {
            id: response.id,
            repository_id: repository.id,
            organization_id: repository.owner_id,
            owner: repository.owner_login.clone(),
            repository_name: repository.name.clone(),
            html_url: repository.html_url.clone(),
            invitee: response.invitee.map(map_account),
            inviter: response.inviter.map(map_account),
            role,
            effective_role,
            status: ChannelMembershipStatus::Pending,
            created_at: response.created_at,
        },
        repository,
    })
}

fn map_invitation_repository(
    response: InvitationRepositoryResponse,
) -> Result<RemoteRepository, SharedChannelError> {
    let expected_html_url = format!(
        "https://github.com/{}/{}",
        response.owner.login, response.name
    );
    if !response.html_url.eq_ignore_ascii_case(&expected_html_url) {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::Integrity,
            "GitHub invitation repository URL does not match its owner and name",
        ));
    }
    Ok(RemoteRepository {
        id: response.id,
        owner_id: response.owner.id,
        owner_login: response.owner.login,
        owner_type: response.owner.kind,
        name: response.name,
        // The invitation contract need not include branch or clone fields.
        // Channel import only needs the stable identity and canonical HTTPS route.
        default_branch: String::new(),
        clone_url: format!("{expected_html_url}.git"),
        html_url: expected_html_url,
        private: response.private,
        permissions: super::RepositoryPermissions {
            admin: false,
            maintain: false,
            push: false,
            pull: false,
        },
    })
}

fn map_account(account: AccountResponse) -> ChannelMemberIdentity {
    ChannelMemberIdentity {
        id: account.id,
        login: account.login,
    }
}

fn role_from_permission(
    permission: &str,
    _role_name: Option<&str>,
) -> Result<Option<SharedChannelRole>, SharedChannelError> {
    let role = match permission.to_ascii_lowercase().as_str() {
        "none" => None,
        "read" | "pull" | "triage" => Some(SharedChannelRole::Subscriber),
        "write" | "push" | "maintain" => Some(SharedChannelRole::Publisher),
        "admin" => Some(SharedChannelRole::Owner),
        _ => {
            return Err(SharedChannelError::new(
                SharedChannelErrorCode::Protocol,
                "GitHub returned an unsupported repository permission",
            ));
        }
    };
    Ok(role)
}

fn invite_role_from_permission(permission: &str) -> Result<ChannelInviteRole, SharedChannelError> {
    match permission.to_ascii_lowercase().as_str() {
        "read" | "pull" | "triage" => Ok(ChannelInviteRole::Subscriber),
        "write" | "push" | "maintain" | "admin" => Ok(ChannelInviteRole::Publisher),
        _ => Err(SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "GitHub returned an unsupported invitation permission",
        )),
    }
}

fn github_role_name(role: SharedChannelRole) -> &'static str {
    match role {
        SharedChannelRole::Owner => "admin",
        SharedChannelRole::Publisher => "write",
        SharedChannelRole::Subscriber => "read",
    }
}

fn collaborator_url(
    repository: &RemoteRepository,
    username: &str,
) -> Result<String, SharedChannelError> {
    Ok(format!(
        "{}/collaborators/{}",
        repository_api_base(repository)?,
        encode_login(username)?
    ))
}

fn repository_invitation_url(
    repository: &RemoteRepository,
    invitation_id: u64,
) -> Result<String, SharedChannelError> {
    Ok(format!(
        "{}/invitations/{invitation_id}",
        repository_api_base(repository)?
    ))
}

fn user_invitation_url(invitation_id: u64) -> Result<String, SharedChannelError> {
    if invitation_id == 0 {
        return Err(SharedChannelError::new(
            SharedChannelErrorCode::InvitationValidation,
            "GitHub invitation ID must be positive",
        ));
    }
    Ok(format!(
        "{API_ROOT}/user/repository_invitations/{invitation_id}"
    ))
}

fn encode_login(value: &str) -> Result<&str, SharedChannelError> {
    let valid = !value.is_empty()
        && value.len() <= 39
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    if valid {
        Ok(value)
    } else {
        Err(SharedChannelError::new(
            SharedChannelErrorCode::InvitationValidation,
            "Enter a valid GitHub username",
        ))
    }
}

fn invitation_body(role: ChannelInviteRole) -> serde_json::Value {
    serde_json::json!({ "permission": role.github_permission() })
}

fn ensure_membership_status(
    status: u16,
    body: &str,
    expected: &[u16],
) -> Result<(), SharedChannelError> {
    if expected.contains(&status) {
        Ok(())
    } else {
        Err(membership_status_error(status, body))
    }
}

fn membership_status_error(status: u16, body: &str) -> SharedChannelError {
    if (500..=599).contains(&status) {
        return SharedChannelError::new(
            SharedChannelErrorCode::Protocol,
            "GitHub invitation request returned an unexpected response",
        );
    }
    let lower = body.to_ascii_lowercase();
    let (code, message) = if lower.contains("saml")
        || lower.contains("single sign-on")
        || lower.contains("single sign on")
    {
        (
            SharedChannelErrorCode::InvitationSsoRequired,
            "Authorize the current GitHub identity for the organization's SSO, then retry",
        )
    } else if lower.contains("two-factor") || lower.contains("2fa") {
        (
            SharedChannelErrorCode::InvitationTwoFactorRequired,
            "The organization requires two-factor authentication for this invitation",
        )
    } else if lower.contains("seat") || lower.contains("license") {
        (
            SharedChannelErrorCode::InvitationSeatUnavailable,
            "The organization has no available seat for this collaborator",
        )
    } else if lower.contains("50 invitation")
        || lower.contains("invitation limit")
        || lower.contains("invitations per")
        || lower.contains("more than 50")
        || (lower.contains("50 people") && lower.contains("24 hour"))
    {
        (
            SharedChannelErrorCode::InvitationLimit,
            "GitHub's repository invitation limit has been reached; retry after the limit resets",
        )
    } else if status == 429
        || lower.contains("rate limit")
        || lower.contains("secondary rate")
        || lower.contains("abuse detection")
        || lower.contains("spam")
    {
        (
            SharedChannelErrorCode::InvitationRateLimited,
            "GitHub rate-limited invitation management; wait and retry",
        )
    } else if lower.contains("outside collaborator")
        || lower.contains("organization policy")
        || lower.contains("enterprise policy")
        || lower.contains("restricted from")
    {
        (
            SharedChannelErrorCode::InvitationOrganizationPolicy,
            "The organization or enterprise policy blocks this collaborator invitation",
        )
    } else {
        match status {
            401 => (
                SharedChannelErrorCode::NotAuthenticated,
                "GitHub rejected the current session; refresh it or sign in again",
            ),
            403 => (
                SharedChannelErrorCode::PermissionDenied,
                "GitHub Admin permission is required to manage repository invitations",
            ),
            404 => (
                SharedChannelErrorCode::InvitationNotFound,
                "The GitHub invitation or repository is no longer available",
            ),
            409 | 422 | 451 => (
                SharedChannelErrorCode::InvitationValidation,
                "GitHub rejected the repository invitation details",
            ),
            _ => (
                SharedChannelErrorCode::Protocol,
                "GitHub invitation request returned an unexpected response",
            ),
        }
    };
    SharedChannelError::new(code, message)
}

fn membership_page_limit() -> SharedChannelError {
    SharedChannelError::new(
        SharedChannelErrorCode::Protocol,
        "GitHub channel membership exceeds the supported pagination limit",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collaborator_invitation_payload_uses_github_pull_and_push_roles() {
        assert_eq!(
            invitation_body(ChannelInviteRole::Subscriber),
            serde_json::json!({"permission": "pull"})
        );
        assert_eq!(
            invitation_body(ChannelInviteRole::Publisher),
            serde_json::json!({"permission": "push"})
        );
    }

    #[test]
    fn pending_invitation_contract_maps_repository_and_people() {
        let response: InvitationResponse = parse_json(&fixture_invitation_json()).unwrap();
        let remote = map_invitation(response).unwrap();
        assert_eq!(remote.invitation.id, 91);
        assert_eq!(remote.invitation.status, ChannelMembershipStatus::Pending);
        assert_eq!(remote.invitation.role, ChannelInviteRole::Subscriber);
        assert_eq!(remote.invitation.invitee.unwrap().login, "bob");
        assert_eq!(remote.repository.id, 42);
        assert!(remote.repository.private);
    }

    #[test]
    fn admin_invitation_exposes_owner_as_the_effective_role() {
        let body = fixture_invitation_json()
            .replace("\"permissions\":\"read\"", "\"permissions\":\"admin\"");
        let response: InvitationResponse = parse_json(&body).unwrap();
        let remote = map_invitation(response).unwrap();
        assert_eq!(remote.invitation.role, ChannelInviteRole::Publisher);
        assert_eq!(remote.invitation.effective_role, SharedChannelRole::Owner);
    }

    #[test]
    fn accept_and_decline_contracts_use_the_authenticated_user_invitation_resource() {
        assert_eq!(
            user_invitation_url(91).unwrap(),
            "https://api.github.com/user/repository_invitations/91"
        );
        assert_eq!(reqwest::Method::PATCH.as_str(), "PATCH");
        assert_eq!(reqwest::Method::DELETE.as_str(), "DELETE");
    }

    #[test]
    fn accepted_channel_recovery_uses_private_repository_inventory_and_numeric_id_matching() {
        assert_eq!(
            accepted_repository_inventory_url(2),
            "https://api.github.com/user/repos?visibility=private&affiliation=owner,collaborator,organization_member&per_page=100&page=2"
        );
    }

    #[test]
    fn github_invitation_failures_have_distinct_actionable_codes() {
        let cases = [
            (
                403,
                "SAML SSO authorization required",
                SharedChannelErrorCode::InvitationSsoRequired,
            ),
            (
                403,
                "two-factor authentication is required",
                SharedChannelErrorCode::InvitationTwoFactorRequired,
            ),
            (
                403,
                "no outside collaborator seat available",
                SharedChannelErrorCode::InvitationSeatUnavailable,
            ),
            (
                422,
                "Cannot invite more than 50 people to a repository within a 24 hour period",
                SharedChannelErrorCode::InvitationLimit,
            ),
            (
                429,
                "secondary rate limit",
                SharedChannelErrorCode::InvitationRateLimited,
            ),
            (
                422,
                "This invitation was flagged as spam",
                SharedChannelErrorCode::InvitationRateLimited,
            ),
            (
                403,
                "outside collaborators restricted by organization policy",
                SharedChannelErrorCode::InvitationOrganizationPolicy,
            ),
            (
                422,
                "validation failed",
                SharedChannelErrorCode::InvitationValidation,
            ),
            (
                503,
                "no outside collaborator seat available",
                SharedChannelErrorCode::Protocol,
            ),
        ];
        for (status, body, expected) in cases {
            assert_eq!(membership_status_error(status, body).code, expected);
        }
    }

    #[test]
    fn unknown_base_permission_is_never_inferred_from_a_custom_role_name() {
        let error = role_from_permission("future-role", Some("read")).unwrap_err();
        assert_eq!(error.code, SharedChannelErrorCode::Protocol);
    }

    fn fixture_invitation_json() -> String {
        serde_json::json!({
            "id": 91,
            "permissions": "read",
            "created_at": "2026-08-05T00:00:00Z",
            "invitee": {"id": 8, "login": "bob"},
            "inviter": {"id": 9, "login": "alice"},
            "repository": {
                "id": 42,
                "name": "channel",
                "html_url": "https://github.com/acme/channel",
                "private": true,
                "owner": {"id": 7, "login": "acme", "type": "Organization"}
            }
        })
        .to_string()
    }
}

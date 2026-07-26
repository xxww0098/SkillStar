use super::super::*;
use agent_client_protocol::{self as acp, Client as _};
use std::sync::{Arc, Mutex};

fn test_client(policy: AcpAccessPolicy) -> SkillStarClient {
    SkillStarClient::new(
        Arc::new(Mutex::new(String::new())),
        |_| {},
        std::env::temp_dir(),
        policy,
    )
}

fn permission_options() -> Vec<acp::PermissionOption> {
    vec![
        acp::PermissionOption::new(
            "allow_once",
            "Allow Once",
            acp::PermissionOptionKind::AllowOnce,
        ),
        acp::PermissionOption::new(
            "allow_always",
            "Always Allow",
            acp::PermissionOptionKind::AllowAlways,
        ),
        acp::PermissionOption::new(
            "reject_once",
            "Reject",
            acp::PermissionOptionKind::RejectOnce,
        ),
    ]
}

fn permission_request(
    kind: Option<acp::ToolKind>,
    title: Option<&str>,
) -> acp::RequestPermissionRequest {
    let mut fields = acp::ToolCallUpdateFields::new();
    if let Some(kind) = kind {
        fields = fields.kind(kind);
    }
    if let Some(title) = title {
        fields = fields.title(title.to_string());
    }
    acp::RequestPermissionRequest::new(
        "session-1",
        acp::ToolCallUpdate::new("tool-1", fields),
        permission_options(),
    )
}

fn selected_permission_id(response: acp::RequestPermissionResponse) -> Option<String> {
    match response.outcome {
        acp::RequestPermissionOutcome::Selected(selected) => {
            Some(selected.option_id.0.as_ref().to_string())
        }
        acp::RequestPermissionOutcome::Cancelled => None,
        _ => None,
    }
}

#[tokio::test]
async fn read_only_permission_allows_safe_kinds_once() {
    let client = test_client(AcpAccessPolicy::ReadOnly);

    for kind in [
        acp::ToolKind::Read,
        acp::ToolKind::Search,
        acp::ToolKind::Think,
    ] {
        let response = client
            .request_permission(permission_request(Some(kind), Some("safe operation")))
            .await
            .unwrap();
        assert_eq!(
            selected_permission_id(response).as_deref(),
            Some("allow_once")
        );
    }
}

#[tokio::test]
async fn read_only_permission_allows_only_trusted_title_when_kind_is_missing() {
    let client = test_client(AcpAccessPolicy::ReadOnly);

    for title in [
        "Read SKILL.md",
        "Search files for examples",
        "Inspect directory structure",
        "Think about the tutorial",
    ] {
        let response = client
            .request_permission(permission_request(None, Some(title)))
            .await
            .unwrap();
        assert_eq!(
            selected_permission_id(response).as_deref(),
            Some("allow_once"),
            "title should be allowlisted: {title}"
        );
    }

    let response = client
        .request_permission(permission_request(None, Some("Read and delete SKILL.md")))
        .await
        .unwrap();
    assert_eq!(
        selected_permission_id(response).as_deref(),
        Some("reject_once")
    );
}

#[tokio::test]
async fn read_only_permission_rejects_mutating_external_and_unknown_kinds() {
    let client = test_client(AcpAccessPolicy::ReadOnly);

    for kind in [
        acp::ToolKind::Edit,
        acp::ToolKind::Delete,
        acp::ToolKind::Move,
        acp::ToolKind::Execute,
        acp::ToolKind::Fetch,
        acp::ToolKind::SwitchMode,
        acp::ToolKind::Other,
    ] {
        let response = client
            .request_permission(permission_request(Some(kind), Some("Read SKILL.md")))
            .await
            .unwrap();
        assert_eq!(
            selected_permission_id(response).as_deref(),
            Some("reject_once"),
            "kind should be rejected: {kind:?}"
        );
    }
}

#[tokio::test]
async fn read_only_permission_never_falls_back_to_allow_always() {
    let client = test_client(AcpAccessPolicy::ReadOnly);
    let request = acp::RequestPermissionRequest::new(
        "session-1",
        acp::ToolCallUpdate::new(
            "tool-1",
            acp::ToolCallUpdateFields::new().kind(acp::ToolKind::Read),
        ),
        vec![
            acp::PermissionOption::new(
                "allow_always",
                "Always Allow",
                acp::PermissionOptionKind::AllowAlways,
            ),
            acp::PermissionOption::new(
                "reject_once",
                "Reject",
                acp::PermissionOptionKind::RejectOnce,
            ),
        ],
    );

    let response = client.request_permission(request).await.unwrap();
    assert_eq!(
        selected_permission_id(response).as_deref(),
        Some("reject_once")
    );
}

#[tokio::test]
async fn read_only_client_hard_rejects_write_and_terminal_methods() {
    let client = test_client(AcpAccessPolicy::ReadOnly);
    let write = client
        .write_text_file(acp::WriteTextFileRequest::new(
            "session-1",
            std::env::temp_dir().join("blocked.txt"),
            "blocked",
        ))
        .await;
    assert!(write.is_err());

    let terminal = client
        .create_terminal(acp::CreateTerminalRequest::new("session-1", "echo"))
        .await;
    assert!(terminal.is_err());
}

#[tokio::test]
async fn read_only_file_reads_are_strictly_rooted_in_work_dir() {
    let work_dir = tempfile::tempdir().unwrap();
    let outside_dir = tempfile::tempdir().unwrap();
    std::fs::write(work_dir.path().join("inside.txt"), "inside").unwrap();
    let outside_path = outside_dir.path().join("outside.txt");
    std::fs::write(&outside_path, "outside").unwrap();

    let client = SkillStarClient::new(
        Arc::new(Mutex::new(String::new())),
        |_| {},
        work_dir.path().to_path_buf(),
        AcpAccessPolicy::ReadOnly,
    );
    let inside = client
        .read_text_file(acp::ReadTextFileRequest::new("session-1", "inside.txt"))
        .await
        .unwrap();
    assert_eq!(inside.content, "inside");

    let outside = client
        .read_text_file(acp::ReadTextFileRequest::new("session-1", outside_path))
        .await;
    assert!(outside.is_err());
}

#[test]
fn read_only_capabilities_declare_only_text_reads() {
    let capabilities = capabilities_for_policy(AcpAccessPolicy::ReadOnly);
    assert!(capabilities.fs.read_text_file);
    assert!(!capabilities.fs.write_text_file);
    assert!(!capabilities.terminal);
}

#[test]
fn full_capabilities_preserve_setup_access() {
    let capabilities = capabilities_for_policy(AcpAccessPolicy::Full);
    assert!(capabilities.fs.read_text_file);
    assert!(capabilities.fs.write_text_file);
    assert!(capabilities.terminal);
}

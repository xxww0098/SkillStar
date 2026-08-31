//! ACP client implementation: the `agent_client_protocol::Client` trait impl
//! for `SkillStarClient`. Every session is read-only — permissions approve
//! read/search/think tools once, file reads are rooted in `work_dir`, and the
//! write and terminal methods exist only to reject the agent's requests.

use agent_client_protocol::{self as acp};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

// ── ACP Client Implementation ───────────────────────────────────────

/// Read-only ACP Client. Implements permissions (approve read-only tools
/// once), session notifications (collect + stream text) and file reads rooted
/// in `work_dir`. Writes and terminals are always rejected.
pub(crate) struct SkillStarClient {
    /// Collects agent text for the current prompt turn.
    pub(crate) collected: Arc<Mutex<String>>,
    /// Callback fired for every text chunk (for streaming to UI / logs).
    pub(crate) on_chunk: Box<dyn Fn(&str) + Send + Sync>,
    /// The only directory the agent is allowed to read from.
    work_dir: PathBuf,
}

impl SkillStarClient {
    pub(crate) fn new(
        collected: Arc<Mutex<String>>,
        on_chunk: impl Fn(&str) + Send + Sync + 'static,
        work_dir: PathBuf,
    ) -> Self {
        Self {
            collected,
            on_chunk: Box::new(on_chunk),
            work_dir,
        }
    }
}

/// Rejection returned by every method that would mutate state or run a
/// command. Read-only sessions never expose these, so an agent reaching one
/// is out of contract.
fn unavailable_in_read_only(method: &str) -> acp::Error {
    warn!(
        target: "acp_client",
        method,
        "ACP method blocked by read-only policy"
    );
    acp::Error::method_not_found()
}

fn selected_permission_response(option: &acp::PermissionOption) -> acp::RequestPermissionResponse {
    acp::RequestPermissionResponse::new(acp::RequestPermissionOutcome::Selected(
        acp::SelectedPermissionOutcome::new(option.option_id.clone()),
    ))
}

fn rejected_permission_response(
    options: &[acp::PermissionOption],
) -> acp::RequestPermissionResponse {
    let rejection = options
        .iter()
        .find(|option| option.kind == acp::PermissionOptionKind::RejectOnce)
        .or_else(|| {
            options
                .iter()
                .find(|option| option.kind == acp::PermissionOptionKind::RejectAlways)
        });

    match rejection {
        Some(option) => selected_permission_response(option),
        None => acp::RequestPermissionResponse::new(acp::RequestPermissionOutcome::Cancelled),
    }
}

fn has_trusted_read_only_title(title: Option<&str>) -> bool {
    let Some(title) = title else {
        return false;
    };

    let words: Vec<String> = title
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let Some(first) = words.first().map(String::as_str) else {
        return false;
    };

    const READ_ONLY_PREFIXES: &[&str] = &[
        "read", "readfile", "view", "inspect", "list", "search", "find", "grep", "glob", "think",
        "analyze", "analyse",
    ];
    const MUTATING_OR_EXTERNAL_WORDS: &[&str] = &[
        "write", "edit", "delete", "remove", "move", "rename", "execute", "run", "shell",
        "terminal", "fetch", "download", "upload", "network", "http", "install", "create", "patch",
        "apply",
    ];

    READ_ONLY_PREFIXES.contains(&first)
        && !words
            .iter()
            .any(|word| MUTATING_OR_EXTERNAL_WORDS.contains(&word.as_str()))
}

fn read_only_tool_is_allowed(tool_call: &acp::ToolCallUpdate) -> bool {
    match tool_call.fields.kind {
        Some(acp::ToolKind::Read | acp::ToolKind::Search | acp::ToolKind::Think) => true,
        // A few agents omit the kind while streaming the permission request.
        // In that case only a narrow, verb-first title allowlist is accepted.
        None => has_trusted_read_only_title(tool_call.fields.title.as_deref()),
        Some(_) => false,
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Client for SkillStarClient {
    async fn request_permission(
        &self,
        args: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        // Log the permission request so it's visible in the terminal
        let options_desc: Vec<String> = args
            .options
            .iter()
            .map(|o| format!("[{}] {}", o.option_id, o.name))
            .collect();
        info!(
            target: "acp_client",
            options = %options_desc.join(", "),
            "permission requested"
        );

        if read_only_tool_is_allowed(&args.tool_call) {
            // Read-only sessions deliberately never persist a permission
            // grant, even if AllowAlways is the only allow option.
            if let Some(option) = args
                .options
                .iter()
                .find(|option| option.kind == acp::PermissionOptionKind::AllowOnce)
            {
                info!(
                    target: "acp_client",
                    selected = %option.option_id,
                    "approved one read-only operation"
                );
                return Ok(selected_permission_response(option));
            }
        }

        warn!(
            target: "acp_client",
            kind = ?args.tool_call.fields.kind,
            title = ?args.tool_call.fields.title,
            "permission rejected by read-only policy"
        );
        Ok(rejected_permission_response(&args.options))
    }

    async fn session_notification(&self, args: acp::SessionNotification) -> acp::Result<()> {
        match args.update {
            acp::SessionUpdate::AgentMessageChunk(chunk) => {
                if let acp::ContentBlock::Text(text_content) = chunk.content {
                    let text = &text_content.text;
                    // Stream to terminal via tracing
                    info!(target: "acp_agent", "{}", text);
                    // Forward to caller's callback
                    (self.on_chunk)(text);
                    // Accumulate for script extraction
                    if let Ok(mut collected) = self.collected.lock() {
                        collected.push_str(text);
                    }
                }
            }
            acp::SessionUpdate::ToolCallUpdate(tc) => {
                debug!(target: "acp_client", tool_call_id = ?tc.tool_call_id, "tool call update");
            }
            _ => {
                debug!(target: "acp_client", "session update (non-message)");
            }
        }
        Ok(())
    }

    // ── Filesystem ──────────────────────────────────────────────────

    async fn read_text_file(
        &self,
        args: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        let path = args.path;
        info!(target: "acp_client", path = %path.display(), "read_text_file");

        // Security: resolve and ensure path is within work_dir
        let resolved = self.resolve_safe_path(&path)?;

        let content = std::fs::read_to_string(&resolved).map_err(|e| {
            warn!(target: "acp_client", path = %resolved.display(), error = %e, "read_text_file failed");
            acp::Error::internal_error()
        })?;

        // Apply line/limit if specified
        let content = if args.line.is_some() || args.limit.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let start = args.line.unwrap_or(1).max(1) as usize - 1;
            let limit = args.limit.unwrap_or(u32::MAX) as usize;
            let end = (start + limit).min(lines.len());
            if start >= lines.len() {
                String::new()
            } else {
                lines[start..end].join("\n")
            }
        } else {
            content
        };

        Ok(acp::ReadTextFileResponse::new(content))
    }

    async fn write_text_file(
        &self,
        _args: acp::WriteTextFileRequest,
    ) -> acp::Result<acp::WriteTextFileResponse> {
        Err(unavailable_in_read_only("fs/write_text_file"))
    }

    // ── Terminals ───────────────────────────────────────────────────
    //
    // Never advertised in the client capabilities, so these exist only to
    // satisfy the `acp::Client` trait and refuse out-of-contract agents.

    async fn create_terminal(
        &self,
        _args: acp::CreateTerminalRequest,
    ) -> acp::Result<acp::CreateTerminalResponse> {
        Err(unavailable_in_read_only("terminal/create"))
    }

    async fn terminal_output(
        &self,
        _args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        Err(unavailable_in_read_only("terminal/output"))
    }

    async fn wait_for_terminal_exit(
        &self,
        _args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        Err(unavailable_in_read_only("terminal/wait_for_exit"))
    }

    async fn kill_terminal(
        &self,
        _args: acp::KillTerminalRequest,
    ) -> acp::Result<acp::KillTerminalResponse> {
        Err(unavailable_in_read_only("terminal/kill"))
    }

    async fn release_terminal(
        &self,
        _args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        Err(unavailable_in_read_only("terminal/release"))
    }
}

impl SkillStarClient {
    /// Resolve a path and ensure it's within the allowed work directory.
    fn resolve_safe_path(&self, path: &std::path::Path) -> acp::Result<PathBuf> {
        let work_dir = &self.work_dir;

        // If path is relative, resolve against work_dir
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            work_dir.join(path)
        };

        // Canonicalize both to compare (work_dir may have symlinks)
        let canon_work = std::fs::canonicalize(work_dir).unwrap_or_else(|_| work_dir.clone());

        // A file must exist to canonicalize; a missing one is checked via its
        // nearest existing ancestor.
        if resolved.exists() {
            let canon_resolved =
                std::fs::canonicalize(&resolved).unwrap_or_else(|_| resolved.clone());
            if !canon_resolved.starts_with(&canon_work) {
                warn!(
                    target: "acp_client",
                    path = %resolved.display(),
                    work_dir = %canon_work.display(),
                    "path escape attempt blocked"
                );
                return Err(acp::Error::invalid_params());
            }
            Ok(canon_resolved)
        } else {
            // File doesn't exist (the agent asked for a missing path). Check
            // that the logical path after normalizing ".." stays within
            // work_dir: collapse the non-existent tail, then compare.
            let mut check = resolved.clone();
            while !check.exists() {
                if let Some(parent) = check.parent() {
                    check = parent.to_path_buf();
                } else {
                    break;
                }
            }
            let canon_check = std::fs::canonicalize(&check).unwrap_or(check);
            if !canon_check.starts_with(&canon_work) {
                warn!(
                    target: "acp_client",
                    path = %resolved.display(),
                    work_dir = %canon_work.display(),
                    "path escape attempt blocked (missing path)"
                );
                return Err(acp::Error::invalid_params());
            }
            Ok(resolved)
        }
    }
}

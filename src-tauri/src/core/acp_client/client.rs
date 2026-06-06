//! ACP client implementation: terminal management and the full
//! `agent_client_protocol::Client` trait impl (permissions, session
//! notifications, filesystem ops, terminal ops) for `SkillStarClient`.

use agent_client_protocol::{self as acp};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

use crate::core::path_env;

/// Result of an ACP setup session.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcpSetupResult {
    /// The script that the agent successfully executed.
    pub script: String,
    /// Combined agent output (for display in the UI).
    pub agent_output: String,
}

// ── Terminal Manager ────────────────────────────────────────────────

/// Tracks a spawned terminal subprocess.
struct ManagedTerminal {
    child: tokio::process::Child,
    output: Arc<Mutex<String>>,
    exit_status: Arc<Mutex<Option<i32>>>,
}

/// Thread-safe terminal registry for the ACP session.
pub(crate) struct TerminalManager {
    terminals: Mutex<HashMap<String, ManagedTerminal>>,
    next_id: Mutex<u64>,
    /// The allowed working directory (repo root) for all operations.
    work_dir: PathBuf,
}

impl TerminalManager {
    pub(crate) fn new(work_dir: PathBuf) -> Self {
        Self {
            terminals: Mutex::new(HashMap::new()),
            next_id: Mutex::new(0),
            work_dir,
        }
    }

    fn alloc_id(&self) -> String {
        let mut id = self.next_id.lock().unwrap();
        let cur = *id;
        *id += 1;
        format!("term-{cur}")
    }
}

// ── ACP Client Implementation ───────────────────────────────────────

/// Full ACP Client. Implements permissions (auto-approve), session
/// notifications (collect + stream text), filesystem ops, and terminal ops.
pub(crate) struct SkillStarClient {
    /// Collects all agent text output for later script extraction.
    pub(crate) collected: Arc<Mutex<String>>,
    /// Callback fired for every text chunk (for streaming to UI / logs).
    pub(crate) on_chunk: Box<dyn Fn(&str) + Send + Sync>,
    /// Terminal manager for running commands.
    pub(crate) terminals: Arc<TerminalManager>,
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
            "permission requested → auto-approving"
        );

        // Prefer "allow_always" to minimize repeated permission prompts.
        // Fall back to any allow-kind option, then first option.
        let chosen = args
            .options
            .iter()
            .find(|o| o.kind == acp::PermissionOptionKind::AllowAlways)
            .or_else(|| {
                args.options
                    .iter()
                    .find(|o| o.kind == acp::PermissionOptionKind::AllowOnce)
            })
            .or_else(|| args.options.first());

        if let Some(opt) = chosen {
            info!(
                target: "acp_client",
                selected = %opt.option_id,
                "auto-approved permission"
            );
            Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Selected(acp::SelectedPermissionOutcome::new(
                    opt.option_id.clone(),
                )),
            ))
        } else {
            warn!(target: "acp_client", "no permission options available → cancelling");
            Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Cancelled,
            ))
        }
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
        args: acp::WriteTextFileRequest,
    ) -> acp::Result<acp::WriteTextFileResponse> {
        let path = args.path;
        info!(target: "acp_client", path = %path.display(), "write_text_file");

        // Security: resolve and ensure path is within work_dir
        let resolved = self.resolve_safe_path(&path)?;

        // Create parent directories if needed
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                warn!(target: "acp_client", path = %parent.display(), error = %e, "mkdir failed");
                acp::Error::internal_error()
            })?;
        }

        std::fs::write(&resolved, &args.content).map_err(|e| {
            warn!(target: "acp_client", path = %resolved.display(), error = %e, "write_text_file failed");
            acp::Error::internal_error()
        })?;

        Ok(acp::WriteTextFileResponse::new())
    }

    // ── Terminals ───────────────────────────────────────────────────

    async fn create_terminal(
        &self,
        args: acp::CreateTerminalRequest,
    ) -> acp::Result<acp::CreateTerminalResponse> {
        let cwd = args.cwd.unwrap_or_else(|| self.terminals.work_dir.clone());
        info!(
            target: "acp_client",
            command = %args.command,
            args = ?args.args,
            cwd = %cwd.display(),
            "create_terminal"
        );

        let mut cmd = tokio::process::Command::new(&args.command);
        cmd.args(&args.args)
            .current_dir(&cwd)
            .env("PATH", path_env::enriched_path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);

        // Set env vars from request
        for env_var in &args.env {
            cmd.env(&env_var.name, &env_var.value);
        }

        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn().map_err(|e| {
            warn!(target: "acp_client", command = %args.command, error = %e, "create_terminal spawn failed");
            acp::Error::internal_error()
        })?;

        let term_id = self.terminals.alloc_id();
        let output_buf = Arc::new(Mutex::new(String::new()));
        let exit_status = Arc::new(Mutex::new(None));

        // Spawn background tasks to collect stdout and stderr
        let output_clone = output_buf.clone();
        if let Some(stdout) = child.stdout.take() {
            tokio::task::spawn_local(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stdout);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!(target: "acp_terminal", "[stdout] {}", line);
                    if let Ok(mut buf) = output_clone.lock() {
                        buf.push_str(&line);
                        buf.push('\n');
                    }
                }
            });
        }

        let output_clone2 = output_buf.clone();
        if let Some(stderr) = child.stderr.take() {
            tokio::task::spawn_local(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    debug!(target: "acp_terminal", "[stderr] {}", line);
                    if let Ok(mut buf) = output_clone2.lock() {
                        buf.push_str(&line);
                        buf.push('\n');
                    }
                }
            });
        }

        let managed = ManagedTerminal {
            child,
            output: output_buf,
            exit_status,
        };

        self.terminals
            .terminals
            .lock()
            .unwrap()
            .insert(term_id.clone(), managed);

        info!(target: "acp_client", terminal_id = %term_id, "terminal created");
        Ok(acp::CreateTerminalResponse::new(term_id))
    }

    async fn terminal_output(
        &self,
        args: acp::TerminalOutputRequest,
    ) -> acp::Result<acp::TerminalOutputResponse> {
        let term_id = args.terminal_id.0.as_ref();
        let terminals = self.terminals.terminals.lock().unwrap();
        let term = terminals.get(term_id).ok_or_else(|| {
            warn!(target: "acp_client", terminal_id = %term_id, "terminal not found");
            acp::Error::invalid_params()
        })?;

        let output = term.output.lock().map(|o| o.clone()).unwrap_or_default();
        let exit_status = term.exit_status.lock().ok().and_then(|s| *s);

        let mut resp = acp::TerminalOutputResponse::new(&output, false);
        if let Some(code) = exit_status {
            resp = resp.exit_status(acp::TerminalExitStatus::new().exit_code(code as u32));
        }
        Ok(resp)
    }

    async fn wait_for_terminal_exit(
        &self,
        args: acp::WaitForTerminalExitRequest,
    ) -> acp::Result<acp::WaitForTerminalExitResponse> {
        let term_id = args.terminal_id.0.as_ref().to_string();
        info!(target: "acp_client", terminal_id = %term_id, "wait_for_terminal_exit");

        // We need to take the child out to await it, but we can't hold the
        // mutex across an await point. Use a polling approach.
        loop {
            {
                let mut terminals = self.terminals.terminals.lock().unwrap();
                if let Some(term) = terminals.get_mut(&term_id) {
                    // Try to check if the child has exited
                    match term.child.try_wait() {
                        Ok(Some(status)) => {
                            let code = status.code().unwrap_or(-1);
                            if let Ok(mut es) = term.exit_status.lock() {
                                *es = Some(code);
                            }
                            info!(target: "acp_client", terminal_id = %term_id, exit_code = code, "terminal exited");
                            let exit_status = acp::TerminalExitStatus::new().exit_code(code as u32);
                            return Ok(acp::WaitForTerminalExitResponse::new(exit_status));
                        }
                        Ok(None) => {
                            // Still running, continue polling
                        }
                        Err(e) => {
                            warn!(target: "acp_client", terminal_id = %term_id, error = %e, "try_wait error");
                            return Err(acp::Error::internal_error());
                        }
                    }
                } else {
                    warn!(target: "acp_client", terminal_id = %term_id, "terminal not found for wait");
                    return Err(acp::Error::invalid_params());
                }
            }
            // Sleep briefly before polling again
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    async fn kill_terminal(
        &self,
        args: acp::KillTerminalRequest,
    ) -> acp::Result<acp::KillTerminalResponse> {
        let term_id = args.terminal_id.0.as_ref();
        info!(target: "acp_client", terminal_id = %term_id, "kill_terminal");

        let mut terminals = self.terminals.terminals.lock().unwrap();
        if let Some(term) = terminals.get_mut(term_id) {
            let _ = term.child.start_kill();
            Ok(acp::KillTerminalResponse::new())
        } else {
            Err(acp::Error::invalid_params())
        }
    }

    async fn release_terminal(
        &self,
        args: acp::ReleaseTerminalRequest,
    ) -> acp::Result<acp::ReleaseTerminalResponse> {
        let term_id = args.terminal_id.0.as_ref();
        info!(target: "acp_client", terminal_id = %term_id, "release_terminal");

        let mut terminals = self.terminals.terminals.lock().unwrap();
        if let Some(mut term) = terminals.remove(term_id) {
            let _ = term.child.start_kill();
            Ok(acp::ReleaseTerminalResponse::new())
        } else {
            // Already released, that's fine
            Ok(acp::ReleaseTerminalResponse::new())
        }
    }
}

impl SkillStarClient {
    /// Resolve a path and ensure it's within the allowed work directory.
    fn resolve_safe_path(&self, path: &std::path::Path) -> acp::Result<PathBuf> {
        let work_dir = &self.terminals.work_dir;

        // If path is relative, resolve against work_dir
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            work_dir.join(path)
        };

        // Canonicalize both to compare (work_dir may have symlinks)
        let canon_work = std::fs::canonicalize(work_dir).unwrap_or_else(|_| work_dir.clone());

        // For reads, the file must exist to canonicalize. For writes, check parent.
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
            // File doesn't exist yet (write case). Check that the logical
            // path after normalizing ".." stays within work_dir.
            // Simple approach: ensure the path starts with work_dir after
            // collapsing the non-existent tail.
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
                    "path escape attempt blocked (write)"
                );
                return Err(acp::Error::invalid_params());
            }
            Ok(resolved)
        }
    }
}

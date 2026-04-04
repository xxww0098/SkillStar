//! ACP (Agent Client Protocol) integration for SkillStar.
//!
//! Launches an external Agent (Claude Code / OpenCode / Codex) as a subprocess
//! and sends it a task to analyse a skill repo and generate a working setup
//! script.  The agent does ALL the heavy lifting.
#![allow(dead_code)]

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, error, info, warn};

use super::{path_env, paths};

// ── ACP Config ──────────────────────────────────────────────────────

/// Persisted ACP configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcpConfig {
    /// Whether ACP is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// The agent command to use (e.g. "npx -y @agentclientprotocol/claude-agent-acp").
    #[serde(default = "default_agent_command")]
    pub agent_command: String,
    /// Display name for reference.
    #[serde(default = "default_agent_label")]
    pub agent_label: String,
}

fn default_agent_command() -> String {
    "npx -y @agentclientprotocol/claude-agent-acp".to_string()
}

fn default_agent_label() -> String {
    "Claude Code".to_string()
}

impl Default for AcpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            agent_command: default_agent_command(),
            agent_label: default_agent_label(),
        }
    }
}

fn config_path() -> PathBuf {
    paths::acp_config_path()
}

/// Load the ACP config from disk.
pub fn load_config() -> AcpConfig {
    let p = config_path();
    match std::fs::read_to_string(&p) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => AcpConfig::default(),
    }
}

/// Save the ACP config to disk.
pub fn save_config(config: &AcpConfig) -> Result<()> {
    let p = config_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&p, content)?;
    info!(target: "acp_client", "saved ACP config");
    Ok(())
}

// ── Data Types ──────────────────────────────────────────────────────

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
struct TerminalManager {
    terminals: Mutex<HashMap<String, ManagedTerminal>>,
    next_id: Mutex<u64>,
    /// The allowed working directory (repo root) for all operations.
    work_dir: PathBuf,
}

impl TerminalManager {
    fn new(work_dir: PathBuf) -> Self {
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
struct SkillStarClient {
    /// Collects all agent text output for later script extraction.
    collected: Arc<Mutex<String>>,
    /// Callback fired for every text chunk (for streaming to UI / logs).
    on_chunk: Box<dyn Fn(&str) + Send + Sync>,
    /// Terminal manager for running commands.
    terminals: Arc<TerminalManager>,
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

// ── Core Function ───────────────────────────────────────────────────

/// The prompt template sent to the Agent.
const SETUP_PROMPT: &str = r#"This repository needs a setup script to work properly.  Please:

1. Read the README.md and analyze the directory structure.
2. Identify what setup steps are needed (e.g. running `./setup`, `npm install`, `make`, etc.).
3. Write a bash script that performs the setup.  The script should be idempotent.
4. Execute the script to verify it works.
5. At the very end of your response, output the **final working script** wrapped in a fenced code block with the language tag `setup-script`, like this:

```setup-script
#!/bin/bash
set -euo pipefail
# your working script here
```

IMPORTANT: Only output the `setup-script` block AFTER you have successfully run the script and verified it works."#;

/// Prompt template for multi-skill repos that need rebuild.
///
/// Instructs the agent to build the repo AND produce a `skills-rebuild/`
/// directory containing one subdirectory per skill, each with a `SKILL.md`.
const REBUILD_PROMPT: &str = r#"This repository contains multiple skills that need a build step before they can be used individually.

Please:

1. Read the README.md, AGENTS.md, and analyze the directory structure.
2. Look for `SKILL.md` files in subdirectories — each one represents an individual skill.
3. Look for a build system (package.json, setup script, Makefile, etc.) and run the build.
4. After the build succeeds, create a directory called `skills-rebuild/` in the repo root.
5. For EACH skill found (each directory containing a SKILL.md), create a subdirectory in `skills-rebuild/` named after the skill.
6. Each `skills-rebuild/<skill-name>/` subdirectory must contain at minimum:
   - A `SKILL.md` file (copy from the original location, or use any generated/adapted version if one exists)
   - Any supporting files the skill references (scripts, binaries, etc.) — use symlinks to the originals when possible
7. If the repo has pre-generated skill directories (e.g. `.agents/skills/`, `.claude/skills/`) that already contain adapted SKILL.md files, prefer those.
8. The script should be idempotent — safe to re-run.
9. Do NOT include the root-level SKILL.md as a separate skill (it's a meta-skill for the whole repo).

At the very end output the **final working script** wrapped in a fenced code block with the language tag `setup-script`:

```setup-script
#!/bin/bash
set -euo pipefail
# your working script here
```

CRITICAL: Only output the `setup-script` block AFTER you have successfully run the script and verified that `skills-rebuild/` exists and has at least one subdirectory with a SKILL.md."#;

/// Parse the agent's response to extract the setup-script block.
fn extract_script(response: &str) -> Option<String> {
    let marker_start = "```setup-script";
    let marker_end = "```";

    let start_idx = response.find(marker_start)?;
    let after_marker = start_idx + marker_start.len();
    let rest = &response[after_marker..];

    // Skip optional newline after marker
    let content_start = if rest.starts_with('\n') { 1 } else { 0 };
    let content = &rest[content_start..];

    let end_idx = content.find(marker_end)?;
    let script = content[..end_idx].trim().to_string();

    if script.is_empty() {
        None
    } else {
        Some(script)
    }
}

/// Run the ACP setup flow for a skill (default setup prompt).
///
/// Launches the specified agent command, opens a session in the skill's repo
/// directory, sends the setup prompt, collects the result, and extracts the
/// working script.
pub async fn run_setup_via_acp(
    agent_command: &str,
    skill_name: &str,
    on_chunk: impl Fn(&str) + Send + Sync + 'static,
) -> Result<AcpSetupResult> {
    run_acp_with_prompt(agent_command, skill_name, SETUP_PROMPT, on_chunk).await
}

/// Run the ACP rebuild flow for a multi-skill repo.
///
/// Uses the REBUILD_PROMPT which instructs the agent to build the repo and
/// create a `skills-rebuild/` directory containing one subdirectory per skill.
pub async fn run_rebuild_via_acp(
    agent_command: &str,
    skill_name: &str,
    on_chunk: impl Fn(&str) + Send + Sync + 'static,
) -> Result<AcpSetupResult> {
    run_acp_with_prompt(agent_command, skill_name, REBUILD_PROMPT, on_chunk).await
}

/// Internal: run an ACP session with the given prompt text.
async fn run_acp_with_prompt(
    agent_command: &str,
    skill_name: &str,
    prompt: &str,
    on_chunk: impl Fn(&str) + Send + Sync + 'static,
) -> Result<AcpSetupResult> {
    // Resolve the repo directory for this skill
    let skills_dir = paths::hub_skills_dir();
    let skill_path = skills_dir.join(skill_name);

    let work_dir = resolve_repo_root(&skill_path);
    if !work_dir.exists() {
        return Err(anyhow!("Skill directory not found: {}", work_dir.display()));
    }

    info!(
        target: "acp_client",
        skill = %skill_name,
        agent = %agent_command,
        dir = %work_dir.display(),
        "starting ACP session"
    );

    // Parse agent command into program + args
    let parts: Vec<String> = agent_command.split_whitespace().map(String::from).collect();
    let program = parts
        .first()
        .ok_or_else(|| anyhow!("Empty agent command"))?
        .clone();
    let args: Vec<String> = parts[1..].to_vec();
    let agent_cmd_display = agent_command.to_string();
    let prompt_text = prompt.to_string();

    // Shared buffer for collecting agent output
    let collected = Arc::new(Mutex::new(String::new()));
    let collected_clone = collected.clone();

    let work_dir_clone = work_dir.clone();

    // Run ACP session in a dedicated single-threaded runtime because
    // ClientSideConnection futures are !Send (LocalSet required).
    // IMPORTANT: the child process MUST be spawned inside this runtime so
    // that tokio I/O handles are bound to the correct reactor.
    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create ACP runtime")?;

        rt.block_on(async {
            // Spawn agent subprocess INSIDE this runtime so I/O handles
            // are registered with this runtime's reactor.
            // Use enriched PATH and Windows CREATE_NO_WINDOW like all other spawns.
            let mut cmd = tokio::process::Command::new(&program);
            cmd.args(&args)
                .env("PATH", path_env::enriched_path())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);

            #[cfg(windows)]
            {
                cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }

            let mut child = cmd.spawn().with_context(|| {
                format!(
                    "Failed to start ACP agent '{}'. Is it installed and in PATH? (enriched PATH: {})",
                    agent_cmd_display,
                    path_env::enriched_path()
                )
            })?;

            info!(target: "acp_client", "agent subprocess spawned (pid: {:?})", child.id());

            let stdin = child.stdin.take().unwrap().compat_write();
            let stdout = child.stdout.take().unwrap().compat();
            let stderr_handle = child.stderr.take();

            // Stream agent stderr in background so diagnostics are visible
            // in the terminal in real-time (not just on failure).
            let stderr_collected = Arc::new(Mutex::new(String::new()));
            let stderr_collected_bg = stderr_collected.clone();
            if let Some(mut se) = stderr_handle {
                tokio::task::spawn(async move {
                    use tokio::io::AsyncBufReadExt;
                    let reader = tokio::io::BufReader::new(&mut se);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        info!(target: "acp_agent_stderr", "{}", line);
                        if let Ok(mut buf) = stderr_collected_bg.lock() {
                            buf.push_str(&line);
                            buf.push('\n');
                        }
                    }
                });
            }

            let local_set = tokio::task::LocalSet::new();
            let session_result = local_set
                .run_until(async {
                    let terminal_mgr = Arc::new(TerminalManager::new(work_dir_clone.clone()));
                    let client = SkillStarClient {
                        collected: collected_clone,
                        on_chunk: Box::new(on_chunk),
                        terminals: terminal_mgr,
                    };

                    let (conn, handle_io) = acp::ClientSideConnection::new(
                        client,
                        stdin,
                        stdout,
                        |fut| { tokio::task::spawn_local(fut); },
                    );

                    tokio::task::spawn_local(handle_io);

                    // Initialize handshake — declare capabilities so the agent
                    // knows it can use our terminal and filesystem methods.
                    info!(target: "acp_client", "sending ACP initialize (with capabilities)...");
                    conn.initialize(
                        acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                            .client_info(
                                acp::Implementation::new("skillstar", env!("CARGO_PKG_VERSION"))
                                    .title("SkillStar"),
                            )
                            .client_capabilities(
                                acp::ClientCapabilities::new()
                                    .terminal(true)
                                    .fs(
                                        acp::FileSystemCapabilities::new()
                                            .read_text_file(true)
                                            .write_text_file(true),
                                    ),
                            ),
                    )
                    .await
                    .map_err(|e| anyhow!("ACP initialize failed: {e}"))?;

                    info!(target: "acp_client", "ACP initialized successfully (terminal + fs capabilities declared)");

                    // Create session rooted at the repo directory
                    let session = conn
                        .new_session(acp::NewSessionRequest::new(work_dir_clone.clone()))
                        .await
                        .map_err(|e| anyhow!("ACP new_session failed: {e}"))?;

                    info!(target: "acp_client", session_id = %session.session_id, "ACP session created");

                    // Send the prompt
                    match conn
                        .prompt(acp::PromptRequest::new(
                            session.session_id.clone(),
                            vec![prompt_text.into()],
                        ))
                        .await
                    {
                        Ok(_) => info!(target: "acp_client", "ACP prompt completed"),
                        Err(e) => warn!(target: "acp_client", error = %e, "ACP prompt error"),
                    }

                    Ok::<_, anyhow::Error>(())
                })
                .await;

            // On failure, attach any collected stderr to the error
            if let Err(ref e) = session_result {
                let stderr_text = stderr_collected
                    .lock()
                    .map(|s| s.clone())
                    .unwrap_or_default();
                if !stderr_text.is_empty() {
                    error!(
                        target: "acp_client",
                        stderr = %stderr_text,
                        "agent stderr output"
                    );
                    let _ = child.kill().await;
                    return Err(anyhow!("{e}\n\nAgent stderr:\n{stderr_text}"));
                }
            }

            // Clean up child
            let _ = child.kill().await;
            session_result
        })
    })
    .await
    .context("ACP task panicked")?;

    result?;

    let output = collected.lock().map(|o| o.clone()).unwrap_or_default();

    // Extract the script from the agent's response
    let script = extract_script(&output).ok_or_else(|| {
        let preview = if output.len() > 2000 {
            &output[..2000]
        } else {
            &output
        };
        anyhow!(
            "Agent did not return a setup-script block. Output preview:\n{}",
            preview
        )
    })?;

    info!(
        target: "acp_client",
        skill = %skill_name,
        script_len = script.len(),
        "ACP session completed, script extracted"
    );

    Ok(AcpSetupResult {
        script,
        agent_output: output,
    })
}

/// Resolve a skill path to its repo root (for running hooks at repo level).
fn resolve_repo_root(skill_path: &std::path::Path) -> PathBuf {
    let Ok(canonical) = std::fs::canonicalize(skill_path) else {
        return skill_path.to_path_buf();
    };

    let repos_cache = paths::repos_cache_dir();
    if !canonical.starts_with(&repos_cache) {
        return canonical;
    }

    if let Ok(rel) = canonical.strip_prefix(&repos_cache) {
        if let Some(first) = rel.components().next() {
            return repos_cache.join(first);
        }
    }
    canonical
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::Client as _;

    // ── Script extraction ───────────────────────────────────────────

    #[test]
    fn extract_script_from_response() {
        let response = r#"I analyzed the repo and here's the setup:

```setup-script
#!/bin/bash
set -euo pipefail
./setup
```

Done!"#;
        let script = extract_script(response).unwrap();
        assert_eq!(script, "#!/bin/bash\nset -euo pipefail\n./setup");
    }

    #[test]
    fn extract_script_no_block() {
        assert!(extract_script("No script here").is_none());
    }

    #[test]
    fn extract_script_empty_block() {
        assert!(extract_script("```setup-script\n```").is_none());
    }

    #[test]
    fn extract_script_multiple_blocks_picks_first() {
        let response = r#"
```setup-script
echo first
```
some text
```setup-script
echo second
```
"#;
        let script = extract_script(response).unwrap();
        assert_eq!(script, "echo first");
    }

    #[test]
    fn extract_script_preserves_multiline_indentation() {
        let response = "```setup-script\n#!/bin/bash\nif true; then\n  echo ok\nfi\n```";
        let script = extract_script(response).unwrap();
        assert!(script.contains("  echo ok"));
    }

    // ── Config round-trip ───────────────────────────────────────────

    #[test]
    fn config_defaults() {
        let cfg = AcpConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(
            cfg.agent_command,
            "npx -y @agentclientprotocol/claude-agent-acp"
        );
        assert_eq!(cfg.agent_label, "Claude Code");
    }

    #[test]
    fn config_serde_roundtrip() {
        let cfg = AcpConfig {
            enabled: true,
            agent_command: "opencode acp".to_string(),
            agent_label: "Codex".to_string(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: AcpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled, cfg.enabled);
        assert_eq!(parsed.agent_command, cfg.agent_command);
        assert_eq!(parsed.agent_label, cfg.agent_label);
    }

    #[test]
    fn config_deserialize_missing_fields_gets_defaults() {
        let json = r#"{}"#;
        let cfg: AcpConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.enabled);
        assert_eq!(
            cfg.agent_command,
            "npx -y @agentclientprotocol/claude-agent-acp"
        );
    }

    // ── Subprocess spawn pattern ────────────────────────────────────

    /// Validates that the spawn_blocking + inner runtime pattern works
    /// on all platforms by running a minimal subprocess.
    #[tokio::test]
    async fn spawn_blocking_inner_runtime_subprocess() {
        let result = tokio::task::spawn_blocking(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                #[cfg(unix)]
                let child = tokio::process::Command::new("echo")
                    .arg("acp-test-ok")
                    .stdout(std::process::Stdio::piped())
                    .spawn()
                    .expect("echo should be available");

                #[cfg(windows)]
                let mut child = tokio::process::Command::new("cmd")
                    .args(["/C", "echo", "acp-test-ok"])
                    .stdout(std::process::Stdio::piped())
                    .creation_flags(0x08000000u32)
                    .spawn()
                    .expect("cmd should be available");

                let output = child.wait_with_output().await.unwrap();
                assert!(output.status.success());
                let text = String::from_utf8_lossy(&output.stdout);
                assert!(text.contains("acp-test-ok"), "got: {}", text);
            });
        })
        .await;

        assert!(result.is_ok(), "spawn_blocking task panicked");
    }

    // ── Permission auto-approval ────────────────────────────────────

    /// Verify that request_permission prefers AllowAlways over AllowOnce.
    #[tokio::test]
    async fn permission_prefers_allow_always() {
        use tokio::task::LocalSet;

        let local = LocalSet::new();
        local
            .run_until(async {
                let client = SkillStarClient {
                    collected: Arc::new(Mutex::new(String::new())),
                    on_chunk: Box::new(|_| {}),
                    terminals: Arc::new(TerminalManager::new(PathBuf::from("/tmp"))),
                };

                let req = acp::RequestPermissionRequest::new(
                    "session-1",
                    acp::ToolCallUpdate::new(
                        "tc-1",
                        acp::ToolCallUpdateFields::new().title("test_tool".to_string()),
                    ),
                    vec![
                        acp::PermissionOption::new(
                            "allow",
                            "Allow",
                            acp::PermissionOptionKind::AllowOnce,
                        ),
                        acp::PermissionOption::new(
                            "allow_always",
                            "Always Allow",
                            acp::PermissionOptionKind::AllowAlways,
                        ),
                        acp::PermissionOption::new(
                            "reject",
                            "Reject",
                            acp::PermissionOptionKind::RejectOnce,
                        ),
                    ],
                );

                let resp = client.request_permission(req).await.unwrap();
                match resp.outcome {
                    acp::RequestPermissionOutcome::Selected(sel) => {
                        assert_eq!(sel.option_id.0.as_ref(), "allow_always");
                    }
                    _ => panic!("Expected Selected outcome"),
                }
            })
            .await;
    }

    // ── Real ACP protocol integration test ──────────────────────────

    /// A minimal in-process ACP agent that echoes prompt content back
    /// with a setup-script block, used for protocol integration testing.
    struct MockAgent {
        session_update_tx: tokio::sync::mpsc::UnboundedSender<(
            acp::SessionNotification,
            tokio::sync::oneshot::Sender<()>,
        )>,
        next_session_id: std::cell::Cell<u64>,
    }

    #[async_trait::async_trait(?Send)]
    impl acp::Agent for MockAgent {
        async fn initialize(
            &self,
            _args: acp::InitializeRequest,
        ) -> Result<acp::InitializeResponse, acp::Error> {
            Ok(acp::InitializeResponse::new(acp::ProtocolVersion::V1)
                .agent_info(acp::Implementation::new("mock-agent", "0.1.0")))
        }

        async fn authenticate(
            &self,
            _args: acp::AuthenticateRequest,
        ) -> Result<acp::AuthenticateResponse, acp::Error> {
            Ok(acp::AuthenticateResponse::default())
        }

        async fn new_session(
            &self,
            _args: acp::NewSessionRequest,
        ) -> Result<acp::NewSessionResponse, acp::Error> {
            let id = self.next_session_id.get();
            self.next_session_id.set(id + 1);
            Ok(acp::NewSessionResponse::new(id.to_string()))
        }

        async fn load_session(
            &self,
            _args: acp::LoadSessionRequest,
        ) -> Result<acp::LoadSessionResponse, acp::Error> {
            Ok(acp::LoadSessionResponse::new())
        }

        async fn prompt(
            &self,
            args: acp::PromptRequest,
        ) -> Result<acp::PromptResponse, acp::Error> {
            // Send a text chunk containing a setup-script block
            let script_text = "Here is the script:\n\n```setup-script\n#!/bin/bash\nset -euo pipefail\necho hello\n```\n\nDone!";
            let chunk = acp::ContentChunk::new(acp::ContentBlock::Text(acp::TextContent::new(
                script_text.to_string(),
            )));
            let notification = acp::SessionNotification::new(
                args.session_id.clone(),
                acp::SessionUpdate::AgentMessageChunk(chunk),
            );
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.session_update_tx
                .send((notification, tx))
                .map_err(|_| acp::Error::internal_error())?;
            rx.await.map_err(|_| acp::Error::internal_error())?;
            Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
        }

        async fn cancel(&self, _args: acp::CancelNotification) -> Result<(), acp::Error> {
            Ok(())
        }

        async fn set_session_mode(
            &self,
            _args: acp::SetSessionModeRequest,
        ) -> Result<acp::SetSessionModeResponse, acp::Error> {
            Ok(acp::SetSessionModeResponse::default())
        }

        async fn set_session_config_option(
            &self,
            _args: acp::SetSessionConfigOptionRequest,
        ) -> Result<acp::SetSessionConfigOptionResponse, acp::Error> {
            Ok(acp::SetSessionConfigOptionResponse::new(vec![]))
        }

        async fn ext_method(&self, _args: acp::ExtRequest) -> Result<acp::ExtResponse, acp::Error> {
            Err(acp::Error::method_not_found())
        }

        async fn ext_notification(&self, _args: acp::ExtNotification) -> Result<(), acp::Error> {
            Ok(())
        }
    }

    /// Full ACP protocol test: client ↔ agent over in-memory pipe,
    /// doing initialize → new_session → prompt → extract script.
    #[tokio::test]
    async fn acp_protocol_full_roundtrip() {
        use tokio::io::duplex;
        use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

        // Create two duplex channels to wire client ↔ agent
        let (client_write, agent_read) = duplex(64 * 1024);
        let (agent_write, client_read) = duplex(64 * 1024);

        let collected = Arc::new(Mutex::new(String::new()));
        let collected_for_client = collected.clone();

        let result = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.block_on(async {
                let local = tokio::task::LocalSet::new();
                local
                    .run_until(async {
                        // ── Agent side ──────────────────────────────
                        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel();
                        let agent = MockAgent {
                            session_update_tx: agent_tx,
                            next_session_id: std::cell::Cell::new(0),
                        };
                        let (agent_conn, agent_io) = acp::AgentSideConnection::new(
                            agent,
                            agent_write.compat_write(),
                            agent_read.compat(),
                            |fut| {
                                tokio::task::spawn_local(fut);
                            },
                        );

                        // Agent notification forwarder
                        tokio::task::spawn_local(async move {
                            while let Some((notif, done)) = agent_rx.recv().await {
                                let _ = agent_conn.session_notification(notif).await;
                                done.send(()).ok();
                            }
                        });
                        tokio::task::spawn_local(agent_io);

                        // ── Client side ─────────────────────────────
                        let terminal_mgr = Arc::new(TerminalManager::new(PathBuf::from("/tmp")));
                        let client = SkillStarClient {
                            collected: collected_for_client,
                            on_chunk: Box::new(|_| {}),
                            terminals: terminal_mgr,
                        };
                        let (conn, client_io) = acp::ClientSideConnection::new(
                            client,
                            client_write.compat_write(),
                            client_read.compat(),
                            |fut| {
                                tokio::task::spawn_local(fut);
                            },
                        );
                        tokio::task::spawn_local(client_io);

                        // Initialize with capabilities
                        let init_resp = conn
                            .initialize(
                                acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                                    .client_info(acp::Implementation::new("test", "0.0.1"))
                                    .client_capabilities(
                                        acp::ClientCapabilities::new().terminal(true).fs(
                                            acp::FileSystemCapabilities::new()
                                                .read_text_file(true)
                                                .write_text_file(true),
                                        ),
                                    ),
                            )
                            .await
                            .expect("initialize should succeed");
                        assert_eq!(init_resp.agent_info.as_ref().unwrap().name, "mock-agent");

                        // New session
                        let session = conn
                            .new_session(acp::NewSessionRequest::new(std::path::PathBuf::from(
                                "/tmp",
                            )))
                            .await
                            .expect("new_session should succeed");
                        assert_eq!(session.session_id, "0".into());

                        // Prompt
                        let prompt_resp = conn
                            .prompt(acp::PromptRequest::new(
                                session.session_id,
                                vec!["test prompt".into()],
                            ))
                            .await
                            .expect("prompt should succeed");
                        assert_eq!(prompt_resp.stop_reason, acp::StopReason::EndTurn);
                    })
                    .await;
            });
        })
        .await
        .expect("integration test task should not panic");

        // Verify collected text contains the script
        let output = collected.lock().unwrap().clone();
        assert!(
            output.contains("```setup-script"),
            "output should contain setup-script block, got: {}",
            output
        );

        // Verify script extraction works on real ACP output
        let script = extract_script(&output).expect("should extract script");
        assert!(script.contains("echo hello"));
        assert!(script.contains("set -euo pipefail"));
    }
}

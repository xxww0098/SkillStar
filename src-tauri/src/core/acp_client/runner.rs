//! ACP session transport plus the setup/rebuild compatibility prompts.
//! The transport owns subprocess lifecycle, capability negotiation, bounded
//! multi-turn prompting, and per-turn response collection.

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::{Context, Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info};

use skillstar_core::infra::path_env;
use skillstar_core::infra::paths;

use super::client::{AcpAccessPolicy, AcpSetupResult, SkillStarClient};

const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

// ── Core Function ───────────────────────────────────────────────────

// Platform-specific scripting guidance. The agent both *runs* the script
// during the session and emits it for later re-use, so on Windows we must
// ask for a PowerShell script (no /bin/bash there) rather than a bash one.

#[cfg(windows)]
const SCRIPT_KIND: &str = "a PowerShell script for Windows";
#[cfg(windows)]
const SCRIPT_EXAMPLE: &str = "```setup-script\n#Requires -Version 5.1\n$ErrorActionPreference = 'Stop'\n# your working script here\n```";

#[cfg(not(windows))]
const SCRIPT_KIND: &str = "a bash script";
#[cfg(not(windows))]
const SCRIPT_EXAMPLE: &str =
    "```setup-script\n#!/bin/bash\nset -euo pipefail\n# your working script here\n```";

/// The prompt template sent to the Agent (platform-aware scripting).
fn setup_prompt() -> String {
    format!(
        r#"This repository needs a setup script to work properly.  Please:

1. Read the README.md and analyze the directory structure.
2. Identify what setup steps are needed (e.g. running `./setup`, `npm install`, `make`, etc.).
3. Write {SCRIPT_KIND} that performs the setup.  The script should be idempotent and use commands available on the current operating system.
4. Execute the script to verify it works.
5. At the very end of your response, output the **final working script** wrapped in a fenced code block with the language tag `setup-script`, like this:

{SCRIPT_EXAMPLE}

IMPORTANT: Only output the `setup-script` block AFTER you have successfully run the script and verified it works."#
    )
}

/// Prompt template for multi-skill repos that need rebuild (platform-aware).
///
/// Instructs the agent to build the repo AND produce a `skills-rebuild/`
/// directory containing one subdirectory per skill, each with a `SKILL.md`.
fn rebuild_prompt() -> String {
    format!(
        r#"This repository contains multiple skills that need a build step before they can be used individually.

Please:

1. Read the README.md, AGENTS.md, and analyze the directory structure.
2. Look for `SKILL.md` files in subdirectories — each one represents an individual skill.
3. Look for a build system (package.json, setup script, Makefile, etc.) and run the build.
4. After the build succeeds, create a directory called `skills-rebuild/` in the repo root.
5. For EACH skill found (each directory containing a SKILL.md), create a subdirectory in `skills-rebuild/` named after the skill.
6. Each `skills-rebuild/<skill-name>/` subdirectory must contain at minimum:
   - A `SKILL.md` file (copy from the original location, or use any generated/adapted version if one exists)
   - Any supporting files the skill references (scripts, binaries, etc.) — use symlinks to the originals when possible
7. If the repo has pre-generated skill directories (e.g. `.agent/skills/`, `.agents/skills/`, `.claude/skills/`) that already contain adapted SKILL.md files, prefer those.
8. The script should be idempotent — safe to re-run — and use commands available on the current operating system.
9. Do NOT include the root-level SKILL.md as a separate skill (it's a meta-skill for the whole repo).

At the very end output the **final working script** ({SCRIPT_KIND}) wrapped in a fenced code block with the language tag `setup-script`:

{SCRIPT_EXAMPLE}

CRITICAL: Only output the `setup-script` block AFTER you have successfully run the script and verified that `skills-rebuild/` exists and has at least one subdirectory with a SKILL.md."#
    )
}

/// Parse the agent's response to extract the setup-script block.
pub(crate) fn extract_script(response: &str) -> Option<String> {
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
    run_acp_with_prompt(agent_command, skill_name, &setup_prompt(), on_chunk).await
}

/// Run the ACP rebuild flow for a multi-skill repo.
///
/// Uses the rebuild prompt which instructs the agent to build the repo and
/// create a `skills-rebuild/` directory containing one subdirectory per skill.
pub async fn run_rebuild_via_acp(
    agent_command: &str,
    skill_name: &str,
    on_chunk: impl Fn(&str) + Send + Sync + 'static,
) -> Result<AcpSetupResult> {
    run_acp_with_prompt(agent_command, skill_name, &rebuild_prompt(), on_chunk).await
}

/// Run a bounded, read-only ACP conversation rooted at `work_dir`.
///
/// A single agent subprocess and ACP session are reused for every prompt. The
/// returned vector has one entry per prompt, and each entry contains only that
/// turn's agent text. The client exposes file reads but no writes or terminal
/// methods to the agent.
pub async fn run_read_only_conversation_via_acp(
    agent_command: &str,
    work_dir: &Path,
    prompts: &[String],
    on_chunk: impl Fn(&str) + Send + Sync + 'static,
) -> Result<Vec<String>> {
    run_acp_conversation(
        agent_command,
        work_dir,
        prompts,
        AcpAccessPolicy::ReadOnly,
        DEFAULT_TURN_TIMEOUT,
        on_chunk,
    )
    .await
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
    let prompts = [prompt.to_string()];
    let mut outputs = run_acp_conversation(
        agent_command,
        &work_dir,
        &prompts,
        AcpAccessPolicy::Full,
        DEFAULT_TURN_TIMEOUT,
        on_chunk,
    )
    .await?;
    let output = outputs.pop().unwrap_or_default();

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

pub(crate) fn capabilities_for_policy(policy: AcpAccessPolicy) -> acp::ClientCapabilities {
    match policy {
        AcpAccessPolicy::Full => {
            acp::ClientCapabilities::new()
                .terminal(true)
                .fs(acp::FileSystemCapabilities::new()
                    .read_text_file(true)
                    .write_text_file(true))
        }
        AcpAccessPolicy::ReadOnly => {
            acp::ClientCapabilities::new().fs(acp::FileSystemCapabilities::new()
                .read_text_file(true)
                .write_text_file(false))
        }
    }
}

pub(crate) async fn drive_prompt_turns(
    conn: &acp::ClientSideConnection,
    session_id: &acp::SessionId,
    prompts: &[String],
    collected: &Arc<Mutex<String>>,
    turn_timeout: Duration,
) -> Result<Vec<String>> {
    let mut outputs = Vec::with_capacity(prompts.len());

    for (index, prompt) in prompts.iter().enumerate() {
        collected
            .lock()
            .map_err(|_| anyhow!("ACP response buffer lock poisoned"))?
            .clear();

        let turn_number = index + 1;
        let response = tokio::time::timeout(
            turn_timeout,
            conn.prompt(acp::PromptRequest::new(
                session_id.clone(),
                vec![prompt.clone().into()],
            )),
        )
        .await
        .map_err(|_| {
            anyhow!(
                "ACP prompt turn {turn_number} timed out after {:?}",
                turn_timeout
            )
        })?
        .map_err(|error| anyhow!("ACP prompt turn {turn_number} failed: {error}"))?;

        if response.stop_reason != acp::StopReason::EndTurn {
            return Err(anyhow!(
                "ACP prompt turn {turn_number} stopped with {:?}",
                response.stop_reason
            ));
        }

        let output = collected
            .lock()
            .map_err(|_| anyhow!("ACP response buffer lock poisoned"))?
            .clone();
        outputs.push(output);
        info!(
            target: "acp_client",
            turn = turn_number,
            "ACP prompt turn completed"
        );
    }

    Ok(outputs)
}

async fn run_acp_conversation(
    agent_command: &str,
    work_dir: &Path,
    prompts: &[String],
    access_policy: AcpAccessPolicy,
    turn_timeout: Duration,
    on_chunk: impl Fn(&str) + Send + Sync + 'static,
) -> Result<Vec<String>> {
    if prompts.is_empty() {
        return Err(anyhow!("ACP conversation requires at least one prompt"));
    }

    let work_dir = std::fs::canonicalize(work_dir)
        .with_context(|| format!("Skill directory not found: {}", work_dir.display()))?;
    if !work_dir.is_dir() {
        return Err(anyhow!(
            "ACP working directory is not a directory: {}",
            work_dir.display()
        ));
    }

    info!(
        target: "acp_client",
        agent = %agent_command,
        dir = %work_dir.display(),
        turns = prompts.len(),
        policy = ?access_policy,
        "starting ACP conversation"
    );

    let parts: Vec<String> = agent_command.split_whitespace().map(String::from).collect();
    let program = parts
        .first()
        .ok_or_else(|| anyhow!("Empty agent command"))?
        .clone();
    let args: Vec<String> = parts[1..].to_vec();
    let agent_cmd_display = agent_command.to_string();
    let prompts = prompts.to_vec();
    let collected = Arc::new(Mutex::new(String::new()));
    let collected_for_client = collected.clone();
    let work_dir_for_session = work_dir.clone();

    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("Failed to create ACP runtime")?;

        rt.block_on(async move {
            let mut cmd = tokio::process::Command::new(&program);
            cmd.args(&args)
                .current_dir(&work_dir_for_session)
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
            let stderr_collected = Arc::new(Mutex::new(String::new()));
            let stderr_collected_bg = stderr_collected.clone();
            if let Some(mut stderr) = stderr_handle {
                tokio::task::spawn(async move {
                    use tokio::io::AsyncBufReadExt;
                    let reader = tokio::io::BufReader::new(&mut stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        info!(target: "acp_agent_stderr", "{}", line);
                        if let Ok(mut buffer) = stderr_collected_bg.lock() {
                            buffer.push_str(&line);
                            buffer.push('\n');
                        }
                    }
                });
            }

            let local_set = tokio::task::LocalSet::new();
            let session_result = local_set
                .run_until(async move {
                    let client = SkillStarClient::new(
                        collected_for_client.clone(),
                        on_chunk,
                        work_dir_for_session.clone(),
                        access_policy,
                    );
                    let (conn, handle_io) =
                        acp::ClientSideConnection::new(client, stdin, stdout, |future| {
                            tokio::task::spawn_local(future);
                        });
                    tokio::task::spawn_local(handle_io);

                    conn.initialize(
                        acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                            .client_info(
                                acp::Implementation::new("skillstar", env!("CARGO_PKG_VERSION"))
                                    .title("SkillStar"),
                            )
                            .client_capabilities(capabilities_for_policy(access_policy)),
                    )
                    .await
                    .map_err(|error| anyhow!("ACP initialize failed: {error}"))?;

                    let session = conn
                        .new_session(acp::NewSessionRequest::new(work_dir_for_session.clone()))
                        .await
                        .map_err(|error| anyhow!("ACP new_session failed: {error}"))?;
                    info!(target: "acp_client", session_id = %session.session_id, "ACP session created");

                    drive_prompt_turns(
                        &conn,
                        &session.session_id,
                        &prompts,
                        &collected_for_client,
                        turn_timeout,
                    )
                    .await
                })
                .await;

            let stderr_text = stderr_collected
                .lock()
                .map(|buffer| buffer.clone())
                .unwrap_or_default();
            let _ = child.kill().await;

            match session_result {
                Err(error) if !stderr_text.is_empty() => {
                    error!(target: "acp_client", stderr = %stderr_text, "agent stderr output");
                    Err(anyhow!("{error}\n\nAgent stderr:\n{stderr_text}"))
                }
                result => result,
            }
        })
    })
    .await
    .context("ACP task panicked")?
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

    if let Ok(rel) = canonical.strip_prefix(&repos_cache)
        && let Some(first) = rel.components().next()
    {
        return repos_cache.join(first);
    }
    canonical
}

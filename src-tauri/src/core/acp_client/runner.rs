//! ACP session transport: subprocess lifecycle, capability negotiation,
//! bounded multi-turn prompting, and per-turn response collection.

use agent_client_protocol::{self as acp, Agent as _};
use anyhow::{Context, Result, anyhow};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info};

use skillstar_core::infra::path_env;

use super::client::SkillStarClient;

const TURN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

// ── Core Function ───────────────────────────────────────────────────

/// The only capabilities we ever advertise: text reads, no writes, no
/// terminals.
pub(crate) fn read_only_capabilities() -> acp::ClientCapabilities {
    acp::ClientCapabilities::new().fs(acp::FileSystemCapabilities::new()
        .read_text_file(true)
        .write_text_file(false))
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
        "starting read-only ACP conversation"
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
                            .client_capabilities(read_only_capabilities()),
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
                        TURN_TIMEOUT,
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

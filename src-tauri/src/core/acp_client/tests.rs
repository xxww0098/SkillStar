use super::client::SkillStarClient;
use super::runner::drive_prompt_turns;
use agent_client_protocol::{self as acp, Agent as _, Client as _};
use skillstar_core::config::acp::AcpConfig;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod permission_tests;

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
        ..AcpConfig::default()
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

// ── Prompt driver over a real ACP connection ────────────────────

enum ScriptedTurn {
    Text(Vec<String>),
    Stop(acp::StopReason),
    Error,
    Delay(Duration),
}

struct ScriptedAgent {
    session_update_tx: tokio::sync::mpsc::UnboundedSender<(
        acp::SessionNotification,
        tokio::sync::oneshot::Sender<()>,
    )>,
    turns: RefCell<VecDeque<ScriptedTurn>>,
    seen_prompts: Arc<Mutex<Vec<String>>>,
    seen_session_ids: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait(?Send)]
impl acp::Agent for ScriptedAgent {
    async fn initialize(
        &self,
        _args: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        Ok(acp::InitializeResponse::new(acp::ProtocolVersion::V1))
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
        Ok(acp::NewSessionResponse::new("shared-session"))
    }

    async fn load_session(
        &self,
        _args: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        Ok(acp::LoadSessionResponse::new())
    }

    async fn prompt(&self, args: acp::PromptRequest) -> Result<acp::PromptResponse, acp::Error> {
        let prompt_text = args
            .prompt
            .iter()
            .find_map(|block| match block {
                acp::ContentBlock::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        self.seen_prompts.lock().unwrap().push(prompt_text);
        self.seen_session_ids
            .lock()
            .unwrap()
            .push(args.session_id.to_string());

        let turn = self
            .turns
            .borrow_mut()
            .pop_front()
            .ok_or_else(acp::Error::internal_error)?;
        match turn {
            ScriptedTurn::Text(chunks) => {
                for text in chunks {
                    let notification = acp::SessionNotification::new(
                        args.session_id.clone(),
                        acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                            acp::ContentBlock::Text(acp::TextContent::new(text)),
                        )),
                    );
                    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
                    self.session_update_tx
                        .send((notification, done_tx))
                        .map_err(|_| acp::Error::internal_error())?;
                    done_rx.await.map_err(|_| acp::Error::internal_error())?;
                }
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
            ScriptedTurn::Stop(reason) => Ok(acp::PromptResponse::new(reason)),
            ScriptedTurn::Error => Err(acp::Error::internal_error()),
            ScriptedTurn::Delay(duration) => {
                tokio::time::sleep(duration).await;
                Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
            }
        }
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

async fn run_scripted_conversation(
    turns: Vec<ScriptedTurn>,
    prompts: &[String],
    timeout: Duration,
) -> (anyhow::Result<Vec<String>>, Vec<String>, Vec<String>) {
    use tokio::io::duplex;
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    let (client_write, agent_read) = duplex(64 * 1024);
    let (agent_write, client_read) = duplex(64 * 1024);
    let seen_prompts = Arc::new(Mutex::new(Vec::new()));
    let seen_session_ids = Arc::new(Mutex::new(Vec::new()));
    let seen_prompts_for_agent = seen_prompts.clone();
    let seen_sessions_for_agent = seen_session_ids.clone();
    let prompts = prompts.to_vec();

    let result = tokio::task::LocalSet::new()
        .run_until(async move {
            let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel();
            let agent = ScriptedAgent {
                session_update_tx: agent_tx,
                turns: RefCell::new(turns.into()),
                seen_prompts: seen_prompts_for_agent,
                seen_session_ids: seen_sessions_for_agent,
            };
            let (agent_conn, agent_io) = acp::AgentSideConnection::new(
                agent,
                agent_write.compat_write(),
                agent_read.compat(),
                |future| {
                    tokio::task::spawn_local(future);
                },
            );
            tokio::task::spawn_local(async move {
                while let Some((notification, done)) = agent_rx.recv().await {
                    let _ = agent_conn.session_notification(notification).await;
                    let _ = done.send(());
                }
            });
            tokio::task::spawn_local(agent_io);

            let collected = Arc::new(Mutex::new(String::new()));
            let client = SkillStarClient::new(collected.clone(), |_| {}, std::env::temp_dir());
            let (conn, client_io) = acp::ClientSideConnection::new(
                client,
                client_write.compat_write(),
                client_read.compat(),
                |future| {
                    tokio::task::spawn_local(future);
                },
            );
            tokio::task::spawn_local(client_io);

            conn.initialize(acp::InitializeRequest::new(acp::ProtocolVersion::V1))
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let session = conn
                .new_session(acp::NewSessionRequest::new(std::env::temp_dir()))
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            drive_prompt_turns(&conn, &session.session_id, &prompts, &collected, timeout).await
        })
        .await;

    let prompts = seen_prompts.lock().unwrap().clone();
    let sessions = seen_session_ids.lock().unwrap().clone();
    (result, prompts, sessions)
}

#[tokio::test]
async fn prompt_driver_reuses_session_and_clears_each_turn_output() {
    let prompts = vec!["analyze".to_string(), "draft".to_string()];
    let (result, seen_prompts, session_ids) = run_scripted_conversation(
        vec![
            ScriptedTurn::Text(vec!["first ".to_string(), "turn".to_string()]),
            ScriptedTurn::Text(vec!["second turn".to_string()]),
        ],
        &prompts,
        Duration::from_secs(1),
    )
    .await;

    assert_eq!(result.unwrap(), vec!["first turn", "second turn"]);
    assert_eq!(seen_prompts, prompts);
    assert_eq!(session_ids, vec!["shared-session", "shared-session"]);
}

#[tokio::test]
async fn prompt_driver_rejects_non_end_turn_stop_reason() {
    let prompts = vec!["draft".to_string()];
    let (result, _, _) = run_scripted_conversation(
        vec![ScriptedTurn::Stop(acp::StopReason::MaxTokens)],
        &prompts,
        Duration::from_secs(1),
    )
    .await;

    let error = result.unwrap_err().to_string();
    assert!(error.contains("MaxTokens"), "unexpected error: {error}");
}

#[tokio::test]
async fn prompt_driver_propagates_protocol_error() {
    let prompts = vec!["draft".to_string()];
    let (result, _, _) =
        run_scripted_conversation(vec![ScriptedTurn::Error], &prompts, Duration::from_secs(1))
            .await;

    let error = result.unwrap_err().to_string();
    assert!(error.contains("turn 1 failed"), "unexpected error: {error}");
}

#[tokio::test]
async fn prompt_driver_times_out_slow_turn() {
    let prompts = vec!["draft".to_string()];
    let (result, _, _) = run_scripted_conversation(
        vec![ScriptedTurn::Delay(Duration::from_millis(50))],
        &prompts,
        Duration::from_millis(5),
    )
    .await;

    let error = result.unwrap_err().to_string();
    assert!(error.contains("timed out"), "unexpected error: {error}");
}

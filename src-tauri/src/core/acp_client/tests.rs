    use super::*;
    use agent_client_protocol::{self as acp, Agent as _, Client as _};
    use skillstar_core::config::acp::AcpConfig;
    use std::sync::{Arc, Mutex};

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
                    terminals: Arc::new(TerminalManager::new(std::env::temp_dir())),
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

        tokio::task::spawn_blocking(move || {
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
                        let terminal_mgr = Arc::new(TerminalManager::new(std::env::temp_dir()));
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
                            .new_session(acp::NewSessionRequest::new(std::env::temp_dir()))
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

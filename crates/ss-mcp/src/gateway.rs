//! Grok 1.0.40 opens stdio with `initialize` at `2025-11-25`.
//! This process answers that handshake as `2026-07-28` and forwards every
//! later call to the server with the per-request metadata that revision
//! requires. The server itself still rejects a legacy session.

use anyhow::Result;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use super::stdio::serve_transport;

const PROTOCOL: &str = "2026-07-28";

pub(crate) async fn serve_gateway<R, W>(read: R, write: W) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (inner_client, inner_server) = tokio::io::duplex(256 * 1024);
    let server = tokio::spawn(serve_transport(inner_server));
    let (inner_read, mut inner_write) = tokio::io::split(inner_client);
    let mut inner_lines = BufReader::new(inner_read).lines();
    write_json(
        &mut inner_write,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {"_meta": base_meta(json!({}), json!({"name": "skillstar-gateway", "version": "0"}))}
        }),
    )
    .await?;
    let discovered = next_value(&mut inner_lines).await?;
    if discovered.get("result").is_none() {
        anyhow::bail!("mcp discover failed: {discovered}");
    }

    let (outbound, mut outbound_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut write = write;
        while let Some(line) = outbound_rx.recv().await {
            write.write_all(line.as_bytes()).await?;
            write.write_all(b"\n").await?;
            write.flush().await?;
        }
        Ok::<(), std::io::Error>(())
    });
    let to_client = outbound.clone();
    let pump = tokio::spawn(async move {
        while let Ok(Some(line)) = inner_lines.next_line().await {
            if line.is_empty() {
                continue;
            }
            if to_client.send(line).is_err() {
                break;
            }
        }
    });

    let mut capabilities = json!({});
    let mut client_info = json!({"name": "client", "version": "0"});
    let mut client_lines = BufReader::new(read).lines();
    while let Some(line) = client_lines.next_line().await? {
        if line.is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(msg) => msg,
            Err(err) => {
                let parse_error = json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {err}"),
                    }
                });
                if outbound.send(serde_json::to_string(&parse_error)?).is_err() {
                    break;
                }
                continue;
            }
        };
        match message.get("method").and_then(Value::as_str) {
            Some("initialize") => {
                if let Some(value) = message["params"].get("capabilities") {
                    capabilities = value.clone();
                }
                if let Some(value) = message["params"].get("clientInfo") {
                    client_info = value.clone();
                }
                let reply = json!({
                    "jsonrpc": "2.0",
                    "id": message.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "protocolVersion": PROTOCOL,
                        "capabilities": discovered["result"]["capabilities"].clone(),
                        "serverInfo": server_info(&discovered),
                        "instructions": discovered["result"]["instructions"].clone(),
                    }
                });
                outbound.send(serde_json::to_string(&reply)?)?;
            }
            Some("notifications/initialized") => {}
            Some("server/discover") => {
                write_json(&mut inner_write, &message).await?;
            }
            Some(_) => {
                write_json(
                    &mut inner_write,
                    &stamp(message, &capabilities, &client_info),
                )
                .await?;
            }
            None => write_json(&mut inner_write, &message).await?,
        }
    }
    drop(inner_write);
    drop(outbound);
    let _ = pump.await;
    let _ = writer.await;
    server.await??;
    Ok(())
}

fn server_info(discovered: &Value) -> Value {
    discovered["result"]["_meta"]
        .get("io.modelcontextprotocol/serverInfo")
        .cloned()
        .filter(|value| value.is_object())
        .unwrap_or_else(|| json!({"name": "skillstar", "version": "0.0.0"}))
}

fn stamp(mut message: Value, capabilities: &Value, client_info: &Value) -> Value {
    let Some(object) = message.as_object_mut() else {
        return message;
    };
    let params = object.entry("params").or_insert_with(|| json!({}));
    if !params.is_object() {
        *params = json!({});
    }
    let mut meta = base_meta(capabilities.clone(), client_info.clone());
    if let Some(existing) = params.get("_meta").and_then(Value::as_object)
        && let Some(target) = meta.as_object_mut()
    {
        for (key, value) in existing {
            target.insert(key.clone(), value.clone());
        }
    }
    params["_meta"] = meta;
    message
}

fn base_meta(capabilities: Value, client_info: Value) -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": PROTOCOL,
        "io.modelcontextprotocol/clientInfo": client_info,
        "io.modelcontextprotocol/clientCapabilities": capabilities,
    })
}

async fn write_json<W>(write: &mut W, message: &Value) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    write.write_all(&line).await?;
    write.flush().await?;
    Ok(())
}

async fn next_value<R>(lines: &mut tokio::io::Lines<R>) -> Result<Value>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    let line = lines
        .next_line()
        .await?
        .ok_or_else(|| anyhow::anyhow!("mcp server closed before discover"))?;
    Ok(serde_json::from_str(&line)?)
}

#[cfg(test)]
mod tests {
    use super::serve_gateway;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    #[test]
    fn grok_initialize_upgrades_to_the_latest_protocol() {
        let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
        let temp = tempfile::tempdir().unwrap();
        let previous_home = std::env::var_os("HOME");
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            std::env::set_var("HOME", temp.path().join("home"));
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let (client_read, mut client_write) = tokio::io::split(client);
            let (server_read, server_write) = tokio::io::split(server);
            let gateway = tokio::spawn(serve_gateway(server_read, server_write));
            let initialize = r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"elicitation":{"form":{}}},"clientInfo":{"name":"grok","version":"1"}}}"#;
            client_write.write_all(initialize.as_bytes()).await.unwrap();
            client_write.write_all(b"\n").await.unwrap();
            client_write
                .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
                .await
                .unwrap();
            client_write
                .write_all(
                    b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{\"_meta\":{\"progressToken\":0}}}\n",
                )
                .await
                .unwrap();
            client_write.shutdown().await.unwrap();

            let mut incoming = String::new();
            let mut reader = tokio::io::BufReader::new(client_read).lines();
            for _ in 0..2 {
                let line = tokio::time::timeout(std::time::Duration::from_secs(5), reader.next_line())
                    .await
                    .expect("timed out waiting for gateway")
                    .unwrap()
                    .expect("gateway closed");
                incoming.push_str(&line);
                incoming.push('\n');
            }
            let frames: Vec<serde_json::Value> = incoming
                .lines()
                .filter(|line| !line.is_empty())
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(frames[0]["result"]["protocolVersion"], "2026-07-28");
            assert!(frames[0]["result"].get("error").is_none());
            let tools = frames[1]["result"]["tools"].as_array().unwrap();
            assert_eq!(tools.len(), 3);
            gateway.abort();
        });
        unsafe {
            restore("HOME", previous_home);
            restore("SKILLSTAR_DATA_DIR", previous_data);
        }
    }

    unsafe fn restore(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var(key, value) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    #[test]
    fn stamp_preserves_client_declared_protocol_version() {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "server/discover",
            "params": {
                "_meta": {
                    "io.modelcontextprotocol/protocolVersion": "2025-11-25"
                }
            }
        });
        let stamped = super::stamp(
            msg,
            &serde_json::json!({}),
            &serde_json::json!({"name": "legacy", "version": "1"}),
        );
        assert_eq!(
            stamped["params"]["_meta"]["io.modelcontextprotocol/protocolVersion"],
            "2025-11-25"
        );
    }

    #[test]
    fn gateway_returns_parse_error_and_rejects_legacy_discover_without_crashing() {
        let _lock = futures::executor::block_on(crate::test_support::ENV_LOCK.lock());
        let temp = tempfile::tempdir().unwrap();
        let previous_home = std::env::var_os("HOME");
        let previous_data = std::env::var_os("SKILLSTAR_DATA_DIR");
        unsafe {
            std::env::set_var("HOME", temp.path().join("home"));
            std::env::set_var("SKILLSTAR_DATA_DIR", temp.path().join("data"));
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let (client_read, mut client_write) = tokio::io::split(client);
            let (server_read, server_write) = tokio::io::split(server);
            let gateway = tokio::spawn(serve_gateway(server_read, server_write));

            // 1. Malformed JSON line must emit -32700 and keep the loop alive.
            client_write.write_all(b"{bad json\n").await.unwrap();
            // 2. server/discover declaring legacy protocolVersion must not be rewritten to 2026-07-28.
            client_write
                .write_all(
                    br#"{"jsonrpc":"2.0","id":2,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2025-11-25","io.modelcontextprotocol/clientInfo":{"name":"legacy","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}}}"#,
                )
                .await
                .unwrap();
            client_write.write_all(b"\n").await.unwrap();
            // 3. Subsequent valid request after initialize still succeeds cleanly.
            client_write
                .write_all(
                    br#"{"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"grok","version":"1"}}}"#,
                )
                .await
                .unwrap();
            client_write.write_all(b"\n").await.unwrap();
            client_write
                .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/list\",\"params\":{}}\n")
                .await
                .unwrap();
            client_write.shutdown().await.unwrap();

            let mut frames = Vec::new();
            let mut reader = tokio::io::BufReader::new(client_read).lines();
            while frames.len() < 4 {
                let line = tokio::time::timeout(std::time::Duration::from_secs(5), reader.next_line())
                    .await
                    .expect("timed out waiting for gateway")
                    .unwrap()
                    .expect("gateway closed early");
                if !line.is_empty() {
                    frames.push(serde_json::from_str::<serde_json::Value>(&line).unwrap());
                }
            }
            assert_eq!(frames[0]["error"]["code"], -32700, "{:?}", frames[0]);
            let discover_resp = frames
                .iter()
                .find(|f| f["id"] == 2)
                .unwrap_or_else(|| panic!("missing id 2 in {frames:?}"));
            assert!(
                discover_resp.get("error").is_some(),
                "legacy discover must error: {discover_resp:?}"
            );
            let init_resp = frames
                .iter()
                .find(|f| f["id"] == 3)
                .unwrap_or_else(|| panic!("missing id 3 in {frames:?}"));
            assert_eq!(init_resp["result"]["protocolVersion"], "2026-07-28");
            let tools_resp = frames
                .iter()
                .find(|f| f["id"] == 4)
                .unwrap_or_else(|| panic!("missing id 4 in {frames:?}"));
            assert_eq!(tools_resp["result"]["tools"].as_array().unwrap().len(), 3);
            gateway.abort();
        });
        unsafe {
            restore("HOME", previous_home);
            restore("SKILLSTAR_DATA_DIR", previous_data);
        }
    }
}

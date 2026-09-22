//! Process entry for `skillstar mcp serve --stdio`.
//!
//! stdout carries newline-delimited JSON-RPC only. Logs go to stderr.
//! Marketplace snapshot initialization stays on the GUI and CLI paths.

use std::io::Write;

use rmcp::ServiceExt;
use rmcp::transport::IntoTransport;

use super::protocol::ProjectSkillsMcp;

/// `argv[1] == "mcp"`. Main checks this before Git askpass and the GUI.
pub fn is_mcp_invocation(args: &[String]) -> bool {
    args.get(1).map(String::as_str) == Some("mcp")
}

/// `skillstar mcp serve ...` stays on the stdio serve path, including a
/// missing `--stdio`, so the usage error does not fall through to the CLI.
pub fn is_mcp_serve(args: &[String]) -> bool {
    is_mcp_invocation(args) && args.get(2).map(String::as_str) == Some("serve")
}

/// Run the stdio server, or report a usage error on stderr.
///
/// Returns a process exit code. Does not call marketplace snapshot
/// `initialize`.
pub fn serve() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    serve_with(&args, &mut std::io::stderr())
}

pub fn serve_with(args: &[String], stderr: &mut dyn Write) -> i32 {
    if !is_stdio_serve(args) {
        let _ = writeln!(
            stderr,
            "usage: skillstar mcp serve --stdio\nstdout is reserved for JSON-RPC"
        );
        return 2;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            let _ = writeln!(stderr, "{err}");
            return 1;
        }
    };
    // stdin()/stdout() must be created inside the runtime. Grok still
    // opens with initialize; the gateway answers 2026-07-28 and forwards.
    match runtime.block_on(async {
        super::gateway::serve_gateway(tokio::io::stdin(), tokio::io::stdout()).await
    }) {
        Ok(()) => 0,
        Err(err) => {
            let _ = writeln!(stderr, "{err:#}");
            1
        }
    }
}

fn is_stdio_serve(args: &[String]) -> bool {
    args.get(1).map(String::as_str) == Some("mcp")
        && args.get(2).map(String::as_str) == Some("serve")
        && args.get(3).map(String::as_str) == Some("--stdio")
        && args.len() == 4
}

pub(crate) async fn serve_transport<T, E, A>(transport: T) -> anyhow::Result<()>
where
    T: IntoTransport<rmcp::RoleServer, E, A>,
    E: std::error::Error + Send + Sync + 'static,
{
    prepare_process();
    let running = ProjectSkillsMcp::new()
        .serve(transport)
        .await
        .map_err(|err| anyhow::anyhow!("mcp initialize failed: {err}"))?;
    running
        .waiting()
        .await
        .map_err(|err| anyhow::anyhow!("mcp server stopped: {err}"))?;
    Ok(())
}

fn prepare_process() {
    install_stderr_tracing();
    skillstar_channels::policy::install_global_policy();
    skillstar_core::infra::migration::migrate_legacy_paths();
}

fn install_stderr_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("error"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
}

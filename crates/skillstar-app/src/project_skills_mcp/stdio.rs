//! Process entry for `skillstar mcp serve --stdio`.
//!
//! stdout carries newline-delimited JSON-RPC only. Logs go to stderr.
//! Marketplace snapshot initialization stays on the GUI and CLI paths.

use std::io::Write;

use rmcp::ServiceExt;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::IntoTransport;

/// `argv[1] == "mcp"`. Main checks this before Git askpass and the GUI.
pub fn is_mcp_invocation(args: &[String]) -> bool {
    args.get(1).map(String::as_str) == Some("mcp")
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
    // stdin()/stdout() must be created inside the runtime.
    match runtime.block_on(async { serve_transport(rmcp::transport::stdio()).await }) {
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
    let running = ProjectSkillsMcp
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

/// No business tools yet. Later tools replace this handler without changing
/// the process rules above.
#[derive(Clone)]
struct ProjectSkillsMcp;

impl ServerHandler for ProjectSkillsMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("skillstar", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "SkillStar project skills. This process advertises no business tools yet.",
            )
    }
}

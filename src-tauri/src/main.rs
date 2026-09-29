// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
unsafe extern "system" {
    fn SetErrorMode(uMode: u32) -> u32;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Claude Code starts this process as an MCP helper. stdout is only JSON-RPC
    // frames, so this returns before askpass, marketplace migration, and the window.
    if args.get(1).map(String::as_str) == Some("claude-mcp-helper") {
        let code = skillstar_app::cli::run_claude_mcp_helper(&args);
        std::process::exit(code);
    }
    // MCP serve must run before askpass. Askpass prints to stdout and returns
    // when SKILLSTAR_GIT_ASKPASS_MODE=1, which would swallow the JSON-RPC stream.
    // Other `mcp` commands, including `mcp approve`, also skip askpass, then
    // use the normal CLI so their text stays out of the serve stdout.
    if skillstar_app::project_skills_mcp::is_mcp_serve(&args) {
        let code = skillstar_app::project_skills_mcp::serve();
        std::process::exit(code);
    }
    if !skillstar_app::project_skills_mcp::is_mcp_invocation(&args)
        && skillstar_git::transport::handle_internal_askpass(&args)
    {
        return;
    }

    #[cfg(windows)]
    unsafe {
        // SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX
        SetErrorMode(0x0001 | 0x0002 | 0x8000);
    }

    // CLI mode: first arg is a known subcommand owned by skillstar-app's Clap surface.
    // Unknown args fall through to GUI so deep-links / OS launchers still work.
    // `gateway` is recognized here, before a window, and calls the app serve.
    if args.len() > 1 {
        let first_arg = args[1].as_str();
        if first_arg == "gateway" {
            let code = skillstar_app::cli::run_gateway(&args);
            std::process::exit(code);
        }
        if skillstar_app::cli::is_gui_force_arg(first_arg) {
            // Fall through to GUI mode
        } else if skillstar_app::cli::is_cli_subcommand(first_arg) {
            skillstar_lib::run_cli(args);
            return;
        }
    }

    // GUI mode. The listener is the same serve the CLI calls, and it stays up
    // for the life of this process, including after the Models page is left.
    skillstar_app::cli::start_desktop_gateway();
    skillstar_lib::run();
}

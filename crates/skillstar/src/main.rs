//! Process entry for the skillstar binary.
//!
//! MCP serve runs before askpass and before any GUI or marketplace init.
//! Unknown arguments fall through to the GPUI shell so OS launchers can
//! still start the app. gui and the old gui-gpui alias both open that shell.
//! There is no Tauri path.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
unsafe extern "system" {
    fn SetErrorMode(uMode: u32) -> u32;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // MCP serve must run before askpass. Askpass prints to stdout and returns
    // when SKILLSTAR_GIT_ASKPASS_MODE=1, which would swallow the JSON-RPC stream.
    // Other mcp commands, including mcp approve, also skip askpass, then use
    // the normal CLI so their text stays out of the serve stdout.
    if ss_app::project_skills_mcp::is_mcp_serve(&args) {
        let code = ss_app::project_skills_mcp::serve();
        std::process::exit(code);
    }
    if !ss_app::project_skills_mcp::is_mcp_invocation(&args)
        && ss_git::transport::handle_internal_askpass(&args)
    {
        return;
    }

    suppress_windows_error_dialogs();

    if args.len() > 1 {
        let first_arg = args[1].as_str();
        if is_gui_arg(first_arg) {
            launch_gui();
            return;
        }
        if ss_app::cli::is_cli_subcommand(first_arg) {
            ss_app::cli::run(args, ss_app::bootstrap::prepare_process);
            return;
        }
    }

    launch_gui();
}

fn is_gui_arg(first_arg: &str) -> bool {
    ss_app::cli::is_gui_force_arg(first_arg) || first_arg == "gui-gpui"
}

fn launch_gui() {
    if let Err(err) = ss_gpui::run() {
        eprintln!("skillstar gui failed: {err}");
        std::process::exit(1);
    }
}

fn suppress_windows_error_dialogs() {
    #[cfg(windows)]
    unsafe {
        // SAFETY: SetErrorMode takes a documented SEM_* bitmask and returns the
        // previous mode. It does not dereference caller memory. Do not call
        // AllocConsole here: MCP serve inherits the parent's redirected pipes.
        SetErrorMode(0x0001 | 0x0002 | 0x8000);
    }
}

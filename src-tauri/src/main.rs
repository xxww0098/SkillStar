// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
unsafe extern "system" {
    fn SetErrorMode(uMode: u32) -> u32;
}

fn main() {
    #[cfg(windows)]
    unsafe {
        // SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX
        SetErrorMode(0x0001 | 0x0002 | 0x8000);
    }

    let args: Vec<String> = std::env::args().collect();

    // CLI mode: first arg is a known subcommand owned by skillstar-app's Clap surface.
    // Unknown args fall through to GUI so deep-links / OS launchers still work.
    if args.len() > 1 {
        let first_arg = args[1].as_str();
        if skillstar_app::cli::is_gui_force_arg(first_arg) {
            // Fall through to GUI mode
        } else if skillstar_app::cli::is_cli_subcommand(first_arg) {
            skillstar_lib::run_cli(args);
            return;
        }
    }

    // GUI mode
    skillstar_lib::run();
}

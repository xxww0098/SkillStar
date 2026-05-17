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

    // CLI mode: if there are arguments and the first arg is a known subcommand
    if args.len() > 1 {
        let first_arg = &args[1];
        let cli_commands = [
            "list",
            "install",
            "update",
            "create",
            "publish",
            "scan",
            "doctor",
            "pack",
            "launch",
            "gui",
            "-h",
            "--help",
            "-V",
            "--version",
        ];

        if cli_commands.contains(&first_arg.as_str()) {
            if first_arg == "gui" {
                // Fall through to GUI mode
            } else {
                skillstar_lib::run_cli(args);
                return;
            }
        }
    }

    // GUI mode
    skillstar_lib::run();
}

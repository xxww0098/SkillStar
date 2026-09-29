//! `skillstar gateway` entry. The desktop process calls the same `serve`.

use std::io::{self, Write};

use skillstar_gateway::{ServeOptions, serve};

/// Subcommand of `skillstar gateway`.
#[derive(clap::Subcommand)]
pub enum GatewayCli {
    /// Listen on the loopback address. Does not open a window.
    Serve,
}

/// Run `gateway` from argv. Missing `serve` writes to `stderr` and returns non-zero.
/// stdout is left untouched.
pub fn run_gateway(args: &[String]) -> i32 {
    run_gateway_io(args, &mut io::stdout(), &mut io::stderr())
}

pub fn run_gateway_io(args: &[String], stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
    let _ = stdout;
    if args.get(2).map(String::as_str) != Some("serve") || args.len() != 3 {
        let _ = writeln!(stderr, "用法: skillstar gateway serve");
        return 2;
    }
    match ServeOptions::from_env() {
        Ok(options) => match serve(options) {
            Ok(()) => 0,
            Err(error) => {
                let _ = writeln!(stderr, "{error}");
                1
            }
        },
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            1
        }
    }
}

/// Parsed clap form of [`run_gateway`].
pub fn run_parsed(command: Option<GatewayCli>) -> i32 {
    let mut args = vec!["skillstar".to_string(), "gateway".to_string()];
    if command.is_some() {
        args.push("serve".to_string());
    }
    run_gateway(&args)
}

/// Start the listener on a background thread. A bind failure is written to
/// stderr and does not stop the desktop process, and does not stop a listener
/// that already holds the address.
pub fn start_desktop_gateway() {
    let _ = std::thread::Builder::new()
        .name("skillstar-gateway".into())
        .spawn(|| {
            let _ = run_gateway(&[
                "skillstar".to_string(),
                "gateway".to_string(),
                "serve".to_string(),
            ]);
        });
}

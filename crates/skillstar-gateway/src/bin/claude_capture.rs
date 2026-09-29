//! Test double for the `claude` binary. Integration tests copy this executable
//! onto `PATH` as `claude`. It records argv and the environment, optionally
//! writes 2 MiB of stderr, then sleeps until it is killed.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::exit;
use std::thread;
use std::time::Duration;

fn main() {
    if let Err(error) = capture() {
        let _ = writeln!(io::stderr(), "{error}");
        exit(1);
    }
    if env::var_os("SKILLSTAR_CLAUDE_FLOOD").is_some()
        && let Err(error) = flood()
    {
        let _ = writeln!(io::stderr(), "{error}");
        exit(1);
    }
    thread::sleep(Duration::from_secs(24 * 60 * 60));
}

fn capture() -> io::Result<()> {
    let Some(dir) = env::var_os("SKILLSTAR_CLAUDE_CAPTURE") else {
        return Ok(());
    };
    let dir = PathBuf::from(dir);
    let mut args_text = String::new();
    for arg in env::args().skip(1) {
        args_text.push_str(&arg);
        args_text.push('\n');
    }
    atomic_write(&dir.join("args"), args_text.as_bytes())?;
    let mut env_text = String::new();
    for (key, value) in env::vars() {
        env_text.push_str(&key);
        env_text.push('=');
        env_text.push_str(&value);
        env_text.push('\n');
    }
    atomic_write(&dir.join("env"), env_text.as_bytes())
}

fn flood() -> io::Result<()> {
    let mut err = io::stderr().lock();
    err.write_all(b"HEAD")?;
    let zeros = [0u8; 8192];
    let mut left = 2 * 1024 * 1024;
    while left > 0 {
        let n = left.min(zeros.len());
        err.write_all(&zeros[..n])?;
        left -= n;
    }
    err.flush()?;
    if let Some(dir) = env::var_os("SKILLSTAR_CLAUDE_CAPTURE") {
        fs::write(PathBuf::from(dir).join("stderr_done"), b"")?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

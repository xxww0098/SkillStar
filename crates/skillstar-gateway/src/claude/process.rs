//! Spawn and reap one `claude` process.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};

use super::{
    ADDED_ENV, AccountSnapshot, ClaudeError, ClaudeLaunch, ClaudeRun, ClaudeTool, Inner, Life,
    RunInner, STDERR_CAP, STRIPPED_ENV, TEMP_PREFIX, TURN_ABORT, random_hex, table,
};

pub(super) fn launch_process(
    inner: &Arc<Inner>,
    spec: &ClaudeLaunch,
    binary: &Path,
    env: Vec<(String, String)>,
) -> Result<ClaudeRun, ClaudeError> {
    let token = random_hex(24);
    let temp = make_temp()?;
    let tools_path = temp.join("tools.json");
    if let Err(error) = write_tools(&tools_path, &spec.tools) {
        let _ = fs::remove_dir_all(&temp);
        return Err(error);
    }
    let callback = callback_url(&spec.callback_base, &token);
    let mcp = mcp_config(&spec.executable, &callback, &tools_path);
    let args = cli_args(&spec.model, &mcp, &spec.effort, spec.web_search);
    let mut command = Command::new(binary);
    command
        .args(&args)
        .current_dir(&temp)
        .env_clear()
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate_process_group(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_dir_all(&temp);
            return Err(ClaudeError::Io(error));
        }
    };
    let pid = child.id();
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let run = Arc::new(RunInner {
        token: token.clone(),
        temp,
        pid,
        clock: Arc::clone(&inner.clock),
        life: Mutex::new(Life {
            child: Some(child),
            stdin,
            claimed: false,
        }),
        stderr: Mutex::new(Vec::new()),
        pending: Mutex::new(HashMap::new()),
        deadline_ms: Mutex::new(0),
        generation: AtomicU64::new(0),
        finished: AtomicBool::new(false),
    });
    let now = inner.clock.now_ms();
    run.arm(now.saturating_add(TURN_ABORT.as_millis()));
    table(inner).by_token.insert(token, Arc::clone(&run));
    if let Err(error) = spawn_io(&run, stdout, stderr, inner) {
        inner.release(&run.token);
        return Err(error);
    }
    Ok(ClaudeRun {
        bridge: Arc::clone(inner),
        run,
    })
}

fn spawn_io(
    run: &Arc<RunInner>,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    inner: &Arc<Inner>,
) -> Result<(), ClaudeError> {
    if let Some(stdout) = stdout {
        thread::Builder::new()
            .name("skillstar-claude-out".into())
            .spawn(move || discard(stdout))
            .map_err(ClaudeError::Io)?;
    }
    if let Some(stderr) = stderr {
        let run_err = Arc::clone(run);
        thread::Builder::new()
            .name("skillstar-claude-err".into())
            .spawn(move || retain_stderr(stderr, &run_err))
            .map_err(ClaudeError::Io)?;
    }
    let supervised = Arc::clone(run);
    let bridge = Arc::clone(inner);
    thread::Builder::new()
        .name("skillstar-claude".into())
        .spawn(move || supervise(supervised, bridge))
        .map_err(ClaudeError::Io)?;
    Ok(())
}

fn supervise(run: Arc<RunInner>, bridge: Arc<Inner>) {
    loop {
        if run.finished.load(Ordering::Acquire) {
            return;
        }
        let generation = run.generation.load(Ordering::Acquire);
        let deadline = run.deadline();
        run.clock.wait_until(deadline, || {
            !run.finished.load(Ordering::Acquire)
                && run.generation.load(Ordering::Acquire) == generation
        });
        if run.finished.load(Ordering::Acquire) {
            return;
        }
        if run.generation.load(Ordering::Acquire) != generation {
            continue;
        }
        if run.clock.now_ms() >= run.deadline() {
            bridge.release(&run.token);
            return;
        }
    }
}

pub(super) fn child_env(account: &AccountSnapshot) -> Vec<(String, String)> {
    // The access token stays on the snapshot. It is not an environment value.
    let AccountSnapshot {
        access_token: _,
        account_id: _,
        api_key: _,
    } = account;
    let mut env = Vec::new();
    for (key, value) in std::env::vars() {
        if STRIPPED_ENV.contains(&key.as_str()) || ADDED_ENV.iter().any(|(name, _)| *name == key) {
            continue;
        }
        env.push((key, value));
    }
    for (key, value) in ADDED_ENV {
        env.push(((*key).to_string(), (*value).to_string()));
    }
    env
}

fn cli_args(model: &str, mcp_config: &str, effort: &str, web_search: bool) -> Vec<String> {
    let tools = if web_search { "WebSearch" } else { "" };
    let mut args = vec![
        "-p".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--include-partial-messages".to_string(),
        "--verbose".to_string(),
        "--model".to_string(),
        model.to_string(),
        "--tools".to_string(),
        tools.to_string(),
        "--strict-mcp-config".to_string(),
        "--mcp-config".to_string(),
        mcp_config.to_string(),
        "--setting-sources".to_string(),
        String::new(),
        "--dangerously-skip-permissions".to_string(),
        "--no-session-persistence".to_string(),
    ];
    if !effort.is_empty() {
        let effort = if effort == "xhigh" { "max" } else { effort };
        args.push("--effort".to_string());
        args.push(effort.to_string());
        args.push("--thinking-display".to_string());
        args.push("summarized".to_string());
    }
    args
}

fn mcp_config(executable: &Path, callback: &str, tools_path: &Path) -> String {
    json!({
        "mcpServers": {
            "skillstar": {
                "command": executable.to_string_lossy(),
                "args": ["claude-mcp-helper", callback, tools_path.to_string_lossy()],
            }
        }
    })
    .to_string()
}

fn callback_url(base: &str, token: &str) -> String {
    format!(
        "{}/_skillstar/claude-mcp/{token}",
        base.trim_end_matches('/')
    )
}

fn write_tools(path: &Path, tools: &[ClaudeTool]) -> Result<(), ClaudeError> {
    let listed: Vec<Value> = tools
        .iter()
        .map(|tool| {
            let mut value = json!({
                "name": tool.name,
                "inputSchema": tool.input_schema,
            });
            if !tool.description.is_empty() {
                value["description"] = Value::String(tool.description.clone());
            }
            value
        })
        .collect();
    let bytes = serde_json::to_vec(&listed)
        .map_err(|error| ClaudeError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))?;
    write_private(path, &bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), ClaudeError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(ClaudeError::Io)?;
        file.write_all(bytes).map_err(ClaudeError::Io)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes).map_err(ClaudeError::Io)
    }
}

/// `PATH`, then `~/.local/bin/claude`, then the absolute fallbacks, in order.
pub fn find_claude_binary(
    path: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
    fallbacks: &[PathBuf],
) -> Result<PathBuf, ClaudeError> {
    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            if let Some(candidate) = first_runnable(&claude_names(&dir)) {
                return Ok(candidate);
            }
        }
    }
    if let Some(home) = home {
        let bin = home.join(".local").join("bin");
        if let Some(candidate) = first_runnable(&claude_names(&bin)) {
            return Ok(candidate);
        }
    }
    for candidate in fallbacks {
        if is_runnable(candidate) {
            return Ok(candidate.clone());
        }
    }
    Err(ClaudeError::NotInstalled)
}

fn claude_names(dir: &Path) -> Vec<PathBuf> {
    // Windows installs the program as claude.exe. PATH lookup has to see that name.
    #[cfg(windows)]
    {
        vec![dir.join("claude"), dir.join("claude.exe")]
    }
    #[cfg(not(windows))]
    {
        vec![dir.join("claude")]
    }
}

fn first_runnable(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| is_runnable(candidate))
        .cloned()
}

pub(super) fn default_fallbacks() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/local/bin/claude"),
        PathBuf::from("/opt/homebrew/bin/claude"),
    ]
}

pub(super) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn is_runnable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn make_temp() -> Result<PathBuf, ClaudeError> {
    for _ in 0..8 {
        let dir = std::env::temp_dir().join(format!("{TEMP_PREFIX}{}", random_hex(24)));
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ClaudeError::Io(error)),
        }
    }
    Err(ClaudeError::Io(io::Error::other("temp dir")))
}

fn retain_stderr(mut reader: impl Read, run: &RunInner) {
    let mut tmp = [0u8; 8192];
    loop {
        match reader.read(&mut tmp) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut buf = run.stderr.lock().unwrap_or_else(|p| p.into_inner());
                if buf.len() < STDERR_CAP {
                    let room = STDERR_CAP - buf.len();
                    buf.extend_from_slice(&tmp[..n.min(room)]);
                }
            }
        }
    }
}

fn discard(mut reader: impl Read) {
    let mut tmp = [0u8; 8192];
    while reader.read(&mut tmp).unwrap_or(0) > 0 {}
}

pub(super) fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        let pid = child.id();
        if pid > 0 {
            kill(-(pid as i32), 9);
        }
    }
    let _ = child.kill();
}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn setpgid(pid: i32, pgid: i32) -> i32;
}

fn isolate_process_group(command: &mut Command) {
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            // SAFETY: setpgid is async-signal-safe, and the child has no other
            // threads yet. A new group lets abort kill grandchildren with the child.
            let _ = setpgid(0, 0);
            Ok(())
        });
    }
    #[cfg(not(unix))]
    {
        let _ = command;
    }
}

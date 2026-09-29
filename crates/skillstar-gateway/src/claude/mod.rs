//! Claude subscription process bridge.
//!
//! The gateway starts the local `claude` binary and parks its MCP tool calls.
//! An account access token is accepted on the launch value and then ignored:
//! it is not copied into the child environment and it is not sent to Anthropic.

mod callback;
mod clock;
mod helper;
mod process;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

pub use callback::{CallbackOutcome, begin_callback, callback_token};
pub use helper::run_mcp_helper;
pub use process::find_claude_binary;

use clock::SharedClock;

/// How long one turn may run before the process is killed.
pub const TURN_ABORT: Duration = Duration::from_secs(30 * 60);

/// How long a finished turn keeps its process for the next one.
pub const IDLE_LONGEST: Duration = Duration::from_secs(20 * 60);

/// How many idle processes are kept. The one waiting longest is dropped first.
pub const IDLE_MOST: usize = 6;

/// How long a turn that asked for a tool waits for the result.
pub const PARK_LONGEST: Duration = Duration::from_secs(5 * 60);

/// Stderr bytes kept from one process. The rest is read and discarded.
pub const STDERR_CAP: usize = 1 << 20;

/// Prefix of the per-run temp directory.
pub const TEMP_PREFIX: &str = "skillstar-claude-";

const STRIPPED_ENV: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SSE_PORT",
];

const ADDED_ENV: &[(&str, &str)] = &[
    ("ENABLE_CLAUDEAI_MCP_SERVERS", "0"),
    ("DISABLE_AUTO_COMPACT", "1"),
    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
];

/// Account material copied in by the app. The bridge does not read a store,
/// and none of these fields is copied into the child environment.
#[derive(Debug, Clone, Default)]
pub struct AccountSnapshot {
    pub access_token: Option<String>,
    /// Codex sends this as `chatgpt-account-id`. Other catalogs leave it empty.
    pub account_id: Option<String>,
    /// ZCode's own key. Other catalogs leave it empty.
    pub api_key: Option<String>,
}

/// One tool the helper will advertise to Claude Code.
#[derive(Debug, Clone)]
pub struct ClaudeTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Result the gateway sends back through a parked MCP call.
#[derive(Clone, Debug, Serialize)]
pub struct ToolResult {
    pub content: Value,
    #[serde(skip_serializing_if = "is_false")]
    pub is_error: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl ToolResult {
    pub(crate) fn json_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_else(|_| b"{}".to_vec())
    }
}

/// Why a Claude process did not start.
#[derive(Debug)]
pub enum ClaudeError {
    NotInstalled,
    Io(io::Error),
}

impl std::fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("未找到 claude"),
            Self::Io(error) => write!(f, "claude 进程桥失败: {error}"),
        }
    }
}

impl std::error::Error for ClaudeError {}

/// What one subscription turn asks the local binary to do.
pub struct ClaudeLaunch {
    pub model: String,
    pub effort: String,
    pub web_search: bool,
    pub tools: Vec<ClaudeTool>,
    pub account: AccountSnapshot,
    pub executable: PathBuf,
    pub callback_base: String,
}

impl ClaudeLaunch {
    pub fn new(
        model: impl Into<String>,
        executable: impl Into<PathBuf>,
        callback_base: impl Into<String>,
    ) -> Self {
        Self {
            model: model.into(),
            effort: String::new(),
            web_search: false,
            tools: Vec::new(),
            account: AccountSnapshot::default(),
            executable: executable.into(),
            callback_base: callback_base.into(),
        }
    }
}

/// Live bridge. Idle runs and their deadlines live here.
pub struct ClaudeBridge {
    inner: Arc<Inner>,
}

struct Inner {
    clock: Arc<SharedClock>,
    runs: Mutex<RunTable>,
}

struct RunTable {
    by_token: HashMap<String, Arc<RunInner>>,
    idle: Vec<IdleSlot>,
}

struct IdleSlot {
    token: String,
    at: u128,
}

struct RunInner {
    token: String,
    temp: PathBuf,
    pid: u32,
    clock: Arc<SharedClock>,
    life: Mutex<Life>,
    stderr: Mutex<Vec<u8>>,
    pending: Mutex<HashMap<String, mpsc::SyncSender<ToolResult>>>,
    deadline_ms: Mutex<u128>,
    generation: AtomicU64,
    finished: AtomicBool,
}

struct Life {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    claimed: bool,
}

/// Handle for one running `claude`. Dropping it kills the process.
pub struct ClaudeRun {
    bridge: Arc<Inner>,
    run: Arc<RunInner>,
}

impl ClaudeBridge {
    pub fn system() -> Self {
        Self::new(SharedClock::system())
    }

    /// Clock that moves only when [`Self::advance`] is called.
    pub fn manual() -> Self {
        Self::new(SharedClock::manual())
    }

    fn new(clock: SharedClock) -> Self {
        Self {
            inner: Arc::new(Inner {
                clock: Arc::new(clock),
                runs: Mutex::new(RunTable {
                    by_token: HashMap::new(),
                    idle: Vec::new(),
                }),
            }),
        }
    }

    pub fn launch(&self, spec: ClaudeLaunch) -> Result<ClaudeRun, ClaudeError> {
        let binary = process::find_claude_binary(
            std::env::var_os("PATH").as_deref(),
            process::home_dir().as_deref(),
            &process::default_fallbacks(),
        )?;
        process::launch_process(
            &self.inner,
            &spec,
            &binary,
            process::child_env(&spec.account),
        )
    }

    /// Expire due runs after moving a manual clock. Returns false on a system clock.
    pub fn advance(&self, by: Duration) -> bool {
        if !self.inner.clock.advance(by) {
            return false;
        }
        self.inner.sweep();
        true
    }

    pub fn idle_count(&self) -> usize {
        table(&self.inner).idle.len()
    }

    fn lookup(&self, token: &str) -> Option<Arc<RunInner>> {
        table(&self.inner).by_token.get(token).cloned()
    }
}

impl ClaudeRun {
    pub fn token(&self) -> &str {
        &self.run.token
    }

    pub fn pid(&self) -> u32 {
        self.run.pid
    }

    pub fn stderr(&self) -> Vec<u8> {
        self.run
            .stderr
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn is_running(&self) -> bool {
        self.run.is_running()
    }

    /// The reply finished. The process stays up, subject to the idle cap.
    pub fn keep_idle(&self) {
        self.bridge.park(&self.run.token, IDLE_LONGEST);
    }

    /// The reply asked for a tool. The process waits [`PARK_LONGEST`].
    pub fn wait_for_tool(&self) {
        self.bridge.arm_only(&self.run.token, PARK_LONGEST);
    }

    /// Unblock the helper that posted this tool call.
    pub fn resolve_tool(&self, id: &str, result: ToolResult) -> bool {
        self.run.resolve_tool(id, result)
    }
}

impl Drop for ClaudeRun {
    fn drop(&mut self) {
        self.bridge.release(&self.run.token);
    }
}

impl Inner {
    fn park(&self, token: &str, keep: Duration) {
        let doomed = {
            let mut runs = table(self);
            let Some(run) = runs.by_token.get(token).cloned() else {
                return;
            };
            if run.finished.load(Ordering::Acquire) {
                return;
            }
            let now = self.clock.now_ms();
            run.arm(now.saturating_add(keep.as_millis()));
            runs.idle.retain(|slot| slot.token != token);
            runs.idle.push(IdleSlot {
                token: token.to_string(),
                at: now,
            });
            let mut doomed = Vec::new();
            while runs.idle.len() > IDLE_MOST {
                let oldest = runs
                    .idle
                    .iter()
                    .enumerate()
                    .min_by_key(|(index, slot)| (slot.at, *index))
                    .map(|(index, _)| index)
                    .expect("idle is non-empty");
                doomed.push(runs.idle.remove(oldest).token);
            }
            doomed
        };
        for token in doomed {
            self.release(&token);
        }
    }

    fn arm_only(&self, token: &str, wait: Duration) {
        let mut runs = table(self);
        runs.idle.retain(|slot| slot.token != token);
        let Some(run) = runs.by_token.get(token).cloned() else {
            return;
        };
        if run.finished.load(Ordering::Acquire) {
            return;
        }
        let now = self.clock.now_ms();
        run.arm(now.saturating_add(wait.as_millis()));
    }

    fn sweep(&self) {
        let now = self.clock.now_ms();
        let due: Vec<String> = {
            let runs = table(self);
            runs.by_token
                .iter()
                .filter(|(_, run)| run.deadline() <= now && !run.finished.load(Ordering::Acquire))
                .map(|(token, _)| token.clone())
                .collect()
        };
        for token in due {
            self.release(&token);
        }
    }

    fn release(&self, token: &str) {
        let run = {
            let mut runs = table(self);
            runs.idle.retain(|slot| slot.token != token);
            runs.by_token.remove(token)
        };
        if let Some(run) = run {
            run.shutdown();
        }
    }
}

impl RunInner {
    fn arm(&self, deadline_ms: u128) {
        *self.deadline_ms.lock().unwrap_or_else(|p| p.into_inner()) = deadline_ms;
        self.generation.fetch_add(1, Ordering::AcqRel);
        self.clock.poke();
    }

    fn deadline(&self) -> u128 {
        *self.deadline_ms.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn is_running(&self) -> bool {
        let mut life = self.life.lock().unwrap_or_else(|p| p.into_inner());
        match life.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    fn park_call(&self, id: String) -> mpsc::Receiver<ToolResult> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);
        rx
    }

    fn resolve_tool(&self, id: &str, result: ToolResult) -> bool {
        match self
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id)
        {
            Some(tx) => tx.send(result).is_ok(),
            None => false,
        }
    }

    fn shutdown(&self) {
        {
            let mut life = self.life.lock().unwrap_or_else(|p| p.into_inner());
            if life.claimed {
                return;
            }
            life.claimed = true;
            drop(life.stdin.take());
            if let Some(child) = life.child.as_mut() {
                process::kill_tree(child);
                let _ = child.wait();
            }
        }
        self.finished.store(true, Ordering::Release);
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        let _ = fs::remove_dir_all(&self.temp);
        self.clock.poke();
    }
}

pub(super) fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    if File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut buf))
        .is_err()
    {
        fill_fallback(&mut buf);
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(buf.len() * 2);
    for byte in buf {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

fn fill_fallback(buf: &mut [u8]) {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let tick = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for (index, byte) in buf.iter_mut().enumerate() {
        let mixed = tick
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(nanos as u64)
            .wrapping_add(index as u64)
            .wrapping_add(std::process::id() as u64);
        *byte = mixed as u8;
    }
}

fn table(inner: &Inner) -> std::sync::MutexGuard<'_, RunTable> {
    inner
        .runs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Process-wide bridge the listener parks callbacks on.
pub fn listener_bridge() -> &'static ClaudeBridge {
    shared()
}

pub(crate) fn shared() -> &'static ClaudeBridge {
    static BRIDGE: std::sync::OnceLock<ClaudeBridge> = std::sync::OnceLock::new();
    BRIDGE.get_or_init(ClaudeBridge::system)
}

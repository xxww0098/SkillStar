//! Claude Desktop's third-party gateway profile.
//!
//! [`desktop_accepts`] is the id check Desktop applies to `/v1/models`.
//! The live model list is still empty, so this module does not change it.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

use super::body::file;
use super::{Written, sandboxed};

/// Profile id in Claude-3p's config library. The last group is `skills` in hex.
pub const DESKTOP_PROFILE_ID: &str = "00000000-0000-4000-8000-736b696c6c73";

const ALIAS_PREFIX: &str = "anthropic/skillstar-";
const EFFORT_PREFIX: &str = "mythos-skillstar-";
const ALIAS_MOD: u64 = 10_000_000_000;

const DENIED: &str = r"ark-code|astron|command-r|deepseek|doubao|gemini|gemma|glm|gpt|grok|hermes|hy3|kimi|lfm|\bling\b|llama|longcat|mimo|minimax|mistral|mixtral|moonshot|nemotron|openai|phi-|qianfan|qwen|tc-code|\bunic\b|yi-|stepfun|step-3|seed-|bytedance|hunyuan|granite|amazon\.nova|nova-|devstral|ministral|ernie|codex|arcee|trinity|abab|phi\d|\bk2\.|\bm2\.|jamba|arctic|solar|mercury|zamba|kat-coder|\bds-|dpsk";
const TIER: &str = r"^(sonnet|opus|haiku|fable|mythos)(-[\d.]+)?$";
const WORDS: &[&str] = &[
    "claude", "sonnet", "opus", "haiku", "fable", "mythos", "anthropic",
];

/// Claude and Claude-3p folders for `goos`.
///
/// `darwin` uses Application Support. `windows` uses `LOCALAPPDATA` when the
/// caller supplies it, else `<home>/AppData/Local`, and prefers an existing
/// `Claude*` folder. Anything else uses an absolute `XDG_CONFIG_HOME`, else
/// `<home>/.config`.
pub fn desktop_dirs(
    goos: &str,
    home: &Path,
    getenv: impl Fn(&str) -> Option<String>,
) -> (PathBuf, PathBuf) {
    match goos {
        "darwin" => {
            let root = home.join("Library").join("Application Support");
            (root.join("Claude"), root.join("Claude-3p"))
        }
        "windows" => {
            let local = getenv("LOCALAPPDATA")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Local"));
            (
                windows_desktop_dir(&local, false),
                windows_desktop_dir(&local, true),
            )
        }
        _ => {
            let root = match getenv("XDG_CONFIG_HOME") {
                Some(value) if !value.is_empty() && Path::new(&value).is_absolute() => {
                    PathBuf::from(value)
                }
                _ => home.join(".config"),
            };
            (root.join("Claude"), root.join("Claude-3p"))
        }
    }
}

/// `anthropic/skillstar-` plus the 10-digit FNV-1a 64 of `id`.
pub fn desktop_alias(id: &str) -> String {
    format!("{ALIAS_PREFIX}{}", alias_number(id))
}

/// `mythos-skillstar-` plus the same 10-digit number.
pub fn desktop_effort_alias(id: &str) -> String {
    format!("{EFFORT_PREFIX}{}", alias_number(id))
}

/// Desktop keeps an id when it reads as an Anthropic model.
pub fn desktop_accepts(id: &str) -> bool {
    let lower = id.to_lowercase();
    if tier().is_match(&lower) {
        return true;
    }
    if denied().is_match(&lower) {
        return false;
    }
    WORDS.iter().any(|word| lower.contains(word))
}

pub(super) fn files(home: &Path, origin: &str) -> Vec<Written> {
    let (dir, dir3p) = dirs_for_home(home);
    let library = dir3p.join("configLibrary");
    let config = config_body();
    vec![
        file(
            home,
            dir.join("claude_desktop_config.json"),
            config.clone(),
        ),
        file(home, dir3p.join("claude_desktop_config.json"), config),
        file(
            home,
            library.join(format!("{DESKTOP_PROFILE_ID}.json")),
            profile_body(origin),
        ),
        file(home, library.join("_meta.json"), meta_body()),
    ]
}

fn dirs_for_home(home: &Path) -> (PathBuf, PathBuf) {
    let goos = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    if sandboxed() {
        desktop_dirs(goos, home, |_| None)
    } else {
        desktop_dirs(goos, home, env_get)
    }
}

fn env_get(key: &str) -> Option<String> {
    std::env::var_os(key).and_then(|value| {
        let text = value.to_str()?;
        (!text.is_empty()).then(|| text.to_string())
    })
}

fn windows_desktop_dir(local: &Path, threep: bool) -> PathBuf {
    let name = if threep { "Claude-3p" } else { "Claude" };
    let exact = local.join(name);
    if exact.exists() {
        return exact;
    }
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(local) {
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name.starts_with("Claude") && name.contains("-3p") == threep {
                found.push(name);
            }
        }
    }
    if found.is_empty() {
        return exact;
    }
    found.sort();
    local.join(&found[0])
}

fn alias_number(id: &str) -> String {
    format!("{:010}", fnv1a64(id.as_bytes()) % ALIAS_MOD)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn config_body() -> String {
    "{\n  \"deploymentMode\": \"3p\"\n}\n".to_string()
}

fn profile_body(origin: &str) -> String {
    let key = super::token_for("claude-desktop");
    format!(
        "{{\n  \"inferenceProvider\": \"gateway\",\n  \"inferenceGatewayBaseUrl\": {origin},\n  \"inferenceGatewayApiKey\": {key},\n  \"inferenceGatewayAuthScheme\": \"bearer\",\n  \"disableDeploymentModeChooser\": true,\n  \"coworkEgressAllowedHosts\": [\"*\"]\n}}\n",
        origin = json_string(origin),
        key = json_string(&key),
    )
}

fn meta_body() -> String {
    format!(
        "{{\n  \"appliedId\": {id},\n  \"entries\": [\n    {{\n      \"id\": {id},\n      \"name\": \"skillstar\"\n    }}\n  ]\n}}\n",
        id = json_string(DESKTOP_PROFILE_ID),
    )
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn ascii_regex(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .unicode(false)
        .build()
        .unwrap_or_else(|error| panic!("desktop filter: {error}"))
}

fn denied() -> &'static Regex {
    static DENIED_RE: LazyLock<Regex> = LazyLock::new(|| ascii_regex(DENIED));
    &DENIED_RE
}

fn tier() -> &'static Regex {
    static TIER_RE: LazyLock<Regex> = LazyLock::new(|| ascii_regex(TIER));
    &TIER_RE
}

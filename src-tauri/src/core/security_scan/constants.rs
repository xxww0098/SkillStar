//! Compile-time prompts and scan limits shared across security scan modules.

#[allow(dead_code)]
pub(crate) const SKILL_AGENT_PROMPT: &str = include_str!("../../../prompts/security/skill_agent.md");
#[allow(dead_code)]
pub(crate) const SCRIPT_AGENT_PROMPT: &str = include_str!("../../../prompts/security/script_agent.md");
#[allow(dead_code)]
pub(crate) const RESOURCE_AGENT_PROMPT: &str = include_str!("../../../prompts/security/resource_agent.md");
#[allow(dead_code)]
pub(crate) const GENERAL_AGENT_PROMPT: &str = include_str!("../../../prompts/security/general_agent.md");
pub(crate) const AGGREGATOR_PROMPT: &str = include_str!("../../../prompts/security/aggregator.md");
pub(crate) const CHUNK_BATCH_PROMPT: &str = include_str!("../../../prompts/security/chunk_batch.md");
pub(crate) const DEFAULT_SECURITY_SCAN_POLICY_YAML: &str = include_str!("security_policy_default.yaml");

pub(crate) const MAX_FILE_CHARS: usize = 8_000;
pub(crate) const CACHE_MAX_ENTRIES: usize = 200;
pub(crate) const MAX_RECURSION_DEPTH: usize = 10;
pub(crate) const SNIPPET_MAX_CHARS: usize = 200;
pub(crate) const CACHE_SCHEMA_VERSION: &str = "security-scan-v4";
pub(crate) const SCAN_LOG_ARCHIVE_MAX_ENTRIES: usize = 500;
pub(crate) const SCAN_TELEMETRY_MAX_ENTRIES: usize = 2_000;
pub(crate) const CHUNK_MAX_RETRIES: usize = 2;
pub(crate) const CHUNK_RETRY_DELAY_MS: u64 = 1500;

pub(crate) const FILE_CACHE_MAX_ENTRIES: usize = 5_000;

pub(crate) const SCANNABLE_EXTENSIONS: &[&str] = &[
    "md", "sh", "py", "js", "ts", "yaml", "yml", "json", "toml", "txt", "cfg", "ini", "bat", "ps1",
    "rb", "lua", "bash", "zsh", "fish", "pl", "r",
];

pub(crate) const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "__pycache__",
    ".next",
    "build",
    ".venv",
    "venv",
];

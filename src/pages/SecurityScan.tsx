import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import {
  AlertTriangle,
  Brain,
  ChevronDown,
  ChevronRight,
  Clock,
  Download,
  FileText,
  FolderOpen,
  Radar,
  RotateCcw,
  ScanLine,
  ShieldAlert,
  ShieldCheck,
  ShieldX,
  Trash2,
  Zap,
} from "lucide-react";
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { MOTION_TRANSITION, motionDelay } from "../comm/motion";
import { CardTemplate } from "../components/ui/card-template";
import { RadarSweep } from "../features/security/components/RadarSweep";
import { ScanFilePanel } from "../features/security/components/ScanFilePanel";
import { type ScanMode, useSecurityScan } from "../features/security/hooks/useSecurityScan";
import type {
  AiFinding,
  RiskLevel,
  SecurityScanEstimate,
  SecurityScanLogEntry,
  SecurityScanPolicy,
  SecurityScanResult,
  StaticFinding,
} from "../types";

// ── Risk Styling ──────────────────────────────────────────────────

const riskColor: Record<RiskLevel, string> = {
  Safe: "text-success",
  Low: "text-amber-400",
  Medium: "text-orange-400",
  High: "text-destructive",
  Critical: "text-destructive",
};

const riskDot: Record<RiskLevel, string> = {
  Safe: "bg-success",
  Low: "bg-amber-400",
  Medium: "bg-orange-400",
  High: "bg-destructive",
  Critical: "bg-destructive",
};

const riskBgSubtle: Record<RiskLevel, string> = {
  Safe: "",
  Low: "bg-amber-500/5",
  Medium: "bg-orange-500/5",
  High: "bg-destructive/5",
  Critical: "bg-destructive/8",
};

const RiskIcon = ({ level, size = 14 }: { level: RiskLevel; size?: number }) => {
  const cn = riskColor[level];
  switch (level) {
    case "Safe":
      return <ShieldCheck size={size} className={cn} />;
    case "Low":
    case "Medium":
      return <ShieldAlert size={size} className={cn} />;
    case "High":
    case "Critical":
      return <ShieldX size={size} className={cn} />;
  }
};

// ── Helpers ───────────────────────────────────────────────────────

const fallbackRiskScoreByLevel: Record<RiskLevel, number> = {
  Safe: 0,
  Low: 2.5,
  Medium: 5,
  High: 7.5,
  Critical: 9.5,
};

function resolveSkillRiskScore(result: SecurityScanResult): number {
  const riskScore = result.risk_score;
  if (typeof riskScore === "number" && Number.isFinite(riskScore)) {
    return Math.max(0, Math.min(10, riskScore));
  }
  return fallbackRiskScoreByLevel[result.risk_level] ?? 0;
}

function calculateOverallRisk(results: SecurityScanResult[]): {
  score: number;
  safetyScore: number;
  level: RiskLevel;
  riskScore10: number;
} {
  if (results.length === 0) return { score: 0, safetyScore: 100, level: "Safe", riskScore10: 0 };

  const avgRiskScore10 = results.reduce((sum, result) => sum + resolveSkillRiskScore(result), 0) / results.length;
  const riskScore10 = Math.round(avgRiskScore10 * 10) / 10;
  const score = Math.round(riskScore10 * 10);
  const safetyScore = 100 - score;

  if (riskScore10 >= 8.0) return { score, safetyScore, level: "Critical", riskScore10 };
  if (riskScore10 >= 6.0) return { score, safetyScore, level: "High", riskScore10 };
  if (riskScore10 >= 3.5) return { score, safetyScore, level: "Medium", riskScore10 };
  if (riskScore10 >= 1.0) return { score, safetyScore, level: "Low", riskScore10 };
  return { score, safetyScore, level: "Safe", riskScore10 };
}

function formatTimestamp(iso: string): string {
  try {
    const timestamp = new Date(iso);
    return timestamp.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  } catch {
    return iso;
  }
}

function getRiskLabel(t: any, level: RiskLevel): string {
  switch (level) {
    case "Safe":
      return t("securityScan.risk.safe", "Safe");
    case "Low":
      return t("securityScan.risk.low", "Low Risk");
    case "Medium":
      return t("securityScan.risk.medium", "Medium Risk");
    case "High":
      return t("securityScan.risk.high", "High Risk");
    case "Critical":
      return t("securityScan.risk.critical", "Critical");
  }
}

function getRiskShortLabel(t: any, level: RiskLevel): string {
  switch (level) {
    case "Safe":
      return t("securityScan.riskShort.safe", "Safe");
    case "Low":
      return t("securityScan.riskShort.low", "Low");
    case "Medium":
      return t("securityScan.riskShort.medium", "Medium");
    case "High":
      return t("securityScan.riskShort.high", "High");
    case "Critical":
      return t("securityScan.riskShort.critical", "Critical");
  }
}

function defaultScanPolicy(): SecurityScanPolicy {
  return {
    preset: "balanced",
    severity_threshold: "low",
    enabled_analyzers: [],
    ignore_rules: [],
    rule_overrides: {},
  };
}

function getStaticDescription(t: any, patternId: string, fallback: string): string {
  const keyByPattern: Record<string, string> = {
    curl_pipe_sh: "securityScan.staticDescription.curl_pipe_sh",
    wget_pipe_sh: "securityScan.staticDescription.wget_pipe_sh",
    base64_decode_exec: "securityScan.staticDescription.base64_decode_exec",
    eval_fetch: "securityScan.staticDescription.eval_fetch",
    exec_requests: "securityScan.staticDescription.exec_requests",
    sensitive_ssh: "securityScan.staticDescription.sensitive_ssh",
    sensitive_aws: "securityScan.staticDescription.sensitive_aws",
    sensitive_env: "securityScan.staticDescription.sensitive_env",
    sensitive_etc_passwd: "securityScan.staticDescription.sensitive_etc_passwd",
    sensitive_gnupg: "securityScan.staticDescription.sensitive_gnupg",
    npm_global_install: "securityScan.staticDescription.npm_global_install",
    pip_install: "securityScan.staticDescription.pip_install",
    unicode_bidi: "securityScan.staticDescription.unicode_bidi",
    reverse_shell: "securityScan.staticDescription.reverse_shell",
    bash_reverse: "securityScan.staticDescription.bash_reverse",
    modify_shell_rc: "securityScan.staticDescription.modify_shell_rc",
    cron_persistence: "securityScan.staticDescription.cron_persistence",
    long_base64: "securityScan.staticDescription.long_base64",
    powershell_encoded: "securityScan.staticDescription.powershell_encoded",
    schtasks_persistence: "securityScan.staticDescription.schtasks_persistence",
    registry_persistence: "securityScan.staticDescription.registry_persistence",
  };

  const key = keyByPattern[patternId];
  if (key) {
    return t(key, fallback);
  }
  return fallback;
}

/** Map common English AI category names to i18n keys.
 *  Handles the case where local models ignore the target-language instruction
 *  and still return English category names. */
function localizeAiCategory(t: any, category: string): string {
  const normalized = category
    .trim()
    .toLowerCase()
    .replace(/[\s_-]+/g, "_");
  const key = `securityScan.aiCategory.${normalized}`;
  const localized = t(key, { defaultValue: "" });
  return localized || category;
}

function localizeScanSummary(t: any, summary: string): string {
  if (!summary) return "";

  if (summary === "No issues found.") {
    return t("securityScan.summary.noIssues", { defaultValue: "No issues found." });
  }

  if (summary === "No scannable files found.") {
    return t("securityScan.summary.noScannableFiles", { defaultValue: "No scannable files found." });
  }

  if (summary === "Scan complete.") {
    return t("securityScan.summary.scanComplete", { defaultValue: "Scan complete." });
  }

  if (summary === "Scan was cancelled before all AI chunks completed. Results are partial and may miss issues.") {
    return t("securityScan.summary.cancelled", {
      defaultValue: "Scan was cancelled before all AI chunks completed. Results are partial and may miss issues.",
    });
  }

  if (
    summary ===
    "AI summary generation failed after file analysis. Results may be incomplete; manual review is recommended."
  ) {
    return t("securityScan.summary.summaryGenerationFailed", {
      defaultValue:
        "AI summary generation failed after file analysis. Results may be incomplete; manual review is recommended.",
    });
  }

  const staticNotConfigured = summary.match(
    /^Static scan found (\d+) pattern match\(es\)\. AI analysis not configured\.$/,
  );
  if (staticNotConfigured) {
    const count = Number(staticNotConfigured[1] || 0);
    return t("securityScan.summary.staticMatchesAiNotConfigured", {
      count,
      defaultValue: "Static scan found {{count}} pattern match(es). AI analysis not configured.",
    });
  }

  const staticNoAdditional = summary.match(
    /^Static scan found (\d+) pattern match\(es\)\. AI analysis found no additional issues\.$/,
  );
  if (staticNoAdditional) {
    const count = Number(staticNoAdditional[1] || 0);
    return t("securityScan.summary.staticMatchesNoAdditionalIssues", {
      count,
      defaultValue: "Static scan found {{count}} pattern match(es). AI analysis found no additional issues.",
    });
  }

  const aiIncomplete = summary.match(
    /^AI analysis was incomplete for (\d+) file\(s\)\. Results may miss issues; manual review is recommended\.$/,
  );
  if (aiIncomplete) {
    const count = Number(aiIncomplete[1] || 0);
    return t("securityScan.summary.analysisIncomplete", {
      count,
      defaultValue:
        "AI analysis was incomplete for {{count}} file(s). Results may miss issues; manual review is recommended.",
    });
  }

  return summary;
}

const OWASP_AGENTIC_RULES: Array<{ tag: string; keywords: string[] }> = [
  {
    tag: "AS-01 Prompt Injection",
    keywords: ["prompt injection", "jailbreak", "system prompt", "ignore previous instructions"],
  },
  {
    tag: "AS-02 Insecure Tool Execution",
    keywords: ["exec(", "spawn(", "eval(", "shell", "command", "subprocess"],
  },
  {
    tag: "AS-03 Data Exfiltration",
    keywords: ["exfil", "upload", "outbound", "data leak", "webhook"],
  },
  {
    tag: "AS-04 Secrets Exposure",
    keywords: ["secret", "token", "password", "api key", "api_key", "private key"],
  },
  {
    tag: "AS-05 Supply Chain & Dependency Risk",
    keywords: ["dependency", "supply chain", "cve", "trivy", "grype", "osv", "pip install", "npm install"],
  },
  {
    tag: "AS-06 Privilege Escalation & Persistence",
    keywords: [
      "sudo",
      "setuid",
      "chmod",
      "chown",
      "authorized_keys",
      "persistence",
      ".bashrc",
      ".zshrc",
      "reg add",
      "schtasks",
      "net user",
      "runas",
      "Set-ExecutionPolicy",
      "powershell -enc",
      "HKLM\\\\",
      "HKCU\\\\",
    ],
  },
  {
    tag: "AS-07 Sandbox Escape / Isolation Failure",
    keywords: ["sandbox", "seccomp", "escape", "bwrap", "unshare"],
  },
  {
    tag: "AS-08 Insecure Network Interaction",
    keywords: ["http://", "https://", "socket", "dns", "curl ", "wget ", "network"],
  },
  {
    tag: "AS-09 Obfuscation & Integrity Evasion",
    keywords: ["base64", "unicode bidi", "obfuscation", "encodedcommand", "\\x"],
  },
  {
    tag: "AS-10 Insufficient Validation & Guardrails",
    keywords: ["validation", "unsafe", "bypass", "policy override", "guardrail"],
  },
];

function inferOwaspAgenticTags(text: string): string[] {
  const lowered = text.toLowerCase();
  const tags = OWASP_AGENTIC_RULES.filter((rule) => rule.keywords.some((keyword) => lowered.includes(keyword))).map(
    (rule) => rule.tag,
  );
  return tags.length > 0 ? tags : ["AS-10 Insufficient Validation & Guardrails"];
}

function staticFindingOwaspTags(finding: StaticFinding): string[] {
  if (finding.owasp_agentic_tags && finding.owasp_agentic_tags.length > 0) {
    return finding.owasp_agentic_tags;
  }
  return inferOwaspAgenticTags(`${finding.pattern_id} ${finding.description} ${finding.snippet || ""}`);
}

function aiFindingOwaspTags(finding: AiFinding): string[] {
  if (finding.owasp_agentic_tags && finding.owasp_agentic_tags.length > 0) {
    return finding.owasp_agentic_tags;
  }
  return inferOwaspAgenticTags(
    `${finding.category} ${finding.description} ${finding.evidence || ""} ${finding.recommendation || ""}`,
  );
}

function OwaspTagList({ tags }: { tags?: string[] }) {
  if (!tags || tags.length === 0) return null;
  return (
    <div className="mt-1 flex flex-wrap gap-1">
      {tags.map((tag) => (
        <span
          key={tag}
          className="text-[9px] font-semibold tracking-wide uppercase text-muted-foreground/80 bg-muted/40 border border-muted/60 rounded-full px-2 py-0.5"
        >
          {tag}
        </span>
      ))}
    </div>
  );
}

// ── Severity Stat Pill ────────────────────────────────────────────

function StatPill({ level, count }: { level: RiskLevel; count: number }) {
  const { t } = useTranslation();
  if (count === 0) return null;
  return (
    <span
      className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md text-[11px] font-medium tabular-nums ${riskColor[level]} ${riskBgSubtle[level]} border border-current/8`}
    >
      <span className={`w-1.5 h-1.5 rounded-full ${riskDot[level]}`} />
      {count} {getRiskShortLabel(t, level)}
    </span>
  );
}

// ── Scan Timer ────────────────────────────────────────────────────

function ScanTimer({ startedAt }: { startedAt: number | null }) {
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    if (!startedAt) return;
    // update immediately to stay in sync
    setNow(Date.now());
    const interval = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(interval);
  }, [startedAt]);

  if (!startedAt) return null;

  const elapsed = Math.max(0, Math.floor((now - startedAt) / 1000));
  const m = Math.floor(elapsed / 60)
    .toString()
    .padStart(2, "0");
  const s = (elapsed % 60).toString().padStart(2, "0");

  return (
    <span className="font-mono tabular-nums ml-1 px-1.5 py-0.5 bg-success/10 rounded">
      {m}:{s}
    </span>
  );
}

// ── Main Page ─────────────────────────────────────────────────────

export function SecurityScan() {
  const { t } = useTranslation();
  const {
    phase,
    results,
    activeSkills,
    currentSkill,
    currentStage,
    currentFile,
    syncPulseKey,
    recentFiles,
    scanned,
    total,
    activeChunkFiles,
    activeChunkWorkers,
    maxChunkWorkers,
    progressPercent,
    errors,
    startScan,
    clearCache,
    cancelScan,
    scanStartedAt,
  } = useSecurityScan();

  const [expandedSkill, setExpandedSkill] = useState<string | null>(null);
  const [scanLogs, setScanLogs] = useState<SecurityScanLogEntry[]>([]);
  const [scanLogDir, setScanLogDir] = useState<string>("");
  const [openingLogFolder, setOpeningLogFolder] = useState(false);
  const [selectedMode, setSelectedModeState] = useState<ScanMode>(() => {
    try {
      const stored = localStorage.getItem("skillstar:scan-mode");
      if (stored === "static" || stored === "smart" || stored === "deep") {
        return stored as ScanMode;
      }
    } catch {
      // ignore
    }
    return "static";
  });

  const setSelectedMode = useCallback((mode: ScanMode) => {
    try {
      localStorage.setItem("skillstar:scan-mode", mode);
    } catch {
      // ignore
    }
    setSelectedModeState(mode);
  }, []);
  const [incrementalScan, setIncrementalScanState] = useState<boolean>(() => {
    try {
      return localStorage.getItem("skillstar:scan-incremental") !== "0";
    } catch {
      return true;
    }
  });
  const setIncrementalScan = useCallback((enabled: boolean) => {
    try {
      localStorage.setItem("skillstar:scan-incremental", enabled ? "1" : "0");
    } catch {
      // ignore
    }
    setIncrementalScanState(enabled);
  }, []);
  const [scanEstimate, setScanEstimate] = useState<SecurityScanEstimate | null>(null);
  const [estimating, setEstimating] = useState(false);
  const [scanPolicy, setScanPolicy] = useState<SecurityScanPolicy>(() => defaultScanPolicy());
  const [policySaving, setPolicySaving] = useState(false);
  const [ignoreRulesDraft, setIgnoreRulesDraft] = useState("");
  const [analyzersDraft, setAnalyzersDraft] = useState("");
  const [exportingFormat, setExportingFormat] = useState<"sarif" | "json" | "markdown" | "html" | null>(null);
  const [latestReportPath, setLatestReportPath] = useState<string>("");
  const [ignoringRuleIds, setIgnoringRuleIds] = useState<Set<string>>(new Set());

  const parseIgnoreRulesInput = useCallback((raw: string): string[] => {
    return raw
      .split(",")
      .map((item) => item.trim().toLowerCase())
      .filter(Boolean);
  }, []);

  const parseAnalyzerInput = useCallback((raw: string): string[] => {
    return raw
      .split(",")
      .map((item) => item.trim().toLowerCase())
      .filter(Boolean);
  }, []);

  const ignoredRuleSet = useMemo(
    () => new Set((scanPolicy.ignore_rules ?? []).map((rule) => rule.trim().toLowerCase())),
    [scanPolicy.ignore_rules],
  );

  const loadScanLogs = useCallback(async () => {
    try {
      const [logs, dir] = await Promise.all([
        invoke<SecurityScanLogEntry[]>("list_security_scan_logs", { limit: 30 }),
        invoke<string>("get_security_scan_log_dir"),
      ]);
      setScanLogs(logs);
      setScanLogDir(dir);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    void loadScanLogs();
  }, [loadScanLogs, phase]);

  const loadScanPolicy = useCallback(async () => {
    try {
      const policy = await invoke<SecurityScanPolicy>("get_security_scan_policy");
      setScanPolicy(policy);
      setIgnoreRulesDraft(policy.ignore_rules.join(", "));
      setAnalyzersDraft((policy.enabled_analyzers ?? []).join(", "));
    } catch {
      setScanPolicy(defaultScanPolicy());
      setIgnoreRulesDraft("");
      setAnalyzersDraft("");
    }
  }, []);

  useEffect(() => {
    void loadScanPolicy();
  }, [loadScanPolicy]);

  const loadScanEstimate = useCallback(async () => {
    if (phase === "scanning") return;
    setEstimating(true);
    try {
      const estimate = await invoke<SecurityScanEstimate>("estimate_security_scan", {
        skillNames: [],
        mode: selectedMode,
      });
      setScanEstimate(estimate);
    } catch {
      setScanEstimate(null);
    } finally {
      setEstimating(false);
    }
  }, [phase, selectedMode]);

  useEffect(() => {
    void loadScanEstimate();
  }, [loadScanEstimate]);

  const openScanLogFolder = useCallback(async () => {
    if (!scanLogDir) return;
    setOpeningLogFolder(true);
    try {
      await invoke("open_folder", { path: scanLogDir });
    } catch {
      // ignore
    } finally {
      setOpeningLogFolder(false);
    }
  }, [scanLogDir]);

  const persistScanPolicy = useCallback(
    async (next: SecurityScanPolicy) => {
      setScanPolicy(next);
      setIgnoreRulesDraft(next.ignore_rules.join(", "));
      setAnalyzersDraft((next.enabled_analyzers ?? []).join(", "));
      setPolicySaving(true);
      try {
        await invoke("save_security_scan_policy", { policy: next });
      } catch {
        await loadScanPolicy();
      } finally {
        setPolicySaving(false);
      }
    },
    [loadScanPolicy],
  );

  const exportScanReport = useCallback(
    async (format: "sarif" | "json" | "markdown" | "html") => {
      setExportingFormat(format);
      try {
        const names = results.map((item) => item.skill_name);
        const path = await invoke<string>("export_security_scan_report", {
          format,
          skillNames: names.length > 0 ? names : undefined,
          requestLabel: `ui-${selectedMode}`,
        });
        setLatestReportPath(path);
        await loadScanLogs();
      } catch {
        // ignore
      } finally {
        setExportingFormat(null);
      }
    },
    [loadScanLogs, results, selectedMode],
  );

  const ignoreStaticRule = useCallback(
    async (patternId: string) => {
      const normalized = patternId.trim().toLowerCase();
      if (!normalized) return;
      if (ignoredRuleSet.has(normalized)) return;
      if (ignoringRuleIds.has(normalized)) return;

      setIgnoringRuleIds((prev) => {
        const next = new Set(prev);
        next.add(normalized);
        return next;
      });

      try {
        const mergedIgnoreRules = Array.from(
          new Set(
            [...(scanPolicy.ignore_rules ?? []).map((rule) => rule.trim().toLowerCase()), normalized].filter(Boolean),
          ),
        );
        await persistScanPolicy({
          ...scanPolicy,
          ignore_rules: mergedIgnoreRules,
        });
      } finally {
        setIgnoringRuleIds((prev) => {
          const next = new Set(prev);
          next.delete(normalized);
          return next;
        });
      }
    },
    [ignoredRuleSet, ignoringRuleIds, persistScanPolicy, scanPolicy],
  );

  const riskCounts = useMemo(() => {
    const counts = { safe: 0, low: 0, medium: 0, high: 0, critical: 0 };
    for (const result of results) {
      switch (result.risk_level) {
        case "Safe":
          counts.safe++;
          break;
        case "Low":
          counts.low++;
          break;
        case "Medium":
          counts.medium++;
          break;
        case "High":
          counts.high++;
          break;
        case "Critical":
          counts.critical++;
          break;
      }
    }
    return counts;
  }, [results]);

  const overallRisk = useMemo(() => calculateOverallRisk(results), [results]);

  const sortedResults = useMemo(() => {
    const order: Record<RiskLevel, number> = { Critical: 0, High: 1, Medium: 2, Low: 3, Safe: 4 };
    return [...results].sort((a, b) => order[a.risk_level] - order[b.risk_level]);
  }, [results]);

  const lastScanTime = useMemo(() => {
    if (results.length === 0) return null;
    const latestScanTimestamp = results.reduce((latestTimestamp, result) => {
      return result.scanned_at > latestTimestamp ? result.scanned_at : latestTimestamp;
    }, results[0].scanned_at);
    return formatTimestamp(latestScanTimestamp);
  }, [results]);

  const totalFindings = useMemo(() => {
    return results.reduce((sum, result) => sum + result.static_findings.length + result.ai_findings.length, 0);
  }, [results]);

  const pinnedHighRiskTrail = useMemo(() => {
    return recentFiles.find((item) => item.riskLevel === "Critical" || item.riskLevel === "High") ?? null;
  }, [recentFiles]);
  const activeSkillsLabel = activeSkills.length > 0 ? activeSkills.join(" · ") : "…";

  const hasIssues = riskCounts.low + riskCounts.medium + riskCounts.high + riskCounts.critical > 0;
  const latestLog = scanLogs[0] ?? null;
  const latestLogTime = latestLog ? formatTimestamp(latestLog.created_at) : null;
  const showLogHint = phase === "done" || (phase === "idle" && results.length > 0);
  const modeOptions: Array<{ mode: ScanMode; label: string; icon: typeof ScanLine; desc: string }> = [
    {
      mode: "static",
      label: t("securityScan.staticScan", "Static"),
      icon: ScanLine,
      desc: t("securityScan.modeDesc.static", "Pattern matching only"),
    },
    {
      mode: "smart",
      label: t("securityScan.smartScan", "Smart"),
      icon: Zap,
      desc: t("securityScan.modeDesc.smart", "Static + AI analysis"),
    },
    {
      mode: "deep",
      label: t("securityScan.deepScan", "Deep"),
      icon: Brain,
      desc: t("securityScan.modeDesc.deep", "Full AI deep scan"),
    },
  ];
  const exportOptions: Array<{
    format: "sarif" | "json" | "markdown" | "html";
    label: string;
  }> = [
    { format: "sarif", label: "SARIF" },
    { format: "json", label: "JSON" },
    { format: "markdown", label: "Markdown" },
    { format: "html", label: "HTML" },
  ];
  const selectedModeMeta = modeOptions.find((item) => item.mode === selectedMode) ?? modeOptions[1];
  const SelectedModeIcon = selectedModeMeta.icon;
  const canStartScan = phase !== "scanning";
  const showLivePanel = phase === "scanning" || (phase !== "done" && results.length === 0);

  // Dropdown state for mode picker
  const [modeDropdownOpen, setModeDropdownOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!modeDropdownOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setModeDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [modeDropdownOpen]);
  const showIdlePanel = phase !== "scanning" && results.length === 0;

  return (
    <div className="flex-1 min-w-0 min-h-0 flex flex-col overflow-hidden">
      {/* ── Header ── */}
      <div className="flex items-center justify-between px-6 py-3 border-b border-border shrink-0">
        <div className="flex items-center gap-2.5">
          <ShieldCheck className="w-4 h-4 text-success" />
          <h1 className="text-sm font-semibold text-foreground">{t("sidebar.security", "Security Scan")}</h1>
          {phase === "done" && (
            <span className="text-[11px] text-muted-foreground ml-1">
              {results.length} {t("securityScan.skillsLabel", "skills")}
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {phase === "done" && (
            <button
              onClick={clearCache}
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[11px] text-muted-foreground hover:text-foreground hover:bg-muted transition-all cursor-pointer"
            >
              <Trash2 className="w-3 h-3" />
              {t("securityScan.clearCache", "Clear Cache")}
            </button>
          )}
          {phase === "scanning" ? (
            <div className="flex items-center gap-2">
              <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs text-success/80">
                <motion.div animate={{ rotate: 360 }} transition={{ duration: 1, repeat: Infinity, ease: "linear" }}>
                  <Zap className="w-3 h-3" />
                </motion.div>
                {t("securityScan.scanning", "Scanning...")}
                <ScanTimer startedAt={scanStartedAt} />
              </div>
              <button
                onClick={cancelScan}
                className="group flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-destructive/10 hover:bg-destructive/20 text-destructive border border-destructive/20 transition-all cursor-pointer shadow-sm"
              >
                <ShieldX className="w-3 h-3" />
                {t("securityScan.stopScan", "Stop")}
              </button>
            </div>
          ) : (
            <div className="relative flex items-center" ref={dropdownRef}>
              {/* Split combo button: [Mode ▾ | Action] */}
              <div className="inline-flex items-stretch rounded-lg overflow-hidden shadow-sm shadow-emerald-600/15">
                {/* ── Mode selector half ── */}
                <button
                  onClick={() => setModeDropdownOpen((v) => !v)}
                  className="inline-flex items-center gap-1.5 pl-3 pr-2 py-1.5 text-[11px] font-medium bg-gradient-to-r from-emerald-600/90 to-emerald-600/80 text-white/90 hover:text-white hover:from-emerald-600 hover:to-emerald-500/90 transition-all cursor-pointer border-r border-white/15"
                >
                  <SelectedModeIcon className="w-3.5 h-3.5" />
                  {selectedModeMeta.label}
                  <ChevronDown
                    className={`w-3 h-3 opacity-60 transition-transform duration-200 ${modeDropdownOpen ? "rotate-180" : ""}`}
                  />
                </button>
                {/* ── Action half ── */}
                <button
                  onClick={() => {
                    setModeDropdownOpen(false);
                    startScan([], !incrementalScan, selectedMode);
                  }}
                  disabled={!canStartScan}
                  className="group inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-white bg-gradient-to-r from-emerald-600/80 to-emerald-500 hover:brightness-110 disabled:opacity-60 disabled:cursor-not-allowed transition-all cursor-pointer"
                >
                  {phase === "done" ? (
                    <RotateCcw className="w-3.5 h-3.5 group-hover:-rotate-45 transition-transform duration-300" />
                  ) : (
                    <SelectedModeIcon className="w-3.5 h-3.5" />
                  )}
                  {phase === "done" ? t("securityScan.rescan", "Rescan") : t("securityScan.startScan", "Start Scan")}
                </button>
              </div>

              {/* ── Mode dropdown ── */}
              {modeDropdownOpen && (
                <motion.div
                  initial={{ opacity: 0, y: -4, scale: 0.97 }}
                  animate={{ opacity: 1, y: 0, scale: 1 }}
                  exit={{ opacity: 0, y: -4, scale: 0.97 }}
                  transition={MOTION_TRANSITION.fadeFast}
                  className="absolute right-0 top-full mt-1.5 z-50 w-56 rounded-xl border border-border/60 bg-card/95 backdrop-blur-xl shadow-xl shadow-black/30 p-1"
                >
                  {modeOptions.map((option) => {
                    const Icon = option.icon;
                    const active = option.mode === selectedMode;
                    return (
                      <button
                        key={option.mode}
                        onClick={() => {
                          setSelectedMode(option.mode);
                          setModeDropdownOpen(false);
                        }}
                        className={`w-full flex items-start gap-2.5 rounded-lg px-3 py-2.5 text-left transition-colors cursor-pointer ${
                          active ? "bg-emerald-500/12 text-emerald-300" : "text-foreground hover:bg-muted/60"
                        }`}
                      >
                        <Icon
                          className={`w-4 h-4 mt-0.5 shrink-0 ${active ? "text-emerald-400" : "text-muted-foreground"}`}
                        />
                        <div className="flex-1 min-w-0">
                          <div className="text-[12px] font-medium leading-tight">{option.label}</div>
                          <div className="text-[10px] text-muted-foreground mt-0.5 leading-snug">{option.desc}</div>
                        </div>
                        {active && <div className="w-1.5 h-1.5 rounded-full bg-emerald-400 mt-1.5 shrink-0" />}
                      </button>
                    );
                  })}
                  {/* Estimate hint inside dropdown */}
                  {scanEstimate && !estimating && (
                    <div className="mt-1 border-t border-border/40 pt-2 pb-1 px-3">
                      <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
                        <span className="tabular-nums">
                          {scanEstimate.totalFiles} {t("securityScan.estimateFiles", "files")}
                        </span>
                        <span className="text-border">·</span>
                        <span className="tabular-nums">
                          {scanEstimate.estimatedApiCalls} {t("securityScan.estimateCalls", "API calls")}
                        </span>
                      </div>
                    </div>
                  )}
                </motion.div>
              )}
            </div>
          )}
        </div>
      </div>

      {phase !== "scanning" && scanEstimate && !estimating && scanEstimate.effectiveMode !== selectedMode && (
        <div className="px-6 py-2 border-b border-border/50 bg-card/20 shrink-0">
          <div className="flex items-center gap-2 text-[11px] text-amber-300">
            <AlertTriangle size={12} />
            {t("securityScan.estimateFallback", "AI unavailable, using STATIC mode")}
          </div>
        </div>
      )}

      {phase !== "scanning" && (
        <div className="px-6 py-2 border-b border-border/50 bg-card/25 shrink-0">
          <div className="flex flex-wrap items-center gap-2 text-[11px]">
            <span className="text-muted-foreground">{t("securityScan.policyLabel", "Policy")}</span>
            <select
              value={scanPolicy.preset}
              onChange={(event) =>
                void persistScanPolicy({
                  ...scanPolicy,
                  preset: event.target.value,
                })
              }
              disabled={policySaving}
              className="h-7 rounded-md border border-border/60 bg-card px-2 text-[11px] text-foreground disabled:opacity-60"
            >
              <option value="strict">{t("securityScan.policyPreset.strict", "Strict")}</option>
              <option value="balanced">{t("securityScan.policyPreset.balanced", "Balanced")}</option>
              <option value="permissive">{t("securityScan.policyPreset.permissive", "Permissive")}</option>
            </select>
            <span className="text-muted-foreground">{t("securityScan.thresholdLabel", "Min Severity")}</span>
            <select
              value={scanPolicy.severity_threshold}
              onChange={(event) =>
                void persistScanPolicy({
                  ...scanPolicy,
                  severity_threshold: event.target.value,
                })
              }
              disabled={policySaving}
              className="h-7 rounded-md border border-border/60 bg-card px-2 text-[11px] text-foreground disabled:opacity-60"
            >
              <option value="safe">{t("securityScan.risk.safe", "Safe")}</option>
              <option value="low">{t("securityScan.risk.low", "Low Risk")}</option>
              <option value="medium">{t("securityScan.risk.medium", "Medium Risk")}</option>
              <option value="high">{t("securityScan.risk.high", "High Risk")}</option>
              <option value="critical">{t("securityScan.risk.critical", "Critical")}</option>
            </select>
            <span className="text-[10px] text-muted-foreground">
              {policySaving
                ? t("securityScan.policySaving", "Saving policy...")
                : t("securityScan.policySaved", "Policy saved")}
            </span>
            <button
              onClick={() => setIncrementalScan(!incrementalScan)}
              disabled={policySaving}
              className={`h-7 rounded-md border px-2.5 text-[10px] font-medium transition-colors cursor-pointer disabled:opacity-60 disabled:cursor-not-allowed ${
                incrementalScan
                  ? "border-success/40 bg-success/10 text-success"
                  : "border-border/60 bg-card text-muted-foreground"
              }`}
            >
              {incrementalScan
                ? t("securityScan.incrementalOn", "Incremental: ON")
                : t("securityScan.incrementalOff", "Incremental: OFF")}
            </button>
            <input
              value={analyzersDraft}
              onChange={(event) => setAnalyzersDraft(event.target.value)}
              disabled={policySaving}
              placeholder={t(
                "securityScan.analyzersPlaceholder",
                "pattern, doc_consistency, secrets, semantic, dynamic, semgrep, trivy, osv, grype, gitleaks, shellcheck, bandit, sbom, virustotal",
              )}
              className="h-7 min-w-[170px] rounded-md border border-border/60 bg-card px-2 text-[11px] text-foreground placeholder:text-muted-foreground/60 disabled:opacity-60"
            />
            <button
              onClick={() =>
                void persistScanPolicy({
                  ...scanPolicy,
                  enabled_analyzers: parseAnalyzerInput(analyzersDraft),
                })
              }
              disabled={policySaving}
              className="h-7 rounded-md border border-border/60 bg-card px-2.5 text-[10px] font-medium text-foreground hover:bg-card-hover disabled:opacity-60 disabled:cursor-not-allowed transition-colors cursor-pointer"
            >
              {t("securityScan.applyAnalyzers", "Apply Analyzers")}
            </button>
            <input
              value={ignoreRulesDraft}
              onChange={(event) => setIgnoreRulesDraft(event.target.value)}
              disabled={policySaving}
              placeholder={t("securityScan.ignoreRulesPlaceholder", "ignore_rule_a, ignore_rule_b")}
              className="h-7 min-w-[180px] rounded-md border border-border/60 bg-card px-2 text-[11px] text-foreground placeholder:text-muted-foreground/60 disabled:opacity-60"
            />
            <button
              onClick={() =>
                void persistScanPolicy({
                  ...scanPolicy,
                  ignore_rules: parseIgnoreRulesInput(ignoreRulesDraft),
                })
              }
              disabled={policySaving}
              className="h-7 rounded-md border border-border/60 bg-card px-2.5 text-[10px] font-medium text-foreground hover:bg-card-hover disabled:opacity-60 disabled:cursor-not-allowed transition-colors cursor-pointer"
            >
              {t("securityScan.applyIgnoreRules", "Apply Ignore Rules")}
            </button>
          </div>
        </div>
      )}

      {showLogHint && (
        <div className="px-6 py-2 border-b border-border/50 bg-card/20 shrink-0">
          <div className="flex flex-wrap items-center gap-2 text-[11px]">
            <div className="inline-flex items-center gap-1.5 text-muted-foreground">
              <FolderOpen className="w-3.5 h-3.5 text-success/80" />
              <span>
                {t(
                  "securityScan.logHint",
                  "Each scan is saved as a timestamped log. You can open the log folder for details.",
                )}
              </span>
              {latestLogTime && (
                <span className="text-foreground/70">
                  {t("securityScan.latestLog", "Latest: {{time}}", { time: latestLogTime })}
                </span>
              )}
            </div>
            <button
              onClick={openScanLogFolder}
              disabled={!scanLogDir || openingLogFolder}
              className="inline-flex items-center gap-1.5 rounded-md border border-border/60 bg-card px-2.5 py-1 text-[10px] font-medium text-foreground hover:bg-card-hover disabled:opacity-60 disabled:cursor-not-allowed transition-colors cursor-pointer"
            >
              <FolderOpen className="w-3 h-3" />
              {t("securityScan.openLogs", "Open Logs")}
            </button>
            {exportOptions.map((item) => (
              <button
                key={item.format}
                onClick={() => void exportScanReport(item.format)}
                disabled={exportingFormat !== null}
                className="inline-flex items-center gap-1.5 rounded-md border border-border/60 bg-card px-2.5 py-1 text-[10px] font-medium text-foreground hover:bg-card-hover disabled:opacity-60 disabled:cursor-not-allowed transition-colors cursor-pointer"
              >
                <Download className="w-3 h-3" />
                {exportingFormat === item.format
                  ? t("securityScan.exportingFormat", {
                      defaultValue: "Exporting {{format}}...",
                      format: item.label,
                    })
                  : t("securityScan.exportFormat", {
                      defaultValue: "Export {{format}}",
                      format: item.label,
                    })}
              </button>
            ))}
            {latestReportPath && (
              <span className="text-[10px] text-muted-foreground truncate max-w-[420px]">
                {t("securityScan.latestReport", "Last Report: {{path}}", { path: latestReportPath })}
              </span>
            )}
          </div>
        </div>
      )}

      {phase === "scanning" && (
        <div className="px-6 py-2 border-b border-border/60 bg-card/30 shrink-0">
          <div className="flex flex-wrap items-center gap-2.5 text-[10px]">
            <div className="inline-flex items-center gap-1.5 rounded-full border border-success/20 bg-success/8 px-2.5 py-1 text-success">
              <Radar className="w-3.5 h-3.5" />
              <span className="uppercase tracking-[0.18em]">{currentStage ?? "scan"}</span>
            </div>

            <div className="inline-flex min-w-0 items-center gap-1.5 rounded-full border border-border/60 bg-background/60 px-2.5 py-1 text-muted-foreground">
              <span className="max-w-[280px] truncate text-foreground">{activeSkillsLabel}</span>
            </div>

            {pinnedHighRiskTrail && (
              <div className="inline-flex min-w-0 items-center gap-1.5 rounded-full border border-orange-400/20 bg-orange-500/8 px-2.5 py-1 text-orange-200">
                <AlertTriangle className="w-3.5 h-3.5" />
                <span className="max-w-[220px] truncate text-foreground/95">{pinnedHighRiskTrail.fileName}</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* ── Main content ── */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {/* ── Idle / Scanning: Radar ── */}
        {showLivePanel && (
          <div className="flex flex-col items-center justify-center h-full min-h-[400px] gap-8 px-6 py-8">
            {phase === "scanning" ? (
              <div className="flex w-full max-w-5xl flex-col items-center justify-center gap-8 lg:flex-row lg:items-center lg:justify-center lg:gap-10">
                <div className="flex flex-col items-center gap-6">
                  <RadarSweep
                    active={phase === "scanning"}
                    activeSkills={activeSkills}
                    currentStage={currentStage}
                    syncPulseKey={syncPulseKey}
                    scanned={scanned}
                    total={total}
                    progressPercent={progressPercent}
                  />

                  {total > 0 && (
                    <motion.div
                      initial={{ opacity: 0, y: 8 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={MOTION_TRANSITION.enter}
                      className="w-64"
                    >
                      <div className="h-1 rounded-full bg-muted overflow-hidden">
                        <motion.div
                          className="h-full rounded-full bg-success origin-left"
                          style={{ width: "100%" }}
                          initial={{ scaleX: 0 }}
                          animate={{ scaleX: progressPercent / 100 }}
                          transition={MOTION_TRANSITION.progress}
                        />
                      </div>
                      <div className="text-center text-[10px] text-muted-foreground mt-2">
                        {scanned} / {total} skills analyzed
                      </div>
                    </motion.div>
                  )}
                </div>

                <ScanFilePanel
                  activeSkills={activeSkills}
                  currentSkill={currentSkill}
                  fileName={currentFile}
                  stage={currentStage}
                  syncPulseKey={syncPulseKey}
                  recentFiles={recentFiles}
                  activeChunkFiles={activeChunkFiles}
                  progressPercent={progressPercent}
                  activeChunkWorkers={activeChunkWorkers}
                  maxChunkWorkers={maxChunkWorkers}
                />
              </div>
            ) : (
              <>
                <RadarSweep
                  active={false}
                  activeSkills={[]}
                  currentStage={currentStage}
                  syncPulseKey={syncPulseKey}
                  scanned={scanned}
                  total={total}
                  progressPercent={progressPercent}
                />

                {showIdlePanel && (
                  <div className="text-center max-w-sm space-y-4">
                    <p className="text-muted-foreground text-xs leading-relaxed">
                      {t(
                        "securityScan.idleDescription",
                        "Scan your installed skills for security threats using static pattern matching and AI-powered analysis.",
                      )}
                    </p>
                    <div className="flex items-center justify-center gap-3">
                      <button
                        onClick={() => startScan([], !incrementalScan, selectedMode)}
                        className="group relative inline-flex items-center gap-2 px-5 py-2.5 rounded-xl text-sm font-medium text-white transition-all cursor-pointer overflow-hidden shadow-lg shadow-emerald-600/20 hover:shadow-emerald-600/40"
                      >
                        <div className="absolute inset-0 bg-gradient-to-r from-emerald-600 to-emerald-500 transition-transform duration-500 group-hover:scale-105" />
                        <div className="relative flex items-center gap-2 z-10">
                          <SelectedModeIcon className="w-4 h-4 opacity-90 transition-transform duration-300 group-hover:scale-110" />
                          <span>{t("securityScan.startScan", "Start Scan")}</span>
                        </div>
                      </button>
                    </div>
                  </div>
                )}

                {results.length > 0 && (
                  <motion.div
                    initial={{ opacity: 0, y: 10 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={MOTION_TRANSITION.enter}
                    className="text-center"
                  >
                    <p className="text-muted-foreground text-[11px]">
                      {t("securityScan.cachedResultsHint", {
                        count: results.length,
                        defaultValue: "{{count}} cached result(s) from previous scan",
                      })}
                    </p>
                  </motion.div>
                )}
              </>
            )}
          </div>
        )}

        {/* ── Results ── */}
        {phase === "done" && (
          <motion.div
            key="results"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ ...MOTION_TRANSITION.fadeMedium, delay: 0.1 }}
            className="p-5 space-y-4"
          >
            {/* ── Top: Score + Summary ── */}
            <motion.div
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={MOTION_TRANSITION.fadeMedium}
              className="flex items-start gap-5"
            >
              {/* Score ring — left */}
              <div className="shrink-0 relative w-[72px] h-[72px]">
                <svg viewBox="0 0 36 36" className="w-full h-full -rotate-90">
                  <circle
                    cx="18"
                    cy="18"
                    r="15.5"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    className="text-border"
                  />
                  <motion.circle
                    cx="18"
                    cy="18"
                    r="15.5"
                    fill="none"
                    strokeWidth="2.5"
                    strokeLinecap="round"
                    className={riskColor[overallRisk.level]}
                    stroke="currentColor"
                    initial={{ strokeDasharray: "0 100" }}
                    animate={{
                      strokeDasharray: `${Math.max(overallRisk.safetyScore, 2)} ${100 - Math.max(overallRisk.safetyScore, 2)}`,
                    }}
                    transition={{ ...MOTION_TRANSITION.ring, delay: 0.15 }}
                  />
                </svg>
                <div
                  className={`absolute inset-0 flex items-center justify-center text-lg font-bold tabular-nums ${riskColor[overallRisk.level]}`}
                >
                  {overallRisk.safetyScore}
                </div>
              </div>

              {/* Summary — right */}
              <div className="flex-1 min-w-0 pt-1">
                <div className="flex items-center gap-2 mb-1.5">
                  <RiskIcon level={overallRisk.level} size={15} />
                  <span className={`text-sm font-semibold ${riskColor[overallRisk.level]}`}>
                    {getRiskLabel(t, overallRisk.level)}
                  </span>
                </div>

                {/* Meta row */}
                <div className="flex items-center gap-3 text-[11px] text-muted-foreground flex-wrap mb-3">
                  <span className="flex items-center gap-1 tabular-nums">
                    <ShieldAlert size={11} />
                    {t("securityScan.riskScore", {
                      score: overallRisk.riskScore10.toFixed(1),
                      defaultValue: "Risk {{score}}/10",
                    })}
                  </span>
                  <span className="flex items-center gap-1">
                    <ScanLine size={11} />
                    {t("securityScan.scannedCount", {
                      count: results.length,
                      defaultValue: "{{count}} scanned",
                    })}
                  </span>
                  {totalFindings > 0 && (
                    <span className="flex items-center gap-1">
                      <AlertTriangle size={11} />
                      {t("securityScan.findingsCount", {
                        count: totalFindings,
                        defaultValue: "{{count}} findings",
                      })}
                    </span>
                  )}
                  {lastScanTime && (
                    <span className="flex items-center gap-1">
                      <Clock size={11} />
                      {lastScanTime}
                    </span>
                  )}
                </div>

                {/* Inline stat pills — only non-zero counts */}
                <div className="flex items-center gap-1.5 flex-wrap">
                  <StatPill level="Safe" count={riskCounts.safe} />
                  <StatPill level="Low" count={riskCounts.low} />
                  <StatPill level="Medium" count={riskCounts.medium} />
                  <StatPill level="High" count={riskCounts.high} />
                  <StatPill level="Critical" count={riskCounts.critical} />
                </div>
              </div>
            </motion.div>

            {/* ── Separator ── */}
            <div className="border-t border-border" />

            {/* ── Error notices ── */}
            {errors.length > 0 && (
              <div className="rounded-lg border border-destructive/15 bg-destructive/5 px-4 py-3">
                <div className="text-xs text-destructive font-medium mb-1.5 flex items-center gap-1.5">
                  <AlertTriangle size={12} />
                  {t("securityScan.errors", "Scan Errors")}
                </div>
                {errors.map((e, i) => (
                  <div key={`${e.skillName}-${i}`} className="text-[11px] text-destructive/60 pl-4">
                    {e.skillName}: {e.message}
                  </div>
                ))}
              </div>
            )}

            {/* ── Results table header ── */}
            {hasIssues && (
              <div className="flex items-center gap-2 px-1 text-[10px] font-medium text-muted-foreground/60 uppercase tracking-wider select-none">
                <span className="w-5" />
                <span className="flex-1">{t("securityScan.table.skill", "Skill")}</span>
                <span className="w-16 text-right">{t("securityScan.table.files", "Files")}</span>
                <span className="w-24 text-right">{t("securityScan.table.status", "Status")}</span>
                <span className="w-4" />
              </div>
            )}

            {/* ── Results list ── */}
            <div className="space-y-px">
              {sortedResults.map((result, i) => (
                <motion.div
                  key={result.skill_name}
                  initial={{ opacity: 0, y: 6 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ ...MOTION_TRANSITION.enter, delay: motionDelay(i) }}
                >
                  <SkillResultRow
                    result={result}
                    expanded={expandedSkill === result.skill_name}
                    ignoredRules={ignoredRuleSet}
                    ignoringRules={ignoringRuleIds}
                    onIgnoreRule={ignoreStaticRule}
                    onToggle={() => setExpandedSkill((prev) => (prev === result.skill_name ? null : result.skill_name))}
                  />
                </motion.div>
              ))}
            </div>
          </motion.div>
        )}
      </div>
    </div>
  );
}

// ── Skill Result Row ──────────────────────────────────────────────

const SkillResultRow = React.memo(function SkillResultRow({
  result,
  expanded,
  ignoredRules,
  ignoringRules,
  onIgnoreRule,
  onToggle,
}: {
  result: SecurityScanResult;
  expanded: boolean;
  ignoredRules: Set<string>;
  ignoringRules: Set<string>;
  onIgnoreRule: (patternId: string) => void | Promise<void>;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const totalFindings = result.static_findings.length + result.ai_findings.length;
  const isSafe = result.risk_level === "Safe";
  const riskScore = resolveSkillRiskScore(result);
  const metaDeduped = result.meta_deduped_count ?? 0;
  const metaConsensus = result.meta_consensus_count ?? 0;
  const analyzerExecutions = result.analyzer_executions ?? [];
  const confidencePct = Math.round(
    (typeof result.confidence_score === "number" && Number.isFinite(result.confidence_score)
      ? result.confidence_score
      : 0.5) * 100,
  );
  const localizedSummary = localizeScanSummary(t, result.summary || "");
  const hideSummary =
    !localizedSummary || localizedSummary === t("securityScan.summary.noIssues", { defaultValue: "No issues found." });

  return (
    <CardTemplate
      className={`rounded-lg transition-colors hover:-translate-y-0 ${expanded ? "bg-card/80 ring-1 ring-border" : `hover:bg-card/40 ${riskBgSubtle[result.risk_level]}`}`}
    >
      <button onClick={onToggle} className="w-full flex items-center gap-3 px-3 py-2.5 text-left cursor-pointer group">
        {/* Risk dot */}
        <div className="w-5 flex items-center justify-center shrink-0">
          <span
            className={`w-2 h-2 rounded-full ${riskDot[result.risk_level]} ${!isSafe ? "ring-2 ring-current/10" : ""}`}
          />
        </div>

        {/* Name + summary */}
        <div className="flex-1 min-w-0">
          <div className="text-[13px] font-medium text-foreground truncate leading-tight">{result.skill_name}</div>
          {!hideSummary && (
            <div className="text-[10px] text-muted-foreground mt-0.5 whitespace-normal break-words [overflow-wrap:anywhere] leading-relaxed pr-1">
              {localizedSummary}
            </div>
          )}
        </div>

        {/* Files */}
        <div className="w-16 flex items-center justify-end gap-1 text-muted-foreground/60 shrink-0">
          <FileText size={10} />
          <span className="text-[10px] tabular-nums">{result.files_scanned}</span>
        </div>

        {/* Status */}
        <div className="w-24 flex items-center justify-end shrink-0">
          {totalFindings > 0 ? (
            <span
              className={`inline-flex items-center gap-1 text-[10px] font-medium tabular-nums px-1.5 py-0.5 rounded-md ${riskColor[result.risk_level]} bg-current/8`}
            >
              {t("securityScan.findingsCount", {
                count: totalFindings,
                defaultValue: "{{count}} findings",
              })}
            </span>
          ) : (
            <span className="flex items-center gap-1 text-success text-[10px]">
              <ShieldCheck size={10} />
              {t("securityScan.clean", "Clean")}
            </span>
          )}
        </div>

        {/* Risk score */}
        <div className="w-20 flex items-center justify-end shrink-0 text-[10px] tabular-nums text-muted-foreground">
          <span>{riskScore.toFixed(1)}/10</span>
          <span className="ml-1 text-[9px] text-muted-foreground/70">{confidencePct}%</span>
        </div>

        {/* Chevron */}
        <div className="w-4 shrink-0">
          <ChevronRight
            size={12}
            className={`text-muted-foreground/40 transition-transform duration-200 ${expanded ? "rotate-90" : "group-hover:translate-x-0.5"}`}
          />
        </div>
      </button>

      {/* ── Expandable findings ── */}
      <div
        className="grid transition-[grid-template-rows] duration-200 ease-out"
        style={{ gridTemplateRows: expanded ? "1fr" : "0fr" }}
      >
        <div className="overflow-hidden">
          <div className="px-4 pb-3 space-y-3 border-t border-border/50 pt-3 ml-5">
            {(metaDeduped > 0 || metaConsensus > 0) && (
              <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
                {metaDeduped > 0 && <span>deduped {metaDeduped}</span>}
                {metaConsensus > 0 && <span>consensus {metaConsensus}</span>}
              </div>
            )}
            {analyzerExecutions.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5 text-[10px] text-muted-foreground">
                <span className="opacity-70">analyzers</span>
                {analyzerExecutions.map((exec) => {
                  const tone =
                    exec.status === "failed"
                      ? "text-destructive border-destructive/40 bg-destructive/10"
                      : exec.status === "unavailable"
                        ? "text-amber-300 border-amber-300/40 bg-amber-300/10"
                        : exec.status === "ran"
                          ? "text-success border-success/40 bg-success/10"
                          : "text-muted-foreground border-border/50 bg-muted/20";
                  return (
                    <span
                      key={exec.id}
                      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 ${tone}`}
                      title={exec.error || undefined}
                    >
                      <span>{exec.id}</span>
                      <span className="opacity-70">{exec.status}</span>
                      <span className="tabular-nums opacity-80">{exec.findings}</span>
                    </span>
                  );
                })}
              </div>
            )}
            {/* Static findings */}
            {result.static_findings.length > 0 && (
              <div>
                <div className="flex items-center gap-1.5 mb-2">
                  <Zap size={11} className="text-amber-400" />
                  <span className="text-[11px] font-medium text-muted-foreground">
                    {t("securityScan.staticMatches", "Static Pattern Matches")}
                  </span>
                  <span className="text-[10px] text-muted-foreground/60">
                    {t("securityScan.confidence", {
                      defaultValue: "confidence",
                    })}
                  </span>
                </div>
                <div className="space-y-1">
                  {result.static_findings.map((f) => {
                    const normalizedPattern = f.pattern_id.trim().toLowerCase();
                    const isIgnored = ignoredRules.has(normalizedPattern);
                    const isIgnoring = ignoringRules.has(normalizedPattern);
                    return (
                      <div
                        key={`${f.file_path}:${f.line_number}`}
                        className="flex items-start gap-2 text-[11px] pl-3 py-1.5 rounded-md bg-muted/30"
                      >
                        <span className={`shrink-0 font-medium ${riskColor[f.severity]}`}>
                          [{getRiskShortLabel(t, f.severity)}]
                        </span>
                        <div className="min-w-0">
                          <span className="text-muted-foreground font-mono text-[10px]">
                            {f.file_path}:{f.line_number}
                          </span>
                          <span className="text-muted-foreground/60 ml-1 text-[10px] tabular-nums">
                            conf {Math.round((f.confidence ?? 0.78) * 100)}%
                          </span>
                          <span className="text-foreground/60 ml-1">
                            — {getStaticDescription(t, f.pattern_id, f.description)}
                          </span>
                          <button
                            onClick={() => onIgnoreRule(f.pattern_id)}
                            disabled={isIgnored || isIgnoring}
                            className={`ml-2 inline-flex items-center rounded border px-1.5 py-0.5 text-[9px] uppercase tracking-wide transition-colors ${
                              isIgnored
                                ? "border-success/40 bg-success/10 text-success"
                                : "border-border/60 bg-card text-muted-foreground hover:text-foreground hover:bg-card-hover"
                            } disabled:cursor-not-allowed disabled:opacity-70`}
                          >
                            {isIgnored
                              ? t("securityScan.ruleIgnored", "Ignored")
                              : isIgnoring
                                ? t("securityScan.ruleIgnoring", "Ignoring...")
                                : t("securityScan.ignoreRule", "Ignore Rule")}
                          </button>
                          <OwaspTagList tags={staticFindingOwaspTags(f)} />
                        </div>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}

            {/* AI findings */}
            {result.ai_findings.length > 0 && (
              <div>
                <div className="flex items-center gap-1.5 mb-2">
                  <Brain size={11} className="text-ai-text" />
                  <span className="text-[11px] font-medium text-muted-foreground">
                    {t("securityScan.aiAnalysis", "AI Analysis")}
                  </span>
                </div>
                <div className="space-y-1.5">
                  {result.ai_findings.map((f, i) => (
                    <div key={`${f.category}-${i}`} className="pl-3 py-2 rounded-md bg-muted/30 space-y-1">
                      <div className="text-[11px]">
                        <span className={`font-medium ${riskColor[f.severity]}`}>
                          [{getRiskShortLabel(t, f.severity)}]
                        </span>{" "}
                        <span className="text-foreground font-medium">{localizeAiCategory(t, f.category)}</span>
                        <span className="text-muted-foreground/60 ml-1 text-[10px] tabular-nums">
                          conf {Math.round((f.confidence ?? 0.72) * 100)}%
                        </span>
                        <span className="text-foreground/60 ml-1">— {f.description}</span>
                      </div>
                      {f.evidence && (
                        <div className="text-[10px] text-muted-foreground font-mono bg-card/80 rounded px-2 py-1 truncate">
                          {f.evidence}
                        </div>
                      )}
                      {f.recommendation && (
                        <div className="text-[10px] text-success flex items-start gap-1">
                          <span className="shrink-0 mt-px">→</span>
                          <span>{f.recommendation}</span>
                        </div>
                      )}
                      <OwaspTagList tags={aiFindingOwaspTags(f)} />
                    </div>
                  ))}
                </div>
              </div>
            )}

            {totalFindings === 0 && (
              <div className="flex items-center justify-center gap-2 text-[11px] text-success py-2">
                <ShieldCheck size={13} />
                <span>{t("securityScan.noIssues", "No issues found")}</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </CardTemplate>
  );
});

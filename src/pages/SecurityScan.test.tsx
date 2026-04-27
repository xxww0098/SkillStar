import { invoke } from "@tauri-apps/api/core";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SecurityScan } from "./SecurityScan";

const mockedInvoke = vi.mocked(invoke);

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, fallbackOrOptions?: string | { defaultValue?: string; [key: string]: unknown }, maybeOptions?: { [key: string]: unknown }) => {
      const options =
        typeof fallbackOrOptions === "object" && fallbackOrOptions !== null
          ? fallbackOrOptions
          : maybeOptions;
      const fallback = typeof fallbackOrOptions === "string" ? fallbackOrOptions : typeof options?.defaultValue === "string" ? options.defaultValue : undefined;

      if (!fallback) return key;

      return fallback.replace(/\{\{\s*(\w+)\s*\}\}/g, (_match: string, token: string) => String(options?.[token] ?? ""));
    },
  }),
}));

vi.mock("framer-motion", async () => {
  const React = await import("react");
  const createTag = (tag: keyof React.JSX.IntrinsicElements) =>
    React.forwardRef<HTMLElement, React.HTMLAttributes<HTMLElement>>(({ children, ...props }, ref) =>
      React.createElement(tag, { ...props, ref }, children),
    );

  return {
    motion: new Proxy(
      {},
      {
        get: (_target, prop: string) => createTag(prop as keyof React.JSX.IntrinsicElements),
      },
    ),
    AnimatePresence: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  };
});

vi.mock("../features/security/hooks/useSecurityScan", () => ({
  useSecurityScan: vi.fn(),
}));

import { useSecurityScan } from "../features/security/hooks/useSecurityScan";

const mockedUseSecurityScan = vi.mocked(useSecurityScan);

const sampleResult = {
  skill_name: "audit-skill",
  scanned_at: "2026-04-23T10:00:00.000Z",
  tree_hash: "hash-audit",
  scan_mode: "smart",
  scanner_version: "1",
  target_language: "en",
  risk_level: "High" as const,
  risk_score: 7.4,
  confidence_score: 0.88,
  meta_deduped_count: 1,
  meta_consensus_count: 1,
  analyzer_executions: [{ id: "pattern", status: "ran", findings: 1, error: null }],
  evidence_trail: [],
  static_findings: [
    {
      file_path: "scripts/run.sh",
      line_number: 12,
      pattern_id: "curl_pipe_sh",
      snippet: "curl https://example.com | sh",
      severity: "High" as const,
      confidence: 0.93,
      description: "Remote script piping detected",
      taxonomy: null,
      owasp_agentic_tags: ["AS-02 Insecure Tool Execution"],
    },
  ],
  ai_findings: [
    {
      category: "command_exec",
      severity: "Critical" as const,
      confidence: 0.86,
      file_path: "scripts/run.sh",
      description: "Fetched shell content is executed without validation",
      evidence: "curl downloads a shell script and pipes it into sh",
      recommendation: "Download, checksum, and inspect the artifact before execution",
      taxonomy: null,
      owasp_agentic_tags: ["AS-02 Insecure Tool Execution"],
    },
  ],
  summary: "Potential remote execution flow discovered during scan.",
  files_scanned: 4,
  total_chars_analyzed: 1337,
  incomplete: false,
  ai_files_analyzed: 2,
  chunks_used: 3,
};

function createHookState() {
  return {
    phase: "done" as const,
    results: [sampleResult],
    activeSkills: [],
    currentSkill: null,
    currentMode: "smart" as const,
    currentStage: null,
    currentFile: null,
    syncPulseKey: 1,
    scanAngle: 0,
    scanStartedAt: null,
    recentFiles: [],
    scanned: 1,
    total: 1,
    skillFileProgress: {},
    skillChunkProgress: {},
    activeChunkFiles: [],
    activeChunkWorkers: 0,
    maxChunkWorkers: 0,
    errors: [],
    riskMap: { "audit-skill": "High" as const },
    progressPercent: 100,
    startScan: vi.fn(),
    resetScan: vi.fn(),
    loadCached: vi.fn(),
    clearCache: vi.fn(),
    cancelScan: vi.fn(),
  };
}

describe("SecurityScan page", () => {
  beforeEach(() => {
    vi.clearAllMocks();

    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn((key: string) => {
          if (key === "skillstar:scan-mode") return "smart";
          if (key === "skillstar:scan-incremental") return "1";
          return null;
        }),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });

    mockedUseSecurityScan.mockReturnValue(createHookState());
    mockedInvoke.mockImplementation(async (command) => {
      switch (command) {
        case "list_security_scan_logs":
          return [
            {
              file_name: "scan-20260423-100000-ui-smart.log",
              path: "/tmp/scan-20260423-100000-ui-smart.log",
              created_at: "2026-04-23T10:05:00.000Z",
              size_bytes: 1024,
            },
          ];
        case "get_security_scan_log_dir":
          return "/tmp/security-logs";
        case "get_security_scan_policy":
          return {
            preset: "balanced",
            severity_threshold: "low",
            enabled_analyzers: [],
            ignore_rules: [],
            rule_overrides: {},
          };
        case "estimate_security_scan":
          return {
            requestedMode: "smart",
            effectiveMode: "smart",
            totalSkills: 1,
            totalFiles: 4,
            aiEligibleFiles: 2,
            estimatedChunks: 3,
            estimatedApiCalls: 3,
            estimatedTotalChars: 1337,
            chunkCharLimit: 4000,
          };
        default:
          return undefined;
      }
    });
  });

  it("exports reports with current result names and refreshes latest path", async () => {
    mockedInvoke.mockImplementation(async (command, args) => {
      switch (command) {
        case "list_security_scan_logs":
          return [
            {
              file_name: "scan-20260423-100000-ui-smart.log",
              path: "/tmp/scan-20260423-100000-ui-smart.log",
              created_at: "2026-04-23T10:05:00.000Z",
              size_bytes: 1024,
            },
          ];
        case "get_security_scan_log_dir":
          return "/tmp/security-logs";
        case "get_security_scan_policy":
          return {
            preset: "balanced",
            severity_threshold: "low",
            enabled_analyzers: [],
            ignore_rules: [],
            rule_overrides: {},
          };
        case "estimate_security_scan":
          return {
            requestedMode: "smart",
            effectiveMode: "smart",
            totalSkills: 1,
            totalFiles: 4,
            aiEligibleFiles: 2,
            estimatedChunks: 3,
            estimatedApiCalls: 3,
            estimatedTotalChars: 1337,
            chunkCharLimit: 4000,
          };
        case "export_security_scan_report":
          expect(args).toEqual({
            format: "html",
            skillNames: ["audit-skill"],
            requestLabel: "ui-smart",
          });
          return "/tmp/reports/security-ui-smart.html";
        default:
          return undefined;
      }
    });

    render(<SecurityScan />);

    fireEvent.click(await screen.findByRole("button", { name: "Export HTML" }));

    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("export_security_scan_report", {
        format: "html",
        skillNames: ["audit-skill"],
        requestLabel: "ui-smart",
      });
    });

    expect(await screen.findByText("Last Report: /tmp/reports/security-ui-smart.html")).toBeInTheDocument();
  });

  it("opens the scan log folder from the report hint toolbar", async () => {
    render(<SecurityScan />);

    fireEvent.click(await screen.findByRole("button", { name: "Open Logs" }));

    await waitFor(() => {
      expect(mockedInvoke).toHaveBeenCalledWith("open_folder", { path: "/tmp/security-logs" });
    });
  });
});

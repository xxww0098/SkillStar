import { describe, expect, it } from "vitest";
import type { McpSyncResult } from "../../../types";
import {
  classifyMcpSyncResult,
  failedMcpSyncCount,
  isRetryableMcpOutcome,
  type McpSyncOutcome,
  mergeMcpSyncResults,
  summarizeMcpSyncResults,
} from "./syncResults";

function result(patch: Partial<McpSyncResult> = {}): McpSyncResult {
  return {
    toolId: "claude-code",
    serverId: "srv-1",
    success: true,
    skipped: false,
    rolledBack: false,
    ...patch,
  };
}

describe("failedMcpSyncCount", () => {
  it("ignores skipped targets, which are deliberate no-ops", () => {
    expect(
      failedMcpSyncCount([
        result(),
        result({ toolId: "kiro", success: false, skipped: true }),
        result({ toolId: "codex", success: false, error: "boom" }),
      ]),
    ).toBe(1);
  });
});

describe("classifyMcpSyncResult", () => {
  it("separates the four failure shapes that need different remedies", () => {
    expect(classifyMcpSyncResult(result())).toBe("success");
    expect(classifyMcpSyncResult(result({ success: false, skipped: true }))).toBe("skipped");
    expect(classifyMcpSyncResult(result({ success: false, rolledBack: true }))).toBe("rolledBack");
    expect(classifyMcpSyncResult(result({ success: false, rolledBack: false, error: "boom" }))).toBe("failed");
  });

  it("calls out a failed rollback separately from a clean one", () => {
    // A drifted config needs its backup restored by hand; retrying makes it worse.
    expect(
      classifyMcpSyncResult(result({ success: false, rolledBack: true, rollbackError: "permission denied" })),
    ).toBe("drifted");
  });

  it("marks every failure shape retryable and the settled ones not", () => {
    expect(isRetryableMcpOutcome("success")).toBe(false);
    expect(isRetryableMcpOutcome("skipped")).toBe(false);
    const failures: McpSyncOutcome[] = ["rolledBack", "failed", "drifted"];
    expect(failures.every(isRetryableMcpOutcome)).toBe(true);
  });
});

describe("summarizeMcpSyncResults", () => {
  it("reports a fully applied batch as consistent", () => {
    const report = summarizeMcpSyncResults([result(), result({ toolId: "kiro", success: false, skipped: true })]);

    expect(report.consistency).toEqual({
      consistent: true,
      applied: ["claude-code", "kiro"],
      rolledBack: [],
      drifted: [],
    });
    expect(report.failedCount).toBe(0);
    expect(report.retryableToolIds).toEqual([]);
  });

  it("partitions a partial failure the way the backend's McpSyncConsistency does", () => {
    const report = summarizeMcpSyncResults([
      result({ toolId: "claude-code" }),
      result({ toolId: "codex", success: false, rolledBack: true, error: "write failed" }),
      result({ toolId: "cursor", success: false, rolledBack: true, rollbackError: "restore failed" }),
      result({ toolId: "kiro", success: false, error: "parse error" }),
    ]);

    expect(report.consistency).toEqual({
      consistent: false,
      applied: ["claude-code"],
      rolledBack: ["codex"],
      drifted: ["cursor"],
    });
    expect(report.failedCount).toBe(3);
    expect(report.retryableToolIds).toEqual(["kiro", "codex", "cursor"]);
  });

  it("carries the recovery handles per row", () => {
    const report = summarizeMcpSyncResults([
      result({
        toolId: "codex",
        success: false,
        error: "write failed",
        configPath: "~/.codex/config.toml",
        backupPath: "~/.codex/config.toml.bak.1",
      }),
    ]);

    expect(report.rows[0]).toMatchObject({
      outcome: "failed",
      error: "write failed",
      configPath: "~/.codex/config.toml",
      backupPath: "~/.codex/config.toml.bak.1",
    });
  });
});

describe("mergeMcpSyncResults", () => {
  it("replaces a retried tool's row in place", () => {
    const before = [result({ toolId: "claude-code" }), result({ toolId: "codex", success: false, error: "boom" })];
    const merged = mergeMcpSyncResults(before, [result({ toolId: "codex" })]);

    expect(merged.map((r) => [r.toolId, r.success])).toEqual([
      ["claude-code", true],
      ["codex", true],
    ]);
    expect(summarizeMcpSyncResults(merged).consistency.consistent).toBe(true);
  });

  it("appends a tool the original batch did not report", () => {
    const merged = mergeMcpSyncResults([result({ toolId: "claude-code" })], [result({ toolId: "zed" })]);
    expect(merged.map((r) => r.toolId)).toEqual(["claude-code", "zed"]);
  });
});

import type { McpSyncConsistency, McpSyncResult } from "../../../types";

/** Count real projection failures; skipped targets are expected no-ops. */
export function failedMcpSyncCount(results: readonly McpSyncResult[]): number {
  return results.filter((result) => !result.success && !result.skipped).length;
}

/**
 * What happened to one tool in a sync batch.
 *
 * The five outcomes are not severity levels — they demand different remedies:
 * a `skipped` tool needs nothing, a `rolledBack` one can simply be retried, and
 * a `drifted` one needs its backup restored by hand because the undo itself
 * failed. Collapsing them into "N failed" (the previous toast) is what made
 * every one of those indistinguishable.
 */
export type McpSyncOutcome = "success" | "skipped" | "rolledBack" | "drifted" | "failed";

export function classifyMcpSyncResult(result: McpSyncResult): McpSyncOutcome {
  if (result.success) return "success";
  if (result.skipped) return "skipped";
  if (result.rollbackError) return "drifted";
  if (result.rolledBack) return "rolledBack";
  return "failed";
}

/** True for outcomes the user can fix by re-running the projection. */
export function isRetryableMcpOutcome(outcome: McpSyncOutcome): boolean {
  return outcome === "rolledBack" || outcome === "failed" || outcome === "drifted";
}

export interface McpSyncRow {
  toolId: string;
  serverId: string;
  outcome: McpSyncOutcome;
  configPath: string | null;
  backupPath: string | null;
  error: string | null;
  rollbackError: string | null;
}

export interface McpSyncReport {
  rows: McpSyncRow[];
  /**
   * The same partition `McpSyncConsistency` describes on the Rust side,
   * recomputed from the per-tool results the command actually returns.
   */
  consistency: McpSyncConsistency;
  failedCount: number;
  /** Tool ids worth offering a single-target retry for. */
  retryableToolIds: string[];
}

/**
 * Turn a batch of per-tool results into the detail panel's model.
 *
 * Per-tool writes are individually atomic but the batch is not transactional:
 * tool 3 failing does not un-write tools 1 and 2, because those writes are
 * correct and undoing them would take a working config away from an unrelated
 * tool. That is precisely why the applied/rolled-back/drifted split has to be
 * shown rather than summed — "3 of 5 tools now have this server, 1 was rolled
 * back, 1 may be half-written" is the honest sentence.
 */
export function summarizeMcpSyncResults(results: readonly McpSyncResult[]): McpSyncReport {
  const rows: McpSyncRow[] = results.map((result) => ({
    toolId: result.toolId,
    serverId: result.serverId,
    outcome: classifyMcpSyncResult(result),
    configPath: result.configPath ?? null,
    backupPath: result.backupPath ?? null,
    error: result.error ?? null,
    rollbackError: result.rollbackError ?? null,
  }));

  const applied = rows.filter((row) => row.outcome === "success" || row.outcome === "skipped").map((r) => r.toolId);
  const rolledBack = rows.filter((row) => row.outcome === "rolledBack").map((r) => r.toolId);
  const drifted = rows.filter((row) => row.outcome === "drifted").map((r) => r.toolId);
  const failed = rows.filter((row) => row.outcome === "failed").map((r) => r.toolId);

  return {
    rows,
    consistency: {
      consistent: rolledBack.length === 0 && drifted.length === 0 && failed.length === 0,
      applied,
      rolledBack,
      drifted,
    },
    failedCount: failed.length + rolledBack.length + drifted.length,
    retryableToolIds: [...new Set([...failed, ...rolledBack, ...drifted])],
  };
}

/** Merge a single-target retry's results over an existing report's rows. */
export function mergeMcpSyncResults(
  previous: readonly McpSyncResult[],
  retried: readonly McpSyncResult[],
): McpSyncResult[] {
  const byTool = new Map(retried.map((result) => [result.toolId, result]));
  const merged = previous.map((result) => byTool.get(result.toolId) ?? result);
  const seen = new Set(previous.map((result) => result.toolId));
  return [...merged, ...retried.filter((result) => !seen.has(result.toolId))];
}

import type { McpSyncResult } from "../../../types";

/** Count real projection failures; skipped targets are expected no-ops. */
export function failedMcpSyncCount(results: readonly McpSyncResult[]): number {
  return results.filter((result) => !result.success && !result.skipped).length;
}

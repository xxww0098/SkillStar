/**
 * Display formatting for model catalog metadata and sync timestamps.
 * Pure functions — extracted from the old ToolActivationPanel.
 */
import type { ModelCatalogEntry } from "../../../types";

export function formatModelOptionLabel(modelId: string, metadata?: ModelCatalogEntry): string {
  if (!metadata) return modelId;
  const name = metadata.display_name || modelId;
  const details = [
    metadata.context_length ? `${Math.round(metadata.context_length / 1000)}K ctx` : null,
    metadata.max_completion_tokens ? `${Math.round(metadata.max_completion_tokens / 1000)}K out` : null,
    formatCost(metadata.cost),
  ].filter(Boolean);
  return details.length > 0 ? `${name} (${modelId}) · ${details.join(" · ")}` : `${name} (${modelId})`;
}

export function formatModelMetadata(metadata: ModelCatalogEntry): string {
  const details = [
    metadata.context_length ? `上下文 ${metadata.context_length.toLocaleString()}` : null,
    metadata.max_completion_tokens ? `输出 ${metadata.max_completion_tokens.toLocaleString()}` : null,
    formatCost(metadata.cost),
  ].filter(Boolean);
  return details.length > 0 ? details.join(" · ") : metadata.id;
}

export function formatCost(cost: ModelCatalogEntry["cost"]): string | null {
  if (!cost || typeof cost !== "object") return null;
  const input = typeof cost.input === "number" ? cost.input : null;
  const output = typeof cost.output === "number" ? cost.output : null;
  if (input == null && output == null) return null;
  return `$${input ?? "?"}/$${output ?? "?"} / 1M`;
}

export function formatSyncTime(timestamp: string): string {
  try {
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return timestamp;
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return "刚刚";
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour} 小时前`;
    return date.toLocaleDateString("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return timestamp;
  }
}

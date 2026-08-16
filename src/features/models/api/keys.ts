/**
 * Query-key factory for the models feature. Every TanStack Query key in this
 * feature must come from here so invalidation stays consistent.
 */
export const modelsKeys = {
  all: ["models"] as const,
  providersFlat: () => [...modelsKeys.all, "providers-flat"] as const,
  presets: () => [...modelsKeys.all, "presets-flat"] as const,
  install: (toolId: string) => [...modelsKeys.all, "install", toolId] as const,
  agentDescriptors: () => [...modelsKeys.all, "agent-descriptors"] as const,
  /**
   * Roles the last write to `toolId` skipped. Not a fetch — the backend can
   * only compute this while writing, so the sync result is the only source and
   * the cache is where it is parked for the panels to read.
   */
  roleDrops: (toolId: string) => [...modelsKeys.all, "role-drops", toolId] as const,
};

/**
 * Query-key factory for the backend-owned AI config (`config/ai.json`).
 * Kept separate from `modelsKeys` (whose root is `["models"]`) because the
 * existing cache entries use the standalone `["ai-config"]` key — changing
 * the root here would silently invalidate a different cache bucket.
 */
export const aiConfigKeys = {
  all: ["ai-config"] as const,
};

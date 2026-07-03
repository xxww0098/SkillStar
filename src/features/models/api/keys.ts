/**
 * Query-key factory for the models feature. Every TanStack Query key in this
 * feature must come from here so invalidation stays consistent.
 */
export const modelsKeys = {
  all: ["models"] as const,
  providersFlat: () => [...modelsKeys.all, "providers-flat"] as const,
  presets: () => [...modelsKeys.all, "presets-flat"] as const,
  install: (toolId: string) => [...modelsKeys.all, "install", toolId] as const,
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

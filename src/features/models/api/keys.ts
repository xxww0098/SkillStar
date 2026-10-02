/**
 * Query-key factory for the models feature. Every TanStack Query key in this
 * feature must come from here so invalidation stays consistent.
 */
export const modelsKeys = {
  all: ["models"] as const,
  providersFlat: () => [...modelsKeys.all, "providers-flat"] as const,
  board: () => [...modelsKeys.all, "board"] as const,
  recentCalls: () => [...modelsKeys.all, "recent-calls"] as const,
  routingPage: (providerId: string) => [...modelsKeys.all, "routing-page", providerId] as const,
  savedGroups: () => [...modelsKeys.all, "saved-groups"] as const,
  modelEfforts: (id: string) => [...modelsKeys.all, "model-efforts", id] as const,
  profileNames: () => [...modelsKeys.all, "profile-names"] as const,
  listenMode: () => [...modelsKeys.all, "listen-mode"] as const,
  loopbackOrigin: () => [...modelsKeys.all, "loopback-origin"] as const,
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

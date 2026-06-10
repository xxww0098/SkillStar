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

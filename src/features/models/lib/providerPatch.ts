/**
 * Pure provider-form domain: meta-key constants, form values shape, and the
 * provider ⇄ form conversions (initial values, dirty check, patch building,
 * validation). Extracted from useProviderFormState so the behavior is locked
 * by unit tests independent of React.
 */
import type { ModelCatalogEntry, ProviderEntryFlat, ProviderPatchFlat } from "../../../types";

export type ModelFetchTarget = "claude" | "codex";
export type CodexWireApi = "chat" | "responses";
export type CodexAuthMode = "api_key" | "oauth" | "third_party";

export const CLAUDE_MODEL_META_KEYS = {
  main: "claude_main_model",
  haiku: "claude_haiku_model",
  sonnet: "claude_sonnet_model",
  opus: "claude_opus_model",
} as const;

export const CODEX_WIRE_API_META_KEY = "codex_wire_api" as const;
export const CODEX_AUTH_MODE_META_KEY = "codex_auth_mode" as const;
export const MODEL_CATALOG_META_KEY = "model_catalog" as const;

export const LATEST_CLAUDE_MODELS = {
  main: "claude-sonnet-4-6",
  haiku: "claude-haiku-4-5-20251001",
  sonnet: "claude-sonnet-4-6",
  opus: "claude-opus-4-7",
} as const;

export const LATEST_CODEX_MODELS = ["gpt-5.5", "gpt-5.4-mini", "gpt-5.3-codex-spark", "gpt-5.4"] as const;

export function getMetaString(meta: Record<string, unknown> | undefined, key: string): string {
  const value = meta?.[key];
  return typeof value === "string" ? value : "";
}

/** Trim, drop empties, dedupe — used for model id lists and URL lists alike. */
export function buildModelCatalog(values: string[]): string[] {
  const seen = new Set<string>();
  const catalog: string[] = [];
  for (const value of values) {
    const trimmed = value.trim();
    if (trimmed && !seen.has(trimmed)) {
      seen.add(trimmed);
      catalog.push(trimmed);
    }
  }
  return catalog;
}

export function getModelCatalogFromMeta(meta: Record<string, unknown> | undefined): ModelCatalogEntry[] {
  const value = meta?.[MODEL_CATALOG_META_KEY];
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is ModelCatalogEntry => {
    return Boolean(entry && typeof entry === "object" && typeof (entry as ModelCatalogEntry).id === "string");
  });
}

function isValidHttpUrl(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return true;
  try {
    const parsed = new URL(trimmed);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

export function validatePatch(patch: ProviderPatchFlat): string | null {
  if (!patch.name?.trim()) return "供应商名称不能为空";
  if (!isValidHttpUrl(patch.base_url_openai ?? "")) return "OpenAI Base URL 格式无效";
  if (!isValidHttpUrl(patch.base_url_anthropic ?? "")) return "Anthropic Base URL 格式无效";
  if (!isValidHttpUrl(patch.models_url ?? "")) return "获取模型 URL 格式无效";
  return null;
}

/** Everything the provider editor form edits, as one value object. */
export interface ProviderFormValues {
  name: string;
  baseUrlOpenai: string;
  baseUrlAnthropic: string;
  modelsUrl: string;
  notes: string;
  apiKey: string;
  models: string[];
  modelCatalog: ModelCatalogEntry[];
  defaultModel: string;
  claudeMainModel: string;
  claudeHaikuModel: string;
  claudeSonnetModel: string;
  claudeOpusModel: string;
  codexWireApi: CodexWireApi;
  codexAuthMode: CodexAuthMode;
  contextLength: number;
  maxTokens: number;
  timeout: number;
  retryCount: number;
  streaming: boolean;
}

export function providerCodexWireApi(provider: ProviderEntryFlat): CodexWireApi {
  return (
    (provider.codex_wire_api as CodexWireApi) ||
    (getMetaString(provider.meta, CODEX_WIRE_API_META_KEY) as CodexWireApi) ||
    "responses"
  );
}

export function providerCodexAuthMode(provider: ProviderEntryFlat): CodexAuthMode {
  return (
    (provider.codex_auth_mode as CodexAuthMode) ||
    (getMetaString(provider.meta, CODEX_AUTH_MODE_META_KEY) as CodexAuthMode) ||
    "api_key"
  );
}

/**
 * Infer the recommended Codex `wireApi` + `authMode` for a provider from its
 * OpenAI-compatible base URL. Mirrors the backend `recommended_codex_defaults`
 * rule verbatim (single source of truth lives in Rust; this is the TS twin):
 *
 * - `api.openai.com` → `responses` + `api_key` (OpenAI native Responses API).
 * - everything else  → `chat` + `third_party` (third-party OpenAI-compatible
 *   endpoints only implement `/v1/chat/completions`, and `third_party` routes
 *   the key through `env_key` so `auth.json` is never touched).
 *
 * Used by `CodexSettingsForm` to flag a sub-optimal existing config and offer a
 * one-click fix (e.g. a DeepSeek provider that was created before the default
 * inference shipped and still carries `responses` + `api_key`).
 */
export function recommendedCodexDefaults(baseUrlOpenai: string): { wireApi: CodexWireApi; authMode: CodexAuthMode } {
  if (baseUrlOpenai.includes("api.openai.com")) {
    return { wireApi: "responses", authMode: "api_key" };
  }
  return { wireApi: "chat", authMode: "third_party" };
}

/**
 * Derive the env var name Codex reads a third-party API key from. Mirrors the
 * backend `codex_env_key_for` rule: `SKILLSTAR_<UPPER_PREFIX>_KEY` where the
 * prefix is the first 8 chars of the provider id (non-alphanumeric → `_`).
 * Two providers never share a var, and the name is shell-safe.
 */
export function codexEnvKeyName(provider: ProviderEntryFlat): string {
  const rawPrefix = provider.id.slice(0, 8);
  let safe = "";
  for (const ch of rawPrefix) {
    safe += /[A-Za-z0-9]/.test(ch) ? ch.toUpperCase() : "_";
  }
  if (!safe) safe = "PROVIDER";
  return `SKILLSTAR_${safe}_KEY`;
}

/** Mask a key for display (e.g. `sk-abc…wxyz`). Returns "" if the key is empty. */
export function maskApiKey(key: string): string {
  const trimmed = key.trim();
  if (!trimmed) return "";
  if (trimmed.length <= 8) return `${trimmed.slice(0, 2)}…`;
  return `${trimmed.slice(0, 6)}…${trimmed.slice(-4)}`;
}

/** Initial form values mirroring the persisted provider. */
export function providerToFormValues(provider: ProviderEntryFlat): ProviderFormValues {
  const meta = provider.meta ?? {};
  return {
    name: provider.name,
    baseUrlOpenai: provider.base_url_openai,
    baseUrlAnthropic: provider.base_url_anthropic,
    modelsUrl: provider.models_url ?? "",
    notes: provider.notes ?? "",
    apiKey: provider.api_key,
    models: provider.models,
    modelCatalog: getModelCatalogFromMeta(meta),
    defaultModel: provider.default_model,
    claudeMainModel: getMetaString(meta, CLAUDE_MODEL_META_KEYS.main),
    claudeHaikuModel: getMetaString(meta, CLAUDE_MODEL_META_KEYS.haiku),
    claudeSonnetModel: getMetaString(meta, CLAUDE_MODEL_META_KEYS.sonnet),
    claudeOpusModel: getMetaString(meta, CLAUDE_MODEL_META_KEYS.opus),
    codexWireApi: providerCodexWireApi(provider),
    codexAuthMode: providerCodexAuthMode(provider),
    contextLength: (meta.context_length as number) ?? 128000,
    maxTokens: (meta.max_tokens as number) ?? 4096,
    timeout: (meta.timeout as number) ?? 30,
    retryCount: (meta.retry_count as number) ?? 3,
    streaming: (meta.streaming as boolean) ?? true,
  };
}

/** Model id list persisted as `models`: form models + every referenced model id. */
function collectModelIds(values: ProviderFormValues): string[] {
  return buildModelCatalog([
    ...values.models,
    values.defaultModel,
    values.claudeMainModel,
    values.claudeHaikuModel,
    values.claudeSonnetModel,
    values.claudeOpusModel,
  ]);
}

/** Build the update patch from form values, preserving unrelated meta keys. */
export function buildProviderPatch(
  values: ProviderFormValues,
  baseMeta: Record<string, unknown> | undefined,
): ProviderPatchFlat {
  return {
    name: values.name.trim(),
    base_url_openai: values.baseUrlOpenai.trim(),
    base_url_anthropic: values.baseUrlAnthropic.trim(),
    models_url: values.modelsUrl.trim(),
    api_key: values.apiKey,
    models: collectModelIds(values),
    default_model: values.defaultModel.trim(),
    notes: values.notes.trim() || undefined,
    codex_wire_api: values.codexWireApi,
    codex_auth_mode: values.codexAuthMode,
    meta: {
      ...(baseMeta ?? {}),
      context_length: values.contextLength,
      max_tokens: values.maxTokens,
      timeout: values.timeout,
      retry_count: values.retryCount,
      streaming: values.streaming,
      [MODEL_CATALOG_META_KEY]: values.modelCatalog,
      [CLAUDE_MODEL_META_KEYS.main]: values.claudeMainModel.trim(),
      [CLAUDE_MODEL_META_KEYS.haiku]: values.claudeHaikuModel.trim(),
      [CLAUDE_MODEL_META_KEYS.sonnet]: values.claudeSonnetModel.trim(),
      [CLAUDE_MODEL_META_KEYS.opus]: values.claudeOpusModel.trim(),
      [CODEX_WIRE_API_META_KEY]: values.codexWireApi,
      [CODEX_AUTH_MODE_META_KEY]: values.codexAuthMode,
    },
  };
}

/** Would saving `values` change anything on `provider`? */
export function computeDirty(provider: ProviderEntryFlat, values: ProviderFormValues): boolean {
  const persisted = providerToFormValues(provider);
  const modelIds = collectModelIds(values);
  const modelsChanged =
    modelIds.length !== provider.models.length || modelIds.some((model, index) => model !== provider.models[index]);
  const modelCatalogChanged = JSON.stringify(values.modelCatalog) !== JSON.stringify(persisted.modelCatalog);

  return (
    values.name !== persisted.name ||
    values.baseUrlOpenai !== persisted.baseUrlOpenai ||
    values.baseUrlAnthropic !== persisted.baseUrlAnthropic ||
    values.modelsUrl !== persisted.modelsUrl ||
    values.notes !== persisted.notes ||
    values.apiKey !== persisted.apiKey ||
    modelsChanged ||
    modelCatalogChanged ||
    values.defaultModel.trim() !== provider.default_model ||
    values.claudeMainModel !== persisted.claudeMainModel ||
    values.claudeHaikuModel !== persisted.claudeHaikuModel ||
    values.claudeSonnetModel !== persisted.claudeSonnetModel ||
    values.claudeOpusModel !== persisted.claudeOpusModel ||
    values.codexWireApi !== persisted.codexWireApi ||
    values.codexAuthMode !== persisted.codexAuthMode ||
    values.contextLength !== persisted.contextLength ||
    values.maxTokens !== persisted.maxTokens ||
    values.timeout !== persisted.timeout ||
    values.retryCount !== persisted.retryCount ||
    values.streaming !== persisted.streaming
  );
}

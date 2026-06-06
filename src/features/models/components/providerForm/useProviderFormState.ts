import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import type { ProviderEntryFlat, ProviderPatchFlat } from "../../../../types";
import { useModelFetch } from "../../hooks/useModelFetch";
import { useProviderPresets } from "../../hooks/useProviderPresets";

/**
 * Auto-save lifecycle state surfaced to the form host (drawer header / footer).
 *
 * - `idle`: form mirrors persisted state; nothing in flight.
 * - `dirty`: user has typed but the debounce hasn't fired yet.
 * - `saving`: a save is in flight.
 * - `saved`: most recent save succeeded.
 * - `error`: most recent save (or validation) failed.
 */
export type ProviderSaveState = "idle" | "dirty" | "saving" | "saved" | "error";

export type ModelFetchTarget = "claude" | "codex";
export type CodexWireApi = "chat" | "responses";

export const CLAUDE_MODEL_META_KEYS = {
  main: "claude_main_model",
  haiku: "claude_haiku_model",
  sonnet: "claude_sonnet_model",
  opus: "claude_opus_model",
} as const;

export const CODEX_WIRE_API_META_KEY = "codex_wire_api" as const;
export const CODEX_AUTH_MODE_META_KEY = "codex_auth_mode" as const;

export const LATEST_CLAUDE_MODELS = {
  main: "claude-sonnet-4-6",
  haiku: "claude-haiku-4-5-20251001",
  sonnet: "claude-sonnet-4-6",
  opus: "claude-opus-4-7",
} as const;

export const LATEST_CODEX_MODELS = [
  "codex-mini-latest",
  "gpt-4.1",
  "gpt-4.1-mini",
  "gpt-4.1-nano",
  "gpt-4o",
  "gpt-4o-mini",
  "o3",
  "o4-mini",
] as const;

const AUTO_SAVE_DEBOUNCE_MS = 600;

export function getMetaString(meta: Record<string, unknown> | undefined, key: string): string {
  const value = meta?.[key];
  return typeof value === "string" ? value : "";
}

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

export interface UseProviderFormStateOptions {
  provider: ProviderEntryFlat;
  onSave: (patch: ProviderPatchFlat) => Promise<void>;
  onSaveStateChange?: (state: ProviderSaveState) => void;
}

export function useProviderFormState({ provider, onSave, onSaveStateChange }: UseProviderFormStateOptions) {
  const { presets } = useProviderPresets();
  const preset = presets.find((p) => p.id === provider.preset_id);

  const [name, setName] = useState(provider.name);
  const [baseUrlOpenai, setBaseUrlOpenai] = useState(provider.base_url_openai);
  const [baseUrlAnthropic, setBaseUrlAnthropic] = useState(provider.base_url_anthropic);
  const [modelsUrl, setModelsUrl] = useState(provider.models_url ?? "");
  const [notes, setNotes] = useState(provider.notes ?? "");
  const [apiKey, setApiKey] = useState(provider.api_key);
  const [models, setModels] = useState<string[]>(provider.models);
  const [defaultModel, setDefaultModel] = useState(provider.default_model);
  const [claudeMainModel, setClaudeMainModel] = useState(getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main));
  const [claudeHaikuModel, setClaudeHaikuModel] = useState(getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.haiku));
  const [claudeSonnetModel, setClaudeSonnetModel] = useState(
    getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.sonnet),
  );
  const [claudeOpusModel, setClaudeOpusModel] = useState(getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.opus));
  const [codexWireApi, setCodexWireApi] = useState<CodexWireApi>(
    (getMetaString(provider.meta, CODEX_WIRE_API_META_KEY) as CodexWireApi) || "chat",
  );
  const [codexAuthMode, setCodexAuthMode] = useState<"api_key" | "oauth">(
    (provider.codex_auth_mode as "api_key" | "oauth") ||
      (getMetaString(provider.meta, CODEX_AUTH_MODE_META_KEY) as "api_key" | "oauth") ||
      "api_key",
  );
  const [contextLength, setContextLength] = useState<number>((provider.meta?.context_length as number) ?? 128000);
  const [maxTokens, setMaxTokens] = useState<number>((provider.meta?.max_tokens as number) ?? 4096);
  const [timeout, setTimeout_] = useState<number>((provider.meta?.timeout as number) ?? 30);
  const [retryCount, setRetryCount] = useState<number>((provider.meta?.retry_count as number) ?? 3);
  const [streaming, setStreaming] = useState<boolean>((provider.meta?.streaming as boolean) ?? true);

  const [showApiKey, setShowApiKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [showAnthropicUrl, setShowAnthropicUrl] = useState(Boolean(provider.base_url_anthropic?.trim()));

  const { fetchModels, isLoading: isFetchingModels, error: fetchError } = useModelFetch();
  const [claudeModelOptions, setClaudeModelOptions] = useState<string[]>(() =>
    buildModelCatalog([
      LATEST_CLAUDE_MODELS.main,
      LATEST_CLAUDE_MODELS.haiku,
      LATEST_CLAUDE_MODELS.sonnet,
      LATEST_CLAUDE_MODELS.opus,
      ...provider.models,
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main),
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.haiku),
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.sonnet),
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.opus),
    ]),
  );
  const [codexModelOptions, setCodexModelOptions] = useState<string[]>(() =>
    buildModelCatalog([...provider.models, provider.default_model]),
  );
  const [modelFetchStatus, setModelFetchStatus] = useState<{ target: ModelFetchTarget; count: number } | null>(null);

  useEffect(() => {
    const providerClaudeMainModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main);
    const providerClaudeHaikuModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.haiku);
    const providerClaudeSonnetModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.sonnet);
    const providerClaudeOpusModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.opus);

    setName(provider.name);
    setBaseUrlOpenai(provider.base_url_openai);
    setBaseUrlAnthropic(provider.base_url_anthropic);
    setModelsUrl(provider.models_url ?? "");
    setNotes(provider.notes ?? "");
    setApiKey(provider.api_key);
    setModels(provider.models);
    setDefaultModel(provider.default_model);
    setClaudeMainModel(providerClaudeMainModel);
    setClaudeHaikuModel(providerClaudeHaikuModel);
    setClaudeSonnetModel(providerClaudeSonnetModel);
    setClaudeOpusModel(providerClaudeOpusModel);
    setCodexWireApi((getMetaString(provider.meta, CODEX_WIRE_API_META_KEY) as CodexWireApi) || "chat");
    setCodexAuthMode(
      (provider.codex_auth_mode as "api_key" | "oauth") ||
        (getMetaString(provider.meta, CODEX_AUTH_MODE_META_KEY) as "api_key" | "oauth") ||
        "api_key",
    );
    setContextLength((provider.meta?.context_length as number) ?? 128000);
    setMaxTokens((provider.meta?.max_tokens as number) ?? 4096);
    setTimeout_((provider.meta?.timeout as number) ?? 30);
    setRetryCount((provider.meta?.retry_count as number) ?? 3);
    setStreaming((provider.meta?.streaming as boolean) ?? true);
    setShowAnthropicUrl(Boolean(provider.base_url_anthropic?.trim()));
    setClaudeModelOptions(
      buildModelCatalog([
        LATEST_CLAUDE_MODELS.main,
        LATEST_CLAUDE_MODELS.haiku,
        LATEST_CLAUDE_MODELS.sonnet,
        LATEST_CLAUDE_MODELS.opus,
        ...provider.models,
        providerClaudeMainModel,
        providerClaudeHaikuModel,
        providerClaudeSonnetModel,
        providerClaudeOpusModel,
      ]),
    );
    setCodexModelOptions(buildModelCatalog([...provider.models, provider.default_model]));
    setModelFetchStatus(null);
  }, [provider]);

  const handleFetchModels = useCallback(async () => {
    const url = modelsUrl.trim();
    if (!url || !apiKey.trim()) return;
    try {
      const result = await fetchModels(url, apiKey.trim());
      const fetchedCatalog = buildModelCatalog(result);
      setModels((prev) => buildModelCatalog([...prev, ...fetchedCatalog]));
      setClaudeModelOptions((prev) => buildModelCatalog([...fetchedCatalog, ...prev]));
      setCodexModelOptions((prev) => buildModelCatalog([...fetchedCatalog, ...prev]));
      setModelFetchStatus({ target: "claude", count: fetchedCatalog.length });
    } catch {
      setModelFetchStatus(null);
    }
  }, [fetchModels, modelsUrl, apiKey]);

  const buildPatch = useCallback((): ProviderPatchFlat => {
    const modelCatalog = buildModelCatalog([
      ...models,
      defaultModel,
      claudeMainModel,
      claudeHaikuModel,
      claudeSonnetModel,
      claudeOpusModel,
    ]);

    return {
      name: name.trim(),
      base_url_openai: baseUrlOpenai.trim(),
      base_url_anthropic: baseUrlAnthropic.trim(),
      models_url: modelsUrl.trim(),
      api_key: apiKey,
      models: modelCatalog,
      default_model: defaultModel.trim(),
      notes: notes.trim() || undefined,
      codex_wire_api: codexWireApi,
      codex_auth_mode: codexAuthMode,
      meta: {
        ...(provider.meta ?? {}),
        context_length: contextLength,
        max_tokens: maxTokens,
        timeout: timeout,
        retry_count: retryCount,
        streaming,
        [CLAUDE_MODEL_META_KEYS.main]: claudeMainModel.trim(),
        [CLAUDE_MODEL_META_KEYS.haiku]: claudeHaikuModel.trim(),
        [CLAUDE_MODEL_META_KEYS.sonnet]: claudeSonnetModel.trim(),
        [CLAUDE_MODEL_META_KEYS.opus]: claudeOpusModel.trim(),
        [CODEX_WIRE_API_META_KEY]: codexWireApi,
        [CODEX_AUTH_MODE_META_KEY]: codexAuthMode,
      },
    };
  }, [
    name,
    baseUrlOpenai,
    baseUrlAnthropic,
    modelsUrl,
    apiKey,
    models,
    defaultModel,
    claudeMainModel,
    claudeHaikuModel,
    claudeSonnetModel,
    claudeOpusModel,
    codexWireApi,
    codexAuthMode,
    notes,
    contextLength,
    maxTokens,
    timeout,
    retryCount,
    streaming,
    provider.meta,
  ]);

  const persistChanges = useCallback(async () => {
    const patch = buildPatch();
    const validationError = validatePatch(patch);
    if (validationError) {
      onSaveStateChange?.("error");
      toast.error(validationError);
      return;
    }

    setSaving(true);
    onSaveStateChange?.("saving");
    try {
      await onSave(patch);
      onSaveStateChange?.("saved");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      onSaveStateChange?.("error");
      toast.error(`保存失败：${message}`);
    } finally {
      setSaving(false);
    }
  }, [buildPatch, onSave, onSaveStateChange]);

  const hasUnsavedChanges = useMemo(() => {
    const providerMeta = provider.meta ?? {};
    const providerContextLength = (providerMeta.context_length as number) ?? 128000;
    const providerMaxTokens = (providerMeta.max_tokens as number) ?? 4096;
    const providerTimeout = (providerMeta.timeout as number) ?? 30;
    const providerRetryCount = (providerMeta.retry_count as number) ?? 3;
    const providerStreaming = (providerMeta.streaming as boolean) ?? true;
    const providerClaudeMainModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.main);
    const providerClaudeHaikuModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.haiku);
    const providerClaudeSonnetModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.sonnet);
    const providerClaudeOpusModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.opus);
    const providerCodexWireApi = (getMetaString(providerMeta, CODEX_WIRE_API_META_KEY) as CodexWireApi) || "chat";
    const providerCodexAuthMode =
      (provider.codex_auth_mode as "api_key" | "oauth") ||
      (getMetaString(providerMeta, CODEX_AUTH_MODE_META_KEY) as "api_key" | "oauth") ||
      "api_key";
    const modelCatalog = buildModelCatalog([
      ...models,
      defaultModel,
      claudeMainModel,
      claudeHaikuModel,
      claudeSonnetModel,
      claudeOpusModel,
    ]);
    const modelsChanged =
      modelCatalog.length !== provider.models.length ||
      modelCatalog.some((model, index) => model !== provider.models[index]);

    return (
      name !== provider.name ||
      baseUrlOpenai !== provider.base_url_openai ||
      baseUrlAnthropic !== provider.base_url_anthropic ||
      modelsUrl !== (provider.models_url ?? "") ||
      notes !== (provider.notes ?? "") ||
      apiKey !== provider.api_key ||
      modelsChanged ||
      defaultModel.trim() !== provider.default_model ||
      claudeMainModel !== providerClaudeMainModel ||
      claudeHaikuModel !== providerClaudeHaikuModel ||
      claudeSonnetModel !== providerClaudeSonnetModel ||
      claudeOpusModel !== providerClaudeOpusModel ||
      codexWireApi !== providerCodexWireApi ||
      codexAuthMode !== providerCodexAuthMode ||
      contextLength !== providerContextLength ||
      maxTokens !== providerMaxTokens ||
      timeout !== providerTimeout ||
      retryCount !== providerRetryCount ||
      streaming !== providerStreaming
    );
  }, [
    name,
    baseUrlOpenai,
    baseUrlAnthropic,
    modelsUrl,
    notes,
    apiKey,
    models,
    defaultModel,
    claudeMainModel,
    claudeHaikuModel,
    claudeSonnetModel,
    claudeOpusModel,
    codexWireApi,
    codexAuthMode,
    contextLength,
    maxTokens,
    timeout,
    retryCount,
    streaming,
    provider,
  ]);

  useEffect(() => {
    if (saving || !hasUnsavedChanges) return;
    onSaveStateChange?.("dirty");
  }, [hasUnsavedChanges, saving, onSaveStateChange]);

  useEffect(() => {
    if (!hasUnsavedChanges || saving) return;
    const timer = window.setTimeout(() => void persistChanges(), AUTO_SAVE_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [hasUnsavedChanges, saving, persistChanges, buildPatch]);

  const speedTestUrls = useMemo(() => {
    const urls = [...(preset?.endpoint_candidates ?? []), baseUrlOpenai, baseUrlAnthropic, modelsUrl];
    return buildModelCatalog(urls);
  }, [preset?.endpoint_candidates, baseUrlOpenai, baseUrlAnthropic, modelsUrl]);

  const handleApplyFastestEndpoint = useCallback((url: string, field: "openai" | "anthropic" | "models") => {
    const normalized = url.trim();
    if (!normalized) return;
    if (field === "openai") setBaseUrlOpenai(normalized);
    else if (field === "anthropic") setBaseUrlAnthropic(normalized);
    else setModelsUrl(normalized);
    toast.success("已应用最快端点");
  }, []);

  const agentSummary = useMemo(() => {
    const active: string[] = [];
    if (claudeMainModel.trim() || claudeSonnetModel.trim()) active.push("Claude");
    if (defaultModel.trim()) active.push("Codex");
    return active.length > 0 ? active.join(" · ") : "模型映射与磁盘配置";
  }, [claudeMainModel, claudeSonnetModel, defaultModel]);

  const advancedSummary = useMemo(() => {
    return `${contextLength.toLocaleString()} ctx · ${timeout}s · ${streaming ? "流式" : "非流式"}`;
  }, [contextLength, timeout, streaming]);

  return {
    preset,
    name,
    setName,
    baseUrlOpenai,
    setBaseUrlOpenai,
    baseUrlAnthropic,
    setBaseUrlAnthropic,
    modelsUrl,
    setModelsUrl,
    notes,
    setNotes,
    apiKey,
    setApiKey,
    defaultModel,
    setDefaultModel,
    claudeMainModel,
    setClaudeMainModel,
    claudeHaikuModel,
    setClaudeHaikuModel,
    claudeSonnetModel,
    setClaudeSonnetModel,
    claudeOpusModel,
    setClaudeOpusModel,
    codexWireApi,
    setCodexWireApi,
    codexAuthMode,
    setCodexAuthMode,
    contextLength,
    setContextLength,
    maxTokens,
    setMaxTokens,
    timeout,
    setTimeout_,
    retryCount,
    setRetryCount,
    streaming,
    setStreaming,
    showApiKey,
    setShowApiKey,
    showAnthropicUrl,
    setShowAnthropicUrl,
    saving,
    isFetchingModels,
    fetchError,
    handleFetchModels,
    modelFetchStatus,
    claudeModelOptions,
    codexModelOptions,
    speedTestUrls,
    handleApplyFastestEndpoint,
    agentSummary,
    advancedSummary,
  };
}

export type ProviderFormState = ReturnType<typeof useProviderFormState>;

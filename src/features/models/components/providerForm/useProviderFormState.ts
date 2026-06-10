import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import type { ModelCatalogEntry, ProviderEntryFlat, ProviderPatchFlat } from "../../../../types";
import { useModelFetch } from "../../api/modelCatalog";
import { useProviderPresets } from "../../api/presets";
import {
  buildModelCatalog,
  buildProviderPatch,
  type CodexAuthMode,
  type CodexWireApi,
  computeDirty,
  LATEST_CLAUDE_MODELS,
  type ModelFetchTarget,
  type ProviderFormValues,
  providerToFormValues,
  validatePatch,
} from "../../lib/providerPatch";

// Re-exports kept for existing import sites (settings picker, drawer form, hub).
export {
  buildModelCatalog,
  CLAUDE_MODEL_META_KEYS,
  CODEX_AUTH_MODE_META_KEY,
  CODEX_WIRE_API_META_KEY,
  getMetaString,
  getModelCatalogFromMeta,
  LATEST_CLAUDE_MODELS,
  LATEST_CODEX_MODELS,
  MODEL_CATALOG_META_KEY,
  validatePatch,
} from "../../lib/providerPatch";
export type { CodexWireApi, ModelFetchTarget } from "../../lib/providerPatch";

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

const AUTO_SAVE_DEBOUNCE_MS = 600;

export interface UseProviderFormStateOptions {
  provider: ProviderEntryFlat;
  onSave: (patch: ProviderPatchFlat) => Promise<void>;
  onSaveStateChange?: (state: ProviderSaveState) => void;
}

export function useProviderFormState({ provider, onSave, onSaveStateChange }: UseProviderFormStateOptions) {
  const { presets } = useProviderPresets();
  const preset = presets.find((p) => p.id === provider.preset_id);
  const initial = useMemo(() => providerToFormValues(provider), [provider]);

  const [name, setName] = useState(initial.name);
  const [baseUrlOpenai, setBaseUrlOpenai] = useState(initial.baseUrlOpenai);
  const [baseUrlAnthropic, setBaseUrlAnthropic] = useState(initial.baseUrlAnthropic);
  const [modelsUrl, setModelsUrl] = useState(initial.modelsUrl);
  const [notes, setNotes] = useState(initial.notes);
  const [apiKey, setApiKey] = useState(initial.apiKey);
  const [models, setModels] = useState<string[]>(initial.models);
  const [modelCatalog, setModelCatalog] = useState<ModelCatalogEntry[]>(initial.modelCatalog);
  const [defaultModel, setDefaultModel] = useState(initial.defaultModel);
  const [claudeMainModel, setClaudeMainModel] = useState(initial.claudeMainModel);
  const [claudeHaikuModel, setClaudeHaikuModel] = useState(initial.claudeHaikuModel);
  const [claudeSonnetModel, setClaudeSonnetModel] = useState(initial.claudeSonnetModel);
  const [claudeOpusModel, setClaudeOpusModel] = useState(initial.claudeOpusModel);
  const [codexWireApi, setCodexWireApi] = useState<CodexWireApi>(initial.codexWireApi);
  const [codexAuthMode, setCodexAuthMode] = useState<CodexAuthMode>(initial.codexAuthMode);
  const [contextLength, setContextLength] = useState<number>(initial.contextLength);
  const [maxTokens, setMaxTokens] = useState<number>(initial.maxTokens);
  const [timeout, setTimeout_] = useState<number>(initial.timeout);
  const [retryCount, setRetryCount] = useState<number>(initial.retryCount);
  const [streaming, setStreaming] = useState<boolean>(initial.streaming);

  const [showApiKey, setShowApiKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [showAnthropicUrl, setShowAnthropicUrl] = useState(Boolean(initial.baseUrlAnthropic.trim()));

  const { fetchModelCatalog, isLoading: isFetchingModels, error: fetchError } = useModelFetch();
  const [claudeModelOptions, setClaudeModelOptions] = useState<string[]>(() =>
    buildModelCatalog([
      LATEST_CLAUDE_MODELS.main,
      LATEST_CLAUDE_MODELS.haiku,
      LATEST_CLAUDE_MODELS.sonnet,
      LATEST_CLAUDE_MODELS.opus,
      ...initial.models,
      initial.claudeMainModel,
      initial.claudeHaikuModel,
      initial.claudeSonnetModel,
      initial.claudeOpusModel,
    ]),
  );
  const [codexModelOptions, setCodexModelOptions] = useState<string[]>(() =>
    buildModelCatalog([...initial.models, initial.defaultModel]),
  );
  const [modelFetchStatus, setModelFetchStatus] = useState<{ target: ModelFetchTarget; count: number } | null>(null);

  // Reset every field when the persisted provider changes identity or content.
  useEffect(() => {
    const next = providerToFormValues(provider);
    setName(next.name);
    setBaseUrlOpenai(next.baseUrlOpenai);
    setBaseUrlAnthropic(next.baseUrlAnthropic);
    setModelsUrl(next.modelsUrl);
    setNotes(next.notes);
    setApiKey(next.apiKey);
    setModels(next.models);
    setModelCatalog(next.modelCatalog);
    setDefaultModel(next.defaultModel);
    setClaudeMainModel(next.claudeMainModel);
    setClaudeHaikuModel(next.claudeHaikuModel);
    setClaudeSonnetModel(next.claudeSonnetModel);
    setClaudeOpusModel(next.claudeOpusModel);
    setCodexWireApi(next.codexWireApi);
    setCodexAuthMode(next.codexAuthMode);
    setContextLength(next.contextLength);
    setMaxTokens(next.maxTokens);
    setTimeout_(next.timeout);
    setRetryCount(next.retryCount);
    setStreaming(next.streaming);
    setShowAnthropicUrl(Boolean(next.baseUrlAnthropic.trim()));
    setClaudeModelOptions(
      buildModelCatalog([
        LATEST_CLAUDE_MODELS.main,
        LATEST_CLAUDE_MODELS.haiku,
        LATEST_CLAUDE_MODELS.sonnet,
        LATEST_CLAUDE_MODELS.opus,
        ...next.models,
        next.claudeMainModel,
        next.claudeHaikuModel,
        next.claudeSonnetModel,
        next.claudeOpusModel,
      ]),
    );
    setCodexModelOptions(buildModelCatalog([...next.models, next.defaultModel]));
    setModelFetchStatus(null);
  }, [provider]);

  const values: ProviderFormValues = useMemo(
    () => ({
      name,
      baseUrlOpenai,
      baseUrlAnthropic,
      modelsUrl,
      notes,
      apiKey,
      models,
      modelCatalog,
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
    }),
    [
      name,
      baseUrlOpenai,
      baseUrlAnthropic,
      modelsUrl,
      notes,
      apiKey,
      models,
      modelCatalog,
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
    ],
  );

  const handleFetchModels = useCallback(async () => {
    const url = modelsUrl.trim();
    if (!url || !apiKey.trim()) return;
    try {
      const result = await fetchModelCatalog(url, apiKey.trim());
      const fetchedCatalog = buildModelCatalog(result.models);
      setModels((prev) => buildModelCatalog([...prev, ...fetchedCatalog]));
      setModelCatalog(result.catalog);
      setClaudeModelOptions((prev) => buildModelCatalog([...fetchedCatalog, ...prev]));
      setCodexModelOptions((prev) => buildModelCatalog([...fetchedCatalog, ...prev]));
      setModelFetchStatus({ target: "claude", count: fetchedCatalog.length });
    } catch {
      setModelFetchStatus(null);
    }
  }, [fetchModelCatalog, modelsUrl, apiKey]);

  const persistChanges = useCallback(async () => {
    const patch = buildProviderPatch(values, provider.meta);
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
  }, [values, provider.meta, onSave, onSaveStateChange]);

  const hasUnsavedChanges = useMemo(() => computeDirty(provider, values), [provider, values]);

  useEffect(() => {
    if (saving || !hasUnsavedChanges) return;
    onSaveStateChange?.("dirty");
  }, [hasUnsavedChanges, saving, onSaveStateChange]);

  useEffect(() => {
    if (!hasUnsavedChanges || saving) return;
    const timer = window.setTimeout(() => void persistChanges(), AUTO_SAVE_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [hasUnsavedChanges, saving, persistChanges]);

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
    models,
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
    modelCatalog,
    claudeModelOptions,
    codexModelOptions,
    speedTestUrls,
    handleApplyFastestEndpoint,
    agentSummary,
    advancedSummary,
  };
}

export type ProviderFormState = ReturnType<typeof useProviderFormState>;

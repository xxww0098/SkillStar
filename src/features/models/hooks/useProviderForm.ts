/**
 * Provider editor form state: ONE values object managed by a reducer
 * (replacing the 21 useState fields of the old useProviderFormState), plus
 * the derived data the tabs need. Persisting goes through the api layer's
 * update mutation; debounce/state-machine concerns live in useAutosave.
 *
 * The form intentionally does NOT reset when the cached provider object is
 * refreshed with the same id (the drawer remounts on id change via
 * `key={provider.id}`). The old implementation reset every field on each
 * cache refresh, which could clobber keystrokes typed while an autosave was
 * in flight.
 */
import { useCallback, useMemo, useReducer, useState } from "react";
import { toast } from "sonner";
import type { ProviderEntryFlat } from "../../../types";
import { useModelFetch } from "../api/modelCatalog";
import { useProviderPresets } from "../api/presets";
import { useProviderMutations } from "../api/providers";
import {
  buildModelCatalog,
  buildProviderPatch,
  computeDirty,
  LATEST_CLAUDE_MODELS,
  type ProviderFormValues,
  providerToFormValues,
  validatePatch,
} from "../lib/providerPatch";
import type { SaveAttemptResult } from "../types";

type FormAction =
  | { type: "set"; key: keyof ProviderFormValues; value: ProviderFormValues[keyof ProviderFormValues] }
  | { type: "merge"; patch: Partial<ProviderFormValues> };

function reducer(values: ProviderFormValues, action: FormAction): ProviderFormValues {
  switch (action.type) {
    case "set":
      return { ...values, [action.key]: action.value };
    case "merge":
      return { ...values, ...action.patch };
  }
}

export function useProviderForm(provider: ProviderEntryFlat) {
  const [values, dispatch] = useReducer(reducer, provider, providerToFormValues);
  const { presets } = useProviderPresets();
  const { updateProvider } = useProviderMutations();
  const { fetchModelCatalog, isLoading: isFetchingModels, error: fetchError } = useModelFetch();
  const [modelFetchCount, setModelFetchCount] = useState<number | null>(null);

  const preset = presets.find((p) => p.id === provider.preset_id);

  const setField = useCallback(<K extends keyof ProviderFormValues>(key: K, value: ProviderFormValues[K]) => {
    dispatch({ type: "set", key, value });
  }, []);

  const dirty = useMemo(() => computeDirty(provider, values), [provider, values]);

  /** One save attempt for useAutosave — validates, then persists via the api layer. */
  const save = useCallback(async (): Promise<SaveAttemptResult> => {
    const patch = buildProviderPatch(values, provider.meta);
    const validationError = validatePatch(patch);
    if (validationError) {
      toast.error(validationError);
      return "validation";
    }
    try {
      await updateProvider(provider.id, patch);
      return "saved";
    } catch (error) {
      toast.error(`保存失败：${error instanceof Error ? error.message : String(error)}`);
      return "error";
    }
  }, [values, provider.id, provider.meta, updateProvider]);

  /** Fetch the provider's /models catalog and merge it into the form. */
  const handleFetchModels = useCallback(async () => {
    const url = values.modelsUrl.trim();
    if (!url || !values.apiKey.trim()) return;
    try {
      const result = await fetchModelCatalog(url, values.apiKey.trim());
      const fetched = buildModelCatalog(result.models);
      dispatch({
        type: "merge",
        patch: {
          models: buildModelCatalog([...values.models, ...fetched]),
          modelCatalog: result.catalog,
        },
      });
      setModelFetchCount(fetched.length);
      toast.success(`已拉取 ${fetched.length} 个模型`);
      if (result.missing_cost_count > 0) {
        toast.message(`${result.missing_cost_count} 个模型缺少价格信息`);
      }
    } catch (error) {
      toast.error(`拉取模型失败：${error instanceof Error ? error.message : String(error)}`);
      setModelFetchCount(null);
    }
  }, [fetchModelCatalog, values.modelsUrl, values.apiKey, values.models]);

  const claudeModelOptions = useMemo(
    () =>
      buildModelCatalog([
        LATEST_CLAUDE_MODELS.main,
        LATEST_CLAUDE_MODELS.haiku,
        LATEST_CLAUDE_MODELS.sonnet,
        LATEST_CLAUDE_MODELS.opus,
        ...values.models,
        values.claudeMainModel,
        values.claudeHaikuModel,
        values.claudeSonnetModel,
        values.claudeOpusModel,
      ]),
    [values.models, values.claudeMainModel, values.claudeHaikuModel, values.claudeSonnetModel, values.claudeOpusModel],
  );

  const codexModelOptions = useMemo(
    () => buildModelCatalog([...values.models, values.defaultModel]),
    [values.models, values.defaultModel],
  );

  const speedTestUrls = useMemo(
    () =>
      buildModelCatalog([
        ...(preset?.endpoint_candidates ?? []),
        values.baseUrlOpenai,
        values.baseUrlAnthropic,
        values.modelsUrl,
      ]),
    [preset?.endpoint_candidates, values.baseUrlOpenai, values.baseUrlAnthropic, values.modelsUrl],
  );

  const handleApplyFastestEndpoint = useCallback(
    (url: string, field: "openai" | "anthropic" | "models") => {
      const normalized = url.trim();
      if (!normalized) return;
      if (field === "openai") setField("baseUrlOpenai", normalized);
      else if (field === "anthropic") setField("baseUrlAnthropic", normalized);
      else setField("modelsUrl", normalized);
      toast.success("已应用最快端点");
    },
    [setField],
  );

  return {
    values,
    setField,
    dirty,
    save,
    preset,
    isFetchingModels,
    fetchError,
    modelFetchCount,
    handleFetchModels,
    claudeModelOptions,
    codexModelOptions,
    speedTestUrls,
    handleApplyFastestEndpoint,
  };
}

export type ProviderForm = ReturnType<typeof useProviderForm>;

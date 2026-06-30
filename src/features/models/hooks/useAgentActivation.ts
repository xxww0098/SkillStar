/**
 * The single activation flow for one agent. Consumed by the agent cards and
 * the settings dialog — replaces the three divergent paths that previously
 * lived in ModelsHub, ToolActivationPanel and AgentHeroCard.
 *
 * Data comes from the providers-flat cache; mutations (and their toasts) from
 * the api layer. The agent's binding is exposed both as the active entry (for
 * the single-provider surface) and the full entry list (for the multi-provider
 * surface). `busy` covers the in-flight window for syncing status.
 */
import { useCallback, useMemo, useState } from "react";
import { useActivationMutations } from "../api/activations";
import { useToolInstallStatuses } from "../api/install";
import { useProvidersQuery } from "../api/providers";
import { type AgentDescriptor, getAgent, type ProviderToolId, providerCompatibleWithAgent } from "../lib/agentRegistry";
import { activeEntry as bindingActiveEntry, EMPTY_BINDING } from "../lib/toolBinding";

export function useAgentActivation(toolId: ProviderToolId) {
  const agent = getAgent(toolId) as AgentDescriptor;
  const { data } = useProvidersQuery();
  const { activateTool, deactivateTool, updateToolSettings, setActiveBinding, removeBindingEntry } =
    useActivationMutations();
  const toolIds = useMemo(() => [toolId], [toolId]);
  const { byTool: installByTool, isLoading: installLoading } = useToolInstallStatuses(toolIds);
  const [busy, setBusy] = useState(false);

  const providers = useMemo(() => {
    if (!data) return [];
    return [...data.providers].sort((a, b) => a.sort_index - b.sort_index);
  }, [data]);

  const isMulti = agent.kind === "multi";
  const binding = data?.tool_activations?.[toolId] ?? EMPTY_BINDING;
  const activeEntry = useMemo(() => bindingActiveEntry(binding), [binding]);

  /** All bound entries paired with their resolved provider (skips orphans). */
  const entries = useMemo(
    () =>
      binding.entries
        .map((entry) => ({
          entry,
          provider: providers.find((p) => p.id === entry.provider_id) ?? null,
        }))
        .filter((e): e is { entry: typeof e.entry; provider: NonNullable<typeof e.provider> } => e.provider !== null),
    [binding, providers],
  );

  const boundProvider = useMemo(() => {
    if (!activeEntry?.provider_id) return null;
    return providers.find((p) => p.id === activeEntry.provider_id) ?? null;
  }, [activeEntry, providers]);

  const compatibleProviders = useMemo(
    () => providers.filter((p) => providerCompatibleWithAgent(agent, p)),
    [providers, agent],
  );

  const currentModel = activeEntry?.model || boundProvider?.default_model || "";

  const withBusy = useCallback(async (op: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await op();
    } catch {
      // Error toasts are handled by the api-layer mutations.
    } finally {
      setBusy(false);
    }
  }, []);

  const activate = useCallback(
    (providerId: string, model?: string) =>
      withBusy(() => {
        const provider = providers.find((p) => p.id === providerId);
        return activateTool(providerId, toolId, model ?? provider?.default_model ?? undefined);
      }),
    [withBusy, providers, activateTool, toolId],
  );

  const deactivate = useCallback(() => withBusy(() => deactivateTool(toolId)), [withBusy, deactivateTool, toolId]);

  /** Re-write the on-disk config with the current binding. */
  const resync = useCallback(() => {
    if (!activeEntry?.provider_id) return Promise.resolve();
    return withBusy(() => activateTool(activeEntry.provider_id, toolId, currentModel || undefined));
  }, [withBusy, activeEntry, activateTool, toolId, currentModel]);

  /** Set the model of a specific bound provider (defaults to the active one). */
  const pickModel = useCallback(
    (model: string, providerId?: string) => {
      const target = providerId ?? activeEntry?.provider_id;
      if (!target) return Promise.resolve();
      return withBusy(() => activateTool(target, toolId, model));
    },
    [withBusy, activeEntry, activateTool, toolId],
  );

  /** Multi-provider: add (or re-activate) a provider as a binding entry. */
  const addProvider = useCallback((providerId: string, model?: string) => activate(providerId, model), [activate]);

  /** Multi-provider: switch which bound provider is active. */
  const setActive = useCallback(
    (providerId: string) => withBusy(() => setActiveBinding(toolId, providerId)),
    [withBusy, setActiveBinding, toolId],
  );

  /** Multi-provider: remove one bound provider entry. */
  const removeEntry = useCallback(
    (providerId: string) => withBusy(() => removeBindingEntry(toolId, providerId)),
    [withBusy, removeBindingEntry, toolId],
  );

  const updateSettings = useCallback(
    (settings: Record<string, unknown>) => withBusy(() => updateToolSettings(toolId, settings)),
    [withBusy, updateToolSettings, toolId],
  );

  return {
    agent,
    isMulti,
    providers,
    binding,
    /** The active binding entry (single-provider shape). */
    activeEntry,
    /** All bound entries + resolved providers (multi-provider shape). */
    entries,
    boundProvider,
    compatibleProviders,
    currentModel,
    install: { installed: installByTool[toolId]?.installed ?? true, loading: installLoading },
    busy,
    activate,
    deactivate,
    resync,
    pickModel,
    addProvider,
    setActive,
    removeEntry,
    updateSettings,
  };
}

export type AgentActivation = ReturnType<typeof useAgentActivation>;

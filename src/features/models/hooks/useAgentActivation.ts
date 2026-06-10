/**
 * The single activation flow for one agent. Consumed by AgentHeroCard and
 * AgentSettingsDialog — replaces the three divergent paths that previously
 * lived in ModelsHub (callback threading), ToolActivationPanel (toggle with
 * its own state) and AgentHeroCard (busy handler cluster).
 *
 * Data comes from the providers-flat cache; mutations (and their toasts) from
 * the api layer. `busy` covers the in-flight window for syncing status.
 */
import { useCallback, useMemo, useState } from "react";
import { useActivationMutations } from "../api/activations";
import { useToolInstallStatuses } from "../api/install";
import { useProvidersQuery } from "../api/providers";
import { type AgentDescriptor, getAgent, type ProviderToolId, providerCompatibleWithAgent } from "../lib/agentRegistry";

export function useAgentActivation(toolId: ProviderToolId) {
  const agent = getAgent(toolId) as AgentDescriptor;
  const { data } = useProvidersQuery();
  const { activateTool, deactivateTool, updateToolSettings } = useActivationMutations();
  const toolIds = useMemo(() => [toolId], [toolId]);
  const { byTool: installByTool, isLoading: installLoading } = useToolInstallStatuses(toolIds);
  const [busy, setBusy] = useState(false);

  const providers = useMemo(() => {
    if (!data) return [];
    return [...data.providers].sort((a, b) => a.sort_index - b.sort_index);
  }, [data]);

  const activation = data?.tool_activations?.[toolId] ?? null;

  const boundProvider = useMemo(() => {
    if (!activation?.provider_id) return null;
    return providers.find((p) => p.id === activation.provider_id) ?? null;
  }, [activation, providers]);

  const compatibleProviders = useMemo(
    () => providers.filter((p) => providerCompatibleWithAgent(agent, p)),
    [providers, agent],
  );

  const currentModel = activation?.model || boundProvider?.default_model || "";

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
    if (!activation?.provider_id) return Promise.resolve();
    return withBusy(() => activateTool(activation.provider_id, toolId, currentModel || undefined));
  }, [withBusy, activation, activateTool, toolId, currentModel]);

  const pickModel = useCallback(
    (model: string) => {
      if (!activation?.provider_id) return Promise.resolve();
      return withBusy(() => activateTool(activation.provider_id, toolId, model));
    },
    [withBusy, activation, activateTool, toolId],
  );

  const updateSettings = useCallback(
    (settings: Record<string, unknown>) => withBusy(() => updateToolSettings(toolId, settings)),
    [withBusy, updateToolSettings, toolId],
  );

  return {
    agent,
    providers,
    activation,
    boundProvider,
    compatibleProviders,
    currentModel,
    install: { installed: installByTool[toolId]?.installed ?? true, loading: installLoading },
    busy,
    activate,
    deactivate,
    resync,
    pickModel,
    updateSettings,
  };
}

export type AgentActivation = ReturnType<typeof useAgentActivation>;

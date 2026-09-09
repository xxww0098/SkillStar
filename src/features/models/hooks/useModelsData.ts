import { useCallback, useEffect, useMemo, useState } from "react";
import { useProvidersFlat } from "./useProvidersFlat";
import type { ClaudeClientId } from "../lib/claudeClients";
import type { ModelsNavBridge } from "../lib/navBridge";
import { findOfficialProvider, apiProviders, withEnsuredOfficialProviders } from "../lib/officialProviders";
import { activeEntry } from "../lib/toolBinding";
import type { ProviderEditorTab } from "../types";

export type ModelsHubOverlay =
  | { type: "none" }
  | { type: "create" }
  | { type: "edit"; providerId: string; tab?: ProviderEditorTab }
  | { type: "delete"; providerId: string };

/** View state only: changing a client or preview source never changes a binding. */
export function useModelsData(nav: ModelsNavBridge) {
  const store = useProvidersFlat();
  const { selectedProviderId, setSelectedProviderId, modelsDrawerRequest, clearModelsDrawerRequest } = nav;
  const [clientId, setClientId] = useState<ClaudeClientId>("claude-code");
  const [interactionLocked, setInteractionLocked] = useState(false);
  // null follows the current binding; an empty string previews an empty API connection.
  const [preferredSourceId, setPreferredSourceId] = useState<string | null>(null);
  const [overlay, setOverlayState] = useState<ModelsHubOverlay>({ type: "none" });
  const providers = useMemo(() => withEnsuredOfficialProviders(store.providers), [store.providers]);
  const thirdPartyProviders = useMemo(() => apiProviders(providers), [providers]);
  const officialId = findOfficialProvider(providers, "claude-code")!.id;
  const binding = store.toolActivations[clientId] ?? null;
  const currentEntry = activeEntry(binding);
  const sourceExists = (id: string) => id === officialId || thirdPartyProviders.some((p) => p.id === id);
  const sourceId =
    preferredSourceId === "" || (preferredSourceId !== null && sourceExists(preferredSourceId))
      ? preferredSourceId
      : currentEntry && sourceExists(currentEntry.provider_id)
        ? currentEntry.provider_id
        : officialId;

  const selectClient = useCallback((id: ClaudeClientId) => {
    setClientId(id);
    setPreferredSourceId(null);
  }, []);

  const setOverlay = useCallback(
    (next: ModelsHubOverlay) => {
      setOverlayState(next);
      if (next.type === "edit") {
        setSelectedProviderId(next.providerId);
        setPreferredSourceId(next.providerId);
      }
    },
    [setSelectedProviderId],
  );

  const closeOverlay = useCallback(() => {
    setOverlayState({ type: "none" });
    setSelectedProviderId(null);
  }, [setSelectedProviderId]);

  useEffect(() => {
    // Sidebar requests must obey the same pending-write/draft boundary as local controls.
    if (!modelsDrawerRequest || interactionLocked) return;
    const request = modelsDrawerRequest;
    clearModelsDrawerRequest();
    if (request.kind === "create") {
      setOverlayState({ type: "create" });
    } else if (request.providerId) {
      setSelectedProviderId(request.providerId);
      setPreferredSourceId(request.providerId);
      setOverlayState({ type: "edit", providerId: request.providerId });
    }
  }, [modelsDrawerRequest, clearModelsDrawerRequest, setSelectedProviderId, interactionLocked]);

  return {
    providers,
    thirdPartyProviders,
    toolActivations: store.toolActivations,
    isLoading: store.isLoading,
    error: store.error,
    refresh: store.refresh,
    activateTool: store.activateTool,
    clientId,
    setInteractionLocked,
    selectClient,
    sourceId,
    selectSource: setPreferredSourceId,
    binding,
    currentEntry,
    selectedProviderId,
    overlay,
    setOverlay,
    closeOverlay,
  };
}

export type ModelsHubData = ReturnType<typeof useModelsData>;

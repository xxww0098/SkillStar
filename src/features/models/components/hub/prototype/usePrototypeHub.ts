import { useCallback, useEffect, useMemo, useState } from "react";
import { useProvidersFlat } from "../../../hooks/useProvidersFlat";
import { PROVIDER_AGENTS, type ProviderToolId } from "../../../lib/agentRegistry";
import { activeEntry } from "../../../lib/toolBinding";
import { CLAUDE_OFFICIAL_ID, CODEX_OFFICIAL_ID, withEnsuredOfficialProviders } from "../../../lib/officialProviders";
import { DEFAULT_VISIBLE_COLUMN_IDS, MATRIX_COLUMNS, type MatrixColumnId } from "./matrix/matrixColumns";
import type { ModelsNavBridge } from "./modelsNavBridge";
import type { PrototypeHubData, PrototypeOverlay } from "./types";

/**
 * Shared data + overlay state. Syncs with the left ModelsSidebar via
 * `modelsDrawerRequest` / `selectedProviderId` so Add / Recent open the
 * main-pane editor instead of a drawer.
 *
 * Official seeds come from `get_providers_flat` (backend `ensure_official_providers`)
 * for header activate/deactivate. Matrix rows hide them via `matrixProviders`.
 * Client inject is only a fallback when the store lacks them (e.g. stale mock).
 *
 * Nav fields come from {@link ModelsNavBridge} (App passes them) — do not call
 * `useNavigation` here; the Models page is lazy-loaded and must not depend on
 * NavContext identity inside that chunk.
 */
export function usePrototypeHub(nav: ModelsNavBridge): PrototypeHubData {
  const {
    providers: storeProviders,
    toolActivations,
    isLoading,
    activateTool,
    deactivateTool,
    removeBindingEntry,
    createProvider,
  } = useProvidersFlat();
  const { selectedProviderId, setSelectedProviderId, modelsDrawerRequest, clearModelsDrawerRequest } = nav;
  const [selectedAgentId, setSelectedAgentId] = useState<ProviderToolId>("claude-code");
  const [visibleColumnIds, setVisibleColumnIds] = useState<MatrixColumnId[]>(() => [...DEFAULT_VISIBLE_COLUMN_IDS]);
  const [overlay, setOverlayState] = useState<PrototypeOverlay>({ type: "none" });

  // Prefer store Official seeds; inject only when ensure/mock omitted them.
  const providers = useMemo(() => withEnsuredOfficialProviders(storeProviders), [storeProviders]);

  const toggleVisibleColumn = useCallback((id: MatrixColumnId) => {
    setVisibleColumnIds((prev) => {
      if (prev.includes(id)) {
        if (prev.length <= 1) return prev;
        return prev.filter((x) => x !== id);
      }
      const order = MATRIX_COLUMNS.map((c) => c.columnId);
      return order.filter((columnId) => columnId === id || prev.includes(columnId));
    });
  }, []);

  const setOverlay = useCallback(
    (next: PrototypeOverlay) => {
      setOverlayState(next);
      if (next.type === "edit") {
        setSelectedProviderId(next.providerId);
      }
      // `none` keeps selectedProviderId so B3 can collapse page → row inspector.
    },
    [setSelectedProviderId],
  );

  const closeOverlay = useCallback(() => {
    setOverlayState({ type: "none" });
    setSelectedProviderId(null);
  }, [setSelectedProviderId]);

  // Sidebar "Add provider" / Recent provider clicks arrive as drawer requests —
  // map them to page/modal overlays instead.
  useEffect(() => {
    if (!modelsDrawerRequest) return;
    const req = modelsDrawerRequest;
    clearModelsDrawerRequest();
    if (req.kind === "create") {
      setOverlayState({ type: "create" });
      return;
    }
    if (req.providerId) {
      setSelectedProviderId(req.providerId);
      setOverlayState({ type: "edit", providerId: req.providerId });
    }
  }, [modelsDrawerRequest, clearModelsDrawerRequest, setSelectedProviderId]);

  const stub = useCallback((action: string, detail?: Record<string, unknown>) => {
    // Claude mapping UI still has a few local-only actions; keep quiet in prod.
    if (import.meta.env.DEV) {
      console.info("[models-hub]", action, detail ?? {});
    }
  }, []);

  const bindings = useMemo(() => {
    return PROVIDER_AGENTS.map((agent) => {
      const entry = activeEntry(toolActivations[agent.toolId]);
      const provider = entry ? (providers.find((p) => p.id === entry.provider_id) ?? null) : null;
      return {
        toolId: agent.toolId,
        providerId: entry?.provider_id ?? null,
        model: entry?.model ?? null,
        providerName: provider?.name ?? null,
      };
    });
  }, [providers, toolActivations]);

  const stateDump = useMemo(
    () => ({
      loading: isLoading,
      providerCount: providers.length,
      officialSeeds: providers
        .filter((p) => p.id === CLAUDE_OFFICIAL_ID || p.id === CODEX_OFFICIAL_ID)
        .map((p) => p.id),
      officialFromStore: storeProviders
        .filter((p) => p.id === CLAUDE_OFFICIAL_ID || p.id === CODEX_OFFICIAL_ID)
        .map((p) => p.id),
      toolsOnOfficial: (["claude-code", "claude-desktop", "codex"] as const).filter((toolId) => {
        const entry = activeEntry(toolActivations[toolId]);
        return Boolean(entry && (entry.provider_id === CLAUDE_OFFICIAL_ID || entry.provider_id === CODEX_OFFICIAL_ID));
      }),
      selectedAgentId,
      visibleColumnIds,
      selectedProviderId,
      overlay,
      bindings,
    }),
    [
      isLoading,
      providers,
      storeProviders,
      toolActivations,
      selectedAgentId,
      visibleColumnIds,
      selectedProviderId,
      overlay,
      bindings,
    ],
  );

  return {
    providers,
    toolActivations,
    agents: PROVIDER_AGENTS,
    isLoading,
    visibleColumnIds,
    toggleVisibleColumn,
    selectedAgentId,
    setSelectedAgentId,
    selectedProviderId,
    overlay,
    setOverlay,
    closeOverlay,
    stateDump,
    stub,
    activateTool,
    deactivateTool,
    removeBindingEntry,
    createProvider,
  };
}

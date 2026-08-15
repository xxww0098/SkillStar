/**
 * Shared shape of the Models hub's data + overlay state.
 *
 * Produced by `hooks/useModelsData`, consumed by every matrix component below
 * this directory. It is the seam the WP-4 IA rewrite replaces.
 */

import type { ProviderEntryFlat, ToolActivationsMap, ToolSyncResult } from "../../../../../types";
import type { AgentDescriptor, ProviderToolId } from "../../../lib/agentRegistry";
import type { ProviderEditorTab } from "../../../types";
import type { MatrixColumnId } from "./matrixColumns";

export type ModelsHubOverlay =
  | { type: "none" }
  | { type: "create" }
  | { type: "edit"; providerId: string; tab?: ProviderEditorTab }
  | { type: "agent-settings"; toolId: ProviderToolId }
  | { type: "delete"; providerId: string }
  | { type: "app-ai" };

export type EditorDetailStyle = "tabs" | "sections" | "split";

export type ModelsHubData = {
  providers: ProviderEntryFlat[];
  toolActivations: ToolActivationsMap;
  agents: AgentDescriptor[];
  isLoading: boolean;
  /** Selected carousel icons — drives which matrix columns are shown. */
  visibleColumnIds: MatrixColumnId[];
  toggleVisibleColumn: (id: MatrixColumnId) => void;
  selectedAgentId: ProviderToolId;
  setSelectedAgentId: (id: ProviderToolId) => void;
  selectedProviderId: string | null;
  overlay: ModelsHubOverlay;
  setOverlay: (next: ModelsHubOverlay) => void;
  closeOverlay: () => void;
  /** DEV-only leftover for unmapped Claude mapping UI actions. */
  stub: (action: string, detail?: Record<string, unknown>) => void;
  /** Production activate IPC (Official URL-gate / Codex oauth handled in Rust). */
  activateTool: (
    providerId: string,
    toolId: string,
    model?: string,
    settings?: Record<string, unknown> | null,
  ) => Promise<ToolSyncResult>;
  deactivateTool: (toolId: string) => Promise<void>;
  /**
   * Unbind a single provider from a multi-provider agent, leaving its other
   * bindings alone — unlike `deactivateTool`, which clears the whole tool.
   */
  removeBindingEntry: (toolId: string, providerId: string) => Promise<ToolSyncResult>;
  createProvider: (entry: Partial<ProviderEntryFlat>) => Promise<ProviderEntryFlat>;
};

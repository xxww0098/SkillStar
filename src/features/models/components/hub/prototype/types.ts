/**
 * PROTOTYPE — Models hub information architecture pass.
 *
 * Question: after B2b (rich cells) + cc-switch lessons, which IA should SkillStar
 * ship — matrix with Agent additives, App-vertical, or dual-rail?
 *
 * Plan: Three structurally different layouts via `?variant=D1|D2|D3`.
 */

import type { ProviderEntryFlat, ToolActivationsMap, ToolSyncResult } from "../../../../../types";
import type { AgentDescriptor, ProviderToolId } from "../../../lib/agentRegistry";
import type { ProviderEditorTab } from "../../../types";
import type { MatrixColumnId } from "./matrix/matrixColumns";

export type PrototypeOverlay =
  | { type: "none" }
  | { type: "create" }
  | { type: "edit"; providerId: string; tab?: ProviderEditorTab }
  | { type: "agent-settings"; toolId: ProviderToolId }
  | { type: "delete"; providerId: string }
  | { type: "app-ai" };

export type EditorDetailStyle = "tabs" | "sections" | "split";

export type PrototypeHubData = {
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
  overlay: PrototypeOverlay;
  setOverlay: (next: PrototypeOverlay) => void;
  closeOverlay: () => void;
  stateDump: Record<string, unknown>;
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

/**
 * IA variants the hub can render. D1 is the production matrix; D2/D3 are
 * dev-only alternates reachable through `?variant=`. Display names live in the
 * dev-only variant switcher, not here — this is just the key set.
 */
export const VARIANT_KEYS = ["D1", "D2", "D3"] as const;

export type IaVariantKey = (typeof VARIANT_KEYS)[number];

/** Map older B2 keys onto the current IA set. */
export function normalizeIaVariant(raw: string | null): IaVariantKey {
  if (raw === "D2" || raw === "D3") return raw;
  // Default / D1 / previous B2 winners → matrix+additive
  if (
    raw === "D1" ||
    raw === "B2b" ||
    raw === "B2a" ||
    raw === "B2c" ||
    raw === "B2" ||
    raw === "B1" ||
    raw === "B3" ||
    raw === "B" ||
    raw === "A" ||
    raw === "C" ||
    !raw
  ) {
    return "D1";
  }
  return "D1";
}

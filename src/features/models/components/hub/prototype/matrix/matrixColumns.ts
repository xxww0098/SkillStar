import type { AgentDescriptor, ProviderToolId } from "../../../../lib/agentRegistry";
import { PROVIDER_AGENTS } from "../../../../lib/agentRegistry";
import type { ClaudeSurface } from "./ClaudeSurfaceIcon";

/**
 * Matrix columns. Claude CLI and Claude Desktop are independent bindings
 * (separate `tool_activations` keys + Official toggles + mapping state).
 * They share the Claude Official seed provider id, not each other's binding.
 */
export type MatrixColumnId = "claude-code" | "claude-desktop" | "codex" | "opencode" | "pi" | "omp";

export type MatrixColumn = {
  columnId: MatrixColumnId;
  /** Real tool-sync / activation id. */
  bindToolId: ProviderToolId;
  displayName: string;
  kind: AgentDescriptor["kind"];
  claudeSurface?: ClaudeSurface;
};

export const MATRIX_COLUMNS: MatrixColumn[] = [
  {
    columnId: "claude-code",
    bindToolId: "claude-code",
    displayName: "Claude CLI",
    kind: "single",
    claudeSurface: "cli",
  },
  {
    columnId: "claude-desktop",
    bindToolId: "claude-desktop",
    displayName: "Claude Desktop",
    kind: "single",
    claudeSurface: "desktop",
  },
  {
    columnId: "codex",
    bindToolId: "codex",
    displayName: "Codex",
    kind: "multi",
  },
  {
    columnId: "opencode",
    bindToolId: "opencode",
    displayName: "OpenCode",
    kind: "multi",
  },
  {
    columnId: "pi",
    bindToolId: "pi",
    displayName: "Pi",
    kind: "multi",
  },
  {
    columnId: "omp",
    bindToolId: "omp",
    displayName: "Oh My Pi",
    kind: "multi",
  },
];

export const DEFAULT_VISIBLE_COLUMN_IDS: MatrixColumnId[] = MATRIX_COLUMNS.map((c) => c.columnId);

export function getMatrixColumn(id: MatrixColumnId): MatrixColumn | undefined {
  return MATRIX_COLUMNS.find((c) => c.columnId === id);
}

/** Agent descriptor for bind semantics (URL field, models, etc.). */
export function bindAgentForColumn(column: MatrixColumn): AgentDescriptor {
  return PROVIDER_AGENTS.find((a) => a.toolId === column.bindToolId)!;
}

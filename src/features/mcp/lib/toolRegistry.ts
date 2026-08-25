import { MCP_TOOL_IDS, type McpToolId } from "../../../types";

/**
 * Per-tool display labels and per-tool *field support*.
 *
 * Both tables mirror `skillstar_models::mcp`: labels mirror `McpToolSpec.label`
 * in `registry.rs`, support flags mirror what each `*_spec` / `*_toml_table`
 * writer in `specs.rs` actually emits. That crate is the SSOT; this file exists
 * because the create/edit form has to tell the user *before* they type that a
 * field will be dropped for the targets they picked, and no command exposes the
 * matrix. Keep it in the same change as a Rust-side spec change.
 *
 * `mcp_tool_statuses` returns the authoritative label at runtime — prefer it
 * wherever a status read is already happening (the tool view does); the static
 * table is for the render paths that must not wait on a filesystem probe.
 */
export const MCP_TOOL_LABELS: Record<McpToolId, string> = {
  "claude-code": "Claude Code",
  "claude-desktop-chat": "Claude Desktop",
  codex: "Codex",
  grok: "Grok",
  opencode: "OpenCode",
  zcode: "ZCode",
  kiro: "Kiro",
  cursor: "Cursor",
  vscode: "VS Code",
  windsurf: "Windsurf",
  cline: "Cline",
  "gemini-cli": "Gemini CLI",
  zed: "Zed",
  maka: "Maka",
};

/** Optional entry fields whose projection is tool-specific. */
export type McpOptionalField = "autoApprove" | "disabledTools" | "timeout";

export const MCP_OPTIONAL_FIELDS: readonly McpOptionalField[] = ["autoApprove", "disabledTools", "timeout"];

/**
 * Which tools write each optional field. Everything absent here is silently
 * dropped by that tool's writer — which is exactly the thing the form has to
 * stop hiding (audit D.3-6).
 *
 * Evidence, one entry per row (`crates/skillstar-models/src/mcp/specs.rs`):
 * - `autoApprove`: Kiro `autoApprove` (`kiro_spec`), Cline `autoApprove`
 *   (`cline_spec`). Gemini's `trust: true` is deliberately never projected, so
 *   Gemini CLI does **not** count.
 * - `disabledTools`: Kiro `disabledTools`, Codex `disabled_tools`, Gemini CLI
 *   `excludeTools`.
 * - `timeout`: OpenCode `timeout` (ms), Cline `timeout` (ms), Gemini CLI
 *   `timeout` (ms), Codex `tool_timeout_sec` (whole seconds).
 */
const SUPPORTED_BY_FIELD: Record<McpOptionalField, readonly McpToolId[]> = {
  autoApprove: ["kiro", "cline"],
  disabledTools: ["kiro", "codex", "gemini-cli"],
  timeout: ["opencode", "codex", "cline", "gemini-cli"],
};

export function mcpToolsSupporting(field: McpOptionalField): readonly McpToolId[] {
  return SUPPORTED_BY_FIELD[field];
}

export function mcpToolSupportsField(toolId: McpToolId, field: McpOptionalField): boolean {
  return SUPPORTED_BY_FIELD[field].includes(toolId);
}

/** Human-readable list of the tools that honour a field, for a hint line. */
export function mcpSupportLabels(field: McpOptionalField): string[] {
  return SUPPORTED_BY_FIELD[field].map((toolId) => MCP_TOOL_LABELS[toolId]);
}

export interface McpFieldTargetSplit {
  /** Selected targets that will write the field. */
  honoured: McpToolId[];
  /** Selected targets that will silently drop it. */
  ignored: McpToolId[];
}

/**
 * Split the currently selected targets by whether they honour `field`.
 *
 * `ignored` is what the form warns about; an empty `enabledToolIds` yields two
 * empty lists, so "nothing selected yet" never reads as "everything ignores
 * this".
 */
export function splitTargetsByFieldSupport(
  field: McpOptionalField,
  enabledToolIds: readonly McpToolId[],
): McpFieldTargetSplit {
  const honoured: McpToolId[] = [];
  const ignored: McpToolId[] = [];
  for (const toolId of enabledToolIds) {
    (mcpToolSupportsField(toolId, field) ? honoured : ignored).push(toolId);
  }
  return { honoured, ignored };
}

/** Tool ids currently switched on in a form's `enabled` record, in canonical order. */
export function enabledMcpToolIds(enabled: Readonly<Record<string, boolean>>): McpToolId[] {
  return MCP_TOOL_IDS.filter((toolId) => enabled[toolId] === true);
}

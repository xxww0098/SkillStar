import { cn } from "../../../lib/utils";
import { MCP_TOOL_IDS, type McpToolId } from "../../../types";
import { MCP_TOOL_LABELS } from "../lib/toolRegistry";

interface McpToolTargetPickerProps {
  enabled: Readonly<Record<string, boolean>>;
  onToggle: (toolId: McpToolId, next: boolean) => void;
  /** Per-tool suffix, e.g. "not installed" from `mcp_tool_statuses`. */
  noteFor?: (toolId: McpToolId) => string | null;
}

/**
 * The "which agent tools get this server" grid.
 *
 * Enumerates `MCP_TOOL_IDS` rather than the Agent-profile intersection: the
 * profile mapping decides which targets get a *rail toggle* on a card, but a
 * target with no profile is still a real config file SkillStar writes, and
 * hiding it here would make it unreachable.
 */
export function McpToolTargetPicker({ enabled, onToggle, noteFor }: McpToolTargetPickerProps) {
  return (
    <div className="grid grid-cols-2 gap-2">
      {MCP_TOOL_IDS.map((toolId) => {
        const on = enabled[toolId] ?? false;
        const note = noteFor?.(toolId) ?? null;
        return (
          <button
            key={toolId}
            type="button"
            aria-pressed={on}
            onClick={() => onToggle(toolId, !on)}
            className={cn(
              "flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-xs transition",
              on
                ? "border-primary/50 bg-primary/10 text-foreground"
                : "border-border bg-background/40 text-muted-foreground hover:bg-muted/40",
            )}
          >
            <span className="min-w-0 truncate text-left">
              {MCP_TOOL_LABELS[toolId]}
              {note ? <span className="ml-1 text-muted-foreground/70">{note}</span> : null}
            </span>
            <span className={cn("h-2 w-2 shrink-0 rounded-full", on ? "bg-primary" : "bg-muted-foreground/30")} />
          </button>
        );
      })}
    </div>
  );
}

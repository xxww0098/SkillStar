import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { agentIconCls, cn } from "../../../lib/utils";
import type { McpToolId } from "../../../types";
import type { McpAgentTarget } from "../lib/agentTargets";
import { MCP_NOTE_CLS } from "./McpFormField";

interface McpToolTargetPickerProps {
  /** Settings-enabled MCP agents only — one chip per enabled profile. */
  targets: readonly McpAgentTarget[];
  enabled: Readonly<Record<string, boolean>>;
  onToggle: (toolId: McpToolId, next: boolean) => void;
  /** Per-tool suffix, e.g. "not installed" from `mcp_tool_statuses`. */
  noteFor?: (toolId: McpToolId) => string | null;
}

/**
 * The "which agent tools get this server" grid.
 *
 * Renders the Settings-enabled Agent ∩ MCP-support set (`selectMcpAgentTargets`).
 * How many Agents are on in Settings is how many chips appear. Targets with no
 * Agent profile (e.g. Claude Desktop Chat) stay on the tool-status view.
 */
export function McpToolTargetPicker({ targets, enabled, onToggle, noteFor }: McpToolTargetPickerProps) {
  const { t } = useTranslation();

  if (targets.length === 0) {
    // Sits under the "enabled tools" label, so it takes the note scale (11px):
    // `text-caption` would render this explanation larger than the label above.
    return <p className={cn(MCP_NOTE_CLS, "text-muted-foreground")}>{t("mcp.noEnabledAgents")}</p>;
  }

  return (
    <div className="grid grid-cols-2 gap-1.5">
      {targets.map(({ toolId, profile }) => {
        const on = enabled[toolId] ?? false;
        const note = noteFor?.(toolId) ?? null;
        const label = profile.display_name;
        return (
          <button
            key={toolId}
            type="button"
            aria-pressed={on}
            aria-label={label}
            title={note ? `${label} ${note}` : label}
            onClick={() => onToggle(toolId, !on)}
            className={cn(
              "group flex min-h-9 cursor-pointer items-center gap-2 rounded-lg border px-2.5 py-1 text-left transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
              on
                ? "border-primary/55 bg-primary/[0.08] text-foreground ring-1 ring-primary/25 shadow-2xs"
                : "border-border/60 bg-background/50 text-muted-foreground hover:border-border hover:bg-muted/30 hover:text-foreground hover:shadow-2xs",
            )}
          >
            <span
              className={cn(
                "flex h-6 w-6 shrink-0 items-center justify-center rounded-md transition-colors duration-150",
                on ? "bg-primary/15 text-primary" : "bg-muted/60 text-muted-foreground group-hover:bg-muted/80",
              )}
            >
              <AgentIcon
                profile={profile}
                className={cn(agentIconCls(profile.icon, "h-3.5 w-3.5"), !on && "opacity-80")}
              />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-xs font-medium tracking-tight text-foreground">{label}</span>
              {note ? (
                <span className="block truncate text-[10px] font-normal tracking-normal text-muted-foreground">
                  {note}
                </span>
              ) : null}
            </span>
            <span
              className={cn(
                "flex h-4 w-4 shrink-0 items-center justify-center rounded-full transition-all duration-150",
                on
                  ? "bg-primary text-primary-foreground shadow-2xs scale-100"
                  : "border border-border/80 bg-background/50 text-transparent scale-90",
              )}
              aria-hidden
            >
              <Check className="h-2.5 w-2.5" strokeWidth={2.8} />
            </span>
          </button>
        );
      })}
    </div>
  );
}

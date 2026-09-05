import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { agentIconCls, cn } from "../../../lib/utils";
import type { McpToolId } from "../../../types";
import type { McpAgentTarget } from "../lib/agentTargets";

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
    return <p className="text-caption">{t("mcp.noEnabledAgents")}</p>;
  }

  return (
    <div className="grid grid-cols-2 gap-2">
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
              "flex min-h-11 cursor-pointer items-center gap-2.5 rounded-xl border px-2.5 py-2 text-left transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
              on
                ? "border-primary/45 bg-primary/10 text-foreground shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.12)]"
                : "border-border/70 bg-background/40 text-muted-foreground hover:border-border hover:bg-muted/35 hover:text-foreground",
            )}
          >
            <span
              className={cn(
                "flex h-8 w-8 shrink-0 items-center justify-center rounded-lg",
                on ? "bg-primary/15" : "bg-muted/60",
              )}
            >
              <AgentIcon profile={profile} className={cn(agentIconCls(profile.icon, "h-5 w-5"), !on && "opacity-80")} />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13px] font-medium tracking-tight text-foreground">{label}</span>
              {note ? (
                <span className="mt-0.5 block truncate text-micro font-normal tracking-normal text-muted-foreground">
                  {note}
                </span>
              ) : null}
            </span>
            <span
              className={cn(
                "flex h-5 w-5 shrink-0 items-center justify-center rounded-full transition-colors duration-150",
                on ? "bg-primary text-primary-foreground" : "bg-muted/80 text-transparent",
              )}
              aria-hidden
            >
              <Check className="h-3 w-3" strokeWidth={2.6} />
            </span>
          </button>
        );
      })}
    </div>
  );
}

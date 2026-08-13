import { CircleSlash, FolderOpen, RefreshCw, Wrench } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { cn } from "../../../lib/utils";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { MCP_TOOL_IDS } from "../../../types";
import { mcpToolIdsWithoutAgentProfile } from "../lib/agentTargets";
import { useMcpToolStatuses } from "../hooks/useMcpToolStatuses";
import { MCP_TOOL_LABELS } from "../lib/toolRegistry";

/**
 * Where every MCP config target lives, and what is in it.
 *
 * `mcp_tool_statuses` has always returned `installed`, `configPath` and
 * `serverCount`; the app read them once inside the bulk import and discarded
 * them (audit D.3-7). That left the two questions a user actually asks — "did
 * SkillStar write anything for Cursor?" and "which file do I look at?" —
 * answerable only by guessing.
 *
 * `serverCount` counts what is in the *live* file, SkillStar-managed or not, so
 * a tool the user configured by hand shows a non-zero count with no entries in
 * the SkillStar store. That is the honest number: it is what the agent will
 * load.
 */

interface McpToolStatusPanelProps {
  className?: string;
}

export function McpToolStatusPanel({ className }: McpToolStatusPanelProps) {
  const { t } = useTranslation();
  const { profiles } = useAgentProfiles();
  const { statuses, installedCount, isLoading, isFetching, refetch } = useMcpToolStatuses();

  const unreachable = mcpToolIdsWithoutAgentProfile(MCP_TOOL_IDS, profiles);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16">
        <LoadingLogo size="md" label={t("mcp.toolStatusLoading")} />
      </div>
    );
  }

  return (
    <section className={cn("space-y-3", className)}>
      <div className="flex items-center gap-2 px-1">
        <Wrench className="h-3.5 w-3.5 text-primary" />
        <h2 className="text-sm font-semibold text-foreground">{t("mcp.toolStatusTitle")}</h2>
        <span className="text-xs text-muted-foreground">
          {t("mcp.toolStatusInstalledCount", { installed: installedCount, total: statuses.length })}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="ml-auto h-7 gap-1.5 px-2 text-[11px]"
          onClick={() => void refetch()}
          disabled={isFetching}
        >
          <RefreshCw className={isFetching ? "h-3 w-3 animate-spin" : "h-3 w-3"} />
          {t("common.refresh")}
        </Button>
      </div>

      <div className="grid gap-2 [grid-template-columns:repeat(auto-fill,minmax(320px,1fr))]">
        {statuses.map((status) => (
          <div
            key={status.toolId}
            className={cn(
              "rounded-xl border px-3 py-2.5",
              status.installed ? "border-border/70 bg-background/50" : "border-border/40 bg-background/25",
            )}
          >
            <div className="flex items-center gap-2">
              <span className="text-xs font-medium text-foreground">
                {status.label || MCP_TOOL_LABELS[status.toolId]}
              </span>
              <span
                className={cn(
                  "inline-flex h-4 items-center gap-1 rounded px-1.5 text-micro font-medium ring-1 ring-inset",
                  status.installed
                    ? "bg-emerald-500/12 text-emerald-600 ring-emerald-500/25 dark:text-emerald-400"
                    : "bg-muted text-muted-foreground ring-border/60",
                )}
              >
                {status.installed ? t("mcp.toolInstalled") : t("mcp.toolNotInstalled")}
              </span>
              <span className="ml-auto text-[11px] tabular-nums text-muted-foreground">
                {t("mcp.toolServerCount", { count: status.serverCount })}
              </span>
            </div>

            <p className="mt-1.5 flex items-start gap-1.5 break-all font-mono text-[11px] text-muted-foreground">
              <FolderOpen className="mt-0.5 h-3 w-3 shrink-0" />
              {status.configPath || t("mcp.toolConfigPathUnknown")}
            </p>

            {unreachable.includes(status.toolId) ? (
              <p className="mt-1.5 flex items-start gap-1.5 text-[11px] leading-relaxed text-muted-foreground/80">
                <CircleSlash className="mt-0.5 h-3 w-3 shrink-0" />
                {t("mcp.toolNoAgentProfile")}
              </p>
            ) : null}
          </div>
        ))}
      </div>
    </section>
  );
}

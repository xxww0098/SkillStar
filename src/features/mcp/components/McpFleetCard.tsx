import { ArrowUpCircle, Globe, RefreshCw, Terminal, Zap } from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import { AgentTargetCarousel } from "../../../components/shared/AgentTargetCarousel";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { McpServerEntry, McpToolId } from "../../../types";
import type { McpProbeEntry } from "../hooks/useMcpProbe";
import type { McpAgentTarget } from "../lib/agentTargets";
import { MCP_FLEET_STATUS_DOT, type McpFleetStatus, mcpFleetStatus } from "../lib/fleetStatus";
import { formatSchemaTokens, mcpServerCommandLine } from "../lib/pasteDraft";

export interface McpFleetCardProps {
  server: McpServerEntry;
  agentTargets: readonly McpAgentTarget[];
  /** Catalog version this entry is behind, when the catalog knows a newer one. */
  updateVersion?: string | null;
  probe?: McpProbeEntry;
  onOpen: () => void;
  onToggleTool: (toolId: McpToolId, enabled: boolean) => void;
  onProbe?: () => void;
  compact?: boolean;
}

function statusColorClass(status: McpFleetStatus): string {
  switch (status) {
    case "ok":
      return "text-emerald-500 dark:text-emerald-400";
    case "needs-auth":
      return "text-sky-500 dark:text-sky-400";
    case "runtime-missing":
      return "text-amber-500 dark:text-amber-400";
    case "error":
      return "text-rose-500 dark:text-rose-400";
    case "probing":
      return "text-primary";
    default:
      return "text-muted-foreground";
  }
}

function McpFleetCardInner({
  server,
  agentTargets,
  updateVersion,
  probe,
  onOpen,
  onToggleTool,
  onProbe,
  compact,
}: McpFleetCardProps) {
  const { t } = useTranslation();
  const isRemote = server.transport === "http" || server.transport === "sse";
  const TransportIcon = isRemote ? Globe : Terminal;
  const status = mcpFleetStatus(probe ?? { report: null, error: null, pending: false });
  const summary = mcpServerCommandLine(server);
  const probing = status === "probing";
  const toolsCount = probe?.report?.tools?.length ?? 0;
  const schemaTokens = probe?.report?.schemaTokens;

  return (
    <div
      onClick={onOpen}
      className={cn(
        "group relative h-full flex flex-col justify-between overflow-hidden rounded-xl border border-border bg-card cursor-pointer transition-all duration-200",
        "hover:bg-card-hover hover:border-primary/40 hover:-translate-y-px hover:shadow-[0_8px_24px_-12px_var(--color-shadow)]",
        compact && "p-2",
      )}
    >
      {/* Top Card Body */}
      <div className="p-3.5 pb-2.5 flex-1 flex flex-col gap-2">
        {/* Header Row: Avatar + Title/Badges + Action Slot */}
        <div className="flex items-start gap-2.5 min-w-0">
          {/* Avatar with transport icon & status dot */}
          <div className="relative shrink-0 flex h-9 w-9 items-center justify-center rounded-xl bg-primary/10 border border-primary/20 group-hover:border-primary/40 transition-colors">
            <TransportIcon
              className={cn(
                "h-4 w-4",
                server.transport === "sse"
                  ? "text-amber-400 paper:text-amber-700"
                  : isRemote
                    ? "text-sky-400 paper:text-sky-600"
                    : "text-emerald-400 paper:text-emerald-600",
              )}
            />
            <span
              aria-hidden
              className={cn(
                "absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full ring-2 ring-background",
                MCP_FLEET_STATUS_DOT[status],
              )}
            />
          </div>

          {/* Title & Badge area */}
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 min-w-0">
              <h3
                className="text-sm font-bold tracking-tight text-foreground truncate group-hover:text-primary transition-colors"
                title={server.name}
              >
                {server.name}
              </h3>
              {updateVersion ? (
                <span
                  title={t("mcp.updateAvailableHint", {
                    installed: server.installedVersion ?? "?",
                    latest: updateVersion,
                  })}
                  className="inline-flex shrink-0 items-center gap-0.5 rounded bg-sky-500/15 px-1 py-px text-[10px] font-semibold text-sky-400 paper:text-sky-700"
                >
                  <ArrowUpCircle className="h-2.5 w-2.5" />
                  {t("mcp.badgeUpdateAvailable")}
                </span>
              ) : null}
              {server.autoApproveAll ? (
                <span
                  title={t("mcp.autoApproveAllHint")}
                  className="inline-flex shrink-0 items-center gap-0.5 rounded bg-amber-500/15 px-1 py-px text-[10px] font-semibold text-amber-300 paper:text-amber-700"
                >
                  <Zap className="h-2.5 w-2.5" />
                  {t("mcp.yoloBadge")}
                </span>
              ) : null}
            </div>
            <div className="mt-1 flex items-center gap-1.5 flex-wrap">
              <span className="inline-flex items-center px-1.5 py-0.5 rounded-md text-[10px] font-semibold bg-muted/80 text-foreground/80 border border-border/70 uppercase">
                {server.transport}
              </span>
              <span className={cn("text-[11px] font-medium truncate", statusColorClass(status))}>
                {t(`mcp.fleetStatus_${status}`)}
              </span>
            </div>
          </div>

          {/* Action Slot: Probe button */}
          {onProbe ? (
            <Button
              type="button"
              variant="ghost"
              size="icon-xs"
              className="h-7 w-7 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/80 transition-colors shrink-0"
              onClick={(event) => {
                event.stopPropagation();
                onProbe();
              }}
              disabled={probing}
              aria-label={t("mcp.fleetProbeAria", { name: server.name })}
              title={t("mcp.probeRun")}
            >
              <RefreshCw className={cn("h-3.5 w-3.5", probing && "animate-spin text-primary")} />
            </Button>
          ) : null}
        </div>

        {/* Description or command line preview */}
        <p
          className="text-xs text-muted-foreground/90 line-clamp-2 leading-relaxed mt-0.5 font-normal select-none"
          title={server.description || summary}
        >
          {server.description || (
            <span className="font-mono text-[11px] text-muted-foreground/80">
              {summary || t("mcp.emptyDescription")}
            </span>
          )}
        </p>
      </div>

      {/* Footer / Capability & Cost + Agent Target Rail */}
      <div className="flex min-h-[42px] items-center justify-between gap-2 rounded-b-xl border-t border-border/40 bg-muted/30 px-3.5 py-2">
        <div className="flex shrink-0 items-center gap-1.5 min-w-0 text-[11px] font-semibold text-muted-foreground tabular-nums">
          {status === "ok" && (toolsCount > 0 || schemaTokens) ? (
            <>
              {toolsCount > 0 ? <span>{t("mcp.probeTools", { count: toolsCount })}</span> : null}
              {toolsCount > 0 && schemaTokens ? <span>·</span> : null}
              {schemaTokens ? (
                <span>{t("mcp.cardSchemaTokens", { tokens: formatSchemaTokens(schemaTokens) })}</span>
              ) : null}
            </>
          ) : status === "probing" ? (
            <span className="text-primary text-[11px] font-medium">{t("mcp.probeRunning")}</span>
          ) : status !== "ok" && status !== "unknown" ? (
            <span className={cn("text-[11px] font-medium truncate max-w-[120px]", statusColorClass(status))}>
              {t(`mcp.fleetStatus_${status}`)}
            </span>
          ) : null}
        </div>

        {agentTargets.length > 0 ? (
          <div className="relative z-10 flex min-w-0 flex-1 items-center justify-end">
            <AgentTargetCarousel
              items={agentTargets.map(({ toolId, profile }) => {
                const selected = server.enabled[toolId] ?? false;
                return {
                  id: toolId,
                  profile,
                  selected,
                  title: `${profile.display_name} ${selected ? t("mcp.toggleOff") : t("mcp.toggleOn")}`,
                };
              })}
              onToggle={({ id, selected }) => {
                onToggleTool(id, selected !== true);
              }}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

export const McpFleetCard = memo(McpFleetCardInner);

import { ArrowUpCircle, Globe, Terminal, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AgentTargetCarousel } from "../../../components/shared/AgentTargetCarousel";
import { CardDescription, CardTitle } from "../../../components/ui/card";
import { CardTemplate } from "../../../components/ui/card-template";
import { cn } from "../../../lib/utils";
import type { McpServerEntry, McpToolId } from "../../../types";
import type { McpAgentTarget } from "../lib/agentTargets";
import { formatSchemaTokens } from "../lib/pasteDraft";
import type { McpProbeEntry } from "../hooks/useMcpProbe";

interface McpServerCardProps {
  server: McpServerEntry;
  agentTargets: readonly McpAgentTarget[];
  /** Catalog version this entry is behind, when the catalog knows a newer one. */
  updateVersion?: string | null;
  probe?: McpProbeEntry;
  onOpen: () => void;
  onToggleTool: (toolId: McpToolId, enabled: boolean) => void;
}

export function McpServerCard({
  server,
  agentTargets,
  updateVersion,
  probe,
  onOpen,
  onToggleTool,
}: McpServerCardProps) {
  const { t } = useTranslation();
  const isRemote = server.transport === "http" || server.transport === "sse";
  const summary = isRemote ? server.url : [server.command, ...(server.args ?? [])].filter(Boolean).join(" ");
  const TransportIcon = isRemote ? Globe : Terminal;
  const description = server.description?.trim();
  const bodyText = description && description !== summary ? description : null;
  const hasBadges = Boolean(updateVersion || server.autoApproveAll || probe?.report?.schemaTokens);

  const hasFooter = agentTargets.length > 0;

  return (
    <CardTemplate
      className="group cursor-pointer"
      onClick={onOpen}
      topRightSlot={
        hasBadges ? (
          <span className="flex items-center gap-1">
            {updateVersion ? (
              <span
                title={t("mcp.updateAvailableHint", {
                  installed: server.installedVersion ?? "?",
                  latest: updateVersion,
                })}
                className="inline-flex items-center gap-0.5 rounded-md bg-sky-500/15 px-1.5 py-0.5 text-[10px] font-semibold text-sky-400 paper:text-sky-700"
              >
                <ArrowUpCircle className="h-2.5 w-2.5" />
                {t("mcp.badgeUpdateAvailable")}
              </span>
            ) : null}
            {server.autoApproveAll ? (
              <span
                title={t("mcp.autoApproveAllHint")}
                className="inline-flex items-center gap-0.5 rounded-md bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-semibold text-amber-300 paper:text-amber-700"
              >
                <Zap className="h-2.5 w-2.5" />
                {t("mcp.yoloBadge")}
              </span>
            ) : null}
            {probe?.report?.schemaTokens ? (
              <span
                title={t("mcp.probeSchemaHint")}
                className="inline-flex items-center rounded-md bg-muted/80 px-1.5 py-0.5 font-mono text-[10px] font-medium text-foreground/70"
              >
                {t("mcp.cardSchemaTokens", { tokens: formatSchemaTokens(probe.report.schemaTokens) })}
              </span>
            ) : probe?.report?.status === "authorization-required" ? (
              <span className="inline-flex items-center rounded-md bg-sky-500/15 px-1.5 py-0.5 text-[10px] font-semibold text-sky-400 paper:text-sky-700">
                {t("mcp.probeStatus_authorization-required")}
              </span>
            ) : null}
          </span>
        ) : null
      }
      headerClassName={hasBadges ? "pr-28" : undefined}
      header={
        <div className="flex items-center gap-2.5">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-primary/10">
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
          </div>
          <div className="min-w-0">
            <CardTitle className="truncate ss-card-title" title={server.name}>
              {server.name}
            </CardTitle>
            <span className="block truncate font-mono ss-card-meta" title={summary || undefined}>
              {summary || "—"}
            </span>
          </div>
        </div>
      }
      bodyClassName={bodyText ? "flex-1" : undefined}
      body={bodyText ? <CardDescription className="ss-card-desc">{bodyText}</CardDescription> : undefined}
      footerClassName={hasFooter ? "ss-card-footer flex items-center mt-auto rounded-b-xl" : undefined}
      footer={
        hasFooter ? (
          <div className="relative z-10 flex min-w-0 flex-1 items-center justify-end gap-1.5">
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
        ) : undefined
      }
    />
  );
}

import { Globe, Terminal, Zap } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AgentTargetCarousel } from "../../../components/shared/AgentTargetCarousel";
import { CardDescription, CardTitle } from "../../../components/ui/card";
import { CardTemplate } from "../../../components/ui/card-template";
import { cn } from "../../../lib/utils";
import type { McpServerEntry, McpToolId } from "../../../types";
import type { McpAgentTarget } from "../lib/agentTargets";

interface McpServerCardProps {
  server: McpServerEntry;
  agentTargets: readonly McpAgentTarget[];
  onOpen: () => void;
  onToggleTool: (toolId: McpToolId, enabled: boolean) => void;
}

export function McpServerCard({ server, agentTargets, onOpen, onToggleTool }: McpServerCardProps) {
  const { t } = useTranslation();
  const isRemote = server.transport === "http" || server.transport === "sse";
  const summary = isRemote ? server.url : [server.command, ...(server.args ?? [])].filter(Boolean).join(" ");
  const TransportIcon = isRemote ? Globe : Terminal;

  return (
    <CardTemplate
      className="group cursor-pointer"
      onClick={onOpen}
      topRightSlot={
        <span className="flex items-center gap-1">
          {server.autoApproveAll ? (
            <span
              title={t("mcp.autoApproveAllHint")}
              className="inline-flex items-center gap-0.5 rounded-md bg-amber-500/15 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-amber-600 dark:text-amber-400"
            >
              <Zap className="h-2.5 w-2.5" />
              {t("mcp.yoloBadge")}
            </span>
          ) : null}
          <span className="rounded-md bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
            {server.transport}
          </span>
        </span>
      }
      headerClassName="pr-28"
      header={
        <div className="flex items-center gap-2.5">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-primary/10">
            <TransportIcon className={cn("h-4 w-4", isRemote ? "text-sky-500" : "text-emerald-500")} />
          </div>
          <div className="min-w-0">
            <CardTitle className="truncate ss-card-title">{server.name}</CardTitle>
            <span className="block truncate font-mono ss-card-meta">{summary || "—"}</span>
          </div>
        </div>
      }
      bodyClassName="flex-1"
      body={
        <CardDescription className="ss-card-desc">
          {server.description || (isRemote ? server.url : summary) || "—"}
        </CardDescription>
      }
      footerClassName="ss-card-footer flex items-center mt-auto rounded-b-xl"
      footer={
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
      }
    />
  );
}

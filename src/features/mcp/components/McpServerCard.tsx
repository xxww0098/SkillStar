import { Globe, Terminal } from "lucide-react";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { CardDescription, CardTitle } from "../../../components/ui/card";
import { CardTemplate } from "../../../components/ui/card-template";
import { HScrollRow } from "../../../components/ui/HScrollRow";
import { agentIconCls, cn } from "../../../lib/utils";
import { MCP_TOOL_IDS, type McpServerEntry, type McpToolId, type McpToolStatus } from "../../../types";

/**
 * Map each MCP tool id to its agent SVG logo (under `public/agents/`) so the
 * per-tool enable/disable toggle uses the same icon affordance as skill cards.
 * `claude-code` carries an in-svg terminal badge to distinguish it from
 * `claude-desktop` (which uses the plain Claude logo).
 */
export const MCP_TOOL_ICON: Record<McpToolId, { profileId: string; icon: string; label: string }> = {
  "claude-code": { profileId: "claude", icon: "agents/claude.svg", label: "Claude Code" },
  "claude-desktop": { profileId: "claude", icon: "agents/claude-desktop.svg", label: "Claude Desktop" },
  codex: { profileId: "codex", icon: "agents/codex.svg", label: "Codex" },
  gemini: { profileId: "gemini", icon: "agents/gemini.svg", label: "Gemini CLI" },
  opencode: { profileId: "opencode", icon: "agents/opencode.svg", label: "OpenCode" },
  zcode: { profileId: "zcode", icon: "agents/zcode.svg", label: "ZCode" },
};

interface McpServerCardProps {
  server: McpServerEntry;
  toolStatuses: McpToolStatus[];
  onOpen: () => void;
  onToggleTool: (toolId: string, enabled: boolean) => void;
}

export function McpServerCard({ server, toolStatuses, onOpen, onToggleTool }: McpServerCardProps) {
  const isRemote = server.transport === "http" || server.transport === "sse";
  const summary = isRemote ? server.url : [server.command, ...(server.args ?? [])].filter(Boolean).join(" ");
  const enabledCount = MCP_TOOL_IDS.filter((t) => server.enabled[t]).length;
  const TransportIcon = isRemote ? Globe : Terminal;

  return (
    <CardTemplate
      className="group cursor-pointer"
      onClick={onOpen}
      topRightSlot={
        <span className="rounded-md bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
          {server.transport}
        </span>
      }
      headerClassName="pr-20"
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
      footerClassName="ss-card-footer flex items-center justify-between mt-auto rounded-b-xl"
      footer={
        <>
          <span className="text-micro text-muted-foreground/70 tabular-nums">
            {enabledCount} / {MCP_TOOL_IDS.length}
          </span>
          <div className="relative z-10 flex min-w-0 flex-1 items-center justify-end gap-1.5">
            <HScrollRow count={MCP_TOOL_IDS.length} itemWidth={28} gap={6} className="min-w-0 gap-1.5">
              {MCP_TOOL_IDS.map((toolId) => {
                const on = server.enabled[toolId] ?? false;
                const installed = toolStatuses.find((s) => s.toolId === toolId)?.installed ?? true;
                const meta = MCP_TOOL_ICON[toolId];
                return (
                  <button
                    key={toolId}
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      onToggleTool(toolId, !on);
                    }}
                    title={`${meta.label} ${on ? "(取消)" : "(激活)"}${installed ? "" : " · 未检测到安装"}`}
                    className={cn(
                      "flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-lg border transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
                      on
                        ? "border-primary/40 bg-primary/10 shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.15)] hover:bg-primary/20 hover:shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.3)]"
                        : "border-transparent bg-transparent hover:bg-muted",
                    )}
                  >
                    <AgentIcon
                      profile={{ id: meta.profileId, icon: meta.icon, display_name: meta.label }}
                      className={cn(
                        agentIconCls(meta.icon, "w-4 h-4"),
                        "drop-shadow-sm transition-[filter,opacity]",
                        !on && "grayscale opacity-40 hover:opacity-70 hover:grayscale-0",
                      )}
                    />
                  </button>
                );
              })}
            </HScrollRow>
          </div>
        </>
      }
    />
  );
}

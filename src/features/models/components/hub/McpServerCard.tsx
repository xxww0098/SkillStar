import { Globe, Terminal } from "lucide-react";
import { AgentIcon } from "../../../../components/ui/AgentIcon";
import { agentIconCls, cn } from "../../../../lib/utils";
import { MCP_TOOL_IDS, type McpServerEntry, type McpToolId, type McpToolStatus } from "../../../../types";

/**
 * Map each MCP tool id to its agent SVG logo (under `public/agents/`) so the
 * per-tool enable/disable toggle uses the same icon affordance as skill cards.
 * `claude-desktop` reuses the Claude logo (no dedicated asset).
 */
const MCP_TOOL_ICON: Record<McpToolId, { profileId: string; icon: string; label: string }> = {
  "claude-code": { profileId: "claude", icon: "agents/claude.svg", label: "Claude Code" },
  "claude-desktop": { profileId: "claude", icon: "agents/claude.svg", label: "Claude Desktop" },
  codex: { profileId: "codex", icon: "agents/codex.svg", label: "Codex" },
  gemini: { profileId: "gemini", icon: "agents/gemini.svg", label: "Gemini CLI" },
  opencode: { profileId: "opencode", icon: "agents/opencode.svg", label: "OpenCode" },
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

  return (
    <div className="group flex flex-col gap-3 rounded-xl border border-border/55 bg-card/55 p-4 transition hover:border-primary/30 hover:bg-card/80">
      <button type="button" onClick={onOpen} className="min-w-0 text-left">
        <div className="flex items-center gap-2">
          {isRemote ? (
            <Globe className="h-4 w-4 shrink-0 text-sky-500" />
          ) : (
            <Terminal className="h-4 w-4 shrink-0 text-emerald-500" />
          )}
          <span className="truncate text-sm font-semibold text-foreground">{server.name}</span>
          <span className="ml-auto shrink-0 rounded-md bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-muted-foreground">
            {server.transport}
          </span>
        </div>
        <p className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground">{summary || "—"}</p>
        {server.description ? (
          <p className="mt-1 line-clamp-1 text-[11px] text-muted-foreground/80">{server.description}</p>
        ) : null}
      </button>

      <div className="flex items-center gap-1.5">
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
                "w-7 h-7 shrink-0 rounded-lg flex items-center justify-center border transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 focus-visible:ring-offset-1 focus-visible:ring-offset-background",
                on
                  ? "border-primary/40 bg-primary/10 shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.15)] hover:shadow-[0_0_0_1px_rgba(var(--color-primary-rgb),0.3)] hover:bg-primary/20"
                  : "border-transparent bg-transparent hover:bg-muted",
              )}
            >
              <AgentIcon
                profile={{ id: meta.profileId, icon: meta.icon, display_name: meta.label }}
                className={cn(
                  agentIconCls(meta.icon, "w-4 h-4"),
                  "transition-[filter,opacity] drop-shadow-sm",
                  !on && "grayscale opacity-40 hover:opacity-70 hover:grayscale-0",
                )}
              />
            </button>
          );
        })}
        <span className="ml-auto self-center text-[10px] text-muted-foreground/70">
          {enabledCount} / {MCP_TOOL_IDS.length}
        </span>
      </div>
    </div>
  );
}

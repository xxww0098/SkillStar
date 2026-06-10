import { motion } from "framer-motion";
import { ExternalLink, FileCog, Loader2, Plug, ShieldCheck } from "lucide-react";
import { useMemo } from "react";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import { cn } from "../../../../lib/utils";
import { useToolConfigFiles } from "../../api/configFiles";
import { AgentToolIcon } from "../shared/AgentToolIcon";

export interface ClaudeDesktopCardProps {
  installed: boolean;
  installLoading: boolean;
  /** Opens the right-side drawer scoped to claude-desktop config editing. */
  onOpenConfig: () => void;
}

type Status = "not-installed" | "no-mcp-yet" | "configured";

const STATUS_STYLE: Record<
  Status,
  {
    chip: string;
    label: string;
    border: string;
    glow: string;
  }
> = {
  "not-installed": {
    chip: "bg-amber-500/15 text-amber-400 ring-amber-500/20",
    label: "未安装",
    border: "border-amber-500/20",
    glow: "shadow-none",
  },
  "no-mcp-yet": {
    chip: "bg-muted text-muted-foreground ring-border",
    label: "未配置 MCP",
    border: "border-border/55",
    glow: "shadow-[0_24px_60px_-40px_var(--color-shadow)]",
  },
  configured: {
    chip: "bg-emerald-500/15 text-emerald-400 ring-emerald-500/20",
    label: "已配置",
    border: "border-emerald-500/25",
    glow: "shadow-[0_30px_60px_-32px_rgba(16,185,129,0.35)]",
  },
};

/**
 * Special Hero card for **Claude Desktop App**.
 *
 * Unlike Claude Code / Codex / OpenCode, Claude Desktop does NOT accept a custom
 * `base_url` or third-party API key — it authenticates via the user's Claude.ai
 * account and talks directly to Anthropic's servers. The only user-editable
 * configuration is the `mcpServers` map in `claude_desktop_config.json`, so this
 * card surfaces install state + MCP server count and links into the drawer's
 * JSON editor instead of the provider-binding flow.
 */
export function ClaudeDesktopCard({ installed, installLoading, onOpenConfig }: ClaudeDesktopCardProps) {
  const editor = useToolConfigFiles("claude-desktop");

  const mcpCount = useMemo(() => {
    if (!editor.content) return 0;
    try {
      const parsed = JSON.parse(editor.content) as { mcpServers?: Record<string, unknown> };
      return Object.keys(parsed.mcpServers ?? {}).length;
    } catch {
      return 0;
    }
  }, [editor.content]);

  const status: Status = !installed && !installLoading ? "not-installed" : mcpCount > 0 ? "configured" : "no-mcp-yet";
  const style = STATUS_STYLE[status];

  return (
    <motion.section
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "relative flex h-full flex-col rounded-3xl border bg-card/75 backdrop-blur-2xl",
        "transition-transform duration-300 hover:-translate-y-0.5",
        style.border,
        style.glow,
      )}
    >
      <span
        aria-hidden
        className={cn(
          "absolute inset-x-0 top-0 h-[2px]",
          status === "configured" && "bg-gradient-to-r from-emerald-400/30 via-emerald-400/70 to-emerald-400/30",
          status === "not-installed" && "bg-gradient-to-r from-amber-400/20 via-amber-400/45 to-amber-400/20",
          status === "no-mcp-yet" && "bg-gradient-to-r from-primary/10 via-primary/35 to-primary/10",
        )}
      />

      <header className="flex items-start gap-3 px-5 pt-5">
        <AgentToolIcon toolId="claude-desktop" size="md" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-base font-bold text-foreground">Claude Desktop</h3>
            <span
              className={cn(
                "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ring-1",
                style.chip,
              )}
            >
              {style.label}
            </span>
          </div>
          <p className="mt-0.5 text-[11px] text-muted-foreground">Anthropic 官方桌面端 · 仅可配置 MCP 服务器</p>
        </div>
      </header>

      <div className="flex-1 space-y-3 px-5 pt-4 pb-3">
        {status === "not-installed" ? (
          <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
            <p>未检测到 Claude Desktop 安装。</p>
            <ExternalAnchor
              href="https://claude.ai/download"
              className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
            >
              下载 Claude Desktop <ExternalLink className="h-3 w-3" />
            </ExternalAnchor>
          </div>
        ) : (
          <>
            {/* MCP servers stat */}
            <div className="rounded-xl border border-border/55 bg-input px-3 py-2.5">
              <div className="flex items-center justify-between gap-2">
                <span className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                  MCP 服务器
                </span>
                {editor.loading ? (
                  <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
                ) : (
                  <span className="text-[11px] font-semibold text-foreground">{mcpCount} 个</span>
                )}
              </div>
              <p className="mt-1 text-[11px] text-muted-foreground/90">
                {mcpCount > 0
                  ? "通过 MCP 协议接入文件系统、Git、自建工具等。"
                  : "尚未添加任何 MCP 服务器,点击下方编辑配置。"}
              </p>
            </div>

            {/* Account hint */}
            <div className="flex items-start gap-2 rounded-xl border border-primary/15 bg-primary/[0.04] px-3 py-2.5">
              <ShieldCheck className="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary/80" />
              <p className="text-[11px] leading-relaxed text-muted-foreground/95">
                <strong className="text-foreground/90">无需供应商绑定。</strong> Claude Desktop 通过 Claude.ai
                账户登录访问 Anthropic 官方端点,不接受第三方 API Key 或自定义 base URL。
              </p>
            </div>
          </>
        )}
      </div>

      <footer className="flex items-center gap-1 border-t border-border/40 bg-background/20 px-4 py-2.5">
        {status === "not-installed" ? (
          <ExternalAnchor
            href="https://claude.ai/download"
            className="ml-auto inline-flex h-7 items-center gap-1.5 rounded-lg border border-border/60 px-2.5 text-[11px] font-medium text-foreground/80 hover:border-primary/40 hover:bg-card-hover"
          >
            <Plug className="h-3 w-3" />
            前往下载
          </ExternalAnchor>
        ) : (
          <Button variant="outline" size="sm" onClick={onOpenConfig} className="ml-auto h-7 text-[11px]">
            <FileCog className="mr-1.5 h-3 w-3" />
            编辑 MCP 配置
          </Button>
        )}
      </footer>
    </motion.section>
  );
}

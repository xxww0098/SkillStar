import { ExternalLink, FolderOpen, Loader2, RefreshCw, Save, ShieldCheck, Wand2 } from "lucide-react";
import { useMemo } from "react";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import { tauriInvoke } from "../../../../lib/ipc";
import { cn } from "../../../../lib/utils";
import { useToolConfigFiles } from "../../hooks/useToolConfigFiles";

/**
 * Drawer body for the Claude Desktop App agent.
 *
 * Claude Desktop only exposes `mcpServers` in its config file — there is no
 * provider/key/base-URL to bind. So instead of the provider form we render:
 *
 * 1. A short orientation card explaining what is and isn't configurable.
 * 2. The on-disk JSON editor for `claude_desktop_config.json`, parsed live
 *    to surface the current MCP server count.
 * 3. Convenience links to the official MCP docs and to a quickstart snippet.
 */
export function ClaudeDesktopDrawerContent() {
  const editor = useToolConfigFiles("claude-desktop");
  const activeFile = editor.files.find((f) => f.file_id === editor.activeFileId);

  const mcpServers = useMemo(() => {
    if (!editor.content) return null;
    try {
      const parsed = JSON.parse(editor.content) as { mcpServers?: Record<string, unknown> };
      return parsed.mcpServers ?? null;
    } catch {
      return null;
    }
  }, [editor.content]);

  const serverCount = mcpServers ? Object.keys(mcpServers).length : 0;
  const parseError = !editor.loading && editor.content && mcpServers === null;

  const handleAddTemplate = () => {
    const next: Record<string, unknown> = mcpServers ? { ...mcpServers } : {};
    next[`example-${serverCount + 1}`] = {
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-filesystem", "/Users/you/Documents"],
    };
    const merged: Record<string, unknown> = {};
    if (editor.content) {
      try {
        Object.assign(merged, JSON.parse(editor.content));
      } catch {
        /* ignore */
      }
    }
    merged.mcpServers = next;
    editor.setContent(`${JSON.stringify(merged, null, 2)}\n`);
  };

  return (
    <div className="space-y-3">
      {/* Orientation card */}
      <section className="rounded-xl border border-primary/15 bg-primary/[0.04] p-4">
        <h4 className="flex items-center gap-2 text-sm font-semibold text-foreground">
          <ShieldCheck className="h-4 w-4 text-primary" />
          关于 Claude Desktop 配置
        </h4>
        <ul className="mt-2 space-y-1.5 text-[11px] leading-relaxed text-muted-foreground/95">
          <li>
            <strong className="text-foreground/90">✅ 可配置:</strong>
            <code className="mx-1 rounded bg-muted/50 px-1 py-0.5 font-mono text-[10px]">mcpServers</code>
            —— 让 Claude Desktop 调用文件系统、Git、Notion、自建 MCP 工具等。
          </li>
          <li>
            <strong className="text-foreground/90">❌ 不可配置:</strong> base URL / API Key / 模型 —— Claude Desktop
            强制通过 Claude.ai 账户登录连接 Anthropic 官方端点。
          </li>
          <li>
            想要自定义供应商、自定义模型?用「Claude」(Claude Code CLI)Agent,它支持
            <code className="mx-1 rounded bg-muted/50 px-1 py-0.5 font-mono text-[10px]">ANTHROPIC_BASE_URL</code>
            等环境变量。
          </li>
        </ul>
        <div className="mt-3 flex flex-wrap gap-1.5">
          <ExternalAnchor
            href="https://modelcontextprotocol.io/quickstart/user"
            className="inline-flex items-center gap-1 rounded-md border border-border/60 px-2 py-1 text-[11px] font-medium text-foreground/80 hover:border-primary/40 hover:bg-card-hover"
          >
            MCP 快速开始 <ExternalLink className="h-3 w-3" />
          </ExternalAnchor>
          <ExternalAnchor
            href="https://github.com/modelcontextprotocol/servers"
            className="inline-flex items-center gap-1 rounded-md border border-border/60 px-2 py-1 text-[11px] font-medium text-foreground/80 hover:border-primary/40 hover:bg-card-hover"
          >
            官方 MCP 服务器目录 <ExternalLink className="h-3 w-3" />
          </ExternalAnchor>
        </div>
      </section>

      {/* MCP servers summary */}
      <section className="rounded-xl border border-border/55 bg-card/55 p-4">
        <div className="flex items-center justify-between gap-2">
          <h4 className="text-sm font-semibold text-foreground">MCP 服务器</h4>
          <span className="rounded-full bg-muted/50 px-2 py-0.5 text-[10px] font-medium text-muted-foreground">
            {editor.loading ? <Loader2 className="inline h-3 w-3 animate-spin" /> : `${serverCount} 个已配置`}
          </span>
        </div>

        {mcpServers && serverCount > 0 ? (
          <ul className="mt-3 space-y-1">
            {Object.entries(mcpServers).map(([name, def]) => {
              const command =
                typeof def === "object" && def !== null && "command" in def
                  ? String((def as { command: unknown }).command ?? "")
                  : "";
              return (
                <li key={name} className="flex items-baseline gap-2 truncate text-[11px]">
                  <span className="font-mono font-semibold text-foreground">{name}</span>
                  {command && <span className="truncate text-muted-foreground">— {command}</span>}
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="mt-3 text-[11px] text-muted-foreground/90">
            尚未添加任何 MCP 服务器。可点击下方「插入示例」获得一个文件系统 server 模板。
          </p>
        )}

        <div className="mt-3 flex flex-wrap gap-1.5">
          <Button type="button" size="sm" variant="outline" onClick={handleAddTemplate} disabled={editor.loading}>
            插入示例(filesystem)
          </Button>
        </div>
      </section>

      {/* JSON editor */}
      <section className="rounded-xl border border-border/55 bg-card/55 p-4">
        <div className="mb-2 flex items-center justify-between">
          <h4 className="text-sm font-semibold text-foreground">claude_desktop_config.json</h4>
          {activeFile ? (
            <p className="truncate font-mono text-[10px] text-muted-foreground" title={activeFile.path}>
              {activeFile.path}
            </p>
          ) : null}
        </div>

        {editor.loading ? (
          <div className="flex h-48 items-center justify-center rounded-lg border border-border/50 bg-background/40">
            <Loader2 className="h-5 w-5 animate-spin text-primary" />
          </div>
        ) : (
          <textarea
            value={editor.content}
            onChange={(e) => editor.setContent(e.target.value)}
            spellCheck={false}
            className={cn(
              "min-h-[240px] w-full resize-y rounded-lg border border-border/55 bg-background/50 p-2.5",
              "font-mono text-[11px] leading-5 text-foreground",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35",
            )}
            aria-label="Claude Desktop MCP 配置编辑器"
          />
        )}

        {parseError ? <p className="mt-1 text-[11px] text-destructive">JSON 解析失败,请修正语法。</p> : null}

        <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
          <Button
            type="button"
            size="sm"
            variant="default"
            onClick={() => void editor.save()}
            disabled={editor.saving || editor.loading}
          >
            {editor.saving ? <Loader2 className="h-3 w-3 animate-spin" /> : <Save className="h-3 w-3" />}
            保存
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void editor.formatContent()}
            disabled={editor.loading}
          >
            <Wand2 className="mr-1 h-3 w-3" />
            格式化
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void editor.reload()}
            disabled={editor.loading}
          >
            <RefreshCw className="mr-1 h-3 w-3" />
            重新加载
          </Button>
          {activeFile ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="ml-auto h-7 text-[11px]"
              onClick={() => {
                const dir = activeFile.path.replace(/\/[^/]+$/, "");
                void tauriInvoke("open_folder", { path: dir });
              }}
            >
              <FolderOpen className="mr-1 h-3 w-3" />
              文件夹
            </Button>
          ) : null}
        </div>

        {editor.dirty ? <p className="mt-1 text-[10px] text-amber-500">未保存</p> : null}
      </section>
    </div>
  );
}

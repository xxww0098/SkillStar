import { Boxes } from "lucide-react";
import { McpManager } from "../features/models/components/hub/McpManager";

/**
 * MCP mode page (Skills-mode sidebar entry).
 *
 * Hosts the unified MCP server manager: built-in recommended servers plus the
 * managed store, with one-click enable into each agent tool's native config.
 */
export function Mcp() {
  return (
    <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />
      <main className="ss-page-scroll">
        <div className="mx-auto w-full max-w-6xl px-6 py-6 space-y-6">
          <header>
            <h1 className="flex items-center gap-2 text-2xl font-bold tracking-tight text-foreground">
              <Boxes className="h-5 w-5 text-primary" />
              MCP 服务器
            </h1>
            <p className="mt-1 text-sm text-muted-foreground">
              统一管理 MCP 服务器,一处启用即写入 Claude Code / Codex / Gemini / OpenCode 等工具配置。
            </p>
          </header>

          <McpManager />
        </div>
      </main>
    </div>
  );
}

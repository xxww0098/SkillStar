import { Braces, FolderOpen, Loader2, RefreshCw, Save, Wand2, Zap } from "lucide-react";
import { memo, useCallback, useState } from "react";
import { Button } from "../../../components/ui/button";
import { tauriInvoke } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";
import { AGENT_TOOLS, type AgentToolId, useToolConfigFiles } from "../hooks/useToolConfigFiles";
import { AgentToolIcon } from "./AgentToolIcon";
import { providerCardClass } from "./providerForm/ProviderConfigPrimitives";

export interface ToolJsonConfigPanelProps {
  providerId: string;
  isToolActive: (toolId: string) => boolean;
  /** Render without outer card chrome (inside Agent 高级配置) */
  embedded?: boolean;
}

function ToolJsonConfigPanelInner({ providerId, isToolActive, embedded = false }: ToolJsonConfigPanelProps) {
  const [activeTool, setActiveTool] = useState<AgentToolId>("claude-code");
  const editor = useToolConfigFiles(activeTool);
  const activeFile = editor.files.find((f) => f.file_id === editor.activeFileId);

  const handlePush = useCallback(() => {
    if (!isToolActive(activeTool)) return;
    void editor.pushFromProvider(providerId);
  }, [activeTool, editor, isToolActive, providerId]);

  const body = (
    <>
      <div className={cn("flex flex-wrap gap-1.5", !embedded && "border-b border-border/45 px-4 py-2.5")}>
        {AGENT_TOOLS.map((tool) => (
          <button
            key={tool.toolId}
            type="button"
            onClick={() => setActiveTool(tool.toolId)}
            className={cn(
              "inline-flex items-center gap-1.5 rounded-md border px-2 py-0.5 text-[11px] font-medium transition-colors",
              activeTool === tool.toolId
                ? "border-primary/50 bg-primary/10 text-primary"
                : "border-border/55 text-muted-foreground hover:text-foreground",
            )}
          >
            <AgentToolIcon toolId={tool.toolId} size="sm" />
            {tool.label}
          </button>
        ))}
      </div>

      {editor.files.length > 1 && (
        <div className="flex flex-wrap gap-1 py-1.5">
          {editor.files.map((file) => (
            <button
              key={file.file_id}
              type="button"
              onClick={() => editor.setActiveFileId(file.file_id)}
              className={cn(
                "rounded px-1.5 py-0.5 font-mono text-[10px]",
                editor.activeFileId === file.file_id ? "bg-muted text-foreground" : "text-muted-foreground",
              )}
            >
              {file.label}
            </button>
          ))}
        </div>
      )}

      <div className="space-y-2">
        {activeFile && (
          <p className="truncate font-mono text-[10px] text-muted-foreground">
            {activeFile.path}
            <span className="ml-2 uppercase">{activeFile.format}</span>
          </p>
        )}

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
              "min-h-[200px] w-full resize-y rounded-lg border border-border/55 bg-background/50 p-2.5",
              "font-mono text-[11px] leading-5 text-foreground",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35",
            )}
            aria-label={`${activeTool} 配置编辑器`}
          />
        )}

        <div className="flex flex-wrap items-center gap-1.5">
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
            <Wand2 className="h-3 w-3" />
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => void editor.reload()}
            disabled={editor.loading}
          >
            <RefreshCw className="h-3 w-3" />
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={handlePush}
            disabled={!isToolActive(activeTool) || editor.loading}
            title={isToolActive(activeTool) ? "覆盖托管字段" : "请先启用该工具"}
          >
            <Zap className="h-3 w-3" />
            同步
          </Button>
          {activeFile && (
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
              <FolderOpen className="h-3 w-3" />
            </Button>
          )}
        </div>

        {editor.dirty && <p className="text-[10px] text-amber-500">未保存</p>}
        {!embedded && <p className="text-[11px] text-muted-foreground">启用工具同步后，SkillStar 托管字段会被覆盖。</p>}
      </div>
    </>
  );

  if (embedded) {
    return <div className="space-y-2">{body}</div>;
  }

  return (
    <section className={providerCardClass}>
      <div className="border-b border-border/45 px-4 py-3">
        <div className="flex items-center gap-2">
          <Braces className="h-4 w-4 text-primary" />
          <h3 className="text-sm font-semibold text-foreground">工具磁盘配置</h3>
        </div>
        <p className="mt-1 text-xs text-muted-foreground">Claude / Codex / OpenCode 本地配置文件</p>
      </div>
      <div className="space-y-3 px-4 py-3">{body}</div>
    </section>
  );
}

export const ToolJsonConfigPanel = memo(ToolJsonConfigPanelInner);

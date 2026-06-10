import { FolderOpen, Loader2, RefreshCw, Save, Wand2, Zap } from "lucide-react";
import { useCallback } from "react";
import { Button } from "../../../../components/ui/button";
import { tauriInvoke } from "../../../../lib/ipc";
import { cn } from "../../../../lib/utils";
import { type AgentToolId, useToolConfigFiles } from "../../api/configFiles";

export interface AgentConfigFilesProps {
  toolId: AgentToolId;
  /** Provider currently bound to this tool — enables 同步 (push managed fields). */
  activeProviderId?: string | null;
}

/**
 * On-disk config file editor for ONE tool (the old ToolJsonConfigPanel without
 * the tool tab row — each agent settings dialog edits its own files only).
 */
export function AgentConfigFiles({ toolId, activeProviderId }: AgentConfigFilesProps) {
  const editor = useToolConfigFiles(toolId);
  const activeFile = editor.files.find((f) => f.file_id === editor.activeFileId);

  const handlePush = useCallback(() => {
    if (!activeProviderId) return;
    void editor.pushFromProvider(activeProviderId);
  }, [editor, activeProviderId]);

  return (
    <div className="space-y-2">
      {editor.files.length > 1 && (
        <div className="flex flex-wrap gap-1">
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

      {activeFile && (
        <p className="truncate font-mono text-[10px] text-muted-foreground">
          {activeFile.path}
          <span className="ml-2 uppercase">{activeFile.format}</span>
        </p>
      )}

      {editor.loading ? (
        <div className="flex h-40 items-center justify-center rounded-lg border border-border/50 bg-background/40">
          <Loader2 className="h-5 w-5 animate-spin text-primary" />
        </div>
      ) : (
        <textarea
          value={editor.content}
          onChange={(e) => editor.setContent(e.target.value)}
          spellCheck={false}
          className={cn(
            "min-h-[180px] w-full resize-y rounded-lg border border-border/55 bg-background/50 p-2.5",
            "font-mono text-[11px] leading-5 text-foreground",
            "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35",
          )}
          aria-label={`${toolId} 配置编辑器`}
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
          title="格式化"
        >
          <Wand2 className="h-3 w-3" />
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => void editor.reload()}
          disabled={editor.loading}
          title="重载"
        >
          <RefreshCw className="h-3 w-3" />
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={handlePush}
          disabled={!activeProviderId || editor.loading}
          title={activeProviderId ? "用当前绑定覆盖托管字段" : "请先接入该 Agent"}
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
              const dir = activeFile.path.replace(/[/\\][^/\\]+$/, "");
              void tauriInvoke("open_folder", { path: dir });
            }}
          >
            <FolderOpen className="h-3 w-3" />
            打开目录
          </Button>
        )}
      </div>

      {editor.dirty && <p className="text-[10px] text-amber-500">未保存</p>}
    </div>
  );
}

import { FolderOpen, Loader2, RefreshCw, Save, Wand2, Zap } from "lucide-react";
import { useCallback, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { tauriInvoke } from "../../../../lib/ipc";
import { cn } from "../../../../lib/utils";
import { type AgentToolId, useToolConfigFiles } from "../../api/configFiles";
import {
  ModelSegmentedControl,
  modelControlSurfaceClass,
  modelTextareaClass,
} from "../providerForm/ProviderConfigPrimitives";

export interface AgentConfigFilesProps {
  toolId: AgentToolId;
  /** Provider currently bound to this tool — enables sync (push managed fields). */
  activeProviderId?: string | null;
  onDirtyChange?: (dirty: boolean) => void;
}

/**
 * On-disk config file editor for ONE tool (the old ToolJsonConfigPanel without
 * the tool tab row — each agent settings dialog edits its own files only).
 */
export function AgentConfigFiles({ toolId, activeProviderId, onDirtyChange }: AgentConfigFilesProps) {
  const { t } = useTranslation();
  const editor = useToolConfigFiles(toolId);
  const activeFile = editor.files.find((f) => f.file_id === editor.activeFileId);

  useEffect(() => {
    onDirtyChange?.(editor.dirty);
  }, [editor.dirty, onDirtyChange]);

  const handlePush = useCallback(() => {
    if (!activeProviderId) return;
    void editor.pushFromProvider(activeProviderId);
  }, [editor, activeProviderId]);

  return (
    <div className="space-y-2">
      {editor.files.length > 1 && (
        <ModelSegmentedControl
          value={editor.activeFileId}
          onChange={editor.setActiveFileId}
          disabled={editor.dirty || editor.loading}
          ariaLabel={t("models.configFiles.filePicker")}
          options={editor.files.map((file) => ({
            value: file.file_id,
            label: <span className="font-mono">{file.label}</span>,
          }))}
        />
      )}

      {activeFile && (
        <p className="truncate font-mono text-[10px] text-muted-foreground">
          {activeFile.path}
          <span className="ml-2 uppercase">{activeFile.format}</span>
        </p>
      )}

      {editor.loading ? (
        <div className={cn(modelControlSurfaceClass, "flex h-48 items-center justify-center rounded-[10px]")}>
          <Loader2 className="h-5 w-5 animate-spin text-primary" />
        </div>
      ) : (
        <textarea
          value={editor.content}
          onChange={(e) => editor.setContent(e.target.value)}
          spellCheck={false}
          className={cn(modelTextareaClass, "min-h-[220px] font-mono text-xs leading-5")}
          aria-label={t("models.configFiles.editorAria", { toolId })}
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
          {t("models.configFiles.save")}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => void editor.formatContent()}
          disabled={editor.loading || editor.dirty}
          title={t("models.configFiles.format")}
          aria-label={t("models.configFiles.format")}
        >
          <Wand2 className="h-3 w-3" />
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => void editor.reload()}
          disabled={editor.loading || editor.dirty}
          title={t("models.configFiles.reload")}
          aria-label={t("models.configFiles.reload")}
        >
          <RefreshCw className="h-3 w-3" />
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={handlePush}
          disabled={!activeProviderId || editor.loading || editor.dirty}
          title={activeProviderId ? t("models.configFiles.pushTitle") : t("models.configFiles.pushDisabledTitle")}
        >
          <Zap className="h-3 w-3" />
          {t("models.configFiles.sync")}
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
            {t("models.configFiles.openFolder")}
          </Button>
        )}
      </div>

      {editor.dirty && (
        <p className="text-[11px] leading-4 text-amber-500" role="status">
          {t("models.configFiles.unsavedHint")}
        </p>
      )}
    </div>
  );
}

import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { tauriInvoke } from "../../../lib/ipc";
import type { ToolConfigFileInfo, WriteToolConfigFileResult } from "../../../types";

import { type AgentToolId, CONFIG_FILE_TOOLS } from "../lib/agentRegistry";

export type { AgentToolId } from "../lib/agentRegistry";
export const AGENT_TOOLS = CONFIG_FILE_TOOLS;

export function useToolConfigFiles(toolId: AgentToolId) {
  const [files, setFiles] = useState<ToolConfigFileInfo[]>([]);
  const [activeFileId, setActiveFileId] = useState<string>("");
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  const loadFiles = useCallback(async () => {
    const list = await tauriInvoke("list_tool_config_files", { toolId });
    setFiles(list);
    setActiveFileId((prev) => {
      if (list.length === 0) return prev;
      if (list.some((f) => f.file_id === prev)) return prev;
      return list[0].file_id;
    });
    return list;
  }, [toolId]);

  const loadContent = useCallback(
    async (fileId: string) => {
      setLoading(true);
      try {
        const text = await tauriInvoke("read_tool_config_file", { toolId, fileId });
        setContent(text);
        setDirty(false);
      } finally {
        setLoading(false);
      }
    },
    [toolId],
  );

  useEffect(() => {
    void loadFiles();
  }, [loadFiles]);

  useEffect(() => {
    if (!activeFileId) return;
    void loadContent(activeFileId);
  }, [activeFileId, loadContent]);

  const save = useCallback(async (): Promise<WriteToolConfigFileResult> => {
    if (!activeFileId) {
      return { success: false, error: "未选择配置文件" };
    }
    setSaving(true);
    try {
      const result = await tauriInvoke("write_tool_config_file", {
        toolId,
        fileId: activeFileId,
        content,
      });
      if (result.success) {
        setDirty(false);
        toast.success("配置已保存");
        await loadFiles();
      } else {
        toast.error(result.error ?? "保存失败");
      }
      return result;
    } finally {
      setSaving(false);
    }
  }, [toolId, activeFileId, content, loadFiles]);

  const formatContent = useCallback(async () => {
    if (!activeFileId) return;
    try {
      const formatted = await tauriInvoke("format_tool_config_file", {
        toolId,
        fileId: activeFileId,
      });
      setContent(formatted);
      setDirty(true);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`格式化失败：${message}`);
    }
  }, [toolId, activeFileId]);

  const reload = useCallback(async () => {
    await loadFiles();
    if (activeFileId) await loadContent(activeFileId);
  }, [loadFiles, activeFileId, loadContent]);

  const pushFromProvider = useCallback(
    async (providerId: string) => {
      try {
        const result = await tauriInvoke("push_provider_to_tool_config", {
          providerId,
          toolId,
        });
        if (result.success) {
          toast.success("已从供应商同步到配置文件");
          await reload();
        } else {
          toast.error(result.error ?? "同步失败");
        }
        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        toast.error(message);
        throw err;
      }
    },
    [toolId, reload],
  );

  return {
    files,
    activeFileId,
    setActiveFileId,
    content,
    setContent: (value: string) => {
      setContent(value);
      setDirty(true);
    },
    loading,
    saving,
    dirty,
    save,
    formatContent,
    reload,
    pushFromProvider,
  };
}

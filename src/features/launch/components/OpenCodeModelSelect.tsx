import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import type { PaneNode } from "../hooks/useLaunchConfig";

/** Model selector for OpenCode — fetches all available models via `opencode models` CLI */
export const OpenCodeModelSelect = memo(function OpenCodeModelSelect({
  pane,
  onAssign,
}: {
  pane: PaneNode;
  onAssign: (paneId: string, agentId: string, providerId?: string, providerName?: string, modelId?: string) => void;
}) {
  const cacheKey = "cached_opencode_launch_models";

  const [models, setModels] = useState<string[]>(() => {
    try {
      const cached = localStorage.getItem(cacheKey);
      if (cached) return JSON.parse(cached);
    } catch {
      // ignore
    }
    return [];
  });
  const [loading, setLoading] = useState(false);

  // Auto-fetch on first mount if no cache
  useEffect(() => {
    if (models.length === 0) {
      fetchModels();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const fetchModels = useCallback(
    async () => {
      setLoading(true);
      try {
        const result = await invoke<string[]>("get_opencode_cli_models");
        setModels(result);
        try {
          localStorage.setItem(cacheKey, JSON.stringify(result));
        } catch {
          // ignore
        }
      } catch (err) {
        console.error("Failed to fetch OpenCode models:", err);
      } finally {
        setLoading(false);
      }
    },
    [],
  );

  return (
    <div className="flex items-center gap-0.5 mt-0.5">
      <select
        className="text-[10px] bg-background/80 border border-border/50 rounded-md px-1.5 py-0.5 text-muted-foreground outline-none cursor-pointer hover:border-purple-400/40 focus:border-purple-400/60 transition-colors shadow-sm backdrop-blur-md appearance-none text-center min-w-[90px] max-w-[120px] truncate"
        value={pane.modelId || ""}
        onChange={(e) => {
          onAssign(pane.id, pane.agentId, pane.providerId, pane.providerName, e.target.value);
        }}
      >
        <option value="">默认模型</option>
        {/* Ensure current value is shown even if not in fetched list */}
        {pane.modelId && !models.includes(pane.modelId) && (
          <option value={pane.modelId}>{pane.modelId}</option>
        )}
        {models.map((m) => (
          <option key={m} value={m}>
            {m}
          </option>
        ))}
      </select>
      <button
        type="button"
        onClick={() => fetchModels()}
        disabled={loading}
        title="刷新可用模型列表"
        className="flex items-center justify-center p-0.5 rounded hover:bg-purple-500/10 text-muted-foreground hover:text-purple-400 transition-colors disabled:opacity-50 cursor-pointer"
      >
        <RefreshCw className={`w-2.5 h-2.5 ${loading ? "animate-spin" : ""}`} />
      </button>
    </div>
  );
});

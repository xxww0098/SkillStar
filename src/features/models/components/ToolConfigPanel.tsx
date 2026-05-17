import { CheckCircle2, FolderSync, RefreshCw, XCircle } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { AppId, ToolConfigTarget, ToolSyncResult } from "../../../types";
import { useProviders } from "../hooks/useProviders";
import { useToolConfigs } from "../hooks/useToolConfigs";

export interface ToolConfigPanelProps {
  appId: AppId;
  providerId: string;
}

/** Per-tool row showing config path, existence status, sync button, and result. */
function ToolRow({
  target,
  result,
  isSyncing,
  onSync,
}: {
  target: ToolConfigTarget;
  result?: ToolSyncResult;
  isSyncing: boolean;
  onSync: () => void;
}) {
  return (
    <div
      className={cn(
        "flex flex-col gap-2 p-3 rounded-lg border transition-colors",
        "bg-card/60 backdrop-blur-sm border-border/40",
      )}
    >
      {/* Tool name + existence status */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          {target.exists ? (
            <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
          ) : (
            <XCircle className="w-4 h-4 text-red-400 shrink-0" />
          )}
          <span className="text-sm font-medium text-foreground truncate">{target.display_name}</span>
        </div>
        <Button variant="ghost" size="sm" onClick={onSync} disabled={isSyncing} className="shrink-0 h-7 px-2.5">
          <RefreshCw className={cn("w-3.5 h-3.5 mr-1.5", isSyncing && "animate-spin")} />
          Sync
        </Button>
      </div>

      {/* Config path */}
      <span className="text-xs text-muted-foreground font-mono truncate pl-6">{target.config_path}</span>

      {/* Sync result (if any) */}
      {result && (
        <div className="pl-6">
          {result.success ? (
            <div className="flex items-center gap-1.5">
              <Badge variant="success" className="text-micro px-1.5 py-0 h-4 font-medium">
                Synced
              </Badge>
              {result.backup_path && (
                <span className="text-xs text-muted-foreground truncate">backup: {result.backup_path}</span>
              )}
            </div>
          ) : (
            <div className="flex items-center gap-1.5">
              <Badge variant="destructive" className="text-micro px-1.5 py-0 h-4 font-medium">
                Failed
              </Badge>
              {result.error && <span className="text-xs text-red-400 truncate">{result.error}</span>}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function ToolConfigPanelInner({ appId, providerId }: ToolConfigPanelProps) {
  const { targets, isLoading, syncToTool, syncToAll, recheckTargets } = useToolConfigs(appId, providerId);
  const { current } = useProviders(appId);

  const [syncResults, setSyncResults] = useState<Record<string, ToolSyncResult>>({});
  const [syncingToolId, setSyncingToolId] = useState<string | null>(null);
  const [isSyncingAll, setIsSyncingAll] = useState(false);

  // Re-check targets on mount
  useEffect(() => {
    recheckTargets();
  }, [recheckTargets]);

  const handleSyncTool = useCallback(
    async (toolId: string) => {
      setSyncingToolId(toolId);
      try {
        const result = await syncToTool(toolId);
        setSyncResults((prev) => ({ ...prev, [toolId]: result }));
      } finally {
        setSyncingToolId(null);
      }
    },
    [syncToTool],
  );

  const handleSyncAll = useCallback(async () => {
    setIsSyncingAll(true);
    try {
      const results = await syncToAll();
      const mapped: Record<string, ToolSyncResult> = {};
      for (const r of results) {
        mapped[r.tool_id] = r;
      }
      setSyncResults(mapped);
    } finally {
      setIsSyncingAll(false);
    }
  }, [syncToAll]);

  return (
    <div className={cn("flex flex-col gap-4 p-5 rounded-xl border", "bg-card/80 backdrop-blur-sm border-border/60")}>
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <FolderSync className="w-4.5 h-4.5 text-primary" />
          <h3 className="text-sm font-semibold text-foreground">Tool Configurations</h3>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={handleSyncAll}
          disabled={isSyncingAll || isLoading}
          className="h-7 px-3"
        >
          <RefreshCw className={cn("w-3.5 h-3.5 mr-1.5", isSyncingAll && "animate-spin")} />
          Sync All
        </Button>
      </div>

      {/* Active provider info */}
      {current && (
        <div className="text-xs text-muted-foreground">
          Syncing from: <span className="text-foreground font-medium">{current.name}</span>
        </div>
      )}

      {/* Tool list */}
      {isLoading ? (
        <div className="text-xs text-muted-foreground py-4 text-center">Loading tool configs...</div>
      ) : targets.length === 0 ? (
        <div className="text-xs text-muted-foreground py-4 text-center">No supported tools found.</div>
      ) : (
        <div className="flex flex-col gap-2">
          {targets.map((target) => (
            <ToolRow
              key={target.tool_id}
              target={target}
              result={syncResults[target.tool_id]}
              isSyncing={syncingToolId === target.tool_id || isSyncingAll}
              onSync={() => handleSyncTool(target.tool_id)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export const ToolConfigPanel = memo(ToolConfigPanelInner);

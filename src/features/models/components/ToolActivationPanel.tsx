import { ChevronDown, Copy, ExternalLink, RefreshCw, Terminal } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { Button } from "../../../components/ui/button";
import { Switch } from "../../../components/ui/switch";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { tauriInvoke } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";
import type { ModelCatalogEntry, ToolSyncResult } from "../../../types";
import { useToolActivations } from "../hooks/useToolActivations";
import { AgentToolIcon, type AgentToolIconId } from "./shared/AgentToolIcon";
import { PROVIDER_AGENTS } from "../lib/agentRegistry";
import { buildClaudeLaunchCommand, type ClaudeCommandShell } from "../lib/launchCommand";
import { formatModelMetadata, formatModelOptionLabel, formatSyncTime } from "../lib/modelFormat";

export interface ToolActivationPanelProps {
  providerId: string;
  providerModels: string[];
  defaultModel: string;
  baseUrlOpenai: string;
  baseUrlAnthropic: string;
  modelCatalog?: ModelCatalogEntry[];
  showHeader?: boolean;
  defaultExpanded?: boolean;
  /** Flatter rows for Agent 高级配置 */
  variant?: "default" | "compact";
}

interface ToolItemProps {
  toolId: string;
  displayName: string;
  isActive: boolean;
  selectedModel: string | null;
  lastSyncAt: string | null;
  configPath: string | null;
  providerModels: string[];
  modelCatalog: ModelCatalogEntry[];
  defaultModel: string;
  onToggle: (toolId: string, model?: string) => Promise<ToolSyncResult | void>;
  /** Whether the tool is installed on the system. */
  isInstalled: boolean;
  /** Whether installation detection is still loading. */
  installLoading: boolean;
  /** Whether the required base_url for this tool is empty. */
  urlMissing: boolean;
  /** Tooltip text when disabled due to missing URL. */
  disabledTooltip: string;
  /** Link to installation documentation. */
  installDocsUrl: string;
  defaultExpanded?: boolean;
  compact?: boolean;
}

function ToolItem({
  toolId,
  displayName,
  isActive,
  selectedModel,
  lastSyncAt,
  configPath,
  providerModels,
  modelCatalog,
  defaultModel,
  onToggle,
  isInstalled,
  installLoading,
  urlMissing,
  disabledTooltip,
  installDocsUrl,
  defaultExpanded = false,
  compact = false,
}: ToolItemProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [localModel, setLocalModel] = useState(selectedModel || defaultModel);
  const [isSyncing, setIsSyncing] = useState(false);
  const [claudeCommandShell, setClaudeCommandShell] = useState<ClaudeCommandShell>("unix");

  // Determine if the toggle should be disabled
  const isDisabled = isSyncing || !isInstalled || urlMissing || installLoading;

  const handleToggle = useCallback(
    async (checked: boolean) => {
      if (isDisabled) return;
      setIsSyncing(true);
      try {
        await onToggle(toolId, checked ? localModel : undefined);
      } finally {
        setIsSyncing(false);
      }
    },
    [toolId, localModel, onToggle, isDisabled],
  );

  const handleResync = useCallback(async () => {
    if (!isActive) return;
    setIsSyncing(true);
    try {
      await onToggle(toolId, localModel);
    } finally {
      setIsSyncing(false);
    }
  }, [toolId, localModel, isActive, onToggle]);

  const selectedMetadata = modelCatalog.find((entry) => entry.id === localModel);
  const claudeCommand = toolId === "claude-code" ? buildClaudeLaunchCommand(localModel, claudeCommandShell) : "";

  const handleCopyClaudeCommand = useCallback(async () => {
    if (!claudeCommand || typeof navigator === "undefined" || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(claudeCommand);
    } catch {
      // Clipboard access can be unavailable in test runners or locked-down shells.
    }
  }, [claudeCommand]);

  // Compute status text based on state
  let statusText: string;
  if (!isInstalled && !installLoading) {
    statusText = "○ 未安装";
  } else if (urlMissing) {
    statusText = "○ 未启用";
  } else if (isActive) {
    statusText = `● 已启用 · ${selectedModel || defaultModel}`;
  } else {
    statusText = "○ 未启用";
  }

  // Compute tooltip for the toggle
  let toggleTooltip: string | undefined;
  if (!isInstalled && !installLoading) {
    toggleTooltip = `${displayName} 未安装，请先安装`;
  } else if (urlMissing) {
    toggleTooltip = disabledTooltip;
  }

  return (
    <div
      className={cn(
        compact
          ? "rounded-lg border"
          : "rounded-2xl border shadow-sm transition duration-200 hover:-translate-y-0.5 hover:shadow-[0_16px_36px_-28px_var(--color-shadow)]",
        isActive ? "border-primary/30 bg-primary/10" : "border-border/55 bg-card/55",
        !compact && !isActive && "hover:border-primary/25 hover:bg-card/75",
        !isInstalled && !installLoading && "opacity-60",
      )}
    >
      {/* Collapsed header */}
      <div
        className="flex cursor-pointer select-none items-center gap-2.5 px-3.5 py-3"
        onClick={() => setExpanded((prev) => !prev)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            setExpanded((prev) => !prev);
          }
        }}
        role="button"
        tabIndex={0}
        aria-expanded={expanded}
        aria-label={`${displayName} 工具面板`}
      >
        <AgentToolIcon
          toolId={toolId as AgentToolIconId}
          size="sm"
          className={cn(!isInstalled && !installLoading && "opacity-70")}
        />

        {/* Tool name + status */}
        <div className="flex-1 min-w-0">
          <span className="text-sm font-medium text-foreground">{displayName}</span>
          <p
            className={cn(
              "text-[11px] truncate",
              isActive ? "text-emerald-500" : "text-muted-foreground",
              !isInstalled && !installLoading && "text-amber-500",
            )}
          >
            {statusText}
          </p>
        </div>

        {/* Toggle switch with tooltip for disabled states */}
        <span title={toggleTooltip}>
          <Switch
            checked={isActive}
            onCheckedChange={handleToggle}
            disabled={isDisabled}
            onClick={(e) => e.stopPropagation()}
            aria-label={`${isActive ? "停用" : "启用"} ${displayName}`}
          />
        </span>

        {/* Expand chevron */}
        <ChevronDown
          className={cn(
            "w-3.5 h-3.5 text-muted-foreground transition-transform duration-200",
            expanded && "rotate-180",
          )}
        />
      </div>

      {/* Not installed banner */}
      {!isInstalled && !installLoading && expanded && (
        <div className="px-3 pb-2 pt-1 border-t border-border/30">
          <div className="flex items-center gap-1.5 text-[11px] text-amber-500">
            <span>未检测到 {displayName}，请先安装</span>
            <ExternalAnchor
              href={installDocsUrl}
              className="inline-flex items-center gap-0.5 text-primary hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              安装文档
              <ExternalLink className="w-3 h-3" />
            </ExternalAnchor>
          </div>
        </div>
      )}

      {/* URL missing tooltip banner */}
      {isInstalled && urlMissing && expanded && (
        <div className="px-3 pb-2 pt-1 border-t border-border/30">
          <p className="text-[11px] text-amber-500">{disabledTooltip}</p>
        </div>
      )}

      {/* Expanded content */}
      {expanded && isInstalled && !urlMissing && (
        <div className="space-y-3 border-t border-border/30 px-3.5 pb-3.5 pt-3">
          {/* Model selector */}
          <div className="space-y-1">
            <label className="text-[11px] font-medium text-muted-foreground">使用模型</label>
            <select
              value={localModel}
              onChange={(e) => setLocalModel(e.target.value)}
              disabled={isSyncing}
              className={cn(
                "w-full h-8 px-2.5 rounded-lg border border-input-border bg-input text-xs text-foreground appearance-none cursor-pointer",
                "focus:outline-none focus:ring-2 focus:ring-primary/40 focus:border-primary/60 transition duration-200",
                "disabled:cursor-not-allowed disabled:opacity-50",
              )}
            >
              {providerModels.length === 0 && <option value="">无可用模型</option>}
              {providerModels.map((model) => (
                <option key={model} value={model}>
                  {formatModelOptionLabel(
                    model,
                    modelCatalog.find((entry) => entry.id === model),
                  )}
                </option>
              ))}
            </select>
          </div>

          {selectedMetadata && (
            <div className="rounded-lg border border-border/40 bg-background/35 px-2.5 py-2">
              <p className="truncate text-[11px] font-medium text-foreground">
                {selectedMetadata.display_name || selectedMetadata.id}
              </p>
              <p className="mt-1 text-[11px] text-muted-foreground">{formatModelMetadata(selectedMetadata)}</p>
            </div>
          )}

          {toolId === "claude-code" && localModel && (
            <div className="space-y-2 rounded-lg border border-border/40 bg-background/35 px-2.5 py-2">
              <div className="flex items-center gap-1.5">
                {(["unix", "powershell"] as const).map((shell) => (
                  <button
                    key={shell}
                    type="button"
                    onClick={() => setClaudeCommandShell(shell)}
                    className={cn(
                      "h-6 rounded-md border px-2 text-[11px] font-medium transition-colors",
                      claudeCommandShell === shell
                        ? "border-primary/45 bg-primary/10 text-primary"
                        : "border-border/50 text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {shell === "unix" ? "macOS / Linux" : "Windows"}
                  </button>
                ))}
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={handleCopyClaudeCommand}
                  className="ml-auto h-6 px-2 text-[11px]"
                >
                  <Copy className="mr-1 h-3 w-3" />
                  复制命令
                </Button>
              </div>
              <pre className="max-h-28 overflow-auto whitespace-pre-wrap rounded-md bg-muted/40 p-2 font-mono text-[10px] leading-relaxed text-muted-foreground">
                {claudeCommand}
              </pre>
            </div>
          )}

          {/* Config path */}
          {configPath && (
            <div className="space-y-0.5">
              <span className="text-[11px] font-medium text-muted-foreground">配置路径</span>
              <p className="text-[11px] text-muted-foreground/80 font-mono truncate">{configPath}</p>
            </div>
          )}

          {/* Last sync */}
          <div className="space-y-0.5">
            <span className="text-[11px] font-medium text-muted-foreground">上次同步</span>
            <p className="text-[11px] text-muted-foreground/80">
              {lastSyncAt ? formatSyncTime(lastSyncAt) : "从未同步"}
            </p>
          </div>

          {/* Re-sync button */}
          <Button
            variant="outline"
            size="sm"
            onClick={handleResync}
            disabled={!isActive || isSyncing}
            className="w-full h-7 text-xs"
          >
            <RefreshCw className={cn("w-3 h-3 mr-1.5", isSyncing && "animate-spin")} />
            重新写入
          </Button>
        </div>
      )}
    </div>
  );
}

/** Tool installation status per tool_id. */
interface ToolInstallStatus {
  installed: boolean;
  binary_found: boolean;
  config_dir_found: boolean;
}

function ToolActivationPanelInner({
  providerId,
  providerModels,
  defaultModel,
  baseUrlOpenai,
  baseUrlAnthropic,
  modelCatalog = [],
  showHeader = true,
  defaultExpanded = false,
  variant = "default",
}: ToolActivationPanelProps) {
  const compact = variant === "compact";
  const { activations, isActive, toggle, isLoading } = useToolActivations(providerId);
  const [installStatus, setInstallStatus] = useState<Record<string, ToolInstallStatus>>({});
  const [installLoading, setInstallLoading] = useState(true);

  // Detect tool installation status on mount and when navigating to this panel
  useEffect(() => {
    let cancelled = false;

    async function detectInstallation() {
      setInstallLoading(true);
      const results: Record<string, ToolInstallStatus> = {};

      for (const tool of PROVIDER_AGENTS) {
        try {
          const result = await tauriInvoke("detect_tool_installation", {
            toolId: tool.toolId,
          });
          if (!cancelled) {
            results[tool.toolId] = result;
          }
        } catch {
          // If detection fails, assume installed to avoid blocking the user
          if (!cancelled) {
            results[tool.toolId] = { installed: true, binary_found: false, config_dir_found: false };
          }
        }
      }

      if (!cancelled) {
        setInstallStatus(results);
        setInstallLoading(false);
      }
    }

    detectInstallation();
    return () => {
      cancelled = true;
    };
  }, []);

  if (isLoading) {
    return (
      <div className="space-y-3">
        {showHeader && (
          <div className="mb-2 flex items-center gap-2">
            <Terminal className="h-4 w-4 text-primary" />
            <h4 className="text-xs font-semibold text-foreground">Agent 工具</h4>
          </div>
        )}
        <div className="text-xs text-muted-foreground py-3 text-center">加载中...</div>
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {showHeader && (
        <div className="mb-2 flex items-center gap-2">
          <Terminal className="h-4 w-4 text-primary" />
          <h4 className="text-xs font-semibold text-foreground">Agent 工具</h4>
        </div>
      )}

      <div className="space-y-2">
        {PROVIDER_AGENTS.map((tool) => {
          const activation = activations[tool.toolId];
          const toolIsActive = isActive(tool.toolId);
          const selectedModel = toolIsActive ? activation?.model || null : null;
          const lastSyncAt =
            toolIsActive && activation?.last_sync_at != null
              ? new Date(activation.last_sync_at * 1000).toISOString()
              : null;

          // Determine if the required URL is missing for this tool
          const urlMissing = tool.requiredUrlField === "anthropic" ? !baseUrlAnthropic : !baseUrlOpenai;

          // Get installation status
          const toolInstall = installStatus[tool.toolId];
          const isToolInstalled = installLoading ? true : (toolInstall?.installed ?? true);

          return (
            <ToolItem
              key={tool.toolId}
              toolId={tool.toolId}
              displayName={tool.displayName}
              isActive={toolIsActive}
              selectedModel={selectedModel}
              lastSyncAt={lastSyncAt}
              configPath={tool.configPathDisplay}
              providerModels={providerModels}
              modelCatalog={modelCatalog}
              defaultModel={defaultModel}
              onToggle={toggle}
              isInstalled={isToolInstalled}
              installLoading={installLoading}
              urlMissing={urlMissing}
              disabledTooltip={tool.disabledTooltip}
              installDocsUrl={tool.installDocsUrl}
              defaultExpanded={defaultExpanded}
              compact={compact}
            />
          );
        })}
      </div>
    </div>
  );
}

export const ToolActivationPanel = memo(ToolActivationPanelInner);

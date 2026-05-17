import { ChevronDown, ExternalLink, RefreshCw, Terminal } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { Button } from "../../../components/ui/button";
import { Switch } from "../../../components/ui/switch";
import { tauriInvoke } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";
import type { ToolSyncResult } from "../../../types";
import { useToolActivations } from "../hooks/useToolActivations";

export interface ToolActivationPanelProps {
  providerId: string;
  providerModels: string[];
  defaultModel: string;
  baseUrlOpenai: string;
  baseUrlAnthropic: string;
  showHeader?: boolean;
  defaultExpanded?: boolean;
}

/** Known agent tools that can be activated. */
const KNOWN_TOOLS = [
  {
    toolId: "claude-code",
    displayName: "Claude Code",
    icon: "C",
    requiredUrlField: "anthropic" as const,
    disabledTooltip: "此供应商未提供 Anthropic 兼容端点",
    installDocsUrl: "https://docs.anthropic.com/en/docs/claude-code/overview",
  },
  {
    toolId: "codex",
    displayName: "Codex",
    icon: "X",
    requiredUrlField: "openai" as const,
    disabledTooltip: "此供应商未提供 OpenAI 兼容端点",
    installDocsUrl: "https://github.com/openai/codex",
  },
] as const;

interface ToolItemProps {
  toolId: string;
  displayName: string;
  icon: string;
  isActive: boolean;
  selectedModel: string | null;
  lastSyncAt: string | null;
  configPath: string | null;
  providerModels: string[];
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
}

function ToolItem({
  toolId,
  displayName,
  icon,
  isActive,
  selectedModel,
  lastSyncAt,
  configPath,
  providerModels,
  defaultModel,
  onToggle,
  isInstalled,
  installLoading,
  urlMissing,
  disabledTooltip,
  installDocsUrl,
  defaultExpanded = false,
}: ToolItemProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [localModel, setLocalModel] = useState(selectedModel || defaultModel);
  const [isSyncing, setIsSyncing] = useState(false);

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
        "rounded-2xl border shadow-sm transition duration-200 hover:-translate-y-0.5 hover:shadow-[0_16px_36px_-28px_var(--color-shadow)]",
        isActive
          ? "border-primary/30 bg-primary/10 hover:border-primary/40"
          : "border-border/55 bg-card/55 hover:border-primary/25 hover:bg-card/75",
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
        {/* Tool icon */}
        <span
          className={cn(
            "flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-xs font-bold",
            isActive ? "bg-primary/20 text-primary" : "bg-muted text-muted-foreground",
          )}
        >
          {icon}
        </span>

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
            <a
              href={installDocsUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-0.5 text-primary hover:underline"
              onClick={(e) => e.stopPropagation()}
            >
              安装文档
              <ExternalLink className="w-3 h-3" />
            </a>
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
                  {model}
                </option>
              ))}
            </select>
          </div>

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

function formatSyncTime(timestamp: string): string {
  try {
    const date = new Date(timestamp);
    if (Number.isNaN(date.getTime())) return timestamp;
    const now = new Date();
    const diffMs = now.getTime() - date.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return "刚刚";
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour} 小时前`;
    return date.toLocaleDateString("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return timestamp;
  }
}

/** Config paths for known tools (display only). */
const TOOL_CONFIG_PATHS: Record<string, string> = {
  "claude-code": "~/.claude/settings.json",
  codex: "~/.codex/config.toml",
};

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
  showHeader = true,
  defaultExpanded = false,
}: ToolActivationPanelProps) {
  const { activations, isActive, toggle, isLoading } = useToolActivations(providerId);
  const [installStatus, setInstallStatus] = useState<Record<string, ToolInstallStatus>>({});
  const [installLoading, setInstallLoading] = useState(true);

  // Detect tool installation status on mount and when navigating to this panel
  useEffect(() => {
    let cancelled = false;

    async function detectInstallation() {
      setInstallLoading(true);
      const results: Record<string, ToolInstallStatus> = {};

      for (const tool of KNOWN_TOOLS) {
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
        {KNOWN_TOOLS.map((tool) => {
          const activation = activations[tool.toolId];
          const toolIsActive = isActive(tool.toolId);
          const selectedModel = toolIsActive ? activation?.model || null : null;
          const lastSyncAt = null; // TODO: fetch from backend when available

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
              icon={tool.icon}
              isActive={toolIsActive}
              selectedModel={selectedModel}
              lastSyncAt={lastSyncAt}
              configPath={TOOL_CONFIG_PATHS[tool.toolId] ?? null}
              providerModels={providerModels}
              defaultModel={defaultModel}
              onToggle={toggle}
              isInstalled={isToolInstalled}
              installLoading={installLoading}
              urlMissing={urlMissing}
              disabledTooltip={tool.disabledTooltip}
              installDocsUrl={tool.installDocsUrl}
              defaultExpanded={defaultExpanded}
            />
          );
        })}
      </div>
    </div>
  );
}

export const ToolActivationPanel = memo(ToolActivationPanelInner);

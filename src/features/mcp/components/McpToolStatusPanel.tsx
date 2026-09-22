import { Check, CircleSlash, Copy, ExternalLink, RefreshCw } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { StatusChip } from "../../../components/ui/StatusChip";
import { LobeIcon } from "../../../components/ui/icons/LobeIcon";
import { getAgentIcon } from "../../../components/ui/icons/agentIcons";
import { useAgentProfiles } from "../../../hooks/useAgentProfiles";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import { cn, copyToClipboard } from "../../../lib/utils";
import { MCP_TOOL_IDS } from "../../../types";
import { mcpIconAgentIdForTool, mcpToolIdsWithoutAgentProfile } from "../lib/agentTargets";
import { useMcpToolStatuses, type McpToolStatusRow } from "../hooks/useMcpToolStatuses";
import { MCP_TOOL_LABELS } from "../lib/toolRegistry";

function formatDisplayPath(path: string): string {
  if (!path) return "";
  return path.replace(/^(\/Users\/[^/]+|\/home\/[^/]+|[A-Za-z]:\\Users\\[^\\]+)/, "~");
}

function getDirectoryPath(filePath: string): string {
  if (!filePath) return "";
  const lastSlash = Math.max(filePath.lastIndexOf("/"), filePath.lastIndexOf("\\"));
  return lastSlash > 0 ? filePath.slice(0, lastSlash) : filePath;
}

function McpToolStatusItem({ status, unreachable }: { status: McpToolStatusRow; unreachable: boolean }) {
  const { t } = useTranslation();
  const [hasCopied, setHasCopied] = useState(false);
  const BrandIcon = getAgentIcon(mcpIconAgentIdForTool(status.toolId));
  const displayName = status.label || MCP_TOOL_LABELS[status.toolId];

  const handleCopy = async () => {
    if (!status.configPath) return;
    const ok = await copyToClipboard(status.configPath);
    if (ok) {
      setHasCopied(true);
      toast.success(t("mcp.toolPathCopied"));
      setTimeout(() => setHasCopied(false), 2000);
    }
  };

  const handleOpenFolder = async () => {
    if (!status.configPath) return;
    const dir = getDirectoryPath(status.configPath);
    try {
      await tauriInvoke("open_folder", { path: dir || status.configPath });
    } catch (err) {
      if (import.meta.env.DEV) console.error("Failed to open folder:", err);
      toast.error(String(err));
    }
  };

  return (
    <li
      className={cn(
        "rounded-xl border px-3.5 py-2.5",
        status.installed ? "border-border/70 bg-background/50" : "border-border/40 bg-background/25",
      )}
    >
      <div className="flex items-center gap-2.5">
        <LobeIcon
          icon={BrandIcon}
          size={18}
          className={cn("shrink-0", status.installed ? "text-foreground" : "text-muted-foreground")}
        />
        <span
          className={cn(
            "w-36 shrink-0 truncate text-[13px] font-semibold tracking-tight",
            status.installed ? "text-foreground" : "text-muted-foreground",
          )}
          title={displayName}
        >
          {displayName}
        </span>
        <StatusChip size="sm" tone={status.installed ? "success" : "muted"} className="shrink-0 whitespace-nowrap">
          {status.installed ? t("mcp.toolInstalled") : t("mcp.toolNotInstalled")}
        </StatusChip>
        <span className="ml-auto shrink-0 text-[11px] tabular-nums text-muted-foreground">
          {t("mcp.toolServerCount", { count: status.serverCount })}
        </span>
      </div>

      <div className="mt-1 flex items-center gap-1 pl-7">
        <p
          className="min-w-0 flex-1 truncate font-mono text-[11px] leading-5 text-muted-foreground"
          title={status.configPath || undefined}
        >
          {formatDisplayPath(status.configPath) || t("mcp.toolConfigPathUnknown")}
        </p>
        {status.configPath ? (
          <button
            type="button"
            onClick={() => void handleCopy()}
            title={t("mcp.toolCopyPath")}
            aria-label={t("mcp.toolCopyPath")}
            className="flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground focus-ring"
          >
            {hasCopied ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
          </button>
        ) : null}
        {status.installed && status.configPath ? (
          <button
            type="button"
            onClick={() => void handleOpenFolder()}
            title={t("mcp.toolOpenFolder")}
            aria-label={t("mcp.toolOpenFolder")}
            className="flex h-6 w-6 shrink-0 cursor-pointer items-center justify-center rounded-md text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground focus-ring"
          >
            <ExternalLink className="h-3.5 w-3.5" />
          </button>
        ) : null}
      </div>

      {unreachable ? (
        <p className="mt-1.5 flex items-start gap-1.5 pl-7 text-[11px] leading-relaxed text-muted-foreground/80">
          <CircleSlash className="mt-0.5 h-3 w-3 shrink-0" />
          {t("mcp.toolNoAgentProfile")}
        </p>
      ) : null}
    </li>
  );
}

interface McpToolStatusPanelProps {
  className?: string;
}

/**
 * Where every MCP config target lives, and what is in it.
 *
 * `serverCount` is what is in the live file, SkillStar-managed or not — that
 * is what the agent will load. This panel is an inspector, not a workbench.
 */
export function McpToolStatusPanel({ className }: McpToolStatusPanelProps) {
  const { t } = useTranslation();
  const { profiles } = useAgentProfiles();
  const { statuses, installedCount, isLoading, isFetching, refetch } = useMcpToolStatuses();
  const unreachable = useMemo(() => new Set(mcpToolIdsWithoutAgentProfile(MCP_TOOL_IDS, profiles)), [profiles]);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16">
        <LoadingLogo size="md" label={t("mcp.toolStatusLoading")} />
      </div>
    );
  }

  return (
    <div className={cn("space-y-3", className)}>
      <div className="flex items-center gap-2">
        <span className="text-xs tabular-nums text-muted-foreground">
          {t("mcp.toolStatusInstalledCount", { installed: installedCount, total: statuses.length })}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="ml-auto h-7 gap-1.5 px-2 text-[11px]"
          onClick={() => void refetch()}
          disabled={isFetching}
        >
          <RefreshCw className={isFetching ? "h-3 w-3 animate-spin" : "h-3 w-3"} />
          {t("common.refresh")}
        </Button>
      </div>

      <ul className="space-y-2">
        {statuses.map((status) => (
          <McpToolStatusItem key={status.toolId} status={status} unreachable={unreachable.has(status.toolId)} />
        ))}
      </ul>
    </div>
  );
}

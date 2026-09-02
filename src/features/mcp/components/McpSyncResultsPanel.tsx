import { AlertTriangle, Check, CircleSlash, RefreshCw, RotateCcw, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { InsetPanel } from "../../../components/ui/InsetPanel";
import { cn } from "../../../lib/utils";
import { isMcpToolId, type McpToolId } from "../../../types";
import { MCP_TOOL_LABELS } from "../lib/toolRegistry";
import { isRetryableMcpOutcome, type McpSyncOutcome, type McpSyncReport } from "../lib/syncResults";

/**
 * Per-target detail for one sync batch.
 *
 * `McpSyncResult` has always carried the tool, the error text, the config path
 * and the backup path; until now a batch collapsed into a single "N failed"
 * toast (audit D.3-5), which is unactionable — the user could not tell which
 * agent lost the server, why, or where the recoverable copy was.
 *
 * The consistency line above the rows is the other half: per-tool writes are
 * atomic but the batch is not, so after a partial failure some tools do have the
 * server and some do not. Saying that out loud is what stops the two sides from
 * drifting silently.
 */

const OUTCOME_ICON: Record<McpSyncOutcome, typeof Check> = {
  success: Check,
  skipped: CircleSlash,
  rolledBack: RotateCcw,
  drifted: TriangleAlert,
  failed: AlertTriangle,
};

const OUTCOME_TONE: Record<McpSyncOutcome, string> = {
  success: "text-emerald-600 dark:text-emerald-400",
  skipped: "text-muted-foreground",
  rolledBack: "text-amber-600 dark:text-amber-400",
  drifted: "text-destructive",
  failed: "text-destructive",
};

function toolLabel(toolId: string): string {
  return isMcpToolId(toolId) ? MCP_TOOL_LABELS[toolId] : toolId;
}

interface McpSyncResultsPanelProps {
  report: McpSyncReport;
  onRetryTool?: (toolId: McpToolId) => void;
  onRetryAll?: () => void;
  retrying?: boolean;
  className?: string;
}

export function McpSyncResultsPanel({
  report,
  onRetryTool,
  onRetryAll,
  retrying,
  className,
}: McpSyncResultsPanelProps) {
  const { t } = useTranslation();
  const { consistency } = report;

  return (
    <InsetPanel className={className}>
      <div className="flex flex-wrap items-center gap-2">
        <p
          className={cn(
            "text-xs font-semibold",
            consistency.consistent ? "text-foreground" : "text-amber-600 dark:text-amber-400",
          )}
        >
          {consistency.consistent ? t("mcp.syncConsistent") : t("mcp.syncInconsistent")}
        </p>
        <span className="text-[11px] text-muted-foreground">
          {t("mcp.syncConsistencyCounts", {
            applied: consistency.applied.length,
            rolledBack: consistency.rolledBack.length,
            drifted: consistency.drifted.length,
          })}
        </span>
        {onRetryAll && report.retryableToolIds.length > 0 ? (
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="ml-auto h-7 gap-1.5 px-2 text-[11px]"
            onClick={onRetryAll}
            disabled={retrying}
          >
            <RefreshCw className={retrying ? "h-3 w-3 animate-spin" : "h-3 w-3"} />
            {t("mcp.syncRetryAll")}
          </Button>
        ) : null}
      </div>

      {consistency.drifted.length > 0 ? (
        <p className="rounded-lg bg-destructive/10 px-2.5 py-1.5 text-[11px] leading-relaxed text-destructive">
          {t("mcp.syncDriftWarning")}
        </p>
      ) : null}

      <ul className="space-y-1.5">
        {report.rows.map((row) => {
          const Icon = OUTCOME_ICON[row.outcome];
          const retryable = isRetryableMcpOutcome(row.outcome) && isMcpToolId(row.toolId);
          return (
            <li
              key={`${row.toolId}:${row.serverId}`}
              className="rounded-lg border border-border/50 bg-background/50 px-2.5 py-2"
            >
              <div className="flex items-center gap-2">
                <Icon className={cn("h-3.5 w-3.5 shrink-0", OUTCOME_TONE[row.outcome])} />
                <span className="text-xs font-medium text-foreground">{toolLabel(row.toolId)}</span>
                <span className={cn("text-[11px]", OUTCOME_TONE[row.outcome])}>
                  {t(`mcp.syncOutcome_${row.outcome}`)}
                </span>
                {retryable && onRetryTool ? (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="ml-auto h-6 gap-1 px-1.5 text-[11px]"
                    onClick={() => onRetryTool(row.toolId as McpToolId)}
                    disabled={retrying}
                  >
                    <RefreshCw className="h-3 w-3" />
                    {t("common.retry")}
                  </Button>
                ) : null}
              </div>

              {row.error ? (
                <p className="mt-1 break-all font-mono text-[11px] leading-relaxed text-destructive">{row.error}</p>
              ) : null}
              {row.rollbackError ? (
                <p className="mt-1 break-all font-mono text-[11px] leading-relaxed text-destructive">
                  {t("mcp.syncRollbackError", { error: row.rollbackError })}
                </p>
              ) : null}
              {row.configPath ? (
                <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
                  {t("mcp.syncConfigPath", { path: row.configPath })}
                </p>
              ) : null}
              {row.backupPath ? (
                <p className="mt-0.5 break-all font-mono text-[11px] text-muted-foreground">
                  {t("mcp.syncBackupPath", { path: row.backupPath })}
                </p>
              ) : null}
            </li>
          );
        })}
      </ul>
    </InsetPanel>
  );
}

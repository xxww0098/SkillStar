import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { tauriInvoke } from "../../../lib/ipc";
import { isMcpToolId, MCP_TOOL_IDS, type McpToolId, type McpToolStatus } from "../../../types";
import { mcpKeys } from "../api/keys";
import { MCP_TOOL_LABELS } from "../lib/toolRegistry";

const TOOL_STATUS_STALE_TIME_MS = 30_000;

export interface McpToolStatusRow extends McpToolStatus {
  toolId: McpToolId;
}

/**
 * Per-tool MCP config targets: installed or not, where the config file is, and
 * how many servers it currently holds.
 *
 * `mcp_tool_statuses` already returned all three; until now they were read once
 * inside the bulk import and thrown away (audit D.3-7), so the app knew where
 * every tool's config lived and never told anyone. This hook is the read that
 * makes it a view.
 *
 * Rows are ordered by `MCP_TOOL_IDS` and back-filled for any target the command
 * did not report, so the view is a complete list of what SkillStar can write to
 * rather than a list of what happened to answer.
 *
 * `noteForTool` is that view's one derived reading: the per-target note a picker
 * shows beside an undetected tool. It lives here rather than in each host
 * because both the store page and the marketplace page render target pickers,
 * and two verbatim copies of the same derivation is one copy too many.
 */
export function useMcpToolStatuses(enabled = true) {
  const { t } = useTranslation();
  const query = useQuery<McpToolStatus[]>({
    queryKey: mcpKeys.toolStatuses(),
    queryFn: () => tauriInvoke("mcp_tool_statuses"),
    enabled,
    staleTime: TOOL_STATUS_STALE_TIME_MS,
  });

  const statuses = useMemo<McpToolStatusRow[]>(() => {
    const byId = new Map(
      (query.data ?? [])
        .filter((status): status is McpToolStatusRow => isMcpToolId(status.toolId))
        .map((status) => [status.toolId, status]),
    );
    return MCP_TOOL_IDS.map(
      (toolId) =>
        byId.get(toolId) ?? {
          toolId,
          label: MCP_TOOL_LABELS[toolId],
          configPath: "",
          installed: false,
          serverCount: 0,
        },
    );
  }, [query.data]);

  const noteForTool = useMemo(() => {
    const byId = new Map(statuses.map((status) => [status.toolId, status]));
    return (toolId: McpToolId) => (byId.get(toolId)?.installed === false ? t("mcp.notDetectedSuffix") : null);
  }, [statuses, t]);

  return {
    statuses,
    noteForTool,
    installedCount: statuses.filter((status) => status.installed).length,
    isLoading: query.isLoading,
    isFetching: query.isFetching,
    error: query.error ?? null,
    refetch: query.refetch,
  };
}

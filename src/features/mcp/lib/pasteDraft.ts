import type { McpServerEntry } from "../../../types";

/** Seed values the create form accepts from a paste/deep-link draft. */
export function mcpDraftToFormValue(draft: McpServerEntry, enabled: Record<string, boolean>) {
  return {
    name: draft.name,
    transport: draft.transport,
    command: draft.command ?? undefined,
    args: draft.args,
    env: draft.env,
    cwd: draft.cwd ?? undefined,
    url: draft.url ?? undefined,
    headers: draft.headers,
    description: draft.description ?? undefined,
    homepage: draft.homepage ?? undefined,
    enabled,
    autoApproveAll: draft.autoApproveAll,
    autoApproveTools: draft.autoApproveTools,
    disabledTools: draft.disabledTools,
    timeoutMs: draft.timeoutMs ?? undefined,
  };
}

export function formatSchemaTokens(tokens: number): string {
  if (tokens >= 10_000) return `~${Math.round(tokens / 1000)}k`;
  if (tokens >= 1000) return `~${(tokens / 1000).toFixed(1)}k`;
  return `~${tokens}`;
}

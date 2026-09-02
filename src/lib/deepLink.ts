import type { NavPage } from "../types";

/**
 * OS deep-link routing: the backend emits `skillstar://deep-link` events
 * (scheme registered in tauri.conf.json) carrying `{ host, path, query }`.
 * Map the first path segment to the SPA's navigation surface. Unknown
 * targets return `null` and are ignored — the backend keeps emitting so the
 * OS link still wakes the app, but we never guess a destination.
 */
export type DeepLinkTarget = NavPage | "models" | null;

export function deepLinkNavTarget(host: string | null, path: string): DeepLinkTarget {
  const first = (host ?? path.split("/")[1] ?? "").toLowerCase();
  switch (first) {
    case "my-skills":
    case "skills":
      return "my-skills";
    case "marketplace":
      return "marketplace";
    case "skill-cards":
    case "cards":
      return "skill-cards";
    case "projects":
      return "projects";
    case "mcp":
      return "mcp";
    case "settings":
      return "settings";
    case "models":
      return "models";
    default:
      return null;
  }
}

/** Request-nonce payload asking the MCP command center to open a confirm UI. */
export interface McpImportRequest {
  nonce: number;
  /** Full `skillstar://` URL when the OS supplied one. */
  url: string | null;
  /** Raw query string (`url=` / `catalog=` / `config=` / `command=`). */
  query: string | null;
}

/**
 * Whether an MCP deep-link query is an *install intent*, not just navigation.
 *
 * Presence of these keys is enough — parsing and confirmation still happen
 * in `parse_mcp_paste` / the wizard. Returning the raw query (not a parsed
 * draft) keeps this file from becoming a second parser.
 */
export function mcpImportQuery(query: string | null | undefined): string | null {
  if (!query?.trim()) return null;
  return /(?:^|&)(url|catalog|config|command)=/i.test(query.trim()) ? query.trim() : null;
}

/** Reconstruct the paste text the backend parser already understands. */
export function mcpImportPasteText(request: Pick<McpImportRequest, "url" | "query">): string | null {
  const url = request.url?.trim();
  if (url) return url;
  const query = request.query?.trim();
  if (query) return `skillstar://mcp?${query}`;
  return null;
}

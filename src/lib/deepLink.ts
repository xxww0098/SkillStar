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

import type { DesktopAppId } from "../types";
import { GROK_BOT_FILTER } from "../types";

/** Quota catalog ids that expose instance actions. Not `xai` / `anthropic`. */
export const INSTANCE_CATALOG_IDS = ["cursor", "antigravity"] as const;

export function desktopAppIdForCatalog(catalogId: string): DesktopAppId | null {
  if (catalogId === "cursor") return "cursor";
  if (catalogId === "antigravity") return "antigravity";
  return null;
}

export function isGrokBotFilter(filter: string): boolean {
  return filter === GROK_BOT_FILTER;
}

export function desktopAppsForFilter(filter: string): DesktopAppId[] | null {
  if (filter === "__all__") return ["cursor", "grok-bot", "antigravity"];
  if (filter === "cursor") return ["cursor"];
  if (filter === "antigravity") return ["antigravity"];
  if (isGrokBotFilter(filter)) return ["grok-bot"];
  return null;
}

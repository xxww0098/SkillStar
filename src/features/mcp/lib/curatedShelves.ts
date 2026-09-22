import {
  BookOpen,
  Box,
  Braces,
  Clapperboard,
  Component,
  Drama,
  FolderOpen,
  Gauge,
  GitBranch,
  GitPullRequest,
  Library,
  Network,
  Palette,
  type LucideIcon,
} from "lucide-react";
import type { McpMarketEntry } from "../../../types";

/**
 * Presentation for the curated shortlist.
 *
 * The curated catalog (`mcp_snapshot::seeds::catalog`) is a fixed code
 * registry — this map mirrors its ids so each card gets a service-shaped
 * glyph instead of the generic kind icon. Presentation only: an id the
 * backend doesn't ship, or a new curated row this map hasn't caught up with,
 * simply falls back to the kind icon. Shelf grouping is data-driven off
 * `entry.source`, so it needs no second list here.
 */
export interface CuratedPresentation {
  icon: LucideIcon;
  /** Icon color — `<hue>-400` on dark, `paper:<hue>-600` on the light theme. */
  iconClass: string;
  /** Tinted well behind the icon. */
  wellClass: string;
}

export const CURATED_MCP_PRESENTATION: Record<string, CuratedPresentation> = {
  filesystem: {
    icon: FolderOpen,
    iconClass: "text-amber-400 paper:text-amber-600",
    wellClass: "bg-amber-400/10",
  },
  git: {
    icon: GitBranch,
    iconClass: "text-orange-400 paper:text-orange-600",
    wellClass: "bg-orange-400/10",
  },
  github: {
    icon: GitPullRequest,
    iconClass: "text-foreground/85",
    wellClass: "bg-foreground/10",
  },
  context7: {
    icon: BookOpen,
    iconClass: "text-indigo-400 paper:text-indigo-600",
    wellClass: "bg-indigo-400/10",
  },
  deepwiki: {
    icon: Library,
    iconClass: "text-cyan-400 paper:text-cyan-600",
    wellClass: "bg-cyan-400/10",
  },
  codegraph: {
    icon: Network,
    iconClass: "text-violet-400 paper:text-violet-600",
    wellClass: "bg-violet-400/10",
  },
  serena: {
    icon: Braces,
    iconClass: "text-rose-400 paper:text-rose-600",
    wellClass: "bg-rose-400/10",
  },
  playwright: {
    icon: Drama,
    iconClass: "text-emerald-400 paper:text-emerald-600",
    wellClass: "bg-emerald-400/10",
  },
  "chrome-devtools": {
    icon: Gauge,
    iconClass: "text-sky-400 paper:text-sky-600",
    wellClass: "bg-sky-400/10",
  },
  figma: {
    icon: Component,
    iconClass: "text-fuchsia-400 paper:text-fuchsia-600",
    wellClass: "bg-fuchsia-400/10",
  },
  blender: {
    icon: Box,
    iconClass: "text-orange-400 paper:text-orange-600",
    wellClass: "bg-orange-400/10",
  },
  photoshop: {
    icon: Palette,
    iconClass: "text-sky-400 paper:text-sky-600",
    wellClass: "bg-sky-400/10",
  },
  "after-effects": {
    icon: Clapperboard,
    iconClass: "text-violet-400 paper:text-violet-600",
    wellClass: "bg-violet-400/10",
  },
};

export interface McpMarketSection {
  /** The curated `source` bucket — feeds the `mcp.shelf_*` i18n keys. */
  key: string;
  entries: McpMarketEntry[];
}

/**
 * Group a page of curated cards into shelves by `entry.source`, keeping
 * first-seen order — the backend emits catalog order, so shelf order here is
 * whatever the catalog decided, with no second ordering table. Rows with no
 * bucket land under `""`, which the renderer labels as "other".
 */
export function groupMcpMarketShelves(entries: readonly McpMarketEntry[]): McpMarketSection[] {
  const byShelf = new Map<string, McpMarketEntry[]>();
  for (const entry of entries) {
    const key = entry.source ?? "";
    const shelf = byShelf.get(key);
    if (shelf) shelf.push(entry);
    else byShelf.set(key, [entry]);
  }
  return [...byShelf].map(([key, shelfEntries]) => ({ key, entries: shelfEntries }));
}

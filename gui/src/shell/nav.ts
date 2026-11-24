import { batch, computed, signal, untracked } from "nsh";
import type { IconName } from "../chrome/icons";
import { PANEL_BORDER_X, RAIL_COLLAPSED_W, RAIL_W, SHELL_GAP } from "../layout";

// Route ids and shell state — nav.rs parity. Pages never import each other;
// these signals and the event bus (events.ts) are the only cross-page wiring.

export type PageId =
  | "my_skills"
  | "marketplace"
  | "skill_cards"
  | "projects"
  | "mcp"
  | "accounts"
  | "settings"
  /** Market drill-down pushed by SelectPublisher; not in the sidebar. */
  | "publisher_detail";

export type RailMode = "skills" | "accounts";

export interface NavItem {
  id: PageId;
  /** Locale key (sidebar.* in assets/locales). */
  labelKey: string;
  /** Rail glyph — silhouette must stay distinct at 16px (nav.rs:75-86). */
  icon: IconName;
  /** Stable id for tests and the selection spring (nav.rs dom_id). */
  domId: string;
}

/** Sidebar page entries for Skills mode (nav.rs SKILLS_NAV). */
export const SKILLS_NAV: NavItem[] = [
  { id: "my_skills", labelKey: "sidebar.skills", icon: "sparkle", domId: "nav-skills" },
  { id: "marketplace", labelKey: "sidebar.market", icon: "store", domId: "nav-market" },
  { id: "skill_cards", labelKey: "sidebar.groups", icon: "deck", domId: "nav-decks" },
  { id: "projects", labelKey: "sidebar.projects", icon: "briefcase", domId: "nav-projects" },
  { id: "mcp", labelKey: "sidebar.mcp", icon: "server", domId: "nav-mcp" },
];

// Shell state.
export const currentPage = signal<PageId>("my_skills");
export const railMode = signal<RailMode>("skills");
export const collapsed = signal(false);
/** Shell root width measured onLayout; invariant across sidebar collapse toggles. */
export const shellWidth = signal(0);

/** Main panel inner width derived synchronously from `shellWidth` and `collapsed`
 * so active pages adjust in Frame 1 without waiting for an `onLayout` round-trip. */
export function shellPanelInnerWidth(): number {
  const w = shellWidth();
  if (w <= 0) return 0;
  const rail = collapsed() ? RAIL_COLLAPSED_W : RAIL_W;
  return Math.max(0, w - SHELL_GAP * 3 - rail - PANEL_BORDER_X);
}

/**
 * Reactive pane width for keep-alive pages:
 * - When `Shell` is mounted (`shellPanelInnerWidth() > 0`) and `fromShell` is
 *   provided, derives the width synchronously on Frame 1 when `collapsed()` flips
 *   without waiting for a second-pass `onLayout` round-trip.
 * - Falls back to `onLayout` in standalone tests and suppresses hidden-page updates
 *   while `active()` is false.
 */
export function createPaneWidth(opts: {
  active?: () => boolean;
  fromShell?: (panelInnerWidth: number) => number;
}): {
  width: () => number;
  onLayout: (event: { bounds: { size: { width: number } } }) => void;
} {
  const active = opts.active ?? (() => true);
  const measured = signal(0);
  let pendingWidth = 0;

  const width = computed<number>((prev) => {
    if (!active() && prev !== undefined) return prev;
    const shellW = shellPanelInnerWidth();
    if (shellW > 0 && opts.fromShell) {
      return Math.max(0, opts.fromShell(shellW));
    }
    const local = measured();
    return pendingWidth > 0 ? pendingWidth : local;
  });

  const onLayout = (event: { bounds: { size: { width: number } } }) => {
    const w = event.bounds.size.width;
    if (w <= 0) return;
    pendingWidth = w;
    if (active() && w !== untracked(measured)) {
      measured.set(w);
    }
  };

  return { width, onLayout };
}

/**
 * Last skills-nav row (shell.rs nav_slot). Settings and accounts have no
 * row, so the selection box fades out where it is; publisher detail maps
 * onto the marketplace row.
 */
export const navSlot = signal(0);

/** The sidebar row that owns this page (shell/nav.rs rail_page). */
export function railPage(page: PageId): PageId {
  return page === "publisher_detail" ? "marketplace" : page;
}

/** Index of a skills-nav page; null when the page has no row. */
export function skillsNavSlot(page: PageId): number | null {
  const rail = railPage(page);
  const index = SKILLS_NAV.findIndex((item) => item.id === rail);
  return index >= 0 ? index : null;
}

/** Whether the selection box is shown at all (shell/nav.rs `shown`). */
export function navBoxShown(page: PageId): boolean {
  return skillsNavSlot(page) !== null;
}

/** Accounts is a mode-gated destination: selecting it flips the rail. */
export function setCurrentPage(page: PageId): void {
  batch(() => {
    railMode.set(page === "accounts" ? "accounts" : "skills");
    const slot = skillsNavSlot(page);
    if (slot !== null) navSlot.set(slot);
    currentPage.set(page);
  });
}

/** Flip the rail mode; land on the mode's first page when the current page
 * isn't in that rail (shell.rs set_mode). */
export function setRailMode(mode: RailMode): void {
  batch(() => {
    railMode.set(mode);
    const ok = mode === "skills" ? currentPage() !== "accounts" : currentPage() === "accounts";
    if (!ok) {
      currentPage.set(mode === "skills" ? "my_skills" : "accounts");
    }
    if (mode === "skills") navSlot.set(skillsNavSlot(currentPage()) ?? 0);
  });
}

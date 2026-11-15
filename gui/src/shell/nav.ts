import { signal } from "nsh";
import type { IconName } from "../chrome/icons";

// Route ids and shell state — nav.rs parity. Pages never import each other;
// these signals and the event bus (events.ts) are the only cross-page wiring.

export type PageId =
  | "my_skills"
  | "marketplace"
  | "skill_cards"
  | "projects"
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
];

export const ACCOUNTS_ITEM: NavItem = {
  id: "accounts",
  labelKey: "sidebar.accounts",
  icon: "users",
  domId: "nav-accounts",
};

export const SETTINGS_ITEM: NavItem = {
  id: "settings",
  labelKey: "sidebar.settings",
  icon: "settings",
  domId: "nav-settings",
};

// Shell state.
export const currentPage = signal<PageId>("my_skills");
export const railMode = signal<RailMode>("skills");
export const collapsed = signal(false);

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
  railMode.set(page === "accounts" ? "accounts" : "skills");
  const slot = skillsNavSlot(page);
  if (slot !== null) navSlot.set(slot);
  currentPage.set(page);
}

/** Flip the rail mode; land on the mode's first page when the current page
 * isn't in that rail (shell.rs set_mode). */
export function setRailMode(mode: RailMode): void {
  railMode.set(mode);
  const ok = mode === "skills" ? currentPage() !== "accounts" : currentPage() === "accounts";
  if (!ok) {
    currentPage.set(mode === "skills" ? "my_skills" : "accounts");
  }
  if (mode === "skills") navSlot.set(skillsNavSlot(currentPage()) ?? 0);
}

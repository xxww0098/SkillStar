import type { Skill, SortOption } from "../../../types";

export interface DisplaySkillsInput {
  /** Search results, when a query is active. */
  results: { skills: Skill[] } | null;
  /** Leaderboard skills for the active (non-official) tab. */
  leaderboard: Skill[];
  /** Currently selected sort option. */
  sortBy: SortOption;
  /** Raw search box text (used only to detect "search mode", not for filtering itself). */
  searchQuery: string;
  /** Currently active tab id; "official" never falls back to the leaderboard. */
  activeTab: string;
}

/**
 * Computes the skill list to render in the marketplace grid: resolves
 * search-vs-leaderboard precedence, sorts, and recomputes rank numbers for
 * stars-desc display.
 *
 * Extracted verbatim from `Marketplace.tsx`'s `displaySkills` useMemo — see
 * that file's git history for the original inline version.
 */
export function computeDisplaySkills({
  results,
  leaderboard,
  sortBy,
  searchQuery,
  activeTab,
}: DisplaySkillsInput): Skill[] {
  let skills: Skill[] = [];
  const isSearchMode = Boolean(searchQuery.trim() && results);

  // Search results override
  if (isSearchMode && results) {
    skills = [...results.skills];
  } else if (activeTab !== "official") {
    skills = [...leaderboard];
  }

  // Sort
  if (sortBy === "name") {
    skills.sort((a, b) => a.name.localeCompare(b.name));
  } else if (sortBy === "updated") {
    skills.sort((a, b) => b.last_updated.localeCompare(a.last_updated));
  } else if (isSearchMode) {
    skills.sort((a, b) => b.stars - a.stars);
  }

  return skills.map((s, i) => {
    const rank = sortBy === "stars-desc" ? (isSearchMode ? i + 1 : (s.rank ?? i + 1)) : s.rank;
    if (rank === s.rank) return s;
    return { ...s, rank };
  });
}

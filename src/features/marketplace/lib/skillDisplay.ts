import type { Skill, SortOption } from "../../../types";

export interface DisplaySkillsInput {
  /** Whether the currently active tab is the MCP-only tab (no skills shown there). */
  isMcpTab: boolean;
  /** Search/AI-search results, when a query is active. */
  results: { skills: Skill[] } | null;
  /** Leaderboard skills for the active (non-official) tab. */
  leaderboard: Skill[];
  /** Currently selected sort option. */
  sortBy: SortOption;
  /** Raw search box text (used only to detect "search mode", not for filtering itself). */
  searchQuery: string;
  /** Currently active tab id; "official" never falls back to the leaderboard. */
  activeTab: string;
  /** AI-extracted keywords, or null when AI search is not active. */
  aiKeywords: string[] | null;
  /** Which AI keywords are toggled on. */
  aiActiveKeywords: Set<string>;
  /** Maps each AI keyword to the skill names it matched. */
  aiKeywordSkillMap: Record<string, string[]>;
}

/**
 * Computes the skill list to render in the marketplace grid: resolves
 * search-vs-leaderboard precedence, applies the AI-keyword toggle filter,
 * sorts, and recomputes rank numbers for stars-desc display.
 *
 * Extracted verbatim from `Marketplace.tsx`'s `displaySkills` useMemo — see
 * that file's git history for the original inline version.
 */
export function computeDisplaySkills({
  isMcpTab,
  results,
  leaderboard,
  sortBy,
  searchQuery,
  activeTab,
  aiKeywords,
  aiActiveKeywords,
  aiKeywordSkillMap,
}: DisplaySkillsInput): Skill[] {
  if (isMcpTab) return [];

  let skills: Skill[] = [];
  const isAiMode = Boolean(aiKeywords && results);
  const isSearchMode = Boolean(searchQuery.trim() && results) || isAiMode;

  // Search results override
  if (isSearchMode && results) {
    skills = [...results.skills];
  } else if (activeTab !== "official") {
    skills = [...leaderboard];
  }

  // AI keyword toggle filter: only show skills matching active keywords
  if (isAiMode) {
    if (aiActiveKeywords.size === 0) {
      // Allow deselecting all AI keywords; show empty result set.
      skills = [];
    } else if (Object.keys(aiKeywordSkillMap).length > 0) {
      // Build set of skill names matching any active keyword
      const allowedNames = new Set<string>();
      for (const kw of aiActiveKeywords) {
        const names = aiKeywordSkillMap[kw];
        if (names) names.forEach((n) => allowedNames.add(n));
      }
      skills = skills.filter((s) => allowedNames.has(s.name));
    }
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

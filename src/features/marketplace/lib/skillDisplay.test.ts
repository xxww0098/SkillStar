import { describe, expect, it } from "vitest";
import type { Skill } from "../../../types";
import { computeDisplaySkills, type DisplaySkillsInput } from "./skillDisplay";

function skill(overrides: Partial<Skill> = {}): Skill {
  return {
    name: "writing-plans",
    description: "Plan things",
    skill_type: "hub",
    stars: 10,
    installed: false,
    update_available: false,
    last_updated: "2026-01-01T00:00:00Z",
    git_url: "https://github.com/example/writing-plans",
    tree_hash: null,
    category: "None",
    author: null,
    topics: [],
    ...overrides,
  };
}

/** Builds the full input set for `computeDisplaySkills`, with sane leaderboard-mode defaults. */
function buildInput(overrides: Partial<DisplaySkillsInput> = {}): DisplaySkillsInput {
  return {
    isMcpTab: false,
    results: null,
    leaderboard: [],
    sortBy: "stars-desc",
    searchQuery: "",
    activeTab: "all",
    aiKeywords: null,
    aiActiveKeywords: new Set(),
    aiKeywordSkillMap: {},
    ...overrides,
  };
}

describe("computeDisplaySkills", () => {
  it("returns an empty list for the MCP tab regardless of other inputs", () => {
    const input = buildInput({
      isMcpTab: true,
      leaderboard: [skill({ name: "a" }), skill({ name: "b" })],
    });

    expect(computeDisplaySkills(input)).toEqual([]);
  });

  it("shows the leaderboard when not in search mode and the tab is not official", () => {
    const a = skill({ name: "a", rank: 1 });
    const b = skill({ name: "b", rank: 2 });
    const input = buildInput({ leaderboard: [a, b], activeTab: "all" });

    expect(computeDisplaySkills(input).map((s) => s.name)).toEqual(["a", "b"]);
  });

  it("shows nothing (not the leaderboard) for the official tab when not searching", () => {
    const input = buildInput({
      leaderboard: [skill({ name: "a" })],
      activeTab: "official",
    });

    expect(computeDisplaySkills(input)).toEqual([]);
  });

  it("normal search mode: search results override the leaderboard", () => {
    const leaderboardSkill = skill({ name: "leaderboard-only" });
    const searchSkill = skill({ name: "search-result" });
    const input = buildInput({
      leaderboard: [leaderboardSkill],
      results: { skills: [searchSkill] },
      searchQuery: "search-result",
    });

    const names = computeDisplaySkills(input).map((s) => s.name);
    expect(names).toEqual(["search-result"]);
  });

  it("does not enter search mode when searchQuery is whitespace-only, even with results present", () => {
    const leaderboardSkill = skill({ name: "leaderboard-only" });
    const searchSkill = skill({ name: "search-result" });
    const input = buildInput({
      leaderboard: [leaderboardSkill],
      results: { skills: [searchSkill] },
      searchQuery: "   ",
    });

    expect(computeDisplaySkills(input).map((s) => s.name)).toEqual(["leaderboard-only"]);
  });

  describe("AI keyword mode", () => {
    it("is active whenever aiKeywords and results are both present, even with an empty searchQuery", () => {
      const matched = skill({ name: "matched", stars: 5 });
      const input = buildInput({
        results: { skills: [matched] },
        searchQuery: "",
        aiKeywords: ["planning"],
        aiActiveKeywords: new Set(["planning"]),
        aiKeywordSkillMap: { planning: ["matched"] },
      });

      expect(computeDisplaySkills(input).map((s) => s.name)).toEqual(["matched"]);
    });

    it("filters to only skills matching at least one active keyword", () => {
      const a = skill({ name: "a" });
      const b = skill({ name: "b" });
      const c = skill({ name: "c" });
      const input = buildInput({
        results: { skills: [a, b, c] },
        aiKeywords: ["kw1", "kw2"],
        aiActiveKeywords: new Set(["kw1"]),
        aiKeywordSkillMap: { kw1: ["a", "b"], kw2: ["c"] },
      });

      expect(
        computeDisplaySkills(input)
          .map((s) => s.name)
          .sort(),
      ).toEqual(["a", "b"]);
    });

    it("unions matches across multiple active keywords", () => {
      const a = skill({ name: "a" });
      const b = skill({ name: "b" });
      const c = skill({ name: "c" });
      const input = buildInput({
        results: { skills: [a, b, c] },
        aiKeywords: ["kw1", "kw2"],
        aiActiveKeywords: new Set(["kw1", "kw2"]),
        aiKeywordSkillMap: { kw1: ["a"], kw2: ["c"] },
      });

      expect(
        computeDisplaySkills(input)
          .map((s) => s.name)
          .sort(),
      ).toEqual(["a", "c"]);
    });

    it("returns an empty result set when all AI keywords are deselected", () => {
      const a = skill({ name: "a" });
      const input = buildInput({
        results: { skills: [a] },
        aiKeywords: ["kw1"],
        aiActiveKeywords: new Set(),
        aiKeywordSkillMap: { kw1: ["a"] },
      });

      expect(computeDisplaySkills(input)).toEqual([]);
    });

    it("skips the filter (shows all search results) when the keyword-skill map is still empty", () => {
      const a = skill({ name: "a" });
      const b = skill({ name: "b" });
      const input = buildInput({
        results: { skills: [a, b] },
        aiKeywords: ["kw1"],
        aiActiveKeywords: new Set(["kw1"]),
        aiKeywordSkillMap: {},
      });

      // aiKeywordSkillMap is empty (e.g. not hydrated yet) — the guard on
      // Object.keys(aiKeywordSkillMap).length > 0 means no filtering happens.
      expect(
        computeDisplaySkills(input)
          .map((s) => s.name)
          .sort(),
      ).toEqual(["a", "b"]);
    });
  });

  describe("sorting", () => {
    it("sorts by name (localeCompare) when sortBy is 'name', overriding leaderboard order", () => {
      const b = skill({ name: "banana" });
      const a = skill({ name: "apple" });
      const input = buildInput({ leaderboard: [b, a], sortBy: "name" });

      expect(computeDisplaySkills(input).map((s) => s.name)).toEqual(["apple", "banana"]);
    });

    it("sorts by last_updated descending when sortBy is 'updated'", () => {
      const older = skill({ name: "older", last_updated: "2025-01-01T00:00:00Z" });
      const newer = skill({ name: "newer", last_updated: "2026-01-01T00:00:00Z" });
      const input = buildInput({ leaderboard: [older, newer], sortBy: "updated" });

      expect(computeDisplaySkills(input).map((s) => s.name)).toEqual(["newer", "older"]);
    });

    it("in search mode with the default stars-desc sort, sorts by stars descending", () => {
      const low = skill({ name: "low", stars: 1 });
      const high = skill({ name: "high", stars: 100 });
      const input = buildInput({
        results: { skills: [low, high] },
        searchQuery: "x",
        sortBy: "stars-desc",
      });

      expect(computeDisplaySkills(input).map((s) => s.name)).toEqual(["high", "low"]);
    });

    it("in leaderboard mode with stars-desc, preserves the incoming leaderboard order (no re-sort)", () => {
      // Leaderboard is presumed pre-sorted server-side; stars-desc + non-search-mode
      // does not re-sort by stars, it only recomputes ranks (see rank test below).
      const first = skill({ name: "first", stars: 1 });
      const second = skill({ name: "second", stars: 100 });
      const input = buildInput({ leaderboard: [first, second], sortBy: "stars-desc" });

      expect(computeDisplaySkills(input).map((s) => s.name)).toEqual(["first", "second"]);
    });

    it("sort is stable for equal keys (name sort ties keep original relative order)", () => {
      const first = skill({ name: "same", stars: 1, git_url: "https://github.com/a/same" });
      const second = skill({ name: "same", stars: 2, git_url: "https://github.com/b/same" });
      const input = buildInput({ leaderboard: [first, second], sortBy: "name" });

      const result = computeDisplaySkills(input);
      expect(result.map((s) => s.git_url)).toEqual(["https://github.com/a/same", "https://github.com/b/same"]);
    });
  });

  describe("rank recomputation", () => {
    it("in search mode with stars-desc, assigns 1-indexed ranks by result position", () => {
      const a = skill({ name: "a", stars: 100, rank: 5 });
      const b = skill({ name: "b", stars: 50, rank: 1 });
      const input = buildInput({
        results: { skills: [a, b] },
        searchQuery: "x",
        sortBy: "stars-desc",
      });

      const result = computeDisplaySkills(input);
      expect(result.map((s) => ({ name: s.name, rank: s.rank }))).toEqual([
        { name: "a", rank: 1 },
        { name: "b", rank: 2 },
      ]);
    });

    it("in leaderboard mode with stars-desc, keeps the existing rank field when present", () => {
      const a = skill({ name: "a", rank: 7 });
      const input = buildInput({ leaderboard: [a], sortBy: "stars-desc" });

      expect(computeDisplaySkills(input)[0].rank).toBe(7);
    });

    it("in leaderboard mode with stars-desc, falls back to 1-indexed position when rank is missing", () => {
      const a = skill({ name: "a", rank: undefined });
      const b = skill({ name: "b", rank: undefined });
      const input = buildInput({ leaderboard: [a, b], sortBy: "stars-desc" });

      expect(computeDisplaySkills(input).map((s) => s.rank)).toEqual([1, 2]);
    });

    it("returns the same object reference when the recomputed rank is unchanged", () => {
      const a = skill({ name: "a", rank: 1 });
      const input = buildInput({ leaderboard: [a], sortBy: "stars-desc" });

      expect(computeDisplaySkills(input)[0]).toBe(a);
    });

    it("does not touch rank for non-stars-desc sorts (rank passes through as-is)", () => {
      const a = skill({ name: "a", rank: 42 });
      const input = buildInput({ leaderboard: [a], sortBy: "name" });

      expect(computeDisplaySkills(input)[0].rank).toBe(42);
    });
  });
});

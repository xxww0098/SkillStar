import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LocalFirstResult, OfficialPublisher, Skill, SnapshotStatus } from "../../../types";
import { useMarketplace } from "./useMarketplace";

const mockedInvoke = vi.mocked(invoke);

function localFirst<T>(data: T, status: SnapshotStatus, error: string | null = null): LocalFirstResult<T> {
  return { data, snapshot_status: status, snapshot_updated_at: "2026-01-01T00:00:00Z", error };
}

const SKILL: Skill = {
  name: "writing-plans",
  description: "Plan things",
  skill_type: "hub",
  stars: 1,
  installed: false,
  update_available: false,
  last_updated: "2026-01-01T00:00:00Z",
  git_url: "https://github.com/example/writing-plans",
  tree_hash: null,
  category: "None",
  author: null,
  topics: [],
};

const PUBLISHER: OfficialPublisher = {
  name: "anthropics",
  repo: "anthropics/skills",
  repo_count: 1,
  skill_count: 1,
  url: "https://github.com/anthropics/skills",
};

function wrapper({ children }: { children: ReactNode }) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

beforeEach(() => {
  vi.clearAllMocks();
  // The hook logs raw backend chains here instead of rendering them.
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("useMarketplace snapshot state", () => {
  it("does not claim the snapshot is fresh before anything has been fetched", () => {
    mockedInvoke.mockResolvedValue(localFirst([SKILL], "fresh"));
    const { result } = renderHook(() => useMarketplace(), { wrapper });

    expect(result.current.snapshots.leaderboard.status).toBeNull();
    expect(result.current.snapshots.publishers.status).toBeNull();
    expect(result.current.snapshots.search.status).toBeNull();
  });

  it("keeps leaderboard and publishers state independent", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "get_publishers_local") return localFirst([], "remote_error", "github: 403 rate limited");
      if (command === "list_marketplace_skills_local") return localFirst([SKILL], "fresh");
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });
    await act(async () => {
      await result.current.fetchLeaderboard("all");
      await result.current.fetchOfficialPublishers();
    });

    await waitFor(() => {
      expect(result.current.snapshots.publishers.error?.kind).toBe("remote_error");
    });

    // Publishers failing must not leak into the skills tab (D4).
    expect(result.current.snapshots.leaderboard.status).toBe("fresh");
    expect(result.current.snapshots.leaderboard.error).toBeNull();
    expect(result.current.snapshots.publishers.error?.detail).toBe("github: 403 rate limited");
  });

  // `list_marketplace_skills_local` (the default "all" view) reports `stale`
  // like every other local read: freshness comes from the scope's sync state,
  // not from "the table has rows". A degraded snapshot therefore reaches the
  // default tab as `stale` and gets the same auto-refresh treatment.
  it("stays retryable after a failed stale refresh instead of latching shut", async () => {
    let syncCalls = 0;
    let syncShouldFail = true;
    let leaderboardStatus: SnapshotStatus = "stale";

    mockedInvoke.mockImplementation(async (command) => {
      if (command === "list_marketplace_skills_local") return localFirst([SKILL], leaderboardStatus);
      if (command === "sync_marketplace_scope") {
        syncCalls += 1;
        if (syncShouldFail) throw new Error("mirror unreachable: connect ETIMEDOUT");
        leaderboardStatus = "fresh";
        return undefined;
      }
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });
    await act(async () => {
      await result.current.fetchLeaderboard("all");
    });

    await waitFor(() => {
      expect(result.current.snapshots.leaderboard.error?.kind).toBe("sync_failed");
    });
    expect(syncCalls).toBe(1);
    expect(result.current.snapshots.leaderboard.error?.detail).toContain("ETIMEDOUT");

    // The explicit retry recovers without a tab round-trip (D6).
    syncShouldFail = false;
    await act(async () => {
      await result.current.retrySnapshot("leaderboard");
    });

    await waitFor(() => {
      expect(result.current.snapshots.leaderboard.status).toBe("fresh");
    });
    expect(syncCalls).toBe(2);
    expect(result.current.snapshots.leaderboard.error).toBeNull();
  });

  it("clears a search error once a later search succeeds", async () => {
    let searchStatus: SnapshotStatus = "remote_error";
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "search_marketplace_local") {
        return localFirst([SKILL], searchStatus, searchStatus === "remote_error" ? "upstream 502" : null);
      }
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });
    await act(async () => {
      await result.current.search("plans");
    });
    expect(result.current.snapshots.search.error?.kind).toBe("remote_error");

    searchStatus = "fresh";
    await act(async () => {
      await result.current.search("plans other");
    });

    // D5: the red bar must not survive a recovered request.
    expect(result.current.snapshots.search.error).toBeNull();
    expect(result.current.snapshots.search.status).toBe("fresh");
  });

  it("drops the search verdict once the query text moves off the executed query", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "search_marketplace_local") return localFirst([SKILL], "remote_error", "upstream 502");
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });
    await act(async () => {
      await result.current.search("foo");
    });
    expect(result.current.snapshots.search.error?.kind).toBe("remote_error");
    expect(result.current.snapshots.search.status).toBe("remote_error");

    // Same query (the input still shows what was searched) keeps its verdict.
    act(() => {
      result.current.notePendingSearchQuery("Foo ");
    });
    expect(result.current.snapshots.search.status).toBe("remote_error");

    // D5 at query granularity: "bar" has never been executed, so the previous
    // query's error bar and freshness label must not hang on it.
    act(() => {
      result.current.notePendingSearchQuery("bar");
    });
    expect(result.current.snapshots.search.error).toBeNull();
    expect(result.current.snapshots.search.status).toBeNull();
    expect(result.current.snapshots.search.updatedAt).toBeNull();
  });

  // The page searches on a 400ms debounce, so a request is routinely still in
  // flight when the user types the next character. `notePendingSearchQuery`
  // retires the old verdict, but the late response used to re-park a fresh copy
  // of it on the query now on screen.
  it("ignores a late search failure that belongs to an abandoned query", async () => {
    let releaseFoo = () => {};
    const fooInFlight = new Promise<void>((resolve) => {
      releaseFoo = resolve;
    });

    mockedInvoke.mockImplementation(async (command, args) => {
      if (command !== "search_marketplace_local") return undefined;
      if ((args as { query: string }).query === "foo") {
        await fooInFlight;
        throw new Error("mirror unreachable: connect ETIMEDOUT");
      }
      return localFirst([SKILL], "fresh");
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });

    let pending!: Promise<void>;
    act(() => {
      pending = result.current.search("foo").catch(() => {});
    });
    // The user keeps typing while "foo" is still out there.
    act(() => {
      result.current.notePendingSearchQuery("bar");
    });

    await act(async () => {
      releaseFoo();
      await pending;
    });

    expect(result.current.snapshots.search.error).toBeNull();
    expect(result.current.snapshots.search.status).toBeNull();
    expect(result.current.snapshots.search.updatedAt).toBeNull();
  });

  it("ignores a late online-search success that belongs to an abandoned query", async () => {
    let releaseFoo = () => {};
    const fooInFlight = new Promise<void>((resolve) => {
      releaseFoo = resolve;
    });

    mockedInvoke.mockImplementation(async (command, args) => {
      if (command === "sync_marketplace_scope") return undefined;
      if (command !== "search_marketplace_local") return undefined;
      if ((args as { query: string }).query === "foo") {
        await fooInFlight;
        return localFirst([SKILL], "remote_error", "upstream 502");
      }
      return localFirst([], "fresh");
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });

    let pending!: Promise<void>;
    act(() => {
      pending = result.current.searchOnline("foo");
    });
    act(() => {
      result.current.notePendingSearchQuery("bar");
    });

    await act(async () => {
      releaseFoo();
      await pending;
    });

    // "foo" results must not become "bar" results, and its remote_error verdict
    // must not follow them.
    expect(result.current.results).toBeNull();
    expect(result.current.snapshots.search.status).toBeNull();
    expect(result.current.snapshots.search.error).toBeNull();
  });

  // Keyword extraction is an LLM call, so an AI search is out for seconds while
  // the user keeps editing the toolbar. Nothing starts a second AI search in
  // that window, so the sequence counter still matches when it answers — only
  // the query check can tell that "foo"'s chips, results and verdict no longer
  // describe what is on screen.
  it("ignores a late AI-search success that belongs to an abandoned query", async () => {
    let releaseFoo = () => {};
    const fooInFlight = new Promise<void>((resolve) => {
      releaseFoo = resolve;
    });

    mockedInvoke.mockImplementation(async (command) => {
      if (command === "ai_extract_search_keywords") {
        await fooInFlight;
        return ["foo", "planning"];
      }
      if (command === "ai_search_marketplace_local") {
        return localFirst(
          { skills: [SKILL], keyword_skill_map: { foo: [SKILL.name] } },
          "remote_error",
          "upstream 502",
        );
      }
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });

    let pending!: Promise<void>;
    act(() => {
      pending = result.current.aiSearch("foo");
    });
    // The user types past the AI search while it is still extracting keywords.
    act(() => {
      result.current.notePendingSearchQuery("bar");
    });

    await act(async () => {
      releaseFoo();
      await pending;
    });

    expect(result.current.aiKeywords).toBeNull();
    expect(result.current.results).toBeNull();
    expect(result.current.snapshots.search.status).toBeNull();
    expect(result.current.snapshots.search.error).toBeNull();
    expect(result.current.snapshots.search.updatedAt).toBeNull();
    // The spinner still belongs to this invocation, so it must stop regardless.
    expect(result.current.aiSearching).toBe(false);
  });

  it("ignores a late AI-search failure that belongs to an abandoned query", async () => {
    let releaseFoo = () => {};
    const fooInFlight = new Promise<void>((resolve) => {
      releaseFoo = resolve;
    });

    mockedInvoke.mockImplementation(async (command) => {
      if (command === "ai_extract_search_keywords") {
        await fooInFlight;
        throw new Error("model provider unreachable");
      }
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });

    let pending!: Promise<void>;
    act(() => {
      pending = result.current.aiSearch("foo").catch(() => {});
    });
    act(() => {
      result.current.notePendingSearchQuery("bar");
    });

    await act(async () => {
      releaseFoo();
      await pending;
    });

    expect(result.current.snapshots.search.error).toBeNull();
    expect(result.current.snapshots.search.status).toBeNull();
    expect(result.current.aiSearching).toBe(false);
  });

  it("clears the search verdict when the AI search is cleared", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "search_marketplace_local") return localFirst([SKILL], "remote_error", "upstream 502");
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });
    await act(async () => {
      await result.current.search("foo");
    });
    expect(result.current.snapshots.search.error?.kind).toBe("remote_error");

    act(() => {
      result.current.clearAiSearch();
    });
    expect(result.current.snapshots.search.error).toBeNull();
    expect(result.current.snapshots.search.status).toBeNull();
  });

  it("maps a thrown local read to a structured query_failed error", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "get_publishers_local") throw new Error("Other: sqlite database is locked");
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });
    await act(async () => {
      await result.current.fetchOfficialPublishers();
    });

    await waitFor(() => {
      expect(result.current.snapshots.publishers.error?.kind).toBe("query_failed");
    });
    // D7/D10: the raw chain is a detail, never the headline copy.
    expect(result.current.snapshots.publishers.error?.detail).toContain("sqlite database is locked");
  });

  it("surfaces publishers data on a healthy fetch", async () => {
    mockedInvoke.mockImplementation(async (command) => {
      if (command === "get_publishers_local") return localFirst([PUBLISHER], "fresh");
      return undefined;
    });

    const { result } = renderHook(() => useMarketplace(), { wrapper });
    await act(async () => {
      await result.current.fetchOfficialPublishers();
    });

    await waitFor(() => {
      expect(result.current.publishers).toHaveLength(1);
    });
    expect(result.current.snapshots.publishers.error).toBeNull();
  });
});

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AiKeywordSearchResult,
  LocalFirstResult,
  MarketplaceResult,
  OfficialPublisher,
  Skill,
  SnapshotStatus,
} from "../../../types";
import {
  deriveSnapshotState,
  EMPTY_SNAPSHOT_STATE,
  type MarketplaceError,
  type MarketplaceScope,
  type MarketplaceSnapshotState,
  toErrorDetail,
} from "../lib/snapshotState";

export type AiSearchPhase = "extracting" | "searching" | null;

const MARKETPLACE_STALE_TIME_MS = 5 * 60 * 1000;
const MARKETPLACE_QUERY_ROOT = ["marketplace"] as const;
/**
 * How many times an automatic stale-refresh may fail for one scope before the
 * hook stops retrying on its own. A failed attempt no longer latches the scope
 * shut forever (that stranded users in `stale` + error with no way out); it only
 * burns one of these attempts, and the explicit retry action resets the count.
 */
const MAX_STALE_REFRESH_ATTEMPTS = 3;

function toMarketplaceResult(skills: Skill[]): MarketplaceResult {
  return {
    skills,
    total_count: skills.length,
    page: 1,
    has_more: false,
  };
}

type LeaderboardCategory = "all" | "hot" | "trending";

function normalizeLeaderboardCategory(category: string): LeaderboardCategory {
  if (category === "hot" || category === "trending") return category;
  return "all";
}

function normalizeSearchKeyQuery(query: string): string {
  return query.trim().toLowerCase();
}

function searchQueryKey(query: string, limit: number) {
  return [...MARKETPLACE_QUERY_ROOT, "search", normalizeSearchKeyQuery(query), limit] as const;
}

function leaderboardQueryKey(category: LeaderboardCategory) {
  return [...MARKETPLACE_QUERY_ROOT, "leaderboard", category] as const;
}

const publishersQueryKey = [...MARKETPLACE_QUERY_ROOT, "publishers"] as const;

const PUBLISHERS_REFRESH_KEY = "publishers";
const leaderboardRefreshKey = (category: LeaderboardCategory) => `leaderboard:${category}`;

/** Per-scope bookkeeping for the automatic stale refresh (see D6 in the review). */
interface StaleRefreshEntry {
  /** Consecutive failures; capped by MAX_STALE_REFRESH_ATTEMPTS. */
  attempts: number;
  /** A refresh already succeeded — don't loop if the snapshot is still stale. */
  done: boolean;
  inFlight: boolean;
}

const NO_SCOPE_ERRORS: Record<MarketplaceScope, MarketplaceError | null> = {
  leaderboard: null,
  publishers: null,
  search: null,
};

export function useMarketplace() {
  const queryClient = useQueryClient();
  const [results, setResults] = useState<MarketplaceResult | null>(null);
  const [leaderboard, setLeaderboard] = useState<Skill[]>([]);
  const [publishers, setPublishers] = useState<OfficialPublisher[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  // Search is mutation-driven, so its snapshot meta has no query cache to be
  // derived from; leaderboard/publishers are derived straight from their query.
  const [searchStatus, setSearchStatus] = useState<SnapshotStatus | null>(null);
  const [searchUpdatedAt, setSearchUpdatedAt] = useState<string | null>(null);
  const [scopeErrors, setScopeErrors] = useState<Record<MarketplaceScope, MarketplaceError | null>>(NO_SCOPE_ERRORS);
  const [requestedLeaderboardCategory, setRequestedLeaderboardCategory] = useState<LeaderboardCategory>("all");
  const [leaderboardEnabled, setLeaderboardEnabled] = useState(false);
  const [publishersEnabled, setPublishersEnabled] = useState(false);
  const [aiKeywords, setAiKeywords] = useState<string[] | null>(null);
  const [aiSearching, setAiSearching] = useState(false);
  const [aiPhase, setAiPhase] = useState<AiSearchPhase>(null);
  const [aiAllSkills, setAiAllSkills] = useState<Skill[]>([]);
  const [aiKeywordSkillMap, setAiKeywordSkillMap] = useState<Record<string, string[]>>({});
  const [aiActiveKeywords, setAiActiveKeywords] = useState<Set<string>>(new Set());
  const staleRefreshRef = useRef<Map<string, StaleRefreshEntry>>(new Map());
  const inFlightRefreshesRef = useRef(0);
  const aiSearchSeqRef = useRef(0);
  /**
   * Normalized query the current search snapshot meta describes, or `null` when
   * no search has been executed since the last reset. Search state is per-query,
   * so it must be able to tell "this belongs to what the user is looking at"
   * from "this belongs to a query that is no longer on screen".
   */
  const executedSearchQueryRef = useRef<string | null>(null);
  /**
   * Normalized query the search scope currently *targets* — what the toolbar
   * shows, not what a response belongs to. `null` means the scope was abandoned
   * (input cleared, tab switched), so nothing in flight is worth applying.
   *
   * Search runs on a debounce, so a request can still be in flight when the user
   * types the next character. Without this, the slow response for the abandoned
   * query lands afterwards and unconditionally parks its results, freshness
   * label and error banner on the query now on screen — the exact mirror of the
   * `aiSearchSeqRef` guard, at query granularity instead of call granularity.
   */
  const activeSearchQueryRef = useRef<string | null>(null);
  const requestedLeaderboardCategoryRef = useRef(requestedLeaderboardCategory);
  requestedLeaderboardCategoryRef.current = requestedLeaderboardCategory;

  const setScopeError = useCallback((scope: MarketplaceScope, error: MarketplaceError | null) => {
    setScopeErrors((prev) => (prev[scope] === null && error === null ? prev : { ...prev, [scope]: error }));
  }, []);

  const reportScopeError = useCallback(
    (scope: MarketplaceScope, kind: MarketplaceError["kind"], cause: unknown) => {
      const detail = toErrorDetail(cause);
      // Keep the raw chain out of the primary UI copy but available while debugging.
      console.warn(`[marketplace] ${scope} ${kind}:`, cause);
      setScopeError(scope, { kind, scope, detail });
    },
    [setScopeError],
  );

  const canAttemptStaleRefresh = useCallback((key: string) => {
    const entry = staleRefreshRef.current.get(key);
    if (!entry) return true;
    if (entry.done || entry.inFlight) return false;
    return entry.attempts < MAX_STALE_REFRESH_ATTEMPTS;
  }, []);

  const markStaleRefreshStart = useCallback((key: string) => {
    const entry = staleRefreshRef.current.get(key);
    staleRefreshRef.current.set(key, { attempts: entry?.attempts ?? 0, done: false, inFlight: true });
  }, []);

  const markStaleRefreshSettled = useCallback((key: string, ok: boolean) => {
    const attempts = staleRefreshRef.current.get(key)?.attempts ?? 0;
    // Only success latches `done`. A failure must stay retryable, otherwise a
    // single network blip silently disables this scope for the hook's lifetime.
    staleRefreshRef.current.set(key, { attempts: ok ? attempts : attempts + 1, done: ok, inFlight: false });
  }, []);

  const resetStaleRefresh = useCallback((key: string) => {
    staleRefreshRef.current.delete(key);
  }, []);

  const beginBackgroundRefresh = useCallback(() => {
    inFlightRefreshesRef.current += 1;
    setRefreshing(true);
  }, []);

  const endBackgroundRefresh = useCallback(() => {
    inFlightRefreshesRef.current = Math.max(0, inFlightRefreshesRef.current - 1);
    if (inFlightRefreshesRef.current === 0) {
      setRefreshing(false);
    }
  }, []);

  const fetchLocalLeaderboard = useCallback((category: LeaderboardCategory) => {
    if (category === "all") {
      return tauriInvoke("list_marketplace_skills_local");
    }

    return tauriInvoke("get_leaderboard_local", {
      category,
    });
  }, []);

  const fetchLocalPublishers = useCallback(() => tauriInvoke("get_publishers_local"), []);

  const readLocalSearch = useCallback(
    (query: string, limit: number) =>
      queryClient.fetchQuery({
        queryKey: searchQueryKey(query, limit),
        queryFn: () =>
          tauriInvoke("search_marketplace_local", {
            query,
            limit,
          }),
        staleTime: MARKETPLACE_STALE_TIME_MS,
      }),
    [queryClient],
  );

  /**
   * Drop every trace of the previous query's search snapshot. Status/updatedAt
   * and the error banner all describe one specific query; leaving them behind
   * parks a verdict on a query that was never run.
   */
  const clearSearchSnapshot = useCallback(() => {
    executedSearchQueryRef.current = null;
    setSearchStatus(null);
    setSearchUpdatedAt(null);
    setScopeError("search", null);
  }, [setScopeError]);

  /**
   * Does this response still describe the query the user is looking at? A
   * response for anything else is stale and must not touch shared search state.
   */
  const isActiveSearchQuery = useCallback(
    (query: string) => activeSearchQueryRef.current === normalizeSearchKeyQuery(query),
    [],
  );

  const applySearchSnapshotMeta = useCallback(
    <T>(query: string, result: LocalFirstResult<T>) => {
      executedSearchQueryRef.current = normalizeSearchKeyQuery(query);
      setSearchStatus(result.snapshot_status);
      setSearchUpdatedAt(result.snapshot_updated_at);
      if (result.snapshot_status === "remote_error" || result.snapshot_status === "error_fallback") {
        setScopeError("search", {
          kind: result.snapshot_status,
          scope: "search",
          detail: result.error ?? null,
        });
      } else {
        // Healthy result — clear any error left over from a previous attempt.
        setScopeError("search", null);
      }
    },
    [setScopeError],
  );

  const leaderboardQuery = useQuery({
    queryKey: leaderboardQueryKey(requestedLeaderboardCategory),
    queryFn: () => fetchLocalLeaderboard(requestedLeaderboardCategory),
    enabled: leaderboardEnabled,
    staleTime: MARKETPLACE_STALE_TIME_MS,
  });

  const publishersQuery = useQuery({
    queryKey: publishersQueryKey,
    queryFn: fetchLocalPublishers,
    enabled: publishersEnabled,
    staleTime: MARKETPLACE_STALE_TIME_MS,
  });

  useEffect(() => {
    const result = leaderboardQuery.data;
    if (!result) return;
    setLeaderboard(result.data);
    // A healthy snapshot retires any refresh error still parked on this scope.
    if (result.snapshot_status === "fresh") setScopeError("leaderboard", null);
  }, [leaderboardQuery.data, setScopeError]);

  useEffect(() => {
    const result = publishersQuery.data;
    if (!result) return;
    setPublishers(result.data);
    if (result.snapshot_status === "fresh") setScopeError("publishers", null);
  }, [publishersQuery.data, setScopeError]);

  const refreshLeaderboard = useCallback(
    async (category: LeaderboardCategory) => {
      const attemptKey = leaderboardRefreshKey(category);
      markStaleRefreshStart(attemptKey);
      setScopeError("leaderboard", null);
      beginBackgroundRefresh();

      try {
        await tauriInvoke("sync_marketplace_scope", {
          scope: `leaderboard_${category}`,
        });
        await queryClient.invalidateQueries({
          queryKey: leaderboardQueryKey(category),
          exact: true,
        });
        const fresh = await queryClient.fetchQuery({
          queryKey: leaderboardQueryKey(category),
          queryFn: () => fetchLocalLeaderboard(category),
          staleTime: MARKETPLACE_STALE_TIME_MS,
        });
        markStaleRefreshSettled(attemptKey, true);
        // Stale-response guard: the user may have switched category while
        // this background refresh was in flight.
        if (requestedLeaderboardCategoryRef.current !== category) return;
        setLeaderboard(fresh.data);
      } catch (e) {
        markStaleRefreshSettled(attemptKey, false);
        reportScopeError("leaderboard", "sync_failed", e);
      } finally {
        endBackgroundRefresh();
      }
    },
    [
      beginBackgroundRefresh,
      endBackgroundRefresh,
      fetchLocalLeaderboard,
      markStaleRefreshSettled,
      markStaleRefreshStart,
      queryClient,
      reportScopeError,
      setScopeError,
    ],
  );

  const refreshPublishers = useCallback(async () => {
    markStaleRefreshStart(PUBLISHERS_REFRESH_KEY);
    setScopeError("publishers", null);
    beginBackgroundRefresh();

    try {
      await tauriInvoke("sync_marketplace_scope", {
        scope: "official_publishers",
      });
      await queryClient.invalidateQueries({
        queryKey: publishersQueryKey,
        exact: true,
      });
      const fresh = await queryClient.fetchQuery({
        queryKey: publishersQueryKey,
        queryFn: fetchLocalPublishers,
        staleTime: MARKETPLACE_STALE_TIME_MS,
      });
      markStaleRefreshSettled(PUBLISHERS_REFRESH_KEY, true);
      setPublishers(fresh.data);
    } catch (e) {
      markStaleRefreshSettled(PUBLISHERS_REFRESH_KEY, false);
      reportScopeError("publishers", "sync_failed", e);
    } finally {
      endBackgroundRefresh();
    }
  }, [
    beginBackgroundRefresh,
    endBackgroundRefresh,
    fetchLocalPublishers,
    markStaleRefreshSettled,
    markStaleRefreshStart,
    queryClient,
    reportScopeError,
    setScopeError,
  ]);

  useEffect(() => {
    const result = leaderboardQuery.data;
    if (!result || result.snapshot_status !== "stale") return;
    const category = requestedLeaderboardCategory;
    if (!canAttemptStaleRefresh(leaderboardRefreshKey(category))) return;
    void refreshLeaderboard(category);
  }, [canAttemptStaleRefresh, leaderboardQuery.data, refreshLeaderboard, requestedLeaderboardCategory]);

  useEffect(() => {
    const result = publishersQuery.data;
    if (!result || result.snapshot_status !== "stale") return;
    if (!canAttemptStaleRefresh(PUBLISHERS_REFRESH_KEY)) return;
    void refreshPublishers();
  }, [canAttemptStaleRefresh, publishersQuery.data, refreshPublishers]);

  const searchMutation = useMutation({
    mutationFn: ({ query, limit }: { query: string; limit: number }) => readLocalSearch(query, limit),
    onMutate: () => {
      setScopeError("search", null);
    },
    onSuccess: (result, { query }) => {
      if (!isActiveSearchQuery(query)) return;
      setResults(toMarketplaceResult(result.data));
      applySearchSnapshotMeta(query, result);
    },
    onError: (e, { query }) => {
      if (!isActiveSearchQuery(query)) return;
      executedSearchQueryRef.current = normalizeSearchKeyQuery(query);
      reportScopeError("search", "search_failed", e);
    },
  });

  const searchOnlineMutation = useMutation({
    mutationFn: async ({ query, limit }: { query: string; limit: number }) => {
      await tauriInvoke("sync_marketplace_scope", { scope: `search_seed:${query}` });
      await queryClient.invalidateQueries({
        queryKey: searchQueryKey(query, limit),
        exact: true,
      });
      return readLocalSearch(query, limit);
    },
    onMutate: () => {
      setScopeError("search", null);
    },
    onSuccess: (result, { query }) => {
      if (!isActiveSearchQuery(query)) return;
      setResults(toMarketplaceResult(result.data));
      applySearchSnapshotMeta(query, result);
    },
    onError: (e, { query }) => {
      if (!isActiveSearchQuery(query)) return;
      executedSearchQueryRef.current = normalizeSearchKeyQuery(query);
      reportScopeError("search", "search_failed", e);
    },
  });

  const aiSearchMutation = useMutation({
    mutationFn: async ({ query, limit }: { query: string; limit: number }) => {
      const keywords = await tauriInvoke("ai_extract_search_keywords", {
        query,
      });

      const keywordKey = [...keywords].sort().join("\u001f");
      const result = await queryClient.fetchQuery({
        queryKey: ["marketplace", "ai-search", keywordKey, limit],
        queryFn: () =>
          tauriInvoke("ai_search_marketplace_local", {
            keywords,
            limit,
          }),
        staleTime: MARKETPLACE_STALE_TIME_MS,
      });

      return { keywords, result };
    },
  });

  const loading = useMemo(
    () =>
      searchMutation.isPending ||
      (leaderboardEnabled && leaderboardQuery.isPending) ||
      (publishersEnabled && publishersQuery.isPending),
    [
      leaderboardEnabled,
      leaderboardQuery.isPending,
      publishersEnabled,
      publishersQuery.isPending,
      searchMutation.isPending,
    ],
  );

  const search = useCallback(
    async (query: string, limit = 50) => {
      if (!query.trim()) return;
      activeSearchQueryRef.current = normalizeSearchKeyQuery(query);
      await searchMutation.mutateAsync({ query, limit });
    },
    [searchMutation],
  );

  const searchOnline = useCallback(
    async (query: string, limit = 50) => {
      if (!query.trim()) return;
      activeSearchQueryRef.current = normalizeSearchKeyQuery(query);
      await searchOnlineMutation.mutateAsync({ query, limit });
    },
    [searchOnlineMutation],
  );

  const aiSearch = useCallback(
    async (query: string, limit = 50) => {
      if (!query.trim()) return;

      // Stale-response guard: only the most recent invocation may touch state
      // (overlapping searches would otherwise let the slower, older response
      // overwrite the newer results and flip aiSearching off prematurely).
      const seq = ++aiSearchSeqRef.current;
      // The search scope now targets this query, so any plain search still in
      // flight for the previous one is stale.
      activeSearchQueryRef.current = normalizeSearchKeyQuery(query);

      setAiSearching(true);
      setScopeError("search", null);
      setAiKeywords(null);
      setAiAllSkills([]);
      setAiKeywordSkillMap({});
      setAiActiveKeywords(new Set());
      setAiPhase("extracting");

      try {
        const { keywords, result } = await aiSearchMutation.mutateAsync({
          query,
          limit,
        });
        if (aiSearchSeqRef.current !== seq) return;
        // Keyword extraction is an LLM round-trip, so the toolbar text routinely
        // moves on while this call is out. The sequence check alone does not see
        // that (typing does not start a new AI search), so the query the scope
        // now targets is what decides whether this answer still belongs here.
        if (!isActiveSearchQuery(query)) return;
        setAiKeywords(keywords);
        setAiPhase("searching");
        setAiAllSkills(result.data.skills);
        setAiKeywordSkillMap(result.data.keyword_skill_map);
        setAiActiveKeywords(new Set(keywords));
        setResults(toMarketplaceResult(result.data.skills));
        applySearchSnapshotMeta(query, result);
      } catch (e) {
        if (aiSearchSeqRef.current === seq && isActiveSearchQuery(query)) {
          executedSearchQueryRef.current = normalizeSearchKeyQuery(query);
          reportScopeError("search", "search_failed", e);
        }
      } finally {
        if (aiSearchSeqRef.current === seq) {
          setAiSearching(false);
          setAiPhase(null);
        }
      }
    },
    [aiSearchMutation, applySearchSnapshotMeta, isActiveSearchQuery, reportScopeError, setScopeError],
  );

  const toggleAiKeyword = useCallback((keyword: string) => {
    setAiActiveKeywords((prev) => {
      const next = new Set(prev);
      if (next.has(keyword)) {
        next.delete(keyword);
      } else {
        next.add(keyword);
      }
      return next;
    });
  }, []);

  const clearAiSearch = useCallback(() => {
    // Invalidate any in-flight aiSearch so its late response can't resurrect
    // the cleared state.
    aiSearchSeqRef.current += 1;
    setAiSearching(false);
    setAiKeywords(null);
    setAiPhase(null);
    setAiAllSkills([]);
    setAiKeywordSkillMap({});
    setAiActiveKeywords(new Set());
    // The search scope is being abandoned, so its freshness verdict and error
    // banner go with it — otherwise they survive into the next query. Dropping
    // the target query also disowns every plain search still in flight.
    activeSearchQueryRef.current = null;
    clearSearchSnapshot();
  }, [clearSearchSnapshot]);

  /**
   * The toolbar moved to a query nobody has executed yet (the page searches on
   * a debounce, and the user can keep typing past a failed query). Search
   * snapshot meta belongs to one query, so it is retired as soon as the text
   * diverges from the query that produced it instead of hanging on the new one.
   *
   * This also re-targets the scope, so a request still in flight for the
   * previous query cannot re-park its verdict, results or keywords here when it
   * answers — every response path checks `isActiveSearchQuery` first. It does
   * not cancel that request: an AI search keeps its spinner until its own
   * response lands (only `clearAiSearch` or a newer AI search retires that),
   * and it stays a no-op on the rest of the state.
   */
  const notePendingSearchQuery = useCallback(
    (query: string) => {
      const normalized = normalizeSearchKeyQuery(query);
      activeSearchQueryRef.current = normalized;
      const executed = executedSearchQueryRef.current;
      if (executed === null || executed === normalized) return;
      clearSearchSnapshot();
    },
    [clearSearchSnapshot],
  );

  const fetchLeaderboard = useCallback(
    async (category = "all") => {
      const normalized = normalizeLeaderboardCategory(category);
      setScopeError("leaderboard", null);
      resetStaleRefresh(leaderboardRefreshKey(normalized));
      setRequestedLeaderboardCategory(normalized);
      setLeaderboardEnabled(true);
    },
    [resetStaleRefresh, setScopeError],
  );

  const fetchOfficialPublishers = useCallback(async () => {
    setScopeError("publishers", null);
    resetStaleRefresh(PUBLISHERS_REFRESH_KEY);
    setPublishersEnabled(true);
  }, [resetStaleRefresh, setScopeError]);

  /**
   * User-driven recovery for a scope: clears the failure budget and re-runs the
   * same sync + refetch path the automatic stale refresh uses. `search` has no
   * background path — the page re-runs `searchOnline` for that scope instead.
   */
  const retrySnapshot = useCallback(
    async (scope: MarketplaceScope) => {
      if (scope === "leaderboard") {
        const category = requestedLeaderboardCategoryRef.current;
        resetStaleRefresh(leaderboardRefreshKey(category));
        await refreshLeaderboard(category);
        return;
      }
      if (scope === "publishers") {
        resetStaleRefresh(PUBLISHERS_REFRESH_KEY);
        await refreshPublishers();
      }
    },
    [refreshLeaderboard, refreshPublishers, resetStaleRefresh],
  );

  const snapshots = useMemo<Record<MarketplaceScope, MarketplaceSnapshotState>>(
    () => ({
      leaderboard: leaderboardEnabled
        ? deriveSnapshotState("leaderboard", leaderboardQuery.data, leaderboardQuery.error, scopeErrors.leaderboard)
        : EMPTY_SNAPSHOT_STATE,
      publishers: publishersEnabled
        ? deriveSnapshotState("publishers", publishersQuery.data, publishersQuery.error, scopeErrors.publishers)
        : EMPTY_SNAPSHOT_STATE,
      search: {
        status: searchStatus,
        updatedAt: searchUpdatedAt,
        error: scopeErrors.search,
      },
    }),
    [
      leaderboardEnabled,
      leaderboardQuery.data,
      leaderboardQuery.error,
      publishersEnabled,
      publishersQuery.data,
      publishersQuery.error,
      scopeErrors,
      searchStatus,
      searchUpdatedAt,
    ],
  );

  const patchSkill = useCallback(
    (name: string, updater: (skill: Skill) => Skill) => {
      const apply = (skills: Skill[]) => skills.map((skill) => (skill.name === name ? updater(skill) : skill));

      setLeaderboard((prev) => apply(prev));
      setResults((prev) =>
        prev
          ? {
              ...prev,
              skills: apply(prev.skills),
            }
          : prev,
      );
      setAiAllSkills((prev) => apply(prev));

      queryClient.setQueriesData<LocalFirstResult<Skill[]>>(
        { queryKey: [...MARKETPLACE_QUERY_ROOT, "leaderboard"] },
        (prev) =>
          prev
            ? {
                ...prev,
                data: apply(prev.data),
              }
            : prev,
      );

      queryClient.setQueriesData<LocalFirstResult<Skill[]>>(
        { queryKey: [...MARKETPLACE_QUERY_ROOT, "search"] },
        (prev) =>
          prev
            ? {
                ...prev,
                data: apply(prev.data),
              }
            : prev,
      );

      queryClient.setQueriesData<LocalFirstResult<AiKeywordSearchResult>>(
        { queryKey: [...MARKETPLACE_QUERY_ROOT, "ai-search"] },
        (prev) =>
          prev
            ? {
                ...prev,
                data: {
                  ...prev.data,
                  skills: apply(prev.data.skills),
                },
              }
            : prev,
      );
    },
    [queryClient],
  );

  return {
    results,
    leaderboard,
    publishers,
    loading,
    refreshing: refreshing || searchOnlineMutation.isPending,
    /** Per-scope snapshot status/updatedAt/error. The page picks the scope it renders. */
    snapshots,
    aiKeywords,
    aiSearching,
    aiPhase,
    aiAllSkills,
    aiKeywordSkillMap,
    aiActiveKeywords,
    search,
    searchOnline,
    aiSearch,
    toggleAiKeyword,
    clearAiSearch,
    notePendingSearchQuery,
    fetchLeaderboard,
    fetchOfficialPublishers,
    retrySnapshot,
    patchSkill,
  };
}

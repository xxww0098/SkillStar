import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type {
  AiKeywordSearchResult,
  LocalFirstResult,
  MarketplaceResult,
  OfficialPublisher,
  Skill,
  SnapshotStatus,
} from "../../../types";

export type AiSearchPhase = "extracting" | "searching" | null;

const MARKETPLACE_STALE_TIME_MS = 5 * 60 * 1000;
const MARKETPLACE_QUERY_ROOT = ["marketplace"] as const;

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

export function useMarketplace() {
  const queryClient = useQueryClient();
  const [results, setResults] = useState<MarketplaceResult | null>(null);
  const [leaderboard, setLeaderboard] = useState<Skill[]>([]);
  const [publishers, setPublishers] = useState<OfficialPublisher[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [snapshotStatus, setSnapshotStatus] = useState<SnapshotStatus>("fresh");
  const [snapshotUpdatedAt, setSnapshotUpdatedAt] = useState<string | null>(null);
  const [requestedLeaderboardCategory, setRequestedLeaderboardCategory] =
    useState<LeaderboardCategory>("all");
  const [leaderboardEnabled, setLeaderboardEnabled] = useState(false);
  const [publishersEnabled, setPublishersEnabled] = useState(false);
  const [aiKeywords, setAiKeywords] = useState<string[] | null>(null);
  const [aiSearching, setAiSearching] = useState(false);
  const [aiPhase, setAiPhase] = useState<AiSearchPhase>(null);
  const [aiAllSkills, setAiAllSkills] = useState<Skill[]>([]);
  const [aiKeywordSkillMap, setAiKeywordSkillMap] = useState<
    Record<string, string[]>
  >({});
  const [aiActiveKeywords, setAiActiveKeywords] = useState<Set<string>>(new Set());
  const staleRefreshAttemptedRef = useRef<Set<string>>(new Set());
  const inFlightRefreshesRef = useRef(0);

  const applySnapshotMeta = useCallback(<T,>(result: LocalFirstResult<T>) => {
    setSnapshotStatus(result.snapshot_status);
    setSnapshotUpdatedAt(result.snapshot_updated_at);
    if (result.snapshot_status === "remote_error") {
      setError(result.error ?? "Marketplace request failed");
    }
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

  const fetchLocalLeaderboard = useCallback(
    (category: LeaderboardCategory) =>
      invoke<LocalFirstResult<Skill[]>>("get_leaderboard_local", {
        category,
      }),
    [],
  );

  const fetchLocalPublishers = useCallback(
    () => invoke<LocalFirstResult<OfficialPublisher[]>>("get_publishers_local"),
    [],
  );

  const readLocalSearch = useCallback(
    (query: string, limit: number) =>
      queryClient.fetchQuery({
        queryKey: searchQueryKey(query, limit),
        queryFn: () =>
          invoke<LocalFirstResult<Skill[]>>("search_marketplace_local", {
            query,
            limit,
          }),
        staleTime: MARKETPLACE_STALE_TIME_MS,
      }),
    [queryClient],
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
    applySnapshotMeta(result);
  }, [applySnapshotMeta, leaderboardQuery.data]);

  useEffect(() => {
    const result = publishersQuery.data;
    if (!result) return;
    setPublishers(result.data);
    applySnapshotMeta(result);
  }, [applySnapshotMeta, publishersQuery.data]);

  useEffect(() => {
    if (!leaderboardQuery.error) return;
    setError(String(leaderboardQuery.error));
  }, [leaderboardQuery.error]);

  useEffect(() => {
    if (!publishersQuery.error) return;
    setError(String(publishersQuery.error));
  }, [publishersQuery.error]);

  useEffect(() => {
    const result = leaderboardQuery.data;
    if (!result || result.snapshot_status !== "stale") return;

    const attemptKey = `leaderboard:${requestedLeaderboardCategory}`;
    if (staleRefreshAttemptedRef.current.has(attemptKey)) return;
    staleRefreshAttemptedRef.current.add(attemptKey);
    beginBackgroundRefresh();

    void (async () => {
      try {
        await invoke("sync_marketplace_scope", {
          scope: `leaderboard_${requestedLeaderboardCategory}`,
        });
        await queryClient.invalidateQueries({
          queryKey: leaderboardQueryKey(requestedLeaderboardCategory),
          exact: true,
        });
        const fresh = await queryClient.fetchQuery({
          queryKey: leaderboardQueryKey(requestedLeaderboardCategory),
          queryFn: () => fetchLocalLeaderboard(requestedLeaderboardCategory),
          staleTime: MARKETPLACE_STALE_TIME_MS,
        });
        setLeaderboard(fresh.data);
        applySnapshotMeta(fresh);
      } catch (e) {
        setError(String(e));
      } finally {
        endBackgroundRefresh();
      }
    })();
  }, [
    applySnapshotMeta,
    beginBackgroundRefresh,
    endBackgroundRefresh,
    fetchLocalLeaderboard,
    leaderboardQuery.data,
    queryClient,
    requestedLeaderboardCategory,
  ]);

  useEffect(() => {
    const result = publishersQuery.data;
    if (!result || result.snapshot_status !== "stale") return;

    const attemptKey = "publishers";
    if (staleRefreshAttemptedRef.current.has(attemptKey)) return;
    staleRefreshAttemptedRef.current.add(attemptKey);
    beginBackgroundRefresh();

    void (async () => {
      try {
        await invoke("sync_marketplace_scope", {
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
        setPublishers(fresh.data);
        applySnapshotMeta(fresh);
      } catch (e) {
        setError(String(e));
      } finally {
        endBackgroundRefresh();
      }
    })();
  }, [
    applySnapshotMeta,
    beginBackgroundRefresh,
    endBackgroundRefresh,
    fetchLocalPublishers,
    publishersQuery.data,
    queryClient,
  ]);

  const searchMutation = useMutation({
    mutationFn: ({ query, limit }: { query: string; limit: number }) =>
      readLocalSearch(query, limit),
    onMutate: () => {
      setError(null);
    },
    onSuccess: (result) => {
      setResults(toMarketplaceResult(result.data));
      applySnapshotMeta(result);
    },
    onError: (e) => {
      setError(String(e));
    },
  });

  const searchOnlineMutation = useMutation({
    mutationFn: async ({ query, limit }: { query: string; limit: number }) => {
      await invoke("sync_marketplace_scope", { scope: `search_seed:${query}` });
      await queryClient.invalidateQueries({
        queryKey: searchQueryKey(query, limit),
        exact: true,
      });
      return readLocalSearch(query, limit);
    },
    onMutate: () => {
      setError(null);
    },
    onSuccess: (result) => {
      setResults(toMarketplaceResult(result.data));
      applySnapshotMeta(result);
    },
    onError: (e) => {
      setError(String(e));
    },
  });

  const aiSearchMutation = useMutation({
    mutationFn: async ({ query, limit }: { query: string; limit: number }) => {
      const keywords = await invoke<string[]>("ai_extract_search_keywords", {
        query,
      });

      const keywordKey = [...keywords].sort().join("\u001f");
      const result = await queryClient.fetchQuery({
        queryKey: ["marketplace", "ai-search", keywordKey, limit],
        queryFn: () =>
          invoke<LocalFirstResult<AiKeywordSearchResult>>("ai_search_marketplace_local", {
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
      await searchMutation.mutateAsync({ query, limit });
    },
    [searchMutation],
  );

  const searchOnline = useCallback(
    async (query: string, limit = 50) => {
      if (!query.trim()) return;
      await searchOnlineMutation.mutateAsync({ query, limit });
    },
    [searchOnlineMutation],
  );

  const aiSearch = useCallback(
    async (query: string, limit = 50) => {
      if (!query.trim()) return;

      setAiSearching(true);
      setError(null);
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
        setAiKeywords(keywords);
        setAiPhase("searching");
        setAiAllSkills(result.data.skills);
        setAiKeywordSkillMap(result.data.keyword_skill_map);
        setAiActiveKeywords(new Set(keywords));
        setResults(toMarketplaceResult(result.data.skills));
        applySnapshotMeta(result);
      } catch (e) {
        setError(String(e));
      } finally {
        setAiSearching(false);
        setAiPhase(null);
      }
    },
    [aiSearchMutation, applySnapshotMeta],
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
    setAiKeywords(null);
    setAiPhase(null);
    setAiAllSkills([]);
    setAiKeywordSkillMap({});
    setAiActiveKeywords(new Set());
  }, []);

  const fetchLeaderboard = useCallback(
    async (category = "all") => {
      const normalized = normalizeLeaderboardCategory(category);
      setError(null);
      staleRefreshAttemptedRef.current.delete(`leaderboard:${normalized}`);
      setRequestedLeaderboardCategory(normalized);
      setLeaderboardEnabled(true);
    },
    [],
  );

  const fetchOfficialPublishers = useCallback(async () => {
    setError(null);
    staleRefreshAttemptedRef.current.delete("publishers");
    setPublishersEnabled(true);
  }, []);

  const patchSkill = useCallback(
    (name: string, updater: (skill: Skill) => Skill) => {
      const apply = (skills: Skill[]) =>
        skills.map((skill) => (skill.name === name ? updater(skill) : skill));

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
    error,
    snapshotStatus,
    snapshotUpdatedAt,
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
    fetchLeaderboard,
    fetchOfficialPublishers,
    patchSkill,
  };
}

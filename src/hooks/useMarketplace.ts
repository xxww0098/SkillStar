import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  AiKeywordSearchResult,
  LocalFirstResult,
  MarketplaceResult,
  OfficialPublisher,
  Skill,
  SnapshotStatus,
} from "../types";

export type AiSearchPhase = "extracting" | "searching" | null;

function toMarketplaceResult(skills: Skill[]): MarketplaceResult {
  return {
    skills,
    total_count: skills.length,
    page: 1,
    has_more: false,
  };
}

export function useMarketplace() {
  const mountedRef = useRef(true);
  const [results, setResults] = useState<MarketplaceResult | null>(null);
  const [leaderboard, setLeaderboard] = useState<Skill[]>([]);
  const [publishers, setPublishers] = useState<OfficialPublisher[]>([]);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [snapshotStatus, setSnapshotStatus] = useState<SnapshotStatus>("fresh");
  const [snapshotUpdatedAt, setSnapshotUpdatedAt] = useState<string | null>(
    null,
  );
  const [aiKeywords, setAiKeywords] = useState<string[] | null>(null);
  const [aiSearching, setAiSearching] = useState(false);
  const [aiPhase, setAiPhase] = useState<AiSearchPhase>(null);
  const [aiAllSkills, setAiAllSkills] = useState<Skill[]>([]);
  const [aiKeywordSkillMap, setAiKeywordSkillMap] = useState<
    Record<string, string[]>
  >({});
  const [aiActiveKeywords, setAiActiveKeywords] = useState<Set<string>>(
    new Set(),
  );

  const applySnapshotMeta = useCallback(<T>(result: LocalFirstResult<T>) => {
    if (!mountedRef.current) return;
    setSnapshotStatus(result.snapshot_status);
    setSnapshotUpdatedAt(result.snapshot_updated_at);
    if (result.snapshot_status === "remote_error") {
      setError(result.error ?? "Marketplace request failed");
    }
  }, []);

  const runScopeRefresh = useCallback(
    async <T>(
      scope: string,
      reread: () => Promise<LocalFirstResult<T>>,
      onData: (data: T) => void,
    ) => {
      setRefreshing(true);
      try {
        await invoke("sync_marketplace_scope", { scope });
        if (!mountedRef.current) return;
        const fresh = await reread();
        if (!mountedRef.current) return;
        onData(fresh.data);
        applySnapshotMeta(fresh);
      } catch (e) {
        if (!mountedRef.current) return;
        setError(String(e));
      } finally {
        if (mountedRef.current) setRefreshing(false);
      }
    },
    [applySnapshotMeta],
  );

  const search = useCallback(
    async (query: string, limit = 50) => {
      if (!query.trim()) return;
      setLoading(true);
      setError(null);
      try {
        const result = await invoke<LocalFirstResult<Skill[]>>(
          "search_marketplace_local",
          { query, limit },
        );
        if (!mountedRef.current) return;
        setResults(toMarketplaceResult(result.data));
        applySnapshotMeta(result);
      } catch (e) {
        if (!mountedRef.current) return;
        setError(String(e));
      } finally {
        if (mountedRef.current) setLoading(false);
      }
    },
    [applySnapshotMeta],
  );

  const searchOnline = useCallback(
    async (query: string, limit = 50) => {
      if (!query.trim()) return;
    setRefreshing(true);
    setError(null);
    try {
      await invoke("sync_marketplace_scope", { scope: `search_seed:${query}` });
        if (!mountedRef.current) return;
        const result = await invoke<LocalFirstResult<Skill[]>>(
          "search_marketplace_local",
          { query, limit },
        );
        if (!mountedRef.current) return;
        setResults(toMarketplaceResult(result.data));
        applySnapshotMeta(result);
      } catch (e) {
        if (!mountedRef.current) return;
        setError(String(e));
      } finally {
        if (mountedRef.current) setRefreshing(false);
      }
    },
    [applySnapshotMeta],
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
        const keywords = await invoke<string[]>("ai_extract_search_keywords", {
          query,
        });
        if (!mountedRef.current) return;
        setAiKeywords(keywords);
        setAiPhase("searching");

        const result = await invoke<LocalFirstResult<AiKeywordSearchResult>>(
          "ai_search_marketplace_local",
          { keywords, limit },
        );
        if (!mountedRef.current) return;
        setAiAllSkills(result.data.skills);
        setAiKeywordSkillMap(result.data.keyword_skill_map);
        setAiActiveKeywords(new Set(keywords));
        setResults(toMarketplaceResult(result.data.skills));
        applySnapshotMeta(result);
      } catch (e) {
        if (!mountedRef.current) return;
        setError(String(e));
      } finally {
        if (mountedRef.current) {
          setAiSearching(false);
          setAiPhase(null);
        }
      }
    },
    [applySnapshotMeta],
  );

  const toggleAiKeyword = useCallback((keyword: string) => {
    setAiActiveKeywords((prev) => {
      const next = new Set(prev);
      if (next.has(keyword)) {
        if (next.size <= 1) return prev;
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
      const normalizedCategory =
        category === "hot" || category === "trending" ? category : "all";
      const readLocal = () =>
        invoke<LocalFirstResult<Skill[]>>("get_leaderboard_local", {
          category: normalizedCategory,
        });

      setLoading(true);
      setError(null);
      try {
        const result = await readLocal();
        if (!mountedRef.current) return;
        setLeaderboard(result.data);
        applySnapshotMeta(result);
        if (result.snapshot_status === "stale") {
          void runScopeRefresh(
            `leaderboard_${normalizedCategory}`,
            readLocal,
            setLeaderboard,
          );
        }
      } catch (e) {
        if (!mountedRef.current) return;
        setError(String(e));
      } finally {
        if (mountedRef.current) setLoading(false);
      }
    },
    [applySnapshotMeta, runScopeRefresh],
  );

  const fetchOfficialPublishers = useCallback(async () => {
    const readLocal = () =>
      invoke<LocalFirstResult<OfficialPublisher[]>>("get_publishers_local");

    setLoading(true);
    setError(null);
    try {
      const result = await readLocal();
      if (!mountedRef.current) return;
      setPublishers(result.data);
      applySnapshotMeta(result);
      if (result.snapshot_status === "stale") {
        void runScopeRefresh("official_publishers", readLocal, setPublishers);
      }
    } catch (e) {
      if (!mountedRef.current) return;
      setError(String(e));
    } finally {
      if (mountedRef.current) setLoading(false);
    }
  }, [applySnapshotMeta, runScopeRefresh]);

  // Cleanup mounted flag
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
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
    },
    [],
  );

  return {
    results,
    leaderboard,
    publishers,
    loading,
    refreshing,
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

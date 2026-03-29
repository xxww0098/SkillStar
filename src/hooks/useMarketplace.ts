import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { applyMarketplaceDescriptionPatches } from "../lib/marketplaceDescriptionHydration";
import type {
  Skill,
  MarketplaceResult,
  OfficialPublisher,
  MarketplaceDescriptionPatch,
} from "../types";

export function useMarketplace() {
  const [results, setResults] = useState<MarketplaceResult | null>(null);
  const [leaderboard, setLeaderboard] = useState<Skill[]>([]);
  const [publishers, setPublishers] = useState<OfficialPublisher[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** Search skills.sh */
  const search = useCallback(async (query: string) => {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<MarketplaceResult>("search_skills_sh", { query });
      setResults(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  /** Fetch skills.sh leaderboard: "all" | "trending" | "hot" */
  const fetchLeaderboard = useCallback(async (category: string = "all") => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<Skill[]>("get_skills_sh_leaderboard", { category });
      setLeaderboard(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  /** Fetch official publishers from skills.sh/official */
  const fetchOfficialPublishers = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<OfficialPublisher[]>("get_official_publishers");
      setPublishers(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const applyDescriptionPatches = useCallback(
    (patches: MarketplaceDescriptionPatch[]) => {
      if (patches.length === 0) return;

      setResults((prev) => {
        if (!prev) return prev;
        return {
          ...prev,
          skills: applyMarketplaceDescriptionPatches(prev.skills, patches),
        };
      });
      setLeaderboard((prev) => applyMarketplaceDescriptionPatches(prev, patches));
    },
    []
  );

  return {
    results,
    leaderboard,
    publishers,
    loading,
    error,
    search,
    fetchLeaderboard,
    fetchOfficialPublishers,
    applyDescriptionPatches,
  };
}

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import type { ModelAppId } from "../components/AppCapsuleSwitcher";

export interface DashboardProviderEntry {
  providerId: string;
  name: string;
  healthStatus: "healthy" | "degraded" | "unreachable" | "unknown";
  latencyMs: number | null;
  checkedAt: number | null;
  usagePercent: number | null;
  remaining: string | null;
  resetTime: string | null;
  planName: string | null;
  error: string | null;
}

export interface DashboardSummary {
  total: number;
  healthy: number;
  degraded: number;
  unreachable: number;
  unknown: number;
}

export interface ProviderHealthDashboard {
  appId: string;
  entries: DashboardProviderEntry[];
  summary: DashboardSummary;
  refreshedAt: number;
}

export interface UseProviderHealthDashboardReturn {
  dashboard: ProviderHealthDashboard | null;
  loading: boolean;
  refreshing: boolean;
  load: () => Promise<void>;
  refresh: () => Promise<void>;
}

export function useProviderHealthDashboard(appId: ModelAppId): UseProviderHealthDashboardReturn {
  const [dashboard, setDashboard] = useState<ProviderHealthDashboard | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await invoke<ProviderHealthDashboard>("get_provider_health_dashboard", { appId });
      setDashboard(data);
    } catch (e) {
      console.error("Failed to load health dashboard:", e);
      setDashboard(null);
    } finally {
      setLoading(false);
    }
  }, [appId]);

  const refresh = useCallback(async () => {
    if (refreshing) return;
    setRefreshing(true);
    try {
      const data = await invoke<ProviderHealthDashboard>("refresh_provider_health_dashboard", { appId });
      setDashboard(data);
    } catch (e) {
      console.error("Failed to refresh health dashboard:", e);
    } finally {
      setRefreshing(false);
    }
  }, [appId, refreshing]);

  useEffect(() => {
    load();
  }, [load]);

  return { dashboard, loading, refreshing, load, refresh };
}

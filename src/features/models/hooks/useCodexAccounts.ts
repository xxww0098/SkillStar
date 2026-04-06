import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-shell";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

// ── Types ────────────────────────────────────────────────────────────

export interface CodexTokens {
  idToken: string;
  accessToken: string;
  refreshToken?: string;
}

export interface CodexQuota {
  hourlyPercentage: number;
  hourlyResetTime?: number;
  hourlyWindowMinutes?: number;
  hourlyWindowPresent?: boolean;
  weeklyPercentage: number;
  weeklyResetTime?: number;
  weeklyWindowMinutes?: number;
  weeklyWindowPresent?: boolean;
}

export interface CodexQuotaError {
  code?: string;
  message: string;
  timestamp: number;
}

export interface CodexAccount {
  id: string;
  email: string;
  authMode: "oauth" | "apikey";
  openaiApiKey?: string;
  apiBaseUrl?: string;
  userId?: string;
  planType?: string;
  accountId?: string;
  accountName?: string;
  tokens: CodexTokens;
  quota?: CodexQuota;
  quotaError?: CodexQuotaError;
  createdAt: number;
  lastUsed: number;
}

interface OAuthLoginStartResponse {
  loginId: string;
  authUrl: string;
}

// ── Hook ─────────────────────────────────────────────────────────────

export function useCodexAccounts() {
  const [accounts, setAccounts] = useState<CodexAccount[]>([]);
  const [currentId, setCurrentId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [oauthLoading, setOauthLoading] = useState(false);
  const [quotaRefreshing, setQuotaRefreshing] = useState<Set<string>>(new Set());
  const activeLoginId = useRef<string | null>(null);

  // ── Load ─────────────────────────────────
  const load = useCallback(async () => {
    try {
      const [accts, currId] = await Promise.all([
        invoke<CodexAccount[]>("list_codex_accounts"),
        invoke<string | null>("get_current_codex_account_id"),
      ]);
      setAccounts(accts);
      setCurrentId(currId);
    } catch (e) {
      console.error("Failed to load codex accounts:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // ── Listen to Global Refresh Event ─────────────────────────
  useEffect(() => {
    const handleRefresh = () => load();
    window.addEventListener("codex-accounts-refresh", handleRefresh);
    return () => window.removeEventListener("codex-accounts-refresh", handleRefresh);
  }, [load]);

  const dispatchRefresh = useCallback(() => {
    window.dispatchEvent(new CustomEvent("codex-accounts-refresh"));
  }, []);

  // ── Listen to OAuth events ─────────────────────────────────
  useEffect(() => {
    const unlisteners: (() => void)[] = [];

    listen<{ loginId: string }>("codex-oauth-login-completed", async (event) => {
      const { loginId } = event.payload;
      if (activeLoginId.current !== loginId) return;

      try {
        const account = await invoke<CodexAccount>("codex_oauth_complete", { loginId });
        setOauthLoading(false);
        activeLoginId.current = null;
        toast.success(`OAuth 登录成功: ${account.email}`, {
          description: "如 Codex 正在运行，请手动重启以使新凭证生效",
          duration: 5000,
        });
        dispatchRefresh(); // Tell all instances to reload
        window.dispatchEvent(new CustomEvent("model-providers-refresh"));
      } catch (e) {
        setOauthLoading(false);
        activeLoginId.current = null;
        toast.error(`OAuth 登录完成失败: ${e}`);
      }
    }).then((f) => unlisteners.push(f));

    listen<{ loginId: string }>("codex-oauth-login-timeout", () => {
      setOauthLoading(false);
      activeLoginId.current = null;
      toast.error("OAuth 登录超时，请重试");
    }).then((f) => unlisteners.push(f));

    return () => {
      for (const fn of unlisteners) fn();
    };
  }, [dispatchRefresh]);

  // ── Start OAuth ─────────────────────────────────
  const startOAuth = useCallback(async () => {
    if (oauthLoading) return;
    setOauthLoading(true);

    try {
      const result = await invoke<OAuthLoginStartResponse>("codex_oauth_start");
      activeLoginId.current = result.loginId;

      // Open browser using Tauri shell API
      await open(result.authUrl);
      toast.info("请在浏览器中完成 OpenAI 登录授权");
    } catch (e) {
      setOauthLoading(false);
      toast.error(`启动 OAuth 失败: ${e}`);
    }
  }, [oauthLoading]);

  // ── Cancel OAuth ─────────────────────────────────
  const cancelOAuth = useCallback(async () => {
    try {
      await invoke("codex_oauth_cancel", {
        loginId: activeLoginId.current,
      });
    } catch {
      // ignore
    }
    setOauthLoading(false);
    activeLoginId.current = null;
  }, []);

  // ── Switch Account ─────────────────────────────────
  const switchAccount = useCallback(
    async (accountId: string) => {
      try {
        const updated = await invoke<CodexAccount>("switch_codex_account", { accountId });
        setCurrentId(accountId); // Optimistic: update local state immediately
        toast.success(`已切换到 ${updated.email}`, {
          description: "如 Codex 正在运行，请手动重启以使新凭证生效",
          duration: 5000,
        });
        dispatchRefresh();
        // Refresh provider list — backend cleared provider current
        window.dispatchEvent(new CustomEvent("model-providers-refresh"));
        window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
      } catch (e) {
        toast.error(`切换失败: ${e}`);
      }
    },
    [dispatchRefresh],
  );

  // ── Delete Account ─────────────────────────────────
  const deleteAccount = useCallback(
    async (accountId: string) => {
      try {
        await invoke("delete_codex_account", { accountId });
        toast.success("账号已移除");
        dispatchRefresh();
      } catch (e) {
        toast.error(`删除失败: ${e}`);
      }
    },
    [dispatchRefresh],
  );

  // ── Refresh Single Quota ─────────────────────────────────
  const refreshQuota = useCallback(async (accountId: string) => {
    setQuotaRefreshing((prev) => new Set(prev).add(accountId));
    try {
      const quota = await invoke<CodexQuota>("refresh_codex_quota", { accountId });
      setAccounts((prev) => prev.map((a) => (a.id === accountId ? { ...a, quota, quotaError: undefined } : a)));
    } catch (e) {
      toast.error(`刷新配额失败: ${e}`);
    } finally {
      setQuotaRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(accountId);
        return next;
      });
    }
  }, []);

  // ── Refresh All Quotas ─────────────────────────────────
  const refreshAllQuotas = useCallback(async () => {
    const oauthAccounts = accounts.filter((a) => a.authMode === "oauth");
    if (oauthAccounts.length === 0) return;

    const allIds = new Set(oauthAccounts.map((a) => a.id));
    setQuotaRefreshing(allIds);

    try {
      await invoke("refresh_all_codex_quotas");
      dispatchRefresh();
    } catch (e) {
      console.error("Refresh all quotas error:", e);
    } finally {
      setQuotaRefreshing(new Set());
    }
  }, [accounts, load]);

  // ── Add API Key Account ─────────────────────────────────
  const addApiKeyAccount = useCallback(
    async (apiKey: string, apiBaseUrl?: string) => {
      try {
        const account = await invoke<CodexAccount>("add_codex_api_key_account", {
          apiKey,
          apiBaseUrl: apiBaseUrl || null,
        });
        toast.success(`API Key 账号已添加: ${account.email}`, {
          description: "如 Codex 正在运行，请手动重启以使新凭证生效",
          duration: 5000,
        });
        dispatchRefresh();
        window.dispatchEvent(new CustomEvent("model-providers-refresh"));
      } catch (e) {
        toast.error(`添加失败: ${e}`);
      }
    },
    [dispatchRefresh],
  );

  return {
    accounts,
    currentId,
    loading,
    oauthLoading,
    quotaRefreshing,
    startOAuth,
    cancelOAuth,
    switchAccount,
    deleteAccount,
    refreshQuota,
    refreshAllQuotas,
    addApiKeyAccount,
    reload: load,
  };
}

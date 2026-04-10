import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { openExternalUrl } from "../../../lib/externalOpen";

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

export function useCodexAccounts() {
  const [accounts, setAccounts] = useState<CodexAccount[]>([]);
  const [currentId, setCurrentId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [oauthLoading, setOauthLoading] = useState(false);
  const [quotaRefreshing, setQuotaRefreshing] = useState<Set<string>>(new Set());
  const activeLoginId = useRef<string | null>(null);
  const completingLoginId = useRef<string | null>(null);

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
    void load();
  }, [load]);

  useEffect(() => {
    const handleRefresh = () => {
      void load();
    };
    window.addEventListener("codex-accounts-refresh", handleRefresh);
    return () => window.removeEventListener("codex-accounts-refresh", handleRefresh);
  }, [load]);

  const dispatchRefresh = useCallback(() => {
    window.dispatchEvent(new CustomEvent("codex-accounts-refresh"));
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlistenCompleted: UnlistenFn | null = null;
    let unlistenTimeout: UnlistenFn | null = null;

    const handleCompleted = async (event: { payload: { loginId: string } }) => {
      const { loginId } = event.payload;
      if (activeLoginId.current !== loginId) return;
      if (completingLoginId.current === loginId) return;
      completingLoginId.current = loginId;

      try {
        const account = await invoke<CodexAccount>("codex_oauth_complete", { loginId });
        setOauthLoading(false);
        activeLoginId.current = null;
        toast.success(`OAuth 登录成功: ${account.email}`, {
          description: "如 Codex 正在运行，请手动重启以使新凭证生效",
          duration: 5000,
        });
        dispatchRefresh();
        window.dispatchEvent(new CustomEvent("model-providers-refresh"));
      } catch (e) {
        setOauthLoading(false);
        activeLoginId.current = null;
        toast.error(`OAuth 登录完成失败: ${e}`);
      } finally {
        if (completingLoginId.current === loginId) {
          completingLoginId.current = null;
        }
      }
    };

    const handleTimeout = (event: { payload: { loginId: string } }) => {
      const { loginId } = event.payload;
      if (activeLoginId.current !== loginId) return;
      setOauthLoading(false);
      activeLoginId.current = null;
      if (completingLoginId.current === loginId) {
        completingLoginId.current = null;
      }
      toast.error("OAuth 登录超时，请重试");
    };

    const registerListeners = async () => {
      const completed = await listen<{ loginId: string }>("codex-oauth-login-completed", handleCompleted);
      if (disposed) {
        completed();
      } else {
        unlistenCompleted = completed;
      }

      const timeout = await listen<{ loginId: string }>("codex-oauth-login-timeout", handleTimeout);
      if (disposed) {
        timeout();
      } else {
        unlistenTimeout = timeout;
      }
    };

    void registerListeners();

    return () => {
      disposed = true;
      unlistenCompleted?.();
      unlistenTimeout?.();
    };
  }, [dispatchRefresh]);

  const startOAuth = useCallback(async () => {
    if (oauthLoading) return;
    setOauthLoading(true);
    completingLoginId.current = null;

    try {
      const result = await invoke<OAuthLoginStartResponse>("codex_oauth_start");
      activeLoginId.current = result.loginId;

      const opened = await openExternalUrl(result.authUrl);
      if (!opened) {
        throw new Error("Failed to open browser for OAuth");
      }

      toast.info("请在浏览器中完成 OpenAI 登录授权");
    } catch (e) {
      setOauthLoading(false);
      activeLoginId.current = null;
      completingLoginId.current = null;
      toast.error(`启动 OAuth 失败: ${e}`);
    }
  }, [oauthLoading]);

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
    completingLoginId.current = null;
  }, []);

  const switchAccount = useCallback(
    async (accountId: string) => {
      try {
        const updated = await invoke<CodexAccount>("switch_codex_account", { accountId });
        setCurrentId(accountId);
        toast.success(`已切换到 ${updated.email}`, {
          description: "如 Codex 正在运行，请手动重启以使新凭证生效",
          duration: 5000,
        });
        dispatchRefresh();
        window.dispatchEvent(new CustomEvent("model-providers-refresh"));
        window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
      } catch (e) {
        toast.error(`切换失败: ${e}`);
      }
    },
    [dispatchRefresh],
  );

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

  const refreshQuota = useCallback(async (accountId: string) => {
    setQuotaRefreshing((prev) => new Set(prev).add(accountId));
    toast("正在获取配额信息...");
    try {
      const quota = await invoke<CodexQuota>("refresh_codex_quota", { accountId });
      setAccounts((prev) => prev.map((a) => (a.id === accountId ? { ...a, quota, quotaError: undefined } : a)));
      toast.success("配额刷新成功");
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

  const refreshAllQuotas = useCallback(async () => {
    const oauthAccounts = accounts.filter((a) => a.authMode === "oauth");
    if (oauthAccounts.length === 0) return;

    const allIds = new Set(oauthAccounts.map((a) => a.id));
    setQuotaRefreshing(allIds);
    toast("正在同步刷新配额...");

    try {
      await invoke("refresh_all_codex_quotas");
      toast.success("所有账号配额已刷新");
      dispatchRefresh();
    } catch (e) {
      toast.error(`批量刷新配额失败: ${e}`);
      console.error("Refresh all quotas error:", e);
    } finally {
      setQuotaRefreshing(new Set());
    }
  }, [accounts, dispatchRefresh]);

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

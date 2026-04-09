import { AnimatePresence, motion } from "framer-motion";
import { Key, Loader2, LogIn, Save, ShieldPlus, X, RefreshCw } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";
import { toast } from "sonner";

import { cn } from "../../../lib/utils";
import { useModelProviders, ProviderEntry } from "../hooks/useModelProviders";
import { useGeminiOAuth } from "../hooks/useGeminiOAuth";
import { ApiKeyInput } from "./shared/ApiKeyInput";
import { GeminiAccountRow } from "./shared/GeminiAccountRow";

function AddApiKeyForm({
  onSubmit,
  onCancel,
}: {
  onSubmit: (key: string) => void;
  onCancel: () => void;
}) {
  const [apiKey, setApiKey] = useState("");

  return (
    <motion.div
      initial={{ height: 0, opacity: 0 }}
      animate={{ height: "auto", opacity: 1 }}
      exit={{ height: 0, opacity: 0 }}
      transition={{ duration: 0.2 }}
      className="overflow-hidden"
    >
      <div className="space-y-3 p-3.5 rounded-xl border border-border/60 bg-card/40">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-muted-foreground">添加 Gemini API Key 账号</span>
          <button
            type="button"
            onClick={onCancel}
            className="p-1 rounded text-muted-foreground hover:text-foreground transition-colors"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
        <ApiKeyInput value={apiKey} onChange={setApiKey} />
        <button
          type="button"
          onClick={() => {
            if (!apiKey.trim()) {
              toast.error("请输入 API Key");
              return;
            }
            onSubmit(apiKey.trim());
          }}
          className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-[#4285F4] hover:bg-[#4285F4]/80 text-white text-xs font-medium transition-colors"
        >
          <Save className="w-3.5 h-3.5" />
          保存
        </button>
      </div>
    </motion.div>
  );
}

function EmptyAccountState({
  onOAuth,
  onApiKey,
  oauthLoading,
}: {
  onOAuth: () => void;
  onApiKey: () => void;
  oauthLoading: boolean;
}) {
  return (
    <div className="flex flex-col items-center text-center py-10 px-4 border border-dashed border-border/60 rounded-xl bg-card/20">
      <div className="w-12 h-12 rounded-2xl border border-border flex items-center justify-center mb-4 bg-[#4285F4]/8 shadow-sm">
        <ShieldPlus className="w-6 h-6 text-[#4285F4]" />
      </div>
      <p className="text-[13px] font-semibold text-foreground mb-1.5">尚未添加任何账号</p>
      <p className="text-xs text-muted-foreground max-w-[260px] mb-6 leading-relaxed">
        通过 OAuth 登录或添加 API Key，开始配置您的 Gemini 模型环境
      </p>
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={onOAuth}
          disabled={oauthLoading}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-medium transition-all shadow-sm",
            oauthLoading
              ? "bg-[#4285F4]/20 text-[#4285F4] animate-pulse"
              : "bg-[#4285F4] hover:bg-[#4285F4]/90 text-white",
          )}
        >
          {oauthLoading ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <LogIn className="w-3.5 h-3.5" />}
          {oauthLoading ? "等待授权..." : "OAuth 登录"}
        </button>
        <button
          type="button"
          onClick={onApiKey}
          className="flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-medium bg-secondary text-secondary-foreground hover:bg-secondary/80 transition-all shadow-sm"
        >
          <Key className="w-3.5 h-3.5" />
          API Key
        </button>
      </div>
    </div>
  );
}

export function GeminiAccountSection({
  onAccountSwitched,
}: {
  onAccountSwitched?: () => void;
}) {
  const { providers, addProvider, updateProvider, deleteProvider, switchTo, currentId } = useModelProviders("gemini");
  
  // We consider "Accounts" to be entries explicitly added via OAuth or the inline API Key form.
  const accounts = Object.values(providers).filter(
    (p) => p.id.startsWith("gemini_oauth_") || p.id.startsWith("gemini_apikey_")
  );
  
  const [showAddApiKey, setShowAddApiKey] = useState(false);
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const handleToggle = useCallback((id: string) => {
    setExpandedId((prev) => (prev === id ? null : id));
  }, []);
  
  // Quota states
  const [quotas, setQuotas] = useState<Record<string, { 
    percentage: number; 
    resetTime: string; 
    planName?: string;
    models?: { name: string; displayName?: string; percentage: number; resetTime: string }[];
    availableCredits?: string;
    isForbidden?: boolean;
    errorMessage?: string;
  }>>({});
  const [refreshing, setRefreshing] = useState<Set<string>>(new Set());

  const refreshQuota = useCallback(async (providerId: string) => {
    setRefreshing((prev) => new Set(prev).add(providerId));
    toast("正在获取配额信息...");
    try {
      const quota = await invoke<any>("refresh_gemini_quota", {
        appId: "gemini",
        providerId,
      });
      setQuotas((prev) => ({ ...prev, [providerId]: quota }));
      toast.success("配额刷新成功");
    } catch (e: any) {
      toast.error(`刷新失败: ${e}`);
    } finally {
      setRefreshing((prev) => {
        const next = new Set(prev);
        next.delete(providerId);
        return next;
      });
    }
  }, []);

  const refreshAllQuotas = useCallback(async () => {
    if (accounts.length === 0) return;
    
    const allIds = new Set(accounts.map((a) => a.id));
    setRefreshing(allIds);
    toast("正在同步刷新配额...");

    try {
      const promises = accounts.map(account => 
        invoke<any>("refresh_gemini_quota", {
          appId: "gemini",
          providerId: account.id,
        }).then(quota => ({ id: account.id, quota })).catch(e => ({ id: account.id, error: e }))
      );

      const results = await Promise.all(promises);
      const newQuotas = { ...quotas };
      let successCount = 0;
      results.forEach(res => {
         if ("quota" in res && res.quota) {
           newQuotas[res.id] = res.quota;
           successCount++;
         }
      });
      setQuotas(newQuotas);
      if (successCount > 0 && successCount === results.length) {
        toast.success("所有账号配额已刷新");
      } else if (successCount > 0) {
        toast.warning(`刷新完成: ${successCount}/${results.length} 成功`);
      } else {
        toast.error("所有账号刷新均失败");
      }
    } catch (e) {
      console.error(e);
    } finally {
      setRefreshing(new Set());
    }
  }, [accounts, quotas]);

  const geminiState = useGeminiOAuth({
    onAccountAdded: (provider) => {
      const existing = Object.values(providers).find(
        p => p.id.startsWith("gemini_oauth_") && p.name === provider.name
      );
      if (existing) {
        const merged = { ...provider, id: existing.id };
        updateProvider(merged);
        switchTo(merged.id);
      } else {
        addProvider(provider);
        switchTo(provider.id);
      }
      if (onAccountSwitched) onAccountSwitched();
    }
  });

  const hasAccounts = accounts.length > 0;

  return (
    <div className="space-y-3 mb-6">
      {/* Section header */}
      <div className="flex items-center justify-between px-1">
        <h3 className="text-[11px] font-semibold text-muted-foreground/70 uppercase tracking-widest">Google Gemini 账号</h3>
        {hasAccounts && (
          <button
            type="button"
            onClick={refreshAllQuotas}
            disabled={refreshing.size > 0}
            className="flex items-center gap-1.5 text-[10px] text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
          >
            <RefreshCw className={cn("w-3 h-3", refreshing.size > 0 && "animate-spin")} />
            刷新全部
          </button>
        )}
      </div>

      {hasAccounts ? (
        <div className="space-y-4">
          <div className="space-y-2">
            {accounts.map((account) => (
              <GeminiAccountRow
                key={account.id}
                account={account}
                isCurrent={account.id === currentId}
                expanded={expandedId === account.id}
                quotaRefreshing={refreshing.has(account.id)}
                quota={(quotas[account.id] || (account.meta as any)?.gemini_quota) as any}
                onSwitch={() => {
                  switchTo(account.id);
                  if (onAccountSwitched) onAccountSwitched();
                }}
                onToggle={() => handleToggle(account.id)}
                onDelete={() => deleteProvider(account.id)}
                onRefreshQuota={() => refreshQuota(account.id)}
              />
            ))}
          </div>

          <div className="flex items-center gap-2 pt-1">
            <button
              type="button"
              onClick={geminiState.startOAuth}
              disabled={geminiState.oauthLoading}
              className={cn(
                "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[11px] font-medium transition-all group",
                geminiState.oauthLoading
                  ? "bg-primary/10 text-primary animate-pulse border border-transparent"
                  : "bg-transparent text-muted-foreground hover:text-[#4285F4] hover:bg-[#4285F4]/10 border border-border hover:border-[#4285F4]/30",
              )}
            >
              {geminiState.oauthLoading ? (
                <>
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  等待授权...
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      geminiState.cancelOAuth();
                    }}
                    className="ml-1 p-0.5 rounded hover:bg-black/10 transition-colors"
                  >
                    <X className="w-3 h-3" />
                  </button>
                </>
              ) : (
                <>
                  <LogIn className="w-3.5 h-3.5 opacity-70 group-hover:opacity-100" />
                  添加 OAuth
                </>
              )}
            </button>

            <button
              type="button"
              onClick={() => setShowAddApiKey(!showAddApiKey)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[11px] font-medium bg-transparent text-muted-foreground hover:text-foreground hover:bg-secondary border border-border hover:border-border/80 transition-all group"
            >
              <Key className="w-3.5 h-3.5 opacity-70 group-hover:opacity-100" />
              添加 API Key
            </button>
          </div>
        </div>
      ) : (
        <EmptyAccountState
          onOAuth={geminiState.startOAuth}
          onApiKey={() => setShowAddApiKey(!showAddApiKey)}
          oauthLoading={geminiState.oauthLoading}
        />
      )}

      {/* Inline API Key form */}
      <AnimatePresence>
        {showAddApiKey && (
          <div className="pt-2">
            <AddApiKeyForm
              onSubmit={(key) => {
                const provider: ProviderEntry = {
                  id: `gemini_apikey_${Date.now()}`,
                  name: `Gemini API`,
                  category: "official",
                  settingsConfig: {
                    env: {
                      GEMINI_API_KEY: key,
                    },
                  },
                };
                addProvider(provider);
                switchTo(provider.id);
                setShowAddApiKey(false);
                if (onAccountSwitched) onAccountSwitched();
              }}
              onCancel={() => setShowAddApiKey(false)}
            />
          </div>
        )}
      </AnimatePresence>
    </div>
  );
}

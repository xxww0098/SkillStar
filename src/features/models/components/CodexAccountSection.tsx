/**
 * Codex OAuth multi-account section — rendered above provider cards
 * when the active app is "codex" in ModelsPanel.
 *
 * Now includes full account management: list, switch, delete, add OAuth, add API Key.
 */
import { AnimatePresence, motion } from "framer-motion";
import { Key, Loader2, LogIn, RefreshCw, Save, ShieldPlus, X } from "lucide-react";
import { useCallback, useState } from "react";
import { toast } from "sonner";

import { cn } from "../../../lib/utils";
import { useCodexAccounts } from "../hooks/useCodexAccounts";
import { AccountRow } from "./shared/AccountRow";
import { ApiKeyInput } from "./shared/ApiKeyInput";
import { EndpointInput } from "./shared/EndpointInput";

// ── Inline API Key Form ─────────────────────────────────────────────

function AddApiKeyForm({
  onSubmit,
  onCancel,
}: {
  onSubmit: (key: string, baseUrl?: string) => void;
  onCancel: () => void;
}) {
  const [apiKey, setApiKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");

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
          <span className="text-xs font-medium text-muted-foreground">添加 API Key 账号</span>
          <button
            type="button"
            onClick={onCancel}
            className="p-1 rounded text-muted-foreground hover:text-foreground transition-colors"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
        <ApiKeyInput value={apiKey} onChange={setApiKey} />
        <EndpointInput value={baseUrl} onChange={setBaseUrl} />
        <button
          type="button"
          onClick={() => {
            if (!apiKey.trim()) {
              toast.error("请输入 API Key");
              return;
            }
            onSubmit(apiKey.trim(), baseUrl.trim() || undefined);
          }}
          className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-[#00A67E] hover:bg-[#00A67E]/80 text-white text-xs font-medium transition-colors"
        >
          <Save className="w-3.5 h-3.5" />
          保存
        </button>
      </div>
    </motion.div>
  );
}

// ── Empty State ─────────────────────────────────────────────────────

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
      <div className="w-12 h-12 rounded-2xl border border-border flex items-center justify-center mb-4 bg-[#00A67E]/8 shadow-sm">
        <ShieldPlus className="w-6 h-6 text-[#00A67E]" />
      </div>
      <p className="text-[13px] font-semibold text-foreground mb-1.5">尚未添加任何账号</p>
      <p className="text-xs text-muted-foreground max-w-[260px] mb-6 leading-relaxed">
        通过 OAuth 登录或添加 API Key，开始配置您的模型环境
      </p>
      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={onOAuth}
          disabled={oauthLoading}
          className={cn(
            "flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-medium transition-all shadow-sm",
            oauthLoading
              ? "bg-[#00A67E]/20 text-[#00A67E] animate-pulse"
              : "bg-[#00A67E] hover:bg-[#00A67E]/90 text-white",
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

// ── Main Section ────────────────────────────────────────────────────

export function CodexAccountSection({
  isOAuthActive = false,
  onAccountSwitched,
}: {
  /** True when no provider card is current — OAuth is the active auth method */
  isOAuthActive?: boolean;
  /** Called after switching OAuth account so parent reloads provider state */
  onAccountSwitched?: () => void;
}) {
  const state = useCodexAccounts();
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [showAddApiKey, setShowAddApiKey] = useState(false);

  const handleToggle = useCallback((id: string) => {
    setExpandedId((prev) => (prev === id ? null : id));
  }, []);

  if (state.loading) {
    return (
      <div className="flex items-center justify-center py-6">
        <Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
      </div>
    );
  }

  const hasAccounts = state.accounts.length > 0;
  const oauthCount = state.accounts.filter((a) => a.authMode === "oauth").length;

  return (
    <div className="space-y-3">
      {/* Section header */}
      <div className="flex items-center justify-between px-1">
        <h3 className="text-[11px] font-semibold text-muted-foreground/70 uppercase tracking-widest">
          OPENAI CODEX 账号
        </h3>
        {oauthCount > 0 && (
          <button
            type="button"
            onClick={state.refreshAllQuotas}
            disabled={state.quotaRefreshing.size > 0}
            className="flex items-center gap-1.5 text-[10px] text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
          >
            <RefreshCw className={cn("w-3 h-3", state.quotaRefreshing.size > 0 && "animate-spin")} />
            刷新全部
          </button>
        )}
      </div>

      {/* Account list or empty state */}
      {hasAccounts ? (
        <div className="space-y-4">
          <div className="space-y-2">
            {state.accounts.map((account) => (
              <AccountRow
                key={account.id}
                account={account}
                isCurrent={isOAuthActive && account.id === state.currentId}
                expanded={expandedId === account.id}
                quotaRefreshing={state.quotaRefreshing.has(account.id)}
                onSwitch={async () => {
                  await state.switchAccount(account.id);
                  onAccountSwitched?.();
                }}
                onToggle={() => handleToggle(account.id)}
                onDelete={() => state.deleteAccount(account.id)}
                onRefreshQuota={() => state.refreshQuota(account.id)}
              />
            ))}
          </div>

          {/* Add account buttons (when accounts already exist) */}
          <div className="flex items-center gap-2 pt-1">
            {state.oauthLoading ? (
              <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[11px] font-medium transition-all bg-primary/10 text-primary animate-pulse border border-transparent">
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                等待授权...
                <button
                  type="button"
                  onClick={state.cancelOAuth}
                  aria-label="取消 OAuth 授权"
                  className="ml-1 p-0.5 rounded hover:bg-black/10 transition-colors"
                >
                  <X className="w-3 h-3" />
                </button>
              </div>
            ) : (
              <button
                type="button"
                onClick={state.startOAuth}
                className={cn(
                  "flex items-center gap-1.5 px-3 py-1.5 rounded-md text-[11px] font-medium transition-all group",
                  "bg-transparent text-muted-foreground hover:text-[#00A67E] hover:bg-[#00A67E]/10 border border-border hover:border-[#00A67E]/30",
                )}
              >
                <LogIn className="w-3.5 h-3.5 opacity-70 group-hover:opacity-100" />
                添加 OAuth
              </button>
            )}
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
          onOAuth={state.startOAuth}
          onApiKey={() => setShowAddApiKey(!showAddApiKey)}
          oauthLoading={state.oauthLoading}
        />
      )}

      {/* Inline API Key form */}
      <AnimatePresence>
        {showAddApiKey && (
          <div className="pt-2">
            <AddApiKeyForm
              onSubmit={(key, url) => {
                state.addApiKeyAccount(key, url);
                setShowAddApiKey(false);
              }}
              onCancel={() => setShowAddApiKey(false)}
            />
          </div>
        )}
      </AnimatePresence>
    </div>
  );
}

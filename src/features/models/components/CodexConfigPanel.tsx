import { AnimatePresence, motion } from "framer-motion";
import {
  AlertCircle,
  Check,
  ChevronDown,
  ChevronUp,
  Key,
  Loader2,
  LogIn,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
  User,
  X,
} from "lucide-react";
import { useCallback, useState } from "react";
import { toast } from "sonner";

import { cn } from "../../../lib/utils";
import { useCodexAccounts, type CodexAccount } from "../hooks/useCodexAccounts";
import { useCodexConfig } from "../hooks/useCodexConfig";
import { codexPresets } from "../presets/codexPresets";
import { CodexQuotaBar } from "./CodexQuotaBar";
import { ApiKeyInput } from "./shared/ApiKeyInput";
import { EndpointInput } from "./shared/EndpointInput";
import { ModelInput } from "./shared/ModelInput";

// ── Plan badge helpers ──────────────────────────────────────────────

function getPlanColor(planType?: string): string {
  if (!planType) return "#666";
  const p = planType.toLowerCase();
  if (p.includes("team")) return "#7C3AED";
  if (p.includes("pro") || p.includes("plus")) return "#F59E0B";
  if (p.includes("enterprise")) return "#3B82F6";
  return "#6B7280";
}

function getPlanLabel(planType?: string): string {
  if (!planType) return "FREE";
  const p = planType.toLowerCase();
  if (p.includes("team")) return "TEAM";
  if (p.includes("pro")) return "PRO";
  if (p.includes("plus")) return "PLUS";
  if (p.includes("enterprise")) return "ENT";
  if (p === "api_key") return "KEY";
  return planType.toUpperCase().slice(0, 6);
}

// ── Account Row Component ───────────────────────────────────────────

function AccountRow({
  account,
  isCurrent,
  expanded,
  quotaRefreshing,
  onSwitch,
  onToggle,
  onDelete,
  onRefreshQuota,
}: {
  account: CodexAccount;
  isCurrent: boolean;
  expanded: boolean;
  quotaRefreshing: boolean;
  onSwitch: () => void;
  onToggle: () => void;
  onDelete: () => void;
  onRefreshQuota: () => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState(false);
  const planColor = getPlanColor(account.planType);
  const planLabel = getPlanLabel(account.planType);
  const isOAuth = account.authMode === "oauth";

  return (
    <div
      className={cn(
        "rounded-xl border transition-all duration-200",
        isCurrent ? "border-emerald-500/40 bg-emerald-500/5" : "border-border/60 bg-card/40 hover:bg-card/60",
      )}
    >
      {/* Header row */}
      {/* biome-ignore lint/a11y/useSemanticElements: Interactive row with complex inner elements */}
      <div
        role="button"
        tabIndex={0}
        onClick={onToggle}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggle();
          }
        }}
        className="flex items-center gap-3 px-3.5 py-2.5 cursor-pointer select-none"
      >
        {/* Icon */}
        <div
          className={cn(
            "w-7 h-7 rounded-lg flex items-center justify-center shrink-0",
            isOAuth ? "bg-emerald-500/15" : "bg-amber-500/15",
          )}
        >
          {isOAuth ? <User className="w-3.5 h-3.5 text-emerald-400" /> : <Key className="w-3.5 h-3.5 text-amber-400" />}
        </div>

        {/* Info */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            <span className="text-xs font-semibold text-foreground truncate">
              {isOAuth ? account.email : `API Key: ${(account.openaiApiKey || "").slice(0, 12)}...`}
            </span>
            <span
              className="px-1.5 py-0 rounded text-[9px] font-bold text-white leading-relaxed"
              style={{ backgroundColor: planColor }}
            >
              {planLabel}
            </span>
            {isCurrent && (
              <span className="flex items-center gap-0.5 text-emerald-400">
                <Check className="w-3 h-3" />
                <span className="text-[10px] font-medium">当前</span>
              </span>
            )}
          </div>

          {/* Compact quota bars */}
          {isOAuth && account.quota && !expanded && (
            <div className="mt-1.5 space-y-0.5 max-w-[280px]">
              <CodexQuotaBar label="5h" percentage={account.quota.hourlyPercentage} compact />
              <CodexQuotaBar label="7d" percentage={account.quota.weeklyPercentage} compact />
            </div>
          )}
        </div>

        {/* Actions */}
        {/* biome-ignore lint/a11y/useKeyWithClickEvents: Wrapping action buttons */}
        <div className="flex items-center gap-1 shrink-0" onClick={(e) => e.stopPropagation()}>
          {!isCurrent && (
            <button
              type="button"
              onClick={onSwitch}
              className="px-2 py-1 rounded-md text-[10px] font-medium text-emerald-600 border border-emerald-500/30 hover:bg-emerald-500/10 transition-colors"
            >
              使用
            </button>
          )}
          <ChevronDown
            className={cn(
              "w-3.5 h-3.5 text-muted-foreground/50 transition-transform duration-200",
              expanded && "rotate-180",
            )}
          />
        </div>
      </div>

      {/* Expanded details */}
      <AnimatePresence>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="px-3.5 pb-3.5 pt-1 border-t border-border/30 space-y-3">
              {/* Full quota display */}
              {isOAuth && account.quota && (
                <div className="space-y-2 p-3 rounded-lg bg-muted/20">
                  <CodexQuotaBar
                    label="5小时配额"
                    percentage={account.quota.hourlyPercentage}
                    resetTime={account.quota.hourlyResetTime}
                  />
                  <CodexQuotaBar
                    label="7天配额"
                    percentage={account.quota.weeklyPercentage}
                    resetTime={account.quota.weeklyResetTime}
                  />
                </div>
              )}

              {/* Quota error */}
              {account.quotaError && (
                <div className="flex items-start gap-2 p-2.5 rounded-lg bg-destructive/10 border border-destructive/20">
                  <AlertCircle className="w-3.5 h-3.5 text-destructive shrink-0 mt-0.5" />
                  <p className="text-[10px] text-destructive leading-relaxed">{account.quotaError.message}</p>
                </div>
              )}

              {/* No quota yet */}
              {isOAuth && !account.quota && !account.quotaError && (
                <p className="text-[10px] text-muted-foreground/60 px-1">尚未获取配额数据，点击下方刷新按钮获取</p>
              )}

              {/* Account meta */}
              <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-muted-foreground/60 px-1">
                {account.accountId && (
                  <span>
                    Account: <span className="font-mono">{account.accountId.slice(0, 12)}...</span>
                  </span>
                )}
                <span>创建于 {new Date(account.createdAt * 1000).toLocaleDateString("zh-CN")}</span>
              </div>

              {/* Action buttons */}
              <div className="flex items-center gap-2 pt-1">
                {isOAuth && (
                  <button
                    type="button"
                    onClick={onRefreshQuota}
                    disabled={quotaRefreshing}
                    className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[10px] font-medium bg-muted/30 hover:bg-muted/50 text-muted-foreground transition-colors disabled:opacity-50"
                  >
                    <RefreshCw className={cn("w-3 h-3", quotaRefreshing && "animate-spin")} />
                    刷新配额
                  </button>
                )}

                <div className="flex-1" />

                <button
                  type="button"
                  onClick={() => {
                    if (!confirmDelete) {
                      setConfirmDelete(true);
                      setTimeout(() => setConfirmDelete(false), 3000);
                      return;
                    }
                    onDelete();
                  }}
                  className={cn(
                    "flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-[10px] font-medium transition-colors",
                    confirmDelete ? "bg-destructive text-white" : "text-destructive hover:bg-destructive/10",
                  )}
                >
                  <Trash2 className="w-3 h-3" />
                  {confirmDelete ? "确认删除" : "删除"}
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

// ── Add API Key Form ────────────────────────────────────────────────

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

// ── Main Panel ──────────────────────────────────────────────────────

export function CodexConfigPanel() {
  const config = useCodexConfig();
  const acctState = useCodexAccounts();
  const [showToml, setShowToml] = useState(false);
  const [expandedAccountId, setExpandedAccountId] = useState<string | null>(null);
  const [showAddApiKey, setShowAddApiKey] = useState(false);

  if (config.loading || acctState.loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  const handleSave = () => {
    config.save(config.configText, config.authJson);
  };

  const handleApplyPreset = (preset: (typeof codexPresets)[0]) => {
    config.applyPreset(preset.config);
  };

  // Existing TOML parsing for config section
  const envKeyMatch = config.configText.match(/env_key\s*=\s*"([^"]+)"/);
  const activeEnvKey = envKeyMatch ? envKeyMatch[1] : "OPENAI_API_KEY";
  const apiKey = config.authJson[activeEnvKey] || "";

  const baseUrlMatch = config.configText.match(/base_url\s*=\s*"([^"]+)"/);
  const baseUrl = baseUrlMatch ? baseUrlMatch[1] : "";
  const modelMatch = config.configText.match(/^model\s*=\s*"([^"]+)"/m);
  const currentModel = modelMatch ? modelMatch[1] : "";

  const handleBaseUrlChange = (newUrl: string) => {
    let newText = config.configText;
    if (/base_url\s*=\s*"[^"]*"/.test(newText)) {
      newText = newText.replace(/base_url\s*=\s*"[^"]*"/, `base_url = "${newUrl}"`);
    } else {
      newText += `\nbase_url = "${newUrl}"`;
    }
    config.setConfigText(newText);
  };

  const activePreset = codexPresets.find(
    (p) =>
      config.configText.includes(`model_provider = "${p.name.toLowerCase()}"`) ||
      config.configText.includes(p.config.split("\n")[0]),
  );

  const handleToggleAccount = useCallback((accountId: string) => {
    setExpandedAccountId((prev) => (prev === accountId ? null : accountId));
  }, []);

  return (
    <div className="space-y-5">
      {/* ── Account Management Section ──────────────────────── */}
      <div className="rounded-xl border border-border bg-card p-4 space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">OpenAI 账号</h3>
          {acctState.accounts.filter((a) => a.authMode === "oauth").length > 0 && (
            <button
              type="button"
              onClick={acctState.refreshAllQuotas}
              disabled={acctState.quotaRefreshing.size > 0}
              className="flex items-center gap-1 text-[10px] text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
            >
              <RefreshCw className={cn("w-3 h-3", acctState.quotaRefreshing.size > 0 && "animate-spin")} />
              刷新全部
            </button>
          )}
        </div>

        {/* Account list */}
        {acctState.accounts.length > 0 && (
          <div className="space-y-2">
            {acctState.accounts.map((account) => (
              <AccountRow
                key={account.id}
                account={account}
                isCurrent={account.id === acctState.currentId}
                expanded={expandedAccountId === account.id}
                quotaRefreshing={acctState.quotaRefreshing.has(account.id)}
                onSwitch={() => acctState.switchAccount(account.id)}
                onToggle={() => handleToggleAccount(account.id)}
                onDelete={() => acctState.deleteAccount(account.id)}
                onRefreshQuota={() => acctState.refreshQuota(account.id)}
              />
            ))}
          </div>
        )}

        {/* Add account buttons */}
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={acctState.startOAuth}
            disabled={acctState.oauthLoading}
            className={cn(
              "flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium transition-all",
              acctState.oauthLoading
                ? "bg-primary/20 text-primary animate-pulse"
                : "bg-[#00A67E]/10 text-[#00A67E] border border-[#00A67E]/20 hover:bg-[#00A67E]/20",
            )}
          >
            {acctState.oauthLoading ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                等待授权...
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    acctState.cancelOAuth();
                  }}
                  className="ml-1 p-0.5 rounded hover:bg-white/10 transition-colors"
                >
                  <X className="w-3 h-3" />
                </button>
              </>
            ) : (
              <>
                <LogIn className="w-3.5 h-3.5" />
                OAuth 登录
              </>
            )}
          </button>

          <button
            type="button"
            onClick={() => setShowAddApiKey(!showAddApiKey)}
            className="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium bg-muted/30 text-muted-foreground hover:bg-muted/50 border border-border/50 transition-colors"
          >
            <Key className="w-3.5 h-3.5" />
            API Key
          </button>
        </div>

        {/* Add API Key form */}
        <AnimatePresence>
          {showAddApiKey && (
            <AddApiKeyForm
              onSubmit={(key, url) => {
                acctState.addApiKeyAccount(key, url);
                setShowAddApiKey(false);
              }}
              onCancel={() => setShowAddApiKey(false)}
            />
          )}
        </AnimatePresence>
      </div>

      {/* ── ChatGPT OAuth Session Banner (from CLI) ──────── */}
      {config.authStatus.hasChatgptSession && acctState.accounts.length === 0 && (
        <div className="flex items-center gap-2.5 px-4 py-3 rounded-xl border border-emerald-500/30 bg-emerald-500/8">
          <ShieldCheck className="w-4 h-4 text-emerald-400 shrink-0" />
          <div className="flex-1 min-w-0">
            <p className="text-xs font-medium text-emerald-300">已通过 ChatGPT 登录</p>
            <p className="text-[10px] text-emerald-400/60 mt-0.5">
              OAuth session 由 Codex CLI 管理，SkillStar 不会修改登录状态
            </p>
          </div>
        </div>
      )}

      {/* ── Provider Presets ─────────────────────────────── */}
      <div className="rounded-xl border border-border bg-card p-4">
        <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-3">供应商预设</h3>
        <div className="flex flex-wrap gap-2">
          {codexPresets.map((preset) => (
            <button
              key={preset.name}
              type="button"
              onClick={() => handleApplyPreset(preset)}
              className={cn(
                "group relative px-3 py-1.5 rounded-lg text-xs font-medium border transition-all duration-200",
                "bg-card border-border text-muted-foreground hover:bg-muted/50 hover:text-foreground",
              )}
            >
              <span className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: preset.iconColor || "#888" }} />
                {preset.name}
              </span>
            </button>
          ))}
        </div>
      </div>

      {/* ── Auth & Config Section ────────────────────────── */}
      <div className="rounded-xl border border-border bg-card p-4 space-y-4">
        <ApiKeyInput
          value={apiKey}
          onChange={(v) => config.updateAuthField(activeEnvKey, v)}
          apiKeyUrl={activePreset?.apiKeyUrl}
        />
        <EndpointInput value={baseUrl} onChange={handleBaseUrlChange} />
        <ModelInput
          label="当前模型"
          value={currentModel}
          onChange={(v) => {
            if (/^model\s*=\s*"[^"]*"/m.test(config.configText)) {
              config.setConfigText(config.configText.replace(/^model\s*=\s*"[^"]*"/m, `model = "${v}"`));
            } else {
              config.setConfigText(`model = "${v}"\n${config.configText}`);
            }
          }}
          placeholder="gpt-5.4"
        />
        <p className="text-[10px] text-muted-foreground/60 px-1">
          API key 保存到 <span className="font-mono bg-muted px-1 py-0.5 rounded">auth.json</span> 的{" "}
          <span className="font-mono bg-muted px-1 py-0.5 rounded">{activeEnvKey}</span>{" "}
          字段（合并写入，不影响已有登录状态）
        </p>
      </div>

      {/* ── TOML Config (collapsible) ────────────────────── */}
      <div className="rounded-xl border border-border bg-card overflow-hidden">
        <button
          type="button"
          onClick={() => setShowToml(!showToml)}
          className="w-full flex items-center justify-between px-4 py-3 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
        >
          <span>config.toml 配置</span>
          {showToml ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
        </button>
        {showToml && (
          <div className="px-4 pb-4 border-t border-border pt-4">
            <textarea
              value={config.configText}
              onChange={(e) => config.setConfigText(e.target.value)}
              rows={12}
              className="w-full rounded-lg bg-background/60 border border-border text-sm text-foreground font-mono p-3 resize-y focus:outline-none focus:ring-1 focus:ring-primary/50 focus:border-primary/40 placeholder:text-muted-foreground/40"
              placeholder={`model_provider = "openai"\nmodel = "gpt-5.4"\nmodel_reasoning_effort = "high"`}
              spellCheck={false}
            />
          </div>
        )}
      </div>

      {/* ── Config path + Save ───────────────────────────── */}
      <div className="flex items-center justify-between pt-1">
        <div className="flex flex-col">
          <p className="text-xs text-muted-foreground/60 font-mono truncate">~/.codex/config.toml</p>
          <p className="text-xs text-muted-foreground/60 font-mono truncate">~/.codex/auth.json</p>
        </div>
        <button
          type="button"
          onClick={handleSave}
          disabled={config.saving}
          className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[#00A67E] hover:bg-[#00A67E]/80 text-white text-sm font-medium transition-colors disabled:opacity-50"
        >
          {config.saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
          保存
        </button>
      </div>
    </div>
  );
}

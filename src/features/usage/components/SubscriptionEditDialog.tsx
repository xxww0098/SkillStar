import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { ChevronDown, Copy, ExternalLink, Eye, EyeOff, FolderInput, Loader2, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { openExternalUrl } from "@/lib/externalOpen";
import { cn, navigateToSettingsSection } from "@/lib/utils";
import { usageApi } from "../api";
import {
  LOCAL_IMPORT_CATALOG_IDS,
  type AuthMode,
  type BillingCycle,
  type CatalogEntry,
  selectableAuthModes,
  type Subscription,
} from "../types";
import { ProviderCatalogHero } from "./ProviderCatalogHero";
import { AdvancedBillingSection } from "./subscriptionEdit/AdvancedBillingSection";
import { Field, parseDateInput, toDateInput } from "./subscriptionEdit/fields";

interface SubscriptionEditDialogProps {
  open: boolean;
  catalog: CatalogEntry[];
  editing: Subscription | null;
  preselectCatalogId?: string | null;
  onClose: () => void;
  onCreated: (sub: Subscription) => void;
  onUpdated: (sub: Subscription) => void;
  onDeleted: () => void;
}

export function SubscriptionEditDialog({
  open,
  catalog,
  editing,
  preselectCatalogId,
  onClose,
  onCreated,
  onUpdated,
  onDeleted,
}: SubscriptionEditDialogProps) {
  const { t } = useTranslation();
  const reduceMotion = useReducedMotion();
  const isCreate = !editing;
  const title = isCreate ? t("usage.createTitle") : t("usage.editTitle");

  const [catalogId, setCatalogId] = useState("");
  const [authMode, setAuthMode] = useState<AuthMode>("manual");
  const [planTier, setPlanTier] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [price, setPrice] = useState("");
  const [currency, setCurrency] = useState("CNY");
  const [billingCycle, setBillingCycle] = useState<BillingCycle>("monthly");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
  const [autoRenew, setAutoRenew] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [cookieHeader, setCookieHeader] = useState("");
  const [region, setRegion] = useState("cn");
  const [totalTokens, setTotalTokens] = useState("");
  const [usedTokens, setUsedTokens] = useState("");
  const [periodLabel, setPeriodLabel] = useState(() => t("usage.defaultPeriod"));
  const [note, setNote] = useState("");
  const [fingerprintId, setFingerprintId] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [oauthPendingId, setOauthPendingId] = useState<string | null>(null);
  const [oauthStatus, setOauthStatus] = useState<string | null>(null);
  const [oauthUserCode, setOauthUserCode] = useState<string | null>(null);
  const [deleteConfirming, setDeleteConfirming] = useState(false);

  // 折叠与自动化控制
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [scanningLocal, setScanningLocal] = useState(false);

  useEffect(() => {
    if (!open) return;
    setShowAdvanced(false);
    setScanningLocal(false);
    if (editing) {
      const editingEntry = catalog.find((c) => c.id === editing.catalog_id);
      const editingModes = selectableAuthModes(editingEntry?.auth_modes ?? []);
      let resolvedAuthMode =
        editing.catalog_id === "opencode" && editing.auth_mode === "o-auth" ? "cookie" : editing.auth_mode;
      if (!editingModes.includes(resolvedAuthMode)) {
        resolvedAuthMode = editingModes[0] ?? "manual";
      }
      setCatalogId(editing.catalog_id);
      setAuthMode(resolvedAuthMode);
      setPlanTier(editing.plan_tier ?? "");
      setDisplayName(editing.display_name);
      setPrice(editing.monthly_price?.toString() ?? "");
      setCurrency(editing.currency || "CNY");
      setBillingCycle(editing.billing_cycle);
      setStartDate(toDateInput(editing.start_date));
      setEndDate(toDateInput(editing.renew_date));
      setAutoRenew(editing.auto_renew);
      setApiKey("");
      setRegion(editing.oauth_region ?? "cn");
      setCookieHeader("");
      setTotalTokens(editing.manual_quota?.total_tokens?.toString() ?? "");
      setUsedTokens(editing.manual_quota?.used_tokens?.toString() ?? "");
      setPeriodLabel(editing.manual_quota?.period_label ?? t("usage.defaultPeriod"));
      setNote(editing.note ?? "");
      setFingerprintId(editing.fingerprint_id ?? null);
    } else {
      const preselectedEntry = preselectCatalogId ? catalog.find((c) => c.id === preselectCatalogId) : null;
      const today = toDateInput(Math.floor(Date.now() / 1000));
      setCatalogId(preselectCatalogId ?? "");
      setAuthMode(selectableAuthModes(preselectedEntry?.auth_modes ?? [])[0] ?? "manual");
      setPlanTier("");
      setDisplayName("");
      setPrice("");
      setCurrency(preselectedEntry?.default_currency ?? "CNY");
      setBillingCycle("monthly");
      setStartDate(today);
      setEndDate("");
      setAutoRenew(false);
      setApiKey("");
      setRegion(preselectedEntry?.regions[0] ?? "cn");
      setCookieHeader("");
      setTotalTokens("");
      setUsedTokens("");
      setPeriodLabel(t("usage.defaultPeriod"));
      setNote("");
      setFingerprintId(null);
    }
    setShowKey(false);
    setOauthPendingId(null);
    setOauthStatus(null);
    setOauthUserCode(null);
  }, [open, editing, preselectCatalogId, t]);

  useEffect(() => {
    if (!open || editing) return;
    if (!catalogId) {
      setAuthMode("manual");
      return;
    }
    const entry = catalog.find((c) => c.id === catalogId);
    if (entry) {
      const modes = selectableAuthModes(entry.auth_modes);
      if (!modes.includes(authMode)) {
        setAuthMode(modes[0] ?? "manual");
      } else if (authMode === "manual" && modes.length > 1) {
        setAuthMode(modes.find((m) => m !== "manual") ?? "manual");
      }
      setCurrency(entry.default_currency);
      if (entry.regions.length > 0 && !entry.regions.includes(region)) {
        setRegion(entry.regions[0]);
      }
    }
    if (catalogId === "opencode" && !planTier) {
      setPlanTier("Go");
    }
  }, [catalogId, catalog, editing, authMode, planTier, region, open]);

  const selectedEntry = useMemo(() => catalog.find((c) => c.id === catalogId) ?? null, [catalog, catalogId]);
  const visibleAuthModes = useMemo(
    () => (selectedEntry ? selectableAuthModes(selectedEntry.auth_modes) : []),
    [selectedEntry],
  );
  const supportsLocalImport = LOCAL_IMPORT_CATALOG_IDS.includes(catalogId as (typeof LOCAL_IMPORT_CATALOG_IDS)[number]);
  const showAutoImportBanner = isCreate && (!catalogId || supportsLocalImport);
  const canImportLocal = isCreate && authMode === "o-auth" && supportsLocalImport;

  const priceLabel = (() => {
    switch (billingCycle) {
      case "annual":
        return t("usage.priceAnnual");
      case "one-time":
        return t("usage.priceOneTime");
      default:
        return t("usage.priceMonthly");
    }
  })();

  const endDateLabel = autoRenew ? t("usage.fieldNextRenew") : t("usage.fieldEndDate");

  const labelAuthMode = (mode: AuthMode) => {
    switch (mode) {
      case "api-key":
        return "API Key";
      case "o-auth":
        return "OAuth";
      case "cookie":
        return "Cookie";
      case "manual":
        return t("usage.authModeManual");
    }
  };

  const billingCycleOptions: BillingCycle[] = ["monthly", "annual", "one-time"];

  const labelBillingCycle = (cycle: BillingCycle) => {
    switch (cycle) {
      case "annual":
        return t("usage.cycleAnnualShort");
      case "one-time":
        return t("usage.cycleOneTimeShort");
      default:
        return t("usage.cycleMonthlyShort");
    }
  };

  const billingCycleHint =
    billingCycle === "annual"
      ? t("usage.billingHintAnnual")
      : billingCycle === "one-time"
        ? t("usage.billingHintOneTime")
        : t("usage.billingHintMonthly");

  const handleAutoScanAll = async () => {
    setScanningLocal(true);
    let successCount = 0;

    // 并发扫描四大支持本地导入的服务
    const importPromises = LOCAL_IMPORT_CATALOG_IDS.map(async (id) => {
      try {
        const sub = await usageApi.importSubscriptionFromLocal(id);
        return { id, sub, success: true };
      } catch (err) {
        return { id, success: false, error: err };
      }
    });

    const results = await Promise.all(importPromises);

    results.forEach((res) => {
      if (res.success && res.sub) {
        successCount++;
        onCreated(res.sub);
      }
    });

    setScanningLocal(false);

    if (successCount > 0) {
      toast.success(t("usage.importFromLocalSuccess") + ` (${successCount})`);
      onClose();
    } else {
      toast.error("未在本地环境中探测到任何可用的 Codex / Antigravity / Qoder 凭证");
    }
  };

  if (!open) return null;

  const buildPayload = () => {
    const manualQuota =
      authMode === "manual"
        ? {
            total_tokens: totalTokens ? Number(totalTokens) : null,
            used_tokens: usedTokens ? Number(usedTokens) : null,
            period_label: periodLabel || null,
          }
        : undefined;

    const billing =
      authMode === "api-key"
        ? {}
        : {
            monthly_price: price ? Number(price) : undefined,
            currency: currency || undefined,
            billing_cycle: billingCycle,
            start_date: parseDateInput(startDate),
            renew_date: parseDateInput(endDate),
            auto_renew: autoRenew,
          };

    return {
      display_name: displayName || undefined,
      plan_tier: planTier || undefined,
      ...billing,
      manual_quota: manualQuota,
      note: note || undefined,
    };
  };

  const submit = async () => {
    if (!catalogId) {
      toast.error(t("usage.toastSelectProvider"));
      return;
    }
    setSubmitting(true);
    try {
      const payload = buildPayload();
      if (isCreate) {
        const created = await usageApi.createSubscription({
          catalog_id: catalogId,
          auth_mode: authMode,
          api_key: authMode === "api-key" && apiKey ? apiKey : undefined,
          oauth_region: authMode === "o-auth" ? region : undefined,
          cookie_header: authMode === "cookie" && cookieHeader ? cookieHeader : undefined,
          fingerprint_id: fingerprintId ?? undefined,
          ...payload,
        });
        onCreated(created);
        toast.success(t("usage.toastAdded"));
      } else {
        const updated = await usageApi.updateSubscription(editing.id, {
          ...payload,
          api_key: apiKey || undefined,
          cookie_header: cookieHeader || undefined,
          fingerprint_id: fingerprintId ?? undefined,
          // When the user explicitly switched back to "默认" on an
          // editing row that previously had a binding, we need to
          // tell the backend to drop it.
          clearFingerprint: !!editing.fingerprint_id && fingerprintId === null,
        });
        onUpdated(updated);
        toast.success(t("usage.toastUpdated"));
      }
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(msg);
    } finally {
      setSubmitting(false);
    }
  };

  const startOAuthFlow = async () => {
    if (!catalogId) {
      toast.error(t("usage.toastSelectProvider"));
      return;
    }
    setSubmitting(true);
    setOauthStatus(t("usage.oauthGenerating"));
    try {
      const start = await usageApi.startOAuthLogin(
        catalogId,
        selectedEntry?.regions.length ? region : undefined,
        isCreate ? undefined : editing.id,
      );
      setOauthPendingId(start.pending_id);
      setOauthUserCode(start.user_code ?? null);
      setOauthStatus(t("usage.oauthWaiting"));
      await openExternalUrl(start.auth_url);

      const created = await usageApi.awaitOAuthCompletion(start.pending_id);
      const meta = buildPayload();
      if (meta.monthly_price || meta.renew_date || meta.start_date || meta.plan_tier || meta.note) {
        try {
          const updated = await usageApi.updateSubscription(created.id, meta);
          if (isCreate) {
            onCreated(updated);
          } else {
            onUpdated(updated);
          }
        } catch (e) {
          if (import.meta.env.DEV) console.warn("[usage] post-OAuth metadata update failed", e);
          if (isCreate) {
            onCreated(created);
          } else {
            onUpdated(created);
          }
        }
      } else {
        if (isCreate) {
          onCreated(created);
        } else {
          onUpdated(created);
        }
      }
      toast.success(isCreate ? t("usage.toastAdded") : t("usage.toastUpdated"));
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[usage] OAuth login failed", {
        catalogId,
        region,
        authMode,
        subscriptionId: editing?.id,
        error: err,
      });
      toast.error(t("usage.toastLoginFailed", { error: msg }));
    } finally {
      setOauthPendingId(null);
      setOauthStatus(null);
      setOauthUserCode(null);
      setSubmitting(false);
    }
  };

  const importFromLocal = async () => {
    if (!catalogId) {
      toast.error(t("usage.toastSelectProvider"));
      return;
    }
    setSubmitting(true);
    try {
      const created = await usageApi.importSubscriptionFromLocal(catalogId);
      onCreated(created);
      toast.success(t("usage.importFromLocalSuccess"));
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      console.error("[usage] import from local failed", { catalogId, error: err });
      toast.error(t("usage.importFromLocalFailed", { error: msg }));
    } finally {
      setSubmitting(false);
    }
  };

  const openCookieBridgeSettings = async () => {
    navigateToSettingsSection("cookie-bridge");
    await openExternalUrl("https://opencode.ai/workspace/default/go");
  };

  const copyDeviceCode = async () => {
    if (!oauthUserCode) return;
    try {
      await navigator.clipboard.writeText(oauthUserCode);
      toast.success(t("usage.oauthCodeCopied"));
    } catch {
      toast.error(t("common.copyFailed", { defaultValue: "Copy failed" }));
    }
  };

  const cancelOAuth = async () => {
    if (!oauthPendingId) return;
    try {
      await usageApi.cancelOAuthLogin(oauthPendingId);
    } catch {
      /* ignore */
    }
    setOauthPendingId(null);
    setOauthStatus(null);
    setOauthUserCode(null);
    setSubmitting(false);
  };

  return (
    <AnimatePresence>
      <motion.div
        key="backdrop"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-[80] flex items-center justify-center bg-overlay p-4 backdrop-blur-sm"
        onClick={onClose}
      >
        <motion.div
          key="dialog"
          role="dialog"
          aria-modal="true"
          aria-label={title}
          initial={{ opacity: 0, scale: 0.95, y: 12 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.95, y: 12 }}
          transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          className="modal-surface relative flex max-h-[min(90vh,660px)] w-full max-w-lg flex-col overflow-hidden"
          onClick={(e) => e.stopPropagation()}
        >
          <header className="flex shrink-0 items-center justify-between border-b border-border px-5 py-3">
            <h2 className="text-heading-sm">{title}</h2>
            <button
              type="button"
              className="text-muted-foreground hover:text-foreground transition-colors"
              onClick={onClose}
              aria-label={t("common.close")}
            >
              <X className="h-4 w-4" />
            </button>
          </header>

          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
            {/* ── 智能自动导入板块 (Smart Auto-Import Banner) ── */}
            {showAutoImportBanner && (
              <div className="relative overflow-hidden rounded-2xl border border-primary/20 bg-primary/5 p-4 text-center">
                <h4 className="mb-1.5 flex items-center justify-center gap-1.5 text-xs font-bold uppercase tracking-wider text-primary">
                  ⚡ 智能自动导入 (Smart Auto-Import)
                </h4>
                <p className="mx-auto mb-3.5 max-w-xs text-[11px] leading-normal text-muted-foreground sm:max-w-sm">
                  {catalogId
                    ? `自动扫描并导入本地的 ${selectedEntry?.display_name ?? catalogId} 账号凭证，无需手动填写。`
                    : "一键自动扫描并导入本地的 Codex / Antigravity / Qoder 账号凭证，无需手动填写。"}
                </p>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={catalogId ? importFromLocal : handleAutoScanAll}
                  disabled={scanningLocal || submitting}
                  className="w-full border-primary/25 bg-primary/10 px-6 font-semibold text-primary hover:bg-primary/15 sm:w-auto"
                >
                  {scanningLocal ? (
                    <>
                      <Loader2 className="mr-2 h-3.5 w-3.5 animate-spin" />
                      正在扫描本地环境…
                    </>
                  ) : catalogId ? (
                    "🚀 自动扫描并导入"
                  ) : (
                    "🚀 一键自动扫描并导入"
                  )}
                </Button>
              </div>
            )}

            {selectedEntry ? (
              <ProviderCatalogHero
                entry={selectedEntry}
                displayTitle={!isCreate ? editing?.display_name : undefined}
                authMode={authMode}
              />
            ) : isCreate ? (
              <p className="rounded-xl border border-dashed border-border bg-muted/30 px-4 py-3 text-center text-[11px] leading-relaxed text-muted-foreground">
                {t("usage.pickProviderFromSidebar")}
              </p>
            ) : null}

            {selectedEntry?.warning && (
              <p className="rounded-xl border border-warning/30 bg-warning/10 px-3 py-2 text-[11px] leading-normal text-warning">
                ⚠ {selectedEntry.warning}
              </p>
            )}

            {visibleAuthModes.length > 1 && (
              <Field label={t("usage.fieldAuthMode")}>
                <div className="flex gap-1.5 rounded-xl border border-border bg-muted/50 p-1">
                  {visibleAuthModes.map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => setAuthMode(mode)}
                      className={cn(
                        "flex-1 rounded-lg border px-2.5 py-1.5 text-[11px] font-semibold transition-all duration-200",
                        authMode === mode
                          ? "border-border bg-background text-foreground shadow-sm"
                          : "border-transparent text-muted-foreground hover:text-foreground",
                      )}
                    >
                      {labelAuthMode(mode)}
                    </button>
                  ))}
                </div>
              </Field>
            )}

            {authMode === "o-auth" && selectedEntry?.regions.length ? (
              <Field label={t("usage.fieldRegion")}>
                <div className="flex gap-1.5 rounded-xl border border-border bg-muted/50 p-1">
                  {selectedEntry.regions.map((r) => (
                    <button
                      key={r}
                      type="button"
                      onClick={() => setRegion(r)}
                      className={cn(
                        "flex-1 rounded-lg border px-2.5 py-1.5 text-[11px] font-bold uppercase transition-all duration-200",
                        region === r
                          ? "border-border bg-background text-foreground shadow-sm"
                          : "border-transparent text-muted-foreground hover:text-foreground",
                      )}
                    >
                      {r}
                    </button>
                  ))}
                </div>
              </Field>
            ) : null}

            {/* ── 极其简化的核心字段 (Collapsible Minimal Form Fields) ── */}
            {selectedEntry && (
              <div className="space-y-4 animate-in fade-in slide-in-from-top-1 duration-200">
                {/* 1. API Key 认证仅需填 Key */}
                {authMode === "api-key" && (
                  <Field
                    label={editing ? t("usage.fieldApiKeyOptional") : "API Key"}
                    hint={selectedEntry?.id === "glm" ? t("usage.glmApiKeyHint") : undefined}
                  >
                    <div className="relative">
                      <Input
                        value={apiKey}
                        onChange={(e) => setApiKey(e.target.value)}
                        placeholder="sk-..."
                        type={showKey ? "text" : "password"}
                        autoComplete="off"
                        className="h-9 rounded-xl border-input-border bg-input pr-9 text-xs text-foreground"
                      />
                      <button
                        type="button"
                        className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                        onClick={() => setShowKey((v) => !v)}
                      >
                        {showKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                      </button>
                    </div>
                  </Field>
                )}

                {/* Cookie 模式：粘贴浏览器的 Cookie Header */}
                {authMode === "cookie" && (
                  <div className="space-y-2.5 rounded-2xl border border-border bg-muted/30 p-3.5">
                    <h4 className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                      🍪 浏览器 Cookie
                    </h4>

                    {/* OpenCode: Go / Zen 选择器 */}
                    {catalogId === "opencode" && (
                      <div className="rounded-xl border border-border bg-background/60 p-2.5">
                        <p className="mb-2 text-[10px] font-semibold text-muted-foreground">选择订阅</p>
                        <div className="flex gap-1.5 rounded-lg border border-border bg-muted/50 p-1">
                          {["Go", "Zen"].map((choice) => (
                            <button
                              key={choice}
                              type="button"
                              onClick={() => setPlanTier(choice)}
                              className={cn(
                                "flex-1 rounded-md border px-3 py-1.5 text-[11px] font-bold tracking-wide transition-all duration-200",
                                planTier === choice
                                  ? "border-primary/40 bg-primary/10 text-primary shadow-sm"
                                  : "border-transparent text-muted-foreground hover:text-foreground",
                              )}
                            >
                              {choice === "Go" ? "🚀 Go" : "⚡ Zen"}
                            </button>
                          ))}
                        </div>
                        <p className="mt-1.5 text-[9px] leading-snug text-muted-foreground/70">
                          {planTier === "Go"
                            ? "$10/月 开源模型订阅"
                            : planTier === "Zen"
                              ? "按量付费 AI 网关"
                              : "请选择 Go 或 Zen"}
                        </p>
                      </div>
                    )}

                    {catalogId === "opencode" && (
                      <div className="rounded-xl border border-primary/20 bg-primary/5 p-2.5">
                        <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                          <div>
                            <p className="text-[10px] font-bold text-primary">浏览器插件导入</p>
                            <p className="mt-1 text-[9px] leading-snug text-muted-foreground">
                              首次用绑定码连接 SkillStar；绑定后，插件会记住本机连接，后续点击即可推送 Cookie。
                            </p>
                          </div>
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={openCookieBridgeSettings}
                            disabled={submitting}
                            className="shrink-0 rounded-lg"
                          >
                            去设置中绑定
                          </Button>
                        </div>
                      </div>
                    )}

                    <p className="text-[10px] leading-relaxed text-muted-foreground">
                      在浏览器打开 opencode.ai 的 workspace/default/go 页面后，打开 DevTools → Network → 找到
                      <code className="mx-0.5 rounded bg-muted px-1 text-[10px]">/workspace/...</code> 或
                      <code className="mx-0.5 rounded bg-muted px-1 text-[10px]">/_server</code> 请求 → 右键
                      <code className="mx-0.5 rounded bg-muted px-1 text-[10px]">Copy as cURL</code>→ 粘贴到终端中提取
                      Cookie 字段，或直接从 Request Headers 中复制
                      <code className="mx-0.5 rounded bg-muted px-1 text-[10px]">Cookie:</code> 后面的完整内容。
                    </p>
                    <textarea
                      value={cookieHeader}
                      onChange={(e) => setCookieHeader(e.target.value)}
                      placeholder="sessionid=abc123; csrftoken=xyz789; ..."
                      rows={3}
                      className="w-full rounded-xl border border-input-border bg-input p-2.5 text-xs text-foreground placeholder:text-muted-foreground/60 resize-none focus:border-primary/50 focus:outline-none focus:ring-1 focus:ring-primary/40"
                    />
                    <p className="text-[9px] leading-normal text-muted-foreground/70">
                      Cookie 会加密存储在本机。粘贴后点击「添加」即可保存，之后可点击刷新按钮拉取最新用量数据。
                    </p>
                  </div>
                )}

                {/* 2. 手动 Quota 录入 */}
                {authMode === "manual" && (
                  <div className="grid grid-cols-2 gap-3.5 rounded-2xl border border-border bg-muted/30 p-3.5">
                    <h4 className="col-span-2 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                      📊 {t("usage.manualQuota")}
                    </h4>
                    <Field label={t("usage.fieldTotalTokens")}>
                      <Input
                        type="number"
                        value={totalTokens}
                        onChange={(e) => setTotalTokens(e.target.value)}
                        placeholder="1,000,000"
                        className="h-9 rounded-xl border-input-border bg-input text-xs text-foreground"
                      />
                    </Field>
                    <Field label={t("usage.fieldUsedTokens")}>
                      <Input
                        type="number"
                        value={usedTokens}
                        onChange={(e) => setUsedTokens(e.target.value)}
                        placeholder="120,000"
                        className="h-9 rounded-xl border-input-border bg-input text-xs text-foreground"
                      />
                    </Field>
                  </div>
                )}

                {/* 3. 高级与账单选项折叠面板 (Advanced Folding) */}
                <div className="mt-2.5">
                  <button
                    type="button"
                    onClick={() => setShowAdvanced((v) => !v)}
                    className="flex w-full items-center justify-between rounded-xl border border-border bg-muted/40 px-4 py-2.5 text-xs font-semibold text-muted-foreground transition-all hover:bg-muted/60 hover:text-foreground"
                  >
                    <span>⚙️ 付费及高级选项 (Advanced & Billing Settings)</span>
                    <ChevronDown
                      className={cn("h-4 w-4 transition-transform duration-200", showAdvanced && "rotate-180")}
                    />
                  </button>

                  {showAdvanced && (
                    <AdvancedBillingSection
                      selectedEntry={selectedEntry}
                      authMode={authMode}
                      submitting={submitting}
                      displayName={displayName}
                      setDisplayName={setDisplayName}
                      planTier={planTier}
                      setPlanTier={setPlanTier}
                      fingerprintId={fingerprintId}
                      setFingerprintId={setFingerprintId}
                      billingCycleOptions={billingCycleOptions}
                      billingCycle={billingCycle}
                      setBillingCycle={setBillingCycle}
                      labelBillingCycle={labelBillingCycle}
                      billingCycleHint={billingCycleHint}
                      priceLabel={priceLabel}
                      price={price}
                      setPrice={setPrice}
                      currency={currency}
                      setCurrency={setCurrency}
                      startDate={startDate}
                      setStartDate={setStartDate}
                      endDate={endDate}
                      setEndDate={setEndDate}
                      endDateLabel={endDateLabel}
                      autoRenew={autoRenew}
                      setAutoRenew={setAutoRenew}
                      periodLabel={periodLabel}
                      setPeriodLabel={setPeriodLabel}
                      note={note}
                      setNote={setNote}
                    />
                  )}
                </div>
              </div>
            )}
          </div>

          {oauthUserCode && (
            <div className="shrink-0 space-y-1.5 border-t border-border bg-muted/30 px-5 py-3">
              <p className="text-[10px] font-semibold text-foreground">{t("usage.oauthDeviceCodeTitle")}</p>
              <p className="text-[9px] text-muted-foreground">{t("usage.oauthDeviceCodeHint")}</p>
              <div className="mt-1 flex items-center gap-2">
                <code className="flex-1 rounded-lg border border-border bg-background px-2.5 py-1.5 text-center text-lg font-bold tracking-[0.2em] text-foreground tabular-nums">
                  {oauthUserCode}
                </code>
                <Button type="button" size="sm" variant="outline" onClick={copyDeviceCode} className="rounded-lg h-9">
                  <Copy className="h-3.5 w-3.5 mr-1" />
                  {t("usage.oauthCopyCode")}
                </Button>
              </div>
            </div>
          )}

          {oauthStatus && (
            <div className="relative flex shrink-0 items-center gap-3 overflow-hidden border-t border-border bg-primary/10 px-5 py-3 text-[10px] text-primary">
              <motion.div
                className="pointer-events-none absolute inset-y-0 left-0 w-1/3 bg-gradient-to-r from-transparent via-primary/15 to-transparent"
                animate={reduceMotion ? undefined : { x: ["-120%", "320%"] }}
                transition={reduceMotion ? undefined : { duration: 1.7, repeat: Infinity, ease: "linear" }}
              />
              <div className="relative flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-primary/10 ring-1 ring-primary/20">
                <motion.span
                  className="absolute inset-0 rounded-full border border-primary/30"
                  animate={reduceMotion ? undefined : { scale: [0.75, 1.35], opacity: [0.6, 0] }}
                  transition={reduceMotion ? undefined : { duration: 1.2, repeat: Infinity, ease: "easeOut" }}
                />
                <Loader2 className="relative h-4 w-4 animate-spin" />
              </div>
              <div className="relative flex-1">
                <p className="font-semibold">{oauthStatus}</p>
                <p className="mt-0.5 text-[9px] text-primary/70">{t("usage.oauthWaitingHint")}</p>
              </div>
              {oauthPendingId && (
                <button
                  type="button"
                  className="relative rounded-full px-2 py-1 text-primary underline hover:bg-primary/10 hover:text-primary-hover"
                  onClick={cancelOAuth}
                >
                  {t("usage.cancelOAuth")}
                </button>
              )}
            </div>
          )}

          <footer className="flex shrink-0 justify-between border-t border-border bg-muted/30 px-5 py-3">
            <div>
              {!isCreate && (
                <Button variant="destructive" size="sm" onClick={() => setDeleteConfirming(true)} disabled={submitting}>
                  <Trash2 className="h-3.5 w-3.5 mr-1" />
                  {t("common.delete")}
                </Button>
              )}
            </div>
            <div className="flex gap-2">
              <Button variant="ghost" size="sm" onClick={onClose} disabled={submitting}>
                {t("common.cancel")}
              </Button>
              {authMode === "o-auth" ? (
                <div className="flex items-center gap-2">
                  {canImportLocal && (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={importFromLocal}
                      disabled={submitting || !catalogId}
                      title={t("usage.importFromLocalHint")}
                    >
                      <FolderInput className="mr-1 h-3.5 w-3.5" />
                      {t("usage.importFromLocal")}
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant={isCreate ? "default" : "outline"}
                    onClick={startOAuthFlow}
                    disabled={submitting || !catalogId}
                  >
                    <ExternalLink className="h-3.5 w-3.5 mr-1" />
                    {submitting ? t("usage.btnWaitingLogin") : t("usage.btnLoginBrowser")}
                  </Button>
                  {!isCreate && (
                    <Button size="sm" onClick={submit} disabled={submitting} className="px-5">
                      {submitting ? t("common.saving") : t("common.save")}
                    </Button>
                  )}
                </div>
              ) : (
                <Button size="sm" onClick={submit} disabled={submitting} className="px-5">
                  {submitting ? t("common.saving") : isCreate ? t("common.add") : t("common.save")}
                </Button>
              )}
            </div>
          </footer>

          {deleteConfirming && (
            <div className="absolute inset-0 z-30 flex items-center justify-center rounded-lg bg-background/90 backdrop-blur-sm">
              <div className="mx-6 rounded-2xl border border-red-200 bg-card p-5 shadow-xl">
                <p className="mb-1 text-sm font-semibold text-foreground">
                  {t("usage.confirmDeleteTitle", "确认删除")}
                </p>
                <p className="mb-4 text-xs text-muted-foreground">
                  {t("usage.confirmDeleteMsg", { name: editing?.display_name ?? "" })}
                </p>
                <div className="flex justify-end gap-2">
                  <Button size="sm" variant="ghost" onClick={() => setDeleteConfirming(false)}>
                    {t("common.cancel")}
                  </Button>
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => {
                      setDeleteConfirming(false);
                      onDeleted();
                    }}
                  >
                    {t("common.delete")}
                  </Button>
                </div>
              </div>
            </div>
          )}
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}

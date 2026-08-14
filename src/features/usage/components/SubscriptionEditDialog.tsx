import { useReducedMotion } from "framer-motion";
import { ChevronDown, FolderInput, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { ModalShell } from "@/components/ui/ModalShell";
import { openExternalUrl } from "@/lib/externalOpen";
import { cn } from "@/lib/utils";
import { usageApi } from "../api";
import { selectableAuthModes } from "../lib/authModes";
import { authModeLabel } from "../lib/usageLabels";
import {
  LOCAL_IMPORT_CATALOG_IDS,
  type AuthMode,
  type BillingCycle,
  type CatalogEntry,
  type OAuthStart,
  type Subscription,
} from "../types";
import { ProviderCatalogHero } from "./ProviderCatalogHero";
import { AdvancedBillingSection } from "./subscriptionEdit/AdvancedBillingSection";
import { ApiKeyFields } from "./subscriptionEdit/api/ApiKeyFields";
import { AutoImportBanner } from "./subscriptionEdit/AutoImportBanner";
import { CookieFields } from "./subscriptionEdit/cookie/CookieFields";
import { Field, parseDateInput, toDateInput } from "./subscriptionEdit/fields";
import { OAuthLoginPanel } from "./subscriptionEdit/oauth/OAuthLoginPanel";

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
  const [authMode, setAuthMode] = useState<AuthMode>("o-auth");
  const [planTier, setPlanTier] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [price, setPrice] = useState("");
  const [currency, setCurrency] = useState("CNY");
  const [billingCycle, setBillingCycle] = useState<BillingCycle>("monthly");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
  const [autoRenew, setAutoRenew] = useState(false);
  const [apiKey, setApiKey] = useState("");
  const [platformToken, setPlatformToken] = useState("");
  const [cookieHeader, setCookieHeader] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [showPlatformToken, setShowPlatformToken] = useState(false);
  const [region, setRegion] = useState("cn");
  const [note, setNote] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [oauthStart, setOauthStart] = useState<OAuthStart | null>(null);
  const [oauthPendingId, setOauthPendingId] = useState<string | null>(null);
  const [oauthStatus, setOauthStatus] = useState<string | null>(null);
  const [oauthCallbackInput, setOauthCallbackInput] = useState("");
  const [oauthSubmittingCallback, setOauthSubmittingCallback] = useState(false);
  const [deleteConfirming, setDeleteConfirming] = useState(false);
  const oauthCancelledRef = useRef<string | null>(null);

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
      const resolvedAuthMode = editingModes.includes(editing.auth_mode)
        ? editing.auth_mode
        : (editingModes[0] ?? "o-auth");
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
      setPlatformToken("");
      setCookieHeader("");
      setRegion(editing.oauth_region ?? "cn");
      setNote(editing.note ?? "");
    } else {
      const preselectedEntry = preselectCatalogId ? catalog.find((c) => c.id === preselectCatalogId) : null;
      const today = toDateInput(Math.floor(Date.now() / 1000));
      setCatalogId(preselectCatalogId ?? "");
      setAuthMode(selectableAuthModes(preselectedEntry?.auth_modes ?? [])[0] ?? "o-auth");
      setPlanTier("");
      setDisplayName("");
      setPrice("");
      setCurrency(preselectedEntry?.default_currency ?? "CNY");
      // API-key catalogs default to quota billing; OAuth subscriptions default to monthly.
      const defaultModes = selectableAuthModes(preselectedEntry?.auth_modes ?? []);
      setBillingCycle(defaultModes[0] === "api-key" || preselectedEntry?.tier === "api-key" ? "api-key" : "monthly");
      setStartDate(today);
      setEndDate("");
      setAutoRenew(false);
      setApiKey("");
      setPlatformToken("");
      setCookieHeader("");
      setRegion(preselectedEntry?.regions[0] ?? "cn");
      setNote("");
    }
    setShowKey(false);
    setShowPlatformToken(false);
    setOauthStart(null);
    setOauthPendingId(null);
    setOauthStatus(null);
    setOauthCallbackInput("");
    setOauthSubmittingCallback(false);
    oauthCancelledRef.current = null;
  }, [open, editing, preselectCatalogId, t]);

  useEffect(() => {
    if (!open || editing) return;
    if (!catalogId) {
      setAuthMode("o-auth");
      return;
    }
    const entry = catalog.find((c) => c.id === catalogId);
    if (entry) {
      const modes = selectableAuthModes(entry.auth_modes);
      if (!modes.includes(authMode)) {
        setAuthMode(modes[0] ?? "o-auth");
      }
      setCurrency(entry.default_currency);
      // Prefer quota billing for API-key-tier catalogs (DeepSeek / GLM / Kimi…).
      if (entry.tier === "api-key" || modes[0] === "api-key") {
        setBillingCycle("api-key");
      }
      if (entry.regions.length > 0 && !entry.regions.includes(region)) {
        setRegion(entry.regions[0]);
      }
    }
  }, [catalogId, catalog, editing, authMode, region, open]);

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
      case "api-key":
        return t("usage.priceApiKey");
      default:
        return t("usage.priceMonthly");
    }
  })();

  const endDateLabel = autoRenew ? t("usage.fieldNextRenew") : t("usage.fieldEndDate");

  const labelAuthMode = (mode: AuthMode) => authModeLabel(mode, t);

  const billingCycleOptions: BillingCycle[] = ["monthly", "annual", "one-time", "api-key"];

  const labelBillingCycle = (cycle: BillingCycle) => {
    switch (cycle) {
      case "annual":
        return t("usage.cycleAnnualShort");
      case "one-time":
        return t("usage.cycleOneTimeShort");
      case "api-key":
        return t("usage.cycleApiKeyShort");
      default:
        return t("usage.cycleMonthlyShort");
    }
  };

  const billingCycleHint =
    billingCycle === "annual"
      ? t("usage.billingHintAnnual")
      : billingCycle === "one-time"
        ? t("usage.billingHintOneTime")
        : billingCycle === "api-key"
          ? t("usage.billingHintApiKey")
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
      toast.error(t("usage.autoImportNoCredentials"));
    }
  };

  if (!open) return null;

  const buildPayload = () => {
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
      const apiKeyPayload = authMode === "api-key" && apiKey.trim() ? apiKey.trim() : undefined;
      const platformTokenPayload =
        catalogId === "deepseek" && authMode === "api-key" && platformToken.trim() ? platformToken.trim() : undefined;
      const cookieHeaderPayload = authMode === "cookie" && cookieHeader.trim() ? cookieHeader.trim() : undefined;
      const shouldRefreshAfterSave = Boolean(apiKeyPayload || platformTokenPayload || cookieHeaderPayload);

      const refreshAfterCredentialSave = async (sub: Subscription) => {
        if (!shouldRefreshAfterSave) return sub;
        try {
          return await usageApi.refreshSubscriptionUsage(sub.id);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          toast.warning(t("usage.refreshOneFailed", { name: sub.display_name, error: msg }));
          return sub;
        }
      };

      if (isCreate) {
        const created = await usageApi.createSubscription({
          catalog_id: catalogId,
          auth_mode: authMode,
          api_key: apiKeyPayload,
          platform_token: platformTokenPayload,
          cookie_header: cookieHeaderPayload,
          oauth_region: authMode === "o-auth" ? region : undefined,
          ...payload,
        });
        onCreated(await refreshAfterCredentialSave(created));
        toast.success(t("usage.toastAdded"));
      } else {
        const updated = await usageApi.updateSubscription(editing.id, {
          ...payload,
          api_key: apiKeyPayload,
          platform_token: platformTokenPayload,
          cookie_header: cookieHeaderPayload,
        });
        onUpdated(await refreshAfterCredentialSave(updated));
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

  const resetOAuthState = () => {
    setOauthStart(null);
    setOauthPendingId(null);
    setOauthStatus(null);
    setOauthCallbackInput("");
    setOauthSubmittingCallback(false);
  };

  const finishOAuthSubscription = async (created: Subscription) => {
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
      return;
    }

    if (isCreate) {
      onCreated(created);
    } else {
      onUpdated(created);
    }
  };

  const waitForOAuthCompletion = async (pendingId: string) => {
    try {
      const created = await usageApi.awaitOAuthCompletion(pendingId);
      await finishOAuthSubscription(created);
      toast.success(isCreate ? t("usage.toastAdded") : t("usage.toastUpdated"));
      onClose();
    } catch (err) {
      if (oauthCancelledRef.current === pendingId) return;
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
      if (oauthCancelledRef.current === pendingId) {
        oauthCancelledRef.current = null;
      }
      resetOAuthState();
    }
  };

  const startOAuthFlow = async () => {
    if (!catalogId) {
      toast.error(t("usage.toastSelectProvider"));
      return;
    }
    setSubmitting(true);
    setOauthStart(null);
    setOauthCallbackInput("");
    setOauthStatus(t("usage.oauthGenerating"));
    try {
      const start = await usageApi.startOAuthLogin(
        catalogId,
        selectedEntry?.regions.length ? region : undefined,
        isCreate ? undefined : editing.id,
      );
      oauthCancelledRef.current = null;
      setOauthStart(start);
      setOauthPendingId(start.pending_id);
      setOauthStatus(t("usage.oauthWaiting"));
      void waitForOAuthCompletion(start.pending_id);
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
      setOauthStart(null);
      setOauthPendingId(null);
      setOauthStatus(null);
    } finally {
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

  const copyAuthLink = async () => {
    if (!oauthStart?.auth_url) return;
    try {
      await navigator.clipboard.writeText(oauthStart.auth_url);
      toast.success(t("usage.oauthLinkCopied"));
    } catch {
      toast.error(t("common.copyFailed", { defaultValue: "Copy failed" }));
    }
  };

  const openOAuthLink = async () => {
    if (!oauthStart?.auth_url) return;
    await openExternalUrl(oauthStart.auth_url);
  };

  const copyDeviceCode = async () => {
    if (!oauthStart?.user_code) return;
    try {
      await navigator.clipboard.writeText(oauthStart.user_code);
      toast.success(t("usage.oauthCodeCopied"));
    } catch {
      toast.error(t("common.copyFailed", { defaultValue: "Copy failed" }));
    }
  };

  const submitOAuthCallback = async () => {
    if (!oauthPendingId) return;
    if (!oauthCallbackInput.trim()) {
      toast.error(t("usage.oauthCallbackRequired"));
      return;
    }
    setOauthSubmittingCallback(true);
    try {
      await usageApi.submitOAuthCallback(oauthPendingId, oauthCallbackInput);
      setOauthStatus(t("usage.oauthCompleting"));
      toast.success(t("usage.oauthCallbackSubmitted"));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(t("usage.oauthCallbackFailed", { error: msg }));
    } finally {
      setOauthSubmittingCallback(false);
    }
  };

  const cancelOAuth = async () => {
    if (!oauthPendingId) return;
    const pendingId = oauthPendingId;
    oauthCancelledRef.current = pendingId;
    try {
      await usageApi.cancelOAuthLogin(pendingId);
    } catch {
      /* ignore */
    }
    resetOAuthState();
    setSubmitting(false);
  };

  const requestClose = () => {
    if (oauthPendingId) {
      void cancelOAuth();
    }
    onClose();
  };

  return (
    <ModalShell
      open={open}
      onClose={requestClose}
      ariaLabel={title}
      panelClassName="max-w-lg px-4"
      surfaceClassName="flex max-h-[min(90vh,660px)] flex-col overflow-hidden"
      contentClassName="flex min-h-0 flex-1 flex-col"
    >
      <header className="flex shrink-0 items-center justify-between border-b border-border px-5 py-3">
        <h2 className="text-heading-sm">{title}</h2>
        <button
          type="button"
          className="text-muted-foreground hover:text-foreground transition-colors"
          onClick={requestClose}
          aria-label={t("common.close")}
        >
          <X className="h-4 w-4" />
        </button>
      </header>

      <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4">
        {/* ── 智能自动导入板块 (Smart Auto-Import Banner) ── */}
        {showAutoImportBanner && (
          <AutoImportBanner
            catalogId={catalogId}
            providerName={selectedEntry?.display_name}
            scanningLocal={scanningLocal}
            submitting={submitting}
            onImportLocal={importFromLocal}
            onAutoScanAll={handleAutoScanAll}
          />
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

        {authMode === "o-auth" && selectedEntry && (
          <OAuthLoginPanel
            selectedEntry={selectedEntry}
            submitting={submitting}
            oauthIsActiveMode={authMode === "o-auth"}
            oauthStart={oauthStart}
            oauthPendingId={oauthPendingId}
            oauthStatus={oauthStatus}
            oauthCallbackInput={oauthCallbackInput}
            setOauthCallbackInput={setOauthCallbackInput}
            oauthSubmittingCallback={oauthSubmittingCallback}
            reduceMotion={reduceMotion}
            onStartOAuth={startOAuthFlow}
            onCopyAuthLink={copyAuthLink}
            onOpenOAuthLink={openOAuthLink}
            onCopyDeviceCode={copyDeviceCode}
            onSubmitCallback={submitOAuthCallback}
            onCancelOAuth={cancelOAuth}
          />
        )}

        {/* ── 极其简化的核心字段 (Collapsible Minimal Form Fields) ── */}
        {selectedEntry && (
          <div className="space-y-4 animate-in fade-in slide-in-from-top-1 duration-200">
            {/* 1. API Key 认证仅需填 Key */}
            {authMode === "api-key" && (
              <ApiKeyFields
                editing={editing}
                catalogId={catalogId}
                selectedEntry={selectedEntry}
                apiKey={apiKey}
                setApiKey={setApiKey}
                showKey={showKey}
                setShowKey={setShowKey}
                platformToken={platformToken}
                setPlatformToken={setPlatformToken}
                showPlatformToken={showPlatformToken}
                setShowPlatformToken={setShowPlatformToken}
              />
            )}

            {/* 1b. Cookie 认证：粘贴浏览器 Cookie 头（stepfun / opencode） */}
            {authMode === "cookie" && (
              <CookieFields
                editing={editing}
                selectedEntry={selectedEntry}
                cookieHeader={cookieHeader}
                setCookieHeader={setCookieHeader}
              />
            )}

            {/* 2. 高级与账单选项折叠面板 (Advanced Folding) */}
            <div className="mt-2.5">
              <button
                type="button"
                onClick={() => setShowAdvanced((v) => !v)}
                className="flex w-full items-center justify-between rounded-xl border border-border bg-muted/50 px-4 py-2.5 text-xs font-semibold text-foreground/75 transition-all hover:bg-muted/70 hover:text-foreground"
              >
                <span>{t("usage.advancedBillingOptions")}</span>
                <ChevronDown
                  className={cn("h-4 w-4 transition-transform duration-200", showAdvanced && "rotate-180")}
                />
              </button>

              {showAdvanced && (
                <AdvancedBillingSection
                  selectedEntry={selectedEntry}
                  authMode={authMode}
                  displayName={displayName}
                  setDisplayName={setDisplayName}
                  planTier={planTier}
                  setPlanTier={setPlanTier}
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
                  note={note}
                  setNote={setNote}
                />
              )}
            </div>
          </div>
        )}
      </div>

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
          <Button variant="ghost" size="sm" onClick={requestClose} disabled={submitting}>
            {t("common.cancel")}
          </Button>
          {authMode === "o-auth" ? (
            <div className="flex items-center gap-2">
              {canImportLocal && (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={importFromLocal}
                  disabled={submitting || !!oauthPendingId || !catalogId}
                  title={t("usage.importFromLocalHint")}
                >
                  <FolderInput className="mr-1 h-3.5 w-3.5" />
                  {t("usage.importFromLocal")}
                </Button>
              )}
              {!isCreate && (
                <Button size="sm" onClick={submit} disabled={submitting || !!oauthPendingId} className="px-5">
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
            <p className="mb-1 text-sm font-semibold text-foreground">{t("usage.confirmDeleteTitle", "确认删除")}</p>
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
    </ModalShell>
  );
}

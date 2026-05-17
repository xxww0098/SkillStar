import { AnimatePresence, motion } from "framer-motion";
import { ChevronDown, ExternalLink, Eye, EyeOff, Loader2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { openExternalUrl } from "@/lib/externalOpen";
import { cn } from "@/lib/utils";
import { usageApi } from "../api";
import type { AuthMode, BillingCycle, CatalogEntry, Subscription } from "../types";
import { ProviderLogo } from "./ProviderLogo";

interface SubscriptionEditDialogProps {
  open: boolean;
  catalog: CatalogEntry[];
  editing: Subscription | null;
  preselectCatalogId?: string | null;
  onClose: () => void;
  onCreated: (sub: Subscription) => void;
  onUpdated: (sub: Subscription) => void;
}

const selectClass =
  "h-9 w-full rounded-md border border-border/60 bg-background px-2 text-sm focus:border-primary/50 focus:outline-none focus:ring-1 focus:ring-primary/40";

export function SubscriptionEditDialog({
  open,
  catalog,
  editing,
  preselectCatalogId,
  onClose,
  onCreated,
  onUpdated,
}: SubscriptionEditDialogProps) {
  const isCreate = !editing;
  const title = isCreate ? "新增订阅" : "编辑订阅";

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
  const [region, setRegion] = useState("cn");
  const [totalTokens, setTotalTokens] = useState("");
  const [usedTokens, setUsedTokens] = useState("");
  const [periodLabel, setPeriodLabel] = useState("本月");
  const [note, setNote] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [oauthPendingId, setOauthPendingId] = useState<string | null>(null);
  const [oauthStatus, setOauthStatus] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    if (editing) {
      setCatalogId(editing.catalog_id);
      setAuthMode(editing.auth_mode);
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
      setTotalTokens(editing.manual_quota?.total_tokens?.toString() ?? "");
      setUsedTokens(editing.manual_quota?.used_tokens?.toString() ?? "");
      setPeriodLabel(editing.manual_quota?.period_label ?? "本月");
      setNote(editing.note ?? "");
    } else {
      const today = toDateInput(Math.floor(Date.now() / 1000));
      setCatalogId(preselectCatalogId ?? "");
      setAuthMode("manual");
      setPlanTier("");
      setDisplayName("");
      setPrice("");
      setCurrency("CNY");
      setBillingCycle("monthly");
      setStartDate(today);
      setEndDate("");
      setAutoRenew(false);
      setApiKey("");
      setRegion("cn");
      setTotalTokens("");
      setUsedTokens("");
      setPeriodLabel("本月");
      setNote("");
    }
    setShowKey(false);
    setOauthPendingId(null);
    setOauthStatus(null);
  }, [open, editing, preselectCatalogId]);

  useEffect(() => {
    if (!open || editing) return;
    if (!catalogId) {
      setAuthMode("manual");
      return;
    }
    const entry = catalog.find((c) => c.id === catalogId);
    if (entry && !entry.auth_modes.includes(authMode)) {
      setAuthMode(entry.auth_modes[0] ?? "manual");
    }
    if (entry && entry.regions.length > 0 && !entry.regions.includes(region)) {
      setRegion(entry.regions[0]);
    }
    if (entry) {
      setCurrency(entry.default_currency);
    }
  }, [catalogId, catalog, editing, authMode, region, open]);

  const selectedEntry = useMemo(() => catalog.find((c) => c.id === catalogId) ?? null, [catalog, catalogId]);
  const priceLabel = priceLabelForCycle(billingCycle);
  const endDateLabel = autoRenew ? "下次续费日" : "截止日期";

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

    return {
      display_name: displayName || undefined,
      plan_tier: planTier || undefined,
      monthly_price: price ? Number(price) : undefined,
      currency: currency || undefined,
      billing_cycle: billingCycle,
      start_date: parseDateInput(startDate),
      renew_date: parseDateInput(endDate),
      auto_renew: autoRenew,
      manual_quota: manualQuota,
      note: note || undefined,
    };
  };

  const submit = async () => {
    if (!catalogId) {
      toast.error("请选择供应商");
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
          ...payload,
        });
        onCreated(created);
        toast.success("已添加");
      } else {
        const updated = await usageApi.updateSubscription(editing.id, {
          ...payload,
          api_key: apiKey || undefined,
        });
        onUpdated(updated);
        toast.success("已更新");
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
      toast.error("请选择供应商");
      return;
    }
    setSubmitting(true);
    setOauthStatus("正在生成登录链接…");
    try {
      const { auth_url, pending_id } = await usageApi.startOAuthLogin(
        catalogId,
        selectedEntry?.regions.length ? region : undefined,
      );
      setOauthPendingId(pending_id);
      setOauthStatus("已在浏览器中打开登录页，请完成授权…");
      await openExternalUrl(auth_url);

      const created = await usageApi.awaitOAuthCompletion(pending_id);
      const meta = buildPayload();
      if (meta.monthly_price || meta.renew_date || meta.start_date || meta.plan_tier || meta.note) {
        try {
          const updated = await usageApi.updateSubscription(created.id, meta);
          onCreated(updated);
        } catch (e) {
          console.warn("[usage] post-OAuth metadata update failed", e);
          onCreated(created);
        }
      } else {
        onCreated(created);
      }
      toast.success("已添加");
      onClose();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`登录失败：${msg}`);
    } finally {
      setOauthPendingId(null);
      setOauthStatus(null);
      setSubmitting(false);
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
    setSubmitting(false);
  };

  return (
    <AnimatePresence>
      <motion.div
        key="backdrop"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-[80] flex items-center justify-center bg-background/60 p-4 backdrop-blur-sm"
        onClick={onClose}
      >
        <motion.div
          key="dialog"
          initial={{ opacity: 0, scale: 0.95, y: 12 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.95, y: 12 }}
          transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          className="relative flex max-h-[min(88vh,640px)] w-full max-w-lg flex-col overflow-hidden rounded-2xl border border-border bg-card shadow-2xl"
          onClick={(e) => e.stopPropagation()}
        >
          <header className="flex shrink-0 items-center justify-between border-b border-border px-4 py-2.5">
            <h2 className="text-sm font-semibold">{title}</h2>
            <button
              type="button"
              className="text-muted-foreground hover:text-foreground"
              onClick={onClose}
              aria-label="关闭"
            >
              <X className="h-4 w-4" />
            </button>
          </header>

          <div className="min-h-0 flex-1 space-y-3 overflow-y-auto px-4 py-3">
            {isCreate ? (
              <Field label="供应商">
                <select value={catalogId} onChange={(e) => setCatalogId(e.target.value)} className={selectClass}>
                  <option value="">选择供应商…</option>
                  {catalog.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.display_name}
                    </option>
                  ))}
                </select>
                {selectedEntry && (
                  <div className="mt-1.5 flex items-center gap-2 rounded-md bg-muted/30 px-2 py-1.5">
                    <ProviderLogo
                      catalogId={selectedEntry.id}
                      displayName={selectedEntry.display_name}
                      brandColor={selectedEntry.brand_color}
                      size="sm"
                    />
                    <span className="text-[11px] text-muted-foreground">{selectedEntry.description}</span>
                  </div>
                )}
              </Field>
            ) : (
              selectedEntry && (
                <div className="flex items-center gap-2 rounded-lg border border-border/50 bg-muted/20 px-2.5 py-2">
                  <ProviderLogo
                    catalogId={selectedEntry.id}
                    displayName={selectedEntry.display_name}
                    brandColor={selectedEntry.brand_color}
                    size="sm"
                  />
                  <span className="text-sm font-medium">{editing?.display_name}</span>
                </div>
              )
            )}

            {selectedEntry?.warning && (
              <p className="rounded-md border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-300">
                ⚠ {selectedEntry.warning}
              </p>
            )}

            {selectedEntry && selectedEntry.auth_modes.length > 1 && (
              <Field label="认证方式">
                <div className="flex gap-1">
                  {selectedEntry.auth_modes.map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => setAuthMode(mode)}
                      className={cn(
                        "flex-1 rounded-md border px-2 py-1 text-[11px] font-medium transition-colors",
                        authMode === mode
                          ? "border-primary/50 bg-primary/10 text-foreground"
                          : "border-border/60 text-muted-foreground hover:text-foreground",
                      )}
                    >
                      {labelAuthMode(mode)}
                    </button>
                  ))}
                </div>
              </Field>
            )}

            {authMode === "o-auth" && selectedEntry?.regions.length ? (
              <Field label="区域">
                <div className="flex gap-1">
                  {selectedEntry.regions.map((r) => (
                    <button
                      key={r}
                      type="button"
                      onClick={() => setRegion(r)}
                      className={cn(
                        "rounded-md border px-2 py-1 text-[11px] uppercase",
                        region === r
                          ? "border-primary/50 bg-primary/10"
                          : "border-border/60 text-muted-foreground hover:text-foreground",
                      )}
                    >
                      {r}
                    </button>
                  ))}
                </div>
              </Field>
            ) : null}

            <Section title="基本信息">
              <div className="grid grid-cols-2 gap-2">
                <Field label="显示名（可选）">
                  <Input
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                    placeholder={selectedEntry?.display_name ?? ""}
                    className="h-8 text-sm"
                  />
                </Field>
                {authMode === "manual" ? (
                  <Field label="套餐">
                    <Input
                      value={planTier}
                      onChange={(e) => setPlanTier(e.target.value)}
                      placeholder="Pro / Max"
                      className="h-8 text-sm"
                    />
                  </Field>
                ) : (
                  <Field label="套餐">
                    <Input
                      value={planTier}
                      onChange={(e) => setPlanTier(e.target.value)}
                      placeholder="同步后覆盖"
                      disabled
                      className="h-8 text-sm"
                    />
                  </Field>
                )}
              </div>
            </Section>

            <Section title="付费与期限">
              <div className="grid grid-cols-2 gap-2">
                <Field label={priceLabel}>
                  <Input
                    type="number"
                    step="0.01"
                    min="0"
                    value={price}
                    onChange={(e) => setPrice(e.target.value)}
                    placeholder={billingCycle === "annual" ? "例如 1920" : "例如 20"}
                    className="h-8 text-sm"
                  />
                </Field>
                <Field label="币种">
                  <select value={currency} onChange={(e) => setCurrency(e.target.value)} className={selectClass}>
                    <option value="CNY">CNY ¥</option>
                    <option value="USD">USD $</option>
                  </select>
                </Field>
                <Field label="计费周期">
                  <select
                    value={billingCycle}
                    onChange={(e) => setBillingCycle(e.target.value as BillingCycle)}
                    className={selectClass}
                  >
                    <option value="monthly">按月</option>
                    <option value="annual">按年</option>
                    <option value="one-time">一次性</option>
                  </select>
                </Field>
                <Field label=" " className="hidden sm:block">
                  <p className="flex h-9 items-center text-[10px] leading-snug text-muted-foreground">
                    {billingCycle === "annual"
                      ? "年费填整年实付金额"
                      : billingCycle === "one-time"
                        ? "一次性付清总额"
                        : "月费填每月实付"}
                  </p>
                </Field>
                <Field label="起始日期">
                  <Input
                    type="date"
                    value={startDate}
                    onChange={(e) => setStartDate(e.target.value)}
                    className="h-8 text-sm"
                  />
                </Field>
                <Field label={endDateLabel}>
                  <Input
                    type="date"
                    value={endDate}
                    onChange={(e) => setEndDate(e.target.value)}
                    className="h-8 text-sm"
                  />
                </Field>
              </div>
              <label className="mt-1 flex items-center gap-2 text-[11px] text-muted-foreground">
                <input
                  type="checkbox"
                  checked={autoRenew}
                  onChange={(e) => setAutoRenew(e.target.checked)}
                  className="h-3.5 w-3.5 rounded border-border"
                />
                自动续费（勾选后「{endDateLabel}」表示下次扣款日）
              </label>
            </Section>

            {authMode === "api-key" && (
              <Field
                label={editing ? "API Key（留空保持现状）" : "API Key"}
                hint={selectedEntry?.id === "glm" ? "智谱 GLM 直接粘贴 token，无需 Bearer" : undefined}
              >
                <div className="relative">
                  <Input
                    value={apiKey}
                    onChange={(e) => setApiKey(e.target.value)}
                    placeholder="sk-..."
                    type={showKey ? "text" : "password"}
                    autoComplete="off"
                    className="h-8 pr-9 text-sm"
                  />
                  <button
                    type="button"
                    className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                    onClick={() => setShowKey((v) => !v)}
                  >
                    {showKey ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
                  </button>
                </div>
              </Field>
            )}

            {authMode === "manual" && (
              <details className="rounded-lg border border-dashed border-border/60">
                <summary className="flex cursor-pointer list-none items-center justify-between px-3 py-2 text-[11px] font-medium text-muted-foreground">
                  <span>手动用量（可选）</span>
                  <ChevronDown className="h-3.5 w-3.5" />
                </summary>
                <div className="space-y-2 border-t border-border/40 px-3 pb-3 pt-2">
                  <div className="grid grid-cols-2 gap-2">
                    <Field label="总 tokens">
                      <Input
                        type="number"
                        value={totalTokens}
                        onChange={(e) => setTotalTokens(e.target.value)}
                        placeholder="1000000"
                        className="h-8 text-sm"
                      />
                    </Field>
                    <Field label="已用 tokens">
                      <Input
                        type="number"
                        value={usedTokens}
                        onChange={(e) => setUsedTokens(e.target.value)}
                        placeholder="120000"
                        className="h-8 text-sm"
                      />
                    </Field>
                  </div>
                  <Field label="窗口标签">
                    <Input
                      value={periodLabel}
                      onChange={(e) => setPeriodLabel(e.target.value)}
                      placeholder="本月 / 5h / 周"
                      className="h-8 text-sm"
                    />
                  </Field>
                </div>
              </details>
            )}

            <Field label="备注（可选）">
              <Input
                value={note}
                onChange={(e) => setNote(e.target.value)}
                placeholder="可选"
                className="h-8 text-sm"
              />
            </Field>
          </div>

          {oauthStatus && (
            <div className="flex shrink-0 items-center gap-2 border-t border-border bg-blue-500/5 px-4 py-2 text-[11px] text-blue-300">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              <span className="flex-1">{oauthStatus}</span>
              {oauthPendingId && (
                <button type="button" className="text-blue-300 underline hover:text-blue-200" onClick={cancelOAuth}>
                  取消登录
                </button>
              )}
            </div>
          )}

          <footer className="flex shrink-0 justify-end gap-2 border-t border-border px-4 py-2.5">
            <Button variant="ghost" size="sm" onClick={onClose} disabled={submitting}>
              取消
            </Button>
            {isCreate && authMode === "o-auth" ? (
              <Button size="sm" onClick={startOAuthFlow} disabled={submitting || !catalogId}>
                <ExternalLink className="h-3.5 w-3.5" />
                {submitting ? "等待登录…" : "用浏览器登录"}
              </Button>
            ) : (
              <Button size="sm" onClick={submit} disabled={submitting}>
                {submitting ? "保存中…" : isCreate ? "添加" : "保存"}
              </Button>
            )}
          </footer>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-2">
      <h3 className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/80">{title}</h3>
      {children}
    </section>
  );
}

function priceLabelForCycle(cycle: BillingCycle): string {
  switch (cycle) {
    case "annual":
      return "年费";
    case "one-time":
      return "金额";
    default:
      return "月费";
  }
}

function labelAuthMode(mode: AuthMode): string {
  switch (mode) {
    case "api-key":
      return "API Key";
    case "o-auth":
      return "OAuth";
    case "manual":
      return "手动";
  }
}

function Field({
  label,
  hint,
  className,
  children,
}: {
  label: string;
  hint?: string;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <div className={cn("space-y-1", className)}>
      {label.trim() ? (
        <label className="block text-[10px] font-medium text-muted-foreground">{label}</label>
      ) : null}
      {children}
      {hint && <p className="text-[10px] text-muted-foreground/80">{hint}</p>}
    </div>
  );
}

function toDateInput(epoch: number | undefined | null): string {
  if (!epoch || epoch <= 0) return "";
  const d = new Date(epoch * 1000);
  const y = d.getFullYear();
  const m = `${d.getMonth() + 1}`.padStart(2, "0");
  const day = `${d.getDate()}`.padStart(2, "0");
  return `${y}-${m}-${day}`;
}

function parseDateInput(s: string): number {
  if (!s) return 0;
  const d = new Date(`${s}T00:00:00`);
  if (Number.isNaN(d.getTime())) return 0;
  return Math.floor(d.getTime() / 1000);
}

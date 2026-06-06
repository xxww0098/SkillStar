import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { FingerprintPicker } from "@/features/fingerprints";
import type { AuthMode, BillingCycle, CatalogEntry } from "../../types";
import { Field, selectClass } from "./fields";

interface AdvancedBillingSectionProps {
  selectedEntry: CatalogEntry | null;
  authMode: AuthMode;
  submitting: boolean;
  displayName: string;
  setDisplayName: (value: string) => void;
  planTier: string;
  setPlanTier: (value: string) => void;
  fingerprintId: string | null;
  setFingerprintId: (value: string | null) => void;
  billingCycleOptions: BillingCycle[];
  billingCycle: BillingCycle;
  setBillingCycle: (value: BillingCycle) => void;
  labelBillingCycle: (cycle: BillingCycle) => string;
  billingCycleHint: string;
  priceLabel: string;
  price: string;
  setPrice: (value: string) => void;
  currency: string;
  setCurrency: (value: string) => void;
  startDate: string;
  setStartDate: (value: string) => void;
  endDate: string;
  setEndDate: (value: string) => void;
  endDateLabel: string;
  autoRenew: boolean;
  setAutoRenew: (value: boolean) => void;
  periodLabel: string;
  setPeriodLabel: (value: string) => void;
  note: string;
  setNote: (value: string) => void;
}

export function AdvancedBillingSection({
  selectedEntry,
  authMode,
  submitting,
  displayName,
  setDisplayName,
  planTier,
  setPlanTier,
  fingerprintId,
  setFingerprintId,
  billingCycleOptions,
  billingCycle,
  setBillingCycle,
  labelBillingCycle,
  billingCycleHint,
  priceLabel,
  price,
  setPrice,
  currency,
  setCurrency,
  startDate,
  setStartDate,
  endDate,
  setEndDate,
  endDateLabel,
  autoRenew,
  setAutoRenew,
  periodLabel,
  setPeriodLabel,
  note,
  setNote,
}: AdvancedBillingSectionProps) {
  const { t } = useTranslation();
  return (
    <div className="mt-4 space-y-4 border-t border-border pt-4 animate-in fade-in duration-200">
      <div className="grid grid-cols-2 gap-3">
        <Field label={t("usage.fieldDisplayName")}>
          <Input
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder={selectedEntry?.display_name ?? ""}
            className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground"
          />
        </Field>
        <Field label={t("usage.fieldPlan")}>
          <Input
            value={planTier}
            onChange={(e) => setPlanTier(e.target.value)}
            placeholder={authMode === "manual" ? "Pro / Max" : t("usage.planAutoSync")}
            disabled={authMode !== "manual"}
            className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground"
          />
        </Field>
      </div>

      <div className="rounded-2xl border border-border bg-muted/30 p-4">
        <FingerprintPicker value={fingerprintId} onChange={setFingerprintId} disabled={submitting} />
      </div>

      {authMode !== "api-key" && (
        <div className="grid grid-cols-2 gap-3 rounded-2xl border border-border bg-muted/30 p-4">
          <h4 className="col-span-2 text-[9px] font-bold uppercase tracking-wider text-muted-foreground">
            💳 {t("usage.sectionBilling")}
          </h4>
          <Field label={t("usage.fieldBillingCycle")} className="col-span-2">
            <div
              className="flex gap-1 rounded-full border border-border bg-muted/50 p-1"
              role="group"
              aria-label={t("usage.fieldBillingCycle")}
            >
              {billingCycleOptions.map((cycle) => (
                <button
                  key={cycle}
                  type="button"
                  onClick={() => setBillingCycle(cycle)}
                  aria-pressed={billingCycle === cycle}
                  title={
                    cycle === "annual"
                      ? t("usage.cycleAnnual")
                      : cycle === "one-time"
                        ? t("usage.cycleOneTime")
                        : t("usage.cycleMonthly")
                  }
                  className={cn(
                    "flex-1 rounded-full px-3 py-1.5 text-[11px] font-semibold transition-all duration-200",
                    billingCycle === cycle
                      ? "bg-background text-foreground shadow-sm ring-1 ring-border/80"
                      : "text-muted-foreground hover:text-foreground",
                  )}
                >
                  {labelBillingCycle(cycle)}
                </button>
              ))}
            </div>
            <p className="mt-1.5 px-0.5 text-[9px] leading-snug text-muted-foreground">{billingCycleHint}</p>
          </Field>
          <Field label={priceLabel}>
            <Input
              type="number"
              step="0.01"
              min="0"
              value={price}
              onChange={(e) => setPrice(e.target.value)}
              placeholder={
                billingCycle === "annual"
                  ? t("usage.pricePlaceholderAnnual")
                  : billingCycle === "one-time"
                    ? t("usage.priceOneTime")
                    : t("usage.pricePlaceholderMonthly")
              }
              className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground"
            />
          </Field>
          <Field label={t("usage.fieldCurrency")}>
            <select value={currency} onChange={(e) => setCurrency(e.target.value)} className={selectClass}>
              <option value="CNY">CNY ¥</option>
              <option value="USD">USD $</option>
            </select>
          </Field>
          <Field label={t("usage.fieldStartDate")}>
            <Input
              type="date"
              value={startDate}
              onChange={(e) => setStartDate(e.target.value)}
              className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground"
            />
          </Field>
          <Field label={endDateLabel}>
            <Input
              type="date"
              value={endDate}
              onChange={(e) => setEndDate(e.target.value)}
              className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground"
            />
          </Field>
          <label className="col-span-2 mt-1.5 flex items-center gap-2 text-[10px] text-muted-foreground">
            <input
              type="checkbox"
              checked={autoRenew}
              onChange={(e) => setAutoRenew(e.target.checked)}
              className="h-3.5 w-3.5 rounded border-input-border bg-input"
            />
            {t("usage.autoRenewLabel", { label: endDateLabel })}
          </label>
        </div>
      )}

      {authMode === "manual" && (
        <Field label={t("usage.fieldPeriodLabel")}>
          <Input
            value={periodLabel}
            onChange={(e) => setPeriodLabel(e.target.value)}
            placeholder={t("usage.periodLabelPlaceholder")}
            className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground"
          />
        </Field>
      )}

      <Field label={t("usage.fieldNote")}>
        <Input
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder={t("common.optional")}
          className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground"
        />
      </Field>
    </div>
  );
}

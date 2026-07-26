import { BadgeCheck, GripVertical } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { BrandTheme } from "../../lib/brandThemes";
import type { BillingCycle } from "../../types";
import { PlanBadge } from "../PlanBadge";
import { hasBrandIcon, ProviderLogo } from "../ProviderLogo";

export interface UsageCardHeaderProps {
  catalogId: string;
  displayName: string;
  brandColorHex: string;
  theme: BrandTheme;
  planName: string | null;
  /** Billing type chip: 月付 / 年付 / API Key / 一次性 */
  billingCycle?: BillingCycle;
  /** Active account for this catalog — shown here so MetaStrip height stays stable. */
  isActive?: boolean;
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
}

/** Signature brand band — logo chip + title + billing type + active + plan badge + drag handle. */
export function UsageCardHeader({
  catalogId,
  displayName,
  brandColorHex,
  theme,
  planName,
  billingCycle,
  isActive,
  onDragHandlePointerDown,
}: UsageCardHeaderProps) {
  const { t } = useTranslation();
  const brandIcon = hasBrandIcon(catalogId);
  const billingLabel = billingCycle ? t(`usage.billingCycle_${billingCycle}`) : null;

  return (
    <div
      className="relative overflow-hidden px-4 pt-4 pb-3.5"
      style={{ background: `linear-gradient(135deg, ${theme.header[0]}, ${theme.header[1]})`, color: theme.fg }}
    >
      <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-white/30" />
      <div className="pointer-events-none absolute -top-10 -right-8 h-28 w-28 rounded-full bg-white/15 blur-2xl transition-transform duration-500 group-hover:scale-125" />

      <div className="relative flex items-start gap-3" style={{ textShadow: "0 1px 2px rgba(0,0,0,0.18)" }}>
        {brandIcon ? (
          // text-zinc-900 fixes mono logos that inherit band white fg on the white chip.
          <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-white text-zinc-900 shadow-[0_2px_8px_rgba(0,0,0,0.18)] ring-1 ring-black/5 [text-shadow:none]">
            <ProviderLogo catalogId={catalogId} displayName={displayName} brandColor={brandColorHex} size="md" />
          </div>
        ) : (
          <ProviderLogo
            catalogId={catalogId}
            displayName={displayName}
            brandColor={brandColorHex}
            size="lg"
            className="shrink-0 shadow-[0_2px_8px_rgba(0,0,0,0.2)] ring-1 ring-white/30"
          />
        )}
        <div className="min-w-0 flex-1">
          <h3 className="line-clamp-2 pr-1 text-sm leading-snug font-bold" title={displayName}>
            {displayName}
          </h3>
          {/* Fixed-height chip row (nowrap). Active badge lives here — not MetaStrip —
              so becoming "当前" never wraps meta or grows card height. */}
          <div className="mt-1 flex h-[18px] items-center gap-1.5 overflow-hidden">
            {billingLabel && (
              <span
                className="shrink-0 rounded-md bg-black/25 px-1.5 py-0.5 text-[10px] leading-none font-bold tracking-wide ring-1 ring-white/25 backdrop-blur-[2px]"
                title={billingLabel}
              >
                {billingLabel}
              </span>
            )}
            {isActive && (
              <span
                className="inline-flex shrink-0 items-center gap-0.5 rounded-md bg-emerald-400/25 px-1.5 py-0.5 text-[10px] leading-none font-bold tracking-wide text-emerald-50 ring-1 ring-emerald-200/50 backdrop-blur-[2px]"
                title={t("usage.cardActiveTitle")}
              >
                <BadgeCheck className="h-2.5 w-2.5" />
                {t("usage.cardActive")}
              </span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1 self-start">
          <PlanBadge plan={planName} variant="onBrand" />
          <button
            type="button"
            onPointerDown={onDragHandlePointerDown}
            className={cn(
              "cursor-grab text-current/70 hover:text-current active:cursor-grabbing",
              onDragHandlePointerDown ? "opacity-70 group-hover:opacity-100" : "opacity-0 group-hover:opacity-100",
            )}
            aria-label={t("usage.dragHandle")}
            tabIndex={-1}
          >
            <GripVertical className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
}

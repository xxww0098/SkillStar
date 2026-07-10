import { GripVertical } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { BrandTheme } from "../../lib/brandThemes";
import type { BillingCycle } from "../../types";
import { PlanBadge } from "../PlanBadge";
import { hasBrandIcon, ProviderLogo } from "../ProviderLogo";

export interface UsageCardHeaderProps {
  catalogId: string;
  displayName: string;
  description?: string | null;
  brandColorHex: string;
  theme: BrandTheme;
  planName: string | null;
  /** Billing type chip: 月付 / 年付 / API Key / 一次性 */
  billingCycle?: BillingCycle;
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
}

/** Signature brand band — logo chip + title + billing type + plan badge + drag handle. */
export function UsageCardHeader({
  catalogId,
  displayName,
  description,
  brandColorHex,
  theme,
  planName,
  billingCycle,
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
          <div className="mt-1 flex flex-wrap items-center gap-1.5">
            {billingLabel && (
              <span
                className="rounded-md bg-black/25 px-1.5 py-0.5 text-[10px] font-bold tracking-wide ring-1 ring-white/25 backdrop-blur-[2px]"
                title={billingLabel}
              >
                {billingLabel}
              </span>
            )}
            {description && (
              <p
                className="line-clamp-1 min-w-0 flex-1 text-[10px] leading-snug break-words opacity-90"
                title={description}
              >
                {description}
              </p>
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

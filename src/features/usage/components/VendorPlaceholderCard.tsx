import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { getBrandTheme } from "../lib/brandThemes";
import type { CatalogEntry } from "../types";
import { brandThemeToCssVars, usageCardShellClassName } from "./card";
import { hasBrandIcon, ProviderLogo } from "./ProviderLogo";

interface VendorPlaceholderCardProps {
  entry: CatalogEntry;
  onClick: () => void;
}

/**
 * Single-provider bind prompt — only shown when filtering to an unbound catalog entry.
 *
 * **Intentional visual change (PR5):** uses the same shell tokens + signature band
 * as SubscriptionCard (`getBrandTheme` + `usageCardShellClassName`), not the old
 * ProviderCatalogHero inline strip.
 */
export function VendorPlaceholderCard({ entry, onClick }: VendorPlaceholderCardProps) {
  const { t } = useTranslation();
  const theme = getBrandTheme(entry.id, entry.brand_color);
  const brandIcon = hasBrandIcon(entry.id);

  return (
    <button
      type="button"
      onClick={onClick}
      style={brandThemeToCssVars(theme)}
      className={usageCardShellClassName({
        className: cn("cursor-pointer text-left select-none transition-transform", "hover:-translate-y-0.5"),
      })}
    >
      {/* Signature band — same language as UsageCardHeader */}
      <div
        className="relative overflow-hidden px-4 pt-4 pb-3.5"
        style={{
          background: `linear-gradient(135deg, ${theme.header[0]}, ${theme.header[1]})`,
          color: theme.fg,
        }}
      >
        <div className="pointer-events-none absolute inset-x-0 top-0 h-px bg-white/30" />
        <div className="pointer-events-none absolute -top-10 -right-8 h-28 w-28 rounded-full bg-white/15 blur-2xl transition-transform duration-500 group-hover:scale-125" />

        <div className="relative flex items-start gap-3" style={{ textShadow: "0 1px 2px rgba(0,0,0,0.18)" }}>
          {brandIcon ? (
            <div className="grid h-9 w-9 shrink-0 place-items-center rounded-xl bg-white text-zinc-900 shadow-[0_2px_8px_rgba(0,0,0,0.18)] ring-1 ring-black/5 [text-shadow:none]">
              <ProviderLogo
                catalogId={entry.id}
                displayName={entry.display_name}
                brandColor={entry.brand_color}
                size="md"
              />
            </div>
          ) : (
            <ProviderLogo
              catalogId={entry.id}
              displayName={entry.display_name}
              brandColor={entry.brand_color}
              size="lg"
              className="shrink-0 shadow-[0_2px_8px_rgba(0,0,0,0.2)] ring-1 ring-white/30"
            />
          )}
          <div className="min-w-0 flex-1">
            <h3 className="line-clamp-2 pr-1 text-sm leading-snug font-bold">{entry.display_name}</h3>
          </div>
          <p className="shrink-0 text-[9px] font-mono text-current/70 tabular-nums [text-shadow:none]">
            {t("usage.emptyUsageNotSynced")}
          </p>
        </div>
      </div>

      {/* Ghost meta strip */}
      <div className="space-y-1.5 px-4 pt-2.5">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 text-[9px] font-semibold tracking-wider text-zinc-500 uppercase ring-1 ring-zinc-200/60">
            {entry.auth_modes[0] === "api-key" ? t("usage.authBadgeApiKey") : t("usage.authBadgeOAuth")}
          </span>
        </div>
      </div>

      <div className="relative z-10 flex-1 space-y-3.5 overflow-hidden px-4 pt-3 pb-2">
        <div className="space-y-2 rounded-2xl border border-dashed border-zinc-200 bg-zinc-50/40 p-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[11px] font-bold text-zinc-400">{t("usage.emptyUsageWindowName")}</span>
            <span className="rounded-md bg-zinc-100 px-1.5 py-0.5 font-mono text-[9px] font-bold text-zinc-400">
              —%
            </span>
          </div>
          <div className="flex items-baseline gap-1.5 py-0.5">
            <span className="font-mono text-lg leading-none font-bold text-zinc-300">—</span>
            <span className="text-[10px] text-zinc-300">/</span>
            <span className="font-mono text-[11px] font-semibold text-zinc-400">—</span>
            <span className="ml-auto text-[10px] font-medium text-zinc-400">{t("usage.used")}</span>
          </div>
          <div className="h-2 w-full overflow-hidden rounded-full bg-zinc-100 ring-1 ring-zinc-200/20">
            <div className="h-full w-0 rounded-full bg-zinc-200" />
          </div>
        </div>

        <div className="flex items-start gap-2 rounded-xl border border-dashed border-zinc-200 bg-zinc-50/50 p-2.5 group-hover:border-zinc-300">
          <div className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-full border border-zinc-200/60 bg-zinc-100 text-[9px] font-bold text-zinc-400">
            i
          </div>
          <p className="text-[10px] leading-relaxed text-zinc-400">{entry.warning ?? t("usage.emptyUsageTip")}</p>
        </div>
      </div>

      <footer className="relative z-10 flex flex-col gap-2.5 border-t border-zinc-100 bg-zinc-50/50 px-4 py-3">
        <div className="grid grid-cols-2 gap-2.5 text-[10px]">
          <div className="min-w-0 rounded-xl border border-zinc-200/40 bg-zinc-100/60 px-2.5 py-2">
            <p className="mb-1 text-[10px] whitespace-nowrap text-zinc-500">{t("usage.subscriptionCost")}</p>
            <p className="text-[11px] font-bold whitespace-nowrap text-zinc-400 tabular-nums">
              —<span className="ml-0.5 text-[9px] font-normal">{t("usage.perMonth")}</span>
            </p>
          </div>
          <div className="min-w-0 rounded-xl border border-zinc-200/40 bg-zinc-100/60 px-2.5 py-2">
            <p className="mb-1 text-[10px] whitespace-nowrap text-zinc-500">{t("usage.nextRenew")}</p>
            <div className="text-[11px] font-bold text-zinc-400">—</div>
          </div>
        </div>
        {/* Static CTA text only — whole card is the button (no nested focusables). */}
        <div className="inline-flex h-7 shrink-0 items-center justify-center self-end rounded-lg bg-zinc-900 px-2.5 text-[10px] font-semibold text-white shadow-sm group-hover:bg-zinc-800">
          {t("usage.bindNow")}
        </div>
      </footer>
    </button>
  );
}

import { useTranslation } from "react-i18next";
import { getBrandTheme } from "../lib/brandThemes";
import type { CatalogEntry } from "../types";
import { brandThemeToCssVars, usageCardShellClassName, usageCardSlotClassName } from "./card";
import { hasBrandIcon, ProviderLogo } from "./ProviderLogo";

interface VendorPlaceholderCardProps {
  entry: CatalogEntry;
  onClick: () => void;
}

/**
 * Single-provider bind prompt — only shown when filtering to an unbound catalog entry.
 * Same shell + identity band as SubscriptionCard; no ghost badges or empty KPI tiles.
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
        className: "max-w-[280px] cursor-pointer text-left select-none",
      })}
      aria-label={`${entry.display_name}. ${t("usage.bindNow")}`}
    >
      <div
        className={usageCardSlotClassName.headerBand}
        style={{
          background: `linear-gradient(135deg, ${theme.header[0]}, ${theme.header[1]})`,
          color: theme.fg,
        }}
      >
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
          <h3 className="min-w-0 flex-1 line-clamp-2 text-sm leading-snug font-bold">{entry.display_name}</h3>
        </div>
      </div>

      <div className={usageCardSlotClassName.body}>
        <div className="space-y-2">
          <div className="flex items-baseline justify-between gap-2">
            <span className="text-[11px] font-semibold text-zinc-500">{t("usage.emptyUsageWindowName")}</span>
            <span className="font-mono text-[11px] tabular-nums text-zinc-400">—</span>
          </div>
          <div className="relative h-1.5 w-full overflow-hidden rounded-full bg-zinc-100" aria-hidden>
            <div className="h-full w-0 rounded-full bg-zinc-200" />
          </div>
          <p className="text-[11px] leading-relaxed text-zinc-600">{entry.warning ?? t("usage.emptyUsageTip")}</p>
        </div>
      </div>

      <footer className={usageCardSlotClassName.footer}>
        <div className="inline-flex h-7 shrink-0 items-center justify-center self-end rounded-lg bg-zinc-900 px-2.5 text-[11px] font-semibold text-white group-hover:bg-zinc-800">
          {t("usage.bindNow")}
        </div>
      </footer>
    </button>
  );
}

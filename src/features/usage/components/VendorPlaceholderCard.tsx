import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import type { CatalogEntry } from "../types";
import { catalogBrandVars, ProviderCatalogHero } from "./ProviderCatalogHero";

interface VendorPlaceholderCardProps {
  entry: CatalogEntry;
  onClick: () => void;
}

/** Single-provider bind prompt — only shown when filtering to an unbound catalog entry. */
export function VendorPlaceholderCard({ entry, onClick }: VendorPlaceholderCardProps) {
  const { t } = useTranslation();

  return (
    <button
      type="button"
      onClick={onClick}
      style={catalogBrandVars(entry.brand_color)}
      className={cn(
        "group relative flex w-full flex-col overflow-hidden rounded-3xl border bg-white/95 text-left backdrop-blur-xl select-none sm:w-[280px]",
        "min-h-[320px] shrink-0 border-zinc-200/80 shadow-[0_8px_30px_rgba(0,0,0,0.03)] transition-all duration-300",
        "hover:-translate-y-0.5 hover:border-zinc-300 hover:shadow-[0_8px_30px_rgba(var(--brand-rgb),0.08)] cursor-pointer",
      )}
    >
      <div
        className="pointer-events-none absolute -right-16 -top-16 h-36 w-36 rounded-full opacity-10 blur-[40px] transition-all duration-500 group-hover:scale-110 group-hover:opacity-15"
        style={{ backgroundColor: "rgb(var(--brand-rgb))" }}
      />

      <header className="relative z-10 p-4 pb-2">
        <ProviderCatalogHero
          entry={entry}
          variant="inline"
          trailing={
            <p className="shrink-0 text-[9px] font-mono tabular-nums text-zinc-400">{t("usage.emptyUsageNotSynced")}</p>
          }
        />
      </header>

      <div className="relative z-10 flex-1 space-y-3.5 overflow-hidden px-4 pb-2">
        <div className="space-y-2 rounded-2xl border border-dashed border-zinc-200 bg-zinc-50/40 p-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[11px] font-bold text-zinc-400">{t("usage.emptyUsageWindowName")}</span>
            <span className="rounded-md bg-zinc-100 px-1.5 py-0.5 text-[9px] font-bold font-mono text-zinc-400">
              —%
            </span>
          </div>
          <div className="flex items-baseline gap-1.5 py-0.5">
            <span className="font-mono text-lg font-bold leading-none text-zinc-300">—</span>
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
          <div className="rounded-xl border border-zinc-200/40 bg-zinc-100/60 px-2.5 py-2 min-w-0">
            <p className="mb-1 text-[10px] whitespace-nowrap text-zinc-500">{t("usage.subscriptionCost")}</p>
            <p className="whitespace-nowrap text-[11px] font-bold tabular-nums text-zinc-400">
              —<span className="ml-0.5 text-[9px] font-normal">/月</span>
            </p>
          </div>
          <div className="rounded-xl border border-zinc-200/40 bg-zinc-100/60 px-2.5 py-2 min-w-0">
            <p className="mb-1 text-[10px] whitespace-nowrap text-zinc-500">{t("usage.nextRenew")}</p>
            <div className="text-[11px] font-bold text-zinc-400">—</div>
          </div>
        </div>
        <div className="inline-flex h-7 shrink-0 items-center justify-center self-end rounded-lg bg-zinc-900 px-2.5 text-[10px] font-semibold text-white shadow-sm group-hover:bg-zinc-800">
          {t("usage.bindNow")}
        </div>
      </footer>
    </button>
  );
}

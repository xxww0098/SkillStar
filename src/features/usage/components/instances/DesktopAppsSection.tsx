import { ChevronDown } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ProviderLogo } from "../ProviderLogo";
import { cn } from "@/lib/utils";
import type { DesktopAppId } from "../../types";
import { AppInstancesPanel } from "./AppInstancesPanel";

const APP_META: Record<DesktopAppId, { catalogId: string; brand: string; labelKey: string }> = {
  cursor: { catalogId: "cursor", brand: "00E5BC", labelKey: "usage.desktopAppCursor" },
  "grok-bot": { catalogId: "grok-bot", brand: "18181B", labelKey: "usage.grokBotDesktop" },
  antigravity: { catalogId: "antigravity", brand: "4285F4", labelKey: "usage.desktopAppAntigravity" },
};

export function DesktopAppsSection({ appIds }: { appIds: DesktopAppId[] }) {
  const { t } = useTranslation();
  const single = appIds.length === 1;
  const [open, setOpen] = useState<Record<string, boolean>>(() => Object.fromEntries(appIds.map((id) => [id, single])));

  const items = useMemo(() => appIds.map((id) => ({ id, ...APP_META[id] })), [appIds]);

  return (
    <section className="mb-4 rounded-2xl border border-border/70 bg-card/60 px-3 py-3">
      <div className="mb-2">
        <h2 className="text-sm font-semibold text-foreground">{t("usage.desktopApps")}</h2>
        <p className="mt-0.5 text-[11px] text-muted-foreground">{t("usage.desktopAppsHint")}</p>
      </div>
      <div className="flex flex-col gap-2">
        {items.map((item) => {
          const expanded = open[item.id] ?? single;
          return (
            <div key={item.id} className="rounded-xl border border-border/60 bg-background/70">
              <button
                type="button"
                aria-expanded={expanded}
                onClick={() => setOpen((current) => ({ ...current, [item.id]: !expanded }))}
                className="flex w-full items-center gap-2 px-2.5 py-2 text-left"
              >
                <ProviderLogo
                  catalogId={item.catalogId}
                  displayName={t(item.labelKey)}
                  brandColor={item.brand}
                  size="sm"
                />
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">{t(item.labelKey)}</span>
                <ChevronDown
                  className={cn(
                    "h-4 w-4 shrink-0 text-muted-foreground transition-transform",
                    !expanded && "-rotate-90",
                  )}
                />
              </button>
              {expanded ? (
                <div className="border-t border-border/50 px-2.5 py-2">
                  <AppInstancesPanel appId={item.id} compact />
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}

import { Server } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { ProviderBrandIcon } from "@/components/shared/ProviderBrandIcon";
import { useModelsBoard } from "@/features/models";
import { cn } from "@/lib/utils";

export interface ModelsSidebarProps {
  collapsed: boolean;
  selectedProviderId: string | null;
  onSelectProvider: (id: string) => void;
}

/**
 * Models mode sidebar. Recent names come from the board (id + name only).
 * Adding a provider is not on this strip: the old create form is gone.
 */
export function ModelsSidebar({ collapsed, selectedProviderId, onSelectProvider }: ModelsSidebarProps) {
  const { data } = useModelsBoard();
  const { t } = useTranslation();
  const recent = useMemo(() => (data?.providers ?? []).slice(0, 6), [data?.providers]);

  if (collapsed) {
    return (
      <div className="flex flex-col items-center gap-1.5 py-2">
        {recent.map((provider) => (
          <button
            key={provider.id}
            type="button"
            onClick={() => onSelectProvider(provider.id)}
            title={provider.name}
            className={cn(
              "flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg border bg-background/60 transition hover:bg-card-hover shadow-2xs",
              selectedProviderId === provider.id
                ? "border-primary bg-primary/20 ring-1 ring-primary/40"
                : "border-border/80",
            )}
          >
            <ProviderBrandIcon
              providerName={provider.name}
              size="xs"
              className="h-5 w-5 border-0 bg-transparent shadow-none"
            />
          </button>
        ))}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3 py-1">
      <div className="rounded-2xl border border-border bg-card px-3 py-3">
        <div className="text-[10px] font-bold uppercase tracking-wider text-foreground/70">
          {t("models.sidebar.workbench")}
        </div>
        <p className="mt-1.5 text-[11px] leading-snug text-muted-foreground">{t("models.sidebar.intro")}</p>
      </div>

      {recent.length > 0 ? (
        <div className="space-y-1">
          <div className="flex items-center gap-1 px-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground/80">
            <Server className="h-3 w-3" />
            {t("models.sidebar.recent")}
          </div>
          <div className="space-y-0.5">
            {recent.map((provider) => {
              const active = selectedProviderId === provider.id;
              return (
                <button
                  key={provider.id}
                  type="button"
                  onClick={() => onSelectProvider(provider.id)}
                  className={cn(
                    "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs transition select-none",
                    active
                      ? "bg-primary/18 text-primary font-semibold ring-1 ring-primary/30 shadow-2xs dark:bg-primary/20"
                      : "text-muted-foreground hover:bg-muted/40 hover:text-foreground font-medium",
                  )}
                >
                  <ProviderBrandIcon providerName={provider.name} size="xs" />
                  <span className="min-w-0 flex-1 truncate">{provider.name}</span>
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}

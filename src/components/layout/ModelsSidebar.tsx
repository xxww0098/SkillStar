import { Plug, Server, Sparkles } from "lucide-react";
import { useProvidersFlat } from "@/features/models/hooks/useProvidersFlat";
import { ProviderBrandIcon } from "@/features/models/components/ProviderBrandIcon";
import { cn } from "@/lib/utils";

export interface ModelsSidebarProps {
  collapsed: boolean;
  selectedProviderId: string | null;
  onSelectProvider: (id: string) => void;
  onAddProvider: () => void;
}

/**
 * Minimal sidebar shown in Models mode. The full provider list and Agent
 * connections live in the main hub now — this strip is just a Recent /
 * pinned shortcut so users can jump to a provider quickly without leaving
 * the sidebar.
 */
export function ModelsSidebar({ collapsed, selectedProviderId, onSelectProvider, onAddProvider }: ModelsSidebarProps) {
  const { providers } = useProvidersFlat();
  const recent = providers.slice(0, 6);

  if (collapsed) {
    return (
      <div className="flex flex-col items-center gap-1.5 py-2">
        <button
          type="button"
          onClick={onAddProvider}
          title="新增供应商"
          className="flex h-9 w-9 cursor-pointer items-center justify-center rounded-xl border border-primary/25 bg-primary/10 text-primary transition hover:bg-primary/20"
        >
          <Plug className="h-4 w-4" />
        </button>
        {recent.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => onSelectProvider(p.id)}
            title={p.name}
            className={cn(
              "flex h-8 w-8 cursor-pointer items-center justify-center rounded-lg border bg-background/40 transition hover:bg-card-hover",
              selectedProviderId === p.id ? "border-primary/45 bg-primary/10" : "border-border/55",
            )}
          >
            <ProviderBrandIcon
              presetId={p.preset_id}
              providerName={p.name}
              iconColor={p.icon_color}
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
      <div className="rounded-xl border border-primary/15 bg-primary/[0.04] px-3 py-3">
        <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wider text-primary/90">
          <Sparkles className="h-3 w-3" />
          Models 工作台
        </div>
        <p className="mt-1.5 text-[11px] leading-snug text-muted-foreground">
          Agent 绑定、供应商管理与连接诊断现已统一到主面板。
        </p>
        <button
          type="button"
          onClick={onAddProvider}
          className="mt-2.5 flex w-full cursor-pointer items-center justify-center gap-1.5 rounded-lg bg-primary px-2.5 py-1.5 text-[11px] font-semibold text-primary-foreground transition hover:bg-primary/90"
        >
          <Plug className="h-3 w-3" />
          新增供应商
        </button>
      </div>

      {recent.length > 0 ? (
        <div className="space-y-1">
          <div className="flex items-center gap-1 px-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/70">
            <Server className="h-3 w-3" />
            最近
          </div>
          <div className="space-y-0.5">
            {recent.map((p) => {
              const active = selectedProviderId === p.id;
              return (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => onSelectProvider(p.id)}
                  className={cn(
                    "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs transition",
                    active
                      ? "bg-primary/10 text-primary"
                      : "text-muted-foreground hover:bg-muted/40 hover:text-foreground",
                  )}
                >
                  <ProviderBrandIcon presetId={p.preset_id} providerName={p.name} iconColor={p.icon_color} size="xs" />
                  <span className="min-w-0 flex-1 truncate font-medium">{p.name}</span>
                </button>
              );
            })}
          </div>
        </div>
      ) : null}
    </div>
  );
}

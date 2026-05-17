import { CheckCircle2 } from "lucide-react";
import { useCallback } from "react";
import type { ProviderPatchFlat } from "../../../types";
import { useProvidersFlat } from "../hooks/useProvidersFlat";
import { ConnectionStatusPanel } from "./ConnectionStatusPanel";
import { ConflictWarnings } from "./ConflictWarnings";
import { ProviderBrandIcon } from "./ProviderBrandIcon";
import { ProviderConfigForm } from "./ProviderConfigForm";

export interface ProviderDetailPanelProps {
  providerId: string | null;
}

/**
 * Main detail panel for a selected provider.
 *
 * Two-zone layout:
 * - Left/center: 配置区 (provider settings form)
 * - Right sidebar: 连接状态
 *
 * Shows an empty state when no provider is selected.
 */
export function ProviderDetailPanel({ providerId }: ProviderDetailPanelProps) {
  const { providers, updateProvider } = useProvidersFlat();

  const provider = providerId ? providers.find((p) => p.id === providerId) : undefined;

  const handleSave = useCallback(
    async (patch: ProviderPatchFlat) => {
      if (!providerId) return;
      await updateProvider(providerId, patch);
    },
    [providerId, updateProvider],
  );

  if (!providerId) {
    return (
      <main className="flex min-h-0 min-w-0 flex-1 flex-col items-center justify-center overflow-hidden">
        <div className="rounded-[22px] border border-border/60 bg-card/70 px-8 py-6 text-center shadow-[0_22px_70px_-46px_var(--color-shadow)] backdrop-blur-2xl">
          <p className="text-sm text-muted-foreground">选择一个供应商查看详情</p>
        </div>
      </main>
    );
  }

  if (!provider) {
    return (
      <main className="flex min-h-0 min-w-0 flex-1 flex-col items-center justify-center overflow-hidden">
        <div className="rounded-[22px] border border-border/60 bg-card/70 px-8 py-6 text-center shadow-[0_22px_70px_-46px_var(--color-shadow)] backdrop-blur-2xl">
          <p className="text-sm text-muted-foreground">供应商未找到</p>
        </div>
      </main>
    );
  }

  return (
    <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className="ss-floating-page-inner">
        <section className="flex min-w-0 flex-1 flex-col overflow-hidden">
          <div className="shrink-0 px-5 pb-4 pt-5 sm:px-7">
            <div className="flex items-center gap-3">
              <ProviderBrandIcon
                presetId={provider.preset_id}
                providerName={provider.name}
                iconColor={provider.icon_color}
                size="lg"
                className="bg-background/50"
              />
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-2">
                  <h2 className="truncate text-lg font-semibold leading-tight text-foreground">{provider.name}</h2>
                  <span className="inline-flex shrink-0 items-center gap-1 rounded-full border border-success/20 bg-success/10 px-2 py-0.5 text-[11px] font-medium text-success">
                    <CheckCircle2 className="h-3 w-3" />
                    已保存
                  </span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">编辑模型供应商配置</p>
              </div>
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5 sm:px-7">
            {/* Conflict warnings — env var overrides & legacy config */}
            <div className="mb-4">
              <ConflictWarnings providerId={provider.id} />
            </div>
            <ProviderConfigForm key={provider.id} provider={provider} onSave={handleSave} />
          </div>
        </section>

        <aside className="hidden w-[330px] shrink-0 overflow-y-auto border-l border-border/45 bg-background/20 px-5 py-5 lg:block">
          <ConnectionStatusPanel
            providerId={provider.id}
            presetId={provider.preset_id}
            apiKey={provider.api_key}
            baseUrl={provider.base_url_openai || provider.base_url_anthropic}
          />
        </aside>
      </div>
    </main>
  );
}

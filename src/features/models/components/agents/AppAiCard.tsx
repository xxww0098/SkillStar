import ClaudeIcon from "@lobehub/icons/es/Claude/components/Color";
import CodexIcon from "@lobehub/icons/es/Codex/components/Color";
import { motion } from "framer-motion";
import { ArrowRight, Eraser, Loader2, Sparkles } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { getAiConfigCached } from "../../../../hooks/useAiConfig";
import { cn } from "../../../../lib/utils";
import type { AiConfig } from "../../../../types";
import { type AppAiAppId, useAppAiProvider } from "../../api/appAi";
import { useProvidersQuery } from "../../api/providers";

export interface AppAiCardProps {
  /** Jump to Settings → AI provider (manages local Ollama etc.). */
  onOpenSettings: () => void;
}

/**
 * Compact in-app AI consumer card — the in-app AI (summarize / translate /
 * skill pick) is just another consumer of a provider, so it lives in the
 * agent grid. Replaces the old AppAiProviderInline that sat inside a single
 * provider's drawer. When Settings has local Ollama active this card defers.
 */
export function AppAiCard({ onOpenSettings }: AppAiCardProps) {
  const { t } = useTranslation();
  const { data } = useProvidersQuery();
  const { setAppAiProvider, clearAppAiProvider, isSetting, isClearing } = useAppAiProvider();
  const [config, setConfig] = useState<AiConfig | null>(null);

  const refresh = useCallback(async () => {
    try {
      setConfig(await getAiConfigCached());
    } catch {
      setConfig(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const providers = useMemo(() => {
    if (!data) return [];
    return [...data.providers].sort((a, b) => a.sort_index - b.sort_index);
  }, [data]);

  const isOllama = config?.api_format === "local";
  const ref = config?.provider_ref ?? null;
  const boundProvider = ref ? (providers.find((p) => p.id === ref.provider_id) ?? null) : null;
  const protocol: AppAiAppId | null = ref?.app_id === "claude" || ref?.app_id === "codex" ? ref.app_id : null;
  const bound = Boolean(boundProvider);

  const canClaude = (p: { base_url_anthropic?: string; base_url_openai?: string }) =>
    Boolean(p.base_url_anthropic?.trim() || p.base_url_openai?.trim());
  const canCodex = (p: { base_url_openai?: string }) => Boolean(p.base_url_openai?.trim());

  const handleBind = useCallback(
    async (appId: AppAiAppId, providerId: string, providerName?: string) => {
      try {
        await setAppAiProvider(appId, providerId, providerName);
      } catch {
        /* toast handled by the hook */
      }
      await refresh();
    },
    [setAppAiProvider, refresh],
  );

  return (
    <motion.section
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "relative flex h-full flex-col rounded-3xl border border-border/55 bg-card/65 backdrop-blur-2xl",
        "transition-transform duration-300 hover:-translate-y-0.5",
        "shadow-[0_24px_60px_-40px_var(--color-shadow)]",
      )}
    >
      <span
        aria-hidden
        className="absolute inset-x-0 top-0 h-[2px] bg-gradient-to-r from-primary/5 via-primary/25 to-primary/5"
      />

      <header className="flex items-start gap-3 px-5 pt-5">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg border border-primary/20 bg-primary/10">
          <Sparkles className="h-4 w-4 text-primary" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-base font-bold text-foreground">{t("models.appAi.title")}</h3>
            <span
              className={cn(
                "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ring-1",
                bound || isOllama
                  ? "bg-primary/15 text-primary ring-primary/25"
                  : "bg-muted text-muted-foreground ring-border",
              )}
            >
              {isOllama ? t("models.appAi.localOllama") : bound ? t("models.appAi.bound") : t("models.appAi.unbound")}
            </span>
          </div>
          <p className="mt-0.5 text-[11px] text-muted-foreground">{t("models.appAi.tagline")}</p>
        </div>
      </header>

      <div className="flex-1 space-y-3 px-5 pt-4 pb-3">
        {isOllama ? (
          <div className="rounded-xl border border-border/50 bg-background/35 px-3 py-2.5 text-[11px] text-muted-foreground">
            {t("models.appAi.ollamaActive")}
            <button
              type="button"
              onClick={onOpenSettings}
              className="ml-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
            >
              {t("models.appAi.switchInSettings")} <ArrowRight className="h-3 w-3" />
            </button>
          </div>
        ) : (
          <div className="space-y-2">
            {boundProvider ? (
              <p className="truncate text-xs text-foreground">
                <span className="font-medium">{boundProvider.name}</span>
                <span className="ml-1.5 text-[11px] text-muted-foreground">
                  · {protocol === "claude" ? t("models.appAi.claudeProtocol") : t("models.appAi.openaiProtocol")}
                </span>
              </p>
            ) : (
              <p className="text-[11px] text-muted-foreground">{t("models.appAi.pickHint")}</p>
            )}
            <div className="ss-page-scroll max-h-40 space-y-1 overflow-y-auto pr-0.5">
              {providers.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border/55 px-2.5 py-2 text-center text-[11px] text-muted-foreground">
                  {t("models.appAi.noProviders")}
                </p>
              ) : (
                providers.map((p) => {
                  const isBound = boundProvider?.id === p.id;
                  return (
                    <div
                      key={p.id}
                      className={cn(
                        "flex items-center gap-2 rounded-lg border px-2 py-1.5",
                        isBound ? "border-primary/35 bg-primary/[0.06]" : "border-border/45 bg-background/30",
                      )}
                    >
                      <span className="min-w-0 flex-1 truncate text-[11px] font-medium text-foreground">{p.name}</span>
                      <button
                        type="button"
                        disabled={!canClaude(p) || isSetting}
                        onClick={() => void handleBind("claude", p.id, p.name)}
                        title={t("models.appAi.bindClaudeTitle")}
                        className={cn(
                          "inline-flex h-6 items-center gap-1 rounded-md border px-1.5 text-[10px] font-medium transition-colors",
                          isBound && protocol === "claude"
                            ? "border-primary/50 bg-primary/10 text-primary"
                            : "border-border/55 text-muted-foreground hover:text-foreground",
                          !canClaude(p) && "pointer-events-none opacity-40",
                        )}
                      >
                        {isSetting ? <Loader2 className="h-2.5 w-2.5 animate-spin" /> : <ClaudeIcon size={11} />}
                        Claude
                      </button>
                      <button
                        type="button"
                        disabled={!canCodex(p) || isSetting}
                        onClick={() => void handleBind("codex", p.id, p.name)}
                        title={t("models.appAi.bindOpenaiTitle")}
                        className={cn(
                          "inline-flex h-6 items-center gap-1 rounded-md border px-1.5 text-[10px] font-medium transition-colors",
                          isBound && protocol === "codex"
                            ? "border-primary/50 bg-primary/10 text-primary"
                            : "border-border/55 text-muted-foreground hover:text-foreground",
                          !canCodex(p) && "pointer-events-none opacity-40",
                        )}
                      >
                        {isSetting ? <Loader2 className="h-2.5 w-2.5 animate-spin" /> : <CodexIcon size={11} />}
                        OpenAI
                      </button>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        )}
      </div>

      <footer className="flex items-center gap-1 border-t border-border/40 bg-background/20 px-4 py-2.5">
        {bound && !isOllama ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-7 gap-1.5 text-[11px] text-muted-foreground hover:text-destructive"
            disabled={isClearing}
            onClick={() => void clearAppAiProvider().then(refresh)}
          >
            <Eraser className="h-3 w-3" />
            {t("models.appAi.clearBinding")}
          </Button>
        ) : null}
        <Button
          variant="ghost"
          size="sm"
          className="ml-auto h-7 gap-1.5 text-[11px] text-muted-foreground"
          onClick={onOpenSettings}
        >
          {t("models.appAi.openSettings")} <ArrowRight className="h-3 w-3" />
        </Button>
      </footer>
    </motion.section>
  );
}

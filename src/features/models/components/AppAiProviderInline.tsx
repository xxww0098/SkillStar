import ClaudeIcon from "@lobehub/icons/es/Claude/components/Color";
import CodexIcon from "@lobehub/icons/es/Codex/components/Color";
import { Loader2, Sparkles } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { getAiConfigCached } from "../../../hooks/useAiConfig";
import { cn } from "../../../lib/utils";
import type { ProviderEntryFlat } from "../../../types";
import { useAppAiProvider } from "../api/appAi";
import { providerCardClass } from "./providerForm/ProviderConfigPrimitives";

export interface AppAiProviderInlineProps {
  provider: ProviderEntryFlat;
}

function AppAiProviderInlineInner({ provider }: AppAiProviderInlineProps) {
  const { setAppAiProvider, matchesProviderRef, isSetting } = useAppAiProvider();
  const [activeRef, setActiveRef] = useState<ReturnType<typeof matchesProviderRef>>(null);

  const refresh = useCallback(async () => {
    try {
      const cfg = await getAiConfigCached();
      setActiveRef(matchesProviderRef(cfg, provider.id));
    } catch {
      setActiveRef(null);
    }
  }, [matchesProviderRef, provider.id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const canClaude = Boolean(provider.base_url_anthropic?.trim() || provider.base_url_openai?.trim());
  const canCodex = Boolean(provider.base_url_openai?.trim());
  const isClaudeAi = activeRef?.app_id === "claude";
  const isCodexAi = activeRef?.app_id === "codex";

  return (
    <section className={cn(providerCardClass, "px-4 py-3")}>
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <Sparkles className="h-4 w-4 shrink-0 text-primary" />
          <div className="min-w-0">
            <p className="text-xs font-semibold text-foreground">应用内 AI</p>
            <p className="truncate text-[11px] text-muted-foreground">
              {isClaudeAi
                ? "已绑定 · Claude 协议"
                : isCodexAi
                  ? "已绑定 · OpenAI 协议"
                  : "摘要 / 翻译 / 推荐（与 CLI 工具独立）"}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap gap-1.5">
          <button
            type="button"
            disabled={!canClaude || isSetting}
            onClick={() => void setAppAiProvider("claude", provider.id, provider.name).then(refresh)}
            className={cn(
              "inline-flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium transition-colors",
              isClaudeAi
                ? "border-primary/50 bg-primary/10 text-primary"
                : "border-border/55 text-muted-foreground hover:border-border hover:text-foreground",
              !canClaude && "pointer-events-none opacity-40",
            )}
          >
            {isSetting ? <Loader2 className="h-3 w-3 animate-spin" /> : <ClaudeIcon size={14} />}
            Claude
          </button>
          <button
            type="button"
            disabled={!canCodex || isSetting}
            onClick={() => void setAppAiProvider("codex", provider.id, provider.name).then(refresh)}
            className={cn(
              "inline-flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium transition-colors",
              isCodexAi
                ? "border-primary/50 bg-primary/10 text-primary"
                : "border-border/55 text-muted-foreground hover:border-border hover:text-foreground",
              !canCodex && "pointer-events-none opacity-40",
            )}
          >
            {isSetting ? <Loader2 className="h-3 w-3 animate-spin" /> : <CodexIcon size={14} />}
            OpenAI
          </button>
        </div>
      </div>
    </section>
  );
}

export const AppAiProviderInline = memo(AppAiProviderInlineInner);

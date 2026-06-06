import ClaudeIcon from "@lobehub/icons/es/Claude/components/Color";
import CodexIcon from "@lobehub/icons/es/Codex/components/Color";
import { Loader2, Sparkles } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { getAiConfigCached } from "../../../hooks/useAiConfig";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { ProviderEntryFlat } from "../../../types";
import { useAppAiProvider } from "../hooks/useAppAiProvider";

export interface AppAiProviderCardProps {
  provider: ProviderEntryFlat;
}

function AppAiProviderCardInner({ provider }: AppAiProviderCardProps) {
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

  const handleSet = useCallback(
    async (appId: "claude" | "codex") => {
      await setAppAiProvider(appId, provider.id, provider.name);
      await refresh();
    },
    [setAppAiProvider, provider.id, provider.name, refresh],
  );

  return (
    <section className="mb-6 overflow-hidden rounded-xl border border-border/60 bg-card/55 shadow-sm backdrop-blur-sm">
      <div className="border-b border-border/45 px-4 py-3">
        <div className="flex items-center gap-2">
          <Sparkles className="h-4 w-4 text-primary" />
          <h3 className="text-sm font-semibold text-foreground">应用内 AI</h3>
        </div>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          用于技能摘要、翻译、推荐等应用内功能。与 Claude / Codex 工具激活相互独立。
        </p>
      </div>

      <div className="space-y-3 px-4 py-3">
        {(isClaudeAi || isCodexAi) && (
          <p className="rounded-lg border border-success/25 bg-success/5 px-3 py-2 text-xs text-success">
            当前已绑定为应用内 AI（{isClaudeAi ? "Claude / Anthropic" : "Codex / OpenAI"}）
          </p>
        )}

        <div className="grid gap-2 sm:grid-cols-2">
          <Button
            type="button"
            variant={isClaudeAi ? "default" : "outline"}
            size="sm"
            disabled={!canClaude || isSetting}
            onClick={() => handleSet("claude")}
            className={cn("h-9 justify-start gap-2", !canClaude && "opacity-50")}
          >
            {isSetting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ClaudeIcon size={16} />}
            用于 Claude 协议
          </Button>

          <Button
            type="button"
            variant={isCodexAi ? "default" : "outline"}
            size="sm"
            disabled={!canCodex || isSetting}
            onClick={() => handleSet("codex")}
            className={cn("h-9 justify-start gap-2", !canCodex && "opacity-50")}
          >
            {isSetting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <CodexIcon size={16} />}
            用于 OpenAI 协议
          </Button>
        </div>

        {!canClaude && !canCodex && <p className="text-xs text-amber-500">请先配置至少一个 API 端点</p>}
      </div>
    </section>
  );
}

export const AppAiProviderCard = memo(AppAiProviderCardInner);

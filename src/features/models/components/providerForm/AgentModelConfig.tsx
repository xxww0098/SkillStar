import ClaudeIcon from "@lobehub/icons/es/Claude/components/Color";
import { memo } from "react";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import { Input } from "../../../../components/ui/input";
import { cn } from "../../../../lib/utils";
import { fieldLabelClass } from "./ProviderConfigPrimitives";
import {
  LATEST_CLAUDE_MODELS,
  LATEST_CODEX_MODELS,
  type CodexWireApi,
  type ProviderFormState,
} from "./useProviderFormState";

export interface AgentModelConfigProps {
  form: ProviderFormState;
}

function AgentModelConfigInner({ form }: AgentModelConfigProps) {
  return (
    <div className="space-y-4">
      <div className="space-y-3">
        <div className="flex items-center gap-2">
          <span className="flex h-7 w-7 items-center justify-center rounded-lg border border-border/60 bg-background/70">
            <ClaudeIcon size={18} />
          </span>
          <div>
            <p className="text-xs font-semibold text-foreground">Claude Code 模型映射</p>
            <p className="text-[11px] text-muted-foreground">写入 ~/.claude/settings.json 时使用</p>
          </div>
        </div>
        <div className="grid gap-2.5 sm:grid-cols-2">
          <label className="space-y-1">
            <span className={fieldLabelClass}>主模型</span>
            <Input
              value={form.claudeMainModel}
              onChange={(e) => form.setClaudeMainModel(e.target.value)}
              placeholder={LATEST_CLAUDE_MODELS.main}
              list="agent-claude-models"
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>Haiku</span>
            <Input
              value={form.claudeHaikuModel}
              onChange={(e) => form.setClaudeHaikuModel(e.target.value)}
              placeholder={LATEST_CLAUDE_MODELS.haiku}
              list="agent-claude-models"
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>Sonnet</span>
            <Input
              value={form.claudeSonnetModel}
              onChange={(e) => form.setClaudeSonnetModel(e.target.value)}
              placeholder={LATEST_CLAUDE_MODELS.sonnet}
              list="agent-claude-models"
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>Opus</span>
            <Input
              value={form.claudeOpusModel}
              onChange={(e) => form.setClaudeOpusModel(e.target.value)}
              placeholder={LATEST_CLAUDE_MODELS.opus}
              list="agent-claude-models"
            />
          </label>
        </div>
        <datalist id="agent-claude-models">
          {form.claudeModelOptions.map((m) => (
            <option key={m} value={m} />
          ))}
        </datalist>
      </div>

      <div className="space-y-3 border-t border-border/40 pt-4">
        <div className="flex items-center gap-2">
          <AgentToolIcon toolId="codex" size="md" />
          <div>
            <p className="text-xs font-semibold text-foreground">Codex</p>
            <p className="text-[11px] text-muted-foreground">
              写入 ~/.codex/（config.toml、auth.json）— CLI、`codex app` 桌面端与 VS Code / Cursor / Windsurf IDE
              扩展共用此配置
            </p>
          </div>
        </div>
        <div className="grid gap-2.5 sm:grid-cols-2">
          <label className="space-y-1">
            <span className={fieldLabelClass}>模型</span>
            <Input
              value={form.defaultModel}
              onChange={(e) => form.setDefaultModel(e.target.value)}
              list="agent-codex-models"
            />
            <datalist id="agent-codex-models">
              {form.codexModelOptions.map((m) => (
                <option key={m} value={m} />
              ))}
              {LATEST_CODEX_MODELS.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </label>
          <div className="space-y-1">
            <span className={fieldLabelClass}>wire_api</span>
            <div className="flex gap-1.5">
              {(["chat", "responses"] as const).map((api) => (
                <button
                  key={api}
                  type="button"
                  onClick={() => form.setCodexWireApi(api as CodexWireApi)}
                  className={cn(
                    "flex-1 rounded-lg border px-2 py-1.5 text-[11px] font-medium transition-colors",
                    form.codexWireApi === api
                      ? "border-primary/50 bg-primary/10 text-primary"
                      : "border-border/55 text-muted-foreground hover:text-foreground",
                  )}
                >
                  {api === "chat" ? "Chat" : "Responses"}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      <p className="text-[11px] text-muted-foreground">OpenCode 会写入已拉取模型的名称、上下文、输出上限与价格信息。</p>
    </div>
  );
}

export const AgentModelConfig = memo(AgentModelConfigInner);

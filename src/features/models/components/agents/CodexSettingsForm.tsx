import { cn } from "../../../../lib/utils";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import type { CodexAuthMode, CodexWireApi } from "../../lib/providerPatch";
import { fieldLabelClass } from "../providerForm/ProviderConfigPrimitives";

export interface CodexSettingsFormProps {
  wireApi: CodexWireApi;
  authMode: CodexAuthMode;
  onChangeWireApi: (value: CodexWireApi) => void;
  onChangeAuthMode: (value: CodexAuthMode) => void;
  disabled?: boolean;
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
  disabled,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex gap-1.5">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          disabled={disabled}
          onClick={() => onChange(opt.value)}
          className={cn(
            "flex-1 rounded-lg border px-2 py-1.5 text-[11px] font-medium transition-colors",
            value === opt.value
              ? "border-primary/50 bg-primary/10 text-primary"
              : "border-border/55 text-muted-foreground hover:text-foreground",
            disabled && "cursor-not-allowed opacity-60",
          )}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Codex-specific parameters (wire_api + auth_mode). Persisted on the provider
 * record (codex_* fields + meta) — the caller owns the write path.
 */
export function CodexSettingsForm({
  wireApi,
  authMode,
  onChangeWireApi,
  onChangeAuthMode,
  disabled,
}: CodexSettingsFormProps) {
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <AgentToolIcon toolId="codex" size="md" />
        <div>
          <p className="text-xs font-semibold text-foreground">Codex 参数</p>
          <p className="text-[11px] text-muted-foreground">
            写入 ~/.codex/（config.toml、auth.json）— CLI、桌面端与 IDE 扩展共用此配置
          </p>
        </div>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1">
          <span className={fieldLabelClass}>API 格式</span>
          <Segmented
            value={wireApi}
            onChange={onChangeWireApi}
            disabled={disabled}
            options={[
              { value: "responses", label: "Responses" },
              { value: "chat", label: "Chat Completions" },
            ]}
          />
        </div>
        <div className="space-y-1">
          <span className={fieldLabelClass}>认证模式</span>
          <Segmented
            value={authMode}
            onChange={onChangeAuthMode}
            disabled={disabled}
            options={[
              { value: "api_key", label: "API Key" },
              { value: "oauth", label: "OAuth (ChatGPT)" },
            ]}
          />
        </div>
      </div>
    </div>
  );
}

import { ChevronDown, ChevronUp, ExternalLink, Loader2, Save } from "lucide-react";
import { useMemo, useState } from "react";

import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { cn } from "../../../lib/utils";
import { useClaudeConfig } from "../hooks/useClaudeConfig";

import { claudePresets } from "../presets/claudePresets";
import { ApiKeyInput } from "./shared/ApiKeyInput";
import { EndpointInput } from "./shared/EndpointInput";
import { ModelInput } from "./shared/ModelInput";

export function ClaudeConfigPanel() {
  const config = useClaudeConfig();
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Find the active preset to get apiKeyUrl
  const activePreset = useMemo(() => {
    const baseUrl = config.env.ANTHROPIC_BASE_URL || "";
    return claudePresets.find((p) => p.env.ANTHROPIC_BASE_URL === baseUrl || (!baseUrl && !p.env.ANTHROPIC_BASE_URL));
  }, [config.env.ANTHROPIC_BASE_URL]);

  if (config.loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Loader2 className="w-6 h-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  const apiKey = config.env.ANTHROPIC_AUTH_TOKEN || config.env.ANTHROPIC_API_KEY || "";
  const baseUrl = config.env.ANTHROPIC_BASE_URL || "";
  const model = config.env.ANTHROPIC_MODEL || "";

  const handleSave = () => {
    config.save(config.env);
  };

  const handleApplyPreset = (preset: (typeof claudePresets)[0]) => {
    config.applyPreset(preset.env);
  };

  return (
    <div className="space-y-5">
      {/* Presets */}
      <div className="rounded-xl border border-border bg-card p-4">
        <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-3">供应商预设</h3>
        <div className="flex flex-wrap gap-2">
          {claudePresets.map((preset) => (
            <button
              key={preset.name}
              type="button"
              onClick={() => handleApplyPreset(preset)}
              className={cn(
                "group relative px-3 py-1.5 rounded-lg text-xs font-medium border transition-all duration-200",
                baseUrl === (preset.env.ANTHROPIC_BASE_URL || "")
                  ? "bg-primary/10 border-primary/30 text-foreground shadow-sm"
                  : "bg-card border-border text-muted-foreground hover:bg-muted/50 hover:border-border hover:text-foreground",
              )}
            >
              <span className="flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full" style={{ backgroundColor: preset.iconColor || "#888" }} />
                {preset.name}
              </span>
            </button>
          ))}
        </div>
      </div>

      {/* Core Fields */}
      <div className="rounded-xl border border-border bg-card p-4 space-y-4">
        <ApiKeyInput
          value={apiKey}
          onChange={(v) => config.updateEnv("ANTHROPIC_AUTH_TOKEN", v)}
          apiKeyUrl={activePreset?.apiKeyUrl}
        />
        <EndpointInput value={baseUrl} onChange={(v) => config.updateEnv("ANTHROPIC_BASE_URL", v)} />

        <ModelInput
          value={model}
          onChange={(v) => config.updateEnv("ANTHROPIC_MODEL", v)}
          label="主模型"
          placeholder="claude-sonnet-4-5-20250514"
        />
      </div>

      {/* Advanced: Reasoning Model + Model Mappings */}
      <div className="rounded-xl border border-border bg-card overflow-hidden">
        <button
          type="button"
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="w-full flex items-center justify-between px-4 py-3 text-sm font-medium text-muted-foreground hover:text-foreground transition-colors"
        >
          <span>高级设置</span>
          {showAdvanced ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
        </button>
        {showAdvanced && (
          <div className="px-4 pb-4 space-y-4 border-t border-border pt-4">
            <ModelInput
              value={config.env.ANTHROPIC_REASONING_MODEL || ""}
              onChange={(v) => config.updateEnv("ANTHROPIC_REASONING_MODEL", v)}
              label="推理模型"
              placeholder="claude-sonnet-4..."
            />
            <ModelInput
              value={config.env.ANTHROPIC_DEFAULT_HAIKU_MODEL || ""}
              onChange={(v) => config.updateEnv("ANTHROPIC_DEFAULT_HAIKU_MODEL", v)}
              label="Haiku 模型"
              placeholder="claude-haiku-3-5-20241022"
            />
            <ModelInput
              value={config.env.ANTHROPIC_DEFAULT_SONNET_MODEL || ""}
              onChange={(v) => config.updateEnv("ANTHROPIC_DEFAULT_SONNET_MODEL", v)}
              label="Sonnet 模型"
              placeholder="claude-sonnet-4-5-20250514"
            />
            <ModelInput
              value={config.env.ANTHROPIC_DEFAULT_OPUS_MODEL || ""}
              onChange={(v) => config.updateEnv("ANTHROPIC_DEFAULT_OPUS_MODEL", v)}
              label="Opus 模型"
              placeholder="claude-opus-4-5-20250514"
            />
          </div>
        )}
      </div>

      {/* Config path + Save */}
      <div className="flex items-center justify-between pt-1">
        <p className="text-xs text-muted-foreground/60 font-mono truncate">~/.claude/settings.json</p>
        <div className="flex items-center gap-2">
          {activePreset?.websiteUrl && (
            <ExternalAnchor
              href={activePreset.websiteUrl}
              className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              title="官网"
            >
              <ExternalLink className="w-4 h-4" />
            </ExternalAnchor>
          )}
          <button
            type="button"
            onClick={handleSave}
            disabled={config.saving}
            className="flex items-center gap-2 px-4 py-2 rounded-lg bg-[#D97757] hover:bg-[#D97757]/80 text-white text-sm font-medium transition-colors disabled:opacity-50"
          >
            {config.saving ? <Loader2 className="w-4 h-4 animate-spin" /> : <Save className="w-4 h-4" />}
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

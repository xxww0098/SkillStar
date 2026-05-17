import { ArrowLeft, Eye, EyeOff, Loader2 } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { cn } from "../../../lib/utils";
import type { ProviderEntryFlat, ProviderPresetFlat } from "../../../types";
import { useProvidersFlat } from "../hooks/useProvidersFlat";
import { ProviderBrandIcon } from "./ProviderBrandIcon";

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

export interface PresetSelectorProps {
  onProviderCreated: (provider: ProviderEntryFlat) => void;
  onCancel?: () => void;
}

// ---------------------------------------------------------------------------
// Static preset list (mirrors Rust `get_all_presets_flat()`)
// ---------------------------------------------------------------------------

const PRESETS: ProviderPresetFlat[] = [
  // ── 国内模型 (domestic) ──
  {
    id: "deepseek",
    name: "DeepSeek",
    category: "domestic",
    base_url_openai: "https://api.deepseek.com/v1",
    base_url_anthropic: "https://api.deepseek.com/anthropic",
    models_url: "https://api.deepseek.com/v1/models",
    models: [],
    icon_color: "#4D6BFE",
    api_key_url: "https://platform.deepseek.com/api_keys",
  },
  {
    id: "kimi",
    name: "Kimi",
    category: "domestic",
    base_url_openai: "https://api.moonshot.cn/v1",
    base_url_anthropic: "https://api.moonshot.cn/anthropic",
    models_url: "https://api.moonshot.cn/v1/models",
    models: [],
    icon_color: "#5B45E0",
    api_key_url: "https://platform.moonshot.cn/console/api-keys",
  },
  {
    id: "kimi-coding",
    name: "Kimi For Coding",
    category: "domestic",
    base_url_openai: "https://api.kimi.com/coding/v1",
    base_url_anthropic: "https://api.kimi.com/coding/",
    models_url: "https://api.moonshot.cn/v1/models",
    models: [],
    icon_color: "#5B45E0",
    api_key_url: "https://platform.moonshot.cn/console/api-keys",
  },
  {
    id: "minimax",
    name: "MiniMax",
    category: "domestic",
    base_url_openai: "https://api.minimax.chat/v1",
    base_url_anthropic: "https://api.minimax.chat/anthropic",
    models_url: "https://api.minimax.chat/v1/models",
    models: [],
    icon_color: "#FF6B35",
    api_key_url: "https://platform.minimaxi.com/user-center/basic-information/interface-key",
  },
  {
    id: "qwen",
    name: "通义千问",
    category: "domestic",
    base_url_openai: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    base_url_anthropic: "https://dashscope.aliyuncs.com/api/v2/apps/anthropic",
    models_url: "https://dashscope.aliyuncs.com/compatible-mode/v1/models",
    models: [],
    icon_color: "#6236FF",
    api_key_url: "https://dashscope.console.aliyun.com/apiKey",
  },
  {
    id: "qwen-coding",
    name: "通义千问 Coding Plan",
    category: "domestic",
    base_url_openai: "https://coding-intl.dashscope.aliyuncs.com/v1",
    base_url_anthropic: "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic",
    models_url: "https://coding-intl.dashscope.aliyuncs.com/v1/models",
    models: [],
    icon_color: "#6236FF",
    api_key_url: "https://dashscope.console.aliyun.com/apiKey",
  },
  {
    id: "glm",
    name: "智谱 GLM",
    category: "domestic",
    base_url_openai: "https://open.bigmodel.cn/api/paas/v4",
    base_url_anthropic: "https://open.bigmodel.cn/api/anthropic",
    models_url: "https://open.bigmodel.cn/api/paas/v4/models",
    models: [],
    icon_color: "#3366FF",
    api_key_url: "https://open.bigmodel.cn/usercenter/apikeys",
  },
  {
    id: "glm-coding",
    name: "智谱 GLM Coding Plan",
    category: "domestic",
    base_url_openai: "https://api.z.ai/api/coding/paas/v4",
    base_url_anthropic: "https://api.z.ai/api/anthropic",
    models_url: "https://api.z.ai/api/coding/paas/v4/models",
    models: [],
    icon_color: "#3366FF",
    api_key_url: "https://open.bigmodel.cn/usercenter/apikeys",
  },
  {
    id: "volcengine",
    name: "火山方舟",
    category: "domestic",
    base_url_openai: "https://ark.cn-beijing.volces.com/api/v3",
    base_url_anthropic: "https://ark.cn-beijing.volces.com/api/v3/anthropic",
    models_url: "https://ark.cn-beijing.volces.com/api/v3/models",
    models: [],
    icon_color: "#FF4D4F",
    api_key_url: "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey",
  },
  {
    id: "mimo",
    name: "小米 MiMo",
    category: "domestic",
    base_url_openai: "https://platform.xiaomimimo.com/v1",
    base_url_anthropic: "https://platform.xiaomimimo.com/anthropic",
    models_url: "https://platform.xiaomimimo.com/v1/models",
    models: [],
    icon_color: "#FF6900",
    api_key_url: "https://platform.xiaomimimo.com",
  },
  // ── 官方中转站 (relay) ──
  {
    id: "openrouter",
    name: "OpenRouter",
    category: "relay",
    base_url_openai: "https://openrouter.ai/api/v1",
    base_url_anthropic: "",
    models_url: "https://openrouter.ai/api/v1/models",
    models: [],
    icon_color: "#6366F1",
    api_key_url: "https://openrouter.ai/keys",
  },
  {
    id: "siliconflow",
    name: "SiliconFlow",
    category: "relay",
    base_url_openai: "https://api.siliconflow.cn/v1",
    base_url_anthropic: "",
    models_url: "https://api.siliconflow.cn/v1/models",
    models: [],
    icon_color: "#00D4AA",
    api_key_url: "https://cloud.siliconflow.cn/account/ak",
  },
];

// ---------------------------------------------------------------------------
// Category definitions
// ---------------------------------------------------------------------------

interface CategoryDef {
  key: string;
  label: string;
}

const CATEGORIES: CategoryDef[] = [
  { key: "domestic", label: "国内模型" },
  { key: "relay", label: "官方中转站" },
  { key: "openai_compatible", label: "OpenAI 兼容" },
];

// The "OpenAI 兼容" virtual preset (not in the static list)
const OPENAI_COMPATIBLE_PRESET: ProviderPresetFlat = {
  id: "openai-compatible",
  name: "OpenAI 兼容",
  category: "openai_compatible",
  base_url_openai: "https://api.openai.com/v1",
  base_url_anthropic: "",
  models_url: "https://api.openai.com/v1/models",
  models: [],
  icon_color: "#10A37F",
};

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/**
 * Preset selector for adding a new provider.
 *
 * Two states:
 * 1. Preset grid — shows all presets in 3 categories
 * 2. Form state — after selecting a preset, shows minimal form (API Key + editable Base URL)
 */
export function PresetSelector({ onProviderCreated, onCancel }: PresetSelectorProps) {
  const { createProvider } = useProvidersFlat();

  // ── State ────────────────────────────────────────────────────────────
  const [selectedPreset, setSelectedPreset] = useState<ProviderPresetFlat | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [baseUrlOpenai, setBaseUrlOpenai] = useState("");
  const [baseUrlAnthropic, setBaseUrlAnthropic] = useState("");
  const [showApiKey, setShowApiKey] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // ── Grouped presets ──────────────────────────────────────────────────
  const groupedPresets = useMemo(() => {
    const groups: Record<string, ProviderPresetFlat[]> = {
      domestic: [],
      relay: [],
      openai_compatible: [OPENAI_COMPATIBLE_PRESET],
    };
    for (const preset of PRESETS) {
      if (groups[preset.category]) {
        groups[preset.category].push(preset);
      }
    }
    return groups;
  }, []);

  // ── Handlers ─────────────────────────────────────────────────────────
  const handleSelectPreset = useCallback((preset: ProviderPresetFlat) => {
    setSelectedPreset(preset);
    setBaseUrlOpenai(preset.base_url_openai);
    setBaseUrlAnthropic(preset.base_url_anthropic);
    setApiKey("");
    setShowApiKey(false);
    setError(null);
  }, []);

  const handleBack = useCallback(() => {
    setSelectedPreset(null);
    setApiKey("");
    setBaseUrlOpenai("");
    setBaseUrlAnthropic("");
    setError(null);
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!selectedPreset) return;
    if (!apiKey.trim()) {
      setError("请输入 API Key");
      return;
    }

    setSubmitting(true);
    setError(null);

    try {
      const entry: Partial<ProviderEntryFlat> = {
        id: "",
        name: selectedPreset.name,
        base_url_openai: baseUrlOpenai.trim(),
        base_url_anthropic: baseUrlAnthropic.trim(),
        models_url: selectedPreset.models_url,
        api_key: apiKey.trim(),
        models: [],
        default_model: "",
        preset_id: selectedPreset.id === "openai-compatible" ? undefined : selectedPreset.id,
        icon_color: selectedPreset.icon_color,
      };

      const created = await createProvider(entry);
      onProviderCreated(created);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setSubmitting(false);
    }
  }, [selectedPreset, apiKey, baseUrlOpenai, baseUrlAnthropic, createProvider, onProviderCreated]);

  // ── Render: Form state ───────────────────────────────────────────────
  if (selectedPreset) {
    return (
      <div className="space-y-5">
        {/* Header with back button */}
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={handleBack}
            disabled={submitting}
            className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent/10 transition-colors cursor-pointer disabled:opacity-50"
            aria-label="返回预设列表"
          >
            <ArrowLeft className="w-4 h-4" />
          </button>
          <div className="flex items-center gap-2">
            <ProviderBrandIcon
              presetId={selectedPreset.id}
              providerName={selectedPreset.name}
              iconColor={selectedPreset.icon_color}
              size="sm"
            />
            <h3 className="text-sm font-semibold text-foreground">{selectedPreset.name}</h3>
          </div>
        </div>

        {/* API Key input */}
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground">API Key</label>
          <div className="relative">
            <Input
              type={showApiKey ? "text" : "password"}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              className="pr-10"
              disabled={submitting}
              autoFocus
            />
            <button
              type="button"
              onClick={() => setShowApiKey(!showApiKey)}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1 rounded-md text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
              aria-label={showApiKey ? "隐藏 API Key" : "显示 API Key"}
            >
              {showApiKey ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
            </button>
          </div>
        </div>

        {/* Base URL (OpenAI) — editable */}
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-muted-foreground">Base URL (OpenAI 兼容)</label>
          <Input
            value={baseUrlOpenai}
            onChange={(e) => setBaseUrlOpenai(e.target.value)}
            placeholder="https://api.example.com/v1"
            disabled={submitting}
          />
        </div>

        {/* Base URL (Anthropic) — editable, shown only if preset has one */}
        {(selectedPreset.base_url_anthropic || selectedPreset.id === "openai-compatible") && (
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">Base URL (Anthropic 兼容)</label>
            <Input
              value={baseUrlAnthropic}
              onChange={(e) => setBaseUrlAnthropic(e.target.value)}
              placeholder="https://api.example.com/anthropic"
              disabled={submitting}
            />
          </div>
        )}

        {/* Error message */}
        {error && (
          <div className="rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}

        {/* Action buttons */}
        <div className="flex items-center gap-3 pt-2">
          <Button onClick={handleSubmit} disabled={submitting || !apiKey.trim()} className="min-w-[80px]">
            {submitting ? <Loader2 className="w-4 h-4 animate-spin" /> : null}
            {submitting ? "创建中..." : "创建"}
          </Button>
          <Button variant="outline" onClick={handleBack} disabled={submitting}>
            返回
          </Button>
        </div>
      </div>
    );
  }

  // ── Render: Preset grid ──────────────────────────────────────────────
  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-foreground">选择供应商预设</h3>
        {onCancel && (
          <Button variant="ghost" size="sm" onClick={onCancel}>
            取消
          </Button>
        )}
      </div>

      {/* Categories */}
      {CATEGORIES.map((category) => {
        const presets = groupedPresets[category.key];
        if (!presets || presets.length === 0) return null;

        return (
          <div key={category.key} className="space-y-3">
            <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{category.label}</h4>
            <div className="grid gap-2.5 grid-cols-2 sm:grid-cols-3 lg:grid-cols-4">
              {presets.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  onClick={() => handleSelectPreset(preset)}
                  className={cn(
                    "flex flex-col items-start gap-2 p-3 rounded-xl border transition-all duration-200 text-left",
                    "bg-card/80 backdrop-blur-sm border-border/60",
                    "hover:bg-card-hover hover:-translate-y-0.5 hover:shadow-[0_8px_30px_-10px_var(--color-shadow)]",
                    "hover:border-primary/40 cursor-pointer",
                  )}
                >
                  {/* Provider icon + name */}
                  <div className="flex items-center gap-2 w-full min-w-0">
                    <ProviderBrandIcon
                      presetId={preset.id}
                      providerName={preset.name}
                      iconColor={preset.icon_color}
                      size="sm"
                    />
                    <span className="text-sm font-semibold text-foreground truncate">{preset.name}</span>
                  </div>

                  <p className="text-xs text-muted-foreground line-clamp-2 w-full">创建后从供应商获取模型</p>
                </button>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}

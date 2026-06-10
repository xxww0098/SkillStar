import { motion } from "framer-motion";
import { ArrowLeft, Eye, EyeOff, ExternalLink, KeyRound, Loader2, Search, Sparkles } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { Button } from "../../../../components/ui/button";
import { Input } from "../../../../components/ui/input";
import { openExternalUrl } from "../../../../lib/externalOpen";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat, ProviderPresetFlat } from "../../../../types";
import { useProviderPresets } from "../../api/presets";
import { useProvidersFlat } from "../../hooks/useProvidersFlat";
import { ProviderBrandIcon } from "../shared/ProviderBrandIcon";

export interface PresetPickerProps {
  /** Called once a provider has been created and the drawer should pivot to the edit form. */
  onProviderCreated: (provider: ProviderEntryFlat) => void;
  /** Optional preset to use for the initial selection (e.g. when adding from an agent context). */
  initialPreset?: ProviderPresetFlat | null;
}

interface CategoryDef {
  key: string;
  label: string;
  hint: string;
  emoji: string;
}

const CATEGORIES: CategoryDef[] = [
  {
    key: "official",
    label: "国际大厂",
    hint: "官方端点已预设，填 API Key 即可开始",
    emoji: "🌍",
  },
  {
    key: "domestic",
    label: "国内模型",
    hint: "官方端点已预设，填 API Key 即可开始",
    emoji: "🇨🇳",
  },
  {
    key: "relay",
    label: "中转 / 聚合",
    hint: "中转服务，多家模型统一访问",
    emoji: "🌐",
  },
  {
    key: "openai_compatible",
    label: "OpenAI 兼容",
    hint: "自定义 OpenAI 兼容端点，需手填 Base URL",
    emoji: "🧪",
  },
];

export function PresetPicker({ onProviderCreated, initialPreset = null }: PresetPickerProps) {
  const { createProvider } = useProvidersFlat();
  const { grouped, isLoading } = useProviderPresets();

  const [selected, setSelected] = useState<ProviderPresetFlat | null>(initialPreset);
  const [apiKey, setApiKey] = useState("");
  const [baseUrlOpenai, setBaseUrlOpenai] = useState(initialPreset?.base_url_openai ?? "");
  const [baseUrlAnthropic, setBaseUrlAnthropic] = useState(initialPreset?.base_url_anthropic ?? "");
  const [showApiKey, setShowApiKey] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  const handleSelect = useCallback((p: ProviderPresetFlat) => {
    setSelected(p);
    setBaseUrlOpenai(p.base_url_openai);
    setBaseUrlAnthropic(p.base_url_anthropic);
    setApiKey("");
    setError(null);
  }, []);

  const handleBack = useCallback(() => {
    setSelected(null);
    setApiKey("");
    setError(null);
  }, []);

  const filtered = useMemo(() => {
    if (!search.trim()) return grouped;
    const q = search.toLowerCase();
    return {
      official: grouped.official.filter((p) => p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q)),
      domestic: grouped.domestic.filter((p) => p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q)),
      relay: grouped.relay.filter((p) => p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q)),
      openai_compatible: grouped.openai_compatible.filter(
        (p) => p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q),
      ),
    } as typeof grouped;
  }, [grouped, search]);

  const anyMatch = useMemo(
    () => CATEGORIES.some((c) => (filtered[c.key as keyof typeof filtered] ?? []).length > 0),
    [filtered],
  );

  const handleSubmit = useCallback(async () => {
    if (!selected) return;
    if (!apiKey.trim()) {
      setError("请输入 API Key");
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      const entry: Partial<ProviderEntryFlat> = {
        id: "",
        name: selected.name,
        base_url_openai: baseUrlOpenai.trim(),
        base_url_anthropic: baseUrlAnthropic.trim(),
        models_url: selected.models_url,
        api_key: apiKey.trim(),
        models: [],
        default_model: "",
        preset_id: selected.id === "openai-compatible" ? undefined : selected.id,
        icon_color: selected.icon_color,
      };
      const created = await createProvider(entry);
      onProviderCreated(created);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  }, [selected, apiKey, baseUrlOpenai, baseUrlAnthropic, createProvider, onProviderCreated]);

  if (selected) {
    const isDomestic = selected.category === "domestic";
    const showOpenaiUrl = !isDomestic;
    const showAnthropicUrl =
      !isDomestic && (selected.id === "openai-compatible" || Boolean(selected.base_url_anthropic?.trim()));

    return (
      <motion.div
        key={`fill-${selected.id}`}
        initial={{ opacity: 0, x: 10 }}
        animate={{ opacity: 1, x: 0 }}
        transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
        className="space-y-5"
      >
        <button
          type="button"
          onClick={handleBack}
          className="inline-flex items-center gap-1 text-xs text-muted-foreground transition hover:text-foreground"
        >
          <ArrowLeft className="h-3.5 w-3.5" />
          换一个预设
        </button>

        <div className="rounded-xl border border-border/55 bg-card/60 px-4 py-4 shadow-sm backdrop-blur-sm">
          <div className="flex items-center gap-3">
            <ProviderBrandIcon
              presetId={selected.id}
              providerName={selected.name}
              iconColor={selected.icon_color}
              size="md"
            />
            <div className="min-w-0 flex-1">
              <h4 className="truncate text-sm font-semibold text-foreground">{selected.name}</h4>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                {CATEGORIES.find((c) => c.key === selected.category)?.hint ?? "填写 API Key 创建供应商"}
              </p>
            </div>
          </div>
        </div>

        <div className="space-y-1.5">
          <div className="flex items-center justify-between gap-2">
            <label className="text-xs font-medium text-muted-foreground">API Key</label>
            {selected.api_key_url ? (
              <button
                type="button"
                onClick={() => void openExternalUrl(selected.api_key_url!)}
                className="inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
              >
                获取 Key
                <ExternalLink className="h-3 w-3" />
              </button>
            ) : null}
          </div>
          <div className="relative">
            <KeyRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
            <Input
              type={showApiKey ? "text" : "password"}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              className="pl-9 pr-10"
              autoFocus
              disabled={submitting}
            />
            <button
              type="button"
              onClick={() => setShowApiKey((v) => !v)}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground transition hover:text-foreground"
              aria-label={showApiKey ? "隐藏" : "显示"}
            >
              {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </button>
          </div>
        </div>

        {showOpenaiUrl ? (
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">Base URL (OpenAI 兼容)</label>
            <Input
              value={baseUrlOpenai}
              onChange={(e) => setBaseUrlOpenai(e.target.value)}
              placeholder="https://api.example.com/v1"
              disabled={submitting}
            />
          </div>
        ) : null}

        {showAnthropicUrl ? (
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">Base URL (Anthropic 兼容)</label>
            <Input
              value={baseUrlAnthropic}
              onChange={(e) => setBaseUrlAnthropic(e.target.value)}
              placeholder="https://api.example.com/anthropic"
              disabled={submitting}
            />
          </div>
        ) : null}

        {error ? (
          <div className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        ) : null}

        <div className="flex items-center gap-2 pt-1">
          <Button onClick={handleSubmit} disabled={submitting || !apiKey.trim()} className="min-w-[120px]">
            {submitting ? (
              <Loader2 className="mr-1.5 h-3.5 w-3.5 animate-spin" />
            ) : (
              <Sparkles className="mr-1.5 h-3.5 w-3.5" />
            )}
            {submitting ? "创建中…" : "创建并继续"}
          </Button>
          <Button variant="ghost" onClick={handleBack} disabled={submitting}>
            返回
          </Button>
        </div>

        <p className="text-[11px] text-muted-foreground/80">创建后会自动停留在此抽屉,可继续配置模型与 Agent 绑定。</p>
      </motion.div>
    );
  }

  return (
    <div className="space-y-5">
      <div className="relative">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/60" />
        <Input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="搜索供应商..."
          className="pl-9"
        />
      </div>

      {isLoading ? (
        <div className="flex items-center justify-center py-10">
          <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
        </div>
      ) : !anyMatch ? (
        <div className="py-10 text-center text-sm text-muted-foreground">无匹配结果</div>
      ) : (
        CATEGORIES.map((cat) => {
          const list = filtered[cat.key as keyof typeof filtered] ?? [];
          if (list.length === 0) return null;
          return (
            <section key={cat.key} className="space-y-2.5">
              <header className="flex items-center gap-2">
                <span className="text-base leading-none">{cat.emoji}</span>
                <div>
                  <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{cat.label}</h4>
                  <p className="text-[11px] text-muted-foreground/80">{cat.hint}</p>
                </div>
              </header>
              <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                {list.map((p) => (
                  <PresetTile key={p.id} preset={p} onSelect={handleSelect} />
                ))}
              </div>
            </section>
          );
        })
      )}
    </div>
  );
}

function PresetTile({
  preset,
  onSelect,
}: {
  preset: ProviderPresetFlat;
  onSelect: (preset: ProviderPresetFlat) => void;
}) {
  return (
    <motion.button
      type="button"
      whileHover={{ y: -2 }}
      transition={{ type: "spring", stiffness: 320, damping: 22 }}
      onClick={() => onSelect(preset)}
      className={cn(
        "group flex w-full cursor-pointer items-center gap-2.5 rounded-xl border border-border/60 bg-card/70 px-3 py-2.5 text-left backdrop-blur-sm",
        "transition hover:border-primary/40 hover:bg-card-hover hover:shadow-[0_10px_30px_-16px_var(--color-shadow)]",
      )}
    >
      <ProviderBrandIcon presetId={preset.id} providerName={preset.name} iconColor={preset.icon_color} size="sm" />
      <span className="min-w-0 flex-1 truncate text-sm font-semibold text-foreground">{preset.name}</span>
    </motion.button>
  );
}

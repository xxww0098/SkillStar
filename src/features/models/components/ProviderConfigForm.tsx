import {
  ChevronDown,
  Copy,
  Download,
  Eye,
  EyeOff,
  Globe2,
  Info,
  KeyRound,
  Loader2,
  ShieldCheck,
  SlidersHorizontal,
} from "lucide-react";
import ClaudeIcon from "@lobehub/icons/es/Claude/components/Color";
import CodexIcon from "@lobehub/icons/es/Codex/components/Color";
import { type ElementType, type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { Switch } from "../../../components/ui/switch";
import { cn } from "../../../lib/utils";
import type { ProviderEntryFlat, ProviderPatchFlat } from "../../../types";
import { useModelFetch } from "../hooks/useModelFetch";

export interface ProviderConfigFormProps {
  provider: ProviderEntryFlat;
  onSave: (patch: ProviderPatchFlat) => Promise<void>;
}

const collapseCardClass =
  "overflow-hidden rounded-xl border border-border/60 bg-card/55 shadow-sm backdrop-blur-sm";

type ModelFetchTarget = "claude" | "codex";

const AUTO_SAVE_DEBOUNCE_MS = 600;

const CLAUDE_MODEL_META_KEYS = {
  main: "claude_main_model",
  haiku: "claude_haiku_model",
  sonnet: "claude_sonnet_model",
  opus: "claude_opus_model",
} as const;

const LATEST_CLAUDE_MODELS = {
  main: "claude-sonnet-4-6",
  haiku: "claude-haiku-4-5-20251001",
  sonnet: "claude-sonnet-4-6",
  opus: "claude-opus-4-7",
} as const;

function getMetaString(meta: Record<string, unknown> | undefined, key: string): string {
  const value = meta?.[key];
  return typeof value === "string" ? value : "";
}

function buildModelCatalog(values: string[]): string[] {
  const seen = new Set<string>();
  const catalog: string[] = [];

  for (const value of values) {
    const trimmed = value.trim();
    if (trimmed && !seen.has(trimmed)) {
      seen.add(trimmed);
      catalog.push(trimmed);
    }
  }

  return catalog;
}

interface ConfigSectionGroupProps {
  title: string;
  description?: string;
  children: ReactNode;
}

/** 分组大标题：下面放若干独立折叠卡片 */
function ConfigSectionGroup({ title, description, children }: ConfigSectionGroupProps) {
  return (
    <section className="space-y-2.5">
      <div className="px-1">
        <h3 className="text-[13px] font-semibold tracking-tight text-foreground">{title}</h3>
        {description && <p className="mt-0.5 text-xs leading-5 text-muted-foreground">{description}</p>}
      </div>
      <div className="space-y-2">{children}</div>
    </section>
  );
}

interface ConfigCollapseSectionProps {
  id: string;
  title: string;
  summary?: string;
  expanded: boolean;
  onToggle: () => void;
  children: ReactNode;
  icon?: ElementType;
  iconSlot?: ReactNode;
  headerAction?: ReactNode;
}

function ConfigCollapseSection({
  id,
  icon: Icon,
  iconSlot,
  title,
  summary,
  expanded,
  onToggle,
  headerAction,
  children,
}: ConfigCollapseSectionProps) {
  const contentId = `${id}-content`;

  return (
    <div className={collapseCardClass}>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={expanded}
        aria-controls={contentId}
        aria-label={expanded ? `折叠 ${title}` : `展开 ${title}`}
        className="flex w-full cursor-pointer items-center gap-3 px-4 py-3 text-left transition-colors hover:bg-muted/30"
      >
        {iconSlot ? (
          iconSlot
        ) : Icon ? (
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-primary/15 bg-primary/10 text-primary">
            <Icon className="h-4 w-4" />
          </span>
        ) : null}
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-semibold text-foreground">{title}</h3>
          {!expanded && summary && (
            <p className="mt-0.5 truncate text-xs text-muted-foreground">{summary}</p>
          )}
        </div>
        {headerAction && (
          <span
            className="shrink-0"
            onClick={(event) => event.stopPropagation()}
            onKeyDown={(event) => event.stopPropagation()}
          >
            {headerAction}
          </span>
        )}
        <ChevronDown
          className={cn(
            "h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200",
            !expanded && "-rotate-90",
          )}
        />
      </button>

      {expanded && (
        <div id={contentId} className="space-y-4 border-t border-border/60 px-4 pb-4 pt-3">
          {children}
        </div>
      )}
    </div>
  );
}

/**
 * Provider configuration form rendered inside ProviderDetailPanel's 配置区.
 *
 * Sections:
 * 1. 连接信息 — name, endpoints, models URL, notes, API Key
 * 2. 模型配置 — Claude/Codex model fields and "获取模型列表" actions
 * 3. 高级设置 — context length, max tokens, timeout, retry count, streaming toggle
 * 4. Actions — debounced auto-save
 */
export function ProviderConfigForm({ provider, onSave }: ProviderConfigFormProps) {
  // ── Form state (initialized from provider prop) ──────────────────────
  const [name, setName] = useState(provider.name);
  const [baseUrlOpenai, setBaseUrlOpenai] = useState(provider.base_url_openai);
  const [baseUrlAnthropic, setBaseUrlAnthropic] = useState(provider.base_url_anthropic);
  const [modelsUrl, setModelsUrl] = useState(provider.models_url ?? "");
  const [notes, setNotes] = useState(provider.notes ?? "");
  const [apiKey, setApiKey] = useState(provider.api_key);
  const [models, setModels] = useState<string[]>(provider.models);
  const [defaultModel, setDefaultModel] = useState(provider.default_model);
  const [claudeMainModel, setClaudeMainModel] = useState(getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main));
  const [claudeHaikuModel, setClaudeHaikuModel] = useState(getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.haiku));
  const [claudeSonnetModel, setClaudeSonnetModel] = useState(
    getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.sonnet),
  );
  const [claudeOpusModel, setClaudeOpusModel] = useState(getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.opus));

  // Advanced settings (stored in meta)
  const [contextLength, setContextLength] = useState<number>((provider.meta?.context_length as number) ?? 128000);
  const [maxTokens, setMaxTokens] = useState<number>((provider.meta?.max_tokens as number) ?? 4096);
  const [timeout, setTimeout_] = useState<number>((provider.meta?.timeout as number) ?? 30);
  const [retryCount, setRetryCount] = useState<number>((provider.meta?.retry_count as number) ?? 3);
  const [streaming, setStreaming] = useState<boolean>((provider.meta?.streaming as boolean) ?? true);

  // UI state
  const [showApiKey, setShowApiKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [connectionExpanded, setConnectionExpanded] = useState(true);
  const [advancedExpanded, setAdvancedExpanded] = useState(false);
  const [claudeExpanded, setClaudeExpanded] = useState(false);
  const [codexExpanded, setCodexExpanded] = useState(false);

  // Auto-fetch models state
  const { fetchModels, isLoading: isFetchingModels, error: fetchError } = useModelFetch();
  const [claudeModelOptions, setClaudeModelOptions] = useState<string[]>(() =>
    buildModelCatalog([
      LATEST_CLAUDE_MODELS.main,
      LATEST_CLAUDE_MODELS.haiku,
      LATEST_CLAUDE_MODELS.sonnet,
      LATEST_CLAUDE_MODELS.opus,
      ...provider.models,
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main),
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.haiku),
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.sonnet),
      getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.opus),
    ]),
  );
  const [codexModelOptions, setCodexModelOptions] = useState<string[]>(() =>
    buildModelCatalog([...provider.models, provider.default_model]),
  );
  const [modelFetchStatus, setModelFetchStatus] = useState<{ target: ModelFetchTarget; count: number } | null>(null);
  const [fetchTarget, setFetchTarget] = useState<ModelFetchTarget>("codex");

  // Reset form when provider changes
  useEffect(() => {
    const providerClaudeMainModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.main);
    const providerClaudeHaikuModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.haiku);
    const providerClaudeSonnetModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.sonnet);
    const providerClaudeOpusModel = getMetaString(provider.meta, CLAUDE_MODEL_META_KEYS.opus);

    setName(provider.name);
    setBaseUrlOpenai(provider.base_url_openai);
    setBaseUrlAnthropic(provider.base_url_anthropic);
    setModelsUrl(provider.models_url ?? "");
    setNotes(provider.notes ?? "");
    setApiKey(provider.api_key);
    setModels(provider.models);
    setDefaultModel(provider.default_model);
    setClaudeMainModel(providerClaudeMainModel);
    setClaudeHaikuModel(providerClaudeHaikuModel);
    setClaudeSonnetModel(providerClaudeSonnetModel);
    setClaudeOpusModel(providerClaudeOpusModel);
    setContextLength((provider.meta?.context_length as number) ?? 128000);
    setMaxTokens((provider.meta?.max_tokens as number) ?? 4096);
    setTimeout_((provider.meta?.timeout as number) ?? 30);
    setRetryCount((provider.meta?.retry_count as number) ?? 3);
    setStreaming((provider.meta?.streaming as boolean) ?? true);
    setConnectionExpanded(true);
    setAdvancedExpanded(false);
    setClaudeExpanded(false);
    setCodexExpanded(false);
    setClaudeModelOptions(
      buildModelCatalog([
        LATEST_CLAUDE_MODELS.main,
        LATEST_CLAUDE_MODELS.haiku,
        LATEST_CLAUDE_MODELS.sonnet,
        LATEST_CLAUDE_MODELS.opus,
        ...provider.models,
        providerClaudeMainModel,
        providerClaudeHaikuModel,
        providerClaudeSonnetModel,
        providerClaudeOpusModel,
      ]),
    );
    setCodexModelOptions(buildModelCatalog([...provider.models, provider.default_model]));
    setModelFetchStatus(null);
    setFetchTarget("codex");
  }, [provider]);

  // ── Auto-fetch models handlers ───────────────────────────────────────
  //
  // Single, unified entry point: every agent config (Claude / Codex / …)
  // shares one `models_url` per provider. The target argument only decides
  // which agent panel gets visually highlighted with the resulting catalog.
  const handleFetchModels = useCallback(
    async (target: ModelFetchTarget) => {
      const url = modelsUrl.trim();
      setFetchTarget(target);
      if (target === "claude") {
        setClaudeExpanded(true);
      } else {
        setCodexExpanded(true);
      }
      try {
        const result = await fetchModels(url, apiKey.trim());
        const fetchedCatalog = buildModelCatalog(result);
        setModels((prev) => buildModelCatalog([...prev, ...fetchedCatalog]));
        setClaudeModelOptions((prev) => buildModelCatalog([...fetchedCatalog, ...prev]));
        setCodexModelOptions((prev) => buildModelCatalog([...fetchedCatalog, ...prev]));
        setModelFetchStatus({ target, count: fetchedCatalog.length });
      } catch {
        // Error is already captured in the hook's error state
        setModelFetchStatus(null);
      }
    },
    [fetchModels, modelsUrl, apiKey],
  );

  const buildPatch = useCallback((): ProviderPatchFlat => {
    const modelCatalog = buildModelCatalog([
      ...models,
      defaultModel,
      claudeMainModel,
      claudeHaikuModel,
      claudeSonnetModel,
      claudeOpusModel,
    ]);

    return {
      name: name.trim(),
      base_url_openai: baseUrlOpenai.trim(),
      base_url_anthropic: baseUrlAnthropic.trim(),
      models_url: modelsUrl.trim(),
      api_key: apiKey,
      models: modelCatalog,
      default_model: defaultModel.trim(),
      notes: notes.trim() || undefined,
      meta: {
        ...(provider.meta ?? {}),
        context_length: contextLength,
        max_tokens: maxTokens,
        timeout: timeout,
        retry_count: retryCount,
        streaming,
        [CLAUDE_MODEL_META_KEYS.main]: claudeMainModel.trim(),
        [CLAUDE_MODEL_META_KEYS.haiku]: claudeHaikuModel.trim(),
        [CLAUDE_MODEL_META_KEYS.sonnet]: claudeSonnetModel.trim(),
        [CLAUDE_MODEL_META_KEYS.opus]: claudeOpusModel.trim(),
      },
    };
  }, [
    name,
    baseUrlOpenai,
    baseUrlAnthropic,
    modelsUrl,
    apiKey,
    models,
    defaultModel,
    claudeMainModel,
    claudeHaikuModel,
    claudeSonnetModel,
    claudeOpusModel,
    notes,
    contextLength,
    maxTokens,
    timeout,
    retryCount,
    streaming,
    provider.meta,
  ]);

  const persistChanges = useCallback(async () => {
    setSaving(true);
    try {
      await onSave(buildPatch());
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(`保存失败：${message}`);
    } finally {
      setSaving(false);
    }
  }, [buildPatch, onSave]);

  const hasUnsavedChanges = useMemo(() => {
    const providerMeta = provider.meta ?? {};
    const providerContextLength = (providerMeta.context_length as number) ?? 128000;
    const providerMaxTokens = (providerMeta.max_tokens as number) ?? 4096;
    const providerTimeout = (providerMeta.timeout as number) ?? 30;
    const providerRetryCount = (providerMeta.retry_count as number) ?? 3;
    const providerStreaming = (providerMeta.streaming as boolean) ?? true;
    const providerClaudeMainModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.main);
    const providerClaudeHaikuModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.haiku);
    const providerClaudeSonnetModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.sonnet);
    const providerClaudeOpusModel = getMetaString(providerMeta, CLAUDE_MODEL_META_KEYS.opus);
    const modelCatalog = buildModelCatalog([
      ...models,
      defaultModel,
      claudeMainModel,
      claudeHaikuModel,
      claudeSonnetModel,
      claudeOpusModel,
    ]);
    const modelsChanged =
      modelCatalog.length !== provider.models.length ||
      modelCatalog.some((model, index) => model !== provider.models[index]);

    return (
      name !== provider.name ||
      baseUrlOpenai !== provider.base_url_openai ||
      baseUrlAnthropic !== provider.base_url_anthropic ||
      modelsUrl !== (provider.models_url ?? "") ||
      notes !== (provider.notes ?? "") ||
      apiKey !== provider.api_key ||
      modelsChanged ||
      defaultModel.trim() !== provider.default_model ||
      claudeMainModel !== providerClaudeMainModel ||
      claudeHaikuModel !== providerClaudeHaikuModel ||
      claudeSonnetModel !== providerClaudeSonnetModel ||
      claudeOpusModel !== providerClaudeOpusModel ||
      contextLength !== providerContextLength ||
      maxTokens !== providerMaxTokens ||
      timeout !== providerTimeout ||
      retryCount !== providerRetryCount ||
      streaming !== providerStreaming
    );
  }, [
    name,
    baseUrlOpenai,
    baseUrlAnthropic,
    modelsUrl,
    notes,
    apiKey,
    models,
    defaultModel,
    claudeMainModel,
    claudeHaikuModel,
    claudeSonnetModel,
    claudeOpusModel,
    contextLength,
    maxTokens,
    timeout,
    retryCount,
    streaming,
    provider,
  ]);

  useEffect(() => {
    if (!hasUnsavedChanges || saving) return;

    const timer = window.setTimeout(() => {
      void persistChanges();
    }, AUTO_SAVE_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [
    hasUnsavedChanges,
    saving,
    persistChanges,
    name,
    baseUrlOpenai,
    baseUrlAnthropic,
    modelsUrl,
    notes,
    apiKey,
    models,
    defaultModel,
    claudeMainModel,
    claudeHaikuModel,
    claudeSonnetModel,
    claudeOpusModel,
    contextLength,
    maxTokens,
    timeout,
    retryCount,
    streaming,
  ]);

  const handleCopyApiKey = useCallback(async () => {
    if (!apiKey || typeof navigator === "undefined" || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(apiKey);
    } catch {
      // Clipboard permission failures should not block editing.
    }
  }, [apiKey]);

  const connectionSummary = useMemo(() => {
    const label = name.trim() || "未命名供应商";
    const endpoint = baseUrlOpenai.trim() || baseUrlAnthropic.trim();
    const endpointPart = endpoint ? ` · ${endpoint}` : "";
    const keyPart = apiKey.trim()
      ? ` · ••••${apiKey.trim().slice(-4)}`
      : " · 未配置 API Key";
    return `${label}${endpointPart}${keyPart}`;
  }, [name, baseUrlOpenai, baseUrlAnthropic, apiKey]);

  const claudeSummary = useMemo(
    () => claudeMainModel.trim() || claudeSonnetModel.trim() || "未设置主模型",
    [claudeMainModel, claudeSonnetModel],
  );

  const codexSummary = useMemo(() => defaultModel.trim() || "未设置模型", [defaultModel]);

  const advancedSummary = useMemo(() => {
    return `${contextLength.toLocaleString()} ctx · ${maxTokens.toLocaleString()} tokens · ${streaming ? "流式" : "非流式"}`;
  }, [contextLength, maxTokens, streaming]);

  return (
    <div className="space-y-6 pb-1">
      <ConfigSectionGroup title="连接配置" description="API 地址、密钥与模型列表获取">
        <ConfigCollapseSection
          id="provider-connection"
          icon={Globe2}
          title="连接信息"
          summary={connectionSummary}
          expanded={connectionExpanded}
          onToggle={() => setConnectionExpanded((prev) => !prev)}
        >

        <div className="grid gap-4 xl:grid-cols-2">
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">供应商名称</label>
            <div className="relative">
              <Info className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. DeepSeek"
                className="pl-9"
              />
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">Base URL (OpenAI 兼容)</label>
            <div className="relative">
              <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <Input
                value={baseUrlOpenai}
                onChange={(e) => setBaseUrlOpenai(e.target.value)}
                placeholder="https://api.example.com/v1"
                className="pl-9"
              />
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">Base URL (Anthropic 兼容)</label>
            <div className="relative">
              <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <Input
                value={baseUrlAnthropic}
                onChange={(e) => setBaseUrlAnthropic(e.target.value)}
                placeholder="https://api.example.com/anthropic"
                className="pl-9"
              />
            </div>
          </div>

          <div className="space-y-1.5 xl:col-span-2">
            <div className="flex items-center justify-between gap-2">
              <label className="text-xs font-medium text-muted-foreground">获取模型 URL</label>
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => handleFetchModels(fetchTarget)}
                disabled={isFetchingModels || !modelsUrl.trim() || !apiKey.trim()}
                aria-label="获取可用模型列表"
              >
                {isFetchingModels ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <Download className="h-3.5 w-3.5" />
                )}
                {isFetchingModels ? "获取中..." : "获取模型列表"}
              </Button>
            </div>
            <div className="relative">
              <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <Input
                value={modelsUrl}
                onChange={(e) => setModelsUrl(e.target.value)}
                placeholder="https://api.example.com/v1/models"
                className="pl-9"
              />
            </div>
            <p className="text-xs text-muted-foreground">
              所有 Agent（Claude、Codex 等）共享这一获取模型链接，通常使用 OpenAI 兼容的 <code>/v1/models</code> 端点。
            </p>
          </div>

          <div className="space-y-1.5 xl:col-span-2">
            <label className="text-xs font-medium text-muted-foreground">备注（可选）</label>
            <textarea
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              placeholder="可选备注信息..."
              rows={2}
              className={cn(
                "flex min-h-9 w-full resize-none rounded-xl border border-input-border bg-input px-3 py-2 text-sm text-foreground shadow-sm backdrop-blur-sm placeholder:text-muted-foreground/70",
                "transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60",
                "disabled:cursor-not-allowed disabled:opacity-50",
              )}
            />
          </div>
        </div>

        <div className="space-y-1.5 border-t border-border/45 pt-4">
          <label className="text-xs font-medium text-muted-foreground">API Key</label>
          <div className="flex gap-2">
            <div className="relative min-w-0 flex-1">
              <KeyRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <Input
                type={showApiKey ? "text" : "password"}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder="sk-..."
                className="pl-9 pr-10"
              />
              <button
                type="button"
                onClick={() => setShowApiKey(!showApiKey)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground transition-colors hover:text-foreground cursor-pointer"
                aria-label={showApiKey ? "隐藏 API Key" : "显示 API Key"}
              >
                {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </button>
            </div>
            <Button
              type="button"
              variant="outline"
              size="icon-sm"
              onClick={handleCopyApiKey}
              disabled={!apiKey}
              aria-label="复制 API Key"
            >
              <Copy className="h-4 w-4" />
            </Button>
          </div>
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <ShieldCheck className="h-3.5 w-3.5 text-primary/80" />
            API Key 仅保存在本地，不会上传到任何服务器
          </p>
        </div>
        </ConfigCollapseSection>
      </ConfigSectionGroup>

      <ConfigSectionGroup title="模型配置" description="为 Claude Code、Codex 等工具指定默认模型">
        <ConfigCollapseSection
          id="provider-claude"
          title="Claude"
          summary={claudeSummary}
          expanded={claudeExpanded}
          onToggle={() => setClaudeExpanded((prev) => !prev)}
          iconSlot={
            <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-background/70 shadow-sm">
              <ClaudeIcon size={20} />
            </span>
          }
        >
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">主模型</span>
              <Input
                value={claudeMainModel}
                onChange={(e) => setClaudeMainModel(e.target.value)}
                placeholder={LATEST_CLAUDE_MODELS.main}
                aria-label="Claude 主模型"
                list="claude-model-options"
              />
            </label>
            <label className="space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">Haiku 默认模型</span>
              <Input
                value={claudeHaikuModel}
                onChange={(e) => setClaudeHaikuModel(e.target.value)}
                placeholder={LATEST_CLAUDE_MODELS.haiku}
                aria-label="Claude Haiku 默认模型"
                list="claude-model-options"
              />
            </label>
            <label className="space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">Sonnet 默认模型</span>
              <Input
                value={claudeSonnetModel}
                onChange={(e) => setClaudeSonnetModel(e.target.value)}
                placeholder={LATEST_CLAUDE_MODELS.sonnet}
                aria-label="Claude Sonnet 默认模型"
                list="claude-model-options"
              />
            </label>
            <label className="space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">Opus 默认模型</span>
              <Input
                value={claudeOpusModel}
                onChange={(e) => setClaudeOpusModel(e.target.value)}
                placeholder={LATEST_CLAUDE_MODELS.opus}
                aria-label="Claude Opus 默认模型"
                list="claude-model-options"
              />
            </label>
          </div>
          <datalist id="claude-model-options">
            {claudeModelOptions.map((model) => (
              <option key={model} value={model} />
            ))}
          </datalist>
        </ConfigCollapseSection>

        <ConfigCollapseSection
          id="provider-codex"
          title="Codex"
          summary={codexSummary}
          expanded={codexExpanded}
          onToggle={() => setCodexExpanded((prev) => !prev)}
          iconSlot={
            <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border/60 bg-background/70 shadow-sm">
              <CodexIcon size={20} />
            </span>
          }
        >
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">模型名称</span>
            <Input
              value={defaultModel}
              onChange={(e) => setDefaultModel(e.target.value)}
              placeholder="gpt-5.4"
              aria-label="Codex 模型名称"
              list="codex-model-options"
            />
          </label>
          <datalist id="codex-model-options">
            {codexModelOptions.map((model) => (
              <option key={model} value={model} />
            ))}
          </datalist>
        </ConfigCollapseSection>

        {fetchError && !modelFetchStatus && (
          <div className="rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
            获取模型列表失败: {fetchError.message}
          </div>
        )}

        {modelFetchStatus && modelFetchStatus.count > 0 && (
          <div className="rounded-lg border border-primary/25 bg-primary/5 px-3 py-2 text-xs text-primary">
            已更新 {modelFetchStatus.count} 个{modelFetchStatus.target === "claude" ? " Claude" : " Codex"}{" "}
            模型候选，可在输入框下拉中选择。
          </div>
        )}

        {modelFetchStatus && modelFetchStatus.count === 0 && (
          <div className="rounded-lg border border-border/60 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
            未发现可用模型
          </div>
        )}
      </ConfigSectionGroup>

      <ConfigSectionGroup title="高级选项" description="上下文、超时与流式输出">
        <ConfigCollapseSection
          id="provider-advanced"
          icon={SlidersHorizontal}
          title="高级设置"
          summary={advancedSummary}
          expanded={advancedExpanded}
          onToggle={() => setAdvancedExpanded((prev) => !prev)}
        >
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">上下文长度</label>
            <Input
              type="number"
              value={contextLength}
              onChange={(e) => setContextLength(Number(e.target.value))}
              min={1024}
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">最大输出 Tokens</label>
            <Input
              type="number"
              value={maxTokens}
              onChange={(e) => setMaxTokens(Number(e.target.value))}
              min={1}
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">超时 (秒)</label>
            <Input
              type="number"
              value={timeout}
              onChange={(e) => setTimeout_(Number(e.target.value))}
              min={1}
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground">重试次数</label>
            <Input
              type="number"
              value={retryCount}
              onChange={(e) => setRetryCount(Number(e.target.value))}
              min={0}
            />
          </div>
        </div>

        <div className="flex items-center justify-between rounded-xl border border-border/45 bg-background/35 px-3 py-2">
          <div className="space-y-0.5">
            <label className="text-xs font-medium text-foreground">流式输出</label>
            <p className="text-xs text-muted-foreground">启用 SSE 流式响应</p>
          </div>
          <Switch checked={streaming} onCheckedChange={setStreaming} aria-label="流式输出开关" />
        </div>
        </ConfigCollapseSection>
      </ConfigSectionGroup>

      {saving ? <p className="text-right text-xs text-muted-foreground">正在保存…</p> : null}
    </div>
  );
}

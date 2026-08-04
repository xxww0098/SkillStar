import { ArrowLeft, ArrowRight, Cable, Check, Eraser, Sparkles, Trash2 } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import { ProviderBrandIcon } from "../../../../../components/shared/ProviderBrandIcon";
import { Button } from "../../../../../components/ui/button";
import { cn } from "../../../../../lib/utils";
import { getAgent, type ProviderToolId } from "../../../lib/agentRegistry";
import { activeEntry } from "../../../lib/toolBinding";
import type { EditorDetailStyle, PrototypeHubData } from "./types";

type EditorPageProps = {
  data: PrototypeHubData;
  detailStyle?: EditorDetailStyle;
};

/**
 * Full main-pane editor — replaces hub content. Lives next to ModelsSidebar;
 * never a side drawer. `detailStyle` varies with B1/B2/B3.
 */
export function EditorPage({ data, detailStyle = "tabs" }: EditorPageProps) {
  const { overlay } = data;
  if (overlay.type === "delete") {
    return <EditPage data={data} providerId={overlay.providerId} detailStyle={detailStyle} />;
  }
  if (overlay.type === "none") return null;
  if (overlay.type === "create") return <CreatePage data={data} />;
  if (overlay.type === "edit") {
    return <EditPage data={data} providerId={overlay.providerId} tab={overlay.tab} detailStyle={detailStyle} />;
  }
  if (overlay.type === "agent-settings") {
    return <AgentSettingsPage data={data} toolId={overlay.toolId} />;
  }
  return <AppAiPage data={data} />;
}

function PageChrome({
  title,
  subtitle,
  onBack,
  trailing,
  children,
  wide,
}: {
  title: string;
  subtitle?: string;
  onBack: () => void;
  trailing?: ReactNode;
  children: ReactNode;
  wide?: boolean;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden">
      <div data-tauri-drag-region className="h-4 shrink-0" aria-hidden />
      <header className="flex shrink-0 items-center gap-3 border-b border-border/50 px-5 py-3">
        <Button size="sm" variant="ghost" onClick={onBack} className="gap-1.5">
          <ArrowLeft className="h-3.5 w-3.5" />
          Back
        </Button>
        <div className="min-w-0 flex-1">
          <h1 className="truncate text-base font-semibold">{title}</h1>
          {subtitle ? <p className="truncate text-[11px] text-muted-foreground">{subtitle}</p> : null}
        </div>
        {trailing}
      </header>
      <div className="ss-page-scroll">
        <div className={cn("mx-auto w-full space-y-4 px-5 py-5", wide ? "max-w-5xl" : "max-w-2xl")}>{children}</div>
      </div>
    </div>
  );
}

type CreatePreset = {
  id: string;
  name: string;
  color: string;
  hint: string;
  openai: string;
  anthropic: string;
};

/** Third-party API presets only — Official is a column-header switch, not a createable row. */
const CREATE_PRESETS: CreatePreset[] = [
  {
    id: "deepseek",
    name: "DeepSeek",
    color: "#4D6BFE",
    hint: "OpenAI 兼容 · 适合 Codex / OpenCode / Pi",
    openai: "https://api.deepseek.com/v1",
    anthropic: "",
  },
  {
    id: "kimi",
    name: "Kimi",
    color: "#5B45E0",
    hint: "OpenAI 兼容",
    openai: "https://api.moonshot.cn/v1",
    anthropic: "",
  },
  {
    id: "glm",
    name: "智谱 GLM",
    color: "#3366FF",
    hint: "OpenAI + Anthropic 双端点",
    openai: "https://open.bigmodel.cn/api/paas/v4",
    anthropic: "https://open.bigmodel.cn/api/anthropic",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    color: "#6366F1",
    hint: "聚合路由 · OpenAI 兼容",
    openai: "https://openrouter.ai/api/v1",
    anthropic: "",
  },
  {
    id: "custom",
    name: "自定义 / OpenAI 兼容",
    color: "#64748B",
    hint: "自填 Base URL 与 Key",
    openai: "",
    anthropic: "",
  },
];

function CreatePage({ data }: { data: PrototypeHubData }) {
  const [presetId, setPresetId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [openaiUrl, setOpenaiUrl] = useState("");
  const [anthropicUrl, setAnthropicUrl] = useState("");
  const [defaultModel, setDefaultModel] = useState("");
  const [saving, setSaving] = useState(false);

  const preset = CREATE_PRESETS.find((p) => p.id === presetId) ?? null;

  const pickPreset = (id: string) => {
    const next = CREATE_PRESETS.find((p) => p.id === id)!;
    setPresetId(id);
    setName(next.name === "自定义 / OpenAI 兼容" ? "" : next.name);
    setOpenaiUrl(next.openai);
    setAnthropicUrl(next.anthropic);
    setDefaultModel("");
    setApiKey("");
  };

  if (!preset) {
    return (
      <PageChrome
        title="添加 Provider"
        subtitle="选择预设 → 填写连接 → 回到矩阵绑定 Agent"
        onBack={data.closeOverlay}
      >
        <p className="text-xs text-muted-foreground">
          矩阵行仅第三方 Provider。Claude / Codex 官方账号在表头图标上切换，不在此创建。
        </p>
        <div className="space-y-2">
          <p className="text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
            API Key 预设
          </p>
          {CREATE_PRESETS.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => pickPreset(p.id)}
              className="flex w-full items-center gap-3 rounded-xl border border-border/50 px-3 py-3 text-left hover:bg-muted/40"
            >
              <span className="h-9 w-9 shrink-0 rounded-lg" style={{ background: p.color }} />
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">{p.name}</span>
                <span className="block text-[11px] text-muted-foreground">{p.hint}</span>
              </span>
              <ArrowRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
            </button>
          ))}
        </div>
      </PageChrome>
    );
  }

  const canSave = Boolean(name.trim() && apiKey.trim() && (openaiUrl.trim() || anthropicUrl.trim()));

  return (
    <PageChrome
      title={`新建 · ${preset.name}`}
      subtitle="填写连接信息后写入 Provider store，再回矩阵绑定 Agent"
      onBack={() => setPresetId(null)}
      trailing={
        <Button
          size="sm"
          disabled={!canSave || saving}
          onClick={() => {
            void (async () => {
              setSaving(true);
              try {
                const created = await data.createProvider({
                  id: "",
                  name: name.trim(),
                  api_key: apiKey.trim(),
                  base_url_openai: openaiUrl.trim(),
                  base_url_anthropic: anthropicUrl.trim(),
                  models_url: "",
                  models: defaultModel.trim() ? [defaultModel.trim()] : [],
                  default_model: defaultModel.trim(),
                  preset_id: preset.id === "custom" ? undefined : preset.id,
                  icon_color: preset.color,
                });
                data.setOverlay({ type: "edit", providerId: created.id });
              } catch {
                // Mutation toast handled by api layer.
              } finally {
                setSaving(false);
              }
            })();
          }}
        >
          {saving ? "创建中…" : "创建并继续"}
        </Button>
      }
    >
      <div className="space-y-4">
        <label className="block space-y-1">
          <span className="text-[11px] font-medium text-muted-foreground">显示名称</span>
          <input
            className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 text-sm"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="例如 DeepSeek"
          />
        </label>
        <label className="block space-y-1">
          <span className="text-[11px] font-medium text-muted-foreground">API Key</span>
          <input
            className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 font-mono text-sm"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-..."
          />
        </label>
        <label className="block space-y-1">
          <span className="text-[11px] font-medium text-muted-foreground">OpenAI 兼容端点</span>
          <input
            className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 font-mono text-xs"
            value={openaiUrl}
            onChange={(e) => setOpenaiUrl(e.target.value)}
            placeholder="https://…/v1"
          />
          <span className="text-[10px] text-muted-foreground">Codex / OpenCode / Pi 使用</span>
        </label>
        <label className="block space-y-1">
          <span className="text-[11px] font-medium text-muted-foreground">Anthropic 兼容端点</span>
          <input
            className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 font-mono text-xs"
            value={anthropicUrl}
            onChange={(e) => setAnthropicUrl(e.target.value)}
            placeholder="可选 · Claude Code"
          />
          <span className="text-[10px] text-muted-foreground">Claude Code 使用；可与 OpenAI 端点并存</span>
        </label>
        <label className="block space-y-1">
          <span className="text-[11px] font-medium text-muted-foreground">默认模型</span>
          <input
            className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 font-mono text-xs"
            value={defaultModel}
            onChange={(e) => setDefaultModel(e.target.value)}
            placeholder="可选 · 绑定时代入"
          />
        </label>
        <p className="rounded-lg border border-dashed border-border/55 px-3 py-2 text-[11px] text-muted-foreground">
          创建后打开编辑抽屉完善模型列表，再回矩阵绑定对应 Agent 列。
        </p>
      </div>
    </PageChrome>
  );
}

function EditPage({
  data,
  providerId,
  tab,
  detailStyle,
}: {
  data: PrototypeHubData;
  providerId: string;
  tab?: string;
  detailStyle: EditorDetailStyle;
}) {
  const provider = data.providers.find((p) => p.id === providerId);
  const boundAgents = data.agents.filter(
    (a) => activeEntry(data.toolActivations[a.toolId])?.provider_id === providerId,
  );

  return (
    <PageChrome
      title={provider?.name ?? providerId}
      subtitle={
        detailStyle === "tabs"
          ? "B1 editor · tabbed"
          : detailStyle === "sections"
            ? "B2 editor · stacked sections"
            : "B3 editor · split connection / models+agents"
      }
      onBack={data.closeOverlay}
      wide={detailStyle === "split"}
      trailing={
        <Button
          size="sm"
          variant="ghost"
          className="text-destructive"
          onClick={() => data.setOverlay({ type: "delete", providerId })}
        >
          <Trash2 className="mr-1.5 h-3.5 w-3.5" />
          Delete
        </Button>
      }
    >
      <div className="flex items-center gap-2">
        {provider ? (
          <ProviderBrandIcon
            presetId={provider.preset_id}
            providerName={provider.name}
            iconColor={provider.icon_color}
            size="sm"
          />
        ) : (
          <Cable className="h-4 w-4 text-primary" />
        )}
        <p className="text-xs text-muted-foreground">
          {boundAgents.length > 0
            ? `Bound to ${boundAgents.map((a) => a.displayName).join(", ")}`
            : "Not bound to any agent"}
        </p>
      </div>

      {detailStyle === "tabs" ? <TabsEditor data={data} providerId={providerId} tab={tab} /> : null}
      {detailStyle === "sections" ? <SectionsEditor data={data} providerId={providerId} /> : null}
      {detailStyle === "split" ? <SplitEditor data={data} providerId={providerId} /> : null}
    </PageChrome>
  );
}

function TabsEditor({ data, providerId, tab }: { data: PrototypeHubData; providerId: string; tab?: string }) {
  const provider = data.providers.find((p) => p.id === providerId);
  const tabs = ["connection", "models", "advanced", "diagnostics"] as const;
  const activeTab = tab ?? "connection";
  return (
    <>
      <div className="grid grid-cols-4 gap-1 rounded-xl border border-border/50 p-1">
        {tabs.map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => data.setOverlay({ type: "edit", providerId, tab: id })}
            className={cn(
              "rounded-lg px-2 py-1.5 text-[11px] font-semibold capitalize",
              activeTab === id ? "bg-background shadow-sm" : "text-muted-foreground hover:text-foreground",
            )}
          >
            {id}
          </button>
        ))}
      </div>
      {activeTab === "connection" ? <ConnectionFields data={data} providerId={providerId} /> : null}
      {activeTab === "models" ? (
        <ModelList
          models={provider?.models?.length ? provider.models : [provider?.default_model ?? ""].filter(Boolean)}
          defaultModel={provider?.default_model}
          onPick={(m) => data.stub("setDefaultModel", { providerId, model: m })}
        />
      ) : null}
      {activeTab === "advanced" ? (
        <p className="text-xs text-muted-foreground">Advanced stubs: timeout, wire API, headers…</p>
      ) : null}
      {activeTab === "diagnostics" ? (
        <Button size="sm" variant="outline" onClick={() => data.stub("testConnection", { providerId })}>
          Run probe
        </Button>
      ) : null}
      <Button size="sm" onClick={data.closeOverlay}>
        Done
      </Button>
    </>
  );
}

function SectionsEditor({ data, providerId }: { data: PrototypeHubData; providerId: string }) {
  const provider = data.providers.find((p) => p.id === providerId);
  return (
    <div className="space-y-5">
      <section className="space-y-3 rounded-xl border border-border/50 p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Connection</h3>
        <ConnectionFields data={data} providerId={providerId} />
      </section>
      <section className="space-y-3 rounded-xl border border-border/50 p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Models</h3>
        <ModelList
          models={provider?.models?.length ? provider.models : [provider?.default_model ?? ""].filter(Boolean)}
          defaultModel={provider?.default_model}
          onPick={(m) => data.stub("setDefaultModel", { providerId, model: m })}
        />
      </section>
      <section className="space-y-2 rounded-xl border border-border/50 p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Diagnostics</h3>
        <Button size="sm" variant="outline" onClick={() => data.stub("testConnection", { providerId })}>
          Test connection
        </Button>
      </section>
      <Button size="sm" onClick={data.closeOverlay}>
        Done
      </Button>
    </div>
  );
}

function SplitEditor({ data, providerId }: { data: PrototypeHubData; providerId: string }) {
  const provider = data.providers.find((p) => p.id === providerId);
  const models = provider?.models?.length ? provider.models : [provider?.default_model ?? ""].filter(Boolean);
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <section className="space-y-3 rounded-xl border border-border/50 p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Connection</h3>
        <ConnectionFields data={data} providerId={providerId} />
        <Button size="sm" variant="outline" onClick={() => data.stub("testConnection", { providerId })}>
          Test
        </Button>
      </section>
      <section className="space-y-3 rounded-xl border border-border/50 p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">Models & agents</h3>
        <ModelList
          models={models}
          defaultModel={provider?.default_model}
          onPick={(m) => data.stub("setDefaultModel", { providerId, model: m })}
        />
        <div className="space-y-1.5 border-t border-border/40 pt-3">
          {data.agents.map((agent) => {
            const entry = activeEntry(data.toolActivations[agent.toolId]);
            const bound = entry?.provider_id === providerId;
            return (
              <div key={agent.toolId} className="flex items-center justify-between gap-2 text-xs">
                <span>{agent.displayName}</span>
                {bound ? (
                  <span className="font-mono text-[10px] text-emerald-400">
                    {entry?.model || provider?.default_model}
                  </span>
                ) : (
                  <Button
                    size="xs"
                    variant="ghost"
                    onClick={() => data.stub("activate", { toolId: agent.toolId, providerId })}
                  >
                    Bind
                  </Button>
                )}
              </div>
            );
          })}
        </div>
      </section>
      <div className="lg:col-span-2">
        <Button size="sm" onClick={data.closeOverlay}>
          Done
        </Button>
      </div>
    </div>
  );
}

function ConnectionFields({ data, providerId }: { data: PrototypeHubData; providerId: string }) {
  const provider = data.providers.find((p) => p.id === providerId);
  return (
    <div className="space-y-3">
      <label className="block space-y-1">
        <span className="text-[11px] font-medium text-muted-foreground">API key</span>
        <input
          className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 text-sm"
          defaultValue={provider?.api_key ? "••••••••" : ""}
          placeholder="sk-..."
          onBlur={(e) => data.stub("setApiKey", { providerId, length: e.target.value.length })}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-[11px] font-medium text-muted-foreground">OpenAI base URL</span>
        <input
          className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 font-mono text-xs"
          defaultValue={provider?.base_url_openai ?? ""}
          onBlur={(e) => data.stub("setOpenaiUrl", { providerId, url: e.target.value })}
        />
      </label>
      <label className="block space-y-1">
        <span className="text-[11px] font-medium text-muted-foreground">Anthropic base URL</span>
        <input
          className="h-10 w-full rounded-lg border border-border/60 bg-background px-3 font-mono text-xs"
          defaultValue={provider?.base_url_anthropic ?? ""}
          onBlur={(e) => data.stub("setAnthropicUrl", { providerId, url: e.target.value })}
        />
      </label>
    </div>
  );
}

function ModelList({
  models,
  defaultModel,
  onPick,
}: {
  models: string[];
  defaultModel?: string;
  onPick: (m: string) => void;
}) {
  if (models.length === 0) {
    return <p className="text-xs text-muted-foreground">No models yet — fetch from /models in real UI.</p>;
  }
  return (
    <div className="flex flex-wrap gap-1.5">
      {models.map((m) => (
        <button
          key={m}
          type="button"
          onClick={() => onPick(m)}
          className={cn(
            "rounded-lg border px-2.5 py-1 font-mono text-[11px]",
            m === defaultModel
              ? "border-primary/40 bg-primary/10 text-foreground"
              : "border-border/50 text-muted-foreground hover:text-foreground",
          )}
        >
          {m}
        </button>
      ))}
    </div>
  );
}

function AgentSettingsPage({ data, toolId }: { data: PrototypeHubData; toolId: ProviderToolId }) {
  const agent = getAgent(toolId);
  const entry = activeEntry(data.toolActivations[toolId]);
  const provider = entry ? data.providers.find((p) => p.id === entry.provider_id) : null;
  return (
    <PageChrome
      title={`${agent?.displayName ?? toolId} settings`}
      subtitle={agent?.configPathDisplay}
      onBack={data.closeOverlay}
    >
      <div className="rounded-xl border border-border/50 p-4">
        <p className="text-[11px] text-muted-foreground">Active binding</p>
        <p className="mt-1 text-sm font-medium">
          {provider?.name ?? "—"} · {entry?.model || "no model"}
        </p>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button size="sm" variant="outline" onClick={() => data.stub("resync", { toolId })}>
          Resync config files
        </Button>
        {provider ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => data.setOverlay({ type: "edit", providerId: provider.id })}
          >
            Edit provider
          </Button>
        ) : null}
      </div>
    </PageChrome>
  );
}

/**
 * App AI is not a tool_sync Agent column — it consumes a Models provider
 * (or Settings Ollama). Header entry is the D1 home for this consumer.
 */
function AppAiPage({ data }: { data: PrototypeHubData }) {
  const [bound, setBound] = useState<{ providerId: string; protocol: "claude" | "codex" } | null>(() => {
    const first = data.providers[0];
    return first ? { providerId: first.id, protocol: first.base_url_anthropic ? "claude" : "codex" } : null;
  });
  const [ollamaActive, setOllamaActive] = useState(false);

  const boundProvider = useMemo(
    () => (bound ? (data.providers.find((p) => p.id === bound.providerId) ?? null) : null),
    [bound, data.providers],
  );

  return (
    <PageChrome
      title="应用内 AI"
      subtitle="摘要 · 技能推荐（与 CLI Agent 独立）"
      onBack={data.closeOverlay}
      trailing={
        bound && !ollamaActive ? (
          <Button
            size="sm"
            variant="ghost"
            className="text-muted-foreground"
            onClick={() => {
              setBound(null);
              data.stub("clearAppAi");
            }}
          >
            <Eraser className="mr-1.5 h-3.5 w-3.5" />
            清除绑定
          </Button>
        ) : null
      }
    >
      <div className="flex items-start gap-3 rounded-xl border border-border/55 bg-card/50 px-4 py-3">
        <span className="flex h-9 w-9 items-center justify-center rounded-lg border border-primary/20 bg-primary/10">
          <Sparkles className="h-4 w-4 text-primary" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-semibold">当前来源</p>
            <span
              className={cn(
                "rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ring-1",
                ollamaActive || bound
                  ? "bg-primary/15 text-primary ring-primary/25"
                  : "bg-muted text-muted-foreground ring-border",
              )}
            >
              {ollamaActive ? "本地 Ollama" : bound ? "已绑定" : "未绑定"}
            </span>
          </div>
          {ollamaActive ? (
            <p className="mt-1 text-xs text-muted-foreground">
              当前由本地 Ollama 提供。Models hub 只绑定供应商；Ollama 在设置里切换。
            </p>
          ) : boundProvider ? (
            <p className="mt-1 truncate text-xs">
              <span className="font-medium">{boundProvider.name}</span>
              <span className="ml-1.5 text-muted-foreground">
                · {bound?.protocol === "claude" ? "Claude 协议" : "OpenAI 协议"}
              </span>
            </p>
          ) : (
            <p className="mt-1 text-xs text-muted-foreground">从下方供应商按协议绑定，供应用内摘要与技能推荐使用。</p>
          )}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <button
          type="button"
          onClick={() => {
            setOllamaActive(false);
            data.stub("appAiSource", { source: "models" });
          }}
          className={cn(
            "rounded-xl border px-3 py-3 text-left text-xs",
            !ollamaActive ? "border-primary/35 bg-primary/5" : "border-border/50 text-muted-foreground",
          )}
        >
          {!ollamaActive ? <Check className="mb-1 h-3.5 w-3.5 text-primary" /> : null}
          Models 供应商
          <span className="mt-0.5 block text-[10px] text-muted-foreground">本页配置</span>
        </button>
        <button
          type="button"
          onClick={() => {
            setOllamaActive(true);
            data.stub("appAiSource", { source: "ollama" });
          }}
          className={cn(
            "rounded-xl border px-3 py-3 text-left text-xs",
            ollamaActive ? "border-primary/35 bg-primary/5" : "border-border/50 text-muted-foreground",
          )}
        >
          {ollamaActive ? <Check className="mb-1 h-3.5 w-3.5 text-primary" /> : null}
          本地 Ollama
          <span className="mt-0.5 block text-[10px] text-muted-foreground">设置中管理</span>
        </button>
      </div>

      {ollamaActive ? (
        <div className="rounded-xl border border-border/50 bg-background/40 px-4 py-3 text-xs text-muted-foreground">
          Ollama 的 host / 模型表单在 Settings → AI。此处只声明「当前走本地」。
          <Button
            size="sm"
            variant="ghost"
            className="mt-2 h-7 gap-1 px-0 text-primary"
            onClick={() => data.stub("openSettingsAi")}
          >
            打开设置 <ArrowRight className="h-3 w-3" />
          </Button>
        </div>
      ) : (
        <section className="space-y-2">
          <h3 className="text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
            按协议绑定供应商
          </h3>
          {data.providers.length === 0 ? (
            <div className="rounded-xl border border-dashed border-border/55 px-4 py-6 text-center text-xs text-muted-foreground">
              暂无供应商。
              <Button
                size="sm"
                className="mt-3"
                onClick={() => data.setOverlay({ type: "create" })}
              >
                添加 Provider
              </Button>
            </div>
          ) : (
            data.providers.map((p) => {
              const isBound = boundProvider?.id === p.id;
              const canClaude = Boolean(p.base_url_anthropic?.trim() || p.base_url_openai?.trim());
              const canCodex = Boolean(p.base_url_openai?.trim());
              return (
                <div
                  key={p.id}
                  className={cn(
                    "flex items-center gap-2 rounded-xl border px-3 py-2.5",
                    isBound ? "border-primary/35 bg-primary/[0.06]" : "border-border/45 bg-card/40",
                  )}
                >
                  <ProviderBrandIcon
                    presetId={p.preset_id}
                    providerName={p.name}
                    iconColor={p.icon_color}
                    size="sm"
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs font-medium">{p.name}</p>
                    <p className="truncate font-mono text-[10px] text-muted-foreground">
                      {p.default_model || "—"}
                    </p>
                  </div>
                  <button
                    type="button"
                    disabled={!canClaude}
                    title="以 Claude 协议绑定"
                    onClick={() => {
                      setBound({ providerId: p.id, protocol: "claude" });
                      data.stub("bindAppAi", { providerId: p.id, protocol: "claude" });
                    }}
                    className={cn(
                      "rounded-md border px-2 py-1 text-[10px] font-medium",
                      isBound && bound?.protocol === "claude"
                        ? "border-primary/50 bg-primary/10 text-primary"
                        : "border-border/55 text-muted-foreground hover:text-foreground",
                      !canClaude && "pointer-events-none opacity-40",
                    )}
                  >
                    Claude
                  </button>
                  <button
                    type="button"
                    disabled={!canCodex}
                    title="以 OpenAI 协议绑定"
                    onClick={() => {
                      setBound({ providerId: p.id, protocol: "codex" });
                      data.stub("bindAppAi", { providerId: p.id, protocol: "codex" });
                    }}
                    className={cn(
                      "rounded-md border px-2 py-1 text-[10px] font-medium",
                      isBound && bound?.protocol === "codex"
                        ? "border-primary/50 bg-primary/10 text-primary"
                        : "border-border/55 text-muted-foreground hover:text-foreground",
                      !canCodex && "pointer-events-none opacity-40",
                    )}
                  >
                    OpenAI
                  </button>
                </div>
              );
            })
          )}
        </section>
      )}

    </PageChrome>
  );
}

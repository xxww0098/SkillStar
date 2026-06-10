import { motion } from "framer-motion";
import {
  ArrowRight,
  Check,
  ChevronDown,
  ExternalLink,
  Loader2,
  Plug,
  RefreshCw,
  Settings2,
  Sparkles,
  Unplug,
} from "lucide-react";
import { Popover } from "radix-ui";
import { useCallback, useMemo, useState } from "react";
import { Button } from "../../../../components/ui/button";
import { ExternalAnchor } from "../../../../components/ui/ExternalAnchor";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat, ToolActivation } from "../../../../types";
import { AgentToolIcon, type AgentToolIconId } from "../shared/AgentToolIcon";
import { ProviderBrandIcon } from "../shared/ProviderBrandIcon";

export interface AgentHeroAgent {
  toolId: string;
  displayName: string;
  iconId: AgentToolIconId;
  requiredUrlField: "openai" | "anthropic";
  installDocsUrl: string;
  /** Tagline shown under the title. */
  tagline: string;
}

export interface AgentHeroCardProps {
  agent: AgentHeroAgent;
  providers: ProviderEntryFlat[];
  activation: ToolActivation | null;
  installed: boolean;
  installLoading: boolean;
  onActivate: (providerId: string, model?: string) => Promise<void>;
  onDeactivate: () => Promise<void>;
  onAddProvider: () => void;
  onOpenDrawer: (providerId: string) => void;
}

type Status = "connected" | "disconnected" | "misconfigured" | "not-installed";

const STATUS_STYLE: Record<
  Status,
  {
    chip: string;
    label: string;
    border: string;
    glow: string;
  }
> = {
  connected: {
    chip: "bg-emerald-500/15 text-emerald-400 ring-emerald-500/20",
    label: "已连接",
    border: "border-emerald-500/25",
    glow: "shadow-[0_30px_60px_-32px_rgba(16,185,129,0.35)]",
  },
  disconnected: {
    chip: "bg-muted text-muted-foreground ring-border",
    label: "未连接",
    border: "border-border/55",
    glow: "shadow-[0_24px_60px_-40px_var(--color-shadow)]",
  },
  misconfigured: {
    chip: "bg-amber-500/15 text-amber-400 ring-amber-500/20",
    label: "配置异常",
    border: "border-amber-500/30",
    glow: "shadow-[0_24px_60px_-32px_rgba(245,158,11,0.30)]",
  },
  "not-installed": {
    chip: "bg-amber-500/15 text-amber-400 ring-amber-500/20",
    label: "未安装",
    border: "border-amber-500/20",
    glow: "shadow-none",
  },
};

export function AgentHeroCard({
  agent,
  providers,
  activation,
  installed,
  installLoading,
  onActivate,
  onDeactivate,
  onAddProvider,
  onOpenDrawer,
}: AgentHeroCardProps) {
  const [busy, setBusy] = useState(false);
  const [providerOpen, setProviderOpen] = useState(false);
  const [modelOpen, setModelOpen] = useState(false);

  const boundProvider = useMemo(() => {
    if (!activation?.provider_id) return null;
    return providers.find((p) => p.id === activation.provider_id) ?? null;
  }, [activation, providers]);

  const compatibleProviders = useMemo(
    () =>
      providers.filter((p) => (agent.requiredUrlField === "anthropic" ? !!p.base_url_anthropic : !!p.base_url_openai)),
    [providers, agent.requiredUrlField],
  );

  const availableModels = useMemo(() => {
    if (!boundProvider) return [] as string[];
    const models = [...(boundProvider.models ?? [])];
    if (boundProvider.default_model && !models.includes(boundProvider.default_model)) {
      models.unshift(boundProvider.default_model);
    }
    return models;
  }, [boundProvider]);

  const status: Status = useMemo(() => {
    if (!installed && !installLoading) return "not-installed";
    if (!activation?.provider_id) return "disconnected";
    if (boundProvider) {
      const requiredMissing =
        agent.requiredUrlField === "anthropic" ? !boundProvider.base_url_anthropic : !boundProvider.base_url_openai;
      if (requiredMissing) return "misconfigured";
    }
    return "connected";
  }, [installed, installLoading, activation, boundProvider, agent.requiredUrlField]);

  const style = STATUS_STYLE[status];
  const currentModel = activation?.model || boundProvider?.default_model || "";

  const handlePickProvider = useCallback(
    async (providerId: string) => {
      setProviderOpen(false);
      setBusy(true);
      try {
        const provider = providers.find((p) => p.id === providerId);
        await onActivate(providerId, provider?.default_model || undefined);
      } finally {
        setBusy(false);
      }
    },
    [providers, onActivate],
  );

  const handlePickModel = useCallback(
    async (model: string) => {
      if (!activation?.provider_id) return;
      setModelOpen(false);
      setBusy(true);
      try {
        await onActivate(activation.provider_id, model);
      } finally {
        setBusy(false);
      }
    },
    [activation, onActivate],
  );

  const handleResync = useCallback(async () => {
    if (!activation?.provider_id) return;
    setBusy(true);
    try {
      await onActivate(activation.provider_id, currentModel || undefined);
    } finally {
      setBusy(false);
    }
  }, [activation, currentModel, onActivate]);

  const handleDisconnect = useCallback(async () => {
    setBusy(true);
    try {
      await onDeactivate();
    } finally {
      setBusy(false);
    }
  }, [onDeactivate]);

  return (
    <motion.section
      initial={{ opacity: 0, y: 14 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.3, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "relative flex h-full flex-col rounded-3xl border bg-card/75 backdrop-blur-2xl",
        "transition-transform duration-300 hover:-translate-y-0.5",
        style.border,
        style.glow,
      )}
    >
      {/* Accent strip */}
      <span
        aria-hidden
        className={cn(
          "absolute inset-x-0 top-0 h-[2px]",
          status === "connected" && "bg-gradient-to-r from-emerald-400/30 via-emerald-400/70 to-emerald-400/30",
          status === "misconfigured" && "bg-gradient-to-r from-amber-400/30 via-amber-400/70 to-amber-400/30",
          status === "not-installed" && "bg-gradient-to-r from-amber-400/20 via-amber-400/45 to-amber-400/20",
          status === "disconnected" && "bg-gradient-to-r from-primary/10 via-primary/35 to-primary/10",
        )}
      />

      <header className="flex items-start gap-3 px-5 pt-5">
        <AgentToolIcon toolId={agent.iconId} size="md" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h3 className="truncate text-base font-bold text-foreground">{agent.displayName}</h3>
            <span
              className={cn(
                "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ring-1",
                style.chip,
              )}
            >
              {style.label}
            </span>
          </div>
          <p className="mt-0.5 text-[11px] text-muted-foreground">{agent.tagline}</p>
        </div>
      </header>

      <div className="flex-1 px-5 pt-4 pb-3 space-y-3">
        {status === "not-installed" ? (
          <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
            <p>未检测到本机安装。</p>
            <ExternalAnchor
              href={agent.installDocsUrl}
              className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
            >
              查看安装文档 <ExternalLink className="h-3 w-3" />
            </ExternalAnchor>
          </div>
        ) : null}

        {status === "misconfigured" && boundProvider ? (
          <div className="rounded-xl border border-amber-500/20 bg-amber-500/[0.06] px-3 py-2.5 text-[11px] text-amber-400">
            此供应商缺少 {agent.requiredUrlField === "anthropic" ? "Anthropic" : "OpenAI"} 兼容端点。
            <button
              type="button"
              onClick={() => onOpenDrawer(boundProvider.id)}
              className="mt-1 inline-flex items-center gap-1 font-medium text-primary hover:underline"
            >
              去配置 <ArrowRight className="h-3 w-3" />
            </button>
          </div>
        ) : null}

        {/* Provider Picker */}
        <div className="space-y-1.5">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">供应商</label>
          <Popover.Root open={providerOpen} onOpenChange={setProviderOpen}>
            <Popover.Trigger asChild>
              <button
                type="button"
                disabled={busy || status === "not-installed"}
                className={cn(
                  "flex w-full items-center gap-2 rounded-xl border bg-input px-3 py-2 text-left text-xs transition",
                  "hover:border-primary/30 focus:outline-none focus:ring-2 focus:ring-primary/40",
                  "disabled:cursor-not-allowed disabled:opacity-50",
                  status === "connected" && "border-emerald-500/25 bg-emerald-500/[0.04]",
                  status === "misconfigured" && "border-amber-500/25 bg-amber-500/[0.04]",
                  status !== "connected" && status !== "misconfigured" && "border-input-border",
                )}
              >
                {boundProvider ? (
                  <>
                    <ProviderBrandIcon
                      presetId={boundProvider.preset_id}
                      providerName={boundProvider.name}
                      iconColor={boundProvider.icon_color}
                      size="xs"
                    />
                    <span className="min-w-0 flex-1 truncate font-medium text-foreground">{boundProvider.name}</span>
                  </>
                ) : (
                  <>
                    <span className="flex h-5 w-5 items-center justify-center rounded-md bg-muted text-muted-foreground">
                      <Plug className="h-3 w-3" />
                    </span>
                    <span className="min-w-0 flex-1 truncate text-muted-foreground">选择供应商…</span>
                  </>
                )}
                {busy ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
                ) : (
                  <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                )}
              </button>
            </Popover.Trigger>
            <Popover.Portal>
              <Popover.Content
                align="start"
                sideOffset={6}
                className="z-[60] w-[var(--radix-popover-trigger-width)] min-w-[240px] rounded-xl border border-border/60 bg-card/95 p-1.5 shadow-[0_20px_60px_-24px_var(--color-shadow)] backdrop-blur-2xl"
              >
                <div className="max-h-72 overflow-y-auto">
                  {compatibleProviders.length === 0 ? (
                    <div className="px-3 py-3 text-center text-[11px] text-muted-foreground">暂无兼容供应商</div>
                  ) : (
                    compatibleProviders.map((p) => (
                      <button
                        key={p.id}
                        type="button"
                        onClick={() => void handlePickProvider(p.id)}
                        className={cn(
                          "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs",
                          "transition hover:bg-primary/10",
                          activation?.provider_id === p.id && "bg-primary/10 text-primary",
                        )}
                      >
                        <ProviderBrandIcon
                          presetId={p.preset_id}
                          providerName={p.name}
                          iconColor={p.icon_color}
                          size="xs"
                        />
                        <span className="min-w-0 flex-1 truncate font-medium text-foreground">{p.name}</span>
                        {activation?.provider_id === p.id ? <Check className="h-3 w-3 text-primary" /> : null}
                      </button>
                    ))
                  )}
                </div>
                <div className="border-t border-border/40 pt-1.5 mt-1.5">
                  <button
                    type="button"
                    onClick={() => {
                      setProviderOpen(false);
                      onAddProvider();
                    }}
                    className="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs text-primary transition hover:bg-primary/10"
                  >
                    <Plug className="h-3.5 w-3.5" />
                    新增供应商…
                  </button>
                </div>
              </Popover.Content>
            </Popover.Portal>
          </Popover.Root>
        </div>

        {/* Model Picker — only when connected */}
        {status === "connected" && availableModels.length > 0 ? (
          <div className="space-y-1.5">
            <label className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">模型</label>
            <Popover.Root open={modelOpen} onOpenChange={setModelOpen}>
              <Popover.Trigger asChild>
                <button
                  type="button"
                  disabled={busy}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-xl border border-input-border bg-input px-3 py-2 text-left text-xs transition",
                    "hover:border-primary/30 focus:outline-none focus:ring-2 focus:ring-primary/40",
                    "disabled:cursor-not-allowed disabled:opacity-50",
                  )}
                >
                  <Sparkles className="h-3.5 w-3.5 text-primary/80" />
                  <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">
                    {currentModel || "未选择模型"}
                  </span>
                  <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
                </button>
              </Popover.Trigger>
              <Popover.Portal>
                <Popover.Content
                  align="start"
                  sideOffset={6}
                  className="z-[60] w-[var(--radix-popover-trigger-width)] min-w-[240px] rounded-xl border border-border/60 bg-card/95 p-1.5 shadow-[0_20px_60px_-24px_var(--color-shadow)] backdrop-blur-2xl"
                >
                  <div className="max-h-72 overflow-y-auto">
                    {availableModels.map((m) => (
                      <button
                        key={m}
                        type="button"
                        onClick={() => void handlePickModel(m)}
                        className={cn(
                          "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left",
                          "transition hover:bg-primary/10",
                          m === currentModel && "bg-primary/10 text-primary",
                        )}
                      >
                        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">{m}</span>
                        {m === currentModel ? <Check className="h-3 w-3 text-primary" /> : null}
                      </button>
                    ))}
                  </div>
                </Popover.Content>
              </Popover.Portal>
            </Popover.Root>
          </div>
        ) : null}

        {status === "connected" && availableModels.length === 0 && boundProvider ? (
          <p className="text-[11px] text-amber-500">未拉取模型列表，请到供应商抽屉点击「拉取模型」。</p>
        ) : null}
      </div>

      <footer className="flex items-center gap-1 border-t border-border/40 bg-background/20 px-4 py-2.5">
        {status === "connected" ? (
          <>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={handleResync}
              disabled={busy}
              title="重新同步"
              className="text-muted-foreground hover:text-foreground"
            >
              <RefreshCw className={cn("h-3.5 w-3.5", busy && "animate-spin")} />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => boundProvider && onOpenDrawer(boundProvider.id)}
              title="详细配置"
              className="text-muted-foreground hover:text-foreground"
            >
              <Settings2 className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={handleDisconnect}
              disabled={busy}
              title="断开连接"
              className="ml-auto text-muted-foreground hover:text-destructive"
            >
              <Unplug className="h-3.5 w-3.5" />
            </Button>
          </>
        ) : (
          <Button variant="outline" size="sm" onClick={onAddProvider} className="ml-auto h-7 text-[11px]">
            <Plug className="mr-1.5 h-3 w-3" />
            新增并绑定
          </Button>
        )}
      </footer>
    </motion.section>
  );
}

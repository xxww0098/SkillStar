import { motion } from "framer-motion";
import { Activity, Loader2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { tauriInvoke } from "../../../../lib/ipc";
import { cn } from "../../../../lib/utils";
import type { ConnectionTestResult, ProviderEntryFlat, ToolActivation, ToolActivationsMap } from "../../../../types";
import { AgentToolIcon, type AgentToolIconId } from "../AgentToolIcon";
import { ProviderBrandIcon } from "../ProviderBrandIcon";

interface AgentDef {
  toolId: string;
  displayName: string;
  iconId: AgentToolIconId;
  requiredUrlField: "openai" | "anthropic";
}

const AGENTS: AgentDef[] = [
  { toolId: "claude-code", displayName: "Claude", iconId: "claude-code", requiredUrlField: "anthropic" },
  { toolId: "codex", displayName: "Codex", iconId: "codex", requiredUrlField: "openai" },
  { toolId: "opencode", displayName: "OpenCode", iconId: "opencode", requiredUrlField: "openai" },
  { toolId: "gemini", displayName: "Gemini CLI", iconId: "gemini", requiredUrlField: "openai" },
];

export interface HealthBarProps {
  providers: ProviderEntryFlat[];
  toolActivations: ToolActivationsMap;
  /**
   * Claude Desktop install status. Special-cased — Claude Desktop doesn't bind a provider,
   * so it sits alongside the provider-bound agent chips with its own state.
   */
  claudeDesktopInstalled?: boolean;
  claudeDesktopInstallLoading?: boolean;
  /** Click handler for the Claude Desktop chip — opens the MCP config drawer. */
  onOpenClaudeDesktopConfig?: () => void;
}

type Health = "ok" | "warn" | "off" | "testing" | "fail";

interface AgentHealth {
  agent: AgentDef;
  activation: ToolActivation | null;
  provider: ProviderEntryFlat | null;
  status: Health;
  result: ConnectionTestResult | null;
}

function dotClass(h: AgentHealth): string {
  const { status, result } = h;
  if (status === "off") return "bg-muted-foreground/30";
  if (status === "warn") return "bg-amber-400";
  if (status === "testing") return "bg-primary animate-pulse";
  if (status === "fail") return "bg-red-400";
  const ms = result?.latency_ms;
  if (ms == null) return "bg-emerald-400/60";
  if (ms < 500) return "bg-emerald-400";
  if (ms < 1500) return "bg-amber-400";
  return "bg-red-400";
}

function latencyTone(h: AgentHealth): string {
  const { status, result } = h;
  if (status === "warn") return "text-amber-500";
  if (status === "fail") return "text-destructive";
  if (status === "off") return "text-muted-foreground";
  const ms = result?.latency_ms;
  if (ms == null) return "text-muted-foreground";
  if (ms < 500) return "text-emerald-400";
  if (ms < 1500) return "text-amber-400";
  return "text-red-400";
}

function describe(h: AgentHealth): string {
  if (h.status === "testing") return "测速中…";
  if (h.status === "off") return "未连接";
  if (h.status === "warn") return h.agent.requiredUrlField === "anthropic" ? "缺 Anthropic 端点" : "缺 OpenAI 端点";
  if (h.status === "fail") {
    if (h.result?.status === "timeout") return "超时";
    if (h.result?.status === "auth_failed") return "鉴权失败";
    return "失败";
  }
  const ms = h.result?.latency_ms;
  return ms != null ? `${ms}ms` : "已连接";
}

export function HealthBar({
  providers,
  toolActivations,
  claudeDesktopInstalled,
  claudeDesktopInstallLoading = false,
  onOpenClaudeDesktopConfig,
}: HealthBarProps) {
  const [resultsByTool, setResultsByTool] = useState<Record<string, ConnectionTestResult | null>>({});
  const [testingByTool, setTestingByTool] = useState<Record<string, boolean>>({});
  const probedKeys = useRef<Set<string>>(new Set());

  const healths: AgentHealth[] = useMemo(() => {
    return AGENTS.map<AgentHealth>((agent) => {
      const activation = toolActivations[agent.toolId] ?? null;
      const provider = activation?.provider_id
        ? (providers.find((p) => p.id === activation.provider_id) ?? null)
        : null;
      const required =
        agent.requiredUrlField === "anthropic" ? provider?.base_url_anthropic : provider?.base_url_openai;
      const result = resultsByTool[agent.toolId] ?? null;

      let status: Health = "off";
      if (testingByTool[agent.toolId]) status = "testing";
      else if (!provider) status = "off";
      else if (!required) status = "warn";
      else if (result?.status === "ok") status = "ok";
      else if (result) status = "fail";
      else status = "ok"; // bound and configured, untested optimistic

      return { agent, activation, provider, status, result };
    });
  }, [providers, toolActivations, resultsByTool, testingByTool]);

  const runTest = useCallback(async (h: AgentHealth) => {
    if (!h.provider) return;
    const url = h.agent.requiredUrlField === "anthropic" ? h.provider.base_url_anthropic : h.provider.base_url_openai;
    if (!url || !h.provider.api_key) return;
    const format = h.agent.requiredUrlField === "anthropic" ? "anthropic" : "openai";
    setTestingByTool((prev) => ({ ...prev, [h.agent.toolId]: true }));
    try {
      const result = await tauriInvoke("test_provider_connection", {
        baseUrl: url,
        apiKey: h.provider.api_key,
        model: h.activation?.model ?? h.provider.default_model ?? "",
        format,
      });
      setResultsByTool((prev) => ({ ...prev, [h.agent.toolId]: result }));
    } catch (err) {
      setResultsByTool((prev) => ({
        ...prev,
        [h.agent.toolId]: {
          status: "network_error",
          error: err instanceof Error ? err.message : String(err),
        },
      }));
    } finally {
      setTestingByTool((prev) => ({ ...prev, [h.agent.toolId]: false }));
    }
  }, []);

  // Auto-probe once per (toolId, providerId) pair on mount or rebind.
  useEffect(() => {
    healths.forEach((h) => {
      if (!h.provider) return;
      if (h.status === "warn" || h.status === "testing") return;
      const key = `${h.agent.toolId}:${h.provider.id}`;
      if (probedKeys.current.has(key)) return;
      probedKeys.current.add(key);
      void runTest(h);
    });
  }, [healths, runTest]);

  const summary = useMemo(() => {
    const ok = healths.filter((h) => h.status === "ok").length;
    const warn = healths.filter((h) => h.status === "warn" || h.status === "fail").length;
    return { ok, warn, total: healths.length };
  }, [healths]);

  return (
    <motion.div
      initial={{ opacity: 0, y: -4 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "mx-auto w-full max-w-6xl rounded-xl border border-border/50 bg-card/70 px-4 py-3 backdrop-blur-2xl",
        "shadow-[0_18px_50px_-32px_var(--color-shadow)]",
      )}
    >
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2.5">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Activity className="h-3.5 w-3.5 text-primary/80" />
          <span className="font-medium text-foreground">健康总览</span>
          <span className="text-[11px] text-muted-foreground/75">
            {summary.ok}/{summary.total} 已连接{summary.warn > 0 ? ` · ${summary.warn} 异常` : ""}
          </span>
        </div>

        <div className="flex flex-1 flex-wrap items-center gap-2">
          {healths.map((h) => (
            <HealthChip key={h.agent.toolId} health={h} onTest={() => void runTest(h)} />
          ))}
          <ClaudeDesktopChip
            installed={claudeDesktopInstalled}
            loading={claudeDesktopInstallLoading}
            onClick={onOpenClaudeDesktopConfig}
          />
        </div>
      </div>
    </motion.div>
  );
}

function ClaudeDesktopChip({
  installed,
  loading,
  onClick,
}: {
  installed?: boolean;
  loading: boolean;
  onClick?: () => void;
}) {
  const isInstalled = !!installed;
  const clickable = isInstalled && !!onClick;
  return (
    <button
      type="button"
      onClick={clickable ? onClick : undefined}
      disabled={!clickable}
      title={isInstalled ? "编辑 MCP 配置" : "未安装 Claude Desktop"}
      className={cn(
        "group flex items-center gap-2 rounded-xl border px-3 py-1.5 text-xs transition",
        "border-border/55 bg-background/40 backdrop-blur-sm",
        clickable && "cursor-pointer hover:border-primary/30 hover:bg-card-hover",
        !clickable && "cursor-default opacity-90",
      )}
    >
      <AgentToolIcon toolId="claude-desktop" size="sm" />
      <span className="font-medium text-foreground">Claude Desktop</span>
      <span
        className={cn(
          "h-1.5 w-1.5 rounded-full",
          loading ? "bg-primary animate-pulse" : isInstalled ? "bg-emerald-400" : "bg-muted-foreground/30",
        )}
      />
      <span className={cn("ml-1 text-[11px]", isInstalled ? "text-emerald-400" : "text-muted-foreground")}>
        {loading ? <Loader2 className="inline h-3 w-3 animate-spin" /> : isInstalled ? "已安装" : "未安装"}
      </span>
    </button>
  );
}

function HealthChip({ health, onTest }: { health: AgentHealth; onTest: () => void }) {
  const { agent, provider, status } = health;
  const clickable = status === "ok" || status === "fail";
  return (
    <button
      type="button"
      onClick={clickable ? onTest : undefined}
      disabled={!clickable}
      className={cn(
        "group flex items-center gap-2 rounded-xl border px-3 py-1.5 text-xs transition",
        "border-border/55 bg-background/40 backdrop-blur-sm",
        clickable && "cursor-pointer hover:border-primary/30 hover:bg-card-hover",
        !clickable && "cursor-default opacity-90",
      )}
      title={clickable ? "点击测速" : describe(health)}
    >
      <AgentToolIcon toolId={agent.iconId} size="sm" />
      <span className="font-medium text-foreground">{agent.displayName}</span>
      <span className={cn("h-1.5 w-1.5 rounded-full", dotClass(health))} />
      {provider ? (
        <span className="flex items-center gap-1 text-muted-foreground">
          <ProviderBrandIcon
            presetId={provider.preset_id}
            providerName={provider.name}
            iconColor={provider.icon_color}
            size="xs"
          />
          <span className="max-w-[110px] truncate">{provider.name}</span>
        </span>
      ) : null}
      <span className={cn("ml-1 text-[11px]", latencyTone(health))}>
        {status === "testing" ? <Loader2 className="inline h-3 w-3 animate-spin" /> : describe(health)}
      </span>
    </button>
  );
}

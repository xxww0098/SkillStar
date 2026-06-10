import {
  Bot,
  Copy,
  Download,
  Eye,
  EyeOff,
  FileCog,
  Globe2,
  KeyRound,
  Loader2,
  Plug,
  Plus,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  StickyNote,
  Terminal,
} from "lucide-react";
import { memo, useCallback, useState } from "react";
import { Button } from "../../../../components/ui/button";
import { Input } from "../../../../components/ui/input";
import { Switch } from "../../../../components/ui/switch";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat, ProviderPatchFlat } from "../../../../types";
import { useToolActivations } from "../../hooks/useToolActivations";
import { ConflictWarnings } from "../diagnostics/ConflictWarnings";
import { ConnectionStatusPanel } from "../diagnostics/ConnectionStatusPanel";
import { EndpointSpeedPanel } from "../diagnostics/EndpointSpeedPanel";
import { AppAiProviderInline } from "../AppAiProviderInline";
import { ToolActivationPanel } from "../ToolActivationPanel";
import { ToolJsonConfigPanel } from "../ToolJsonConfigPanel";
import { AgentModelConfig } from "../providerForm/AgentModelConfig";
import { ConfigCollapseSection, fieldLabelClass } from "../providerForm/ProviderConfigPrimitives";
import { useProviderFormState, type ProviderSaveState } from "../providerForm/useProviderFormState";

export interface ProviderDrawerFormProps {
  provider: ProviderEntryFlat;
  onSave: (patch: ProviderPatchFlat) => Promise<void>;
  onSaveStateChange?: (state: ProviderSaveState) => void;
}

function ProviderDrawerFormInner({ provider, onSave, onSaveStateChange }: ProviderDrawerFormProps) {
  const form = useProviderFormState({ provider, onSave, onSaveStateChange });
  const { isActive: isToolActive } = useToolActivations(provider.id);

  const [openSection, setOpenSection] = useState<string | null>("connection");

  const toggle = useCallback((id: string) => {
    setOpenSection((prev) => (prev === id ? null : id));
  }, []);

  const handleCopyApiKey = useCallback(async () => {
    if (!form.apiKey || typeof navigator === "undefined" || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(form.apiKey);
    } catch {
      /* ignore */
    }
  }, [form.apiKey]);

  return (
    <div className="space-y-3">
      <ConflictWarnings providerId={provider.id} />
      <AppAiProviderInline provider={provider} />

      {/* ── Connection ───────────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-connection"
        icon={Plug}
        title="连接"
        summary={`${form.name || "未命名"} · ${form.apiKey ? "Key 已设置" : "Key 缺失"}`}
        expanded={openSection === "connection"}
        onToggle={() => toggle("connection")}
      >
        <div className="grid gap-3">
          <label className="space-y-1">
            <span className={fieldLabelClass}>名称</span>
            <Input value={form.name} onChange={(e) => form.setName(e.target.value)} placeholder="DeepSeek" />
          </label>

          <div className="space-y-1">
            <div className="flex items-center justify-between">
              <span className={fieldLabelClass}>API Key</span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={handleCopyApiKey}
                disabled={!form.apiKey}
                className="h-6 px-2 text-[11px] text-muted-foreground"
              >
                <Copy className="mr-1 h-3 w-3" />
                复制
              </Button>
            </div>
            <div className="relative">
              <KeyRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <Input
                type={form.showApiKey ? "text" : "password"}
                value={form.apiKey}
                onChange={(e) => form.setApiKey(e.target.value)}
                placeholder="sk-..."
                className="pl-9 pr-10"
              />
              <button
                type="button"
                onClick={() => form.setShowApiKey(!form.showApiKey)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground hover:text-foreground"
                aria-label={form.showApiKey ? "隐藏" : "显示"}
              >
                {form.showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </button>
            </div>
          </div>

          <div className="space-y-1">
            <span className={fieldLabelClass}>OpenAI 兼容端点</span>
            <div className="relative">
              <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
              <Input
                value={form.baseUrlOpenai}
                onChange={(e) => form.setBaseUrlOpenai(e.target.value)}
                placeholder="https://api.example.com/v1"
                className="pl-9"
              />
            </div>
          </div>

          {form.showAnthropicUrl ? (
            <div className="space-y-1">
              <span className={fieldLabelClass}>Anthropic 兼容端点</span>
              <div className="relative">
                <Globe2 className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
                <Input
                  value={form.baseUrlAnthropic}
                  onChange={(e) => form.setBaseUrlAnthropic(e.target.value)}
                  placeholder="https://api.example.com/anthropic"
                  className="pl-9"
                />
              </div>
            </div>
          ) : (
            <Button
              type="button"
              variant="ghost"
              size="sm"
              className="h-8 w-fit gap-1.5 text-xs text-muted-foreground"
              onClick={() => form.setShowAnthropicUrl(true)}
            >
              <Plus className="h-3.5 w-3.5" />
              添加 Anthropic 端点（Claude Code）
            </Button>
          )}

          <div className="space-y-1">
            <div className="flex items-center justify-between">
              <span className={fieldLabelClass}>默认模型(Codex / OpenCode)</span>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 text-xs"
                onClick={() => void form.handleFetchModels()}
                disabled={form.isFetchingModels || !form.modelsUrl.trim() || !form.apiKey.trim()}
              >
                {form.isFetchingModels ? (
                  <Loader2 className="mr-1 h-3 w-3 animate-spin" />
                ) : (
                  <Download className="mr-1 h-3 w-3" />
                )}
                拉取模型
              </Button>
            </div>
            <Input
              value={form.defaultModel}
              onChange={(e) => form.setDefaultModel(e.target.value)}
              placeholder="codex-mini-latest"
              list="drawer-codex-models"
            />
            <datalist id="drawer-codex-models">
              {form.codexModelOptions.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
          </div>

          <p className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
            <ShieldCheck className="h-3.5 w-3.5 text-primary/80" />
            凭据仅保存在本机
          </p>
        </div>
      </ConfigCollapseSection>

      {/* ── Agent 同步 ───────────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-agents"
        icon={Bot}
        title="Agent 同步"
        summary={form.agentSummary}
        expanded={openSection === "agents"}
        onToggle={() => toggle("agents")}
      >
        <ToolActivationPanel
          providerId={provider.id}
          providerModels={form.models}
          modelCatalog={form.modelCatalog}
          defaultModel={form.defaultModel}
          baseUrlOpenai={form.baseUrlOpenai}
          baseUrlAnthropic={form.baseUrlAnthropic}
          showHeader={false}
          variant="compact"
        />
      </ConfigCollapseSection>

      {/* ── Codex 默认参数 ───────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-codex"
        icon={Terminal}
        title="Codex 默认参数"
        summary={`${form.codexWireApi === "responses" ? "Responses" : "Chat"} · ${form.codexAuthMode === "oauth" ? "OAuth" : "API Key"}`}
        expanded={openSection === "codex"}
        onToggle={() => toggle("codex")}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="space-y-1">
            <span className={fieldLabelClass}>API 格式</span>
            <select
              value={form.codexWireApi}
              onChange={(e) => form.setCodexWireApi(e.target.value as "responses" | "chat")}
              className={cn(
                "h-9 w-full rounded-xl border border-input-border bg-input px-3 text-xs text-foreground",
                "focus:outline-none focus:ring-2 focus:ring-primary/40",
              )}
            >
              <option value="responses">Responses API</option>
              <option value="chat">Chat Completions</option>
            </select>
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>认证模式</span>
            <select
              value={form.codexAuthMode}
              onChange={(e) => form.setCodexAuthMode(e.target.value as "api_key" | "oauth")}
              className={cn(
                "h-9 w-full rounded-xl border border-input-border bg-input px-3 text-xs text-foreground",
                "focus:outline-none focus:ring-2 focus:ring-primary/40",
              )}
            >
              <option value="api_key">API Key</option>
              <option value="oauth">OAuth (ChatGPT Plus/Pro)</option>
            </select>
          </label>
        </div>
        <p className="text-[11px] text-muted-foreground/80">
          写入 <code className="rounded bg-muted/50 px-1 py-0.5 font-mono text-[10px]">~/.codex/config.toml</code> 与{" "}
          <code className="rounded bg-muted/50 px-1 py-0.5 font-mono text-[10px]">~/.codex/auth.json</code> —{" "}
          <strong className="text-foreground/90">
            Codex CLI、`codex app` 桌面端与 VS Code / Cursor / Windsurf IDE 扩展共用此配置
          </strong>
          。
        </p>
      </ConfigCollapseSection>

      {/* ── Claude 模型映射 ─────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-models"
        icon={Sparkles}
        title="Claude 模型映射"
        summary={
          [form.claudeSonnetModel || form.claudeMainModel, form.claudeOpusModel, form.claudeHaikuModel]
            .filter(Boolean)
            .join(" · ") || "默认映射"
        }
        expanded={openSection === "models"}
        onToggle={() => toggle("models")}
      >
        <AgentModelConfig form={form} />
      </ConfigCollapseSection>

      {/* ── 运行参数 ─────────────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-runtime"
        icon={SlidersHorizontal}
        title="运行参数"
        summary={form.advancedSummary}
        expanded={openSection === "runtime"}
        onToggle={() => toggle("runtime")}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <label className="space-y-1">
            <span className={fieldLabelClass}>上下文</span>
            <Input
              type="number"
              value={form.contextLength}
              onChange={(e) => form.setContextLength(Number(e.target.value))}
              min={1024}
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>最大 Tokens</span>
            <Input
              type="number"
              value={form.maxTokens}
              onChange={(e) => form.setMaxTokens(Number(e.target.value))}
              min={1}
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>超时 (秒)</span>
            <Input
              type="number"
              value={form.timeout}
              onChange={(e) => form.setTimeout_(Number(e.target.value))}
              min={1}
            />
          </label>
          <label className="space-y-1">
            <span className={fieldLabelClass}>重试</span>
            <Input
              type="number"
              value={form.retryCount}
              onChange={(e) => form.setRetryCount(Number(e.target.value))}
              min={0}
            />
          </label>
        </div>
        <div className="flex items-center justify-between rounded-lg border border-border/45 bg-background/35 px-3 py-2">
          <span className="text-xs font-medium text-foreground">流式输出</span>
          <Switch checked={form.streaming} onCheckedChange={form.setStreaming} />
        </div>
      </ConfigCollapseSection>

      {/* ── 诊断与余额 ───────────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-status"
        icon={Sparkles}
        title="连接诊断与余额"
        summary="测速 / 深度测试 / 账户余额"
        expanded={openSection === "status"}
        onToggle={() => toggle("status")}
      >
        <ConnectionStatusPanel
          providerId={provider.id}
          presetId={provider.preset_id}
          apiKey={form.apiKey}
          baseUrlOpenai={form.baseUrlOpenai}
          baseUrlAnthropic={form.baseUrlAnthropic}
        />
      </ConfigCollapseSection>

      {/* ── 磁盘配置 ─────────────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-disk"
        icon={FileCog}
        title="磁盘配置文件"
        summary="查看 / 编辑 ~/.claude / ~/.codex 配置"
        expanded={openSection === "disk"}
        onToggle={() => toggle("disk")}
      >
        <ToolJsonConfigPanel providerId={provider.id} isToolActive={isToolActive} embedded />
      </ConfigCollapseSection>

      {/* ── 其它 ─────────────────────────────────────────────────────── */}
      <ConfigCollapseSection
        id="drawer-misc"
        icon={StickyNote}
        title="附加信息"
        summary="备注 · 模型列表 URL · 端点候选"
        expanded={openSection === "misc"}
        onToggle={() => toggle("misc")}
      >
        <label className="block space-y-1">
          <span className={fieldLabelClass}>模型列表 URL</span>
          <Input
            value={form.modelsUrl}
            onChange={(e) => form.setModelsUrl(e.target.value)}
            placeholder="https://api.example.com/v1/models"
          />
        </label>
        <label className="block space-y-1">
          <span className={fieldLabelClass}>备注</span>
          <textarea
            value={form.notes}
            onChange={(e) => form.setNotes(e.target.value)}
            rows={2}
            className={cn(
              "flex min-h-9 w-full resize-none rounded-xl border border-input-border bg-input px-3 py-2 text-sm",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
            )}
          />
        </label>
        <EndpointSpeedPanel
          urls={form.speedTestUrls}
          apiKey={form.apiKey}
          onApplyFastest={form.handleApplyFastestEndpoint}
        />
      </ConfigCollapseSection>
    </div>
  );
}

export const ProviderDrawerForm = memo(ProviderDrawerFormInner);

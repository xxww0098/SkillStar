import { AnimatePresence, motion } from "framer-motion";
import {
  AlertCircle,
  Check,
  ChevronDown,
  Download,
  ExternalLink,
  GripVertical,
  Loader2,
  MoreHorizontal,
  Save,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { cn } from "../../../lib/utils";
import { useModelFetch } from "../hooks/useModelFetch";
import type { ProviderEntry, ProviderMeta } from "../hooks/useModelProviders";
import type { ModelAppId } from "./AppCapsuleSwitcher";
import { ApiKeyInput } from "./shared/ApiKeyInput";
import { EndpointInput } from "./shared/EndpointInput";
import { ModelInput } from "./shared/ModelInput";
import { ProviderIcon } from "./shared/ProviderIcon";

interface ProviderCardProps {
  provider: ProviderEntry;
  isCurrent: boolean;
  expanded: boolean;
  appId: ModelAppId;
  appColor: string;
  onSwitch: () => void;
  onToggleExpand: () => void;
  onUpdate: (entry: ProviderEntry) => void;
  onDelete: () => void;
  onOpenWebsite?: () => void;
  // Drag handle callback (visual state is CSS-class driven from useDragReorder)
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
  // Drag id used by useDragReorder to identify this card
  dragId?: string;
  /** Read-only display mode: no expand/edit/switch/drag, only delete via menu */
  readOnly?: boolean;
}

export function ProviderCard({
  provider,
  isCurrent,
  expanded,
  appId,
  appColor,
  onSwitch,
  onToggleExpand,
  onUpdate,
  onDelete,
  onOpenWebsite,
  onDragHandlePointerDown,
  dragId,
  readOnly,
}: ProviderCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [draft, setDraft] = useState<ProviderEntry>(provider);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const deleteTimerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const modelFetch = useModelFetch();

  // Sync draft when provider changes externally or card collapses
  useEffect(() => {
    setDraft(JSON.parse(JSON.stringify(provider)));
    setShowAdvanced(false);
    setConfirmDelete(false);
    modelFetch.clear();
  }, [provider, modelFetch.clear]);

  useEffect(() => {
    if (!menuOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [menuOpen]);

  useEffect(() => {
    return () => {
      if (deleteTimerRef.current) clearTimeout(deleteTimerRef.current);
    };
  }, []);

  // Check if API key is configured
  const hasApiKey = (() => {
    const cfg = provider.settingsConfig as Record<string, unknown>;
    if (appId === "claude") {
      const env = cfg?.env as Record<string, unknown> | undefined;
      return !!(env?.ANTHROPIC_AUTH_TOKEN || env?.ANTHROPIC_API_KEY);
    }
    if (appId === "gemini") {
      const env = cfg?.env as Record<string, unknown> | undefined;
      return !!env?.GEMINI_API_KEY;
    }
    if (appId === "codex") {
      const auth = cfg?.auth as Record<string, unknown> | undefined;
      // Official codex uses browser auth, no API key needed
      if (provider.category === "official" && !auth?.OPENAI_API_KEY) return true;
      return !!auth?.OPENAI_API_KEY;
    }
    if (appId === "opencode") {
      const auth = cfg?.auth as Record<string, unknown> | undefined;
      if (auth) return !!auth.key || auth.type === "oauth";

      const p = cfg?.provider as Record<string, Record<string, unknown>> | undefined;
      if (!p) return false;
      const first = Object.values(p)[0];
      const options = first?.options as Record<string, unknown> | undefined;
      return !!options?.apiKey;
    }
    return true;
  })();

  // Extract display URL from settingsConfig
  const displayUrl = (() => {
    const cfg = provider.settingsConfig as Record<string, unknown>;
    const env = cfg?.env as Record<string, unknown> | undefined;
    if (env?.ANTHROPIC_BASE_URL) return env.ANTHROPIC_BASE_URL as string;
    if (env?.GOOGLE_GEMINI_BASE_URL) return env.GOOGLE_GEMINI_BASE_URL as string;
    const options = cfg?.options as Record<string, unknown> | undefined;
    if (options?.baseURL) return options.baseURL as string;
    const configStr = cfg?.config as string | undefined;
    if (configStr) {
      const match = configStr.match(/base_url\s*=\s*"([^"]+)"/);
      if (match) return match[1];
    }
    return provider.websiteUrl || provider.notes || "";
  })();

  // ── Draft updaters ──────────────────────────────────────────

  const updateClaudeEnv = useCallback((key: string, value: string) => {
    setDraft((prev) => {
      const env = { ...((prev.settingsConfig.env as Record<string, unknown>) || {}) };
      env[key] = value;
      return { ...prev, settingsConfig: { ...prev.settingsConfig, env } };
    });
  }, []);

  const updateGeminiEnv = useCallback((key: string, value: string) => {
    setDraft((prev) => {
      const env = { ...((prev.settingsConfig.env as Record<string, unknown>) || {}) };
      env[key] = value;
      return { ...prev, settingsConfig: { ...prev.settingsConfig, env } };
    });
  }, []);

  const updateCodexAuth = useCallback((key: string, value: string) => {
    setDraft((prev) => {
      const auth = { ...((prev.settingsConfig.auth as Record<string, unknown>) || {}) };
      auth[key] = value;
      return { ...prev, settingsConfig: { ...prev.settingsConfig, auth } };
    });
  }, []);

  const updateCodexConfig = useCallback((text: string) => {
    setDraft((prev) => ({
      ...prev,
      settingsConfig: { ...prev.settingsConfig, config: text },
    }));
  }, []);

  const handleSave = () => {
    onUpdate(draft);
    onToggleExpand();
  };

  const handleDeleteClick = () => {
    if (!confirmDelete) {
      setConfirmDelete(true);
      deleteTimerRef.current = setTimeout(() => setConfirmDelete(false), 3000);
      return;
    }
    onDelete();
    setMenuOpen(false);
  };

  // ── Meta helpers ──────────────────────────────────────────────────

  const updateMeta = useCallback((patch: Partial<ProviderMeta>) => {
    setDraft((prev) => ({
      ...prev,
      meta: { ...(prev.meta || {}), ...patch },
    }));
  }, []);

  const getApiKeyField = () => draft.meta?.apiKeyField || "ANTHROPIC_AUTH_TOKEN";

  const handleOpenCodeTest = useCallback(() => {
    const auth = draft.settingsConfig.auth as Record<string, unknown> | undefined;
    const tempKey = (draft.settingsConfig as Record<string, unknown>).tempKey as string | undefined;
    const apiKey = tempKey || (auth?.key as string) || "";
    const baseURL = draft.meta?.baseURL || "";

    if (!baseURL) {
      toast.error("此卡片（可能为自定义供应商）缺乏预设的 Base URL，无法一键进行连通性测试。");
      return;
    }

    if (baseURL.includes("moonshot.cn") || baseURL.includes("dashscope.aliyuncs.com")) {
      toast.info("由于此官方服务商 API 暂不支持拉取模型列表测试，请直接点击保存即可使用。");
      return;
    }

    modelFetch.fetchModels(baseURL, apiKey, false);
  }, [draft, modelFetch]);

  // ── Inline field renderers ──────────────────────────────────

  const renderClaudeFields = () => {
    const env = (draft.settingsConfig.env as Record<string, unknown>) || {};
    const apiKeyField = getApiKeyField();
    const apiFormat = draft.meta?.apiFormat || "anthropic";

    return (
      <div className="space-y-3.5">
        <ApiKeyInput
          value={
            (env[apiKeyField] as string) ||
            (env.ANTHROPIC_AUTH_TOKEN as string) ||
            (env.ANTHROPIC_API_KEY as string) ||
            ""
          }
          onChange={(v) => updateClaudeEnv(apiKeyField, v)}
          apiKeyUrl={provider.apiKeyUrl}
        />
        <EndpointInput
          value={(env.ANTHROPIC_BASE_URL as string) || ""}
          onChange={(v) => updateClaudeEnv("ANTHROPIC_BASE_URL", v)}
        />

        <ModelInput
          label="主模型"
          value={(env.ANTHROPIC_MODEL as string) || ""}
          onChange={(v) => updateClaudeEnv("ANTHROPIC_MODEL", v)}
          fetchedModels={modelFetch.models}
          fetchingModels={modelFetch.loading}
        />

        {/* Advanced: API format, auth field, model mappings */}
        <button
          type="button"
          onClick={() => setShowAdvanced(!showAdvanced)}
          className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
        >
          <ChevronDown className={cn("w-3 h-3 transition-transform duration-200", showAdvanced && "rotate-180")} />
          高级设置
        </button>
        <AnimatePresence>
          {showAdvanced && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.15 }}
              className="overflow-hidden"
            >
              <div className="space-y-3 pt-1">
                {/* API Format + Auth Field */}
                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">API 格式</span>
                    <select
                      value={apiFormat}
                      onChange={(e) => updateMeta({ apiFormat: e.target.value as ProviderMeta["apiFormat"] })}
                      className="w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                    >
                      <option value="anthropic">Anthropic Messages</option>
                      <option value="openai_chat">OpenAI Chat</option>
                      <option value="openai_responses">OpenAI Responses</option>
                    </select>
                  </div>
                  <div className="space-y-1.5">
                    <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">认证字段</span>
                    <select
                      value={apiKeyField}
                      onChange={(e) => {
                        const newField = e.target.value as ProviderMeta["apiKeyField"];
                        const oldField = apiKeyField;
                        // Swap the key in env
                        if (oldField !== newField) {
                          const oldValue = (env[oldField] as string) || "";
                          updateClaudeEnv(newField!, oldValue);
                          updateClaudeEnv(oldField, "");
                        }
                        updateMeta({ apiKeyField: newField });
                      }}
                      className="w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                    >
                      <option value="ANTHROPIC_AUTH_TOKEN">ANTHROPIC_AUTH_TOKEN</option>
                      <option value="ANTHROPIC_API_KEY">ANTHROPIC_API_KEY</option>
                    </select>
                  </div>
                </div>

                {/* Reasoning Model */}
                <ModelInput
                  label="推理模型"
                  value={(env.ANTHROPIC_REASONING_MODEL as string) || ""}
                  onChange={(v) => updateClaudeEnv("ANTHROPIC_REASONING_MODEL", v)}
                  placeholder="claude-sonnet-4..."
                  fetchedModels={modelFetch.models}
                />

                {/* Model mappings 3-col */}
                <div className="grid grid-cols-3 gap-3">
                  <ModelInput
                    label="Haiku"
                    value={(env.ANTHROPIC_DEFAULT_HAIKU_MODEL as string) || ""}
                    onChange={(v) => updateClaudeEnv("ANTHROPIC_DEFAULT_HAIKU_MODEL", v)}
                    placeholder="claude-haiku-..."
                    fetchedModels={modelFetch.models}
                  />
                  <ModelInput
                    label="Sonnet"
                    value={(env.ANTHROPIC_DEFAULT_SONNET_MODEL as string) || ""}
                    onChange={(v) => updateClaudeEnv("ANTHROPIC_DEFAULT_SONNET_MODEL", v)}
                    placeholder="claude-sonnet-..."
                    fetchedModels={modelFetch.models}
                  />
                  <ModelInput
                    label="Opus"
                    value={(env.ANTHROPIC_DEFAULT_OPUS_MODEL as string) || ""}
                    onChange={(v) => updateClaudeEnv("ANTHROPIC_DEFAULT_OPUS_MODEL", v)}
                    placeholder="claude-opus-..."
                    fetchedModels={modelFetch.models}
                  />
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    );
  };

  const renderCodexFields = () => {
    const auth = (draft.settingsConfig.auth as Record<string, unknown>) || {};
    const configText = (draft.settingsConfig.config as string) || "";
    // Extract base_url and model from TOML for display
    const baseUrlMatch = configText.match(/base_url\s*=\s*"([^"]+)"/);
    const modelMatch = configText.match(/^model\s*=\s*"([^"]+)"/m);

    // Helper to ensure custom providers generate proper TOML blocks dynamically
    const getEnsuredConfig = (currentText: string) => {
      if (provider.category === "official") return currentText;
      if (currentText.includes("model_provider")) return currentText;

      const safeId =
        provider.id
          .toLowerCase()
          .replace(/[^a-z0-9_]/g, "_")
          .replace(/^_+|_+$/g, "") || "custom";
      return `model_provider = "${safeId}"\nmodel = "gpt-5.4"\n\n[model_providers.${safeId}]\nname = "${provider.name}"\nbase_url = ""\nrequires_openai_auth = true`;
    };

    return (
      <div className="space-y-3.5">
        <ApiKeyInput
          value={(auth.OPENAI_API_KEY as string) || ""}
          onChange={(v) => updateCodexAuth("OPENAI_API_KEY", v)}
          apiKeyUrl={provider.apiKeyUrl}
        />

        {provider.category === "official" ? (
          <EndpointInput value="https://api.openai.com/v1" onChange={() => {}} readOnly={true} />
        ) : (
          <EndpointInput
            value={baseUrlMatch?.[1] || ""}
            onChange={(v) => {
              let newText = getEnsuredConfig(configText);
              if (/base_url\s*=\s*"[^"]*"/.test(newText)) {
                newText = newText.replace(/base_url\s*=\s*"[^"]*"/, `base_url = "${v}"`);
              } else {
                newText += `\nbase_url = "${v}"`;
              }
              updateCodexConfig(newText);
            }}
          />
        )}

        {/* Extracted fields for quick access */}
        <ModelInput
          label="当前模型"
          value={modelMatch?.[1] || ""}
          onChange={(v) => {
            const newText = getEnsuredConfig(configText);
            if (/^model\s*=\s*"[^"]*"/m.test(newText)) {
              updateCodexConfig(newText.replace(/^model\s*=\s*"[^"]*"/m, `model = "${v}"`));
            } else {
              // No model field yet — prepend it
              updateCodexConfig(`model = "${v}"\n${newText}`);
            }
          }}
          placeholder="gpt-5.4"
          fetchedModels={modelFetch.models}
          fetchingModels={modelFetch.loading}
        />
      </div>
    );
  };

  const renderGeminiFields = () => {
    const env = (draft.settingsConfig.env as Record<string, unknown>) || {};
    return (
      <div className="space-y-3.5">
        <ApiKeyInput
          value={(env.GEMINI_API_KEY as string) || ""}
          onChange={(v) => updateGeminiEnv("GEMINI_API_KEY", v)}
          apiKeyUrl={provider.apiKeyUrl}
        />
        <EndpointInput
          value={(env.GOOGLE_GEMINI_BASE_URL as string) || ""}
          onChange={(v) => updateGeminiEnv("GOOGLE_GEMINI_BASE_URL", v)}
        />
        <ModelInput
          label="主模型"
          value={(env.GEMINI_MODEL as string) || ""}
          onChange={(v) => updateGeminiEnv("GEMINI_MODEL", v)}
          fetchedModels={modelFetch.models}
          fetchingModels={modelFetch.loading}
        />
      </div>
    );
  };

  const renderOpenCodeFields = () => {
    // If it's the new native auth wrapper (already saved)
    if (draft.settingsConfig.auth) {
      const auth = draft.settingsConfig.auth as Record<string, unknown>;
      const isEnv = auth.type === "env";
      return (
        <div className="space-y-3.5">
          <div className="px-3 py-2.5 rounded-lg bg-emerald-500/10 border border-emerald-500/20 flex flex-col gap-1.5">
            <span className="text-[11px] font-medium text-emerald-600 flex items-center gap-1.5">
              <Check className="w-3.5 h-3.5" /> 已通过 OpenCode {isEnv ? "系统环境变量" : "内置授权 (Connect)"}
            </span>
            <p className="text-[10px] text-emerald-600/80 leading-relaxed font-mono">
              {isEnv
                ? `Var: ${auth.key}`
                : auth.type === "api"
                  ? `Key: ${String(auth.key || "").substring(0, 8)}...`
                  : `Auth Type: ${auth.type}`}
            </p>

            <div className="flex items-center gap-2 mt-2 pt-2 border-t border-emerald-500/10">
              <button
                type="button"
                onClick={handleOpenCodeTest}
                disabled={modelFetch.loading || !draft.meta?.baseURL}
                className="px-2 py-1 bg-emerald-500/20 hover:bg-emerald-500/30 text-emerald-700 text-[10px] rounded hover:shadow-sm transition-colors disabled:opacity-50 flex items-center gap-1.5"
              >
                {modelFetch.loading ? <Loader2 className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
                测试连通性
              </button>
              <span className="text-[10px] text-emerald-600/60">
                {draft.meta?.baseURL ? "发送一次请求验证密钥状态" : "缺少预设URL"}
              </span>
            </div>
            {modelFetch.error && (
              <div className="text-[10px] text-destructive mt-1.5 bg-destructive/10 p-1.5 rounded">
                {modelFetch.error}
              </div>
            )}
            {!modelFetch.error && modelFetch.models.length > 0 && (
              <div className="text-[10px] text-emerald-700 font-medium mt-1.5 bg-emerald-500/20 p-1.5 rounded flex items-center gap-1">
                <Check className="w-3 h-3" />
                连通成功！检测到 {modelFetch.models.length} 个可用模型
              </div>
            )}
          </div>
          <div className="text-[11px] text-muted-foreground leading-relaxed p-2 bg-muted/30 rounded-md">
            {isEnv
              ? `${provider.name} 的配置拦截自你 Mac 系统中的 ${auth.key} 终端环境变量，已被 OpenCode 强制关联。为了解决冲突，你现在可以点击右上角垃圾篓，SkillStar 会为你将其添加到系统黑名单中屏蔽。`
              : `${provider.name} 已经作为内建服务商配置到本地授权环境中。此连接由命令行原生接管，如需重新绑定请移除后重新添加。`}
          </div>
        </div>
      );
    }

    // Ephemeral addition mode (recently added from Catalog, not yet saved)
    const tempKey = ((draft.settingsConfig as Record<string, unknown>).tempKey as string) || "";
    return (
      <div className="space-y-3.5">
        <ApiKeyInput
          value={tempKey}
          onChange={(v) => {
            setDraft((prev) => ({
              ...prev,
              settingsConfig: { ...prev.settingsConfig, tempKey: v },
            }));
          }}
          apiKeyUrl={provider.apiKeyUrl}
        />

        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={handleOpenCodeTest}
              disabled={modelFetch.loading || !draft.meta?.baseURL || !tempKey}
              className="px-3 py-1.5 bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-600 text-[11px] rounded-lg border border-emerald-500/20 flex items-center gap-1.5 transition-colors disabled:opacity-50"
            >
              {modelFetch.loading ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <Download className="w-3.5 h-3.5" />
              )}
              测试连通性
            </button>
            <span className="text-[10px] text-muted-foreground/80">
              {draft.meta?.baseURL ? "在保存前发送一次请求验证密钥" : "缺少预设URL"}
            </span>
          </div>

          {modelFetch.error && (
            <div className="text-[11px] text-destructive bg-destructive/10 p-2 rounded-lg">{modelFetch.error}</div>
          )}
          {!modelFetch.error && modelFetch.models.length > 0 && (
            <div className="text-[11px] text-emerald-600 font-medium bg-emerald-500/10 p-2 rounded-lg flex items-center gap-1.5">
              <Check className="w-3.5 h-3.5" />
              连通成功！检测到 {modelFetch.models.length} 个可用模型，现在可以点击保存了
            </div>
          )}
        </div>

        <div className="text-[11px] text-muted-foreground leading-relaxed p-2.5 bg-muted/40 rounded-lg">
          请输入服务商 API Key。可以先测试连通性，通过后点击下方「保存」即可持久化注入到 OpenCode CLI 原生安全配置中。
        </div>
      </div>
    );
  };

  // ── Category badge label ──────────────────────────────────

  const categoryLabel =
    provider.category === "official"
      ? "官方"
      : provider.category === "cn_official"
        ? "国产"
        : provider.category === "cloud_provider"
          ? "云服务"
          : provider.category === "aggregator"
            ? "聚合"
            : provider.category === "third_party"
              ? "第三方"
              : null;

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, scale: 0.95 }}
      transition={{ duration: 0.2 }}
      {...(dragId ? { "data-drag-card-id": dragId } : {})}
      className={cn(
        "group relative rounded-2xl border transition-[border-color,background-color,box-shadow] duration-200",
        isCurrent ? "border-2 shadow-lg" : "border-border/70 hover:border-border",
        expanded || menuOpen
          ? "bg-card/90 backdrop-blur-xl shadow-xl z-[100]"
          : "bg-card/60 backdrop-blur-sm hover:bg-card/80 hover:shadow-md z-10",
      )}
      style={{
        borderColor: isCurrent ? `${appColor}60` : undefined,
        zIndex: expanded || menuOpen ? 100 : 10,
      }}
    >
      {/* Active gradient glow */}
      {isCurrent && (
        <div
          className="absolute inset-0 rounded-2xl pointer-events-none opacity-[0.06]"
          style={{
            background: `linear-gradient(135deg, ${appColor} 0%, transparent 50%)`,
          }}
        />
      )}

      <div
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (readOnly) return;
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onToggleExpand();
          }
        }}
        className={cn(
          "relative w-full flex items-center gap-3 px-4 py-3.5 select-none text-left",
          readOnly ? "cursor-default" : "cursor-pointer",
          !readOnly && expanded && "border-b border-border/50",
        )}
        onClick={readOnly ? undefined : onToggleExpand}
      >
        {/* Drag handle — hidden in readOnly mode */}
        {!readOnly && (
          <>
            <div
              className="flex items-center justify-center p-1 -m-1 opacity-20 hover:opacity-100 transition-opacity cursor-grab active:cursor-grabbing text-foreground"
              style={{ touchAction: "none" }}
              onPointerDown={(e) => {
                e.stopPropagation();
                e.preventDefault();
                onDragHandlePointerDown?.(e);
              }}
              onClick={(e) => e.stopPropagation()}
            >
              <GripVertical className="w-4 h-4 text-muted-foreground hover:text-foreground transition-colors" />
            </div>
          </>
        )}

        {/* Provider icon */}
        <div
          className={cn(
            "w-9 h-9 rounded-xl flex items-center justify-center shrink-0 border transition-all duration-200",
            isCurrent ? "border-transparent shadow-sm" : "border-border/50",
          )}
          style={{
            backgroundColor: `${provider.iconColor || appColor}${isCurrent ? "20" : "10"}`,
          }}
        >
          <ProviderIcon
            name={provider.name}
            fallbackColor={provider.iconColor || appColor}
            size="w-5 h-5"
            className="transition-transform duration-200 group-hover:scale-110"
          />
        </div>

        {/* Name + URL */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="text-sm font-semibold text-foreground leading-tight truncate">
              {provider.name.replace(/^Google \((.+)\)$/, "$1")}
            </h3>
            {isCurrent && (
              <span
                className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-md text-[10px] font-semibold text-white"
                style={{ backgroundColor: appColor }}
              >
                <Check className="w-2.5 h-2.5" />
                当前
              </span>
            )}
            {categoryLabel && provider.category !== "custom" && (
              <span className="px-1.5 py-0.5 rounded-md bg-muted/60 text-[10px] text-muted-foreground font-medium">
                {categoryLabel}
              </span>
            )}
            {!hasApiKey && (
              <span title="未配置 API Key">
                <AlertCircle className="w-3.5 h-3.5 text-amber-500 shrink-0" />
              </span>
            )}
          </div>
          {displayUrl && <p className="text-xs text-muted-foreground truncate mt-0.5 max-w-[340px]">{displayUrl}</p>}
        </div>

        {/* Actions */}
        <div
          role="toolbar"
          className="flex items-center gap-1 shrink-0"
          onClick={(e) => e.stopPropagation()}
          onKeyDown={() => {}}
        >
          {/* Switch button — hidden in readOnly mode */}
          {!readOnly && !isCurrent && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onSwitch();
              }}
              className="px-3 py-1.5 rounded-lg text-xs font-medium border transition-all hover:shadow-sm"
              style={{
                borderColor: `${appColor}40`,
                color: appColor,
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.backgroundColor = `${appColor}10`;
              }}
              onFocus={(e) => {
                e.currentTarget.style.backgroundColor = `${appColor}10`;
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.backgroundColor = "transparent";
              }}
              onBlur={(e) => {
                e.currentTarget.style.backgroundColor = "transparent";
              }}
            >
              使用
            </button>
          )}

          {/* Expand indicator — hidden in readOnly mode */}
          {!readOnly && (
            <ChevronDown
              className={cn(
                "w-4 h-4 text-muted-foreground/50 transition-transform duration-200",
                expanded && "rotate-180",
              )}
            />
          )}

          {/* More menu */}
          <div className="relative" ref={menuRef}>
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                setMenuOpen(!menuOpen);
              }}
              className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors opacity-0 group-hover:opacity-100"
            >
              <MoreHorizontal className="w-4 h-4" />
            </button>
            {menuOpen && (
              <div className="absolute right-0 top-full mt-1 w-36 rounded-xl border border-border bg-card shadow-lg z-[100] py-1 overflow-hidden">
                {onOpenWebsite && provider.websiteUrl && (
                  <button
                    type="button"
                    onClick={() => {
                      onOpenWebsite();
                      setMenuOpen(false);
                    }}
                    className="w-full flex items-center gap-2 px-3 py-2 text-xs text-foreground hover:bg-muted/50 transition-colors"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    官网
                  </button>
                )}
                <div className="border-t border-border my-1" />
                <button
                  type="button"
                  onClick={() => handleDeleteClick()}
                  className={cn(
                    "w-full flex items-center gap-2 px-3 py-2 text-xs transition-colors",
                    confirmDelete
                      ? "text-white bg-destructive hover:bg-destructive/90"
                      : "text-destructive hover:bg-destructive/10",
                  )}
                >
                  <Trash2 className="w-3.5 h-3.5" />
                  {confirmDelete ? "确认删除" : "删除"}
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* ── Expanded inline editor — hidden in readOnly mode ──────────────────────────── */}
      {!readOnly && (
        <AnimatePresence>
          {expanded && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
              className="overflow-hidden"
            >
              <div className="px-4 py-4 space-y-4">
                {/* Display name */}
                <div className="space-y-1.5">
                  <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">显示名称</span>
                  <input
                    type="text"
                    value={draft.name}
                    onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                    className="w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                  />
                </div>

                {/* App-specific fields */}
                <div className="pt-1">
                  {appId === "claude" && renderClaudeFields()}
                  {appId === "codex" && renderCodexFields()}
                  {appId === "opencode" && renderOpenCodeFields()}
                  {appId === "gemini" && renderGeminiFields()}
                </div>

                {/* Notes / URL */}
                <div className="space-y-1.5">
                  <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">备忘录</span>
                  <input
                    type="text"
                    value={draft.notes || draft.websiteUrl || ""}
                    onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
                    className="w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                    placeholder="https://example.com"
                  />
                </div>

                {/* Save / Cancel */}
                <div className="flex items-center justify-end gap-2 pt-1">
                  <button
                    type="button"
                    onClick={onToggleExpand}
                    className="px-3 py-1.5 rounded-lg text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-muted/40 transition-colors"
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    onClick={handleSave}
                    disabled={!draft.name.trim()}
                    className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-xs font-medium text-white transition-all disabled:opacity-40 hover:opacity-90"
                    style={{ backgroundColor: appColor }}
                  >
                    <Save className="w-3.5 h-3.5" />
                    保存
                  </button>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      )}
    </motion.div>
  );
}

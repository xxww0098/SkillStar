import { AnimatePresence, motion } from "framer-motion";
import { ChevronDown, FileJson, Save, X } from "lucide-react";
import { useEffect, useState } from "react";
import { cn } from "../../../lib/utils";
import { useModelFetch } from "../hooks/useModelFetch";
import type { ProviderEntry, ProviderMeta } from "../hooks/useModelProviders";
import type { ModelAppId } from "./AppCapsuleSwitcher";
import { ApiKeyInput } from "./shared/ApiKeyInput";
import { EndpointInput } from "./shared/EndpointInput";
import { ModelInput } from "./shared/ModelInput";

interface EditProviderDialogProps {
  open: boolean;
  appId: ModelAppId;
  provider: ProviderEntry | null;
  isCreate: boolean;
  onClose: () => void;
  onSave: (entry: ProviderEntry) => void;
}

export function EditProviderDialog({ open, appId, provider, isCreate, onClose, onSave }: EditProviderDialogProps) {
  const [draft, setDraft] = useState<ProviderEntry | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const modelFetch = useModelFetch();

  useEffect(() => {
    if (open && provider) {
      setDraft(JSON.parse(JSON.stringify(provider)));
      setShowAdvanced(false);
      modelFetch.clear();
    } else if (!open) {
      setDraft(null);
    }
  }, [open, provider, modelFetch.clear]);

  // ── Fetch models handlers (must be before early return) ────

  if (!draft) return null;

  // ── Meta helpers ──────────────────────────────────────────

  const updateMeta = (patch: Partial<ProviderMeta>) => {
    setDraft((prev) => {
      if (!prev) return prev;
      return { ...prev, meta: { ...(prev.meta || {}), ...patch } };
    });
  };

  const getApiKeyField = () => draft.meta?.apiKeyField || "ANTHROPIC_AUTH_TOKEN";

  // ── Config update helpers ──────────────────────────────────

  const updateClaudeEnv = (key: string, value: string) => {
    setDraft((prev) => {
      if (!prev) return prev;
      const env = { ...((prev.settingsConfig.env as Record<string, unknown>) || {}) };
      env[key] = value;
      return { ...prev, settingsConfig: { ...prev.settingsConfig, env } };
    });
  };

  const updateCodexAuth = (key: string, value: string) => {
    setDraft((prev) => {
      if (!prev) return prev;
      const auth = { ...((prev.settingsConfig.auth as Record<string, unknown>) || {}) };
      auth[key] = value;
      return { ...prev, settingsConfig: { ...prev.settingsConfig, auth } };
    });
  };

  const updateCodexConfig = (text: string) => {
    setDraft((prev) => {
      if (!prev) return prev;
      return { ...prev, settingsConfig: { ...prev.settingsConfig, config: text } };
    });
  };

  const updateOpenCodeProvider = (providerKey: string, field: string, value: string) => {
    setDraft((prev) => {
      if (!prev) return prev;
      const p = { ...((prev.settingsConfig.provider as Record<string, Record<string, unknown>>) || {}) };
      const current = { ...(p[providerKey] || {}) };

      if (field === "npm") {
        current.npm = value;
      } else {
        const options = { ...((current.options as Record<string, unknown>) || {}) };
        options[field] = value;
        current.options = options;
      }

      p[providerKey] = current;
      return { ...prev, settingsConfig: { ...prev.settingsConfig, provider: p } };
    });
  };

  // ── Claude fields ──────────────────────────────────────────

  const renderClaudeFields = () => {
    const env = (draft.settingsConfig.env as Record<string, unknown>) || {};
    const apiKeyField = getApiKeyField();
    const apiFormat = draft.meta?.apiFormat || "anthropic";

    return (
      <div className="space-y-4">
        <ApiKeyInput
          value={
            (env[apiKeyField] as string) ||
            (env.ANTHROPIC_AUTH_TOKEN as string) ||
            (env.ANTHROPIC_API_KEY as string) ||
            ""
          }
          onChange={(v) => updateClaudeEnv(apiKeyField, v)}
          apiKeyUrl={provider?.apiKeyUrl}
        />
        <EndpointInput
          value={(env.ANTHROPIC_BASE_URL as string) || ""}
          onChange={(v) => updateClaudeEnv("ANTHROPIC_BASE_URL", v)}
        />

        <div className="grid grid-cols-2 gap-4">
          <ModelInput
            label="主模型"
            value={(env.ANTHROPIC_MODEL as string) || ""}
            onChange={(v) => updateClaudeEnv("ANTHROPIC_MODEL", v)}
            fetchedModels={modelFetch.models}
            fetchingModels={modelFetch.loading}
          />
          <ModelInput
            label="Sonnet 模型"
            value={(env.ANTHROPIC_DEFAULT_SONNET_MODEL as string) || ""}
            onChange={(v) => updateClaudeEnv("ANTHROPIC_DEFAULT_SONNET_MODEL", v)}
            fetchedModels={modelFetch.models}
          />
        </div>

        {/* Advanced */}
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

                <ModelInput
                  label="推理模型"
                  value={(env.ANTHROPIC_REASONING_MODEL as string) || ""}
                  onChange={(v) => updateClaudeEnv("ANTHROPIC_REASONING_MODEL", v)}
                  placeholder="claude-sonnet-4..."
                  fetchedModels={modelFetch.models}
                />

                <div className="grid grid-cols-3 gap-3">
                  <ModelInput
                    label="Haiku"
                    value={(env.ANTHROPIC_DEFAULT_HAIKU_MODEL as string) || ""}
                    onChange={(v) => updateClaudeEnv("ANTHROPIC_DEFAULT_HAIKU_MODEL", v)}
                    fetchedModels={modelFetch.models}
                  />
                  <ModelInput
                    label="Sonnet"
                    value={(env.ANTHROPIC_DEFAULT_SONNET_MODEL as string) || ""}
                    onChange={(v) => updateClaudeEnv("ANTHROPIC_DEFAULT_SONNET_MODEL", v)}
                    fetchedModels={modelFetch.models}
                  />
                  <ModelInput
                    label="Opus"
                    value={(env.ANTHROPIC_DEFAULT_OPUS_MODEL as string) || ""}
                    onChange={(v) => updateClaudeEnv("ANTHROPIC_DEFAULT_OPUS_MODEL", v)}
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

  // ── Codex fields ──────────────────────────────────────────

  const renderCodexFields = () => {
    const auth = (draft.settingsConfig.auth as Record<string, unknown>) || {};
    const configText = (draft.settingsConfig.config as string) || "";
    const modelMatch = configText.match(/^model\s*=\s*"([^"]+)"/m);

    return (
      <div className="space-y-4">
        <ApiKeyInput
          value={(auth.OPENAI_API_KEY as string) || ""}
          onChange={(v) => updateCodexAuth("OPENAI_API_KEY", v)}
          apiKeyUrl={provider?.apiKeyUrl}
        />

        {modelMatch?.[1] && (
          <ModelInput
            label="当前模型"
            value={modelMatch[1]}
            onChange={(v) => {
              updateCodexConfig(configText.replace(/^model\s*=\s*"[^"]*"/m, `model = "${v}"`));
            }}
            fetchedModels={modelFetch.models}
            fetchingModels={modelFetch.loading}
          />
        )}

        <div className="space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider flex items-center gap-1.5">
            <FileJson className="w-3 h-3" />
            config.toml
          </span>
          <textarea
            value={configText}
            onChange={(e) => updateCodexConfig(e.target.value)}
            rows={8}
            className="w-full rounded-lg bg-background/60 border border-border text-xs text-foreground font-mono p-3 resize-y focus:outline-none focus:ring-1 focus:ring-primary/50"
            placeholder={`model_provider = "openai"\nmodel = "gpt-4o"`}
            spellCheck={false}
          />
        </div>
      </div>
    );
  };

  // ── OpenCode fields ──────────────────────────────────────────

  const renderOpenCodeFields = () => {
    const p = (draft.settingsConfig.provider as Record<string, Record<string, unknown>>) || {};
    const providerKeys = Object.keys(p);
    const providerKey = providerKeys[0];
    if (!providerKey) {
      return (
        <div className="p-4 border border-dashed rounded-lg text-center text-sm text-muted-foreground">
          无 provider 配置可供编辑
        </div>
      );
    }

    const current = p[providerKey];
    const options = (current.options as Record<string, unknown>) || {};

    return (
      <div className="space-y-4">
        <div className="px-3 py-2 rounded-lg bg-muted/40 border border-border text-xs text-muted-foreground font-mono">
          Provider ID: {providerKey}
        </div>
        <ApiKeyInput
          value={(options.apiKey as string) || ""}
          onChange={(v) => updateOpenCodeProvider(providerKey, "apiKey", v)}
          apiKeyUrl={provider?.apiKeyUrl}
        />
        <EndpointInput
          value={(options.baseURL as string) || ""}
          onChange={(v) => updateOpenCodeProvider(providerKey, "baseURL", v)}
        />
        <div className="space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">NPM Package</span>
          <input
            type="text"
            value={(current.npm as string) || ""}
            onChange={(e) => updateOpenCodeProvider(providerKey, "npm", e.target.value)}
            className="w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm font-mono focus:outline-none focus:ring-1 focus:ring-primary/50"
          />
        </div>
      </div>
    );
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
          onClick={(e) => {
            if (e.target === e.currentTarget) onClose();
          }}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: 20 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: 20 }}
            transition={{ type: "spring", stiffness: 400, damping: 30 }}
            className="w-full max-w-lg rounded-2xl border border-border bg-card shadow-2xl overflow-hidden flex flex-col max-h-[85vh]"
          >
            {/* Header */}
            <div className="shrink-0 flex items-center justify-between px-5 py-4 border-b border-border">
              <h2 className="text-base font-semibold text-foreground flex items-center gap-2">
                <span
                  className="w-2.5 h-2.5 rounded-full"
                  style={{ backgroundColor: draft.iconColor || "currentColor" }}
                />
                {isCreate ? "完成供应商配置" : "编辑供应商"}
              </h2>
              <button
                type="button"
                onClick={onClose}
                className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* Body */}
            <div className="flex-1 overflow-y-auto scrollbar-thin px-5 py-5 space-y-6">
              <div className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">显示名称</span>
                <input
                  type="text"
                  value={draft.name}
                  onChange={(e) => setDraft({ ...draft, name: e.target.value })}
                  className="w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                />
              </div>

              <div className="space-y-1.5">
                <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  备忘录 / 网址
                </span>
                <input
                  type="text"
                  value={draft.notes || draft.websiteUrl || ""}
                  onChange={(e) => setDraft({ ...draft, notes: e.target.value })}
                  className="w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                  placeholder="https://example.com"
                />
              </div>

              <div className="border-t border-border pt-5">
                <h3 className="text-sm font-medium text-foreground mb-4">连接设置</h3>
                {appId === "claude" && renderClaudeFields()}
                {appId === "codex" && renderCodexFields()}
                {appId === "opencode" && renderOpenCodeFields()}
              </div>
            </div>

            {/* Footer */}
            <div className="shrink-0 flex items-center justify-end gap-3 px-5 py-4 border-t border-border bg-muted/20">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 rounded-lg text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => {
                  onSave(draft);
                  onClose();
                }}
                disabled={!draft.name.trim()}
                className="flex items-center gap-2 px-4 py-2 rounded-lg bg-primary hover:bg-primary/90 text-primary-foreground text-sm font-medium transition-colors disabled:opacity-50"
              >
                <Save className="w-4 h-4" />
                保存
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

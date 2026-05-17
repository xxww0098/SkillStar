import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Activity, ChevronDown, Eye, EyeOff, Loader2, Plus, Trash2, X, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { tauriInvoke } from "../../../lib/ipc";
import type { AppId, ModelMapping, ProviderEntry, ProviderPreset, ProviderSettings } from "../../../types";
import { ProviderBrandIcon } from "./ProviderBrandIcon";

interface ProviderEditorProps {
  appId: AppId;
  provider: ProviderEntry | null; // null = create mode
  onSave: (entry: Omit<ProviderEntry, "id" | "created_at">) => Promise<void>;
  onClose: () => void;
}

interface FormErrors {
  name?: string;
  base_url?: string;
  api_key?: string;
  models?: string;
  category?: string;
}

type ConnectionStatus = "idle" | "testing" | "ok" | "error";

const CATEGORIES = [
  { value: "cloud", label: "Cloud" },
  { value: "local", label: "Local" },
  { value: "proxy", label: "Proxy" },
] as const;

const MAX_NAME_LENGTH = 64;

function isValidUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

export function ProviderEditor({ appId, provider, onSave, onClose }: ProviderEditorProps) {
  const prefersReducedMotion = useReducedMotion();
  const isEditMode = provider !== null;

  // Form state
  const [name, setName] = useState(provider?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(provider?.settings_config.base_url ?? "");
  const [apiKey, setApiKey] = useState(provider?.settings_config.api_key ?? "");
  const [category, setCategory] = useState(provider?.category ?? "cloud");
  const [models, setModels] = useState<ModelMapping[]>(
    provider?.settings_config.models ?? [{ source_model: "", target_model: "", enabled: true }],
  );
  const [presetId, setPresetId] = useState<string | undefined>(provider?.preset_id);

  // UI state
  const [showApiKey, setShowApiKey] = useState(false);
  const [errors, setErrors] = useState<FormErrors>({});
  const [saving, setSaving] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>("idle");
  const [connectionLatency, setConnectionLatency] = useState<number | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);

  // Presets
  const [presets, setPresets] = useState<ProviderPreset[]>([]);
  const [showPresetDropdown, setShowPresetDropdown] = useState(false);
  const presetDropdownRef = useRef<HTMLDivElement>(null);
  const nameInputRef = useRef<HTMLInputElement>(null);

  // Load presets on mount
  useEffect(() => {
    tauriInvoke("get_provider_presets")
      .then(setPresets)
      .catch(() => {});
  }, []);

  // Auto-focus name input on open
  useEffect(() => {
    if (!isEditMode) {
      setTimeout(() => nameInputRef.current?.focus(), 200);
    }
  }, [isEditMode]);

  // Close preset dropdown on outside click
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (presetDropdownRef.current && !presetDropdownRef.current.contains(e.target as Node)) {
        setShowPresetDropdown(false);
      }
    }
    if (showPresetDropdown) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [showPresetDropdown]);

  // Filter presets relevant to current appId
  const relevantPresets = useMemo(() => {
    return presets.filter((p) => {
      // Official presets are app-specific
      if (p.id === "official") {
        if (appId === "claude") return p.name.includes("Anthropic");
        if (appId === "codex") return p.name.includes("OpenAI");
      }
      // Other presets work with any app
      return true;
    });
  }, [presets, appId]);

  const handlePresetSelect = useCallback((preset: ProviderPreset) => {
    setName(preset.name);
    setBaseUrl(preset.base_url);
    setModels([]);
    setPresetId(preset.id);
    setCategory("cloud");
    setShowPresetDropdown(false);
    setErrors({});
  }, []);

  const validate = useCallback((): FormErrors => {
    const errs: FormErrors = {};

    const trimmedName = name.trim();
    if (!trimmedName) {
      errs.name = "Provider name is required";
    } else if (trimmedName.length > MAX_NAME_LENGTH) {
      errs.name = `Name must be at most ${MAX_NAME_LENGTH} characters`;
    }

    const trimmedUrl = baseUrl.trim();
    if (!trimmedUrl) {
      errs.base_url = "Base URL is required";
    } else if (!isValidUrl(trimmedUrl)) {
      errs.base_url = "Must be a valid HTTP/HTTPS URL";
    }

    const validModels = models.filter((m) => m.source_model.trim() || m.target_model.trim());
    if (validModels.length === 0) {
      errs.models = "At least one model mapping is required";
    }

    return errs;
  }, [name, baseUrl, models]);

  const handleSave = async () => {
    const validationErrors = validate();
    if (Object.keys(validationErrors).length > 0) {
      setErrors(validationErrors);
      return;
    }

    setErrors({});
    setSaving(true);

    const validModels = models.filter((m) => m.source_model.trim() || m.target_model.trim());

    const settingsConfig: ProviderSettings = {
      base_url: baseUrl.trim(),
      api_key: apiKey,
      models: validModels.map((m) => ({
        source_model: m.source_model.trim(),
        target_model: m.target_model.trim() || m.source_model.trim(),
        enabled: m.enabled,
      })),
    };

    const entry: Omit<ProviderEntry, "id" | "created_at"> = {
      name: name.trim(),
      category,
      settings_config: settingsConfig,
      preset_id: presetId,
      sort_index: provider?.sort_index,
    };

    try {
      await onSave(entry);
      onClose();
    } catch (e) {
      setErrors({ name: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const handleConnectionTest = async () => {
    if (!baseUrl.trim()) {
      setErrors((prev) => ({ ...prev, base_url: "Base URL is required for connection test" }));
      return;
    }

    setConnectionStatus("testing");
    setConnectionError(null);
    setConnectionLatency(null);

    try {
      const result = await tauriInvoke("test_provider_latency", {
        app_id: appId,
        provider_id: provider?.id ?? "test",
        base_url: baseUrl.trim(),
        api_key: apiKey,
      });

      if (result.status === "ok") {
        setConnectionStatus("ok");
        setConnectionLatency(result.latency_ms);
      } else {
        setConnectionStatus("error");
        setConnectionError(result.error_message ?? result.status);
      }
    } catch (e) {
      setConnectionStatus("error");
      setConnectionError(String(e));
    }
  };

  const addModel = () => {
    setModels([...models, { source_model: "", target_model: "", enabled: true }]);
  };

  const removeModel = (index: number) => {
    setModels(models.filter((_, i) => i !== index));
    if (errors.models) {
      setErrors((prev) => ({ ...prev, models: undefined }));
    }
  };

  const updateModel = (index: number, field: "source_model" | "target_model", value: string) => {
    setModels(models.map((m, i) => (i === index ? { ...m, [field]: value } : m)));
    if (errors.models) {
      setErrors((prev) => ({ ...prev, models: undefined }));
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      onClose();
    }
  };

  return (
    <AnimatePresence>
      {/* Backdrop */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: prefersReducedMotion ? 0 : 0.2 }}
        className="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm"
        onClick={onClose}
      />

      {/* Drawer panel */}
      <motion.div
        initial={{ x: "100%" }}
        animate={{ x: 0 }}
        exit={{ x: "100%" }}
        transition={prefersReducedMotion ? { duration: 0 } : { type: "spring", stiffness: 300, damping: 30 }}
        role="dialog"
        aria-modal="true"
        aria-label={isEditMode ? "Edit Provider" : "Create Provider"}
        className="fixed right-0 top-0 bottom-0 z-50 w-[520px] max-w-[90vw] border-l border-border bg-background shadow-2xl flex flex-col"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border shrink-0">
          <h2 className="text-heading-sm">{isEditMode ? "Edit Provider" : "Create Provider"}</h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Scrollable form content */}
        <div className="flex-1 overflow-y-auto overscroll-y-contain px-6 py-5 space-y-5">
          {/* Preset selector (create mode only) */}
          {!isEditMode && relevantPresets.length > 0 && (
            <div className="relative" ref={presetDropdownRef}>
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-1.5 block">
                Quick Create from Preset
              </label>
              <button
                type="button"
                onClick={() => setShowPresetDropdown(!showPresetDropdown)}
                className="w-full flex items-center justify-between h-9 px-3 rounded-xl border border-input-border bg-input backdrop-blur-sm text-sm text-foreground hover:border-primary/40 transition-colors cursor-pointer"
              >
                <span className={presetId ? "text-foreground" : "text-muted-foreground/70"}>
                  {presetId
                    ? (relevantPresets.find((p) => p.id === presetId)?.name ?? "Select preset...")
                    : "Select preset..."}
                </span>
                <ChevronDown className="w-4 h-4 text-muted-foreground" />
              </button>

              {showPresetDropdown && (
                <div className="absolute top-full left-0 right-0 mt-1 z-10 rounded-xl border border-border bg-card shadow-lg overflow-hidden">
                  {relevantPresets.map((preset) => (
                    <button
                      key={`${preset.id}-${preset.name}`}
                      type="button"
                      onClick={() => handlePresetSelect(preset)}
                      className="w-full flex items-center gap-3 px-3 py-2.5 text-sm text-left hover:bg-accent/10 transition-colors cursor-pointer"
                    >
                      <ProviderBrandIcon
                        presetId={preset.id}
                        providerName={preset.name}
                        iconColor={preset.icon_color}
                        size="xs"
                      />
                      <div className="flex-1 min-w-0">
                        <div className="font-medium truncate">{preset.name}</div>
                        <div className="text-xs text-muted-foreground truncate">创建后从供应商获取模型</div>
                      </div>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Name field */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Name</label>
            <Input
              ref={nameInputRef}
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                if (errors.name) setErrors((prev) => ({ ...prev, name: undefined }));
              }}
              placeholder="e.g. My DeepSeek Provider"
              maxLength={MAX_NAME_LENGTH}
              aria-invalid={!!errors.name}
              disabled={saving}
            />
            {errors.name && <p className="text-xs text-destructive">{errors.name}</p>}
            <p className="text-xs text-muted-foreground/60">
              {name.length}/{MAX_NAME_LENGTH}
            </p>
          </div>

          {/* Base URL field */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Base URL</label>
            <Input
              value={baseUrl}
              onChange={(e) => {
                setBaseUrl(e.target.value);
                if (errors.base_url) setErrors((prev) => ({ ...prev, base_url: undefined }));
              }}
              placeholder="https://api.example.com/v1"
              aria-invalid={!!errors.base_url}
              disabled={saving}
            />
            {errors.base_url && <p className="text-xs text-destructive">{errors.base_url}</p>}
            <p className="text-xs text-muted-foreground/60">OpenAI-compatible API endpoint</p>
          </div>

          {/* API Key field */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">API Key</label>
            <div className="relative">
              <Input
                type={showApiKey ? "text" : "password"}
                value={apiKey}
                onChange={(e) => {
                  setApiKey(e.target.value);
                  if (errors.api_key) setErrors((prev) => ({ ...prev, api_key: undefined }));
                }}
                placeholder="sk-..."
                className="pr-10"
                disabled={saving}
              />
              <button
                type="button"
                onClick={() => setShowApiKey(!showApiKey)}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 p-1 rounded-md text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                aria-label={showApiKey ? "Hide API key" : "Show API key"}
              >
                {showApiKey ? <EyeOff className="w-4 h-4" /> : <Eye className="w-4 h-4" />}
              </button>
            </div>
            {errors.api_key && <p className="text-xs text-destructive">{errors.api_key}</p>}
            <p className="text-xs text-muted-foreground/60">
              Leave empty for local models that don't require authentication
            </p>
          </div>

          {/* Category field */}
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Category</label>
            <div className="relative">
              <select
                value={category}
                onChange={(e) => setCategory(e.target.value)}
                disabled={saving}
                className="w-full h-9 px-3 rounded-xl border border-input-border bg-input backdrop-blur-sm text-sm text-foreground appearance-none cursor-pointer focus:outline-none focus:ring-2 focus:ring-primary/40 focus:border-primary/60 transition duration-200"
              >
                {CATEGORIES.map((cat) => (
                  <option key={cat.value} value={cat.value}>
                    {cat.label}
                  </option>
                ))}
              </select>
              <ChevronDown className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground pointer-events-none" />
            </div>
          </div>

          {/* Models list */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">Models</label>
              <Button type="button" variant="ghost" size="xs" onClick={addModel} disabled={saving}>
                <Plus className="w-3 h-3" />
                Add
              </Button>
            </div>

            {errors.models && <p className="text-xs text-destructive">{errors.models}</p>}

            <div className="space-y-2">
              {models.map((model, index) => (
                <div
                  key={index}
                  className="flex items-center gap-2 p-2.5 rounded-lg border border-border/60 bg-card/30"
                >
                  <div className="flex-1 min-w-0 space-y-1.5">
                    <Input
                      value={model.source_model}
                      onChange={(e) => updateModel(index, "source_model", e.target.value)}
                      placeholder="Source model name"
                      className="h-7 text-xs"
                      disabled={saving}
                    />
                    <Input
                      value={model.target_model}
                      onChange={(e) => updateModel(index, "target_model", e.target.value)}
                      placeholder="Target model (optional, defaults to source)"
                      className="h-7 text-xs"
                      disabled={saving}
                    />
                  </div>
                  <button
                    type="button"
                    onClick={() => removeModel(index)}
                    disabled={saving || models.length <= 1}
                    className="p-1.5 rounded-md text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors cursor-pointer disabled:opacity-30 disabled:cursor-not-allowed"
                    aria-label="Remove model"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              ))}
            </div>
          </div>

          {/* Connection test */}
          <div className="space-y-2 pt-2 border-t border-border/50">
            <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
              Connection Test
            </label>
            <div className="flex items-center gap-3">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={handleConnectionTest}
                disabled={saving || connectionStatus === "testing" || !baseUrl.trim()}
              >
                {connectionStatus === "testing" ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Zap className="w-3.5 h-3.5" />
                )}
                {connectionStatus === "testing" ? "Testing..." : "Test Connection"}
              </Button>

              {connectionStatus === "ok" && (
                <span className="flex items-center gap-1.5 text-xs text-emerald-400">
                  <Activity className="w-3.5 h-3.5" />
                  Connected ({connectionLatency}ms)
                </span>
              )}
              {connectionStatus === "error" && (
                <span className="text-xs text-destructive truncate max-w-[240px]" title={connectionError ?? undefined}>
                  {connectionError ?? "Connection failed"}
                </span>
              )}
            </div>
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 px-6 py-4 border-t border-border shrink-0 bg-card/50 backdrop-blur-sm">
          <Button variant="ghost" onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={saving} className="min-w-[80px]">
            {saving ? (
              <>
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
                Saving...
              </>
            ) : isEditMode ? (
              "Save Changes"
            ) : (
              "Create Provider"
            )}
          </Button>
        </div>
      </motion.div>
    </AnimatePresence>
  );
}

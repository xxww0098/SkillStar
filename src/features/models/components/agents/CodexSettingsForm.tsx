import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Check, Wand2, FileCheck2, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type { ShellRcWriteResult } from "../../../../lib/ipc/commands/system";
import { cn } from "../../../../lib/utils";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import type { CodexAuthMode, CodexWireApi } from "../../lib/providerPatch";
import { codexEnvKeyName, maskApiKey, recommendedCodexDefaults } from "../../lib/providerPatch";
import type { ProviderEntryFlat } from "../../../../types";
import { fieldLabelClass } from "../providerForm/ProviderConfigPrimitives";

export interface CodexSettingsFormProps {
  wireApi: CodexWireApi;
  authMode: CodexAuthMode;
  onChangeWireApi: (value: CodexWireApi) => void;
  onChangeAuthMode: (value: CodexAuthMode) => void;
  disabled?: boolean;
  /** Bound provider — used to render the env_key export hint in third_party mode. */
  provider?: ProviderEntryFlat | null;
}

function Segmented<T extends string>({
  value,
  options,
  onChange,
  disabled,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="flex gap-1.5">
      {options.map((opt) => (
        <button
          key={opt.value}
          type="button"
          disabled={disabled}
          onClick={() => onChange(opt.value)}
          className={cn(
            "flex-1 rounded-lg border px-2 py-1.5 text-[11px] font-medium transition-colors",
            value === opt.value
              ? "border-primary/50 bg-primary/10 text-primary"
              : "border-border/55 text-muted-foreground hover:text-foreground",
            disabled && "cursor-not-allowed opacity-60",
          )}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}

/**
 * Codex-specific parameters (wire_api + auth_mode). Persisted on the provider
 * record (codex_* fields + meta) — the caller owns the write path.
 */
export function CodexSettingsForm({
  wireApi,
  authMode,
  onChangeWireApi,
  onChangeAuthMode,
  disabled,
  provider,
}: CodexSettingsFormProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const envKeyName = provider ? codexEnvKeyName(provider) : "";
  const maskedKey = provider ? maskApiKey(provider.api_key) : "";
  const exportCommand = provider && envKeyName && provider.api_key ? `export ${envKeyName}=${provider.api_key}` : "";

  const handleCopy = async () => {
    if (!exportCommand) return;
    try {
      await navigator.clipboard.writeText(exportCommand);
      setCopied(true);
      toast.success(t("models.dialog.copied"));
      setTimeout(() => setCopied(false), 1500);
    } catch {
      toast.error(t("models.dialog.copyFailed"));
    }
  };

  // Detect a sub-optimal Codex config (e.g. a third-party provider created
  // before the default-inference shipped, still carrying responses + api_key).
  // When the current values diverge from the URL-appropriate recommendation we
  // surface a gentle one-click "apply recommended" fix. New providers are
  // created correct, so this only ever shows for legacy/mis-configured ones.
  const recommendation = useMemo(
    () => (provider ? recommendedCodexDefaults(provider.base_url_openai) : null),
    [provider],
  );
  const suboptimal = !!recommendation && (recommendation.wireApi !== wireApi || recommendation.authMode !== authMode);

  const applyRecommended = () => {
    if (!recommendation) return;
    if (recommendation.wireApi !== wireApi) onChangeWireApi(recommendation.wireApi);
    if (recommendation.authMode !== authMode) onChangeAuthMode(recommendation.authMode);
  };

  // --- ~/.zshrc export write (third_party auth) ---
  // Detect whether ~/.zshrc already holds the matching export line so the UI
  // can show "already written ✓" instead of a redundant button.
  const [zshrcStatus, setZshrcStatus] = useState<"unknown" | "written" | "missing">("unknown");
  const [writing, setWriting] = useState(false);

  useEffect(() => {
    // Only probe when there is a concrete key + api_key to export; otherwise
    // the button is irrelevant.
    if (authMode !== "third_party" || !provider || !envKeyName || !provider.api_key) {
      setZshrcStatus("unknown");
      return;
    }
    let cancelled = false;
    tauriInvoke<string | null>("read_codex_env_from_zshrc", { envKey: envKeyName })
      .then((current) => {
        if (cancelled) return;
        setZshrcStatus(current === provider.api_key ? "written" : "missing");
      })
      .catch(() => {
        if (!cancelled) setZshrcStatus("unknown");
      });
    return () => {
      cancelled = true;
    };
  }, [authMode, provider, envKeyName]);

  const handleWriteZshrc = async () => {
    if (!provider || !envKeyName || !provider.api_key) return;
    setWriting(true);
    try {
      const result = await tauriInvoke<ShellRcWriteResult>("write_codex_env_to_zshrc", {
        envKey: envKeyName,
        value: provider.api_key,
      });
      setZshrcStatus("written");
      if (result.action === "noop") {
        toast.success(t("models.dialog.zshrcAlreadyWritten"));
      } else {
        toast.success(t("models.dialog.zshrcWritten"));
      }
    } catch (e) {
      if (import.meta.env.DEV) console.error("write_codex_env_to_zshrc failed:", e);
      toast.error(t("models.dialog.zshrcWriteFailed"));
    } finally {
      setWriting(false);
    }
  };

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <AgentToolIcon toolId="codex" size="md" />
        <div>
          <p className="text-xs font-semibold text-foreground">{t("models.dialog.codexTitle")}</p>
          <p className="text-[11px] text-muted-foreground">{t("models.dialog.codexSubtitle")}</p>
        </div>
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="space-y-1">
          <span className={fieldLabelClass}>{t("models.dialog.wireApi")}</span>
          <Segmented
            value={wireApi}
            onChange={onChangeWireApi}
            disabled={disabled}
            options={[
              { value: "responses", label: "Responses" },
              { value: "chat", label: "Chat Completions" },
            ]}
          />
        </div>
        <div className="space-y-1">
          <span className={fieldLabelClass}>{t("models.dialog.authMode")}</span>
          <Segmented
            value={authMode}
            onChange={onChangeAuthMode}
            disabled={disabled}
            options={[
              { value: "api_key", label: t("models.dialog.authModeApiKey") },
              { value: "oauth", label: t("models.dialog.authModeOauth") },
              { value: "third_party", label: t("models.dialog.authModeThirdParty") },
            ]}
          />
        </div>
      </div>

      {authMode === "third_party" ? (
        <div className="rounded-lg border border-primary/30 bg-primary/[0.04] px-3 py-2.5">
          <p className="text-[11px] font-medium text-foreground">{t("models.dialog.envKeyTitle")}</p>
          <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">{t("models.dialog.envKeyHint")}</p>
          {exportCommand ? (
            <div className="mt-2 flex items-center gap-2">
              <code className="flex-1 truncate rounded-md bg-background/60 px-2 py-1 font-mono text-[10px] text-foreground/90">
                <span className="text-primary">{envKeyName}</span>
                <span className="text-muted-foreground">=</span>
                {maskedKey}
              </code>
              <button
                type="button"
                onClick={handleCopy}
                className="inline-flex shrink-0 items-center gap-1 rounded-md border border-border/55 px-2 py-1 text-[10px] font-medium text-foreground transition-colors hover:bg-background/60"
              >
                {copied ? <Check className="h-3 w-3 text-primary" /> : <Copy className="h-3 w-3" />}
                {t("models.dialog.copyExport")}
              </button>
            </div>
          ) : provider && !provider.api_key ? (
            <p className="mt-2 text-[10px] text-amber-400">{t("models.dialog.envKeyMissing")}</p>
          ) : null}
          {exportCommand ? (
            <div className="mt-2 flex items-center gap-2">
              {zshrcStatus === "written" ? (
                <span className="inline-flex items-center gap-1 rounded-md border border-primary/40 bg-primary/10 px-2 py-1 text-[10px] font-medium text-primary">
                  <FileCheck2 className="h-3 w-3" />
                  {t("models.dialog.zshrcWrittenBadge")}
                </span>
              ) : (
                <button
                  type="button"
                  onClick={handleWriteZshrc}
                  disabled={disabled || writing}
                  className="inline-flex shrink-0 items-center gap-1 rounded-md border border-primary/40 bg-primary/10 px-2 py-1 text-[10px] font-medium text-primary transition-colors hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-60"
                >
                  {writing ? <Loader2 className="h-3 w-3 animate-spin" /> : <FileCheck2 className="h-3 w-3" />}
                  {t("models.dialog.writeToZshrc")}
                </button>
              )}
              <span className="text-[10px] text-muted-foreground">{t("models.dialog.zshrcHint")}</span>
            </div>
          ) : null}
        </div>
      ) : null}

      {suboptimal ? (
        <div className="rounded-lg border border-amber-500/30 bg-amber-500/[0.06] px-3 py-2.5">
          <p className="text-[11px] font-medium text-amber-400">{t("models.dialog.codexSuboptimalTitle")}</p>
          <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground">
            {t("models.dialog.codexSuboptimalHint", {
              wireApi: recommendation!.wireApi,
              authMode: recommendation!.authMode,
            })}
          </p>
          <button
            type="button"
            onClick={applyRecommended}
            disabled={disabled}
            className="mt-2 inline-flex items-center gap-1.5 rounded-md border border-amber-500/40 bg-amber-500/10 px-2 py-1 text-[10px] font-medium text-amber-300 transition-colors hover:bg-amber-500/20 disabled:cursor-not-allowed disabled:opacity-60"
          >
            <Wand2 className="h-3 w-3" />
            {t("models.dialog.applyRecommended")}
          </button>
        </div>
      ) : null}
    </div>
  );
}

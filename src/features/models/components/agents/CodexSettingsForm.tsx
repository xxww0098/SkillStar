import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Copy, Check } from "lucide-react";
import { toast } from "sonner";
import { cn } from "../../../../lib/utils";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import type { CodexAuthMode, CodexWireApi } from "../../lib/providerPatch";
import { codexEnvKeyName, maskApiKey } from "../../lib/providerPatch";
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
        </div>
      ) : null}
    </div>
  );
}

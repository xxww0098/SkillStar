import { Eye, EyeOff } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import type { CatalogEntry, Subscription } from "../../types";
import { Field } from "./fields";

interface ApiKeyFieldsProps {
  editing: Subscription | null;
  catalogId: string;
  selectedEntry: CatalogEntry | null;
  apiKey: string;
  setApiKey: (value: string) => void;
  showKey: boolean;
  setShowKey: (updater: (v: boolean) => boolean) => void;
  platformToken: string;
  setPlatformToken: (value: string) => void;
  showPlatformToken: boolean;
  setShowPlatformToken: (updater: (v: boolean) => boolean) => void;
}

/** API-key credential inputs (plus DeepSeek's optional platform token). */
export function ApiKeyFields({
  editing,
  catalogId,
  selectedEntry,
  apiKey,
  setApiKey,
  showKey,
  setShowKey,
  platformToken,
  setPlatformToken,
  showPlatformToken,
  setShowPlatformToken,
}: ApiKeyFieldsProps) {
  const { t } = useTranslation();
  return (
    <>
      <Field
        label={editing ? t("usage.fieldApiKeyOptional") : "API Key"}
        hint={selectedEntry?.id === "glm" ? t("usage.glmApiKeyHint") : undefined}
      >
        <div className="relative">
          <Input
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="sk-..."
            type={showKey ? "text" : "password"}
            autoComplete="off"
            className="h-9 rounded-xl border-input-border bg-input pr-9 text-xs text-foreground"
          />
          <button
            type="button"
            className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
            onClick={() => setShowKey((v) => !v)}
          >
            {showKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
          </button>
        </div>
      </Field>

      {catalogId === "deepseek" && (
        <Field
          label={
            editing?.has_platform_token ? t("usage.deepseekPlatformTokenOptional") : t("usage.deepseekPlatformToken")
          }
          hint={t("usage.deepseekPlatformTokenHint")}
        >
          <div className="space-y-2">
            <div className="relative">
              <Input
                value={platformToken}
                onChange={(e) => setPlatformToken(e.target.value)}
                placeholder={t("usage.deepseekPlatformTokenPlaceholder")}
                type={showPlatformToken ? "text" : "password"}
                autoComplete="off"
                className="h-9 rounded-xl border-input-border bg-input pr-9 text-xs text-foreground"
              />
              <button
                type="button"
                className="absolute right-3 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                onClick={() => setShowPlatformToken((v) => !v)}
              >
                {showPlatformToken ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </button>
            </div>
            {editing?.has_platform_token && (
              <p className="text-[9px] text-muted-foreground">{t("usage.deepseekPlatformTokenConfigured")}</p>
            )}
            <p className="text-[9px] leading-relaxed text-muted-foreground">
              <code className="rounded bg-muted px-1 py-0.5 text-[9px]">JSON.parse(localStorage.userToken).value</code>
            </p>
          </div>
        </Field>
      )}
    </>
  );
}

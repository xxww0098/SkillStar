import { ArrowLeft, Eye, EyeOff, Loader2 } from "lucide-react";
import { type FormEvent, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../../components/ui/button";
import { Input } from "../../../../../components/ui/input";
import type { ProviderEntryFlat } from "../../../../../types";
import { useProvidersFlat } from "../../../hooks/useProvidersFlat";
import { type ProviderFormValues, validatePatch } from "../../../lib/providerPatch";
import { ModelFormField, ModelFormSection, modelInputClass } from "../../providerForm/ProviderConfigPrimitives";

interface ClaudeProviderCreateProps {
  onClose: () => void;
  onCreated: (provider: ProviderEntryFlat) => void;
}

type Draft = Pick<ProviderFormValues, "name" | "baseUrlAnthropic" | "apiKey" | "defaultModel" | "modelsUrl">;

export function ClaudeProviderCreate({ onClose, onCreated }: ClaudeProviderCreateProps) {
  const { t } = useTranslation();
  const { createProvider } = useProvidersFlat();
  const [draft, setDraft] = useState<Draft>({
    name: "",
    baseUrlAnthropic: "",
    apiKey: "",
    defaultModel: "",
    modelsUrl: "",
  });
  const [touched, setTouched] = useState<Partial<Record<keyof Draft, boolean>>>({});
  const [attemptedSubmit, setAttemptedSubmit] = useState(false);
  const [showApiKey, setShowApiKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const savingRef = useRef(false);

  const defaultModel = draft.defaultModel.trim();
  const entry = {
    id: "",
    name: draft.name.trim(),
    api_key: draft.apiKey.trim(),
    base_url_anthropic: draft.baseUrlAnthropic.trim(),
    base_url_openai: "",
    models_url: draft.modelsUrl.trim(),
    default_model: defaultModel,
    models: defaultModel ? [defaultModel] : [],
  };
  const validationCode = validatePatch(entry);
  const canSave = !validationCode && Boolean(entry.base_url_anthropic && entry.api_key);
  const errors: Partial<Record<keyof Draft, string>> = {
    name: !entry.name ? t("models.errors.nameRequired") : undefined,
    baseUrlAnthropic: !entry.base_url_anthropic
      ? t("models.claudeCreate.anthropicUrlRequired")
      : validationCode === "invalidAnthropicUrl"
        ? t("models.errors.invalidAnthropicUrl")
        : undefined,
    apiKey: !entry.api_key ? t("models.claudeCreate.apiKeyRequired") : undefined,
    modelsUrl: validationCode === "invalidModelsUrl" ? t("models.errors.invalidModelsUrl") : undefined,
  };
  const fieldError = (field: keyof Draft) => (attemptedSubmit || touched[field] ? errors[field] : undefined);
  const nameError = fieldError("name");
  const anthropicError = fieldError("baseUrlAnthropic");
  const apiKeyError = fieldError("apiKey");
  const modelsUrlError = fieldError("modelsUrl");

  const setField = (field: keyof Draft, value: string) => setDraft((current) => ({ ...current, [field]: value }));
  const touch = (field: keyof Draft) => setTouched((current) => ({ ...current, [field]: true }));

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (savingRef.current) return;
    setAttemptedSubmit(true);
    if (!canSave) return;

    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    let created: ProviderEntryFlat;
    try {
      created = await createProvider(entry);
    } catch (error) {
      setSaveError(error instanceof Error ? error.message : String(error));
      return;
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
    onCreated(created);
  };

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col overflow-hidden">
      <header className="flex shrink-0 items-start gap-3 border-b border-border/50 px-5 py-4">
        <Button type="button" size="sm" variant="ghost" onClick={onClose} disabled={saving}>
          <ArrowLeft className="h-3.5 w-3.5" />
          {t("models.common.back")}
        </Button>
        <div className="min-w-0 flex-1">
          <h1 className="text-base font-semibold">{t("models.claudeCreate.title")}</h1>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t("models.claudeCreate.description")}</p>
        </div>
      </header>
      <div className="ss-page-scroll">
        <form
          aria-label={t("models.claudeCreate.title")}
          aria-busy={saving}
          autoComplete="off"
          noValidate
          onSubmit={handleSubmit}
          className="mx-auto grid w-full max-w-2xl gap-4 px-5 py-5"
        >
          <fieldset disabled={saving} className="grid min-w-0 gap-4">
            <ModelFormSection title={t("models.connectionTab.identitySection")}>
              <ModelFormField id="claude-create-name" label={t("models.connectionTab.name")} error={nameError} required>
                <Input
                  id="claude-create-name"
                  value={draft.name}
                  onChange={(event) => setField("name", event.target.value)}
                  onBlur={() => touch("name")}
                  placeholder={t("models.claudeCreate.namePlaceholder")}
                  className={modelInputClass}
                  required
                  aria-invalid={Boolean(nameError)}
                  aria-describedby={nameError ? "claude-create-name-error" : undefined}
                />
              </ModelFormField>
              <ModelFormField
                id="claude-create-anthropic-url"
                label={t("models.claudeCreate.anthropicBaseUrl")}
                info={t("models.connectionTab.anthropicEndpointHint")}
                error={anthropicError}
                required
              >
                <Input
                  id="claude-create-anthropic-url"
                  type="url"
                  value={draft.baseUrlAnthropic}
                  onChange={(event) => setField("baseUrlAnthropic", event.target.value)}
                  onBlur={() => touch("baseUrlAnthropic")}
                  placeholder="https://api.example.com/anthropic"
                  className={modelInputClass}
                  required
                  aria-invalid={Boolean(anthropicError)}
                  aria-describedby={anthropicError ? "claude-create-anthropic-url-error" : undefined}
                />
              </ModelFormField>
              <ModelFormField
                id="claude-create-api-key"
                label={t("models.claudeCreate.apiKey")}
                hint={t("models.connectionTab.localCredentials")}
                error={apiKeyError}
                required
              >
                <div className="relative">
                  <Input
                    id="claude-create-api-key"
                    type={showApiKey ? "text" : "password"}
                    value={draft.apiKey}
                    onChange={(event) => setField("apiKey", event.target.value)}
                    onBlur={() => touch("apiKey")}
                    placeholder="sk-..."
                    autoComplete="off"
                    spellCheck={false}
                    className={modelInputClass + " pr-12"}
                    required
                    aria-invalid={Boolean(apiKeyError)}
                    aria-describedby={
                      apiKeyError
                        ? "claude-create-api-key-hint claude-create-api-key-error"
                        : "claude-create-api-key-hint"
                    }
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    onClick={() => setShowApiKey((current) => !current)}
                    aria-label={t(showApiKey ? "models.connectionTab.hide" : "models.connectionTab.show")}
                    className="absolute right-1 top-1/2 -translate-y-1/2"
                  >
                    {showApiKey ? <EyeOff /> : <Eye />}
                  </Button>
                </div>
              </ModelFormField>
            </ModelFormSection>
            <ModelFormSection title={t("models.claudeCreate.optionalSettings")}>
              <ModelFormField id="claude-create-default-model" label={t("models.claudeCreate.defaultModel")}>
                <Input
                  id="claude-create-default-model"
                  value={draft.defaultModel}
                  onChange={(event) => setField("defaultModel", event.target.value)}
                  placeholder={t("models.modelsTab.defaultModelPlaceholder")}
                  className={modelInputClass}
                />
              </ModelFormField>
              <ModelFormField
                id="claude-create-models-url"
                label={t("models.connectionTab.modelsUrl")}
                info={t("models.connectionTab.modelsUrlHint")}
                error={modelsUrlError}
              >
                <Input
                  id="claude-create-models-url"
                  type="url"
                  value={draft.modelsUrl}
                  onChange={(event) => setField("modelsUrl", event.target.value)}
                  onBlur={() => touch("modelsUrl")}
                  placeholder="https://api.example.com/v1/models"
                  className={modelInputClass}
                  aria-invalid={Boolean(modelsUrlError)}
                  aria-describedby={modelsUrlError ? "claude-create-models-url-error" : undefined}
                />
              </ModelFormField>
            </ModelFormSection>
          </fieldset>
          {saveError !== null ? (
            <p role="alert" className="break-words text-sm leading-5 text-destructive">
              {t("models.claudeCreate.saveFailed", { message: saveError })}
            </p>
          ) : null}
          <div className="flex justify-end">
            <Button type="submit" disabled={!canSave || saving}>
              {saving ? <Loader2 className="animate-spin motion-reduce:animate-none" /> : null}
              {t(saving ? "models.claudeCreate.saving" : "models.claudeCreate.save")}
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}

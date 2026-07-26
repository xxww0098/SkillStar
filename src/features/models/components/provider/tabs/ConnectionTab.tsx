import { Copy, Eye, EyeOff, KeyRound, Network, Plus, UserRound } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../../components/ui/button";
import { Input } from "../../../../../components/ui/input";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import { ModelFormField, ModelFormSection, modelInputClass } from "../../providerForm/ProviderConfigPrimitives";

/** 连接页签：名称、API Key、双端点、模型列表 URL。 */
export function ConnectionTab({ form }: { form: ProviderForm }) {
  const { values, setField, validationErrorCode } = form;
  const { t } = useTranslation();
  const [showApiKey, setShowApiKey] = useState(false);
  const [showAnthropicUrl, setShowAnthropicUrl] = useState(Boolean(values.baseUrlAnthropic.trim()));

  const handleCopyApiKey = useCallback(async () => {
    if (!values.apiKey || typeof navigator === "undefined" || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(values.apiKey);
    } catch {
      /* clipboard unavailable in some shells */
    }
  }, [values.apiKey]);

  const fieldError = (code: typeof validationErrorCode) =>
    validationErrorCode === code && code ? t(`models.errors.${code}`) : undefined;
  const nameError = fieldError("nameRequired");
  const openaiError = fieldError("invalidOpenaiUrl");
  const anthropicError = fieldError("invalidAnthropicUrl");
  const modelsUrlError = fieldError("invalidModelsUrl");

  return (
    <div className="grid gap-3.5">
      <ModelFormSection title={t("models.connectionTab.identitySection")} icon={<UserRound className="h-4 w-4" />}>
        <ModelFormField id="provider-name" label={t("models.connectionTab.name")} error={nameError} required>
          <Input
            id="provider-name"
            value={values.name}
            onChange={(e) => setField("name", e.target.value)}
            placeholder="DeepSeek"
            className={modelInputClass}
            aria-invalid={Boolean(nameError)}
            aria-describedby={nameError ? "provider-name-error" : undefined}
          />
        </ModelFormField>
      </ModelFormSection>

      <ModelFormSection
        title={t("models.connectionTab.credentialsSection")}
        description={t("models.connectionTab.localCredentials")}
        icon={<KeyRound className="h-4 w-4" />}
      >
        <ModelFormField
          id="provider-api-key"
          label="API Key"
          action={
            <Button
              type="button"
              variant="ghost"
              size="xs"
              onClick={handleCopyApiKey}
              disabled={!values.apiKey}
              className="text-muted-foreground"
            >
              <Copy className="h-3 w-3" />
              {t("models.connectionTab.copy")}
            </Button>
          }
        >
          <div className="relative">
            <KeyRound className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/70" />
            <Input
              id="provider-api-key"
              type={showApiKey ? "text" : "password"}
              value={values.apiKey}
              onChange={(e) => setField("apiKey", e.target.value)}
              placeholder="sk-..."
              autoComplete="off"
              className={`${modelInputClass} pl-9 pr-10`}
            />
            <button
              type="button"
              onClick={() => setShowApiKey((v) => !v)}
              className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted-foreground transition hover:bg-muted/50 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35"
              aria-label={showApiKey ? t("models.connectionTab.hide") : t("models.connectionTab.show")}
            >
              {showApiKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </button>
          </div>
        </ModelFormField>
      </ModelFormSection>

      <ModelFormSection
        title={t("models.connectionTab.endpointsSection")}
        description={t("models.connectionTab.endpointsSectionHint")}
        icon={<Network className="h-4 w-4" />}
      >
        <ModelFormField
          id="provider-openai-endpoint"
          label={t("models.connectionTab.openaiEndpoint")}
          info={t("models.connectionTab.openaiEndpointHint")}
          error={openaiError}
        >
          <Input
            id="provider-openai-endpoint"
            value={values.baseUrlOpenai}
            onChange={(e) => setField("baseUrlOpenai", e.target.value)}
            placeholder="https://api.example.com/v1"
            className={modelInputClass}
            aria-invalid={Boolean(openaiError)}
            aria-describedby={openaiError ? "provider-openai-endpoint-error" : undefined}
          />
        </ModelFormField>

        {showAnthropicUrl || values.baseUrlAnthropic ? (
          <ModelFormField
            id="provider-anthropic-endpoint"
            label={t("models.connectionTab.anthropicEndpoint")}
            info={t("models.connectionTab.anthropicEndpointHint")}
            error={anthropicError}
          >
            <Input
              id="provider-anthropic-endpoint"
              value={values.baseUrlAnthropic}
              onChange={(e) => setField("baseUrlAnthropic", e.target.value)}
              placeholder="https://api.example.com/anthropic"
              className={modelInputClass}
              aria-invalid={Boolean(anthropicError)}
              aria-describedby={anthropicError ? "provider-anthropic-endpoint-error" : undefined}
            />
          </ModelFormField>
        ) : (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="w-fit gap-1.5 text-xs text-muted-foreground"
            onClick={() => setShowAnthropicUrl(true)}
          >
            <Plus className="h-3.5 w-3.5" />
            {t("models.connectionTab.addAnthropicEndpoint")}
          </Button>
        )}

        <ModelFormField
          id="provider-models-url"
          label={t("models.connectionTab.modelsUrl")}
          info={t("models.connectionTab.modelsUrlHint")}
          error={modelsUrlError}
        >
          <Input
            id="provider-models-url"
            value={values.modelsUrl}
            onChange={(e) => setField("modelsUrl", e.target.value)}
            placeholder="https://api.example.com/v1/models"
            className={modelInputClass}
            aria-invalid={Boolean(modelsUrlError)}
            aria-describedby={modelsUrlError ? "provider-models-url-error" : undefined}
          />
        </ModelFormField>
      </ModelFormSection>
    </div>
  );
}

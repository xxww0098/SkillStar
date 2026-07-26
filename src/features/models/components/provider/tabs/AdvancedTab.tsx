import { Gauge, StickyNote } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../../../components/ui/input";
import { Switch } from "../../../../../components/ui/switch";
import type { ProviderForm } from "../../../hooks/useProviderForm";
import {
  ModelFormField,
  ModelFormSection,
  modelControlSurfaceClass,
  modelInputClass,
  modelTextareaClass,
} from "../../providerForm/ProviderConfigPrimitives";

/** 高级页签：运行参数、备注。Agent 专属参数在各 Agent 的接入设置对话框里。 */
export function AdvancedTab({ form }: { form: ProviderForm }) {
  const { values, setField } = form;
  const { t } = useTranslation();

  return (
    <div className="grid gap-3.5">
      <ModelFormSection
        title={t("models.advancedTab.runtimeParams")}
        description={t("models.advancedTab.runtimeParamsHint")}
        icon={<Gauge className="h-4 w-4" />}
      >
        <div className="grid gap-3 sm:grid-cols-2">
          <ModelFormField
            id="provider-context-length"
            label={t("models.advancedTab.context")}
            info={t("models.advancedTab.contextHint")}
          >
            <Input
              id="provider-context-length"
              type="number"
              value={values.contextLength}
              onChange={(e) => setField("contextLength", Number(e.target.value))}
              min={1024}
              className={modelInputClass}
            />
          </ModelFormField>
          <ModelFormField
            id="provider-max-tokens"
            label={t("models.advancedTab.maxTokens")}
            info={t("models.advancedTab.maxTokensHint")}
          >
            <Input
              id="provider-max-tokens"
              type="number"
              value={values.maxTokens}
              onChange={(e) => setField("maxTokens", Number(e.target.value))}
              min={1}
              className={modelInputClass}
            />
          </ModelFormField>
          <ModelFormField
            id="provider-timeout"
            label={t("models.advancedTab.timeout")}
            info={t("models.advancedTab.timeoutHint")}
          >
            <Input
              id="provider-timeout"
              type="number"
              value={values.timeout}
              onChange={(e) => setField("timeout", Number(e.target.value))}
              min={1}
              className={modelInputClass}
            />
          </ModelFormField>
          <ModelFormField
            id="provider-retry-count"
            label={t("models.advancedTab.retry")}
            info={t("models.advancedTab.retryHint")}
          >
            <Input
              id="provider-retry-count"
              type="number"
              value={values.retryCount}
              onChange={(e) => setField("retryCount", Number(e.target.value))}
              min={0}
              className={modelInputClass}
            />
          </ModelFormField>
        </div>
        <ModelFormField
          id="provider-streaming"
          label={t("models.advancedTab.streaming")}
          info={t("models.advancedTab.streamingHint")}
        >
          <div
            className={`${modelControlSurfaceClass} flex min-h-10 items-center justify-between rounded-[10px] px-3 py-2`}
          >
            <span className="text-[11px] text-muted-foreground">
              {values.streaming ? t("models.advancedTab.streamingOn") : t("models.advancedTab.streamingOff")}
            </span>
            <Switch
              id="provider-streaming"
              checked={values.streaming}
              onCheckedChange={(v) => setField("streaming", v)}
              aria-label={t("models.advancedTab.streaming")}
            />
          </div>
        </ModelFormField>
      </ModelFormSection>

      <ModelFormSection title={t("models.advancedTab.notes")} icon={<StickyNote className="h-4 w-4" />}>
        <ModelFormField id="provider-notes" label={t("models.advancedTab.notes")}>
          <textarea
            id="provider-notes"
            value={values.notes}
            onChange={(e) => setField("notes", e.target.value)}
            rows={4}
            className={modelTextareaClass}
          />
        </ModelFormField>
      </ModelFormSection>
    </div>
  );
}

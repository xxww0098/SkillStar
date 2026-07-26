import { useTranslation } from "react-i18next";
import { ClaudeColor } from "../../../../components/ui/icons/lobe";
import { LobeIcon } from "../../../../components/ui/icons/LobeIcon";
import { LATEST_CLAUDE_MODELS } from "../../lib/providerPatch";
import { ModelFormField } from "../providerForm/ProviderConfigPrimitives";
import { EditableModelCombobox } from "../shared/EditableModelCombobox";

export interface ClaudeModelMappingValues {
  claudeMainModel: string;
  claudeHaikuModel: string;
  claudeSonnetModel: string;
  claudeOpusModel: string;
}

export interface ClaudeModelMappingProps {
  values: ClaudeModelMappingValues;
  /** Model id suggestions shown in the themed combobox. */
  options: string[];
  onChange: <K extends keyof ClaudeModelMappingValues>(key: K, value: string) => void;
  disabled?: boolean;
}

const FIELDS: {
  key: keyof ClaudeModelMappingValues;
  label?: string;
  labelKey?: string;
  infoKey: string;
  placeholder: string;
}[] = [
  {
    key: "claudeMainModel",
    labelKey: "models.dialog.mainModel",
    infoKey: "models.dialog.mainModelInfo",
    placeholder: LATEST_CLAUDE_MODELS.main,
  },
  {
    key: "claudeHaikuModel",
    label: "Haiku",
    infoKey: "models.dialog.haikuModelInfo",
    placeholder: LATEST_CLAUDE_MODELS.haiku,
  },
  {
    key: "claudeSonnetModel",
    label: "Sonnet",
    infoKey: "models.dialog.sonnetModelInfo",
    placeholder: LATEST_CLAUDE_MODELS.sonnet,
  },
  {
    key: "claudeOpusModel",
    label: "Opus",
    infoKey: "models.dialog.opusModelInfo",
    placeholder: LATEST_CLAUDE_MODELS.opus,
  },
];

/**
 * Claude Code tier-model mapping (writes ~/.claude/settings.json env vars via
 * the provider's meta keys). Dumb value-driven component — persisting is the
 * caller's concern.
 */
export function ClaudeModelMapping({ values, options, onChange, disabled }: ClaudeModelMappingProps) {
  const { t } = useTranslation();
  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2">
        <span className="flex h-7 w-7 items-center justify-center rounded-lg border border-border/60 bg-background/70">
          <LobeIcon icon={ClaudeColor} size={18} />
        </span>
        <div>
          <p className="text-xs font-semibold text-foreground">{t("models.dialog.claudeMappingTitle")}</p>
          <p className="text-[11px] text-muted-foreground">{t("models.dialog.claudeMappingSubtitle")}</p>
        </div>
      </div>
      <div className="grid gap-2.5 sm:grid-cols-2">
        {FIELDS.map((field) => (
          <ModelFormField
            key={field.key}
            id={`claude-${field.key}`}
            label={field.labelKey ? t(field.labelKey) : field.label}
            info={t(field.infoKey)}
          >
            <EditableModelCombobox
              id={`claude-${field.key}`}
              value={values[field.key]}
              options={options}
              onChange={(value) => onChange(field.key, value)}
              placeholder={field.placeholder}
              ariaLabel={field.labelKey ? t(field.labelKey) : (field.label ?? "")}
              disabled={disabled}
            />
          </ModelFormField>
        ))}
      </div>
    </div>
  );
}

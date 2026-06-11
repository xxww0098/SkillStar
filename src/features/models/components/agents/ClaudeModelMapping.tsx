import ClaudeIcon from "@lobehub/icons/es/Claude/components/Color";
import { useTranslation } from "react-i18next";
import { Input } from "../../../../components/ui/input";
import { LATEST_CLAUDE_MODELS } from "../../lib/providerPatch";
import { fieldLabelClass } from "../providerForm/ProviderConfigPrimitives";

export interface ClaudeModelMappingValues {
  claudeMainModel: string;
  claudeHaikuModel: string;
  claudeSonnetModel: string;
  claudeOpusModel: string;
}

export interface ClaudeModelMappingProps {
  values: ClaudeModelMappingValues;
  /** Model id suggestions shown in the datalist. */
  options: string[];
  onChange: <K extends keyof ClaudeModelMappingValues>(key: K, value: string) => void;
  disabled?: boolean;
}

const FIELDS: { key: keyof ClaudeModelMappingValues; label?: string; labelKey?: string; placeholder: string }[] = [
  { key: "claudeMainModel", labelKey: "models.dialog.mainModel", placeholder: LATEST_CLAUDE_MODELS.main },
  { key: "claudeHaikuModel", label: "Haiku", placeholder: LATEST_CLAUDE_MODELS.haiku },
  { key: "claudeSonnetModel", label: "Sonnet", placeholder: LATEST_CLAUDE_MODELS.sonnet },
  { key: "claudeOpusModel", label: "Opus", placeholder: LATEST_CLAUDE_MODELS.opus },
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
          <ClaudeIcon size={18} />
        </span>
        <div>
          <p className="text-xs font-semibold text-foreground">{t("models.dialog.claudeMappingTitle")}</p>
          <p className="text-[11px] text-muted-foreground">{t("models.dialog.claudeMappingSubtitle")}</p>
        </div>
      </div>
      <div className="grid gap-2.5 sm:grid-cols-2">
        {FIELDS.map((field) => (
          <label key={field.key} className="space-y-1">
            <span className={fieldLabelClass}>{field.labelKey ? t(field.labelKey) : field.label}</span>
            <Input
              value={values[field.key]}
              onChange={(e) => onChange(field.key, e.target.value)}
              placeholder={field.placeholder}
              list="claude-mapping-models"
              disabled={disabled}
            />
          </label>
        ))}
      </div>
      <datalist id="claude-mapping-models">
        {options.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
    </div>
  );
}

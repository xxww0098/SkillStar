import { Eye, EyeOff, KeyRound, Lock } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../components/ui/input";
import { StatusChip } from "../../../components/ui/StatusChip";
import { cn } from "../../../lib/utils";
import type { McpInputFormat, McpInstallInputScope } from "../../../types";
import type { McpFieldError, McpInstallField } from "../lib/installForm";
import { groupInstallFields } from "../lib/installForm";

/**
 * The install form, rendered from `server.json` `Input` semantics.
 *
 * One control per declared property, no guessing:
 * `isSecret` → password box, `choices` → a select the user must pick from,
 * `format: filepath|number|boolean` → the matching control, `default` → prefill,
 * `placeholder` → grey hint (never a value), `description` → the publisher's own
 * explanation, and a pinned `value` → read-only, with a second-level form for
 * the `{curly_brace}` variables inside it.
 *
 * Secret values are masked by default and revealed only on an explicit click,
 * per field — the reveal never persists across fields or re-renders of the
 * wizard.
 */

function controlFor(format: McpInputFormat): { type: string; inputMode?: "numeric" | "text" } {
  switch (format) {
    case "number":
      return { type: "text", inputMode: "numeric" };
    case "filepath":
      return { type: "text", inputMode: "text" };
    default:
      return { type: "text" };
  }
}

function SecretToggle({ shown, onToggle }: { shown: boolean; onToggle: () => void }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={onToggle}
      title={shown ? t("mcp.secretHide") : t("mcp.secretShow")}
      className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground transition hover:text-foreground"
    >
      {shown ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
    </button>
  );
}

interface ValueControlProps {
  id: string;
  value: string;
  onChange: (next: string) => void;
  format: McpInputFormat;
  choices?: readonly string[];
  isSecret: boolean;
  placeholder?: string | null;
  invalid: boolean;
}

function ValueControl({ id, value, onChange, format, choices, isSecret, placeholder, invalid }: ValueControlProps) {
  const { t } = useTranslation();
  const [revealed, setRevealed] = useState(false);

  if (choices && choices.length > 0) {
    return (
      <select
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className={cn(
          "h-9 w-full rounded-lg border bg-background/60 px-3 text-xs text-foreground outline-none transition focus:border-primary/50 focus:ring-2 focus:ring-primary/20",
          invalid ? "border-destructive/60" : "border-border",
        )}
      >
        <option value="">{t("mcp.inputChoosePlaceholder")}</option>
        {choices.map((choice) => (
          <option key={choice} value={choice}>
            {choice}
          </option>
        ))}
      </select>
    );
  }

  if (format === "boolean") {
    return (
      <div className="flex gap-2">
        {["true", "false"].map((option) => (
          <button
            key={option}
            type="button"
            onClick={() => onChange(option)}
            className={cn(
              "flex-1 rounded-lg border px-3 py-1.5 text-xs font-medium transition",
              value === option
                ? "border-primary/60 bg-primary/10 text-primary"
                : "border-border bg-background/40 text-muted-foreground hover:bg-muted/40",
            )}
          >
            {option}
          </button>
        ))}
      </div>
    );
  }

  const control = controlFor(format);
  return (
    <div className="relative">
      <Input
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        type={isSecret && !revealed ? "password" : control.type}
        inputMode={control.inputMode}
        autoComplete={isSecret ? "off" : undefined}
        spellCheck={isSecret ? false : undefined}
        placeholder={placeholder ?? undefined}
        className={cn("h-9 font-mono text-xs", isSecret && "pr-8", invalid && "border-destructive/60")}
      />
      {isSecret ? <SecretToggle shown={revealed} onToggle={() => setRevealed((prev) => !prev)} /> : null}
    </div>
  );
}

/**
 * A field is addressed by `(scope, index)`, never by its key: two positional
 * arguments with no name and no `valueHint` are both labelled `argument`, and
 * keying on that label made editing the first one edit the second.
 */
interface McpInstallInputsFormProps {
  fields: readonly McpInstallField[];
  errors: readonly McpFieldError[];
  onFieldChange: (scope: McpInstallInputScope, index: number, value: string) => void;
  onVariableChange: (scope: McpInstallInputScope, index: number, variable: string, value: string) => void;
}

export function McpInstallInputsForm({ fields, errors, onFieldChange, onVariableChange }: McpInstallInputsFormProps) {
  const { t } = useTranslation();
  if (fields.length === 0) return null;

  const errorFor = (scope: McpInstallInputScope, index: number, variable?: string) =>
    errors.find((error) => error.scope === scope && error.index === index && error.variable === variable) ?? null;

  return (
    <div className="space-y-4">
      {groupInstallFields(fields).map(({ scope, fields: group }) => (
        <div key={scope} className="space-y-3">
          <p className="text-micro font-semibold uppercase tracking-wider text-muted-foreground">
            {t(`mcp.inputScope_${scope}`)}
          </p>

          {group.map((field) => {
            const id = `mcp-input-${scope}-${field.index}`;
            const fieldError = errorFor(scope, field.index);
            return (
              <div key={`${scope}:${field.index}`} className="space-y-1.5">
                <label htmlFor={id} className="flex flex-wrap items-center gap-1.5 text-xs font-medium text-foreground">
                  <span className="font-mono">{field.key}</span>
                  {field.mustAsk ? <span className="text-destructive">*</span> : null}
                  {field.input.isSecret ? (
                    <StatusChip size="sm" tone="warning">
                      <KeyRound className="h-3 w-3" />
                      {t("mcp.inputSecret")}
                    </StatusChip>
                  ) : null}
                  {field.templated ? (
                    <StatusChip size="sm" tone="muted">
                      <Lock className="h-3 w-3" />
                      {t("mcp.inputPinned")}
                    </StatusChip>
                  ) : null}
                </label>

                {field.input.description ? (
                  <p className="text-[11px] leading-relaxed text-muted-foreground">{field.input.description}</p>
                ) : null}

                {field.templated ? (
                  <>
                    <p className="break-all rounded-lg border border-border/60 bg-muted/40 px-3 py-2 font-mono text-[11px] text-muted-foreground">
                      {field.value}
                    </p>
                    {field.variables.length === 0 ? (
                      <p className="text-[11px] text-muted-foreground/80">{t("mcp.inputPinnedHint")}</p>
                    ) : null}
                    {field.variables.map((variable) => {
                      const variableId = `${id}-${variable.name}`;
                      const variableError = errorFor(scope, field.index, variable.name);
                      return (
                        <div key={variable.name} className="ml-3 space-y-1.5 border-l border-border/60 pl-3">
                          <label
                            htmlFor={variableId}
                            className="flex items-center gap-1.5 text-xs font-medium text-foreground"
                          >
                            <span className="font-mono">{`{${variable.name}}`}</span>
                            {variable.variable.isRequired || variable.variable.isSecret ? (
                              <span className="text-destructive">*</span>
                            ) : null}
                            {variable.variable.isSecret ? (
                              <StatusChip size="sm" tone="warning">
                                <KeyRound className="h-3 w-3" />
                                {t("mcp.inputSecret")}
                              </StatusChip>
                            ) : null}
                          </label>
                          {variable.variable.description ? (
                            <p className="text-[11px] leading-relaxed text-muted-foreground">
                              {variable.variable.description}
                            </p>
                          ) : null}
                          <ValueControl
                            id={variableId}
                            value={variable.value}
                            onChange={(next) => onVariableChange(scope, field.index, variable.name, next)}
                            format={variable.variable.format}
                            choices={variable.variable.choices}
                            isSecret={variable.variable.isSecret}
                            invalid={variableError != null}
                          />
                          {variableError ? (
                            <p className="text-[11px] text-destructive">{t(`mcp.inputError_${variableError.code}`)}</p>
                          ) : null}
                        </div>
                      );
                    })}
                  </>
                ) : (
                  <>
                    <ValueControl
                      id={id}
                      value={field.value}
                      onChange={(next) => onFieldChange(scope, field.index, next)}
                      format={field.input.format}
                      choices={field.input.choices}
                      isSecret={field.input.isSecret}
                      placeholder={field.input.placeholder}
                      invalid={fieldError != null}
                    />
                    {fieldError ? (
                      <p className="text-[11px] text-destructive">{t(`mcp.inputError_${fieldError.code}`)}</p>
                    ) : null}
                  </>
                )}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

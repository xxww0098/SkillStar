import { Check, ChevronDown, CornerDownRight, X } from "lucide-react";
import { Select } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../../../../../lib/utils";
import type { OmpRoleTarget, OmpThinkingLevel, ProviderEntryFlat } from "../../../../../../../types";
import {
  isDefaultThinking,
  OMP_ROLES_INHERITING_DEFAULT,
  OMP_THINKING_LEVELS,
  type OmpRoleDef,
  previewRoleValue,
} from "../../../../../lib/ompRoles";
import {
  modelCompactInputClass,
  modelOptionClass,
  modelPopoverContentClass,
} from "../../../../providerForm/ProviderConfigPrimitives";
import { ProviderSelectPopover } from "../../../../shared/ProviderSelectPopover";
import { providerModels } from "./RichMatrixShell";

export type OmpRoleRowProps = {
  role: OmpRoleDef;
  /** Current assignment, or `undefined` when the role is unassigned. */
  target: OmpRoleTarget | undefined;
  /** Providers already bound to OMP — the only ones the backend can write. */
  providers: ProviderEntryFlat[];
  onAssignProvider: (providerId: string) => void;
  onModelChange: (model: string) => void;
  onThinkingChange: (level: OmpThinkingLevel) => void;
  onClear: () => void;
};

/**
 * One `modelRoles.<id>` row: provider → model → thinking, plus a preview of the
 * exact string SkillStar will write. An unassigned role is a normal state, not
 * an error — `inheritsDefault` roles say so explicitly.
 */
export function OmpRoleRow({
  role,
  target,
  providers,
  onAssignProvider,
  onModelChange,
  onThinkingChange,
  onClear,
}: OmpRoleRowProps) {
  const { t } = useTranslation();
  // Free-typed model ids are committed on blur/Enter so a keystroke never turns
  // into a disk write.
  const [draft, setDraft] = useState<string | null>(null);
  const [listId] = useState(() => `omp-models-${role.id}-${Math.random().toString(36).slice(2, 8)}`);

  const provider = target ? (providers.find((p) => p.id === target.provider_id) ?? null) : null;
  const modelValue = draft ?? target?.model ?? "";
  const preview = target ? previewRoleValue(target.provider_id, target.model, target.thinking) : null;
  const inheritsDefault = OMP_ROLES_INHERITING_DEFAULT.includes(role.id);

  const commitModel = () => {
    if (draft === null) return;
    setDraft(null);
    if (draft !== (target?.model ?? "")) onModelChange(draft);
  };

  return (
    <div className="rounded-[10px] border border-border/45 bg-background/25 px-2.5 py-2">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
        <span className="text-[12px] font-semibold text-foreground">{t(`models.ompRoles.${role.key}.name`)}</span>
        {role.flag ? (
          <code className="rounded bg-muted/60 px-1 py-px font-mono text-[10px] text-muted-foreground">
            {role.flag}
          </code>
        ) : null}
        <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground">
          {t(`models.ompRoles.${role.key}.desc`)}
        </span>
      </div>

      <div className="mt-1.5 grid grid-cols-[1.05fr_1.35fr_92px_28px] items-center gap-1.5">
        <ProviderSelectPopover
          providers={providers}
          currentId={target?.provider_id ?? null}
          onPick={onAssignProvider}
          density="compact"
          ariaLabel={t("models.ompRoles.panel.providerAria", { role: t(`models.ompRoles.${role.key}.name`) })}
        />

        <div className="min-w-0">
          <input
            list={listId}
            className={cn(modelCompactInputClass, "w-full font-mono text-[11px]")}
            placeholder={t("models.ompRoles.panel.modelPlaceholder")}
            aria-label={t("models.ompRoles.panel.modelAria", { role: t(`models.ompRoles.${role.key}.name`) })}
            disabled={!target}
            value={modelValue}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commitModel}
            onKeyDown={(e) => {
              if (e.key === "Enter") e.currentTarget.blur();
            }}
          />
          <datalist id={listId}>
            {(provider ? providerModels(provider) : []).map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </div>

        <ThinkingSelect
          value={target?.thinking ?? null}
          disabled={!target}
          roleName={t(`models.ompRoles.${role.key}.name`)}
          onChange={onThinkingChange}
        />

        <button
          type="button"
          onClick={onClear}
          disabled={!target}
          title={t("models.ompRoles.panel.clear")}
          aria-label={t("models.ompRoles.panel.clearAria", { role: t(`models.ompRoles.${role.key}.name`) })}
          className="flex h-9 w-7 items-center justify-center rounded-[9px] text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive disabled:pointer-events-none disabled:opacity-30"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>

      <p className="mt-1 flex items-center gap-1 truncate font-mono text-[10px] leading-4 text-muted-foreground/80">
        <CornerDownRight className="h-3 w-3 shrink-0 opacity-60" />
        {preview ? (
          <span className="truncate text-emerald-500/85">{preview}</span>
        ) : (
          <span className="truncate font-sans">
            {inheritsDefault
              ? t("models.ompRoles.panel.unassignedInherits")
              : t("models.ompRoles.panel.unassignedKept")}
          </span>
        )}
      </p>
    </div>
  );
}

/** Thinking-level `:suffix` picker. `inherit` and "unset" are the same to OMP. */
function ThinkingSelect({
  value,
  disabled,
  roleName,
  onChange,
}: {
  value: OmpThinkingLevel | null;
  disabled: boolean;
  roleName: string;
  onChange: (level: OmpThinkingLevel) => void;
}) {
  const { t } = useTranslation();
  const current = isDefaultThinking(value) ? "inherit" : (value as OmpThinkingLevel);

  return (
    <Select.Root value={current} onValueChange={(next) => onChange(next as OmpThinkingLevel)} disabled={disabled}>
      <Select.Trigger
        aria-label={t("models.ompRoles.panel.thinkingAria", { role: roleName })}
        className={cn(
          modelCompactInputClass,
          "flex w-full items-center justify-between gap-1 px-2 text-[11px]",
          isDefaultThinking(value) && "text-muted-foreground",
        )}
      >
        <Select.Value />
        <Select.Icon>
          <ChevronDown className="h-3 w-3 shrink-0 opacity-60" />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Content
          position="popper"
          sideOffset={6}
          className={cn(modelPopoverContentClass, "z-[140] max-h-64 min-w-[8rem]")}
        >
          <Select.Viewport>
            {OMP_THINKING_LEVELS.map((level) => (
              <Select.Item key={level} value={level} className={cn(modelOptionClass, "relative pr-6")}>
                <Select.ItemText>
                  {level === "inherit" ? t("models.ompRoles.panel.thinkingInherit") : level}
                </Select.ItemText>
                <Select.ItemIndicator className="absolute right-2">
                  <Check className="h-3 w-3 text-primary" />
                </Select.ItemIndicator>
              </Select.Item>
            ))}
          </Select.Viewport>
        </Select.Content>
      </Select.Portal>
    </Select.Root>
  );
}

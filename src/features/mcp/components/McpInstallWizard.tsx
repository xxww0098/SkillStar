import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import type { McpInstallInputScope, McpServerEntry, McpToolId } from "../../../types";
import { buildCommandConfirmation, buildEnvPreview } from "../lib/commandPreview";
import {
  applyInstallFields,
  buildInstallFields,
  resolvedFieldValue,
  secretFieldKeys,
  setFieldValue,
  setFieldVariable,
  validateInstallFields,
} from "../lib/installForm";
import { useMcpInstallPlan } from "../hooks/useMcpInstallPlan";
import { McpCommandConfirm } from "./McpCommandConfirm";
import { McpInstallInputsForm } from "./McpInstallInputsForm";
import { McpRuntimePicker } from "./McpRuntimePicker";
import { McpToolTargetPicker } from "./McpToolTargetPicker";

/**
 * Install a catalog entry: pick the runtime shape, answer the publisher's
 * declared inputs, then approve the exact command before anything is written.
 *
 * The order is not cosmetic. The shape decides which inputs exist, the inputs
 * decide what the command line and environment end up containing, and the
 * confirmation is therefore the last step and re-renders from the *final*
 * values rather than from the plan's precomputed preview.
 */

interface McpInstallWizardProps {
  serverId: string;
  /** Tool ids to pre-enable, e.g. the ones the user already uses. */
  initialEnabled?: Readonly<Record<string, boolean>>;
  submitting: boolean;
  onSubmit: (entry: McpServerEntry) => Promise<void> | void;
  onCancel?: () => void;
  /** Per-tool note, e.g. "not installed", from `mcp_tool_statuses`. */
  noteForTool?: (toolId: McpToolId) => string | null;
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-2">
      <h4 className="text-xs font-semibold text-foreground">{title}</h4>
      {children}
    </section>
  );
}

export function McpInstallWizard({
  serverId,
  initialEnabled,
  submitting,
  onSubmit,
  onCancel,
  noteForTool,
}: McpInstallWizardProps) {
  const { t } = useTranslation();
  const [runtimeId, setRuntimeId] = useState<string | null>(null);
  const { plan, isLoading, isFetching, error } = useMcpInstallPlan(serverId, runtimeId);

  // The plan is the source of every field, so a shape change re-seeds the form.
  // Keyed by the plan identity rather than an effect: a stale answer carried
  // across shapes would be collected for an input the new shape never declared.
  const planKey = `${plan?.serverId ?? ""}:${plan?.selectedRuntimeId ?? ""}`;
  const [seededFor, setSeededFor] = useState<string | null>(null);
  const [fields, setFields] = useState(() => buildInstallFields(plan?.inputs));
  const [enabled, setEnabled] = useState<Record<string, boolean>>({ ...(initialEnabled ?? {}) });
  const [acknowledged, setAcknowledged] = useState(false);
  const [showErrors, setShowErrors] = useState(false);

  if (plan && seededFor !== planKey) {
    setSeededFor(planKey);
    setFields(buildInstallFields(plan.inputs));
    setAcknowledged(false);
    setShowErrors(false);
  }

  const errors = useMemo(() => validateInstallFields(fields), [fields]);
  const secretValues = useMemo(
    () =>
      fields.flatMap((field) => {
        const values = field.input.isSecret ? [resolvedFieldValue(field)] : [];
        return [...values, ...field.variables.filter((v) => v.variable.isSecret).map((v) => v.value)];
      }),
    [fields],
  );

  const draft = useMemo(() => {
    if (!plan) return null;
    return applyInstallFields({ draft: plan.draft, fields });
  }, [plan, fields]);

  const confirmation = useMemo(
    () =>
      buildCommandConfirmation({
        command: draft?.command,
        args: draft?.args ?? [],
        resolvedCommandPath: plan?.resolvedCommandPath,
        planPreview: plan?.commandPreview,
        secretValues,
        usesShell: plan?.usesShell ?? false,
      }),
    [draft, plan, secretValues],
  );

  if (isLoading || (!plan && isFetching)) {
    return (
      <div className="flex h-40 items-center justify-center">
        <LoadingLogo size="md" label={t("mcp.installPlanLoading")} />
      </div>
    );
  }

  if (error || !plan || !draft) {
    return (
      <div className="rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2.5 text-xs text-destructive">
        {error instanceof Error ? error.message : t("mcp.installPlanFailed")}
      </div>
    );
  }

  const secretKeys = secretFieldKeys(fields);
  const isLocal = confirmation.preview.length > 0;
  const blocked = errors.length > 0 || (isLocal && !acknowledged);

  const handleFieldChange = (scope: McpInstallInputScope, index: number, value: string) =>
    setFields((prev) => setFieldValue(prev, scope, index, value));
  const handleVariableChange = (scope: McpInstallInputScope, index: number, variable: string, value: string) =>
    setFields((prev) => setFieldVariable(prev, scope, index, variable, value));

  const handleSubmit = async () => {
    if (errors.length > 0) {
      setShowErrors(true);
      return;
    }
    if (blocked) return;
    await onSubmit({ ...draft, enabled });
  };

  return (
    <div className="space-y-5">
      <div className="space-y-1">
        <p className="text-sm font-semibold text-foreground">{plan.serverName}</p>
        <p className="break-all font-mono text-[11px] text-muted-foreground">{plan.namespace}</p>
      </div>

      <Section title={t("mcp.runtimeSectionTitle")}>
        <McpRuntimePicker
          selection={plan.selection}
          selectedId={plan.selectedRuntimeId ?? null}
          onSelect={setRuntimeId}
          disabled={isFetching || submitting}
        />
      </Section>

      {fields.length > 0 ? (
        <Section title={t("mcp.inputsSectionTitle")}>
          <McpInstallInputsForm
            fields={fields}
            errors={showErrors ? errors : []}
            onFieldChange={handleFieldChange}
            onVariableChange={handleVariableChange}
          />
        </Section>
      ) : null}

      <Section title={t("mcp.confirmSectionTitle")}>
        <McpCommandConfirm
          confirmation={confirmation}
          env={buildEnvPreview(draft.env, secretKeys)}
          headers={buildEnvPreview(draft.headers, secretKeys)}
          url={draft.url ?? null}
          warnings={plan.warnings ?? []}
          secretPolicy={plan.secretPolicy}
          acknowledged={acknowledged}
          onAcknowledge={setAcknowledged}
          requiresAcknowledgement={isLocal}
        />
      </Section>

      <Section title={t("mcp.fieldEnabledTools")}>
        <McpToolTargetPicker
          enabled={enabled}
          onToggle={(toolId, next) => setEnabled((prev) => ({ ...prev, [toolId]: next }))}
          noteFor={noteForTool}
        />
      </Section>

      {showErrors && errors.length > 0 ? (
        <p className="text-xs text-destructive">{t("mcp.installMissingFields", { count: errors.length })}</p>
      ) : null}

      <div className="flex items-center justify-between gap-3 pt-1">
        {onCancel ? (
          <Button variant="ghost" size="sm" onClick={onCancel} disabled={submitting}>
            {t("common.cancel")}
          </Button>
        ) : (
          <span />
        )}
        <Button onClick={() => void handleSubmit()} disabled={submitting || blocked}>
          {submitting ? t("common.saving") : t("mcp.installConfirmAction")}
        </Button>
      </div>
    </div>
  );
}

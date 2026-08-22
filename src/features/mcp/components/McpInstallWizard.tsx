import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import type { McpInstallOutcome, McpInstallInputScope, McpInstallRejection, McpToolId } from "../../../types";
import { buildCommandConfirmation, buildEnvPreview } from "../lib/commandPreview";
import {
  buildInstallAnswers,
  buildInstallFields,
  secretFieldKeys,
  setFieldValue,
  setFieldVariable,
  validateInstallFields,
} from "../lib/installForm";
import type { McpMarketInstallSubmission } from "../hooks/useMcpServers";
import { useMcpInstallPlan } from "../hooks/useMcpInstallPlan";
import { useMcpInstallPreview } from "../hooks/useMcpInstallPreview";
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
 * confirmation is therefore the last step.
 *
 * Nothing here assembles a launch spec. The answers go back to the backend,
 * which folds them into the draft *before* the arguments are flattened and
 * returns both the entry and the command line it renders from it — so the
 * string the user approves and the object that gets installed are one
 * derivation. While that round trip is in flight the command on screen is
 * stale, which is why submit is disabled until it lands.
 *
 * Submitting sends the answers and the approved command, not an entry: the
 * backend derives it again from the catalog row as it stands at that moment
 * and refuses if that no longer renders what was approved. A refusal keeps
 * every value the user typed — it is an answer to be corrected or a command to
 * be re-read, never a form to be filled twice.
 */

interface McpInstallWizardProps {
  serverId: string;
  /** Tool ids to pre-enable, e.g. the ones the user already uses. */
  initialEnabled?: Readonly<Record<string, boolean>>;
  submitting: boolean;
  /** Commits through `mcp_market_install` and hands back its verdict. */
  onSubmit: (submission: McpMarketInstallSubmission) => Promise<McpInstallOutcome>;
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
  const [rejection, setRejection] = useState<McpInstallRejection | null>(null);
  // Bumped on a refused submit so the preview is re-derived from the row as it
  // stands now; re-approving the stale command on screen would be refused again.
  const [previewToken, setPreviewToken] = useState(0);

  if (plan && seededFor !== planKey) {
    setSeededFor(planKey);
    setFields(buildInstallFields(plan.inputs));
    setAcknowledged(false);
    setShowErrors(false);
    setRejection(null);
  }

  const errors = useMemo(() => validateInstallFields(fields), [fields]);
  const secretValues = useMemo(
    () =>
      fields.flatMap((field) => [
        ...(field.input.isSecret && !field.templated ? [field.value] : []),
        ...field.variables.filter((v) => v.variable.isSecret).map((v) => v.value),
      ]),
    [fields],
  );

  const answers = useMemo(() => buildInstallAnswers(fields), [fields]);
  // The plan's own pick, not the picker's state: the picker starts null and the
  // preview must not silently derive a different shape than the one on screen.
  const {
    preview,
    pending: previewPending,
    error: previewError,
  } = useMcpInstallPreview(plan ? serverId : null, plan?.selectedRuntimeId ?? null, answers, previewToken);

  // Until the first preview lands the plan's own draft *is* the preview: no
  // answer has been given yet, so the two derivations agree by construction.
  const entry = preview?.entry ?? plan?.draft ?? null;
  const confirmation = useMemo(
    () =>
      buildCommandConfirmation({
        preview: preview?.commandPreview ?? plan?.commandPreview,
        resolvedCommandPath: plan?.resolvedCommandPath,
        planPreview: plan?.commandPreview,
        secretValues,
        usesShell: plan?.usesShell ?? false,
      }),
    [preview, plan, secretValues],
  );

  if (isLoading || (!plan && isFetching)) {
    return (
      <div className="flex h-40 items-center justify-center">
        <LoadingLogo size="md" label={t("mcp.installPlanLoading")} />
      </div>
    );
  }

  if (error || !plan || !entry) {
    return (
      <div className="rounded-lg border border-destructive/20 bg-destructive/5 px-3 py-2.5 text-xs text-destructive">
        {error instanceof Error ? error.message : t("mcp.installPlanFailed")}
      </div>
    );
  }

  const secretKeys = secretFieldKeys(fields);
  const isLocal = confirmation.preview.length > 0;
  // A preview in flight means the command on screen is not the one that would
  // be installed, so approving it would approve a stale command.
  const blocked = errors.length > 0 || (isLocal && !acknowledged) || previewPending || preview == null;

  const handleFieldChange = (scope: McpInstallInputScope, index: number, value: string) => {
    setRejection(null);
    setFields((prev) => setFieldValue(prev, scope, index, value));
  };
  const handleVariableChange = (scope: McpInstallInputScope, index: number, variable: string, value: string) => {
    setRejection(null);
    setFields((prev) => setFieldVariable(prev, scope, index, variable, value));
  };

  const handleSubmit = async () => {
    if (errors.length > 0) {
      setShowErrors(true);
      return;
    }
    if (blocked || preview == null) return;
    setRejection(null);
    // The unmasked string the confirmation was rendered from. Masking is for
    // the screen; a masked command would never match what the backend derives.
    // A thrown failure is the caller's to report; only a refusal is rendered
    // here, because only a refusal is something to fix on this form.
    const outcome = await onSubmit({
      serverId,
      runtimeId: plan.selectedRuntimeId ?? null,
      answers,
      enabled,
      approvedPreview: preview.commandPreview ?? preview.entry.url ?? "",
    }).catch(() => null);
    if (outcome?.status !== "rejected") return;
    setRejection(outcome.rejection);
    // Whatever it was, the user has to look again before it can be approved.
    setAcknowledged(false);
    setPreviewToken((token) => token + 1);
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
          env={buildEnvPreview(entry.env, secretKeys)}
          headers={buildEnvPreview(entry.headers, secretKeys)}
          url={entry.url ?? null}
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

      {rejection ? (
        <p className="text-xs text-destructive">
          {rejection.reason === "missingInputs"
            ? t("mcp.installRejectedMissing", {
                fields: rejection.missing.map((input) => input.key).join(", "),
              })
            : t("mcp.installRejectedCommandChanged")}
        </p>
      ) : null}

      {previewError ? <p className="text-xs text-destructive">{previewError.message}</p> : null}

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

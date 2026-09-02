import { AlertTriangle, FileWarning, ShieldCheck, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { InsetPanel } from "../../../components/ui/InsetPanel";
import { cn } from "../../../lib/utils";
import type { McpSecretPolicy } from "../../../types";
import type { McpCommandConfirmation, McpEnvPreviewRow } from "../lib/commandPreview";

/**
 * The pre-install command confirmation.
 *
 * This is a spec MUST and the only effective mitigation for CursorJack-style
 * one-click install attacks (research §7 P1-6): before anything local runs, the
 * user sees the *complete, untruncated, already-resolved* command, the absolute
 * path of the binary it will exec, and the fact that no shell is involved.
 *
 * Deliberately not simplified for flow. The command line wraps instead of
 * truncating, because an ellipsis is exactly where a malicious tail would hide,
 * and the confirmation checkbox is required rather than implied by the Install
 * button — approving is a separate act from installing.
 */

interface McpCommandConfirmProps {
  confirmation: McpCommandConfirmation;
  env: McpEnvPreviewRow[];
  /** Header rows for remote servers; same masking rules as env. */
  headers: McpEnvPreviewRow[];
  url: string | null;
  warnings: readonly string[];
  secretPolicy: McpSecretPolicy;
  acknowledged: boolean;
  onAcknowledge: (next: boolean) => void;
  /** Local (stdio) installs execute a binary; remote ones do not. */
  requiresAcknowledgement: boolean;
}

function KeyValueTable({ rows, title }: { rows: McpEnvPreviewRow[]; title: string }) {
  if (rows.length === 0) return null;
  return (
    <div className="space-y-1.5">
      <p className="text-micro font-semibold uppercase tracking-wider text-muted-foreground">{title}</p>
      <div className="overflow-hidden rounded-lg border border-border/60">
        {rows.map((row) => (
          <div
            key={row.key}
            className="flex items-baseline gap-2 border-b border-border/40 px-2.5 py-1.5 text-[11px] last:border-b-0"
          >
            <span className="shrink-0 font-mono font-medium text-foreground">{row.key}</span>
            <span
              className={cn(
                "min-w-0 break-all font-mono",
                row.secret ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground",
              )}
            >
              {row.value || "—"}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function McpCommandConfirm({
  confirmation,
  env,
  headers,
  url,
  warnings,
  secretPolicy,
  acknowledged,
  onAcknowledge,
  requiresAcknowledgement,
}: McpCommandConfirmProps) {
  const { t } = useTranslation();

  return (
    <InsetPanel className="space-y-3">
      <p className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
        <Terminal className="h-3.5 w-3.5 text-primary" />
        {confirmation.preview ? t("mcp.confirmCommandTitle") : t("mcp.confirmEndpointTitle")}
      </p>

      {confirmation.preview ? (
        <>
          <pre className="max-h-56 overflow-auto whitespace-pre-wrap break-all rounded-lg border border-border/60 bg-muted/40 px-3 py-2 font-mono text-[11px] leading-relaxed text-foreground">
            {confirmation.preview}
          </pre>
          <dl className="space-y-1 text-[11px] text-muted-foreground">
            <div className="flex flex-wrap gap-1.5">
              <dt className="font-medium text-foreground">{t("mcp.confirmResolvedPath")}</dt>
              <dd className="min-w-0 break-all font-mono">
                {confirmation.resolvedPath ?? t("mcp.confirmResolvedPathUnknown")}
              </dd>
            </div>
            <div className="flex flex-wrap gap-1.5">
              <dt className="font-medium text-foreground">{t("mcp.confirmShell")}</dt>
              <dd>{confirmation.usesShell ? t("mcp.confirmShellYes") : t("mcp.confirmShellNo")}</dd>
            </div>
          </dl>
          {confirmation.editedSincePlan ? (
            <p className="flex items-start gap-1.5 rounded-lg bg-sky-500/10 px-2.5 py-1.5 text-[11px] leading-relaxed text-sky-700 dark:text-sky-300">
              <FileWarning className="mt-0.5 h-3 w-3 shrink-0" />
              {t("mcp.confirmEdited")}
            </p>
          ) : null}
        </>
      ) : (
        <p className="break-all rounded-lg border border-border/60 bg-muted/40 px-3 py-2 font-mono text-[11px] text-foreground">
          {url ?? "—"}
        </p>
      )}

      <KeyValueTable rows={env} title={t("mcp.fieldEnv")} />
      <KeyValueTable rows={headers} title={t("mcp.fieldHeaders")} />

      {warnings.map((warning) => (
        <p
          key={warning}
          className="flex items-start gap-1.5 rounded-lg bg-amber-500/10 px-2.5 py-1.5 text-[11px] leading-relaxed text-amber-700 dark:text-amber-300"
        >
          <AlertTriangle className="mt-0.5 h-3 w-3 shrink-0" />
          {warning}
        </p>
      ))}

      <div className="space-y-1.5 rounded-lg border border-border/50 bg-muted/25 px-2.5 py-2">
        <p className="flex items-center gap-1.5 text-[11px] font-medium text-foreground">
          <ShieldCheck className="h-3 w-3 text-primary" />
          {t("mcp.secretPolicyTitle")}
        </p>
        <p className="text-[11px] leading-relaxed text-muted-foreground">{secretPolicy.note}</p>
        {secretPolicy.writesProjectScopedConfig ? (
          <p className="text-[11px] font-medium leading-relaxed text-destructive">
            {t("mcp.secretPolicyProjectScoped")}
          </p>
        ) : (
          <p className="text-[11px] leading-relaxed text-muted-foreground/80">{t("mcp.secretPolicyUserScoped")}</p>
        )}
      </div>

      {requiresAcknowledgement ? (
        <label className="flex cursor-pointer items-start gap-2 text-[11px] leading-relaxed text-foreground">
          <input
            type="checkbox"
            checked={acknowledged}
            onChange={(event) => onAcknowledge(event.target.checked)}
            className="mt-0.5 h-3.5 w-3.5 shrink-0 accent-[var(--primary)]"
          />
          <span>{t("mcp.confirmAcknowledge")}</span>
        </label>
      ) : null}
    </InsetPanel>
  );
}

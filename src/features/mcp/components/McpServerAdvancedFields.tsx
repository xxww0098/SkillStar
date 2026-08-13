import { Info } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../components/ui/input";
import { cn } from "../../../lib/utils";
import type { McpToolId } from "../../../types";
import { MCP_TOOL_LABELS, type McpOptionalField, splitTargetsByFieldSupport } from "../lib/toolRegistry";

/**
 * Approval / exposure / timeout options, and — the part that was missing — who
 * actually honours them.
 *
 * These three fields are projected by a minority of targets: `autoApprove` only
 * reaches Kiro and Cline, `disabledTools` only Kiro, Codex and Gemini CLI,
 * `timeout` only OpenCode, Codex, Cline and Gemini CLI. Every other tool's
 * writer drops them. The form used to present all three unconditionally for all
 * targets (audit D.3-6), so a user could carefully restrict a server's tools for
 * Claude Code and get no restriction at all.
 *
 * The hint is computed against the targets *currently selected*, so it says
 * "Cursor and Claude Code will ignore this" rather than reciting a static
 * support matrix the user then has to cross-reference.
 */

const textareaCls =
  "w-full rounded-lg border border-border bg-background/60 px-3 py-2 text-xs font-mono text-foreground outline-none transition focus:border-primary/50 focus:ring-2 focus:ring-primary/20";

function FieldLabel({ children, hint }: { children: React.ReactNode; hint?: string }) {
  return (
    <label className="mb-1 block text-xs font-medium text-foreground">
      {children}
      {hint ? <span className="ml-1.5 font-normal text-muted-foreground">{hint}</span> : null}
    </label>
  );
}

function SupportNote({ field, enabledToolIds }: { field: McpOptionalField; enabledToolIds: readonly McpToolId[] }) {
  const { t } = useTranslation();
  const { honoured, ignored } = splitTargetsByFieldSupport(field, enabledToolIds);

  if (enabledToolIds.length === 0) {
    return (
      <p className="mt-1 text-[11px] leading-relaxed text-muted-foreground/80">
        {t("mcp.fieldSupportNoTargets", { tools: t(`mcp.fieldSupportList_${field}`) })}
      </p>
    );
  }

  if (ignored.length === 0) {
    return (
      <p className="mt-1 text-[11px] leading-relaxed text-emerald-600 dark:text-emerald-400">
        {t("mcp.fieldSupportAll")}
      </p>
    );
  }

  return (
    <p
      className={cn(
        "mt-1 flex items-start gap-1.5 text-[11px] leading-relaxed",
        honoured.length === 0 ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground",
      )}
    >
      <Info className="mt-0.5 h-3 w-3 shrink-0" />
      {honoured.length === 0
        ? t("mcp.fieldSupportNone", { ignored: ignored.map((id) => MCP_TOOL_LABELS[id]).join(", ") })
        : t("mcp.fieldSupportPartial", {
            honoured: honoured.map((id) => MCP_TOOL_LABELS[id]).join(", "),
            ignored: ignored.map((id) => MCP_TOOL_LABELS[id]).join(", "),
          })}
    </p>
  );
}

export interface McpServerAdvancedFieldsProps {
  enabledToolIds: readonly McpToolId[];
  autoApproveAll: boolean;
  onAutoApproveAllChange: (next: boolean) => void;
  autoApproveText: string;
  onAutoApproveTextChange: (next: string) => void;
  disabledToolsText: string;
  onDisabledToolsTextChange: (next: string) => void;
  timeoutText: string;
  onTimeoutTextChange: (next: string) => void;
}

export function McpServerAdvancedFields({
  enabledToolIds,
  autoApproveAll,
  onAutoApproveAllChange,
  autoApproveText,
  onAutoApproveTextChange,
  disabledToolsText,
  onDisabledToolsTextChange,
  timeoutText,
  onTimeoutTextChange,
}: McpServerAdvancedFieldsProps) {
  const { t } = useTranslation();

  return (
    <div className="space-y-3 rounded-xl border border-border/60 bg-background/30 p-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-xs font-medium text-foreground">{t("mcp.autoApproveAll")}</p>
          <p className="mt-0.5 text-[11px] leading-relaxed text-muted-foreground">{t("mcp.autoApproveAllHint")}</p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={autoApproveAll}
          onClick={() => onAutoApproveAllChange(!autoApproveAll)}
          className={cn(
            "relative h-5 w-9 shrink-0 rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
            autoApproveAll ? "bg-primary" : "bg-muted-foreground/30",
          )}
        >
          <span
            className={cn(
              "absolute top-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform",
              autoApproveAll ? "translate-x-4" : "translate-x-0.5",
            )}
          />
        </button>
      </div>

      {autoApproveAll ? (
        <p className="rounded-lg bg-amber-500/10 px-3 py-2 text-[11px] leading-relaxed text-amber-600 dark:text-amber-400">
          {t("mcp.yoloWarning")}
        </p>
      ) : (
        <div>
          <FieldLabel hint={t("mcp.toolListHint")}>{t("mcp.autoApproveTools")}</FieldLabel>
          <textarea
            value={autoApproveText}
            onChange={(event) => onAutoApproveTextChange(event.target.value)}
            rows={2}
            placeholder={"read_file\nlist_dir"}
            className={textareaCls}
          />
        </div>
      )}
      <SupportNote field="autoApprove" enabledToolIds={enabledToolIds} />

      <div>
        <FieldLabel hint={t("mcp.toolListHint")}>{t("mcp.disabledTools")}</FieldLabel>
        <textarea
          value={disabledToolsText}
          onChange={(event) => onDisabledToolsTextChange(event.target.value)}
          rows={2}
          placeholder={"delete_file\nexecute_command"}
          className={textareaCls}
        />
        <SupportNote field="disabledTools" enabledToolIds={enabledToolIds} />
      </div>

      <div>
        <FieldLabel hint={t("mcp.timeoutHint")}>{t("mcp.timeout")}</FieldLabel>
        <Input
          value={timeoutText}
          onChange={(event) => onTimeoutTextChange(event.target.value.replace(/[^0-9]/g, ""))}
          inputMode="numeric"
          placeholder="30000"
          className="h-9 font-mono"
        />
        <SupportNote field="timeout" enabledToolIds={enabledToolIds} />
      </div>
    </div>
  );
}

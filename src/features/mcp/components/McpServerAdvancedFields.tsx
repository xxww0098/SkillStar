import { CircleCheck, Info, TriangleAlert } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../components/ui/input";
import { InsetPanel } from "../../../components/ui/InsetPanel";
import { Textarea } from "../../../components/ui/textarea";
import { cn } from "../../../lib/utils";
import type { McpToolId } from "../../../types";
import {
  MCP_TOOL_LABELS,
  type McpOptionalField,
  mcpSupportLabels,
  splitTargetsByFieldSupport,
} from "../lib/toolRegistry";
import { MCP_HINT_CLS, MCP_NOTE_CLS, MCP_NOTE_GAP, MCP_TEXTAREA_CLS, McpFieldLabel } from "./McpFormField";

/**
 * Approval / exposure / timeout options, and — the part that was missing — who
 * actually honours them.
 *
 * These three fields are projected by a minority of targets; every other tool's
 * writer drops them. The authoritative set is `SUPPORTED_BY_FIELD` in
 * `lib/toolRegistry.ts` — read it there, don't copy it out, the copy in here
 * had already gone stale. The form used to present all three unconditionally
 * for all targets (audit D.3-6), so a user could carefully restrict a server's
 * tools for Claude Code and get no restriction at all.
 *
 * The hint is computed against the targets *currently selected*, so it says
 * "Codex writes this; the rest ignore it" rather than reciting a static support
 * matrix the user then has to cross-reference.
 */

type SupportTone = "muted" | "ok" | "warn";

/** One support verdict: icon in a fixed gutter, text on the note scale. */
const SUPPORT_TONE: Record<SupportTone, { icon: typeof Info; cls: string }> = {
  muted: { icon: Info, cls: "bg-muted/30 text-muted-foreground" },
  ok: { icon: CircleCheck, cls: "bg-emerald-500/10 text-emerald-600 paper:text-emerald-700" },
  warn: { icon: TriangleAlert, cls: "bg-amber-500/10 font-medium text-amber-600 paper:text-amber-700" },
};

function SupportLine({ tone, className, children }: { tone: SupportTone; className?: string; children: ReactNode }) {
  const { icon: Icon, cls } = SUPPORT_TONE[tone];
  return (
    <p className={cn("flex items-start gap-1.5 rounded-md px-2 py-1", MCP_NOTE_CLS, cls, className)}>
      <Icon className="mt-px h-3 w-3 shrink-0" />
      <span>{children}</span>
    </p>
  );
}

function SupportNote({
  field,
  enabledToolIds,
  className,
}: {
  field: McpOptionalField;
  enabledToolIds: readonly McpToolId[];
  /** Gap to the control above when the note is not a `space-y` sibling. */
  className?: string;
}) {
  const { t, i18n } = useTranslation();
  const { honoured, ignored } = splitTargetsByFieldSupport(field, enabledToolIds);

  // Chinese writes a list with 、, English with ", ". The supported set used to
  // be hand-copied into both locales as `fieldSupportList_*` and had already
  // drifted from the registry (timeout was missing DeepSeek Harness), so the
  // set now comes from `SUPPORTED_BY_FIELD` and only the separator is
  // locale-dependent.
  const separator = i18n.language.startsWith("zh") ? "、" : ", ";
  const join = (labels: readonly string[]) => labels.join(separator);
  const labelOf = (ids: readonly McpToolId[]) => join(ids.map((id) => MCP_TOOL_LABELS[id]));
  const supported = join(mcpSupportLabels(field));

  // Nothing picked yet: the picker above is empty, so the only useful fact is
  // which tools honour this field at all.
  if (enabledToolIds.length === 0) {
    return (
      <SupportLine tone="muted" className={className}>
        {t("mcp.fieldSupportNoTargets", { tools: supported })}
      </SupportLine>
    );
  }

  if (ignored.length === 0) {
    return (
      <SupportLine tone="ok" className={className}>
        {t("mcp.fieldSupportAll")}
      </SupportLine>
    );
  }

  // Nobody in the selection writes it. Re-listing the targets the user just
  // ticked says nothing they cannot already see above this field, so state the
  // verdict plainly and spend the line on the set that *would* honour it.
  if (honoured.length === 0) {
    return (
      <SupportLine tone="warn" className={className}>
        {t("mcp.fieldSupportNone", { tools: supported })}
      </SupportLine>
    );
  }

  // A nine-name river buries the one name that matters. Past a couple of names
  // "the rest" is both shorter and just as precise — the chips above are the
  // list, so naming them again is the same information twice. It is its own
  // sentence rather than a substituted list, because the template's space after
  // the placeholder reads wrong in Chinese once the value ends in 个.
  if (ignored.length > 2) {
    return (
      <SupportLine tone="muted" className={className}>
        {t("mcp.fieldSupportPartialRest", { honoured: labelOf(honoured), count: ignored.length })}
      </SupportLine>
    );
  }

  return (
    <SupportLine tone="muted" className={className}>
      {t("mcp.fieldSupportPartial", { honoured: labelOf(honoured), ignored: labelOf(ignored) })}
    </SupportLine>
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
    <InsetPanel className="bg-muted/10 shadow-2xs">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="text-xs font-semibold leading-none tracking-tight text-foreground">{t("mcp.autoApproveAll")}</p>
          <p className={cn("mt-1", MCP_HINT_CLS)}>{t("mcp.autoApproveAllHint")}</p>
        </div>
        <button
          type="button"
          role="switch"
          aria-checked={autoApproveAll}
          onClick={() => onAutoApproveAllChange(!autoApproveAll)}
          className={cn(
            "relative h-4.5 w-8 shrink-0 cursor-pointer rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
            autoApproveAll ? "bg-primary" : "bg-muted-foreground/30",
          )}
        >
          <span
            className={cn(
              "absolute top-0.5 h-3.5 w-3.5 rounded-full bg-white shadow-xs transition-transform duration-200",
              autoApproveAll ? "translate-x-3.5" : "translate-x-0.5",
            )}
          />
        </button>
      </div>

      {autoApproveAll ? (
        <SupportLine tone="warn">{t("mcp.yoloWarning")}</SupportLine>
      ) : (
        <div>
          <McpFieldLabel hint={t("mcp.toolListHint")}>{t("mcp.autoApproveTools")}</McpFieldLabel>
          <Textarea
            value={autoApproveText}
            onChange={(event) => onAutoApproveTextChange(event.target.value)}
            rows={2}
            placeholder={"read_file\nlist_dir"}
            className={MCP_TEXTAREA_CLS}
          />
        </div>
      )}
      <SupportNote field="autoApprove" enabledToolIds={enabledToolIds} />

      <div>
        <McpFieldLabel hint={t("mcp.toolListHint")}>{t("mcp.disabledTools")}</McpFieldLabel>
        <Textarea
          value={disabledToolsText}
          onChange={(event) => onDisabledToolsTextChange(event.target.value)}
          rows={2}
          placeholder={"delete_file\nexecute_command"}
          className={MCP_TEXTAREA_CLS}
        />
        <SupportNote field="disabledTools" enabledToolIds={enabledToolIds} className={MCP_NOTE_GAP} />
      </div>

      <div>
        <McpFieldLabel hint={t("mcp.timeoutHint")}>{t("mcp.timeout")}</McpFieldLabel>
        <Input
          value={timeoutText}
          onChange={(event) => onTimeoutTextChange(event.target.value.replace(/[^0-9]/g, ""))}
          inputMode="numeric"
          placeholder="30000"
          className="h-8 font-mono text-xs"
        />
        <SupportNote field="timeout" enabledToolIds={enabledToolIds} className={MCP_NOTE_GAP} />
      </div>
    </InsetPanel>
  );
}

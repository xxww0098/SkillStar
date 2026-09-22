import type { ReactNode } from "react";
import { cn } from "../../../lib/utils";

/**
 * Field chrome shared by the hand-edit form and its advanced block.
 *
 * Both files used to carry their own `FieldLabel` and their own textarea class
 * string, and they drifted: field labels rendered at 12px while the support
 * notes rendered at 13px, so an explanation outranked the field it explained,
 * and the two textareas had different minimum heights. One scale, one
 * definition.
 *
 * The scale is label 12px > hint / note 11px, with mono field text at 11.5px.
 * Nothing that explains a field may render larger than the field's own label.
 */
export const MCP_TEXTAREA_CLS =
  "min-h-[3.5rem] resize-y rounded-lg border-input-border bg-input/80 px-2.5 py-1.5 font-mono text-[11.5px] leading-relaxed transition-colors focus-visible:bg-input";

export const MCP_HINT_CLS = "text-[11px] font-normal text-muted-foreground";

/** Explanatory line under a control. Colour is the caller's business. */
export const MCP_NOTE_CLS = "text-[11px] leading-snug";

/**
 * Gap between a control and the explanatory line directly beneath it, for the
 * cases where the two are not `space-y` siblings. One value everywhere keeps
 * hints and support verdicts on the same rhythm.
 */
export const MCP_NOTE_GAP = "mt-1.5";

export interface McpFieldLabelProps {
  children: ReactNode;
  /** Muted hint pinned to the right edge of the label row. */
  hint?: string;
  optional?: boolean;
  optionalLabel?: string;
}

/** Field name on the left, optional muted hint flush right. */
export function McpFieldLabel({ children, hint, optional, optionalLabel }: McpFieldLabelProps) {
  return (
    <div className="mb-1 flex items-baseline justify-between gap-2">
      <label className="flex items-center gap-1.5 text-xs font-medium leading-none tracking-tight text-foreground">
        {children}
        {optional ? (
          <span className="text-[11px] font-normal text-muted-foreground/75">({optionalLabel ?? "可选"})</span>
        ) : null}
      </label>
      {hint ? <p className={cn(MCP_HINT_CLS, "text-right leading-none")}>{hint}</p> : null}
    </div>
  );
}

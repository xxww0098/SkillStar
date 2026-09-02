import { ClipboardPaste, LoaderCircle } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpPasteParse } from "../../../types";

interface McpImportBarProps {
  onParsed: (parsed: McpPasteParse, raw: string) => void;
  disabled?: boolean;
}

/**
 * Paste-anything entry on the fleet page.
 *
 * Parsing is backend-owned. This bar never installs — it only asks the user
 * to review whatever `parse_mcp_paste` returned.
 */
export function McpImportBar({ onParsed, disabled }: McpImportBarProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    const raw = value.trim();
    if (!raw || pending || disabled) return;
    setPending(true);
    setError(null);
    try {
      const parsed = await tauriInvoke("parse_mcp_paste", { text: raw });
      if (
        parsed.kind === "empty" ||
        (parsed.kind === "unknown" && (parsed.drafts?.length ?? 0) === 0 && !parsed.catalogId)
      ) {
        setError(parsed.error ?? t("mcp.pasteUnknown"));
        return;
      }
      onParsed(parsed, raw);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setPending(false);
    }
  };

  return (
    <section className="rounded-xl border border-border/70 bg-sidebar/40 p-3">
      <label className="mb-1.5 flex items-center gap-1.5 text-xs font-medium text-foreground">
        <ClipboardPaste className="h-3.5 w-3.5 text-primary" />
        {t("mcp.pasteTitle")}
      </label>
      <div className="flex flex-col gap-2 sm:flex-row">
        <textarea
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            if (error) setError(null);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              void submit();
            }
          }}
          rows={2}
          disabled={pending || disabled}
          placeholder={t("mcp.pastePlaceholder")}
          className="min-h-16 w-full resize-y rounded-lg border border-border/70 bg-background/70 px-3 py-2 font-mono text-xs leading-relaxed text-foreground placeholder:text-muted-foreground/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
        />
        <Button
          type="button"
          size="sm"
          className="shrink-0 self-end sm:self-stretch"
          onClick={() => void submit()}
          disabled={pending || disabled || value.trim().length === 0}
        >
          {pending ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <ClipboardPaste className="h-3.5 w-3.5" />}
          {pending ? t("mcp.pasteParsing") : t("mcp.pasteReview")}
        </Button>
      </div>
      <p className="mt-1.5 text-[11px] text-muted-foreground">{t("mcp.pasteHint")}</p>
      {error ? <p className="mt-1 text-[11px] text-destructive">{error}</p> : null}
    </section>
  );
}

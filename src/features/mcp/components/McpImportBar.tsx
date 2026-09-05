import { ClipboardPaste, LoaderCircle } from "lucide-react";
import { useEffect, useState, type DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import { tauriInvoke } from "../../../lib/ipc";
import type { McpPasteParse } from "../../../types";
import { mcpServerCommandLine } from "../lib/pasteDraft";

interface McpImportBarProps {
  onParsed: (parsed: McpPasteParse, raw: string) => void;
  disabled?: boolean;
  initialText?: string;
}

const PREVIEW_MS = 300;

function isUsefulParse(parsed: McpPasteParse): boolean {
  if (parsed.kind === "empty") return false;
  if (parsed.catalogId) return true;
  return (parsed.drafts?.length ?? 0) > 0;
}

/**
 * Paste-anything control on the fleet page (Hermes compact import).
 *
 * Parsing is backend-owned. This bar never installs — it previews whatever
 * `parse_mcp_paste` returned, then the parent opens the existing confirm path.
 */
export function McpImportBar({ onParsed, disabled, initialText }: McpImportBarProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState(initialText ?? "");
  const [pending, setPending] = useState(false);
  const [previewPending, setPreviewPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<McpPasteParse | null>(null);
  const [previewFor, setPreviewFor] = useState<string | null>(null);
  const [dropping, setDropping] = useState(false);

  const parseText = async (raw: string, mode: "preview" | "submit") => {
    const text = raw.trim();
    if (!text) {
      setPreview(null);
      setPreviewFor(null);
      setError(null);
      return null;
    }
    if (mode === "submit") setPending(true);
    else setPreviewPending(true);
    try {
      const parsed = await tauriInvoke("parse_mcp_paste", { text });
      if (!isUsefulParse(parsed)) {
        setPreview(null);
        setPreviewFor(text);
        setError(parsed.error ?? t("mcp.pasteUnknown"));
        return null;
      }
      setError(null);
      setPreview(parsed);
      setPreviewFor(text);
      return parsed;
    } catch (err) {
      setPreview(null);
      setPreviewFor(text);
      setError(err instanceof Error ? err.message : String(err));
      return null;
    } finally {
      if (mode === "submit") setPending(false);
      else setPreviewPending(false);
    }
  };

  useEffect(() => {
    const raw = value.trim();
    if (!raw) {
      setPreview(null);
      setPreviewFor(null);
      setError(null);
      return;
    }
    const timer = window.setTimeout(() => {
      void parseText(raw, "preview");
    }, PREVIEW_MS);
    return () => window.clearTimeout(timer);
  }, [value]);

  const submit = async () => {
    const raw = value.trim();
    if (!raw || pending || disabled) return;
    const parsed = preview && previewFor === raw && isUsefulParse(preview) ? preview : await parseText(raw, "submit");
    if (parsed) onParsed(parsed, raw);
  };

  const applyDropped = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    setDropping(false);
    const text = event.dataTransfer.getData("text/plain") || event.dataTransfer.getData("text/uri-list");
    if (text.trim()) {
      setValue(text);
      setError(null);
    }
  };

  const drafts = preview?.drafts ?? [];

  return (
    <section
      onDragEnter={(event) => {
        event.preventDefault();
        setDropping(true);
      }}
      onDragOver={(event) => {
        event.preventDefault();
        setDropping(true);
      }}
      onDragLeave={() => setDropping(false)}
      onDrop={applyDropped}
      className={cn(
        "rounded-xl border bg-sidebar/40 p-2 transition-colors duration-150",
        dropping ? "border-primary/60 bg-primary/5" : "border-border/70",
      )}
    >
      <div className="flex items-start gap-2">
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
          aria-label={t("mcp.pasteTitle")}
          className="min-h-10 w-full resize-y rounded-lg border border-border/70 bg-background/70 px-3 py-2 font-mono text-xs leading-relaxed text-foreground placeholder:text-muted-foreground/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
        />
        <Button
          type="button"
          size="sm"
          className="h-10 shrink-0 self-stretch"
          onClick={() => void submit()}
          disabled={pending || disabled || value.trim().length === 0}
        >
          {pending ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <ClipboardPaste className="h-3.5 w-3.5" />}
          {pending ? t("mcp.pasteParsing") : t("mcp.pasteReview")}
        </Button>
      </div>
      {dropping ? <p className="mt-1.5 text-[11px] text-primary">{t("mcp.pasteDropHint")}</p> : null}
      {previewPending && !preview ? (
        <p className="mt-1.5 text-[11px] text-muted-foreground">{t("mcp.pasteParsing")}</p>
      ) : null}
      {preview?.catalogId ? (
        <p className="mt-1.5 truncate text-[11px] text-foreground/80">
          {t("mcp.pastePreviewCatalog", { id: preview.catalogId })}
        </p>
      ) : null}
      {drafts.length > 0 ? (
        <ul className="mt-1.5 space-y-1">
          {drafts.map((draft, index) => (
            <li key={`${draft.name}-${index}`} className="rounded-md bg-background/60 px-2 py-1">
              <span className="block truncate text-[12px] font-medium text-foreground">{draft.name}</span>
              <span className="block truncate font-mono text-[11px] text-muted-foreground">
                {mcpServerCommandLine(draft) || draft.transport}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      {error ? <p className="mt-1 text-[11px] text-destructive">{error}</p> : null}
    </section>
  );
}

import { ClipboardPaste, Download, LoaderCircle, PenLine, Sparkles } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { tauriInvoke } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";
import type { McpPasteParse, McpPreset, McpToolId } from "../../../types";
import type { McpAgentTarget } from "../lib/agentTargets";
import { mcpServerCommandLine } from "../lib/pasteDraft";
import { McpRecommendedPresets } from "./McpRecommendedPresets";
import { McpServerForm, type McpServerFormValue } from "./McpServerForm";

/**
 * How a server gets added. One modal, four sources — a new server is either
 * picked from the curated recommendations, typed in by hand, pasted in (JSON
 * snippet, URL, command line or `skillstar://mcp` deep link) or lifted out of
 * an Agent's existing live config.
 */
export type McpAddMode = "recommended" | "manual" | "paste" | "import";

const MODES: Array<{ id: McpAddMode; label: string; icon: ReactNode }> = [
  { id: "recommended", label: "mcp.addModeRecommended", icon: <Sparkles className="h-3.5 w-3.5" /> },
  { id: "manual", label: "mcp.addModeManual", icon: <PenLine className="h-3.5 w-3.5" /> },
  { id: "paste", label: "mcp.addModePaste", icon: <ClipboardPaste className="h-3.5 w-3.5" /> },
  { id: "import", label: "mcp.addModeImport", icon: <Download className="h-3.5 w-3.5" /> },
];

const PREVIEW_MS = 300;

export interface McpAddDialogProps {
  mode: McpAddMode;
  onModeChange: (mode: McpAddMode) => void;
  presets: readonly McpPreset[];
  /** Installed config keys, lowercased — already-added chips stay hidden. */
  installedNames: ReadonlySet<string>;
  /** Re-mounts the manual form when a preset seeds fresh defaults. */
  formKey: number;
  defaults?: Partial<McpServerFormValue>;
  /** Paste text pushed in from a deep link; the nonce re-mounts the field. */
  pasteSeed: { key: number; text: string };
  submitting: boolean;
  importing: boolean;
  noteForTool?: (toolId: McpToolId) => string | null;
  targets: readonly McpAgentTarget[];
  onPickPreset: (preset: McpPreset) => void;
  onSubmit: (value: McpServerFormValue) => Promise<void> | void;
  onImport: () => void;
  onParsed: (parsed: McpPasteParse) => void;
}

function isUsefulParse(parsed: McpPasteParse): boolean {
  if (parsed.kind === "empty") return false;
  if (parsed.catalogId) return true;
  return (parsed.drafts?.length ?? 0) > 0;
}

/**
 * Paste-anything field. Parsing is backend-owned (`parse_mcp_paste`); this
 * never installs — it previews what came back, then the dialog hands the
 * result to the same confirm path a click on a catalog card would use.
 */
function McpPasteField({
  initialText,
  disabled,
  onParsed,
}: {
  initialText: string;
  disabled?: boolean;
  onParsed: (parsed: McpPasteParse) => void;
}) {
  const { t } = useTranslation();
  const [value, setValue] = useState(initialText);
  const [pending, setPending] = useState(false);
  const [preview, setPreview] = useState<McpPasteParse | null>(null);
  const [previewFor, setPreviewFor] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const parseText = async (raw: string, mode: "preview" | "submit") => {
    const text = raw.trim();
    if (!text) {
      setPreview(null);
      setPreviewFor(null);
      setError(null);
      return null;
    }
    if (mode === "submit") setPending(true);
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
    if (parsed) onParsed(parsed);
  };

  const drafts = preview?.drafts ?? [];

  return (
    <div className="space-y-2">
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
          rows={4}
          disabled={pending || disabled}
          placeholder={t("mcp.pastePlaceholder")}
          aria-label={t("mcp.pasteTitle")}
          className="min-h-24 w-full resize-y rounded-lg border border-border/70 bg-background/70 px-3 py-2 font-mono text-xs leading-relaxed text-foreground placeholder:text-muted-foreground/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40"
        />
      </div>
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] text-muted-foreground">{t("mcp.pasteHint")}</p>
        <Button
          type="button"
          size="sm"
          className="h-8 shrink-0 gap-1.5"
          onClick={() => void submit()}
          disabled={pending || disabled || value.trim().length === 0}
        >
          {pending ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <ClipboardPaste className="h-3.5 w-3.5" />}
          {pending ? t("mcp.pasteParsing") : t("mcp.pasteReview")}
        </Button>
      </div>
      {preview?.catalogId ? (
        <p className="truncate text-[11px] text-foreground/80">
          {t("mcp.pastePreviewCatalog", { id: preview.catalogId })}
        </p>
      ) : null}
      {drafts.length > 0 ? (
        <ul className="space-y-1">
          {drafts.map((draft, index) => (
            <li key={`${draft.name}-${index}`} className="rounded-md bg-muted/40 px-2 py-1">
              <span className="block truncate text-[12px] font-medium text-foreground">{draft.name}</span>
              <span className="block truncate font-mono text-[11px] text-muted-foreground">
                {mcpServerCommandLine(draft) || draft.transport}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
      {error ? <p className="text-[11px] text-destructive">{error}</p> : null}
    </div>
  );
}

/**
 * The single "add a server" surface.
 *
 * The config page used to expose three separate entry points for this — a
 * permanent paste bar above the list, an "import from tools" toolbar button and
 * an "add" button whose modal then offered a fourth (presets) — which is four
 * ways to answer one question. They are all here now, behind one mode switch.
 */
export function McpAddDialog({
  mode,
  onModeChange,
  presets,
  installedNames,
  formKey,
  defaults,
  pasteSeed,
  submitting,
  importing,
  noteForTool,
  targets,
  onPickPreset,
  onSubmit,
  onImport,
  onParsed,
}: McpAddDialogProps) {
  const { t } = useTranslation();
  const hasRecommended = presets.some((preset) => !installedNames.has(preset.name.trim().toLowerCase()));

  return (
    <div className="space-y-3.5">
      <div role="group" aria-label={t("mcp.addServer")} className="flex flex-wrap gap-1">
        {MODES.map(({ id, label, icon }) => (
          <button
            key={id}
            type="button"
            aria-pressed={mode === id}
            onClick={() => onModeChange(id)}
            className={cn(
              "inline-flex cursor-pointer items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs transition-colors duration-150 focus-ring",
              mode === id
                ? "border-primary/60 bg-primary/10 font-semibold text-primary"
                : "border-border/70 bg-background/60 font-medium text-muted-foreground hover:bg-muted/40 hover:text-foreground",
            )}
          >
            {icon}
            {t(label)}
          </button>
        ))}
      </div>

      {mode === "recommended" ? (
        hasRecommended ? (
          <McpRecommendedPresets presets={presets} installedNames={installedNames} onPick={onPickPreset} />
        ) : (
          <p className="rounded-xl border border-border/70 bg-muted/20 px-3.5 py-3 text-xs text-muted-foreground">
            {t("mcp.addRecommendedEmpty")}
          </p>
        )
      ) : null}

      {mode === "manual" ? (
        <McpServerForm
          key={formKey}
          defaults={defaults}
          onSubmit={onSubmit}
          submitting={submitting}
          noteForTool={noteForTool}
          targets={targets}
        />
      ) : null}

      {mode === "paste" ? (
        <McpPasteField key={pasteSeed.key} initialText={pasteSeed.text} disabled={submitting} onParsed={onParsed} />
      ) : null}

      {mode === "import" ? (
        <div className="space-y-3">
          <p className="text-xs leading-relaxed text-muted-foreground">{t("mcp.addModeImportHint")}</p>
          <Button type="button" variant="outline" onClick={onImport} disabled={importing}>
            {importing ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <Download className="h-3.5 w-3.5" />}
            {t("mcp.importFromTools")}
          </Button>
        </div>
      ) : null}
    </div>
  );
}

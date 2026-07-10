import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { cn } from "../../../lib/utils";
import { MCP_TOOL_IDS, type McpServerEntry, type McpToolId } from "../../../types";

const TOOL_LABELS: Record<McpToolId, string> = {
  "claude-code": "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  grok: "Grok",
  opencode: "OpenCode",
  zcode: "ZCode",
  kiro: "Kiro",
  cursor: "Cursor",
};

export interface McpServerFormValue {
  name: string;
  transport: string;
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  cwd?: string;
  url?: string;
  headers?: Record<string, string>;
  description?: string;
  homepage?: string;
  enabled: Record<string, boolean>;
  autoApproveAll?: boolean;
  autoApproveTools?: string[];
  disabledTools?: string[];
  timeoutMs?: number | null;
}

interface McpServerFormProps {
  /** Existing server when editing; undefined when creating. */
  initial?: McpServerEntry;
  /** Seed values for the create case (e.g. from a recommended preset). */
  defaults?: Partial<McpServerFormValue>;
  onSubmit: (value: McpServerFormValue) => Promise<void> | void;
  onDelete?: () => Promise<void> | void;
  submitting?: boolean;
  /** Override the submit button label (defaults to Save/Add). */
  submitLabel?: string;
}

/** Parse a "KEY=VALUE per line" block into a record. */
function parseKv(text: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    const eq = line.indexOf("=");
    if (eq <= 0) continue;
    out[line.slice(0, eq).trim()] = line.slice(eq + 1).trim();
  }
  return out;
}

function kvToText(rec?: Record<string, string>): string {
  if (!rec) return "";
  return Object.entries(rec)
    .map(([k, v]) => `${k}=${v}`)
    .join("\n");
}

/** Parse a comma/newline-separated list into a trimmed, de-duped string array. */
function parseList(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of text.split(/[\n,]/)) {
    const item = raw.trim();
    if (item && !seen.has(item)) {
      seen.add(item);
      out.push(item);
    }
  }
  return out;
}

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

export function McpServerForm({ initial, defaults, onSubmit, onDelete, submitting, submitLabel }: McpServerFormProps) {
  const { t } = useTranslation();
  const [name, setName] = useState(initial?.name ?? defaults?.name ?? "");
  const [transport, setTransport] = useState(initial?.transport ?? defaults?.transport ?? "stdio");
  const [command, setCommand] = useState(initial?.command ?? defaults?.command ?? "");
  const [argsText, setArgsText] = useState((initial?.args ?? defaults?.args ?? []).join("\n"));
  const [envText, setEnvText] = useState(kvToText(initial?.env ?? defaults?.env));
  const [cwd, setCwd] = useState(initial?.cwd ?? defaults?.cwd ?? "");
  const [url, setUrl] = useState(initial?.url ?? defaults?.url ?? "");
  const [headersText, setHeadersText] = useState(kvToText(initial?.headers ?? defaults?.headers));
  const [description, setDescription] = useState(initial?.description ?? defaults?.description ?? "");
  const [homepage, setHomepage] = useState(initial?.homepage ?? defaults?.homepage ?? "");
  const [enabled, setEnabled] = useState<Record<string, boolean>>(initial?.enabled ?? defaults?.enabled ?? {});
  const [autoApproveAll, setAutoApproveAll] = useState(initial?.autoApproveAll ?? defaults?.autoApproveAll ?? false);
  const [autoApproveText, setAutoApproveText] = useState(
    (initial?.autoApproveTools ?? defaults?.autoApproveTools ?? []).join("\n"),
  );
  const [disabledToolsText, setDisabledToolsText] = useState(
    (initial?.disabledTools ?? defaults?.disabledTools ?? []).join("\n"),
  );
  const [timeoutText, setTimeoutText] = useState(
    initial?.timeoutMs != null
      ? String(initial.timeoutMs)
      : defaults?.timeoutMs != null
        ? String(defaults.timeoutMs)
        : "",
  );
  const [error, setError] = useState<string | null>(null);

  const isRemote = transport === "http" || transport === "sse";

  const handleSubmit = async () => {
    setError(null);
    if (!name.trim()) {
      setError(t("mcp.errorNameRequired"));
      return;
    }
    if (isRemote && !url.trim()) {
      setError(t("mcp.errorUrlRequired"));
      return;
    }
    if (!isRemote && !command.trim()) {
      setError(t("mcp.errorCommandRequired"));
      return;
    }
    const value: McpServerFormValue = {
      name: name.trim(),
      transport,
      description: description.trim() || undefined,
      homepage: homepage.trim() || undefined,
      enabled,
      autoApproveAll,
      autoApproveTools: autoApproveAll ? [] : parseList(autoApproveText),
      disabledTools: parseList(disabledToolsText),
      timeoutMs: timeoutText.trim() ? Math.max(0, Math.round(Number(timeoutText.trim()))) || null : null,
    };
    if (isRemote) {
      value.url = url.trim();
      value.headers = parseKv(headersText);
    } else {
      value.command = command.trim();
      value.args = argsText
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean);
      value.env = parseKv(envText);
      value.cwd = cwd.trim() || undefined;
    }
    await onSubmit(value);
  };

  return (
    <div className="space-y-5">
      <div>
        <FieldLabel hint={t("mcp.fieldNameHint")}>{t("mcp.fieldName")}</FieldLabel>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("mcp.fieldNamePlaceholder")}
          className="h-9"
        />
      </div>

      <div>
        <FieldLabel>{t("mcp.fieldTransport")}</FieldLabel>
        <div className="flex gap-2">
          {(["stdio", "http", "sse"] as const).map((t) => (
            <button
              key={t}
              type="button"
              onClick={() => setTransport(t)}
              className={cn(
                "flex-1 rounded-lg border px-3 py-1.5 text-xs font-medium transition",
                transport === t
                  ? "border-primary/60 bg-primary/10 text-primary"
                  : "border-border bg-background/40 text-muted-foreground hover:bg-muted/40",
              )}
            >
              {t}
            </button>
          ))}
        </div>
      </div>

      {isRemote ? (
        <>
          <div>
            <FieldLabel>URL</FieldLabel>
            <Input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://mcp.example.com/sse"
              className="h-9 font-mono"
            />
          </div>
          <div>
            <FieldLabel hint={t("mcp.kvHint")}>{t("mcp.fieldHeaders")}</FieldLabel>
            <textarea
              value={headersText}
              onChange={(e) => setHeadersText(e.target.value)}
              rows={3}
              placeholder={"Authorization=Bearer xxx"}
              className={textareaCls}
            />
          </div>
        </>
      ) : (
        <>
          <div>
            <FieldLabel>{t("mcp.fieldCommand")}</FieldLabel>
            <Input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="npx"
              className="h-9 font-mono"
            />
          </div>
          <div>
            <FieldLabel hint={t("mcp.oneLineHint")}>{t("mcp.fieldArgs")}</FieldLabel>
            <textarea
              value={argsText}
              onChange={(e) => setArgsText(e.target.value)}
              rows={3}
              placeholder={"-y\n@upstash/context7-mcp"}
              className={textareaCls}
            />
          </div>
          <div>
            <FieldLabel hint={t("mcp.kvHint")}>{t("mcp.fieldEnv")}</FieldLabel>
            <textarea
              value={envText}
              onChange={(e) => setEnvText(e.target.value)}
              rows={2}
              placeholder={"API_KEY=sk-xxx"}
              className={textareaCls}
            />
          </div>
          <div>
            <FieldLabel hint={t("common.optional")}>{t("mcp.fieldCwd")}</FieldLabel>
            <Input
              value={cwd}
              onChange={(e) => setCwd(e.target.value)}
              placeholder="/path/to/dir"
              className="h-9 font-mono"
            />
          </div>
        </>
      )}

      <div className="grid grid-cols-2 gap-3">
        <div>
          <FieldLabel hint={t("common.optional")}>{t("mcp.fieldDescription")}</FieldLabel>
          <Input value={description} onChange={(e) => setDescription(e.target.value)} className="h-9" />
        </div>
        <div>
          <FieldLabel hint={t("common.optional")}>{t("mcp.homepage")}</FieldLabel>
          <Input
            value={homepage}
            onChange={(e) => setHomepage(e.target.value)}
            className="h-9"
            placeholder="https://"
          />
        </div>
      </div>

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
            onClick={() => setAutoApproveAll((v) => !v)}
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
              onChange={(e) => setAutoApproveText(e.target.value)}
              rows={2}
              placeholder={"read_file\nlist_dir"}
              className={textareaCls}
            />
          </div>
        )}

        <div>
          <FieldLabel hint={t("mcp.toolListHint")}>{t("mcp.disabledTools")}</FieldLabel>
          <textarea
            value={disabledToolsText}
            onChange={(e) => setDisabledToolsText(e.target.value)}
            rows={2}
            placeholder={"delete_file\nexecute_command"}
            className={textareaCls}
          />
        </div>

        <div>
          <FieldLabel hint={t("mcp.timeoutHint")}>{t("mcp.timeout")}</FieldLabel>
          <Input
            value={timeoutText}
            onChange={(e) => setTimeoutText(e.target.value.replace(/[^0-9]/g, ""))}
            inputMode="numeric"
            placeholder="30000"
            className="h-9 font-mono"
          />
        </div>

        <p className="text-[11px] leading-relaxed text-muted-foreground/80">{t("mcp.approvalSupportNote")}</p>
      </div>

      <div>
        <FieldLabel hint={t("mcp.fieldEnabledToolsHint")}>{t("mcp.fieldEnabledTools")}</FieldLabel>
        <div className="grid grid-cols-2 gap-2">
          {MCP_TOOL_IDS.map((toolId) => {
            const on = enabled[toolId] ?? false;
            return (
              <button
                key={toolId}
                type="button"
                onClick={() => setEnabled((prev) => ({ ...prev, [toolId]: !on }))}
                className={cn(
                  "flex items-center justify-between rounded-lg border px-3 py-2 text-xs transition",
                  on
                    ? "border-primary/50 bg-primary/10 text-foreground"
                    : "border-border bg-background/40 text-muted-foreground hover:bg-muted/40",
                )}
              >
                <span>{TOOL_LABELS[toolId]}</span>
                <span className={cn("h-2 w-2 rounded-full", on ? "bg-primary" : "bg-muted-foreground/30")} />
              </button>
            );
          })}
        </div>
      </div>

      {error ? <p className="text-xs text-destructive">{error}</p> : null}

      <div className="flex items-center justify-between gap-3 pt-1">
        {onDelete ? (
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:bg-destructive/10"
            onClick={() => void onDelete()}
          >
            {t("common.delete")}
          </Button>
        ) : (
          <span />
        )}
        <Button onClick={() => void handleSubmit()} disabled={submitting}>
          {submitting ? t("common.saving") : (submitLabel ?? (initial ? t("common.save") : t("common.add")))}
        </Button>
      </div>
    </div>
  );
}

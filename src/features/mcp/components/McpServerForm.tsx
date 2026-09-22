import { Folder, Globe, KeyRound, Terminal } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { Textarea } from "../../../components/ui/textarea";
import { cn } from "../../../lib/utils";
import type { McpServerEntry, McpToolId } from "../../../types";
import type { McpAgentTarget } from "../lib/agentTargets";
import { kvToText, parseKv, parseList } from "../lib/kv";
import { enabledMcpToolIds } from "../lib/toolRegistry";
import { MCP_HINT_CLS, MCP_NOTE_GAP, MCP_TEXTAREA_CLS, McpFieldLabel } from "./McpFormField";
import { McpServerAdvancedFields } from "./McpServerAdvancedFields";
import { McpToolTargetPicker } from "./McpToolTargetPicker";
import { McpTransportPicker } from "./McpTransportPicker";

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
  /** Per-tool note for the target picker, e.g. "not installed". */
  noteForTool?: (toolId: McpToolId) => string | null;
  /** Settings-enabled MCP agents; picker length follows this list. */
  targets?: readonly McpAgentTarget[];
}

/**
 * Hand-edit form for one MCP server.
 *
 * Two things moved out of here and are worth knowing about:
 * - `KEY=VALUE` parsing lives in `../lib/kv`, which no longer trims the *value*
 *   unconditionally. The old parser silently rewrote any credential with edge
 *   whitespace, which the server then rejects for reasons nothing in the UI
 *   explains.
 * - The approval / exposure / timeout block lives in
 *   `./McpServerAdvancedFields`, which says per selected target whether the
 *   field will actually be written.
 * - Transport labels live in `./McpTransportPicker`. The stored token `http`
 *   means Streamable HTTP (2026-07-28, stateless); showing the raw token next
 *   to `sse` hid that from anyone filling this form by hand.
 */
export function McpServerForm({
  initial,
  defaults,
  onSubmit,
  onDelete,
  submitting,
  submitLabel,
  noteForTool,
  targets = [],
}: McpServerFormProps) {
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
  const enabledToolIds = useMemo(() => enabledMcpToolIds(enabled), [enabled]);

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
    <div className="flex flex-col gap-3.5">
      <div className="space-y-2.5">
        <div>
          <McpFieldLabel hint={t("mcp.fieldNameHint")}>{t("mcp.fieldName")}</McpFieldLabel>
          <div className="relative">
            <KeyRound className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("mcp.fieldNamePlaceholder")}
              className="h-8 pl-8 text-xs"
            />
          </div>
        </div>
        <McpTransportPicker value={transport} onChange={setTransport} />
      </div>

      {isRemote ? (
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <McpFieldLabel>{t("mcp.fieldUrl")}</McpFieldLabel>
            <div className="relative">
              <Globe className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
              <Input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder={t(transport === "sse" ? "mcp.fieldUrlPlaceholderSse" : "mcp.fieldUrlPlaceholderHttp")}
                className="h-8 pl-8 font-mono text-xs"
              />
            </div>
          </div>
          <div>
            <McpFieldLabel hint={t("mcp.kvHint")}>{t("mcp.fieldHeaders")}</McpFieldLabel>
            <Textarea
              value={headersText}
              onChange={(e) => setHeadersText(e.target.value)}
              rows={2}
              placeholder={"Authorization=Bearer xxx"}
              className={MCP_TEXTAREA_CLS}
            />
            <p className={cn(MCP_NOTE_GAP, MCP_HINT_CLS)}>{t("mcp.kvQuotingHint")}</p>
          </div>
        </div>
      ) : (
        <div className="space-y-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <McpFieldLabel>{t("mcp.fieldCommand")}</McpFieldLabel>
              <div className="relative">
                <Terminal className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
                <Input
                  value={command}
                  onChange={(e) => setCommand(e.target.value)}
                  placeholder="npx"
                  className="h-8 pl-8 font-mono text-xs"
                />
              </div>
            </div>
            <div>
              <McpFieldLabel optional optionalLabel={t("common.optional")}>
                {t("mcp.fieldCwd")}
              </McpFieldLabel>
              <div className="relative">
                <Folder className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
                <Input
                  value={cwd}
                  onChange={(e) => setCwd(e.target.value)}
                  placeholder="/path/to/dir"
                  className="h-8 pl-8 font-mono text-xs"
                />
              </div>
            </div>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <McpFieldLabel hint={t("mcp.oneLineHint")}>{t("mcp.fieldArgs")}</McpFieldLabel>
              <Textarea
                value={argsText}
                onChange={(e) => setArgsText(e.target.value)}
                rows={2}
                placeholder={"-y\n@upstash/context7-mcp"}
                className={MCP_TEXTAREA_CLS}
              />
            </div>
            <div>
              <McpFieldLabel hint={t("mcp.kvHint")}>{t("mcp.fieldEnv")}</McpFieldLabel>
              <Textarea
                value={envText}
                onChange={(e) => setEnvText(e.target.value)}
                rows={2}
                placeholder={"API_KEY=sk-xxx"}
                className={MCP_TEXTAREA_CLS}
              />
              <p className={cn(MCP_NOTE_GAP, MCP_HINT_CLS)}>{t("mcp.kvQuotingHint")}</p>
            </div>
          </div>
        </div>
      )}

      <div className="grid grid-cols-2 gap-3">
        <div>
          <McpFieldLabel optional optionalLabel={t("common.optional")}>
            {t("mcp.fieldDescription")}
          </McpFieldLabel>
          <Input value={description} onChange={(e) => setDescription(e.target.value)} className="h-8 text-xs" />
        </div>
        <div>
          <McpFieldLabel optional optionalLabel={t("common.optional")}>
            {t("mcp.homepage")}
          </McpFieldLabel>
          <Input
            value={homepage}
            onChange={(e) => setHomepage(e.target.value)}
            className="h-8 text-xs font-mono"
            placeholder="https://"
          />
        </div>
      </div>

      <div className="space-y-2">
        <McpFieldLabel hint={t("mcp.fieldEnabledToolsHint")}>{t("mcp.fieldEnabledTools")}</McpFieldLabel>
        <McpToolTargetPicker
          targets={targets}
          enabled={enabled}
          onToggle={(toolId, next) => setEnabled((prev) => ({ ...prev, [toolId]: next }))}
          noteFor={noteForTool}
        />
      </div>

      <McpServerAdvancedFields
        enabledToolIds={enabledToolIds}
        autoApproveAll={autoApproveAll}
        onAutoApproveAllChange={setAutoApproveAll}
        autoApproveText={autoApproveText}
        onAutoApproveTextChange={setAutoApproveText}
        disabledToolsText={disabledToolsText}
        onDisabledToolsTextChange={setDisabledToolsText}
        timeoutText={timeoutText}
        onTimeoutTextChange={setTimeoutText}
      />

      {error ? <p className="text-caption text-destructive">{error}</p> : null}

      <div className="sticky bottom-0 z-10 -mx-5 -mb-4 mt-1 flex items-center justify-between gap-3 border-t border-border/60 bg-card/90 px-5 py-2.5 backdrop-blur-md shadow-[0_-4px_16px_rgba(0,0,0,0.04)]">
        {onDelete ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-8 text-xs text-destructive hover:bg-destructive/10 hover:text-destructive"
            onClick={() => void onDelete()}
          >
            {t("common.delete")}
          </Button>
        ) : (
          <span />
        )}
        <Button
          size="sm"
          onClick={() => void handleSubmit()}
          disabled={submitting}
          className="h-8 min-w-20 text-xs font-medium shadow-2xs"
        >
          {submitting ? t("common.saving") : (submitLabel ?? (initial ? t("common.save") : t("common.add")))}
        </Button>
      </div>
    </div>
  );
}

import { useState } from "react";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { cn } from "../../../lib/utils";
import { MCP_TOOL_IDS, type McpServerEntry, type McpToolId } from "../../../types";

const TOOL_LABELS: Record<McpToolId, string> = {
  "claude-code": "Claude Code",
  "claude-desktop": "Claude Desktop",
  codex: "Codex",
  gemini: "Gemini CLI",
  opencode: "OpenCode",
  zcode: "ZCode",
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
}

interface McpServerFormProps {
  /** Existing server when editing; undefined when creating. */
  initial?: McpServerEntry;
  /** Seed values for the create case (e.g. from a recommended preset). */
  defaults?: Partial<McpServerFormValue>;
  onSubmit: (value: McpServerFormValue) => Promise<void> | void;
  onDelete?: () => Promise<void> | void;
  submitting?: boolean;
  /** Override the submit button label (defaults to 保存/添加). */
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
  const [error, setError] = useState<string | null>(null);

  const isRemote = transport === "http" || transport === "sse";

  const handleSubmit = async () => {
    setError(null);
    if (!name.trim()) {
      setError("请填写服务器名称");
      return;
    }
    if (isRemote && !url.trim()) {
      setError("HTTP / SSE 传输需要填写 URL");
      return;
    }
    if (!isRemote && !command.trim()) {
      setError("stdio 传输需要填写启动命令");
      return;
    }
    const value: McpServerFormValue = {
      name: name.trim(),
      transport,
      description: description.trim() || undefined,
      homepage: homepage.trim() || undefined,
      enabled,
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
        <FieldLabel hint="写入各工具配置的 key">名称</FieldLabel>
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="例如 context7" className="h-9" />
      </div>

      <div>
        <FieldLabel>传输方式</FieldLabel>
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
            <FieldLabel hint="每行 KEY=VALUE">请求头</FieldLabel>
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
            <FieldLabel>启动命令</FieldLabel>
            <Input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="npx"
              className="h-9 font-mono"
            />
          </div>
          <div>
            <FieldLabel hint="每行一个">参数</FieldLabel>
            <textarea
              value={argsText}
              onChange={(e) => setArgsText(e.target.value)}
              rows={3}
              placeholder={"-y\n@upstash/context7-mcp"}
              className={textareaCls}
            />
          </div>
          <div>
            <FieldLabel hint="每行 KEY=VALUE">环境变量</FieldLabel>
            <textarea
              value={envText}
              onChange={(e) => setEnvText(e.target.value)}
              rows={2}
              placeholder={"API_KEY=sk-xxx"}
              className={textareaCls}
            />
          </div>
          <div>
            <FieldLabel hint="可选">工作目录</FieldLabel>
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
          <FieldLabel hint="可选">描述</FieldLabel>
          <Input value={description} onChange={(e) => setDescription(e.target.value)} className="h-9" />
        </div>
        <div>
          <FieldLabel hint="可选">主页</FieldLabel>
          <Input
            value={homepage}
            onChange={(e) => setHomepage(e.target.value)}
            className="h-9"
            placeholder="https://"
          />
        </div>
      </div>

      <div>
        <FieldLabel hint="勾选即写入对应工具的配置文件">启用工具</FieldLabel>
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
            删除
          </Button>
        ) : (
          <span />
        )}
        <Button onClick={() => void handleSubmit()} disabled={submitting}>
          {submitting ? "保存中…" : (submitLabel ?? (initial ? "保存" : "添加"))}
        </Button>
      </div>
    </div>
  );
}

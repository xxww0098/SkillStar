import { ms } from "./mcpStrings";
import type { McpServerRecord } from "./types";

/**
 * Split CLI argument text using shell-like quoting rules (`'...'`, `"..."`, and `\` escapes),
 * preserving spaces inside paths/arguments instead of splitting blindly on whitespace.
 */
export function parseShellArgs(raw: string): { args: string[]; error?: string } {
  const args: string[] = [];
  let cur = "";
  let inSingle = false;
  let inDouble = false;
  let escaped = false;
  let hasToken = false;

  for (let i = 0; i < raw.length; i += 1) {
    const ch = raw[i];
    if (escaped) {
      cur += ch;
      escaped = false;
      hasToken = true;
      continue;
    }
    if (inSingle) {
      if (ch === "'") {
        inSingle = false;
      } else {
        cur += ch;
      }
      hasToken = true;
      continue;
    }
    if (inDouble) {
      if (ch === "\\") {
        const next = raw[i + 1];
        if (next === '"' || next === "\\" || next === "$" || next === "`") {
          escaped = true;
        } else {
          cur += ch;
        }
      } else if (ch === '"') {
        inDouble = false;
      } else {
        cur += ch;
      }
      hasToken = true;
      continue;
    }

    if (ch === "\\") {
      escaped = true;
      hasToken = true;
    } else if (ch === "'") {
      inSingle = true;
      hasToken = true;
    } else if (ch === '"') {
      inDouble = true;
      hasToken = true;
    } else if (/\s/.test(ch)) {
      if (hasToken) {
        args.push(cur);
        cur = "";
        hasToken = false;
      }
    } else {
      cur += ch;
      hasToken = true;
    }
  }

  if (escaped) {
    cur += "\\";
    hasToken = true;
  }
  if (inSingle || inDouble) {
    return { args: [], error: ms().validationArgsUnterminatedQuote };
  }
  if (hasToken) {
    args.push(cur);
  }
  return { args };
}

/**
 * Format arguments back to editable text so paths/values with spaces remain quoted losslessly.
 */
export function formatShellArgs(args: readonly string[]): string {
  return args
    .map((arg) => {
      if (arg.length === 0 || /[\s"'\\]/.test(arg)) {
        return `"${arg.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
      }
      return arg;
    })
    .join(" ");
}

/** Parse multiline `KEY=VALUE` environment variables, failing loudly on malformed lines. */
export function parseEnvLines(raw: string): { env: Record<string, string>; error?: string } {
  const env: Record<string, string> = {};
  for (const rawLine of raw.split(/\r?\n/)) {
    const trimmed = rawLine.trim();
    if (trimmed.length === 0) continue;
    const eqIdx = rawLine.indexOf("=");
    if (eqIdx <= 0) {
      return { env: {}, error: `${ms().validationEnvFormat}${trimmed}` };
    }
    const key = rawLine.slice(0, eqIdx).trim();
    if (key.length === 0 || /\s/.test(key)) {
      return { env: {}, error: `${ms().validationEnvFormat}${trimmed}` };
    }
    env[key] = rawLine.slice(eqIdx + 1).trim();
  }
  return { env };
}

/** Parse multiline `Key: Value` HTTP headers, failing loudly on malformed lines. */
export function parseHeaderLines(raw: string): { headers: Record<string, string>; error?: string } {
  const headers: Record<string, string> = {};
  for (const rawLine of raw.split(/\r?\n/)) {
    const trimmed = rawLine.trim();
    if (trimmed.length === 0) continue;
    const colonIdx = rawLine.indexOf(":");
    if (colonIdx <= 0) {
      return { headers: {}, error: `${ms().validationHeaderFormat}${trimmed}` };
    }
    const key = rawLine.slice(0, colonIdx).trim();
    if (key.length === 0 || /\s/.test(key)) {
      return { headers: {}, error: `${ms().validationHeaderFormat}${trimmed}` };
    }
    headers[key] = rawLine.slice(colonIdx + 1).trim();
  }
  return { headers };
}

/** Validate optional positive startup timeout seconds. */
export function parseTimeoutSec(raw: string): { timeout?: number; error?: string } {
  const trimmed = raw.trim();
  if (trimmed.length === 0) return { timeout: undefined };
  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return { error: ms().validationTimeoutInvalid };
  }
  return { timeout: parsed };
}

/** Validate required draft fields before saving to backend. */
export function validateServerDraft(draft: McpServerRecord): string | null {
  if (draft.name.trim().length === 0) {
    return ms().validationNameRequired;
  }
  if (draft.transport === "stdio") {
    if (draft.command.trim().length === 0) {
      return ms().validationCommandRequired;
    }
    return null;
  }
  const url = draft.url.trim();
  if (url.length === 0) {
    return ms().validationUrlRequired;
  }
  if (!/^https?:\/\/\S+$/i.test(url)) {
    return ms().validationUrlInvalid;
  }
  return null;
}

function sameStringRecord(a: Record<string, string>, b: Record<string, string>): boolean {
  const aKeys = Object.keys(a).sort();
  const bKeys = Object.keys(b).sort();
  if (aKeys.length !== bKeys.length) return false;
  return aKeys.every((k, i) => k === bKeys[i] && a[k] === b[k]);
}

/** Compare functional connection/execution configuration of two MCP server definitions. */
export function sameServerConfig(a: McpServerRecord, b: McpServerRecord): boolean {
  return (
    a.transport === b.transport &&
    a.command === b.command &&
    a.args.length === b.args.length &&
    a.args.every((v, i) => v === b.args[i]) &&
    sameStringRecord(a.env, b.env) &&
    a.url === b.url &&
    sameStringRecord(a.headers, b.headers) &&
    (a.startupTimeoutSec ?? undefined) === (b.startupTimeoutSec ?? undefined)
  );
}

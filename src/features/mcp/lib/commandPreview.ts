/**
 * The pre-install command confirmation (research §7 P1-6, a spec MUST).
 *
 * Deeplink-style one-click installs are a demonstrated attack surface
 * (CursorJack), and the only effective mitigation is showing the user the
 * complete, untruncated, already-resolved command *before* it runs. The backend
 * ships `McpInstallPlan.commandPreview` for exactly that; this module exists
 * because the install form may still add argument values the plan could not
 * know about, and a confirmation that shows a stale command confirms nothing.
 *
 * `renderMcpCommand` is a faithful port of `render_command` in
 * `crates/skillstar-app/src/mcp/install.rs`, quoting rule included, so that an
 * untouched plan re-renders byte-for-byte identically to the string the backend
 * produced (pinned by this module's tests). The output is for reading only —
 * nothing re-parses or executes it, and `usesShell` is `false` because the
 * launcher is exec'd directly.
 */

/**
 * Rust's `char::is_whitespace` is the Unicode `White_Space` property, which is
 * not quite JavaScript's `\s` (Rust has U+0085, JS has U+FEFF). Spelled out so
 * the port cannot drift on an exotic argument.
 */
const UNICODE_WHITESPACE = /[\t\n\v\f\r \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]/;

function needsQuoting(arg: string): boolean {
  return arg === "" || UNICODE_WHITESPACE.test(arg) || arg.includes("'") || arg.includes('"');
}

/** Single-quote an argument the POSIX way, so the boundaries are visible. */
function quoteArg(arg: string): string {
  return `'${arg.replaceAll("'", "'\\''")}'`;
}

/**
 * Render the command line exactly as it will run.
 *
 * Arguments containing whitespace or quotes are shown single-quoted so the user
 * can see where each one starts and ends. Never truncated: the whole point is
 * that a malicious tail cannot hide past an ellipsis.
 */
export function renderMcpCommand(command: string, args: readonly string[] = []): string {
  let out = command;
  for (const arg of args) {
    out += ` ${needsQuoting(arg) ? quoteArg(arg) : arg}`;
  }
  return out;
}

/** Placeholder shown in place of a secret value. */
export const SECRET_MASK = "••••••••";

/**
 * Replace known secret values before display.
 *
 * Secrets normally reach a server through `env` or a header, not the command
 * line — but an argument-scoped input the publisher marked `isSecret` would
 * land in `args`, and the confirmation string is the one place a preview could
 * leak it back onto the screen (and into anything the user copies out of it).
 * Only non-empty values are masked, longest first, so a short secret that is a
 * substring of a longer one cannot leave a partial value behind.
 */
export function maskSecrets(text: string, secretValues: readonly string[]): string {
  const values = [...new Set(secretValues.filter((value) => value.length > 0))].sort((a, b) => b.length - a.length);
  let out = text;
  for (const value of values) {
    out = out.split(value).join(SECRET_MASK);
  }
  return out;
}

export interface McpCommandConfirmation {
  /** The full command line, secrets masked. Empty for remote servers. */
  preview: string;
  /** Absolute path the launcher resolves to on this machine, when known. */
  resolvedPath: string | null;
  /** True when the plan's own preview no longer describes what will run. */
  editedSincePlan: boolean;
  /** Always false — the launcher is exec'd directly, never through a shell. */
  usesShell: boolean;
}

export interface McpCommandConfirmationInput {
  command: string | null | undefined;
  args: readonly string[];
  resolvedCommandPath: string | null | undefined;
  /** `McpInstallPlan.commandPreview`, for the "did anything change" check. */
  planPreview: string | null | undefined;
  secretValues?: readonly string[];
  usesShell?: boolean;
}

/**
 * Build the confirmation payload the install wizard blocks on.
 *
 * `editedSincePlan` is surfaced rather than hidden: when the user's own answers
 * changed the command line, the string they are approving is one this app
 * assembled, and that deserves to be said out loud next to it.
 */
export function buildCommandConfirmation({
  command,
  args,
  resolvedCommandPath,
  planPreview,
  secretValues = [],
  usesShell = false,
}: McpCommandConfirmationInput): McpCommandConfirmation {
  const trimmed = command?.trim() ?? "";
  if (!trimmed) {
    return { preview: "", resolvedPath: null, editedSincePlan: false, usesShell };
  }
  const rendered = renderMcpCommand(trimmed, args);
  return {
    preview: maskSecrets(rendered, secretValues),
    resolvedPath: resolvedCommandPath?.trim() || null,
    editedSincePlan: planPreview != null && planPreview !== rendered,
    usesShell,
  };
}

export interface McpEnvPreviewRow {
  key: string;
  value: string;
  secret: boolean;
}

/**
 * The environment (or headers) the entry will carry, with secret values masked.
 * Shown next to the command so "what does this server receive" is one view, not
 * two.
 */
export function buildEnvPreview(
  values: Readonly<Record<string, string>> | null | undefined,
  secretKeys: readonly string[],
): McpEnvPreviewRow[] {
  const secrets = new Set(secretKeys);
  return Object.entries(values ?? {})
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([key, value]) => ({
      key,
      value: secrets.has(key) && value.length > 0 ? SECRET_MASK : value,
      secret: secrets.has(key),
    }));
}

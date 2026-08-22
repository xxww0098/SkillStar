/**
 * The pre-install command confirmation (research §7 P1-6, a spec MUST).
 *
 * Deeplink-style one-click installs are a demonstrated attack surface
 * (CursorJack), and the only effective mitigation is showing the user the
 * complete, untruncated, already-resolved command *before* it runs.
 *
 * The command line itself is **rendered by the backend**
 * (`skillstar_app::mcp::install::preview_install`) from the same entry it would
 * write, so the string the user approves and the object that gets installed are
 * one derivation. This module only prepares that string for display: masking
 * the secrets the renderer must not echo back onto the screen, and saying
 * whether the user's own answers changed the command since the plan was built.
 */

/** Placeholder shown in place of a secret value. */
export const SECRET_MASK = "••••••••";

/**
 * Replace known secret values before display.
 *
 * Secrets normally reach a server through `env` or a header, not the command
 * line — but an argument-scoped input the publisher marked `isSecret` would
 * land in `args`, and the confirmation string is the one place a preview could
 * leak it back onto the screen (and into anything the user copies out of it).
 * The backend renders the real command; masking happens here, at the edge that
 * displays it.
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
  /** `McpInstallPreview.commandPreview` — the backend's rendering of the final
   * command. Null or empty for a remote server, which runs nothing locally. */
  preview: string | null | undefined;
  resolvedCommandPath: string | null | undefined;
  /** `McpInstallPlan.commandPreview`, for the "did anything change" check. */
  planPreview: string | null | undefined;
  secretValues?: readonly string[];
  usesShell?: boolean;
}

/**
 * Build the confirmation payload the install wizard blocks on.
 *
 * `editedSincePlan` compares the backend's two renderings of the same
 * derivation — the plan's, before any answer, against the preview's, after
 * them — so it is true exactly when the user's own input changed what will
 * run, and that deserves to be said out loud next to it.
 */
export function buildCommandConfirmation({
  preview,
  resolvedCommandPath,
  planPreview,
  secretValues = [],
  usesShell = false,
}: McpCommandConfirmationInput): McpCommandConfirmation {
  const rendered = preview?.trim() ?? "";
  if (!rendered) {
    return { preview: "", resolvedPath: null, editedSincePlan: false, usesShell };
  }
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

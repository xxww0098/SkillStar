import type { AuthMode } from "../types";

/**
 * Auth modes the subscription dialog can actually drive.
 *
 * This is a *capability* statement about the form, not a mirror of the backend
 * enum, which is why it lives beside the other behavioural helpers instead of
 * in `types.ts` — that file is now a pure re-export barrel over
 * `src/types/generated/`, and generated files cannot hold a function.
 *
 * `cookie` is included: `catalog.rs` marks `stepfun` and `opencode` as
 * `[Cookie, Manual]`, and filtering cookie out here is what made those two
 * catalogs impossible to bind — the dialog fell back to `"o-auth"` and pointed
 * a cookie-only provider at `start_oauth_login`.
 *
 * `manual` stays filtered until a manual-quota form exists; a catalog that
 * offers only `manual` therefore yields an empty list, and the caller's
 * `?? "o-auth"` fallback keeps that visible as a broken flow rather than
 * silently pretending a form is there.
 */
export function selectableAuthModes(modes: AuthMode[]): AuthMode[] {
  return modes.filter((mode) => mode === "o-auth" || mode === "api-key" || mode === "cookie");
}

/** Whether this mode's credential is a pasted browser `Cookie:` header. */
export function isCookieMode(mode: AuthMode): boolean {
  return mode === "cookie";
}

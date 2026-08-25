const HIDE_ACCOUNT_EMAILS_KEY = "skillstar:usage-hide-account-emails";

export function readHideAccountEmails(): boolean {
  try {
    return localStorage.getItem(HIDE_ACCOUNT_EMAILS_KEY) === "true";
  } catch {
    return false;
  }
}

export function writeHideAccountEmails(hidden: boolean): void {
  try {
    localStorage.setItem(HIDE_ACCOUNT_EMAILS_KEY, String(hidden));
  } catch {
    // Privacy preference is best-effort in restricted webviews.
  }
}

/** Replace only account-like labels; custom non-email names remain useful. */
export function displayAccountIdentity(identity: string, hideEmail: boolean): string {
  return hideEmail && identity.includes("@") ? "••••••••" : identity;
}

/**
 * Default deck name for GitHub "Quick Pack": the repository name after the
 * last slash in a scan source such as `owner/repo`.
 *
 * `ScanResult.source` is already the short form (`owner/repo`), but the
 * helper also accepts a clone URL so callers do not have to normalize first.
 */
export function deckNameFromRepoSource(source: string): string {
  const trimmed = source.trim().replace(/\/+$/, "");
  if (!trimmed) return "";
  const withoutGit = trimmed.replace(/\.git$/i, "");
  return withoutGit.split("/").filter(Boolean).pop() ?? "";
}

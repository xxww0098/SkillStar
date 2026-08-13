export interface DeckInstallFailure {
  name: string;
  error: string;
}

export function installErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message.trim();
  if (typeof error === "string") return error.trim();
  if (error == null) return "";
  return String(error).trim();
}

export function deckInstallFailureDetails(failures: readonly DeckInstallFailure[]): {
  names: string;
  reason: string;
} {
  return {
    names: failures.map(({ name }) => name).join(", "),
    reason: failures.find(({ error }) => error.length > 0)?.error ?? "",
  };
}

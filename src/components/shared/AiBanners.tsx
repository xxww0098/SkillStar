import { X } from "lucide-react";

// ── AI Error Banner ─────────────────────────────────────────────────
// Dismissible inline error strip shown when an AI operation fails.

interface AiErrorBannerProps {
  /** Localized error message (pass null/undefined to hide). */
  error: string | null | undefined;
  /** Called when the user clicks the dismiss button. */
  onDismiss: () => void;
}

export function AiErrorBanner({ error, onDismiss }: AiErrorBannerProps) {
  if (!error) return null;

  return (
    <div className="px-4 py-2 bg-destructive/10 border-b border-destructive/20 flex items-center gap-2">
      <span className="text-xs text-destructive flex-1">{error}</span>
      <button
        type="button"
        onClick={onDismiss}
        className="text-destructive/60 hover:text-destructive cursor-pointer p-1.5 rounded focus-ring"
      >
        <X className="w-3 h-3" />
      </button>
    </div>
  );
}

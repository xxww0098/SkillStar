import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { navigateToAiSettings } from "../../lib/utils";

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
        onClick={onDismiss}
        className="text-destructive/60 hover:text-destructive cursor-pointer p-1.5 rounded focus-ring"
      >
        <X className="w-3 h-3" />
      </button>
    </div>
  );
}

// ── AI Not Configured Banner ────────────────────────────────────────
// Shown when AI is not configured to prompt the user to set it up.

interface AiNotConfiguredBannerProps {
  /** When true the banner is rendered. */
  show: boolean;
}

export function AiNotConfiguredBanner({ show }: AiNotConfiguredBannerProps) {
  const { t } = useTranslation();

  if (!show) return null;

  return (
    <div className="px-4 py-2 border-b border-border bg-muted/30 flex items-center gap-2">
      <span className="text-micro text-muted-foreground flex-1">{t("skillEditor.aiNotConfigured")}</span>
      <button
        onClick={navigateToAiSettings}
        className="px-2 py-1 rounded-md text-micro font-medium border border-border hover:bg-card-hover transition-colors cursor-pointer"
      >
        {t("skillEditor.configureAI")}
      </button>
    </div>
  );
}

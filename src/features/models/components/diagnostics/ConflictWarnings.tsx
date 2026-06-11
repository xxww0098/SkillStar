import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, FileWarning, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AlertDialog } from "radix-ui";
import { Button } from "../../../../components/ui/button";
import { tauriInvoke } from "../../../../lib/ipc";
import { cn } from "../../../../lib/utils";

export interface ConflictWarningsProps {
  providerId: string;
  /** Only show conflicts for this tool (agent settings dialog scope). */
  toolId?: string;
}

/** Conflict types matching the Rust backend `ConflictType` enum. */
type ConflictType = "EnvVarOverride" | "LegacyConfig" | "ExternalModification";

interface ConfigConflict {
  conflict_type: ConflictType;
  description: string;
  file_path?: string | null;
  details?: string | null;
  tool_id?: string | null;
}

/** Actions available for external modification conflicts. */
type ConflictAction = "overwrite" | "cancel" | "diff";

interface ExternalModificationDialogProps {
  open: boolean;
  conflict: ConfigConflict | null;
  onAction: (action: ConflictAction) => void;
}

/**
 * Dialog shown when an external modification conflict is detected.
 * Offers three options: overwrite, cancel, or view diff.
 */
function ExternalModificationDialog({ open, conflict, onAction }: ExternalModificationDialogProps) {
  const { t } = useTranslation();
  return (
    <AlertDialog.Root open={open} onOpenChange={(isOpen) => !isOpen && onAction("cancel")}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0" />
        <AlertDialog.Content className="fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-border/60 bg-card p-6 shadow-xl">
          <div className="flex items-start gap-3 mb-4">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-amber-500/10 text-amber-500">
              <FileWarning className="h-5 w-5" />
            </div>
            <div className="space-y-1">
              <AlertDialog.Title className="text-sm font-semibold text-foreground">
                {t("models.conflicts.dialogTitle")}
              </AlertDialog.Title>
              <AlertDialog.Description className="text-xs text-muted-foreground leading-relaxed">
                {conflict?.description || t("models.conflicts.externalModified")}
              </AlertDialog.Description>
              {conflict?.file_path && (
                <p className="text-[11px] text-muted-foreground/70 font-mono mt-1 truncate">{conflict.file_path}</p>
              )}
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 pt-2 border-t border-border/40">
            <AlertDialog.Cancel asChild>
              <Button variant="ghost" size="sm" onClick={() => onAction("cancel")}>
                {t("models.conflicts.cancel")}
              </Button>
            </AlertDialog.Cancel>
            <Button variant="outline" size="sm" onClick={() => onAction("diff")}>
              {t("models.conflicts.openFolder")}
            </Button>
            <AlertDialog.Action asChild>
              <Button size="sm" onClick={() => onAction("overwrite")}>
                {t("models.conflicts.overwrite")}
              </Button>
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

/**
 * Warning banner for environment variable overrides and legacy config conflicts.
 * Displays amber/yellow banners with alert icons.
 */
function WarningBanner({ conflict, onDismiss }: { conflict: ConfigConflict; onDismiss?: () => void }) {
  const { t } = useTranslation();
  const message =
    conflict.conflict_type === "LegacyConfig"
      ? t("models.conflicts.legacyWarning")
      : t("models.conflicts.envVarWarning", { name: extractVarName(conflict.details) });

  return (
    <div
      className={cn(
        "flex items-start gap-2.5 rounded-lg border px-3 py-2.5",
        "border-amber-500/30 bg-amber-500/5 text-amber-700 dark:text-amber-400",
      )}
      role="alert"
    >
      <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5 text-amber-500" />
      <p className="flex-1 text-xs leading-relaxed">{message}</p>
      {onDismiss && (
        <button
          type="button"
          onClick={onDismiss}
          className="shrink-0 rounded p-0.5 text-amber-500/70 hover:text-amber-500 transition-colors cursor-pointer"
          aria-label={t("models.conflicts.dismiss")}
        >
          <X className="h-3.5 w-3.5" />
        </button>
      )}
    </div>
  );
}

/** Extract the environment variable name from the details string (e.g., "ANTHROPIC_API_KEY=sk-a***"). */
function extractVarName(details?: string | null): string {
  if (!details) return "UNKNOWN";
  const eqIdx = details.indexOf("=");
  return eqIdx > 0 ? details.substring(0, eqIdx) : details;
}

/**
 * ConflictWarnings component — detects and displays configuration conflicts
 * for a given provider.
 *
 * - EnvVarOverride: Shows warning banners for each detected env var override
 * - LegacyConfig: Shows warning banner for legacy ~/.claude.json conflicts
 * - ExternalModification: Shows a dialog with overwrite / cancel / view-diff options
 *
 * Auto-detects conflicts on mount and when providerId changes.
 */
export function ConflictWarnings({ providerId, toolId }: ConflictWarningsProps) {
  const [conflicts, setConflicts] = useState<ConfigConflict[]>([]);
  const [dismissedIds, setDismissedIds] = useState<Set<string>>(new Set());
  const [externalConflict, setExternalConflict] = useState<ConfigConflict | null>(null);

  // Detect conflicts on mount and when providerId changes
  useEffect(() => {
    let cancelled = false;

    async function detect() {
      try {
        const all = await tauriInvoke("detect_provider_conflicts", { providerId });
        const result = toolId ? all.filter((c) => !c.tool_id || c.tool_id === toolId) : all;
        if (!cancelled) {
          setConflicts(result);
          setDismissedIds(new Set());

          // If there's an ExternalModification conflict, show the dialog
          const extConflict = result.find((c) => c.conflict_type === "ExternalModification");
          if (extConflict) {
            setExternalConflict(extConflict);
          }
        }
      } catch {
        // Silently ignore detection failures — non-critical
      }
    }

    void detect();
    return () => {
      cancelled = true;
    };
  }, [providerId, toolId]);

  const handleDismiss = useCallback((conflict: ConfigConflict) => {
    const key = `${conflict.conflict_type}:${conflict.details || conflict.file_path || ""}`;
    setDismissedIds((prev) => new Set(prev).add(key));
  }, []);

  const handleExternalAction = useCallback(
    async (action: ConflictAction) => {
      const conflict = externalConflict;
      setExternalConflict(null);
      if (!conflict) return;

      if (action === "overwrite" && conflict.tool_id) {
        // Re-sync our config to disk, overwriting the external edits.
        try {
          await tauriInvoke("resync_tool", { toolId: conflict.tool_id });
        } catch (err) {
          if (import.meta.env.DEV) console.error("resync_tool failed:", err);
        }
      } else if (action === "diff" && conflict.file_path) {
        // Open the folder containing the externally-modified config file so the
        // user can inspect it. (A full in-app diff view is not yet implemented.)
        const dir = conflict.file_path.replace(/[/\\][^/\\]*$/, "");
        try {
          await tauriInvoke("open_folder", { path: dir || conflict.file_path });
        } catch (err) {
          if (import.meta.env.DEV) console.error("open_folder failed:", err);
        }
      }
      // "cancel" just closes the dialog (already handled above)
    },
    [externalConflict],
  );

  // Filter to only banner-type conflicts (not ExternalModification)
  const bannerConflicts = conflicts.filter((c) => {
    if (c.conflict_type === "ExternalModification") return false;
    const key = `${c.conflict_type}:${c.details || c.file_path || ""}`;
    return !dismissedIds.has(key);
  });

  if (bannerConflicts.length === 0 && !externalConflict) {
    return null;
  }

  return (
    <>
      {bannerConflicts.length > 0 && (
        <div className="space-y-2" data-testid="conflict-warnings">
          {bannerConflicts.map((conflict) => {
            const key = `${conflict.conflict_type}:${conflict.details || conflict.file_path || ""}`;
            return <WarningBanner key={key} conflict={conflict} onDismiss={() => handleDismiss(conflict)} />;
          })}
        </div>
      )}

      <ExternalModificationDialog
        open={!!externalConflict}
        conflict={externalConflict}
        onAction={handleExternalAction}
      />
    </>
  );
}

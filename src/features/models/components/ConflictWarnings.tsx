import { useCallback, useEffect, useState } from "react";
import { AlertTriangle, FileWarning, X } from "lucide-react";
import { AlertDialog } from "radix-ui";
import { Button } from "../../../components/ui/button";
import { tauriInvoke } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";

export interface ConflictWarningsProps {
  providerId: string;
}

/** Conflict types matching the Rust backend `ConflictType` enum. */
type ConflictType = "EnvVarOverride" | "LegacyConfig" | "ExternalModification";

interface ConfigConflict {
  conflict_type: ConflictType;
  description: string;
  file_path?: string | null;
  details?: string | null;
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
              <AlertDialog.Title className="text-sm font-semibold text-foreground">配置文件冲突</AlertDialog.Title>
              <AlertDialog.Description className="text-xs text-muted-foreground leading-relaxed">
                {conflict?.description || "配置文件在上次同步后被外部修改。"}
              </AlertDialog.Description>
              {conflict?.file_path && (
                <p className="text-[11px] text-muted-foreground/70 font-mono mt-1 truncate">{conflict.file_path}</p>
              )}
            </div>
          </div>

          <div className="flex items-center justify-end gap-2 pt-2 border-t border-border/40">
            <AlertDialog.Cancel asChild>
              <Button variant="ghost" size="sm" onClick={() => onAction("cancel")}>
                取消
              </Button>
            </AlertDialog.Cancel>
            <Button variant="outline" size="sm" onClick={() => onAction("diff")}>
              查看差异
            </Button>
            <AlertDialog.Action asChild>
              <Button size="sm" onClick={() => onAction("overwrite")}>
                覆盖
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
  const message =
    conflict.conflict_type === "LegacyConfig"
      ? "⚠️ 检测到旧版 ~/.claude.json 配置文件可能产生冲突"
      : `⚠️ 检测到环境变量 ${extractVarName(conflict.details)} 可能覆盖配置文件设置`;

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
          aria-label="关闭警告"
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
 * - ExternalModification: Shows a dialog with "覆盖" / "取消" / "查看差异" options
 *
 * Auto-detects conflicts on mount and when providerId changes.
 */
export function ConflictWarnings({ providerId }: ConflictWarningsProps) {
  const [conflicts, setConflicts] = useState<ConfigConflict[]>([]);
  const [dismissedIds, setDismissedIds] = useState<Set<string>>(new Set());
  const [externalConflict, setExternalConflict] = useState<ConfigConflict | null>(null);

  // Detect conflicts on mount and when providerId changes
  useEffect(() => {
    let cancelled = false;

    async function detect() {
      try {
        const result = await tauriInvoke("detect_env_conflicts");
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
  }, [providerId]);

  const handleDismiss = useCallback((conflict: ConfigConflict) => {
    const key = `${conflict.conflict_type}:${conflict.details || conflict.file_path || ""}`;
    setDismissedIds((prev) => new Set(prev).add(key));
  }, []);

  const handleExternalAction = useCallback((action: ConflictAction) => {
    setExternalConflict(null);

    // The parent component or hook can handle the actual action.
    // For now, we just close the dialog. In a full implementation,
    // "overwrite" would proceed with the write, "diff" would open a diff view.
    if (action === "overwrite") {
      // TODO: Emit event or call callback to proceed with overwrite
    } else if (action === "diff") {
      // TODO: Open diff view for the conflicting file
    }
    // "cancel" just closes the dialog (already handled above)
  }, []);

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

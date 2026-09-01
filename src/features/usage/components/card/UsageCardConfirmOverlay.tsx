import { useEffect, useId, useRef } from "react";
import { Button } from "@/components/ui/button";

export interface UsageCardConfirmOverlayProps {
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  confirmVariant?: "destructive" | "default";
  confirmDisabled?: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

/**
 * In-card confirm sheet. Stays inside the card so the decision stays spatially
 * attached to the account; keyboard gets Escape, a real alertdialog name, and
 * initial focus on Cancel (the non-destructive action).
 */
export function UsageCardConfirmOverlay({
  title,
  message,
  confirmLabel,
  cancelLabel,
  confirmVariant = "destructive",
  confirmDisabled = false,
  onCancel,
  onConfirm,
}: UsageCardConfirmOverlayProps) {
  const titleId = useId();
  const descId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const tone = confirmVariant === "destructive" ? "border-red-200" : "border-amber-200";

  useEffect(() => {
    panelRef.current?.querySelector<HTMLButtonElement>("[data-usage-confirm-cancel]")?.focus();
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        event.preventDefault();
        onCancel();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div
      className="absolute inset-0 z-20 flex items-center justify-center rounded-3xl bg-white/90 backdrop-blur-sm"
      onClick={onCancel}
    >
      <div
        ref={panelRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descId}
        className={`mx-4 rounded-2xl border bg-white p-5 shadow-xl ${tone}`}
        onClick={(event) => event.stopPropagation()}
      >
        <p id={titleId} className="mb-1 text-sm font-semibold text-zinc-900">
          {title}
        </p>
        <p id={descId} className="mb-4 text-xs leading-relaxed text-zinc-600">
          {message}
        </p>
        <div className="flex justify-end gap-2">
          <Button size="sm" variant="ghost" onClick={onCancel} data-usage-confirm-cancel>
            {cancelLabel}
          </Button>
          <Button size="sm" variant={confirmVariant} disabled={confirmDisabled} onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}

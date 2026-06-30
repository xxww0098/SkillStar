import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { ModalCloseButton, ModalShell } from "../../../components/ui/ModalShell";

interface UnsavedChangesDialogProps {
  open: boolean;
  /** Discard unsaved changes and continue with the pending navigation. */
  onDiscard: () => void;
  /** Apply the pending changes, then continue. */
  onApply: () => void;
  /** Cancel the navigation and stay on the current project. */
  onCancel: () => void;
}

export function UnsavedChangesDialog({ open, onDiscard, onApply, onCancel }: UnsavedChangesDialogProps) {
  const { t } = useTranslation();

  return (
    <ModalShell
      open={open}
      onClose={onCancel}
      ariaLabel={t("projects.unsavedTitle")}
      role="alertdialog"
      panelClassName="max-w-md px-4"
    >
      <div className="flex items-start justify-between gap-4 px-6 pt-5">
        <div className="flex items-start gap-3">
          <div className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-amber-500/10 text-amber-500">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div className="space-y-1">
            <h2 className="text-heading-sm">{t("projects.unsavedTitle")}</h2>
            <p className="text-caption leading-5">{t("projects.unsavedDescription")}</p>
          </div>
        </div>
        <ModalCloseButton onClose={onCancel} />
      </div>

      <div className="flex items-center justify-end gap-2 border-t border-border/60 px-6 py-3.5">
        <Button variant="ghost" size="sm" onClick={onCancel}>
          {t("projects.unsavedCancel")}
        </Button>
        <Button variant="outline" size="sm" onClick={onDiscard}>
          {t("projects.unsavedDiscard")}
        </Button>
        <Button size="sm" onClick={onApply}>
          {t("projects.unsavedApply")}
        </Button>
      </div>
    </ModalShell>
  );
}

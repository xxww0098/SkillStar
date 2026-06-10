import { motion, useReducedMotion } from "framer-motion";
import { AlertTriangle, Loader2, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { ModalCloseButton, ModalShell } from "../../../components/ui/ModalShell";

interface UninstallConfirmDialogProps {
  open: boolean;
  skillNames: string[];
  uninstalling?: boolean;
  error?: string | null;
  onClose: () => void;
  onConfirm: () => void;
}

export function UninstallConfirmDialog({
  open,
  skillNames,
  uninstalling,
  error,
  onClose,
  onConfirm,
}: UninstallConfirmDialogProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const visibleNames = skillNames.slice(0, 6);
  const extraCount = Math.max(skillNames.length - visibleNames.length, 0);
  const isBatch = skillNames.length > 1;

  return (
    <ModalShell
      open={open && skillNames.length > 0}
      onClose={onClose}
      ariaLabel={isBatch ? t("uninstallDialog.title", { count: skillNames.length }) : t("uninstallDialog.titleSingle")}
      role="alertdialog"
      panelClassName="max-w-md px-4"
      dismissable={!uninstalling}
    >
      <div className="flex items-start justify-between gap-4 px-6 pt-5">
        <div className="flex items-start gap-3">
          <motion.div
            animate={prefersReducedMotion ? undefined : { scale: [1, 1.04, 1], rotate: [0, -3, 0] }}
            transition={{ duration: 0.42, ease: [0.22, 1, 0.36, 1] }}
            className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-destructive/10 text-destructive"
          >
            <AlertTriangle className="h-5 w-5" />
          </motion.div>
          <div className="space-y-1">
            <h2 className="text-heading-sm">
              {isBatch ? t("uninstallDialog.title", { count: skillNames.length }) : t("uninstallDialog.titleSingle")}
            </h2>
            <p className="text-caption leading-5">{t("uninstallDialog.description")}</p>
          </div>
        </div>

        <ModalCloseButton onClose={onClose} disabled={uninstalling} />
      </div>

      <div className="px-6 py-4 space-y-4">
        <div className="rounded-2xl border border-border/70 bg-muted/40 px-3 py-3">
          <div className="mb-2 flex items-center gap-2 text-micro uppercase tracking-[0.18em] text-muted-foreground">
            <Trash2 className="h-3.5 w-3.5" />
            {t("uninstallDialog.removing")}
          </div>

          <div className="flex flex-wrap gap-2">
            {visibleNames.map((name, index) => (
              <motion.span
                key={name}
                initial={prefersReducedMotion ? false : { opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{
                  duration: prefersReducedMotion ? 0.01 : 0.18,
                  delay: prefersReducedMotion ? 0 : index * 0.03,
                  ease: [0.22, 1, 0.36, 1],
                }}
                className="rounded-full border border-border bg-card px-2.5 py-1 text-xs font-medium text-foreground shadow-sm"
              >
                {name}
              </motion.span>
            ))}
            {extraCount > 0 && (
              <span className="rounded-full border border-dashed border-border bg-card px-2.5 py-1 text-xs text-muted-foreground">
                {t("common.more", { count: extraCount })}
              </span>
            )}
          </div>
        </div>

        {error && (
          <motion.div
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            className="rounded-xl border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive"
          >
            {error}
          </motion.div>
        )}
      </div>

      <div className="flex items-center justify-end gap-2 border-t border-border/60 px-6 py-3.5">
        <Button variant="ghost" size="sm" onClick={onClose} disabled={uninstalling}>
          {t("uninstallDialog.cancel")}
        </Button>
        <Button variant="destructive" size="sm" onClick={onConfirm} disabled={uninstalling}>
          {uninstalling ? (
            <span className="flex items-center gap-1.5">
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
              {t("uninstallDialog.uninstalling")}
            </span>
          ) : (
            <span className="flex items-center gap-1.5">
              <Trash2 className="h-3.5 w-3.5" />
              {t("uninstallDialog.confirmUninstall")}
            </span>
          )}
        </Button>
      </div>
    </ModalShell>
  );
}

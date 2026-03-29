import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { AlertTriangle, Loader2, Trash2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/button";

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
    <AnimatePresence>
      {open && skillNames.length > 0 && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.16 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={uninstalling ? undefined : onClose}
          />

          <motion.div
            initial={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: 16 }}
            animate={prefersReducedMotion ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
            exit={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.98, y: 12 }}
            transition={{
              duration: prefersReducedMotion ? 0.01 : 0.24,
              ease: [0.22, 1, 0.36, 1],
            }}
            className="fixed left-1/2 top-1/2 w-full max-w-md -translate-x-1/2 -translate-y-1/2 z-50 px-4"
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
              <div className="relative z-10">
              <div className="flex items-start justify-between gap-4 px-6 pt-5">
                <div className="flex items-start gap-3">
                  <motion.div
                    animate={
                      prefersReducedMotion
                        ? undefined
                        : { scale: [1, 1.04, 1], rotate: [0, -3, 0] }
                    }
                    transition={{ duration: 0.42, ease: [0.22, 1, 0.36, 1] }}
                    className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-destructive/10 text-destructive"
                  >
                    <AlertTriangle className="h-5 w-5" />
                  </motion.div>
                  <div className="space-y-1">
                    <h2 className="text-heading-sm">
                      {isBatch ? t("uninstallDialog.title", { count: skillNames.length }) : t("uninstallDialog.titleSingle")}
                    </h2>
                    <p className="text-caption leading-5">
                      {t("uninstallDialog.description")}
                    </p>
                  </div>
                </div>

                <button
                  onClick={onClose}
                  disabled={uninstalling}
                  className="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-muted cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              <div className="px-6 py-4 space-y-4">
                <div className="rounded-2xl border border-border/70 bg-muted/40 px-3 py-3">
                  <div className="mb-2 flex items-center gap-2 text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
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
            </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

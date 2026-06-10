import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "../../lib/utils";

interface ModalShellProps {
  open: boolean;
  onClose: () => void;
  ariaLabel: string;
  role?: "dialog" | "alertdialog";
  /** Width/height classes for the centered panel, e.g. "max-w-lg" or "max-w-[760px] h-[580px]". */
  panelClassName?: string;
  /** Extra classes merged onto the modal surface (e.g. "flex flex-col max-h-[calc(100vh-2rem)]"). */
  surfaceClassName?: string;
  /** Classes for the content wrapper that sits above the ambient glows. */
  contentClassName?: string;
  /** "subtle" uses modal-surface-subtle and skips the ambient glows. */
  variant?: "default" | "subtle";
  /** Set false to ignore backdrop clicks (e.g. while a mutation is in flight). */
  dismissable?: boolean;
  children: ReactNode;
}

/**
 * Shared scaffold for centered glassmorphism modals: animated backdrop,
 * entrance motion, modal surface, ambient glows, and a z-raised content
 * wrapper. Callers render their own header/body/footer as children
 * (ModalHeader covers the common icon + title + close-button header).
 */
export function ModalShell({
  open,
  onClose,
  ariaLabel,
  role = "dialog",
  panelClassName = "max-w-lg",
  surfaceClassName,
  contentClassName,
  variant = "default",
  dismissable = true,
  children,
}: ModalShellProps) {
  const prefersReducedMotion = useReducedMotion();

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={dismissable ? onClose : undefined}
          />

          <motion.div
            initial={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: 12 }}
            animate={prefersReducedMotion ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
            exit={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: 12 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.3, ease: [0.16, 1, 0.3, 1] }}
            className={cn("fixed left-1/2 top-1/2 z-50 w-full -translate-x-1/2 -translate-y-1/2", panelClassName)}
          >
            {/* biome-ignore lint/a11y/useAriaPropsSupportedByRole: role is always dialog|alertdialog; both support aria-modal */}
            <div
              role={role}
              aria-modal="true"
              aria-label={ariaLabel}
              className={cn(variant === "subtle" ? "modal-surface-subtle" : "modal-surface", surfaceClassName)}
            >
              {variant === "default" && (
                <>
                  <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
                  <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-accent/10 blur-[60px] opacity-70" />
                </>
              )}
              <div className={cn("relative z-10", contentClassName)}>{children}</div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

export function ModalCloseButton({ onClose, disabled }: { onClose: () => void; disabled?: boolean }) {
  const { t } = useTranslation();
  return (
    <button
      onClick={onClose}
      disabled={disabled}
      aria-label={t("common.close")}
      className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer disabled:cursor-not-allowed disabled:opacity-50"
    >
      <X className="w-4 h-4" />
    </button>
  );
}

interface ModalHeaderProps {
  /** Small icon rendered inside the rounded accent box, e.g. <Download className="w-4 h-4 text-primary" />. */
  icon?: ReactNode;
  title: ReactNode;
  onClose: () => void;
  closeDisabled?: boolean;
  /** Layout overrides (padding / border). */
  className?: string;
}

export function ModalHeader({ icon, title, onClose, closeDisabled, className }: ModalHeaderProps) {
  return (
    <div
      className={cn("flex items-center justify-between px-6 pt-4 pb-3 shrink-0 border-b border-border/60", className)}
    >
      <div className="flex items-center gap-2.5">
        {icon && <div className="w-8 h-8 rounded-xl bg-primary/10 flex items-center justify-center">{icon}</div>}
        <h2 className="text-heading-sm">{title}</h2>
      </div>
      <ModalCloseButton onClose={onClose} disabled={closeDisabled} />
    </div>
  );
}

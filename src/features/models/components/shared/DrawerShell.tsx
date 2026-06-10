import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { X } from "lucide-react";
import { Dialog } from "radix-ui";
import type { ReactNode } from "react";
import { cn } from "../../../../lib/utils";

export interface DrawerShellProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Renders inside the drawer header on the left; usually a brand icon + title. */
  title: ReactNode;
  /** Secondary line under the title — provider status, save state, etc. */
  subtitle?: ReactNode;
  /** Renders on the right of the header, before the close button. */
  headerAction?: ReactNode;
  /** Sticky footer for actions/status. Hidden when omitted. */
  footer?: ReactNode;
  /** Drawer body content. Scrolls vertically. */
  children: ReactNode;
  /** Optional CSS class for the drawer panel. */
  className?: string;
  /** Max panel width utility (default 560px). */
  maxWidthClassName?: string;
}

/**
 * Right-side slide-in drawer used by the Models hub for creating and editing
 * providers. Built on Radix `Dialog` for focus management + accessibility, and
 * Framer Motion for the slide animation.
 */
export function DrawerShell({
  open,
  onOpenChange,
  title,
  subtitle,
  headerAction,
  footer,
  children,
  className,
  maxWidthClassName = "max-w-[560px]",
}: DrawerShellProps) {
  const prefersReducedMotion = useReducedMotion();

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <AnimatePresence>
        {open ? (
          <Dialog.Portal forceMount>
            <Dialog.Overlay asChild>
              <motion.div
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: prefersReducedMotion ? 0.01 : 0.2 }}
                className="fixed inset-0 z-[80] bg-black/45 backdrop-blur-sm"
              />
            </Dialog.Overlay>
            <Dialog.Content
              asChild
              onOpenAutoFocus={(e) => {
                // Avoid stealing focus on open — the body has its own inputs.
                e.preventDefault();
              }}
            >
              <motion.aside
                initial={{ x: prefersReducedMotion ? 0 : "100%", opacity: prefersReducedMotion ? 0 : 1 }}
                animate={{ x: 0, opacity: 1 }}
                exit={{ x: prefersReducedMotion ? 0 : "100%", opacity: prefersReducedMotion ? 0 : 1 }}
                transition={{ duration: prefersReducedMotion ? 0.01 : 0.28, ease: [0.22, 1, 0.36, 1] }}
                className={cn(
                  "fixed right-0 top-0 bottom-0 z-[81] flex w-full flex-col",
                  maxWidthClassName,
                  "border-l border-border/60 bg-card/95 backdrop-blur-2xl",
                  "shadow-[-32px_0_80px_-32px_var(--color-shadow)]",
                  className,
                )}
              >
                <header className="flex shrink-0 items-center gap-3 border-b border-border/40 px-5 py-4">
                  <div className="min-w-0 flex-1">
                    <Dialog.Title asChild>
                      <div className="flex min-w-0 items-center gap-2 text-base font-semibold text-foreground">
                        {title}
                      </div>
                    </Dialog.Title>
                    {subtitle ? (
                      <Dialog.Description asChild>
                        <div className="mt-0.5 truncate text-[11px] text-muted-foreground">{subtitle}</div>
                      </Dialog.Description>
                    ) : null}
                  </div>
                  {headerAction}
                  <Dialog.Close asChild>
                    <button
                      type="button"
                      aria-label="关闭"
                      className="shrink-0 cursor-pointer rounded-lg p-1.5 text-muted-foreground transition hover:bg-muted/50 hover:text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
                    >
                      <X className="h-4 w-4" />
                    </button>
                  </Dialog.Close>
                </header>

                <div className="ss-page-scroll min-h-0 flex-1 overflow-y-auto">
                  <div className="px-5 py-5">{children}</div>
                </div>

                {footer ? (
                  <footer className="shrink-0 border-t border-border/40 bg-background/30 px-5 py-3 backdrop-blur-md">
                    {footer}
                  </footer>
                ) : null}
              </motion.aside>
            </Dialog.Content>
          </Dialog.Portal>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}

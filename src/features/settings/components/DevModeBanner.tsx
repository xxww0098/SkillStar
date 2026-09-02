import { AnimatePresence, motion } from "framer-motion";
import { ExternalLink, Monitor, ShieldAlert, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { tauriInvoke } from "../../../lib/ipc";

const DISMISS_KEY = "skillstar:devmode-banner-dismissed";

/**
 * Banner that guides Windows users to enable Developer Mode.
 *
 * Only shown when:
 * 1. Running on Windows (detected via navigator.userAgent)
 * 2. Developer Mode is not enabled (check_developer_mode returns false)
 * 3. User hasn't dismissed it before (localStorage)
 */
export function DevModeBanner() {
  const { t } = useTranslation();
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    // Only relevant on Windows
    const isWindows = navigator.userAgent.includes("Windows");
    if (!isWindows) return;

    // Check if user already dismissed
    try {
      if (localStorage.getItem(DISMISS_KEY)) return;
    } catch {
      // ignore
    }

    // Check developer mode status
    tauriInvoke("check_developer_mode")
      .then((enabled) => {
        if (!enabled) setVisible(true);
      })
      .catch(() => {
        // If the command fails (old backend), don't show the banner
      });
  }, []);

  const handleDismiss = () => {
    setVisible(false);
    try {
      localStorage.setItem(DISMISS_KEY, "1");
    } catch {
      // ignore
    }
  };

  const handleOpenSettings = () => {
    // Open Windows Settings → Developer page via ms-settings URI
    tauriInvoke("open_folder", { path: "ms-settings:developers" }).catch(() => {
      // Fallback: at least dismiss
    });
  };

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: 0.25, ease: "easeOut" }}
          className="overflow-hidden"
        >
          <div className="relative mx-auto max-w-[720px] mt-6 mb-2 lg:pl-16">
            <div className="rounded-xl border border-warning/25 bg-warning/5 p-4 backdrop-blur-sm">
              {/* Close button */}
              <button
                type="button"
                onClick={handleDismiss}
                aria-label={t("settings.devModeBannerDismiss")}
                className="absolute top-3 right-3 cursor-pointer rounded-md p-1 text-muted-foreground/50 transition-colors hover:bg-muted/40 hover:text-foreground/70 focus-ring"
                title={t("settings.devModeBannerDismiss")}
              >
                <X className="h-3.5 w-3.5" aria-hidden />
              </button>

              {/* Header */}
              <div className="mb-2.5 flex items-center gap-2.5">
                <div
                  aria-hidden
                  className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg bg-warning/15 text-warning"
                >
                  <ShieldAlert className="h-4 w-4" />
                </div>
                <h3 className="text-sm font-semibold text-foreground/90">{t("settings.devModeBannerTitle")}</h3>
              </div>

              {/* Description */}
              <p className="text-xs text-muted-foreground/80 leading-relaxed mb-3 pr-6">
                {t("settings.devModeBannerDesc")}
              </p>

              {/* Steps */}
              <div className="space-y-1.5 mb-3.5">
                <div className="flex items-start gap-2">
                  <Monitor className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning/70" aria-hidden />
                  <div>
                    <span className="text-xs font-medium text-foreground/70">Windows 11: </span>
                    <span className="text-xs text-muted-foreground/75 font-mono tracking-tight">
                      {t("settings.devModeBannerStepsWin11")}
                    </span>
                  </div>
                </div>
                <div className="flex items-start gap-2">
                  <Monitor className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning/70" aria-hidden />
                  <div>
                    <span className="text-xs font-medium text-foreground/70">Windows 10: </span>
                    <span className="text-xs text-muted-foreground/75 font-mono tracking-tight">
                      {t("settings.devModeBannerStepsWin10")}
                    </span>
                  </div>
                </div>
              </div>

              {/* Actions */}
              <div className="flex items-center gap-2.5">
                <button
                  type="button"
                  onClick={handleOpenSettings}
                  className="inline-flex cursor-pointer items-center gap-1.5 rounded-lg bg-warning/15 px-3 py-1.5 text-xs font-medium text-warning transition-colors hover:bg-warning/25 focus-ring"
                >
                  <ExternalLink className="h-3 w-3" aria-hidden />
                  {t("settings.devModeBannerLearnMore")}
                </button>
                <button
                  type="button"
                  onClick={handleDismiss}
                  className="cursor-pointer text-xs text-muted-foreground/70 transition-colors hover:text-muted-foreground focus-ring"
                >
                  {t("settings.devModeBannerDismiss")}
                </button>
              </div>
            </div>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

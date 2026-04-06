import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { ExternalLink, Monitor, ShieldAlert, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

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
    invoke<boolean>("check_developer_mode")
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
    invoke("open_folder", { path: "ms-settings:developers" }).catch(() => {
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
            <div className="rounded-xl border border-sky-400/20 bg-sky-500/5 backdrop-blur-sm p-4">
              {/* Close button */}
              <button
                onClick={handleDismiss}
                className="absolute top-3 right-3 p-1 rounded-md text-muted-foreground/50 hover:text-foreground/70 hover:bg-muted/40 transition-colors cursor-pointer"
                title={t("settings.devModeBannerDismiss")}
              >
                <X className="w-3.5 h-3.5" />
              </button>

              {/* Header */}
              <div className="flex items-center gap-2.5 mb-2.5">
                <div className="w-7 h-7 rounded-lg bg-sky-500/12 flex items-center justify-center shrink-0">
                  <ShieldAlert className="w-4 h-4 text-sky-400" />
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
                  <Monitor className="w-3.5 h-3.5 text-sky-400/70 mt-0.5 shrink-0" />
                  <div>
                    <span className="text-xs font-medium text-foreground/70">Windows 11: </span>
                    <span className="text-xs text-muted-foreground/75 font-mono tracking-tight">
                      {t("settings.devModeBannerStepsWin11")}
                    </span>
                  </div>
                </div>
                <div className="flex items-start gap-2">
                  <Monitor className="w-3.5 h-3.5 text-sky-400/70 mt-0.5 shrink-0" />
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
                  onClick={handleOpenSettings}
                  className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-sky-500/15 text-sky-400 hover:bg-sky-500/25 transition-colors cursor-pointer"
                >
                  <ExternalLink className="w-3 h-3" />
                  {t("settings.devModeBannerLearnMore")}
                </button>
                <button
                  onClick={handleDismiss}
                  className="text-xs text-muted-foreground/50 hover:text-muted-foreground/80 transition-colors cursor-pointer"
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

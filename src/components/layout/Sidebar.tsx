import { tauriInvokeDynamic } from "../../lib/ipc";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  AlertCircle,
  ArrowUpCircle,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Download,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  RefreshCw,
  RotateCcw,
  Settings,
  Sun,
  X,
} from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { applyBackgroundStyle, type BackgroundStyle, readBackgroundStyle } from "../../lib/backgroundStyle";
import { useNavigation } from "../../hooks/useNavigation";
import type { UpdateStatus } from "../../hooks/useUpdater";
import { cn, detectPlatform } from "../../lib/utils";
import type { NavPage } from "../../types";
import { FILTER_ALL } from "@/features/usage/types";
import { ModeSwitcher } from "./ModeSwitcher";
import { SkillsNav } from "./SkillsNav";
import { ModelsSidebar } from "./ModelsSidebar";
import { UsageNav } from "./UsageNav";

interface SidebarProps {
  activePage: NavPage;
  onNavigate: (page: NavPage) => void;
  onPrefetch?: (page: NavPage) => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  updateStatus?: UpdateStatus;
  updateVersion?: string;
  updateProgress?: number;
  updateError?: string;
  onUpdate?: () => void;
  onRestart?: () => void;
  onSkip?: () => void;
  onDismiss?: () => void;
  onRetry?: () => void;
  ghostSkillCount?: number;
  pendingUpdatesCount?: number;
}

const footerIconBtn = (collapsed?: boolean, active?: boolean) =>
  cn(
    "flex items-center justify-center rounded-lg text-muted-foreground transition duration-150 cursor-pointer focus-ring",
    collapsed ? "w-8 h-8" : "w-7 h-7",
    active && "bg-primary/10 text-primary",
  );

/* ---------- Theme Switcher (bottom toolbar) ---------- */

function ThemeSwitcher({ collapsed, className }: { collapsed?: boolean; className?: string }) {
  const { t } = useTranslation();
  const [currentStyle, setCurrentStyle] = useState<BackgroundStyle>(() => readBackgroundStyle());

  const toggle = useCallback(() => {
    setCurrentStyle((prev) => {
      const next: BackgroundStyle = prev === "current" ? "paper" : "current";
      applyBackgroundStyle(next);
      return next;
    });
  }, []);

  const ThemeIcon = currentStyle === "current" ? Moon : Sun;

  return (
    <button
      type="button"
      onClick={toggle}
      title={t("settings.backgroundStyle")}
      aria-pressed={currentStyle === "current"}
      className={cn(footerIconBtn(collapsed), className)}
    >
      <ThemeIcon className="w-[15px] h-[15px]" />
    </button>
  );
}

/* ---------- Inline Update Banner ---------- */

function UpdateBanner({
  status,
  version,
  progress,
  error,
  collapsed,
  onUpdate,
  onRestart,
  onSkip,
  onDismiss,
  onRetry,
  t,
}: {
  status: UpdateStatus;
  version: string;
  progress: number;
  error: string;
  collapsed?: boolean;
  onUpdate?: () => void;
  onRestart?: () => void;
  onSkip?: () => void;
  onDismiss?: () => void;
  onRetry?: () => void;
  t: (key: string, opts?: Record<string, unknown>) => string;
}) {
  const [errorExpanded, setErrorExpanded] = useState(false);

  if (status === "idle" || status === "checking") return null;

  if (collapsed) {
    const colorMap: Record<string, string> = {
      available: "bg-primary",
      downloading: "bg-amber-500",
      ready: "bg-emerald-500",
      error: "bg-destructive",
    };
    const iconMap: Record<string, React.ElementType> = {
      available: ArrowUpCircle,
      downloading: Download,
      ready: CheckCircle2,
      error: AlertCircle,
    };
    const Icon = iconMap[status] ?? ArrowUpCircle;
    const badgeBg = colorMap[status] ?? "bg-primary";

    return (
      <div className="px-1.5 pb-1">
        <motion.button
          initial={{ scale: 0, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          transition={{ type: "spring", stiffness: 400, damping: 25 }}
          onClick={status === "ready" ? onRestart : status === "error" ? onRetry : onUpdate}
          title={
            status === "available"
              ? `${t("sidebar.newUpdate")} v${version}`
              : status === "downloading"
                ? `${t("sidebar.downloading")} ${progress}%`
                : status === "ready"
                  ? t("sidebar.readyToInstall")
                  : t("sidebar.updateError")
          }
          className={cn(
            "w-full flex items-center justify-center py-2 rounded-lg cursor-pointer transition-all duration-200",
            "active:scale-95",
            status === "downloading" && "relative overflow-hidden",
          )}
        >
          <div className={cn("w-6 h-6 rounded-full flex items-center justify-center", `${badgeBg}/15`)}>
            <Icon
              className={cn(
                "w-3.5 h-3.5",
                badgeBg.replace("bg-", "text-"),
                status === "downloading" && "animate-pulse",
              )}
            />
          </div>
        </motion.button>
      </div>
    );
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: 10 }}
      transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      className="mx-3 mb-2"
    >
      <div className="rounded-xl border border-border bg-card/80 backdrop-blur-sm overflow-hidden">
        {status === "available" && (
          <div className="p-3">
            <div className="flex items-center gap-2 mb-2">
              <ArrowUpCircle className="w-3.5 h-3.5 text-primary shrink-0" />
              <span className="text-xs font-medium text-foreground">{t("sidebar.newUpdate")}</span>
              <span className="text-[10px] font-mono text-primary ml-auto">v{version}</span>
            </div>
            <div className="flex gap-1.5">
              <button
                onClick={onUpdate}
                className="flex-1 inline-flex justify-center items-center gap-1.5 text-[11px] font-medium bg-primary text-primary-foreground rounded-lg py-1.5 transition-all duration-200 cursor-pointer active:scale-[0.97]"
              >
                <Download className="w-3 h-3" />
                {t("sidebar.updateNow")}
              </button>
              <button
                onClick={onSkip}
                className="inline-flex justify-center items-center text-[11px] text-muted-foreground rounded-lg px-2.5 py-1.5 transition-colors cursor-pointer"
              >
                {t("sidebar.skip")}
              </button>
            </div>
          </div>
        )}

        {status === "downloading" && (
          <div className="p-3">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <RefreshCw className="w-3.5 h-3.5 animate-spin text-primary" />
                <span className="text-xs font-medium text-foreground">{t("sidebar.downloading")}</span>
              </div>
              <span className="text-[11px] font-mono font-semibold text-primary tabular-nums">{progress}%</span>
            </div>
            <div className="h-1.5 bg-muted/60 rounded-full overflow-hidden">
              <motion.div
                className="h-full rounded-full bg-primary"
                initial={{ width: 0 }}
                animate={{ width: `${progress}%` }}
                transition={{ duration: 0.4, ease: "easeOut" }}
              />
            </div>
          </div>
        )}

        {status === "ready" && (
          <div className="p-3">
            <div className="flex items-center gap-2 mb-2">
              <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" />
              <span className="text-xs font-medium text-emerald-500">{t("sidebar.readyToInstall")}</span>
            </div>
            <button
              onClick={onRestart}
              className="w-full inline-flex justify-center items-center gap-1.5 text-[11px] font-medium bg-emerald-500 text-white rounded-lg py-1.5 transition-all duration-200 cursor-pointer active:scale-[0.97]"
            >
              <RotateCcw className="w-3 h-3" />
              {t("sidebar.restart")}
            </button>
          </div>
        )}

        {status === "error" && (
          <div className="p-3">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <AlertCircle className="w-3.5 h-3.5 text-destructive" />
                <span className="text-xs font-medium text-destructive">{t("sidebar.updateError")}</span>
              </div>
              {onDismiss && (
                <button
                  onClick={onDismiss}
                  aria-label="Dismiss"
                  className="text-muted-foreground p-0.5 rounded transition-colors cursor-pointer"
                >
                  <X className="w-3 h-3" />
                </button>
              )}
            </div>
            {error && (
              <div className="mb-2">
                <div
                  className={cn(
                    "text-[10px] text-muted-foreground leading-relaxed break-words",
                    !errorExpanded && "line-clamp-2",
                  )}
                >
                  {error}
                </div>
                {error.length > 80 && (
                  <button
                    onClick={() => setErrorExpanded((v) => !v)}
                    className="text-[10px] text-muted-foreground/70 mt-0.5 flex items-center gap-0.5 cursor-pointer"
                  >
                    {errorExpanded ? <ChevronUp className="w-2.5 h-2.5" /> : <ChevronDown className="w-2.5 h-2.5" />}
                    {errorExpanded ? t("common.hide") : t("common.more")}
                  </button>
                )}
              </div>
            )}
            <button
              onClick={onRetry}
              className="w-full inline-flex justify-center items-center gap-1.5 text-[11px] font-medium bg-destructive/10 text-destructive border border-destructive/20 rounded-lg py-1.5 transition-colors cursor-pointer"
            >
              <RefreshCw className="w-3 h-3" />
              {t("common.retry")}
            </button>
          </div>
        )}
      </div>
    </motion.div>
  );
}

/* ---------- Main Sidebar Component (Shell) ---------- */

export function Sidebar({
  activePage,
  onNavigate,
  onPrefetch,
  collapsed = false,
  onToggleCollapse,
  updateStatus,
  updateVersion,
  updateProgress,
  updateError,
  onUpdate,
  onRestart,
  onSkip,
  onDismiss,
  onRetry,
  ghostSkillCount,
  pendingUpdatesCount,
}: SidebarProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const isDev = import.meta.env.DEV;
  const {
    appMode,
    setAppMode,
    navigate,
    selectedProviderId,
    setSelectedProviderId,
    openModelsDrawer,
    usageCatalogFilter,
    setUsageCatalogFilter,
    openUsageCreate,
  } = useNavigation();

  const isSettingsActive = appMode === "skills" && activePage === "settings";

  const openSettings = useCallback(() => {
    navigate("settings");
  }, [navigate]);

  const handleLogoClick = async () => {
    if (!isDev) return;
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
    try {
      await tauriInvokeDynamic("plugin:webview|internal_toggle_devtools");
    } catch (error) {
      if (import.meta.env.DEV) console.warn("[Sidebar] Failed to toggle devtools from logo click", error);
    }
  };

  const handleAddProvider = useCallback(() => {
    setSelectedProviderId(null);
    openModelsDrawer({ kind: "create" });
  }, [setSelectedProviderId, openModelsDrawer]);

  const handleSelectProvider = useCallback(
    (id: string) => {
      openModelsDrawer({ kind: "edit", providerId: id });
    },
    [openModelsDrawer],
  );

  const handleUsageAddNew = useCallback(() => {
    const preselect = usageCatalogFilter === FILTER_ALL ? null : usageCatalogFilter;
    openUsageCreate(preselect);
  }, [openUsageCreate, usageCatalogFilter]);

  const isMacDesktop = detectPlatform() === "macos";

  const logoMark = (
    <motion.div
      whileHover={prefersReducedMotion ? undefined : "hover"}
      initial="idle"
      variants={
        prefersReducedMotion
          ? undefined
          : {
              idle: { scale: 1 },
              hover: { scale: 1.06 },
            }
      }
      transition={{ duration: 0.16, ease: "easeOut" }}
      onClick={isDev ? handleLogoClick : undefined}
      onKeyDown={
        isDev
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                void handleLogoClick();
              }
            }
          : undefined
      }
      role={isDev ? "button" : undefined}
      tabIndex={isDev ? 0 : undefined}
      title={isDev ? "Open DevTools" : undefined}
      className="w-8 h-8 rounded-lg overflow-hidden bg-white shadow-sm shrink-0 cursor-pointer relative"
      style={{ transform: "translateZ(0)" }}
    >
      <motion.img
        variants={
          prefersReducedMotion
            ? undefined
            : {
                idle: { rotate: 0 },
                hover: {
                  rotate: 360,
                  transition: { duration: 0.8, ease: [0.16, 1, 0.3, 1] },
                },
              }
        }
        src="/skillstar-icon.svg"
        alt="SkillStar"
        className="w-full h-full origin-center will-change-transform"
      />
      {collapsed && updateStatus && updateStatus !== "idle" && updateStatus !== "checking" && (
        <motion.span
          initial={{ scale: 0 }}
          animate={{ scale: 1 }}
          className={cn(
            "absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full border border-card",
            updateStatus === "available" && "bg-primary",
            updateStatus === "downloading" && "bg-amber-500 animate-pulse",
            updateStatus === "ready" && "bg-emerald-500",
            updateStatus === "error" && "bg-destructive",
          )}
        />
      )}
    </motion.div>
  );

  return (
    <motion.aside
      initial={{ opacity: 0, x: -10 }}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: prefersReducedMotion ? 0.01 : 0.2 }}
      className={cn(
        "fixed top-0 left-2 bottom-2 flex flex-col z-50",
        "transition-[width] duration-200 ease-out will-change-[width]",
        collapsed ? "w-14" : "w-[180px]",
        collapsed ? "overflow-hidden" : "",
      )}
    >
      {/* ── Top: traffic-light lane, then centered brand (shared) ── */}
      <div className="flex w-full shrink-0 flex-col">
        <div
          data-tauri-drag-region
          className={cn("w-full shrink-0", collapsed ? "h-7" : isMacDesktop ? "h-9" : "h-7")}
          aria-hidden
        />
        {!collapsed ? (
          <div data-tauri-drag-region className="flex items-center justify-center gap-2.5 px-3 pb-2.5">
            {logoMark}
            <div className="flex min-w-0 flex-col items-center text-center">
              <span className="text-[15px] font-semibold text-foreground leading-tight tracking-tight truncate">
                SkillStar
              </span>
              <span className="text-[11px] text-muted-foreground/75 leading-snug">{t("sidebar.tagline")}</span>
            </div>
          </div>
        ) : (
          <div className="flex justify-center px-2 pb-2">{logoMark}</div>
        )}
      </div>

      {/* ── Mode Switcher (shared) ── */}
      <div className={cn("pb-1", collapsed ? "px-0" : "px-3")}>
        <ModeSwitcher currentMode={appMode} onModeChange={setAppMode} collapsed={collapsed} />
      </div>

      {/* ── Navigation (conditional based on appMode) ── */}
      <nav className={cn("flex-1 py-2 overflow-y-auto", collapsed ? "px-1.5" : "px-2")}>
        {appMode === "models" ? (
          <ModelsSidebar
            selectedProviderId={selectedProviderId}
            onSelectProvider={handleSelectProvider}
            onAddProvider={handleAddProvider}
            collapsed={collapsed}
          />
        ) : appMode === "usage" ? (
          <UsageNav
            selected={usageCatalogFilter}
            onSelect={setUsageCatalogFilter}
            onAddNew={handleUsageAddNew}
            collapsed={collapsed}
          />
        ) : (
          <SkillsNav
            activePage={activePage}
            onNavigate={onNavigate}
            onPrefetch={onPrefetch}
            collapsed={collapsed}
            ghostSkillCount={ghostSkillCount}
            pendingUpdatesCount={pendingUpdatesCount}
          />
        )}
      </nav>

      {/* ── Update banner (shared) ── */}
      <AnimatePresence>
        {updateStatus && updateStatus !== "idle" && updateStatus !== "checking" && (
          <UpdateBanner
            status={updateStatus}
            version={updateVersion ?? ""}
            progress={updateProgress ?? 0}
            error={updateError ?? ""}
            collapsed={collapsed}
            onUpdate={onUpdate}
            onRestart={onRestart}
            onSkip={onSkip}
            onDismiss={onDismiss}
            onRetry={onRetry}
            t={t}
          />
        )}
      </AnimatePresence>

      {/* ── Bottom: settings + theme + collapse (shared) ── */}
      <div className={cn("py-2 border-t border-border/40", collapsed ? "px-2" : "px-3")}>
        <div
          className={cn(
            "flex items-center",
            collapsed
              ? "flex-col gap-0.5"
              : "justify-between gap-1 rounded-lg bg-muted/30 p-0.5 ring-1 ring-inset ring-border/20 dark:bg-muted/15",
          )}
        >
          {onToggleCollapse && (
            <button
              type="button"
              onClick={onToggleCollapse}
              title={collapsed ? t("sidebar.expand") : t("sidebar.collapse")}
              className={cn(footerIconBtn(collapsed), collapsed && "order-last")}
            >
              {collapsed ? (
                <PanelLeftOpen className="w-[15px] h-[15px]" />
              ) : (
                <PanelLeftClose className="w-[15px] h-[15px]" />
              )}
            </button>
          )}
          <ThemeSwitcher collapsed={collapsed} className={cn(collapsed && "order-2")} />
          <button
            type="button"
            onClick={openSettings}
            title={t("sidebar.settings")}
            aria-current={isSettingsActive ? "page" : undefined}
            className={cn(footerIconBtn(collapsed, isSettingsActive), collapsed && "order-first")}
          >
            <Settings className="w-[15px] h-[15px]" strokeWidth={isSettingsActive ? 2.25 : 2} />
          </button>
        </div>
      </div>
    </motion.aside>
  );
}

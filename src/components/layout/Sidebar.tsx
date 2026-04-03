import { useMemo, useState } from "react";
import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import {
  Globe,
  Settings,
  Package,
  Layers,
  FolderKanban,
  ArrowUpCircle,
  RefreshCw,
  CheckCircle2,
  AlertCircle,
  X,
  ShieldCheck,
  Radar,
  Orbit,
  FileText,
  ChevronDown,
  ChevronUp,
  Download,
  RotateCcw,
} from "lucide-react";
import { cn } from "../../lib/utils";
import { getFileTheme, getRiskTone } from "../../lib/securityScanTheme";
import type { NavPage } from "../../types";
import type { UpdateStatus } from "../../hooks/useUpdater";
import { useSecurityScan } from "../../features/security/hooks/useSecurityScan";

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
}

/* ---------- Inline Update Banner (sidebar bottom) ---------- */

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

  // Collapsed mode: show a compact badge that hints at the update
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
          title={status === "available" ? `${t("sidebar.newUpdate")} v${version}` : status === "downloading" ? `${t("sidebar.downloading")} ${progress}%` : status === "ready" ? t("sidebar.readyToInstall") : t("sidebar.updateError")}
          className={cn(
            "w-full flex items-center justify-center py-2.5 rounded-lg cursor-pointer transition-all duration-200",
            "hover:scale-105 active:scale-95",
            status === "downloading" && "relative overflow-hidden",
          )}
        >
          <div className={cn("w-7 h-7 rounded-full flex items-center justify-center", `${badgeBg}/15`)}>
            <Icon className={cn("w-4 h-4", badgeBg.replace("bg-", "text-"), status === "downloading" && "animate-pulse")} />
          </div>
          {/* Downloading ring progress */}
          {status === "downloading" && (
            <svg className="absolute inset-0 w-full h-full" viewBox="0 0 40 40" style={{ transform: "rotate(-90deg)" }}>
              <circle cx="20" cy="20" r="16" fill="none" stroke="currentColor" strokeWidth="2" className="text-border" />
              <motion.circle
                cx="20" cy="20" r="16" fill="none" stroke="currentColor" strokeWidth="2.5"
                className="text-amber-500"
                strokeLinecap="round"
                strokeDasharray={100.5}
                initial={{ strokeDashoffset: 100.5 }}
                animate={{ strokeDashoffset: 100.5 - (progress / 100) * 100.5 }}
                transition={{ duration: 0.3 }}
              />
            </svg>
          )}
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
        {/* ── Available ───────────────────────────────────── */}
        {status === "available" && (
          <div className="p-3">
            <div className="flex items-center gap-2 mb-2.5">
              <div className="w-6 h-6 rounded-lg bg-primary/10 flex items-center justify-center shrink-0">
                <ArrowUpCircle className="w-3.5 h-3.5 text-primary" />
              </div>
              <div className="flex-1 min-w-0">
                <div className="text-xs font-semibold text-foreground leading-none">{t("sidebar.newUpdate")}</div>
              </div>
              <span className="text-[10px] font-mono font-medium px-1.5 py-0.5 rounded-md border border-primary/20 bg-primary/5 text-primary shrink-0">
                v{version}
              </span>
            </div>
            <div className="flex gap-1.5">
              <button
                onClick={onUpdate}
                className="flex-1 inline-flex justify-center items-center gap-1.5 text-[11px] font-medium bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg py-1.5 transition-all duration-200 cursor-pointer shadow-sm active:scale-[0.97]"
              >
                <Download className="w-3 h-3" />
                {t("sidebar.updateNow")}
              </button>
              <button
                onClick={onSkip}
                className="inline-flex justify-center items-center text-[11px] font-medium text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded-lg px-2.5 py-1.5 transition-colors cursor-pointer"
              >
                {t("sidebar.skip")}
              </button>
            </div>
          </div>
        )}

        {/* ── Downloading ────────────────────────────────── */}
        {status === "downloading" && (
          <div className="p-3">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <RefreshCw className="w-3.5 h-3.5 animate-spin text-primary" />
                <span className="text-xs font-medium text-foreground">{t("sidebar.downloading")}</span>
              </div>
              <span className="text-[11px] font-mono font-semibold text-primary tabular-nums">{progress}%</span>
            </div>
            {/* Progress bar */}
            <div className="h-1.5 bg-muted/60 rounded-full overflow-hidden">
              <motion.div
                className="h-full rounded-full relative"
                style={{ background: "linear-gradient(90deg, var(--color-primary) 0%, color-mix(in oklch, var(--color-primary), white 20%) 100%)" }}
                initial={{ width: 0 }}
                animate={{ width: `${progress}%` }}
                transition={{ duration: 0.4, ease: "easeOut" }}
              >
                <motion.div
                  className="absolute inset-0 bg-white/25 rounded-full"
                  animate={{ opacity: [0.3, 0.6, 0.3] }}
                  transition={{ duration: 1.5, repeat: Infinity, ease: "easeInOut" }}
                />
              </motion.div>
            </div>
            {version && (
              <div className="text-[10px] text-muted-foreground mt-1.5">v{version}</div>
            )}
          </div>
        )}

        {/* ── Ready ──────────────────────────────────────── */}
        {status === "ready" && (
          <div className="p-3">
            <div className="flex items-center gap-2 mb-2.5">
              <div className="w-6 h-6 rounded-lg bg-emerald-500/10 flex items-center justify-center shrink-0">
                <CheckCircle2 className="w-3.5 h-3.5 text-emerald-500" />
              </div>
              <span className="text-xs font-semibold text-emerald-500">{t("sidebar.readyToInstall")}</span>
            </div>
            <button
              onClick={onRestart}
              className="w-full inline-flex justify-center items-center gap-1.5 text-[11px] font-medium bg-emerald-500 hover:bg-emerald-600 text-white rounded-lg py-1.5 transition-all duration-200 cursor-pointer shadow-sm active:scale-[0.97]"
            >
              <RotateCcw className="w-3 h-3" />
              {t("sidebar.restart")}
            </button>
          </div>
        )}

        {/* ── Error ──────────────────────────────────────── */}
        {status === "error" && (
          <div className="p-3">
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <AlertCircle className="w-3.5 h-3.5 text-destructive" />
                <span className="text-xs font-semibold text-destructive">{t("sidebar.updateError")}</span>
              </div>
              {onDismiss && (
                <button
                  onClick={onDismiss}
                  aria-label="Dismiss"
                  className="text-muted-foreground hover:text-foreground p-1 rounded transition-colors cursor-pointer"
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
                    className="text-[10px] text-muted-foreground/70 hover:text-muted-foreground mt-0.5 flex items-center gap-0.5 cursor-pointer"
                  >
                    {errorExpanded ? <ChevronUp className="w-2.5 h-2.5" /> : <ChevronDown className="w-2.5 h-2.5" />}
                    {errorExpanded ? t("common.hide") : t("common.more")}
                  </button>
                )}
              </div>
            )}
            <button
              onClick={onRetry}
              className="w-full inline-flex justify-center items-center gap-1.5 text-[11px] font-medium bg-destructive/10 text-destructive hover:bg-destructive/20 border border-destructive/20 rounded-lg py-1.5 transition-colors cursor-pointer"
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
}: SidebarProps) {
  const { t } = useTranslation();
  const { phase, activeSkills, currentFile, currentStage, scanAngle, recentFiles } = useSecurityScan();
  const [isAnimating, setIsAnimating] = useState(false);
  const [hoveredNavItem, setHoveredNavItem] = useState<NavPage | null>(null);
  const prefersReducedMotion = useReducedMotion();
  const isDev = import.meta.env.DEV;
  const scanTooltip = useMemo(() => {
    if (phase !== "scanning") return null;
    return [
      currentStage ? `${currentStage.toUpperCase()}` : "SCANNING",
      activeSkills[0] || "skill",
      currentFile || "queue warmup",
    ].join("\n");
  }, [phase, activeSkills, currentFile, currentStage]);

  const handleLogoClick = async () => {
    if (!isDev) return;
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
      return;
    }

    try {
      await invoke("plugin:webview|internal_toggle_devtools");
    } catch (error) {
      console.warn("[Sidebar] Failed to toggle devtools from logo click", error);
    }
  };

  const navItems: { id: NavPage; label: string; icon: React.ElementType }[] = [
    { id: "my-skills", label: t("sidebar.skills"), icon: Package },
    { id: "marketplace", label: t("sidebar.market"), icon: Globe },
    { id: "skill-cards", label: t("sidebar.groups"), icon: Layers },
    { id: "projects", label: t("sidebar.projects"), icon: FolderKanban },
    { id: "security-scan", label: t("sidebar.security", "Security"), icon: ShieldCheck },
    { id: "settings", label: t("sidebar.settings"), icon: Settings },
  ];

    return (
    <motion.aside
      initial={{ opacity: 0, x: -10 }}
      animate={{ opacity: 1, x: 0, width: collapsed ? 56 : 220 }}
      transition={{ duration: prefersReducedMotion ? 0.01 : 0.2 }}
      onAnimationStart={() => setIsAnimating(true)}
      onAnimationComplete={() => setIsAnimating(false)}
      className={cn(
        "h-full bg-sidebar backdrop-blur-md border-r border-border flex flex-col relative z-50 will-change-transform shrink-0",
        (isAnimating || collapsed) ? "overflow-hidden" : ""
      )}
    >
      {/* Logo */}
      <div className="border-b border-border-subtle backdrop-blur-md">
        <div className={cn("flex flex-col items-center justify-center", collapsed ? "pt-4 pb-3" : "pt-6 pb-4")}>
          <div className={cn("flex items-center justify-center gap-2 mb-1 w-full", collapsed ? "px-0" : "px-2")}>
            <motion.div
              whileHover="hover"
              initial="idle"
              onClick={isDev ? handleLogoClick : undefined}
              onKeyDown={
                isDev
                  ? (event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        void handleLogoClick();
                      }
                    }
                  : undefined
              }
              role={isDev ? "button" : undefined}
              tabIndex={isDev ? 0 : undefined}
              title={isDev ? "Open DevTools" : undefined}
              variants={{
                idle: { scale: 1 },
                hover: { scale: 1.06 },
              }}
              transition={{ duration: 0.16, ease: "easeOut" }}
              className="w-7 h-7 rounded-lg overflow-hidden bg-white border-border shadow-sm shrink-0 cursor-pointer relative"
              style={{ transform: "translateZ(0)" }}
            >
              <motion.img
                variants={{
                  idle: { rotate: 0 },
                  hover: { 
                    rotate: 360, 
                    transition: { duration: 0.8, ease: [0.16, 1, 0.3, 1] } 
                  },
                }}
                src="/skillstar-icon.svg"
                alt="SkillStar icon"
                className="w-full h-full origin-center will-change-transform"
              />
              {/* Badge overlay for update status on logo when collapsed */}
              {collapsed && updateStatus && updateStatus !== "idle" && updateStatus !== "checking" && (
                <motion.span
                  initial={{ scale: 0 }}
                  animate={{ scale: 1 }}
                  className={cn(
                    "absolute -top-0.5 -right-0.5 w-2.5 h-2.5 rounded-full border-2 border-sidebar",
                    updateStatus === "available" && "bg-primary",
                    updateStatus === "downloading" && "bg-amber-500 animate-pulse",
                    updateStatus === "ready" && "bg-emerald-500",
                    updateStatus === "error" && "bg-destructive",
                  )}
                />
              )}
            </motion.div>
            {!collapsed && (
              <span className="text-lg font-bold tracking-tight text-foreground whitespace-nowrap">SkillStar</span>
            )}
          </div>
          {!collapsed && (
            <span className="text-micro font-medium text-muted-foreground/60 tracking-widest uppercase">{t("sidebar.tagline")}</span>
          )}
        </div>
      </div>

      {/* Navigation */}
      <nav className={cn("flex-1 py-3", collapsed ? "px-1.5" : "px-3")}>
        {!collapsed && (
          <div className="text-caption uppercase tracking-wider px-2 mb-2 font-medium">
            {t("sidebar.navigation")}
          </div>
        )}
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = activePage === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onNavigate(item.id)}
              onMouseEnter={() => onPrefetch?.(item.id)}
              onMouseOver={() => setHoveredNavItem(item.id)}
              onMouseLeave={() => setHoveredNavItem((prev) => (prev === item.id ? null : prev))}
              onFocus={() => onPrefetch?.(item.id)}
              onBlur={() => setHoveredNavItem((prev) => (prev === item.id ? null : prev))}
              title={collapsed ? item.label : undefined}
              aria-current={isActive ? "page" : undefined}
              className={cn(
                "w-full flex items-center rounded-lg text-sm transition duration-200 mb-0.5 cursor-pointer focus-ring",
                collapsed ? "justify-center px-0 py-2.5" : "gap-2.5 px-3 py-2",
                isActive
                  ? "bg-sidebar-active text-primary font-medium shadow-[0_0_12px_rgba(var(--color-primary-rgb),0.15)]"
                  : "text-muted-foreground hover:bg-sidebar-hover hover:text-foreground"
              )}
            >
              <div className="relative flex items-center justify-center">
                <Icon className={cn("shrink-0", collapsed ? "w-5 h-5" : "w-4 h-4")} />
                {item.id === "security-scan" && phase === "scanning" && (
                  <>
                    <motion.span
                      className="absolute -inset-1 rounded-full border border-success/40"
                      animate={{ opacity: [0.5, 0, 0.5], scale: [0.9, 1.25, 0.9] }}
                      transition={{ duration: 1.5, repeat: Infinity, ease: "easeInOut" }}
                    />
                    <motion.span
                      className="absolute -bottom-1.5 -right-1.5 flex h-3.5 w-3.5 items-center justify-center rounded-full border border-success/30 bg-success/15 shadow-[0_0_10px_rgba(var(--color-success-rgb),0.3)]"
                      title={scanTooltip || undefined}
                    >
                      <motion.div
                        animate={{ rotate: scanAngle }}
                        transition={{ duration: 0.55, ease: [0.22, 1, 0.36, 1] }}
                        className="flex items-center justify-center"
                      >
                        <Radar className="h-2 w-2 text-success" />
                      </motion.div>
                    </motion.span>
                  </>
                )}
              </div>
              {!collapsed && item.label}
              {item.id === "security-scan" && phase === "scanning" && !collapsed && (
                <div className="ml-auto flex min-w-0 items-center gap-1.5">
                  <span className="max-w-[74px] truncate text-[9px] uppercase tracking-[0.18em] text-success/70">
                    {currentStage ?? "scan"}
                  </span>
                  <span className="h-1.5 w-1.5 rounded-full bg-success shadow-[0_0_8px_rgba(var(--color-success-rgb),0.6)]" />
                </div>
              )}

              <AnimatePresence>
                {item.id === "security-scan" && phase === "scanning" && hoveredNavItem === item.id && (
                  <motion.div
                    initial={{ opacity: 0, x: collapsed ? 8 : 6, y: 4, scale: 0.97 }}
                    animate={{ opacity: 1, x: collapsed ? 12 : 8, y: 0, scale: 1 }}
                    exit={{ opacity: 0, x: collapsed ? 8 : 6, y: 4, scale: 0.97 }}
                    transition={{ duration: 0.16, ease: "easeOut" }}
                    className={cn(
                      "absolute z-[120] min-w-[220px] rounded-[14px] border border-success/15 bg-background/95 backdrop-blur-xl p-3 shadow-[0_10px_28px_rgba(0,0,0,0.35)]",
                      collapsed ? "left-[calc(100%+8px)] top-1/2 -translate-y-1/2" : "left-[calc(100%-4px)] top-1/2 -translate-y-1/2"
                    )}
                  >
                    <div className="flex items-center justify-between gap-3 mb-2">
                      <div className="flex items-center gap-2 text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
                        <Radar className="w-3.5 h-3.5 text-success/80" />
                        Live Scan
                      </div>
                      <span className="font-mono text-[10px] text-success/90 tabular-nums">
                        {Math.round(scanAngle)} deg
                      </span>
                    </div>

                    <div className="rounded-lg border border-success/20 bg-muted/50 px-2.5 py-2 mb-2">
                      <div className="text-[9px] uppercase tracking-[0.18em] text-muted-foreground mb-1">
                        Current Target
                      </div>
                      <div className="text-[11px] font-medium text-success truncate">
                        {activeSkills[0] ?? "Awaiting skill"}
                      </div>
                      <div className="mt-1 text-[10px] text-muted-foreground truncate">
                        {currentFile ?? "Preparing file queue..."}
                      </div>
                      <div className="mt-1 text-[9px] uppercase tracking-[0.16em] text-success/80">
                        {currentStage ?? "scan"}
                      </div>
                    </div>

                    <div className="space-y-1.5">
                      <div className="flex items-center gap-1.5 text-[9px] uppercase tracking-[0.18em] text-muted-foreground">
                        <Orbit className="w-3.5 h-3.5 text-success/80" />
                        Recent Trail
                      </div>
                      {recentFiles.length > 0 ? (
                        recentFiles.slice(0, 3).map((item_) => {
                          const fileTheme = getFileTheme(item_.fileName);
                          const riskTone = getRiskTone(item_.riskLevel);

                          return (
                            <div key={`${item_.fileName}-${item_.timestamp}`} className={`flex items-start gap-2 rounded-lg border border-border/50 bg-muted/50 px-2 py-1.5 text-[10px] ${riskTone.glow}`}>
                              <span className={`mt-1.5 h-1.5 w-1.5 rounded-full shrink-0 ${riskTone.dot}`} />
                              <div className="min-w-0 flex-1">
                                <div className="flex min-w-0 items-center gap-2">
                                  <div className={`truncate ${fileTheme.tintText}`}>{item_.fileName.split("/").pop()}</div>
                                  <span className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[8px] uppercase tracking-[0.16em] ${riskTone.text} ${riskTone.pill}`}>
                                    {item_.riskLevel ?? "Safe"}
                                  </span>
                                </div>
                                <div className="truncate text-muted-foreground">{item_.fileName}</div>
                                {item_.reasonLabels && item_.reasonLabels.length > 0 && (
                                  <div className="mt-1 flex flex-wrap gap-1">
                                    {item_.reasonLabels.map((label) => (
                                      <span
                                        key={label}
                                        className="rounded-full border border-border bg-muted/50 px-1.5 py-0.5 text-[8px] uppercase tracking-[0.14em] text-muted-foreground"
                                      >
                                        {label}
                                      </span>
                                    ))}
                                  </div>
                                )}
                              </div>
                            </div>
                          );
                        })
                      ) : (
                        <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
                          <FileText className="w-3 h-3" />
                          No trail yet
                        </div>
                      )}
                    </div>
                  </motion.div>
                )}
              </AnimatePresence>
            </button>
          );
        })}
      </nav>

      {/* Update banner (above collapse toggle) */}
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

      {/* Collapse toggle */}
      {onToggleCollapse && (
        <div className={cn("py-3 border-t border-border-subtle", collapsed ? "px-1.5" : "px-3")}>
          <button
            onClick={onToggleCollapse}
            title={collapsed ? t("sidebar.expand") : t("sidebar.collapse")}
            className={cn(
              "w-full flex items-center rounded-lg text-sm text-muted-foreground hover:bg-sidebar-hover hover:text-foreground transition duration-200 cursor-pointer focus-ring",
              collapsed ? "justify-center px-0 py-2.5" : "gap-2.5 px-3 py-2"
            )}
          >
            {collapsed ? <PanelLeftOpen className="w-4 h-4 shrink-0" /> : <PanelLeftClose className="w-4 h-4 shrink-0" />}
            {!collapsed && t("sidebar.collapse")}
          </button>
        </div>
      )}
    </motion.aside>
  );
}

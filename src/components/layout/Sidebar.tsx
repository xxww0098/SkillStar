import { useState } from "react";
import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";
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
} from "lucide-react";
import { cn } from "../../lib/utils";
import type { NavPage } from "../../types";
import type { UpdateStatus } from "../../hooks/useUpdater";

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
}

/* ---------- tiny status indicator with hover popover ---------- */

function UpdateIndicator({
  status,
  version,
  progress,
  error,
  onUpdate,
  onRestart,
  onSkip,
  onDismiss,
  t,
}: {
  status: UpdateStatus;
  version: string;
  progress: number;
  error: string;
  onUpdate?: () => void;
  onRestart?: () => void;
  onSkip?: () => void;
  onDismiss?: () => void;
  t: (key: string, opts?: Record<string, unknown>) => string;
}) {
  const [hovered, setHovered] = useState(false);

  if (status === "idle" || status === "checking") return null;

  const iconMap: Record<string, { Icon: React.ElementType; color: string; bg: string; pulse?: boolean; spin?: boolean }> = {
    available:   { Icon: ArrowUpCircle, color: "text-blue-500",    bg: "bg-blue-500/10 hover:bg-blue-500/20" },
    downloading: { Icon: RefreshCw,     color: "text-amber-500",   bg: "bg-amber-500/10 hover:bg-amber-500/20", spin: true },
    ready:       { Icon: CheckCircle2,  color: "text-emerald-500", bg: "bg-emerald-500/10 hover:bg-emerald-500/20", pulse: true },
    error:       { Icon: AlertCircle,   color: "text-red-500",     bg: "bg-red-500/10 hover:bg-red-500/20" },
  };
  const cfg = iconMap[status];
  if (!cfg) return null;
  const { Icon, color, bg, pulse, spin } = cfg;

  const renderPopover = () => {
    switch (status) {
      case "available":
        return (
          <div className="flex flex-col gap-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-semibold text-foreground">{t("sidebar.newUpdate")}</span>
              <span className="text-[10px] font-medium px-1.5 py-0.5 rounded border border-primary/20 bg-primary/5 text-primary">v{version}</span>
            </div>
            <div className="flex gap-1.5 mt-1">
              <button onClick={onUpdate} className="flex-1 inline-flex justify-center items-center text-[11px] font-medium bg-primary text-primary-foreground hover:bg-primary/90 rounded-lg py-1.5 transition-colors cursor-pointer shadow-sm">
                {t("sidebar.updateNow")}
              </button>
              <button onClick={onSkip} className="inline-flex justify-center items-center text-[11px] font-medium bg-secondary text-secondary-foreground hover:bg-secondary/80 rounded-lg px-2.5 py-1.5 transition-colors cursor-pointer">
                {t("sidebar.skip")}
              </button>
            </div>
          </div>
        );
      case "downloading":
        return (
          <div className="flex flex-col gap-2.5">
            <div className="flex items-center justify-between text-xs">
              <span className="font-medium text-foreground flex items-center gap-1.5">
                <RefreshCw className="w-3.5 h-3.5 animate-spin text-muted-foreground" />
                {t("sidebar.downloading")}
              </span>
              <span className="text-[10px] font-semibold text-muted-foreground">{progress}%</span>
            </div>
            <div className="h-1.5 bg-secondary/60 rounded-full overflow-hidden">
              <motion.div className="h-full bg-primary rounded-full relative" initial={{ width: 0 }} animate={{ width: `${progress}%` }} transition={{ duration: 0.3 }}>
                <div className="absolute inset-0 bg-white/20 animate-pulse" />
              </motion.div>
            </div>
          </div>
        );
      case "ready":
        return (
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-1.5 text-emerald-500 mb-0.5">
              <CheckCircle2 className="w-3.5 h-3.5" />
              <span className="text-xs font-semibold">{t("sidebar.readyToInstall")}</span>
            </div>
            <button onClick={onRestart} className="w-full inline-flex justify-center items-center text-[11px] font-medium bg-emerald-500 hover:bg-emerald-600 text-white rounded-lg py-1.5 transition-colors cursor-pointer shadow-sm">
              {t("sidebar.restart")}
            </button>
          </div>
        );
      case "error":
        return (
          <div className="flex flex-col gap-2 relative">
            <div className="flex items-center gap-1.5 text-red-500 pr-5">
              <AlertCircle className="w-3.5 h-3.5" />
              <span className="text-xs font-semibold">{t("sidebar.updateError")}</span>
            </div>
            {onDismiss && (
              <button onClick={onDismiss} className="absolute top-0 right-0 text-muted-foreground hover:text-foreground p-0.5 rounded transition-colors cursor-pointer">
                <X className="w-3.5 h-3.5" />
              </button>
            )}
            <div className="text-[10px] text-muted-foreground leading-relaxed line-clamp-3" title={error}>
              {error}
            </div>
            <button onClick={onUpdate} className="mt-1 w-full inline-flex justify-center items-center text-[11px] font-medium bg-red-500/10 text-red-600 dark:text-red-400 hover:bg-red-500/20 border border-red-500/20 rounded-lg py-1.5 transition-colors cursor-pointer">
              {t("common.retry")}
            </button>
          </div>
        );
      default:
        return null;
    }
  };

  return (
    <div
      className="relative flex items-center"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      {/* Indicator Dot */}
      <motion.div
        initial={{ scale: 0 }}
        animate={{ scale: 1 }}
        className={cn(
          "w-5 h-5 rounded-full flex items-center justify-center cursor-default transition-colors",
          bg
        )}
      >
        <Icon className={cn("w-3 h-3", color, spin && "animate-spin", pulse && "animate-pulse")} />
      </motion.div>

      {/* Floating Card Popover */}
      <AnimatePresence>
        {hovered && (
          <motion.div
            initial={{ opacity: 0, y: 6, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.96 }}
            transition={{ duration: 0.15, ease: "easeOut" }}
             className="absolute top-[calc(100%+8px)] left-1/2 -translate-x-1/2 z-[100] w-[190px] p-3.5 rounded-[14px] bg-sidebar backdrop-blur-xl border border-border overflow-hidden shadow-[0_8px_30px_rgba(0,0,0,0.4)] origin-top"
          >
            {renderPopover()}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
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
}: SidebarProps) {
  const { t } = useTranslation();
  const [isAnimating, setIsAnimating] = useState(false);

  const navItems: { id: NavPage; label: string; icon: React.ElementType }[] = [
    { id: "my-skills", label: t("sidebar.skills"), icon: Package },
    { id: "marketplace", label: t("sidebar.market"), icon: Globe },
    { id: "skill-cards", label: t("sidebar.groups"), icon: Layers },
    { id: "projects", label: t("sidebar.projects"), icon: FolderKanban },
    { id: "settings", label: t("sidebar.settings"), icon: Settings },
  ];

    return (
    <motion.aside
      initial={{ opacity: 0, x: -10 }}
      animate={{ opacity: 1, x: 0, width: collapsed ? 56 : 220 }}
      transition={{ duration: 0.2 }}
      onAnimationStart={() => setIsAnimating(true)}
      onAnimationComplete={() => setIsAnimating(false)}
      className={cn(
        "h-full bg-sidebar backdrop-blur-md border-r border-border flex flex-col relative z-50 will-change-transform shrink-0",
        collapsed ? "min-w-[56px]" : "min-w-[220px]",
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
                    transition: { duration: 2.5, ease: "linear", repeat: Infinity } 
                  },
                }}
                src="/skillstar-icon.svg"
                alt="SkillStar icon"
                className="w-full h-full origin-center will-change-transform"
              />
            </motion.div>
            {!collapsed && (
              <>
                <span className="text-lg font-bold tracking-tight text-foreground whitespace-nowrap">SkillStar</span>
                {updateStatus && updateStatus !== "idle" && updateStatus !== "checking" && (
                  <UpdateIndicator
                    status={updateStatus}
                    version={updateVersion ?? ""}
                    progress={updateProgress ?? 0}
                    error={updateError ?? ""}
                    onUpdate={onUpdate}
                    onRestart={onRestart}
                    onSkip={onSkip}
                    onDismiss={onDismiss}
                    t={t}
                  />
                )}
              </>
            )}
          </div>
          {!collapsed && (
            <span className="text-[10px] font-medium text-muted-foreground/60 tracking-widest uppercase">{t("sidebar.tagline")}</span>
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
              onFocus={() => onPrefetch?.(item.id)}
              title={collapsed ? item.label : undefined}
              className={cn(
                "w-full flex items-center rounded-lg text-sm transition-all duration-200 mb-0.5 cursor-pointer",
                collapsed ? "justify-center px-0 py-2.5" : "gap-2.5 px-3 py-2",
                isActive
                  ? "bg-sidebar-active text-primary font-medium shadow-[0_0_12px_rgba(59,130,246,0.15)]"
                  : "text-muted-foreground hover:bg-sidebar-hover hover:text-foreground hover:backdrop-blur-sm"
              )}
            >
              <Icon className={cn("shrink-0", collapsed ? "w-5 h-5" : "w-4 h-4")} />
              {!collapsed && item.label}
            </button>
          );
        })}
      </nav>

      {/* Collapse toggle */}
      {onToggleCollapse && (
        <div className={cn("py-3 border-t border-border-subtle", collapsed ? "px-1.5" : "px-3")}>
          <button
            onClick={onToggleCollapse}
            className={cn(
              "w-full flex items-center rounded-lg text-sm text-muted-foreground hover:bg-sidebar-hover hover:text-foreground transition-all duration-200 cursor-pointer",
              collapsed ? "justify-center px-0 py-2.5" : "gap-2.5 px-3 py-2"
            )}
          >
            {collapsed ? <PanelLeftOpen className="w-4 h-4 shrink-0" /> : <PanelLeftClose className="w-4 h-4 shrink-0" />}
            {!collapsed && "收起"}
          </button>
        </div>
      )}
    </motion.aside>
  );
}

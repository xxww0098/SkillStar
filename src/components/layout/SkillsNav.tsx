import { motion, useReducedMotion } from "framer-motion";
import { Boxes, FolderKanban, Globe, Layers, Package } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSkillBadgeCounts } from "../../features/my-skills/hooks/useSkills";
import { cn } from "../../lib/utils";
import type { NavPage } from "../../types";

export interface SkillsNavProps {
  activePage: NavPage;
  onNavigate: (page: NavPage) => void;
  onPrefetch?: (page: NavPage) => void;
  collapsed: boolean;
}

type NavItemNode =
  | { type?: "item"; id: NavPage; label: string; icon: React.ElementType }
  | { type: "divider"; id: string };

export function SkillsNav({ activePage, onNavigate, onPrefetch, collapsed }: SkillsNavProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const { ghostSkillCount, pendingUpdatesCount } = useSkillBadgeCounts();

  const navItems: NavItemNode[] = [
    { id: "my-skills", label: t("sidebar.skills"), icon: Package },
    { id: "marketplace", label: t("sidebar.market"), icon: Globe },
    { id: "skill-cards", label: t("sidebar.groups"), icon: Layers },
    { id: "projects", label: t("sidebar.projects"), icon: FolderKanban },
    { id: "mcp", label: t("sidebar.mcp"), icon: Boxes },
  ];

  return (
    <>
      {navItems.map((item) => {
        if (item.type === "divider") {
          return (
            <div key={item.id} className="py-1.5 px-2">
              <div className="h-px bg-border/50" />
            </div>
          );
        }

        const Icon = item.icon;
        const isActive = activePage === item.id;
        return (
          <button
            key={item.id}
            onClick={() => onNavigate(item.id)}
            onMouseEnter={() => onPrefetch?.(item.id)}
            onFocus={() => onPrefetch?.(item.id)}
            title={collapsed ? item.label : undefined}
            aria-current={isActive ? "page" : undefined}
            className={cn(
              "group relative w-full flex items-center rounded-lg text-[13px] transition-colors duration-150 mb-0.5 cursor-pointer focus-ring select-none",
              collapsed ? "justify-center px-0 py-2" : "gap-2.5 px-2.5 py-[7px]",
              isActive
                ? "text-primary font-semibold"
                : "text-muted-foreground hover:text-foreground hover:bg-sidebar-hover font-medium",
              // Collapsed rail paints the highlight statically — same split as
              // ModeSwitcher: only the expanded list gets the sliding thumb.
              isActive && collapsed && "bg-primary/18 shadow-2xs ring-1 ring-inset ring-primary/30 dark:bg-primary/20",
            )}
          >
            {isActive && !collapsed && (
              <motion.div
                layoutId="skills-nav-active"
                aria-hidden="true"
                className="pointer-events-none absolute inset-0 z-0 rounded-lg bg-primary/18 shadow-2xs ring-1 ring-inset ring-primary/30 dark:bg-primary/20"
                transition={
                  prefersReducedMotion ? { duration: 0 } : { type: "spring", stiffness: 520, damping: 38, mass: 0.7 }
                }
              />
            )}
            <div className="relative z-10 flex items-center justify-center">
              <Icon
                className={cn(
                  "shrink-0 transition-transform duration-200 group-hover:scale-110",
                  collapsed ? "w-[18px] h-[18px]" : "w-4 h-4",
                )}
                strokeWidth={isActive ? 2.4 : 2}
              />
            </div>
            {!collapsed && <span className="relative z-10">{item.label}</span>}
            {item.id === "my-skills" && !collapsed && (!!ghostSkillCount || !!pendingUpdatesCount) && (
              <div className="relative z-10 ml-auto flex items-center gap-1">
                {!!pendingUpdatesCount && pendingUpdatesCount > 0 && (
                  <span className="inline-flex items-center justify-center min-w-[16px] h-4 px-1 rounded-full bg-amber-500 text-amber-950 text-[9px] font-bold tabular-nums shadow-xs">
                    {pendingUpdatesCount}
                  </span>
                )}
                {!!ghostSkillCount && ghostSkillCount > 0 && (
                  <span className="inline-flex items-center justify-center min-w-[16px] h-4 px-1 rounded-full bg-primary text-primary-foreground text-[9px] font-bold tabular-nums shadow-xs">
                    +{ghostSkillCount}
                  </span>
                )}
              </div>
            )}
            {item.id === "my-skills" && collapsed && !!ghostSkillCount && ghostSkillCount > 0 && (
              <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-primary border border-card shadow-xs" />
            )}
            {item.id === "my-skills" &&
              collapsed &&
              (!ghostSkillCount || ghostSkillCount === 0) &&
              !!pendingUpdatesCount &&
              pendingUpdatesCount > 0 && (
                <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-amber-500 border border-card shadow-xs" />
              )}
          </button>
        );
      })}
    </>
  );
}

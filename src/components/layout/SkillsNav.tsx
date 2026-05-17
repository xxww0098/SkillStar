import { FolderKanban, Globe, Layers, Package } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../../lib/utils";
import type { NavPage } from "../../types";

export interface SkillsNavProps {
  activePage: NavPage;
  onNavigate: (page: NavPage) => void;
  onPrefetch?: (page: NavPage) => void;
  collapsed: boolean;
  ghostSkillCount?: number;
  pendingUpdatesCount?: number;
}

type NavItemNode =
  | { type?: "item"; id: NavPage; label: string; icon: React.ElementType }
  | { type: "divider"; id: string };

export function SkillsNav({
  activePage,
  onNavigate,
  onPrefetch,
  collapsed,
  ghostSkillCount,
  pendingUpdatesCount,
}: SkillsNavProps) {
  const { t } = useTranslation();

  const navItems: NavItemNode[] = [
    { id: "my-skills", label: t("sidebar.skills"), icon: Package },
    { id: "marketplace", label: t("sidebar.market"), icon: Globe },
    { id: "skill-cards", label: t("sidebar.groups"), icon: Layers },
    { id: "projects", label: t("sidebar.projects"), icon: FolderKanban },
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
              "w-full flex items-center rounded-lg text-[13px] transition duration-150 mb-0.5 cursor-pointer focus-ring",
              collapsed ? "justify-center px-0 py-2" : "gap-2.5 px-2.5 py-[7px]",
              isActive ? "bg-primary/10 text-primary font-medium" : "text-muted-foreground",
            )}
          >
            <div className="relative flex items-center justify-center">
              <Icon className={cn("shrink-0", collapsed ? "w-[18px] h-[18px]" : "w-4 h-4")} />
            </div>
            {!collapsed && <span>{item.label}</span>}
            {item.id === "my-skills" && !collapsed && (!!ghostSkillCount || !!pendingUpdatesCount) && (
              <div className="ml-auto flex items-center gap-1">
                {!!pendingUpdatesCount && pendingUpdatesCount > 0 && (
                  <span className="inline-flex items-center justify-center min-w-[16px] h-4 px-1 rounded-full bg-warning/15 text-warning text-[9px] font-bold tabular-nums">
                    {pendingUpdatesCount}
                  </span>
                )}
                {!!ghostSkillCount && ghostSkillCount > 0 && (
                  <span className="inline-flex items-center justify-center min-w-[16px] h-4 px-1 rounded-full bg-primary/15 text-primary text-[9px] font-bold tabular-nums">
                    +{ghostSkillCount}
                  </span>
                )}
              </div>
            )}
            {item.id === "my-skills" && collapsed && !!ghostSkillCount && ghostSkillCount > 0 && (
              <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-primary border border-card" />
            )}
            {item.id === "my-skills" &&
              collapsed &&
              (!ghostSkillCount || ghostSkillCount === 0) &&
              !!pendingUpdatesCount &&
              pendingUpdatesCount > 0 && (
                <span className="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full bg-warning border border-card" />
              )}
          </button>
        );
      })}
    </>
  );
}

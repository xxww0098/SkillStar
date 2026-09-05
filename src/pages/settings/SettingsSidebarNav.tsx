import {
  Activity,
  EyeOff,
  Globe,
  HardDrive,
  Languages as LanguagesIcon,
  type LucideIcon,
  Paintbrush,
  Sparkles,
  Terminal,
  Unlink,
  Zap,
  Store,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SettingsFocusTarget } from "../../lib/utils";

export const SETTINGS_SECTIONS: { id: string; labelKey: string; icon: LucideIcon }[] = [
  { id: "settings-agents", labelKey: "settings.agentConnections", icon: Unlink },
  { id: "settings-proxy", labelKey: "settings.networkProxy", icon: Globe },
  { id: "settings-mirror", labelKey: "settings.githubMirror", icon: Zap },
  { id: "settings-marketplace-mirror", labelKey: "settings.marketplaceMirror", icon: Store },
  { id: "settings-network-doctor", labelKey: "settings.networkDoctor", icon: Activity },
  { id: "settings-ai", labelKey: "settings.aiProvider", icon: Sparkles },
  { id: "settings-background", labelKey: "settings.backgroundRun", icon: EyeOff },
  { id: "settings-appearance", labelKey: "settings.backgroundStyle", icon: Paintbrush },
  { id: "settings-language", labelKey: "settings.language", icon: LanguagesIcon },
  { id: "settings-storage", labelKey: "settings.storage", icon: HardDrive },
  { id: "settings-about", labelKey: "settings.about", icon: Terminal },
];

export const SETTINGS_FOCUS_TO_SECTION_ID: Record<SettingsFocusTarget, string> = {
  "ai-provider": "settings-ai",
  storage: "settings-storage",
};

export function SettingsSidebarNav() {
  const { t } = useTranslation();
  const [activeId, setActiveId] = useState(SETTINGS_SECTIONS[0].id);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const pendingIdRef = useRef(SETTINGS_SECTIONS[0].id);

  useEffect(() => {
    const scrollRoot = document.getElementById("settings-scroll-container");
    if (!scrollRoot) return;

    const visibleIds = new Set<string>();

    const updateActiveId = () => {
      // If we are at the very bottom, forcefully select the last item
      // Add a 5px threshold to account for fractional pixel rounding errors
      if (Math.abs(scrollRoot.scrollHeight - scrollRoot.scrollTop - scrollRoot.clientHeight) < 5) {
        const lastId = SETTINGS_SECTIONS[SETTINGS_SECTIONS.length - 1].id;
        if (pendingIdRef.current !== lastId) {
          pendingIdRef.current = lastId;
          clearTimeout(timerRef.current);
          timerRef.current = setTimeout(() => setActiveId(lastId), 100);
        }
        return;
      }

      // Otherwise evaluate based on IntersectionObserver visibleIds
      let newId = pendingIdRef.current;
      for (const section of SETTINGS_SECTIONS) {
        if (visibleIds.has(section.id)) {
          newId = section.id;
          break;
        }
      }
      if (newId !== pendingIdRef.current) {
        pendingIdRef.current = newId;
        clearTimeout(timerRef.current);
        timerRef.current = setTimeout(() => setActiveId(newId), 100);
      }
    };

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            visibleIds.add(entry.target.id);
          } else {
            visibleIds.delete(entry.target.id);
          }
        }
        updateActiveId();
      },
      {
        root: scrollRoot,
        rootMargin: "-10px 0px -70% 0px",
        threshold: 0,
      },
    );

    const handleScroll = () => {
      updateActiveId();
    };

    scrollRoot.addEventListener("scroll", handleScroll, { passive: true });

    for (const section of SETTINGS_SECTIONS) {
      const el = document.getElementById(section.id);
      if (el) observer.observe(el);
    }

    return () => {
      scrollRoot.removeEventListener("scroll", handleScroll);
      observer.disconnect();
      clearTimeout(timerRef.current);
    };
  }, []);

  const handleClick = (id: string) => {
    const scrollRoot = document.getElementById("settings-scroll-container");
    const el = document.getElementById(id);
    if (el && scrollRoot) {
      const rootRect = scrollRoot.getBoundingClientRect();
      const sectionRect = el.getBoundingClientRect();
      const offset = 12;
      const targetTop = scrollRoot.scrollTop + (sectionRect.top - rootRect.top) - offset;
      scrollRoot.scrollTo({ top: Math.max(0, targetTop), behavior: "smooth" });
      clearTimeout(timerRef.current);
      pendingIdRef.current = id;
      setActiveId(id);
    }
  };

  return (
    <nav className="hidden lg:flex z-20 flex-col items-center gap-1.5 rounded-xl border border-border bg-card px-1.5 py-3">
      {SETTINGS_SECTIONS.map((section) => {
        const isActive = activeId === section.id;
        const Icon = section.icon;

        let nudgeClass = "";
        if (section.id === "settings-storage") nudgeClass = "translate-y-[1px]";
        if (section.id === "settings-about") nudgeClass = "translate-y-[1px] translate-x-[1px]";

        return (
          <button
            key={section.id}
            type="button"
            onClick={() => handleClick(section.id)}
            title={t(section.labelKey)}
            aria-label={t(section.labelKey)}
            aria-current={isActive ? "true" : undefined}
            className={`flex h-9 w-9 cursor-pointer items-center justify-center rounded-xl focus-ring ${
              isActive
                ? "bg-primary/15 text-primary"
                : "text-muted-foreground/70 hover:bg-muted/50 hover:text-foreground"
            }`}
          >
            <Icon className={`h-[18px] w-[18px] ${nudgeClass}`} strokeWidth={isActive ? 2.2 : 1.7} aria-hidden />
          </button>
        );
      })}
    </nav>
  );
}

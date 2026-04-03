import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import {
  Search,
  Package,
  Globe,
  Layers,
  FolderKanban,
  ShieldCheck,
  Settings,
  Download,
  RefreshCw,
  Command,
  Sparkles,
} from "lucide-react";
import type { NavPage } from "../../types";

interface CommandPaletteAction {
  id: string;
  label: string;
  icon: React.ReactNode;
  shortcut?: string;
  section: string;
  onSelect: () => void;
  keywords?: string[];
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
  onNavigate: (page: NavPage) => void;
  onImport?: () => void;
  onRefresh?: () => void;
}

export function CommandPalette({
  open,
  onClose,
  onNavigate,
  onImport,
  onRefresh,
}: CommandPaletteProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Build action list
  const actions: CommandPaletteAction[] = useMemo(() => {
    const navActions: CommandPaletteAction[] = [
      {
        id: "nav-skills",
        label: t("sidebar.skills"),
        icon: <Package className="w-4 h-4" />,
        shortcut: "⌘1",
        section: t("commandPalette.navigation"),
        onSelect: () => onNavigate("my-skills"),
        keywords: ["skills", "cards", "my skills", "技能"],
      },
      {
        id: "nav-marketplace",
        label: t("sidebar.market"),
        icon: <Globe className="w-4 h-4" />,
        shortcut: "⌘2",
        section: t("commandPalette.navigation"),
        onSelect: () => onNavigate("marketplace"),
        keywords: ["marketplace", "market", "browse", "discover", "市场"],
      },
      {
        id: "nav-decks",
        label: t("sidebar.groups"),
        icon: <Layers className="w-4 h-4" />,
        shortcut: "⌘3",
        section: t("commandPalette.navigation"),
        onSelect: () => onNavigate("skill-cards"),
        keywords: ["decks", "groups", "卡组"],
      },
      {
        id: "nav-projects",
        label: t("sidebar.projects"),
        icon: <FolderKanban className="w-4 h-4" />,
        shortcut: "⌘4",
        section: t("commandPalette.navigation"),
        onSelect: () => onNavigate("projects"),
        keywords: ["projects", "项目"],
      },
      {
        id: "nav-security",
        label: t("sidebar.security"),
        icon: <ShieldCheck className="w-4 h-4" />,
        shortcut: "⌘5",
        section: t("commandPalette.navigation"),
        onSelect: () => onNavigate("security-scan"),
        keywords: ["security", "scan", "安全"],
      },
      {
        id: "nav-settings",
        label: t("sidebar.settings"),
        icon: <Settings className="w-4 h-4" />,
        shortcut: "⌘,",
        section: t("commandPalette.navigation"),
        onSelect: () => onNavigate("settings"),
        keywords: ["settings", "preferences", "config", "设置"],
      },
    ];

    const actionItems: CommandPaletteAction[] = [];

    if (onImport) {
      actionItems.push({
        id: "action-import",
        label: t("common.import"),
        icon: <Download className="w-4 h-4" />,
        shortcut: "⌘I",
        section: t("commandPalette.actions"),
        onSelect: () => onImport(),
        keywords: ["import", "add", "导入"],
      });
    }

    if (onRefresh) {
      actionItems.push({
        id: "action-refresh",
        label: t("commandPalette.refresh"),
        icon: <RefreshCw className="w-4 h-4" />,
        shortcut: "⌘R",
        section: t("commandPalette.actions"),
        onSelect: () => onRefresh(),
        keywords: ["refresh", "reload", "刷新"],
      });
    }

    return [...navActions, ...actionItems];
  }, [t, onNavigate, onImport, onRefresh]);

  // Filter actions by query
  const filteredActions = useMemo(() => {
    if (!query.trim()) return actions;
    const q = query.toLowerCase();
    return actions.filter(
      (a) =>
        a.label.toLowerCase().includes(q) ||
        a.keywords?.some((kw) => kw.toLowerCase().includes(q))
    );
  }, [actions, query]);

  // Group by section
  const groupedActions = useMemo(() => {
    const groups: Record<string, CommandPaletteAction[]> = {};
    for (const action of filteredActions) {
      if (!groups[action.section]) groups[action.section] = [];
      groups[action.section].push(action);
    }
    return groups;
  }, [filteredActions]);

  // Focus input when opening
  useEffect(() => {
    if (open) {
      setQuery("");
      setActiveIndex(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  // Reset active index on query change
  useEffect(() => {
    setActiveIndex(0);
  }, [query]);

  // Scroll active item into view
  useEffect(() => {
    const el = listRef.current?.querySelector(`[data-index="${activeIndex}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [activeIndex]);

  const handleSelect = useCallback(
    (action: CommandPaletteAction) => {
      onClose();
      // Small delay so close animation starts before navigation
      requestAnimationFrame(() => action.onSelect());
    },
    [onClose]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      switch (e.key) {
        case "ArrowDown":
          e.preventDefault();
          setActiveIndex((prev) =>
            prev < filteredActions.length - 1 ? prev + 1 : 0
          );
          break;
        case "ArrowUp":
          e.preventDefault();
          setActiveIndex((prev) =>
            prev > 0 ? prev - 1 : filteredActions.length - 1
          );
          break;
        case "Enter":
          e.preventDefault();
          if (filteredActions[activeIndex]) {
            handleSelect(filteredActions[activeIndex]);
          }
          break;
        case "Escape":
          e.preventDefault();
          onClose();
          break;
      }
    },
    [filteredActions, activeIndex, handleSelect, onClose]
  );

  if (!open) return null;

  let flatIndex = 0;

  return (
    <AnimatePresence>
      {open && (
        <>
          {/* Backdrop */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.12 }}
            className="fixed inset-0 z-[200] bg-black/50 backdrop-blur-sm"
            onClick={onClose}
          />

          {/* Palette */}
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: -20 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: -10 }}
            transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            className="fixed left-1/2 top-[15%] -translate-x-1/2 w-full max-w-lg z-[201]"
          >
            <div
              className="overflow-hidden rounded-2xl border border-border/60 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5"
              onKeyDown={handleKeyDown}
            >
              {/* Search input */}
              <div className="flex items-center gap-3 px-4 py-3 border-b border-border/50">
                <Search className="w-4 h-4 text-muted-foreground shrink-0" />
                <input
                  ref={inputRef}
                  type="text"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("commandPalette.placeholder")}
                  className="flex-1 bg-transparent text-sm text-foreground placeholder:text-muted-foreground/60 outline-none"
                />
                <kbd className="hidden sm:inline-flex items-center gap-0.5 rounded-md border border-border/60 bg-muted/40 px-1.5 py-0.5 text-[10px] text-muted-foreground/80 font-mono">
                  ESC
                </kbd>
              </div>

              {/* Results */}
              <div
                ref={listRef}
                className="max-h-[320px] overflow-y-auto overscroll-contain py-2"
                role="listbox"
              >
                {filteredActions.length === 0 ? (
                  <div className="flex flex-col items-center py-8 text-center">
                    <Sparkles className="w-5 h-5 text-muted-foreground/50 mb-2" />
                    <p className="text-sm text-muted-foreground">
                      {t("common.noResults")}
                    </p>
                  </div>
                ) : (
                  Object.entries(groupedActions).map(([section, items]) => (
                    <div key={section}>
                      <div className="px-4 py-1.5 text-[10px] uppercase tracking-wider text-muted-foreground/60 font-medium">
                        {section}
                      </div>
                      {items.map((action) => {
                        const idx = flatIndex++;
                        const isActive = idx === activeIndex;
                        return (
                          <button
                            key={action.id}
                            data-index={idx}
                            role="option"
                            aria-selected={isActive}
                            onClick={() => handleSelect(action)}
                            onMouseEnter={() => setActiveIndex(idx)}
                            className={`w-full flex items-center gap-3 px-4 py-2 text-sm cursor-pointer transition-colors ${
                              isActive
                                ? "bg-primary/10 text-primary"
                                : "text-foreground/80 hover:bg-muted/40"
                            }`}
                          >
                            <span
                              className={`shrink-0 ${
                                isActive
                                  ? "text-primary"
                                  : "text-muted-foreground"
                              }`}
                            >
                              {action.icon}
                            </span>
                            <span className="flex-1 text-left truncate">
                              {action.label}
                            </span>
                            {action.shortcut && (
                              <kbd className="shrink-0 rounded border border-border/50 bg-muted/30 px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground/70">
                                {action.shortcut}
                              </kbd>
                            )}
                          </button>
                        );
                      })}
                    </div>
                  ))
                )}
              </div>

              {/* Footer hint */}
              <div className="flex items-center justify-between px-4 py-2 border-t border-border/40 text-[10px] text-muted-foreground/50">
                <div className="flex items-center gap-3">
                  <span className="flex items-center gap-1">
                    <kbd className="rounded border border-border/40 bg-muted/30 px-1 py-0.5 font-mono">↑</kbd>
                    <kbd className="rounded border border-border/40 bg-muted/30 px-1 py-0.5 font-mono">↓</kbd>
                    {t("commandPalette.navigate")}
                  </span>
                  <span className="flex items-center gap-1">
                    <kbd className="rounded border border-border/40 bg-muted/30 px-1 py-0.5 font-mono">↵</kbd>
                    {t("commandPalette.select")}
                  </span>
                </div>
                <div className="flex items-center gap-1">
                  <Command className="w-3 h-3" />
                  <span>K</span>
                </div>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Command, Loader2, Search, Sparkles, X } from "lucide-react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "../../lib/utils";

/** One selectable row in the Spotlight result list. */
export interface SpotlightSearchItem {
  id: string;
  title: string;
  subtitle?: string;
  meta?: string;
}

export interface SpotlightSearchProps {
  open: boolean;
  onClose: () => void;
  query: string;
  onQueryChange: (q: string) => void;
  items: SpotlightSearchItem[];
  onSelect: (id: string) => void;
  placeholder?: string;
  /** Optional AI search action shown next to the input (e.g. Marketplace). */
  onAiSearch?: () => void;
  aiSearching?: boolean;
  /** Max rows shown (default 40). */
  maxResults?: number;
}

function matchesItem(item: SpotlightSearchItem, q: string): boolean {
  if (!q) return true;
  const hay = `${item.title} ${item.subtitle ?? ""} ${item.meta ?? ""}`.toLowerCase();
  return hay.includes(q);
}

const SpotlightRow = memo(function SpotlightRow({
  item,
  index,
  active,
  onHover,
  onSelect,
}: {
  item: SpotlightSearchItem;
  index: number;
  active: boolean;
  onHover: (index: number) => void;
  onSelect: (id: string) => void;
}) {
  return (
    <button
      type="button"
      data-spotlight-index={index}
      role="option"
      aria-selected={active}
      onClick={() => onSelect(item.id)}
      onMouseEnter={() => onHover(index)}
      className={cn(
        // No transition — selection must feel instant while arrowing / hovering.
        "flex w-full cursor-pointer items-center gap-3 rounded-lg px-3 py-2 text-left",
        active ? "bg-foreground/[0.07] dark:bg-foreground/[0.12]" : "bg-transparent",
      )}
    >
      <span className="min-w-0 flex-1 overflow-hidden">
        <span className="block truncate text-sm font-medium text-foreground">{item.title}</span>
        {item.subtitle ? (
          <span className="mt-0.5 block truncate text-xs leading-4 text-muted-foreground">{item.subtitle}</span>
        ) : null}
      </span>
      {item.meta ? (
        <span
          className={cn(
            "max-w-[7.5rem] shrink-0 self-center truncate rounded-md px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground",
            active ? "bg-background/80 ring-1 ring-border/70" : "bg-muted",
          )}
        >
          {item.meta}
        </span>
      ) : null}
    </button>
  );
});

/**
 * Spotlight-style skill search overlay (⌘F / toolbar trigger).
 * Filters `items` client-side; selecting a row closes and reports the id.
 * Query is owned by the parent so the page grid stays in sync when closed.
 */
export function SpotlightSearch({
  open,
  onClose,
  query,
  onQueryChange,
  items,
  onSelect,
  placeholder,
  onAiSearch,
  aiSearching,
  maxResults = 40,
}: SpotlightSearchProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const activeIndexRef = useRef(0);
  const [activeIndex, setActiveIndex] = useState(0);
  /** Only scroll list into view for keyboard navigation, not mouse hover. */
  const keyboardNavRef = useRef(false);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    const matched = q ? items.filter((item) => matchesItem(item, q)) : items;
    return matched.slice(0, maxResults);
  }, [items, query, maxResults]);

  const filteredRef = useRef(filtered);
  filteredRef.current = filtered;

  const totalMatchHint = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items.length;
    return items.filter((item) => matchesItem(item, q)).length;
  }, [items, query]);

  const moveActive = useCallback((next: number) => {
    activeIndexRef.current = next;
    setActiveIndex(next);
  }, []);

  useEffect(() => {
    if (open) {
      moveActive(0);
      keyboardNavRef.current = false;
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open, moveActive]);

  useEffect(() => {
    moveActive(0);
    keyboardNavRef.current = false;
  }, [query, moveActive]);

  useEffect(() => {
    if (!keyboardNavRef.current) return;
    const el = listRef.current?.querySelector(`[data-spotlight-index="${activeIndex}"]`);
    if (el instanceof HTMLElement) {
      el.scrollIntoView({ block: "nearest" });
    }
    keyboardNavRef.current = false;
  }, [activeIndex]);

  const handleSelect = useCallback(
    (id: string) => {
      onClose();
      requestAnimationFrame(() => onSelect(id));
    },
    [onClose, onSelect],
  );

  const handleHover = useCallback(
    (index: number) => {
      if (activeIndexRef.current === index) return;
      keyboardNavRef.current = false;
      moveActive(index);
    },
    [moveActive],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const list = filteredRef.current;
      switch (e.key) {
        case "ArrowDown": {
          e.preventDefault();
          if (list.length === 0) return;
          keyboardNavRef.current = true;
          const prev = activeIndexRef.current;
          moveActive(prev < list.length - 1 ? prev + 1 : 0);
          break;
        }
        case "ArrowUp": {
          e.preventDefault();
          if (list.length === 0) return;
          keyboardNavRef.current = true;
          const prev = activeIndexRef.current;
          moveActive(prev > 0 ? prev - 1 : list.length - 1);
          break;
        }
        case "Enter": {
          e.preventDefault();
          const item = list[activeIndexRef.current];
          if (item) handleSelect(item.id);
          break;
        }
        case "Escape":
          e.preventDefault();
          onClose();
          break;
      }
    },
    [handleSelect, moveActive, onClose],
  );

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.12 }}
            className="fixed inset-0 z-[200] bg-black/50 backdrop-blur-sm"
            onClick={onClose}
            aria-hidden
          />

          <motion.div
            role="dialog"
            aria-modal="true"
            aria-label={placeholder ?? t("toolbar.spotlightTitle")}
            initial={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: -20 }}
            animate={prefersReducedMotion ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
            exit={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: -10 }}
            transition={{ duration: prefersReducedMotion ? 0.01 : 0.18, ease: [0.16, 1, 0.3, 1] }}
            className="fixed left-1/2 top-[12%] z-[201] w-full max-w-xl -translate-x-1/2 px-4"
          >
            <div
              className="overflow-hidden rounded-2xl border border-border/70 bg-card shadow-[0_24px_80px_-20px_rgba(0,0,0,0.55)] ring-1 ring-black/5 dark:ring-white/10"
              onKeyDown={handleKeyDown}
            >
              <div className="flex items-center gap-3 border-b border-border px-4 py-3.5">
                <Search className="h-4 w-4 shrink-0 text-foreground/70" aria-hidden />
                <input
                  ref={inputRef}
                  type="text"
                  value={query}
                  onChange={(e) => onQueryChange(e.target.value)}
                  placeholder={placeholder ?? t("toolbar.searchPlaceholder")}
                  className="min-w-0 flex-1 bg-transparent text-[15px] text-foreground outline-none placeholder:text-muted-foreground"
                  autoComplete="off"
                  spellCheck={false}
                />
                {query.trim() ? (
                  <button
                    type="button"
                    onClick={() => onQueryChange("")}
                    className="flex h-7 w-7 items-center justify-center rounded-lg border border-border bg-muted/60 text-foreground/70 hover:bg-muted hover:text-foreground focus-ring cursor-pointer"
                    title={t("common.clear", { defaultValue: "Clear" })}
                    aria-label={t("common.clear", { defaultValue: "Clear" })}
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                ) : null}
                {onAiSearch ? (
                  <button
                    type="button"
                    onClick={onAiSearch}
                    disabled={aiSearching || !query.trim()}
                    className={cn(
                      "flex h-7 items-center gap-1 rounded-lg border px-2.5 text-[11px] font-semibold cursor-pointer shrink-0 focus-ring",
                      aiSearching
                        ? "border-ai-border bg-ai-bg-hover text-ai-text-hover animate-pulse"
                        : query.trim()
                          ? "border-ai-border bg-ai-bg-hover/50 text-ai-text hover:bg-ai-bg-hover hover:text-ai-text-hover"
                          : "border-border bg-muted/40 text-muted-foreground cursor-not-allowed opacity-60",
                    )}
                    title={t("marketplace.aiSearch", { defaultValue: "AI Search" })}
                  >
                    {aiSearching ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Sparkles className="h-3.5 w-3.5" />
                    )}
                    <span className="hidden sm:inline">{t("marketplace.aiSearch", { defaultValue: "AI" })}</span>
                  </button>
                ) : null}
                <kbd className="hidden shrink-0 items-center rounded-md border border-border bg-muted px-2 py-1 font-mono text-[10px] font-medium text-foreground/70 sm:inline-flex">
                  ESC
                </kbd>
              </div>

              <div
                ref={listRef}
                className="max-h-[min(360px,50vh)] overflow-y-auto overscroll-contain p-1.5"
                role="listbox"
                aria-label={t("toolbar.spotlightResults")}
              >
                {filtered.length === 0 ? (
                  <div className="flex flex-col items-center py-10 text-center">
                    <Search className="mb-2 h-5 w-5 text-muted-foreground" />
                    <p className="text-sm text-muted-foreground">
                      {query.trim() ? t("common.noResults") : t("toolbar.spotlightEmpty")}
                    </p>
                  </div>
                ) : (
                  filtered.map((item, idx) => (
                    <SpotlightRow
                      key={item.id}
                      item={item}
                      index={idx}
                      active={idx === activeIndex}
                      onHover={handleHover}
                      onSelect={handleSelect}
                    />
                  ))
                )}
              </div>

              <div className="flex items-center justify-between border-t border-border bg-muted/30 px-4 py-2.5 text-[11px] text-foreground/65">
                <div className="flex items-center gap-3">
                  <span className="flex items-center gap-1.5">
                    <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded-md border border-border bg-background px-1 font-mono text-[10px] font-semibold text-foreground/80 shadow-sm">
                      ↑
                    </kbd>
                    <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded-md border border-border bg-background px-1 font-mono text-[10px] font-semibold text-foreground/80 shadow-sm">
                      ↓
                    </kbd>
                    <span className="font-medium">{t("commandPalette.navigate")}</span>
                  </span>
                  <span className="flex items-center gap-1.5">
                    <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded-md border border-border bg-background px-1 font-mono text-[10px] font-semibold text-foreground/80 shadow-sm">
                      ↵
                    </kbd>
                    <span className="font-medium">{t("commandPalette.select")}</span>
                  </span>
                </div>
                <div className="flex items-center gap-2.5 tabular-nums">
                  {query.trim() ? (
                    <span className="text-foreground/55">
                      {t("toolbar.spotlightMatchCount", {
                        shown: filtered.length,
                        total: totalMatchHint,
                      })}
                    </span>
                  ) : null}
                  <kbd className="inline-flex h-5 items-center gap-0.5 rounded-md border border-border bg-background px-1.5 font-mono text-[10px] font-semibold text-foreground/80 shadow-sm">
                    <Command className="h-3 w-3" />
                    <span>F</span>
                  </kbd>
                </div>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

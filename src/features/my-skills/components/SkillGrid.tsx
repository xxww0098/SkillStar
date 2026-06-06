import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { X } from "lucide-react";
import { type CSSProperties, type RefObject, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { MOTION_DURATION, MOTION_TRANSITION } from "../../../comm/motion";
import { EmptyState } from "../../../components/ui/EmptyState";
import { cn } from "../../../lib/utils";
import type { AgentProfile, RepoNewSkill, Skill, ViewMode } from "../../../types";
import { GhostSkillCard } from "./GhostSkillCard";
import { SkillCard } from "./SkillCard";

/**
 * Above this count we use virtualized row rendering
 * to keep the DOM small.
 */
const VIRTUALIZE_THRESHOLD = 500;

/**
 * Above this count we skip framer-motion layout animations
 * (AnimatePresence + layout="position") for performance.
 */
const ANIMATE_THRESHOLD = 100;

const CARD_ROW_HEIGHT_GRID = 160;
const CARD_ROW_HEIGHT_LIST = 56;
const GRID_GAP_PX = 16;
const OVERSCAN_ROWS = 4;

const fullItemVariants = {
  hidden: { opacity: 0, y: 6 },
  show: {
    opacity: 1,
    y: 0,
    transition: MOTION_TRANSITION.enter,
  },
  exit: {
    opacity: 0,
    y: -14,
    transition: MOTION_TRANSITION.fadeBase,
  },
};

const reducedItemVariants = {
  hidden: { opacity: 0 },
  show: { opacity: 1, transition: { duration: MOTION_DURATION.instant } },
  exit: { opacity: 0, transition: { duration: MOTION_DURATION.instant } },
};

interface SkillGridProps {
  skills: Skill[];
  viewMode: ViewMode;
  columnStrategy?: "breakpoint" | "auto-fit" | "auto-fill";
  minColumnWidth?: number;
  onSkillClick: (skill: Skill) => void;
  onInstall: (url: string, name: string) => void;
  onUpdate: (name: string) => void;
  onVisibleCountChange?: (visible: number, total: number) => void;
  scrollParentRef?: RefObject<HTMLElement | null>;
  emptyMessage?: string;
  emptyAction?: React.ReactNode;
  selectable?: boolean;
  selectedSkills?: Set<string>;
  onSelectSkill?: (name: string) => void;
  profiles?: AgentProfile[];
  onToggleAgent?: (skillName: string, agentId: string, enable: boolean, agentName?: string) => void;
  installingNames?: Set<string>;
  pendingUpdateNames?: Set<string>;
  pendingAgentToggleKeys?: Set<string>;
  ghostSkills?: RepoNewSkill[];
  onInstallGhost?: (skill: RepoNewSkill) => void;
  onDismissGhost?: (repoSource: string, skillId: string) => void;
  onDismissGhostRepo?: (repoSource: string) => void;
  onGhostClick?: (skill: RepoNewSkill) => void;
}

function useScrollParent(
  ref: React.RefObject<HTMLElement | null>,
  explicitScrollParentRef?: RefObject<HTMLElement | null>,
): HTMLElement | null {
  const [parent, setParent] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    if (explicitScrollParentRef?.current) {
      setParent(explicitScrollParentRef.current);
      return;
    }

    let el = ref.current?.parentElement ?? null;
    while (el) {
      const style = getComputedStyle(el);
      if (/(auto|scroll)/.test(style.overflow + style.overflowY)) {
        setParent(el);
        return;
      }
      el = el.parentElement;
    }
    setParent(null);
  }, [ref, explicitScrollParentRef]);
  return parent;
}

function useVirtualRows(
  scrollParent: HTMLElement | null,
  containerRef: React.RefObject<HTMLElement | null>,
  rowCount: number,
  rowHeight: number,
  enabled: boolean,
) {
  const [range, setRange] = useState({ start: 0, end: 20 });

  useEffect(() => {
    if (!enabled || !scrollParent || rowCount === 0) return;

    const update = () => {
      const container = containerRef.current;
      if (!container) return;

      const containerTop = container.getBoundingClientRect().top;
      const scrollTop = scrollParent.getBoundingClientRect().top;
      const offset = scrollTop - containerTop;
      const viewportH = scrollParent.clientHeight;

      const firstVisible = Math.max(0, Math.floor(offset / rowHeight) - OVERSCAN_ROWS);
      const lastVisible = Math.min(rowCount - 1, Math.ceil((offset + viewportH) / rowHeight) + OVERSCAN_ROWS);
      setRange((prev) => {
        if (prev.start === firstVisible && prev.end === lastVisible) return prev;
        return { start: firstVisible, end: lastVisible };
      });
    };

    update();
    scrollParent.addEventListener("scroll", update, { passive: true });
    const ro = new ResizeObserver(update);
    ro.observe(scrollParent);
    return () => {
      scrollParent.removeEventListener("scroll", update);
      ro.disconnect();
    };
  }, [scrollParent, containerRef, rowCount, rowHeight, enabled]);

  return range;
}

export function SkillGrid({
  skills,
  viewMode,
  columnStrategy = "breakpoint",
  minColumnWidth = 320,
  onSkillClick,
  onInstall,
  onUpdate,
  onVisibleCountChange,
  scrollParentRef,
  emptyMessage,
  emptyAction,
  selectable,
  selectedSkills,
  onSelectSkill,
  profiles,
  onToggleAgent,
  installingNames,
  pendingUpdateNames,
  pendingAgentToggleKeys,
  ghostSkills,
  onInstallGhost,
  onDismissGhost,
  onDismissGhostRepo,
  onGhostClick,
}: SkillGridProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const itemVariants = prefersReducedMotion ? reducedItemVariants : fullItemVariants;

  const containerRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);

  const useLayoutAnimations = skills.length <= ANIMATE_THRESHOLD;
  const needsVirtualize = skills.length > VIRTUALIZE_THRESHOLD;

  const HYSTERESIS_PX = 8;
  const prevColCountRef = useRef(0);
  const gridColumnCount = useMemo(() => {
    if (viewMode !== "grid") return 1;
    if (containerWidth === 0) return prevColCountRef.current || 1;
    let cols: number;
    if (columnStrategy === "auto-fit" || columnStrategy === "auto-fill") {
      const safeMinWidth = Math.max(220, minColumnWidth);
      cols = Math.max(1, Math.floor((containerWidth + GRID_GAP_PX) / (safeMinWidth + GRID_GAP_PX)));
      if (prevColCountRef.current > 0 && cols < prevColCountRef.current) {
        const thresholdForPrev = prevColCountRef.current * (safeMinWidth + GRID_GAP_PX) - GRID_GAP_PX;
        if (containerWidth >= thresholdForPrev - HYSTERESIS_PX) {
          cols = prevColCountRef.current;
        }
      }
    } else {
      if (containerWidth >= 1280) cols = 3;
      else if (containerWidth >= 768) cols = 2;
      else cols = 1;
    }
    prevColCountRef.current = cols;
    return cols;
  }, [columnStrategy, containerWidth, minColumnWidth, viewMode]);

  const gridRendered = skills.length > 0;
  useLayoutEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const updateWidth = () => setContainerWidth(element.clientWidth);
    updateWidth();

    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, [gridRendered]);

  useLayoutEffect(() => {
    onVisibleCountChange?.(skills.length, skills.length);
  }, [onVisibleCountChange, skills.length]);

  const ghostGroups = useMemo(() => {
    if (!ghostSkills || ghostSkills.length === 0) return [];
    const groups: Map<string, RepoNewSkill[]> = new Map();
    for (const s of ghostSkills) {
      const list = groups.get(s.repo_source) ?? [];
      list.push(s);
      groups.set(s.repo_source, list);
    }
    return Array.from(groups.entries());
  }, [ghostSkills]);

  // ── Virtualization ──
  const rowCount = useMemo(() => Math.ceil(skills.length / gridColumnCount), [skills.length, gridColumnCount]);
  const cardHeight = viewMode === "grid" ? CARD_ROW_HEIGHT_GRID : CARD_ROW_HEIGHT_LIST;
  const stride = cardHeight + GRID_GAP_PX;
  const scrollParent = useScrollParent(containerRef, scrollParentRef);
  const { start: startRow, end: endRow } = useVirtualRows(
    scrollParent,
    containerRef,
    rowCount,
    stride,
    needsVirtualize,
  );

  if (skills.length === 0 && ghostGroups.length === 0) {
    return <EmptyState title={emptyMessage ?? t("skillGrid.noSkills")} action={emptyAction} size="lg" />;
  }

  const gridStyle: CSSProperties | undefined =
    viewMode === "grid" && gridColumnCount > 0
      ? { gridTemplateColumns: `repeat(${gridColumnCount}, minmax(0, 1fr))` }
      : undefined;

  const renderCard = (skill: Skill) => (
    <SkillCard
      skill={skill}
      onClick={onSkillClick}
      onInstall={onInstall}
      onUpdate={onUpdate}
      compact={viewMode === "list"}
      selectable={selectable}
      selected={selectedSkills?.has(skill.name)}
      onSelect={onSelectSkill}
      profiles={profiles}
      onToggleAgent={onToggleAgent}
      pendingAgentToggleKeys={pendingAgentToggleKeys}
      installing={installingNames?.has(skill.name)}
      updating={pendingUpdateNames?.has(skill.name)}
      noAnimate
    />
  );

  const ghostSection =
    ghostGroups.length > 0 && onInstallGhost && onDismissGhost ? (
      <div className="mb-4">
        <AnimatePresence>
          {ghostGroups.map(([repoSource, groupSkills]) => (
            <motion.div
              key={repoSource}
              initial={{ opacity: 0, y: -10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, height: 0 }}
              transition={{ duration: 0.25 }}
              className="mb-3"
            >
              <div className="flex items-center gap-2 px-1 mb-2">
                <div className="h-px flex-1 bg-primary/15" />
                <span className="text-[11px] font-medium text-primary/60 whitespace-nowrap">
                  {repoSource}{" "}
                  {t("ghostCard.foundCount", {
                    count: groupSkills.length,
                    defaultValue: `发现 ${groupSkills.length} 个新技能`,
                  })}
                </span>
                {onDismissGhostRepo && (
                  <button
                    onClick={() => onDismissGhostRepo(repoSource)}
                    className="p-0.5 rounded text-muted-foreground/40 hover:text-foreground hover:bg-muted/60 transition-all duration-150 cursor-pointer"
                    title={t("ghostCard.dismissAll", "全部忽略")}
                  >
                    <X className="w-3.5 h-3.5" />
                  </button>
                )}
                <div className="h-px flex-1 bg-primary/15" />
              </div>
              <div className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")} style={gridStyle}>
                <AnimatePresence>
                  {groupSkills.map((gs) => (
                    <GhostSkillCard
                      key={`${gs.repo_source}/${gs.skill_id}`}
                      skill={gs}
                      onInstall={onInstallGhost}
                      onDismiss={onDismissGhost}
                      onClick={onGhostClick}
                    />
                  ))}
                </AnimatePresence>
              </div>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    ) : null;

  // ── Virtualized: only render visible rows ──
  if (needsVirtualize) {
    const totalHeight = rowCount * stride - GRID_GAP_PX;
    const visibleRows: React.ReactNode[] = [];
    for (let rowIdx = startRow; rowIdx <= endRow && rowIdx < rowCount; rowIdx++) {
      const startIdx = rowIdx * gridColumnCount;
      const rowSkills = skills.slice(startIdx, startIdx + gridColumnCount);
      visibleRows.push(
        <div
          key={rowIdx}
          style={{
            position: "absolute",
            top: rowIdx * stride,
            left: 0,
            width: "100%",
            height: cardHeight,
          }}
        >
          <div
            className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")}
            style={{ ...gridStyle, height: "100%" }}
          >
            {rowSkills.map((skill) => (
              <div key={skill.name + skill.git_url} className="h-full">
                {renderCard(skill)}
              </div>
            ))}
          </div>
        </div>,
      );
    }

    return (
      <div ref={containerRef}>
        {ghostSection}
        <div style={{ position: "relative", height: totalHeight, width: "100%" }}>{visibleRows}</div>
      </div>
    );
  }

  // ── Large dataset without virtualization: plain CSS grid ──
  if (!useLayoutAnimations) {
    return (
      <div ref={containerRef}>
        {ghostSection}
        <div className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")} style={gridStyle}>
          {skills.map((skill) => (
            <div key={skill.name + skill.git_url} className="h-full">
              {renderCard(skill)}
            </div>
          ))}
        </div>
      </div>
    );
  }

  // ── Small dataset: full framer-motion layout animations ──
  return (
    <div ref={containerRef}>
      {ghostSection}
      <div className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")} style={gridStyle}>
        <AnimatePresence mode="popLayout">
          {skills.map((skill) => (
            <motion.div
              key={skill.name + skill.git_url}
              layout="position"
              initial="hidden"
              animate="show"
              variants={itemVariants}
              exit="exit"
              className="h-full"
            >
              {renderCard(skill)}
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}

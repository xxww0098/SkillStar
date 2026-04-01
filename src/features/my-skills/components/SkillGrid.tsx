import { useState, useEffect, useLayoutEffect, useRef, useMemo, useCallback, type CSSProperties } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { SkillCard } from "./SkillCard";
import { EmptyState } from "../../../components/ui/EmptyState";
import type { AgentProfile, RiskLevel, Skill, ViewMode } from "../../../types";
import { cn } from "../../../lib/utils";
import { MOTION_DURATION, MOTION_TRANSITION } from "../../../comm/motion";

/**
 * Above this count we use progressive loading (infinite scroll)
 * instead of rendering everything at once.
 */
const PROGRESSIVE_THRESHOLD = 60;

/** How many items to add per scroll-to-bottom batch. */
const PAGE_SIZE = 30;

/**
 * Above this count we skip framer-motion layout animations
 * (AnimatePresence + layout="position") for performance.
 */
const ANIMATE_THRESHOLD = 100;

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
  emptyMessage?: string;
  selectable?: boolean;
  selectedSkills?: Set<string>;
  onSelectSkill?: (name: string) => void;
  profiles?: AgentProfile[];
  onToggleAgent?: (
    skillName: string,
    agentId: string,
    enable: boolean,
    agentName?: string,
  ) => void;
  installingNames?: Set<string>;
  pendingUpdateNames?: Set<string>;
  pendingAgentToggleKeys?: Set<string>;
  riskMap?: Record<string, RiskLevel>;
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
  emptyMessage,
  selectable,
  selectedSkills,
  onSelectSkill,
  profiles,
  onToggleAgent,
  installingNames,
  pendingUpdateNames,
  pendingAgentToggleKeys,
  riskMap,
}: SkillGridProps) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const itemVariants = prefersReducedMotion ? reducedItemVariants : fullItemVariants;

  const containerRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);

  // ── Progressive loading state ──
  const needsProgressive = skills.length > PROGRESSIVE_THRESHOLD;
  const [visibleCount, setVisibleCount] = useState(PROGRESSIVE_THRESHOLD);

  // Reset visible count when data source changes (tab switch, search, etc.)
  const dataKeyRef = useRef(skills);
  if (dataKeyRef.current !== skills) {
    dataKeyRef.current = skills;
    // Reset inline so the very first render after data change shows the right slice
    if (needsProgressive && visibleCount !== PROGRESSIVE_THRESHOLD) {
      setVisibleCount(PROGRESSIVE_THRESHOLD);
    }
  }

  const displayedSkills = needsProgressive ? skills.slice(0, visibleCount) : skills;
  const hasMore = needsProgressive && visibleCount < skills.length;

  // Load more when sentinel enters viewport
  const loadMore = useCallback(() => {
    setVisibleCount((prev) => Math.min(prev + PAGE_SIZE, skills.length));
  }, [skills.length]);

  useEffect(() => {
    if (!hasMore) return;
    const sentinel = sentinelRef.current;
    if (!sentinel) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) {
          loadMore();
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, loadMore, displayedSkills.length]);

  const useLayoutAnimations = displayedSkills.length <= ANIMATE_THRESHOLD;
  const GRID_GAP_PX = 16;
  const HYSTERESIS_PX = 8;
  const prevColCountRef = useRef(0);
  const gridColumnCount = useMemo(() => {
    if (viewMode !== "grid") return 1;
    if (containerWidth === 0) return prevColCountRef.current || 1;
    let cols: number;
    if (columnStrategy === "auto-fit" || columnStrategy === "auto-fill") {
      const safeMinWidth = Math.max(220, minColumnWidth);
      cols = Math.max(
        1,
        Math.floor((containerWidth + GRID_GAP_PX) / (safeMinWidth + GRID_GAP_PX))
      );
      if (prevColCountRef.current > 0 && cols < prevColCountRef.current) {
        const thresholdForPrev =
          prevColCountRef.current * (safeMinWidth + GRID_GAP_PX) - GRID_GAP_PX;
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

  // Re-run when the grid div first appears in the DOM (skills goes from empty → non-empty)
  const gridRendered = skills.length > 0;
  // Synchronous measurement before first paint prevents the 0→N column flash
  useLayoutEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const updateWidth = () => setContainerWidth(element.clientWidth);
    updateWidth();

    const observer = new ResizeObserver(updateWidth);
    observer.observe(element);
    return () => observer.disconnect();
  }, [gridRendered]);

  // Report visible count
  useLayoutEffect(() => {
    onVisibleCountChange?.(displayedSkills.length, skills.length);
  }, [onVisibleCountChange, displayedSkills.length, skills.length]);

  if (skills.length === 0) {
    return <EmptyState title={emptyMessage ?? t("skillGrid.noSkills")} size="lg" />;
  }

  const gridStyle: CSSProperties | undefined =
    viewMode === "grid" && gridColumnCount > 0
      ? { gridTemplateColumns: `repeat(${gridColumnCount}, minmax(0, 1fr))` }
      : undefined;

  const renderCard = (skill: Skill) => (
    <SkillCard
      skill={skill}
      onClick={() => onSkillClick(skill)}
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
      riskLevel={riskMap?.[skill.name]}
      noAnimate
    />
  );

  // Sentinel element for infinite scroll trigger
  const sentinel = hasMore ? (
    <div ref={sentinelRef} className="h-px w-full" aria-hidden />
  ) : null;

  // ── Large dataset: plain CSS grid, no layout animations ──
  if (!useLayoutAnimations) {
    return (
      <div
        ref={containerRef}
        className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")}
        style={gridStyle}
      >
        {displayedSkills.map((skill) => (
          <div key={skill.name + skill.git_url} className="h-full">
            {renderCard(skill)}
          </div>
        ))}
        {sentinel}
      </div>
    );
  }

  // ── Small dataset: full framer-motion layout animations ──
  return (
    <div
      ref={containerRef}
      className={cn(viewMode === "grid" ? "ss-cards-grid" : "ss-cards-list")}
      style={gridStyle}
    >
      <AnimatePresence mode="popLayout">
        {displayedSkills.map((skill) => (
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
      {sentinel}
    </div>
  );
}

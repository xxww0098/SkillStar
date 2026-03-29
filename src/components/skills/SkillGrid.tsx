import { useState, useEffect, useRef, useCallback } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SkillCard } from "./SkillCard";
import { EmptyState } from "../ui/EmptyState";
import type { AgentProfile, Skill, ViewMode } from "../../types";
import { cn } from "../../lib/utils";

const PAGE_SIZE = 30;
const EAGER_RENDER_THRESHOLD = PAGE_SIZE * 2;

function getInitialVisibleCount(total: number): number {
  return total <= EAGER_RENDER_THRESHOLD ? total : PAGE_SIZE;
}

function findScrollRoot(el: HTMLElement | null): HTMLElement | null {
  let current = el?.parentElement ?? null;
  while (current) {
    const style = window.getComputedStyle(current);
    const overflowY = style.overflowY;
    if (overflowY === "auto" || overflowY === "scroll" || overflowY === "overlay") {
      return current;
    }
    current = current.parentElement;
  }
  return null;
}

const containerVariants = {
  hidden: {},
  show: {
    transition: {
      staggerChildren: 0.03,
    },
  },
};

const TRANSITION_EASE = [0.22, 1, 0.36, 1] as const;

const fullItemVariants = {
  hidden: { opacity: 0, y: 6, scale: 0.985 },
  show: {
    opacity: 1,
    y: 0,
    scale: 1,
    transition: { duration: 0.2, ease: TRANSITION_EASE },
  },
  exit: {
    opacity: 0,
    y: -14,
    scale: 0.96,
    transition: { duration: 0.22, ease: TRANSITION_EASE },
  },
};

const reducedItemVariants = {
  hidden: { opacity: 0 },
  show: { opacity: 1, transition: { duration: 0.01 } },
  exit: { opacity: 0, transition: { duration: 0.01 } },
};

interface SkillGridProps {
  skills: Skill[];
  viewMode: ViewMode;
  onSkillClick: (skill: Skill) => void;
  onInstall: (url: string, name: string) => void;
  onUpdate: (name: string) => void;
  onVisibleCountChange?: (visible: number, total: number) => void;
  emptyMessage?: string;
  selectable?: boolean;
  selectedSkills?: Set<string>;
  onSelectSkill?: (name: string) => void;
  profiles?: AgentProfile[];
  onToggleAgent?: (skillName: string, agentId: string, enable: boolean, agentName?: string) => void;
  /** Set of skill names currently being installed */
  installingNames?: Set<string>;
  /** Set of toggle keys currently in-flight: `${skillName}::${agentId}` */
  pendingAgentToggleKeys?: Set<string>;
}

export function SkillGrid({
  skills,
  viewMode,
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
  pendingAgentToggleKeys,
}: SkillGridProps) {
  const { t } = useTranslation();
  const [visibleCount, setVisibleCount] = useState(() => getInitialVisibleCount(skills.length));
  const sentinelRef = useRef<HTMLDivElement>(null);
  const prefersReducedMotion = useReducedMotion();
  const itemVariants = prefersReducedMotion ? reducedItemVariants : fullItemVariants;

  // Reset visible count when skills array changes (tab switch, search)
  useEffect(() => {
    setVisibleCount(getInitialVisibleCount(skills.length));
  }, [skills]);

  useEffect(() => {
    onVisibleCountChange?.(Math.min(visibleCount, skills.length), skills.length);
  }, [visibleCount, skills.length, onVisibleCountChange]);

  // IntersectionObserver for infinite scroll
  const loadMore = useCallback(() => {
    setVisibleCount((prev) => Math.min(prev + PAGE_SIZE, skills.length));
  }, [skills.length]);

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;

    const root = findScrollRoot(sentinel);
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          loadMore();
        }
      },
      { root, rootMargin: "200px" }
    );

    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [loadMore]);

  if (skills.length === 0) {
    return <EmptyState title={emptyMessage ?? t("skillGrid.noSkills")} size="lg" />;
  }

  const visibleSkills = skills.slice(0, visibleCount);
  const hasMore = visibleCount < skills.length;
  const initialBatch = visibleSkills.slice(0, PAGE_SIZE);
  const laterBatches = visibleSkills.slice(PAGE_SIZE);

  return (
    <>
      <motion.div
        variants={containerVariants}
        initial="hidden"
        animate="show"
        className={cn(
          viewMode === "grid"
            ? "grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4"
            : "flex flex-col gap-2"
        )}
      >
        <AnimatePresence mode="popLayout">
          {initialBatch.map((skill) => (
            <motion.div
              key={skill.name + skill.git_url}
              layout
              variants={itemVariants}
              exit="exit"
              className="h-full"
            >
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
                noAnimate
              />
            </motion.div>
          ))}
          {laterBatches.map((skill) => (
            <motion.div
              key={skill.name + skill.git_url}
              layout
              initial={false}
              animate="show"
              variants={itemVariants}
              exit="exit"
              className="h-full"
            >
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
                noAnimate
              />
            </motion.div>
          ))}
        </AnimatePresence>
      </motion.div>

      {/* Scroll sentinel + loading indicator */}
      <div ref={sentinelRef} className="w-full py-6 flex items-center justify-center">
        {hasMore && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className="flex items-center gap-2 text-muted-foreground text-xs"
          >
            <Loader2 className="w-3.5 h-3.5 animate-spin" />
            <span>
              {t("skillGrid.showing", { visible: visibleCount, total: skills.length })}
            </span>
          </motion.div>
        )}
      </div>
    </>
  );
}

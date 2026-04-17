import { motion } from "framer-motion";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { AgentIcon } from "../../models/components/shared/ProviderIcon";
import { cn } from "../../../lib/utils";
import type { AgentCliInfo } from "../hooks/useAgentClis";

const AGENT_HEX_COLORS: Record<string, string> = {
  claude: "#F97316",
  codex: "#10B981",
  opencode: "#A855F7",
  gemini: "#3B82F6",
};

interface AgentCoverflowProps {
  agents: AgentCliInfo[];
  value: string;
  onChange: (id: string) => void;
  /** Unique scope ID (e.g. pane id) to isolate animation state between instances. */
  instanceId?: string;
}

export function AgentCoverflow({ agents, value, onChange, instanceId }: AgentCoverflowProps) {
  const installedAgents = agents.filter((a) => a.installed);
  const activeIndex = installedAgents.findIndex((a) => a.id === value);
  const safeActiveIndex = activeIndex >= 0 ? activeIndex : 0;

  const handleNext = (e?: React.MouseEvent) => {
    e?.stopPropagation();
    if (safeActiveIndex < installedAgents.length - 1) {
      onChange(installedAgents[safeActiveIndex + 1].id);
    } else {
      onChange(installedAgents[0].id); // loop wrap
    }
  };

  const handlePrev = (e?: React.MouseEvent) => {
    e?.stopPropagation();
    if (safeActiveIndex > 0) {
      onChange(installedAgents[safeActiveIndex - 1].id);
    } else {
      onChange(installedAgents[installedAgents.length - 1].id); // loop wrap
    }
  };

  if (installedAgents.length === 0) return null;

  return (
    <div className="relative w-full max-w-[200px] h-16 flex items-center justify-center my-2 select-none group" style={{ perspective: "800px" }}>
      {installedAgents.map((agent, i) => {
        const offset = i - safeActiveIndex;
        const absOffset = Math.abs(offset);
        const zIndex = 10 - absOffset;
        
        let x = 0;
        let scale = 1;
        let opacity = 1;
        let rotateY = 0;

        if (offset === 0) {
          x = 0;
          scale = 1.1;
          opacity = 1;
          rotateY = 0;
        } else if (Math.abs(offset) === 1 || Math.abs(offset) === installedAgents.length - 1) {
          // Adjacent or wrapped adjacent
          const dir = offset === 1 || offset === -(installedAgents.length - 1) ? 1 : -1;
          x = dir * 45;
          scale = 0.75;
          opacity = 0.5;
          rotateY = dir * -30;
        } else {
          // Hidden
          const dir = offset > 0 ? 1 : -1;
          x = dir * 60;
          scale = 0.5;
          opacity = 0;
          rotateY = dir * -45;
        }

        const isActive = offset === 0;

        return (
          <motion.div
            key={instanceId ? `${instanceId}-${agent.id}` : agent.id}
            initial={false}
            animate={{ x, scale, opacity, rotateY }}
            transition={{ type: "spring", stiffness: 350, damping: 30 }}
            className={cn(
              "absolute top-0 bottom-0 m-auto w-12 h-12 rounded-xl flex items-center justify-center cursor-pointer border transition-colors",
              isActive ? "border-primary/50 bg-background/80 shadow-md backdrop-blur-md" : "border-border/30 bg-background/30"
            )}
            style={{ zIndex }}
            onClick={(e) => {
              e.stopPropagation();
              onChange(agent.id);
            }}
          >
            <AgentIcon appId={agent.id} color={AGENT_HEX_COLORS[agent.id] ?? "#94a3b8"} size={isActive ? "w-6 h-6" : "w-5 h-5"} />
          </motion.div>
        );
      })}

      {/* Navigation arrows overlay */}
      <button
        type="button"
        onClick={handlePrev}
        className="absolute left-[-16px] z-20 p-1 opacity-0 group-hover:opacity-100 hover:bg-muted rounded-full transition-all"
      >
        <ChevronLeft className="w-4 h-4 text-muted-foreground hover:text-foreground" />
      </button>

      <button
        type="button"
        onClick={handleNext}
        className="absolute right-[-16px] z-20 p-1 opacity-0 group-hover:opacity-100 hover:bg-muted rounded-full transition-all"
      >
        <ChevronRight className="w-4 h-4 text-muted-foreground hover:text-foreground" />
      </button>

      <div className="absolute -bottom-5 text-[10px] font-medium text-muted-foreground tracking-wide px-2 py-0.5 rounded-full bg-muted/40">
        {installedAgents[safeActiveIndex]?.name}
      </div>
    </div>
  );
}

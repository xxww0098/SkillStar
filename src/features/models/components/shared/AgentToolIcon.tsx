import { memo } from "react";
import {
  ClaudeColor,
  CodexColor,
  LobeHubMono,
  OpenCodeMono,
  PiMono,
  type LobeIconComponent,
} from "@/components/ui/icons/lobe";
import { LobeIcon } from "@/components/ui/icons/LobeIcon";
import { cn } from "@/lib/utils";
import type { ProviderToolId } from "../../lib/agentRegistry";

/** Same id space as `PROVIDER_AGENTS` — keep icons keyed by registry tool ids. */
export type AgentToolIconId = ProviderToolId;

export interface AgentToolIconProps {
  toolId: AgentToolIconId;
  /** Icon box size in px */
  size?: "sm" | "md";
  className?: string;
}

const SIZE_MAP = {
  sm: { box: "h-6 w-6", icon: 14 },
  md: { box: "h-7 w-7", icon: 18 },
} as const;

const CHIP = "flex h-full w-full items-center justify-center rounded-md border border-border/50 bg-background/70";

const GLYPH_BY_TOOL_ID: Record<AgentToolIconId, LobeIconComponent> = {
  "claude-code": ClaudeColor,
  "claude-desktop": ClaudeColor,
  codex: CodexColor,
  opencode: OpenCodeMono,
  pi: PiMono,
  omp: LobeHubMono,
};

function AgentToolIconInner({ toolId, size = "sm", className }: AgentToolIconProps) {
  const s = SIZE_MAP[size];

  return (
    <span className={cn("relative inline-flex shrink-0", s.box, className)} aria-hidden>
      <span className={CHIP}>
        <LobeIcon icon={GLYPH_BY_TOOL_ID[toolId]} size={s.icon} />
      </span>
    </span>
  );
}

export const AgentToolIcon = memo(AgentToolIconInner);

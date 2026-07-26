import { memo } from "react";
import { ClaudeColor, CodexColor, GeminiColor, type LobeIconComponent } from "@/components/ui/icons/lobe";
import { LobeIcon } from "@/components/ui/icons/LobeIcon";
import { cn } from "@/lib/utils";

export type AgentToolIconId = "claude-code" | "codex" | "opencode" | "gemini" | "pi";

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

const DEFAULT_CHIP = "border-border/50 bg-background/70";

type ToolGlyph = { icon: LobeIconComponent; chipClass: string } | { letter: string };

const GLYPH_BY_TOOL_ID: Record<AgentToolIconId, ToolGlyph> = {
  "claude-code": { icon: ClaudeColor, chipClass: DEFAULT_CHIP },
  codex: { icon: CodexColor, chipClass: DEFAULT_CHIP },
  gemini: { icon: GeminiColor, chipClass: DEFAULT_CHIP },
  opencode: { letter: "O" },
  pi: { letter: "P" },
};

function AgentToolIconInner({ toolId, size = "sm", className }: AgentToolIconProps) {
  const s = SIZE_MAP[size];
  const glyph = GLYPH_BY_TOOL_ID[toolId];

  return (
    <span className={cn("relative inline-flex shrink-0", s.box, className)} aria-hidden>
      {"letter" in glyph ? (
        <span className="flex h-full w-full items-center justify-center rounded-md bg-muted text-xs font-bold text-muted-foreground">
          {glyph.letter}
        </span>
      ) : (
        <span className={cn("flex h-full w-full items-center justify-center rounded-md border", glyph.chipClass)}>
          <LobeIcon icon={glyph.icon} size={s.icon} />
        </span>
      )}
    </span>
  );
}

export const AgentToolIcon = memo(AgentToolIconInner);

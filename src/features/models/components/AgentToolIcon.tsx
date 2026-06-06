import ClaudeIcon from "@lobehub/icons/es/Claude/components/Color";
import CodexIcon from "@lobehub/icons/es/Codex/components/Color";
import GeminiIcon from "@lobehub/icons/es/Gemini/components/Color";
import { memo, type ReactNode } from "react";
import { cn } from "../../../lib/utils";

export type AgentToolIconId = "claude-code" | "codex" | "opencode" | "claude-desktop" | "gemini";

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

function LetterIcon({ letter, active }: { letter: string; active?: boolean }) {
  return (
    <span
      className={cn(
        "flex h-full w-full items-center justify-center rounded-md text-xs font-bold",
        active ? "bg-primary/20 text-primary" : "bg-muted text-muted-foreground",
      )}
    >
      {letter}
    </span>
  );
}

function AgentToolIconInner({ toolId, size = "sm", className }: AgentToolIconProps) {
  const s = SIZE_MAP[size];
  let content: ReactNode;

  switch (toolId) {
    case "claude-code":
      content = (
        <span className="flex h-full w-full items-center justify-center rounded-md border border-border/50 bg-background/70">
          <ClaudeIcon size={s.icon} />
        </span>
      );
      break;
    case "codex":
      content = (
        <span className="flex h-full w-full items-center justify-center rounded-md border border-border/50 bg-background/70">
          <CodexIcon size={s.icon} />
        </span>
      );
      break;
    case "opencode":
      content = <LetterIcon letter="O" />;
      break;
    case "gemini":
      content = (
        <span className="flex h-full w-full items-center justify-center rounded-md border border-border/50 bg-background/70">
          <GeminiIcon size={s.icon} />
        </span>
      );
      break;
    case "claude-desktop":
      content = (
        <span className="flex h-full w-full items-center justify-center rounded-md border border-primary/25 bg-primary/[0.05]">
          <ClaudeIcon size={s.icon} />
        </span>
      );
      break;
  }

  return (
    <span className={cn("relative inline-flex shrink-0", s.box, className)} aria-hidden>
      {content}
    </span>
  );
}

export const AgentToolIcon = memo(AgentToolIconInner);

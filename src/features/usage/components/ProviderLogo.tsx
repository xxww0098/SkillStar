import AntigravityIcon from "@lobehub/icons/es/Antigravity/components/Color";
import CodexIcon from "@lobehub/icons/es/Codex/components/Color";
import CursorIcon from "@lobehub/icons/es/Cursor/components/Mono";
import DeepSeekIcon from "@lobehub/icons/es/DeepSeek/components/Color";
import GrokIcon from "@lobehub/icons/es/Grok/components/Mono";
import KimiIcon from "@lobehub/icons/es/Kimi/components/Color";
import MiniMaxIcon from "@lobehub/icons/es/Minimax/components/Color";
import OpenCodeIcon from "@lobehub/icons/es/OpenCode/components/Mono";
import QoderIcon from "@lobehub/icons/es/Qoder/components/Color";
import StepfunIcon from "@lobehub/icons/es/Stepfun/components/Color";
import TraeIcon from "@lobehub/icons/es/Trae/components/Color";
import ZhipuIcon from "@lobehub/icons/es/Zhipu/components/Color";
import { type ComponentType, type CSSProperties, useLayoutEffect, useRef } from "react";
import { cn } from "@/lib/utils";

type IconComponent = ComponentType<{ size?: number | string; className?: string; style?: CSSProperties }>;

interface ProviderLogoProps {
  catalogId: string;
  displayName: string;
  brandColor: string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const ICON_BY_CATALOG_ID: Record<string, IconComponent> = {
  cursor: CursorIcon,
  codex: CodexIcon,
  antigravity: AntigravityIcon,
  trae: TraeIcon,
  qoder: QoderIcon,
  xai: GrokIcon,
  deepseek: DeepSeekIcon,
  glm: ZhipuIcon,
  kimi: KimiIcon,
  minimax: MiniMaxIcon,
  stepfun: StepfunIcon,
  opencode: OpenCodeIcon,
};

/** Whether a brand-authentic icon (not the letter fallback) exists for this id. */
export function hasBrandIcon(catalogId: string): boolean {
  return catalogId in ICON_BY_CATALOG_ID;
}

const SIZE_CLASS = {
  sm: "h-5 w-5",
  md: "h-7 w-7",
  lg: "h-10 w-10",
} as const;

const ICON_SIZE = {
  sm: 16,
  md: 20,
  lg: 28,
} as const;

const FALLBACK_TEXT = {
  sm: "text-[10px]",
  md: "text-xs",
  lg: "text-sm",
} as const;

function pickInitial(name: string, fallback: string): string {
  const trimmed = name.trim();
  if (trimmed.length === 0) return fallback.charAt(0).toUpperCase();
  for (const ch of trimmed) {
    if (/[A-Za-z0-9]/.test(ch)) return ch.toUpperCase();
  }
  return Array.from(trimmed)[0] ?? "?";
}

export function ProviderLogo({ catalogId, displayName, brandColor, size = "md", className }: ProviderLogoProps) {
  const iconRef = useRef<HTMLSpanElement>(null);
  const Icon = ICON_BY_CATALOG_ID[catalogId];
  const bg = brandColor.startsWith("#") ? brandColor : `#${brandColor}`;

  useLayoutEffect(() => {
    iconRef.current?.querySelectorAll("title").forEach((title) => title.remove());
  });

  if (Icon) {
    return (
      <span
        ref={iconRef}
        className={cn("inline-flex shrink-0 items-center justify-center", SIZE_CLASS[size], className)}
        aria-hidden
      >
        <Icon size={ICON_SIZE[size]} />
      </span>
    );
  }

  const initial = pickInitial(displayName, catalogId);
  return (
    <div
      className={cn(
        "flex items-center justify-center rounded-md font-semibold text-white shrink-0",
        SIZE_CLASS[size],
        FALLBACK_TEXT[size],
        className,
      )}
      style={{ background: bg }}
      aria-hidden
    >
      {initial}
    </div>
  );
}

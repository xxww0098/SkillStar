import {
  AntigravityColor,
  ClaudeColor,
  CodexColor,
  CursorMono,
  DeepSeekColor,
  GrokMono,
  KimiMono,
  type LobeIconComponent,
  MinimaxColor,
  OllamaMono,
  ZhipuColor,
} from "@/components/ui/icons/lobe";
import { LobeIcon } from "@/components/ui/icons/LobeIcon";
import { cn } from "@/lib/utils";

interface ProviderLogoProps {
  catalogId: string;
  displayName: string;
  brandColor: string;
  size?: "sm" | "md" | "lg";
  className?: string;
}

/** Icons for the OAuth + API-key Usage catalog only. */
const ICON_BY_CATALOG_ID: Record<string, LobeIconComponent> = {
  // oauth
  cursor: CursorMono,
  codex: CodexColor,
  antigravity: AntigravityColor,
  xai: GrokMono,
  "grok-bot": GrokMono,
  anthropic: ClaudeColor,
  // api-key
  deepseek: DeepSeekColor,
  glm: ZhipuColor,
  kimi: KimiMono,
  minimax: MinimaxColor,
  ollama: OllamaMono,
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
  const Icon = ICON_BY_CATALOG_ID[catalogId];

  if (Icon) {
    return <LobeIcon icon={Icon} size={ICON_SIZE[size]} className={cn(SIZE_CLASS[size], className)} />;
  }

  const bg = brandColor.startsWith("#") ? brandColor : `#${brandColor}`;
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

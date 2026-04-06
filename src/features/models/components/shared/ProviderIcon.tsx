import { cn } from "../../../../lib/utils";

/**
 * Maps a provider/preset name to its SVG icon path under /providers/ or a known agent icon.
 * Returns undefined if no icon match is found.
 */
const PROVIDER_ICON_MAP: Record<string, string> = {
  // Provider icons
  deepseek: "/providers/deepseek.svg",
  "zhipu glm": "/providers/zhipu.svg",
  zhipu: "/providers/zhipu.svg",
  "zhipuai-coding-plan": "/providers/zhipu.svg",
  bailian: "/providers/bailian.svg",
  kimi: "/providers/kimi.svg",
  "kimi k2.5": "/providers/kimi.svg",
  minimax: "/providers/minimax.svg",
  "minimax-cn": "/providers/minimax.svg",
  doubaoseed: "/providers/doubao.svg",
  doubao: "/providers/doubao.svg",
  "xiaomi mimo": "/providers/xiaomimimo.svg",
  xiaomimimo: "/providers/xiaomimimo.svg",
  openrouter: "/providers/openrouter.svg",
  siliconflow: "/providers/siliconcloud.svg",

  // Agent icons used as providers
  "claude official": "/agents/claude.svg",
  claude: "/agents/claude.svg",
  anthropic: "/agents/claude.svg",
  "openai official": "/agents/codex.svg",
  "openai oauth": "/agents/codex.svg",
  "azure openai": "/agents/codex.svg",
  openai: "/agents/codex.svg",
  codex: "/agents/codex.svg",
};

/**
 * SVGs that use `fill="currentColor"` — they need CSS mask rendering
 * instead of plain <img> to show their brand color correctly.
 */
const CURRENT_COLOR_SVGS = new Set([
  "/providers/kimi.svg",
  "/providers/openrouter.svg",
  "/providers/xiaomimimo.svg",
  "/agents/claude.svg",
  "/agents/opencode.svg",
]);

/**
 * Agent app icons for the capsule switcher
 */
export const AGENT_ICON_MAP: Record<string, string> = {
  claude: "/agents/claude.svg",
  codex: "/agents/codex.svg",
  opencode: "/agents/opencode.svg",
};

/**
 * Look up a provider icon path by name (case-insensitive).
 */
export function getProviderIconPath(name: string): string | undefined {
  const key = name.toLowerCase().trim();
  return PROVIDER_ICON_MAP[key];
}

/**
 * Renders an SVG as a CSS-mask colored icon.
 * This allows currentColor-based SVGs to display with a specific color.
 */
function MaskedIcon({
  src,
  color,
  alt,
  size = "w-5 h-5",
  className,
}: {
  src: string;
  color: string;
  alt: string;
  size?: string;
  className?: string;
}) {
  return (
    <span
      role="img"
      aria-label={alt}
      className={cn(size, "inline-block shrink-0", className)}
      style={{
        backgroundColor: color,
        WebkitMaskImage: `url(${src})`,
        WebkitMaskSize: "contain",
        WebkitMaskRepeat: "no-repeat",
        WebkitMaskPosition: "center",
        maskImage: `url(${src})`,
        maskSize: "contain",
        maskRepeat: "no-repeat",
        maskPosition: "center",
      }}
    />
  );
}

interface ProviderIconProps {
  /** Provider display name — used to look up the icon */
  name: string;
  /** Fallback color for the dot if no icon is found */
  fallbackColor?: string;
  /** Size class, e.g. "w-5 h-5" */
  size?: string;
  className?: string;
}

/**
 * Renders an SVG provider icon from /providers or /agents assets.
 * For `currentColor` SVGs, uses CSS mask to render with the brand color.
 * Falls back to a colored dot when no icon match is found.
 */
export function ProviderIcon({ name, fallbackColor = "#888", size = "w-5 h-5", className }: ProviderIconProps) {
  const iconPath = getProviderIconPath(name);

  if (iconPath) {
    if (CURRENT_COLOR_SVGS.has(iconPath)) {
      return <MaskedIcon src={iconPath} color={fallbackColor} alt={name} size={size} className={className} />;
    }
    return <img src={iconPath} alt={name} className={cn(size, "object-contain", className)} draggable={false} />;
  }

  // Fallback: colored dot
  return (
    <span
      className={cn("rounded-full shrink-0", size === "w-5 h-5" ? "w-3 h-3" : "w-2.5 h-2.5", className)}
      style={{ backgroundColor: fallbackColor }}
    />
  );
}

interface AgentIconProps {
  appId: string;
  /** Brand color — used for currentColor SVGs like opencode */
  color?: string;
  size?: string;
  className?: string;
}

/**
 * Renders an agent app icon (Claude/Codex/OpenCode) from /agents assets.
 * For `currentColor` SVGs (like opencode.svg), uses CSS mask rendering to apply the given color.
 */
export function AgentIcon({ appId, color, size = "w-5 h-5", className }: AgentIconProps) {
  const iconPath = AGENT_ICON_MAP[appId];
  if (!iconPath) return null;

  if (CURRENT_COLOR_SVGS.has(iconPath) && color) {
    return <MaskedIcon src={iconPath} color={color} alt={appId} size={size} className={className} />;
  }

  return <img src={iconPath} alt={appId} className={cn(size, "object-contain", className)} draggable={false} />;
}

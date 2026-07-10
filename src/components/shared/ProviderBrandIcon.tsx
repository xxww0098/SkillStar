import {
  AnthropicMono,
  DeepSeekColor,
  GrokMono,
  KimiMono,
  type LobeIconComponent,
  LongCatColor,
  MinimaxColor,
  OpenAIMono,
  OpenRouterMono,
  SiliconCloudColor,
  XiaomiMiMoMono,
  ZhipuColor,
} from "@/components/ui/icons/lobe";
import { LobeIcon } from "@/components/ui/icons/LobeIcon";
import { cn } from "@/lib/utils";

interface ProviderBrandIconProps {
  presetId?: string | null;
  providerName?: string | null;
  iconColor?: string | null;
  size?: "xs" | "sm" | "md" | "lg";
  className?: string;
}

/** `tinted` glyphs are Mono variants painted with the provider's accent color. */
interface BrandGlyph {
  icon: LobeIconComponent;
  tinted?: boolean;
}

const GLYPH_BY_PRESET_ID: Record<string, BrandGlyph> = {
  deepseek: { icon: DeepSeekColor },
  kimi: { icon: KimiMono },
  "kimi-coding": { icon: KimiMono },
  minimax: { icon: MinimaxColor },
  longcat: { icon: LongCatColor },
  "xiaomi-mimo": { icon: XiaomiMiMoMono, tinted: true },
  glm: { icon: ZhipuColor },
  "glm-coding": { icon: ZhipuColor },
  openrouter: { icon: OpenRouterMono, tinted: true },
  siliconflow: { icon: SiliconCloudColor },
  grok: { icon: GrokMono, tinted: true },
  anthropic: { icon: AnthropicMono, tinted: true },
  "openai-compatible": { icon: OpenAIMono, tinted: true },
  official: { icon: OpenAIMono, tinted: true },
};

const BOX_CLASS = "border border-border/55 bg-background/75 shadow-sm";

const SIZE_CLASS = {
  xs: "h-5 w-5 rounded-lg",
  sm: "h-7 w-7 rounded-xl",
  md: "h-9 w-9 rounded-2xl",
  lg: "h-12 w-12 rounded-2xl",
} as const;

const ICON_SIZE = {
  xs: 14,
  sm: 18,
  md: 22,
  lg: 28,
} as const;

function normalize(value: string): string {
  return value.toLowerCase().replace(/\s+/g, "-");
}

function resolvePresetId(presetId?: string | null, providerName?: string | null): string | null {
  if (!providerName) return presetId ?? null;

  const name = normalize(providerName);
  if (presetId === "official") {
    return name.includes("anthropic") || name.includes("claude") ? "anthropic" : "official";
  }
  if (presetId) return presetId;

  if (name.includes("deepseek")) return "deepseek";
  if (name.includes("kimi") || name.includes("moonshot")) return "kimi";
  if (name.includes("minimax")) return "minimax";
  if (name.includes("longcat")) return "longcat";
  if (name.includes("mimo") || name.includes("xiaomi") || providerName.includes("小米")) return "xiaomi-mimo";
  if (name.includes("glm") || providerName.includes("智谱")) return "glm";
  if (name.includes("openrouter")) return "openrouter";
  if (name.includes("siliconflow") || providerName.includes("硅基")) return "siliconflow";
  if (name.includes("anthropic") || name.includes("claude")) return "anthropic";
  if (name.includes("openai")) return "openai-compatible";
  if (name.includes("grok") || name.includes("x.ai") || name.includes("xai")) return "grok";

  return null;
}

export function ProviderBrandIcon({
  presetId,
  providerName,
  iconColor,
  size = "sm",
  className,
}: ProviderBrandIconProps) {
  const resolvedPresetId = resolvePresetId(presetId, providerName);
  const glyph = resolvedPresetId ? GLYPH_BY_PRESET_ID[resolvedPresetId] : undefined;
  const fallbackColor = iconColor ?? "rgb(var(--color-primary-rgb))";

  if (glyph) {
    return (
      <LobeIcon
        icon={glyph.icon}
        size={ICON_SIZE[size]}
        className={cn(BOX_CLASS, SIZE_CLASS[size], className)}
        style={glyph.tinted ? { color: fallbackColor } : undefined}
      />
    );
  }

  return (
    <span
      className={cn("inline-flex shrink-0 items-center justify-center", BOX_CLASS, SIZE_CLASS[size], className)}
      aria-hidden
    >
      <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: fallbackColor }} />
    </span>
  );
}

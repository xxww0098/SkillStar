import AnthropicIcon from "@lobehub/icons/es/Anthropic/components/Mono";
import DeepSeekIcon from "@lobehub/icons/es/DeepSeek/components/Color";
import GrokIcon from "@lobehub/icons/es/Grok/components/Mono";
import KimiIcon from "@lobehub/icons/es/Kimi/components/Mono";
import MiniMaxIcon from "@lobehub/icons/es/Minimax/components/Color";
import OpenAIIcon from "@lobehub/icons/es/OpenAI/components/Mono";
import OpenRouterIcon from "@lobehub/icons/es/OpenRouter/components/Mono";
import QwenIcon from "@lobehub/icons/es/Qwen/components/Color";
import SiliconCloudIcon from "@lobehub/icons/es/SiliconCloud/components/Color";
import VolcengineIcon from "@lobehub/icons/es/Volcengine/components/Color";
import XiaomiMiMoIcon from "@lobehub/icons/es/XiaomiMiMo/components/Mono";
import ZhipuIcon from "@lobehub/icons/es/Zhipu/components/Color";
import { type ComponentType, type CSSProperties, useLayoutEffect, useRef } from "react";
import { cn } from "../../../lib/utils";

type IconComponent = ComponentType<{ size?: number | string; className?: string; style?: CSSProperties }>;

interface ProviderBrandIconProps {
  presetId?: string | null;
  providerName?: string | null;
  iconColor?: string | null;
  size?: "xs" | "sm" | "md" | "lg";
  className?: string;
}

const ICON_BY_PRESET_ID: Record<string, IconComponent> = {
  deepseek: DeepSeekIcon,
  kimi: KimiIcon,
  "kimi-coding": KimiIcon,
  minimax: MiniMaxIcon,
  qwen: QwenIcon,
  "qwen-coding": QwenIcon,
  glm: ZhipuIcon,
  "glm-coding": ZhipuIcon,
  volcengine: VolcengineIcon,
  mimo: XiaomiMiMoIcon,
  openrouter: OpenRouterIcon,
  siliconflow: SiliconCloudIcon,
  grok: GrokIcon,
  anthropic: AnthropicIcon,
  "openai-compatible": OpenAIIcon,
  official: OpenAIIcon,
};

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
  if (name.includes("qwen") || providerName.includes("通义") || name.includes("dashscope")) return "qwen";
  if (name.includes("glm") || providerName.includes("智谱")) return "glm";
  if (name.includes("doubao") || providerName.includes("火山")) return "volcengine";
  if (name.includes("mimo") || providerName.includes("小米")) return "mimo";
  if (name.includes("openrouter")) return "openrouter";
  if (name.includes("siliconflow") || providerName.includes("硅基")) return "siliconflow";
  if (name.includes("anthropic") || name.includes("claude")) return "anthropic";
  if (name.includes("openai")) return "openai-compatible";
  if (name.includes("grok") || name.includes("x.ai") || name.includes("xai")) return "grok";
  if (name.includes("alibaba") || providerName.includes("阿里")) return "qwen";

  return null;
}

export function ProviderBrandIcon({
  presetId,
  providerName,
  iconColor,
  size = "sm",
  className,
}: ProviderBrandIconProps) {
  const iconRef = useRef<HTMLSpanElement>(null);
  const resolvedPresetId = resolvePresetId(presetId, providerName);
  const Icon = resolvedPresetId ? ICON_BY_PRESET_ID[resolvedPresetId] : undefined;
  const fallbackColor = iconColor ?? "rgb(var(--color-primary-rgb))";
  const monoColor =
    resolvedPresetId === "openrouter" ||
    resolvedPresetId === "openai-compatible" ||
    resolvedPresetId === "official" ||
    resolvedPresetId === "anthropic" ||
    resolvedPresetId === "grok"
      ? fallbackColor
      : undefined;

  useLayoutEffect(() => {
    iconRef.current?.querySelectorAll("title").forEach((title) => title.remove());
  });

  return (
    <span
      ref={iconRef}
      className={cn(
        "inline-flex shrink-0 items-center justify-center border border-border/55 bg-background/75 shadow-sm",
        SIZE_CLASS[size],
        className,
      )}
      aria-hidden
    >
      {Icon ? (
        <Icon size={ICON_SIZE[size]} style={monoColor ? { color: monoColor } : undefined} />
      ) : (
        <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: fallbackColor }} />
      )}
    </span>
  );
}

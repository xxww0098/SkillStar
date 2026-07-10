import type { ComponentType, CSSProperties } from "react";

/**
 * Single import point for `@lobehub/icons`.
 *
 * The package's root barrel is huge, so we deep-import each brand glyph and
 * re-export it here with a `<Brand><Variant>` name. Everything else in the app
 * imports from this module (and renders through `LobeIcon`) — never from
 * `@lobehub/icons` directly — so the fragile deep paths and the Color/Mono
 * choice per brand live in exactly one place.
 *
 * `Mono` variants paint with `currentColor`; pass a `style={{ color }}` through
 * `LobeIcon` to tint them.
 */
export type LobeIconComponent = ComponentType<{
  size?: number | string;
  className?: string;
  style?: CSSProperties;
}>;

export { default as AnthropicMono } from "@lobehub/icons/es/Anthropic/components/Mono";
export { default as AntigravityColor } from "@lobehub/icons/es/Antigravity/components/Color";
export { default as ClaudeColor } from "@lobehub/icons/es/Claude/components/Color";
export { default as CodexColor } from "@lobehub/icons/es/Codex/components/Color";
export { default as CursorMono } from "@lobehub/icons/es/Cursor/components/Mono";
export { default as DeepSeekColor } from "@lobehub/icons/es/DeepSeek/components/Color";
export { default as GeminiColor } from "@lobehub/icons/es/Gemini/components/Color";
export { default as GrokMono } from "@lobehub/icons/es/Grok/components/Mono";
export { default as KimiMono } from "@lobehub/icons/es/Kimi/components/Mono";
export { default as LongCatColor } from "@lobehub/icons/es/LongCat/components/Color";
export { default as MinimaxColor } from "@lobehub/icons/es/Minimax/components/Color";
export { default as OpenAIMono } from "@lobehub/icons/es/OpenAI/components/Mono";
export { default as OpenCodeMono } from "@lobehub/icons/es/OpenCode/components/Mono";
export { default as OpenRouterMono } from "@lobehub/icons/es/OpenRouter/components/Mono";
export { default as QoderColor } from "@lobehub/icons/es/Qoder/components/Color";
export { default as SiliconCloudColor } from "@lobehub/icons/es/SiliconCloud/components/Color";
export { default as StepfunColor } from "@lobehub/icons/es/Stepfun/components/Color";
export { default as TraeColor } from "@lobehub/icons/es/Trae/components/Color";
export { default as XiaomiMiMoMono } from "@lobehub/icons/es/XiaomiMiMo/components/Mono";
export { default as ZhipuColor } from "@lobehub/icons/es/Zhipu/components/Color";

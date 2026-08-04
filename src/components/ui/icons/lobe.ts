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
export { default as AmpColor } from "@lobehub/icons/es/Amp/components/Color";
export { default as AntigravityColor } from "@lobehub/icons/es/Antigravity/components/Color";
export { default as ClaudeColor } from "@lobehub/icons/es/Claude/components/Color";
export { default as ClaudeCodeColor } from "@lobehub/icons/es/ClaudeCode/components/Color";
export { default as ClineMono } from "@lobehub/icons/es/Cline/components/Mono";
export { default as CodeBuddyColor } from "@lobehub/icons/es/CodeBuddy/components/Color";
export { default as CodexColor } from "@lobehub/icons/es/Codex/components/Color";
export { default as CursorMono } from "@lobehub/icons/es/Cursor/components/Mono";
export { default as DeepSeekColor } from "@lobehub/icons/es/DeepSeek/components/Color";
export { default as DevinColor } from "@lobehub/icons/es/Devin/components/Color";
export { default as GithubCopilotMono } from "@lobehub/icons/es/GithubCopilot/components/Mono";
export { default as GooseMono } from "@lobehub/icons/es/Goose/components/Mono";
export { default as GrokMono } from "@lobehub/icons/es/Grok/components/Mono";
export { default as HermesAgentMono } from "@lobehub/icons/es/HermesAgent/components/Mono";
export { default as IBMMono } from "@lobehub/icons/es/IBM/components/Mono";
export { default as InferenceMono } from "@lobehub/icons/es/Inference/components/Mono";
export { default as JunieColor } from "@lobehub/icons/es/Junie/components/Color";
export { default as KiloCodeMono } from "@lobehub/icons/es/KiloCode/components/Mono";
export { default as KimiMono } from "@lobehub/icons/es/Kimi/components/Mono";
export { default as KiroColor } from "@lobehub/icons/es/Kiro/components/Color";
export { default as LobeHubMono } from "@lobehub/icons/es/LobeHub/components/Mono";
export { default as LongCatColor } from "@lobehub/icons/es/LongCat/components/Color";
export { default as MinimaxColor } from "@lobehub/icons/es/Minimax/components/Color";
export { default as MCPMono } from "@lobehub/icons/es/MCP/components/Mono";
export { default as MistralColor } from "@lobehub/icons/es/Mistral/components/Color";
export { default as OpenAIMono } from "@lobehub/icons/es/OpenAI/components/Mono";
export { default as OpenCodeMono } from "@lobehub/icons/es/OpenCode/components/Mono";
export { default as OpenClawColor } from "@lobehub/icons/es/OpenClaw/components/Color";
export { default as OpenHandsColor } from "@lobehub/icons/es/OpenHands/components/Color";
export { default as OpenRouterMono } from "@lobehub/icons/es/OpenRouter/components/Mono";
// Pi has no Color variant upstream; only Mono ships.
export { default as PiMono } from "@lobehub/icons/es/Pi/components/Mono";
export { default as QoderColor } from "@lobehub/icons/es/Qoder/components/Color";
export { default as QwenColor } from "@lobehub/icons/es/Qwen/components/Color";
export { default as ReplitColor } from "@lobehub/icons/es/Replit/components/Color";
export { default as RooCodeMono } from "@lobehub/icons/es/RooCode/components/Mono";
export { default as SiliconCloudColor } from "@lobehub/icons/es/SiliconCloud/components/Color";
export { default as SnowflakeColor } from "@lobehub/icons/es/Snowflake/components/Color";
export { default as TraeColor } from "@lobehub/icons/es/Trae/components/Color";
export { default as WindsurfMono } from "@lobehub/icons/es/Windsurf/components/Mono";
export { default as XiaomiMiMoMono } from "@lobehub/icons/es/XiaomiMiMo/components/Mono";
export { default as ZhipuColor } from "@lobehub/icons/es/Zhipu/components/Color";
export { default as ZencoderColor } from "@lobehub/icons/es/Zencoder/components/Color";

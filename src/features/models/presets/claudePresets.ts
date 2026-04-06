/**
 * Claude Code provider presets — official & commonly used third-party providers.
 * Ported from cc-switch claudeProviderPresets.ts
 */

export interface ClaudePreset {
  name: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  env: Record<string, string>;
  category: "official" | "cn_official" | "cloud_provider" | "aggregator" | "third_party";
  icon?: string;
  iconColor?: string;
}

export const claudePresets: ClaudePreset[] = [
  {
    name: "Claude Official",
    websiteUrl: "https://www.anthropic.com/claude-code",
    apiKeyUrl: "https://console.anthropic.com/settings/keys",
    env: {},
    category: "official",
    iconColor: "#D97757",
  },
  {
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    env: {
      ANTHROPIC_BASE_URL: "https://api.deepseek.com/anthropic",
      ANTHROPIC_MODEL: "DeepSeek-V3.2",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "DeepSeek-V3.2",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "DeepSeek-V3.2",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "DeepSeek-V3.2",
    },
    category: "cn_official",
    iconColor: "#1E88E5",
  },
  {
    name: "Zhipu GLM",
    websiteUrl: "https://open.bigmodel.cn",
    apiKeyUrl: "https://www.bigmodel.cn/claude-code",
    env: {
      ANTHROPIC_BASE_URL: "https://open.bigmodel.cn/api/anthropic",
      ANTHROPIC_MODEL: "glm-5.1",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "glm-5.1",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "glm-5.1",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "glm-5.1",
    },
    category: "cn_official",
    iconColor: "#0F62FE",
  },
  {
    name: "Bailian",
    websiteUrl: "https://bailian.console.aliyun.com",
    apiKeyUrl: "https://bailian.console.aliyun.com/#/api-key",
    env: {
      ANTHROPIC_BASE_URL: "https://dashscope.aliyuncs.com/apps/anthropic",
    },
    category: "cn_official",
    iconColor: "#624AFF",
  },
  {
    name: "Kimi",
    websiteUrl: "https://platform.moonshot.cn/console",
    apiKeyUrl: "https://platform.moonshot.cn/console/api-keys",
    env: {
      ANTHROPIC_BASE_URL: "https://api.moonshot.cn/anthropic",
      ANTHROPIC_MODEL: "kimi-k2.5",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "kimi-k2.5",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "kimi-k2.5",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "kimi-k2.5",
    },
    category: "cn_official",
    iconColor: "#6366F1",
  },

  {
    name: "MiniMax",
    websiteUrl: "https://platform.minimaxi.com",
    apiKeyUrl: "https://platform.minimaxi.com/user-center/basic-information/interface-key",
    env: {
      ANTHROPIC_BASE_URL: "https://api.minimaxi.com/anthropic",
      ANTHROPIC_MODEL: "MiniMax-M2.7",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "MiniMax-M2.7",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "MiniMax-M2.7",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "MiniMax-M2.7",
    },
    category: "cn_official",
    iconColor: "#FF6B6B",
  },
  {
    name: "DouBaoSeed",
    websiteUrl: "https://www.volcengine.com/product/doubao",
    apiKeyUrl: "https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey",
    env: {
      ANTHROPIC_BASE_URL: "https://ark.cn-beijing.volces.com/api/coding",
      ANTHROPIC_MODEL: "doubao-seed-2-0-code-preview-latest",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "doubao-seed-2-0-code-preview-latest",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "doubao-seed-2-0-code-preview-latest",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "doubao-seed-2-0-code-preview-latest",
    },
    category: "cn_official",
    iconColor: "#3370FF",
  },
  {
    name: "Xiaomi MiMo",
    websiteUrl: "https://platform.xiaomimimo.com",
    env: {
      ANTHROPIC_BASE_URL: "https://api.xiaomimimo.com/anthropic",
      ANTHROPIC_MODEL: "mimo-v2-pro",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "mimo-v2-pro",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "mimo-v2-pro",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "mimo-v2-pro",
    },
    category: "cn_official",
    iconColor: "#000000",
  },
  {
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    env: {
      ANTHROPIC_BASE_URL: "https://openrouter.ai/api",
      ANTHROPIC_MODEL: "anthropic/claude-sonnet-4.6",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "anthropic/claude-haiku-4.5",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "anthropic/claude-sonnet-4.6",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "anthropic/claude-opus-4.6",
    },
    category: "aggregator",
    iconColor: "#6566F1",
  },
  {
    name: "SiliconFlow",
    websiteUrl: "https://siliconflow.cn",
    apiKeyUrl: "https://cloud.siliconflow.cn/account/ak",
    env: {
      ANTHROPIC_BASE_URL: "https://api.siliconflow.cn",
      ANTHROPIC_MODEL: "Pro/MiniMaxAI/MiniMax-M2.7",
      ANTHROPIC_DEFAULT_HAIKU_MODEL: "Pro/MiniMaxAI/MiniMax-M2.7",
      ANTHROPIC_DEFAULT_SONNET_MODEL: "Pro/MiniMaxAI/MiniMax-M2.7",
      ANTHROPIC_DEFAULT_OPUS_MODEL: "Pro/MiniMaxAI/MiniMax-M2.7",
    },
    category: "aggregator",
    iconColor: "#6E29F6",
  },
];

/**
 * OpenCode provider presets — official & commonly used providers.
 * Ported from cc-switch opencodeProviderPresets.ts
 */

export interface OpenCodePreset {
  name: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  settingsConfig: {
    npm: string;
    name?: string;
    options: Record<string, unknown>;
    models: Record<string, { name: string }>;
  };
  category: "official" | "cn_official" | "aggregator" | "third_party";
  iconColor?: string;
}

export const opencodeNpmPackages = [
  { value: "@ai-sdk/openai", label: "OpenAI Responses" },
  { value: "@ai-sdk/openai-compatible", label: "OpenAI Compatible" },
  { value: "@ai-sdk/anthropic", label: "Anthropic" },
  { value: "@ai-sdk/amazon-bedrock", label: "Amazon Bedrock" },
  { value: "@ai-sdk/google", label: "Google (Gemini)" },
] as const;

export const opencodePresets: OpenCodePreset[] = [
  {
    name: "DeepSeek",
    websiteUrl: "https://platform.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com/api_keys",
    settingsConfig: {
      npm: "@ai-sdk/openai-compatible",
      options: { baseURL: "https://api.deepseek.com/v1", apiKey: "", setCacheKey: true },
      models: {
        "deepseek-chat": { name: "DeepSeek V3.2" },
        "deepseek-reasoner": { name: "DeepSeek R1" },
      },
    },
    category: "cn_official",
    iconColor: "#1E88E5",
  },
  {
    name: "Zhipu GLM",
    websiteUrl: "https://open.bigmodel.cn",
    settingsConfig: {
      npm: "@ai-sdk/openai-compatible",
      name: "Zhipu GLM",
      options: { baseURL: "https://open.bigmodel.cn/api/paas/v4", apiKey: "", setCacheKey: true },
      models: { "glm-5.1": { name: "GLM-5.1" } },
    },
    category: "cn_official",
    iconColor: "#0F62FE",
  },
  {
    name: "Kimi K2.5",
    websiteUrl: "https://platform.moonshot.cn/console",
    settingsConfig: {
      npm: "@ai-sdk/openai-compatible",
      name: "Kimi k2.5",
      options: { baseURL: "https://api.moonshot.cn/v1", apiKey: "", setCacheKey: true },
      models: { "kimi-k2.5": { name: "Kimi K2.5" } },
    },
    category: "cn_official",
    iconColor: "#6366F1",
  },
  {
    name: "Bailian",
    websiteUrl: "https://bailian.console.aliyun.com",
    settingsConfig: {
      npm: "@ai-sdk/openai-compatible",
      name: "Bailian",
      options: { baseURL: "https://dashscope.aliyuncs.com/compatible-mode/v1", apiKey: "", setCacheKey: true },
      models: {},
    },
    category: "cn_official",
    iconColor: "#624AFF",
  },
  {
    name: "MiniMax",
    websiteUrl: "https://platform.minimaxi.com",
    settingsConfig: {
      npm: "@ai-sdk/openai-compatible",
      name: "MiniMax",
      options: { baseURL: "https://api.minimaxi.com/v1", apiKey: "", setCacheKey: true },
      models: {},
    },
    category: "cn_official",
    iconColor: "#FF6B6B",
  },

  {
    name: "DouBaoSeed",
    websiteUrl: "https://www.volcengine.com/product/doubao",
    settingsConfig: {
      npm: "@ai-sdk/openai-compatible",
      name: "DouBaoSeed",
      options: { baseURL: "https://ark.cn-beijing.volces.com/api/v3", apiKey: "", setCacheKey: true },
      models: { "doubao-seed-2-0-code-preview-latest": { name: "Doubao Seed Code Preview" } },
    },
    category: "cn_official",
    iconColor: "#3370FF",
  },
  {
    name: "Xiaomi MiMo",
    websiteUrl: "https://platform.xiaomimimo.com",
    settingsConfig: {
      npm: "@ai-sdk/openai-compatible",
      name: "Xiaomi MiMo",
      options: { baseURL: "https://api.xiaomimimo.com/v1", apiKey: "", setCacheKey: true },
      models: { "mimo-v2-pro": { name: "MiMo V2 Pro" } },
    },
    category: "cn_official",
    iconColor: "#000000",
  },
];

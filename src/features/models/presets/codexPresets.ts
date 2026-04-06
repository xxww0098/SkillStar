/**
 * Codex provider presets — official & commonly used providers.
 * Ported from cc-switch codexProviderPresets.ts
 */

export interface CodexPreset {
  name: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  config: string; // TOML string
  category: "official" | "cn_official" | "aggregator" | "third_party";
  iconColor?: string;
}

function generateThirdPartyConfig(providerName: string, baseUrl: string, modelName = "gpt-5.4"): string {
  const clean =
    providerName
      .toLowerCase()
      .replace(/[^a-z0-9_]/g, "_")
      .replace(/^_+|_+$/g, "") || "custom";
  return `model_provider = "${clean}"
model = "${modelName}"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.${clean}]
name = "${clean}"
base_url = "${baseUrl}"
wire_api = "responses"
requires_openai_auth = true`;
}

export const codexPresets: CodexPreset[] = [
  {
    name: "OpenAI Official",
    websiteUrl: "https://chatgpt.com/codex",
    apiKeyUrl: "https://platform.openai.com/api-keys",
    config: 'model = "gpt-5.4"\nmodel_reasoning_effort = "high"\ndisable_response_storage = true',
    category: "official",
    iconColor: "#00A67E",
  },

  {
    name: "OpenRouter",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai/keys",
    config: generateThirdPartyConfig("openrouter", "https://openrouter.ai/api/v1"),
    category: "aggregator",
    iconColor: "#6566F1",
  },
];

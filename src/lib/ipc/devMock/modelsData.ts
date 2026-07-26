/**
 * Dev-mock sample data: models — the flat provider store (mutated in place by
 * the stateful models fragment) and the provider preset catalog. Consumed by
 * ./models.ts.
 */

export const FLAT_PROVIDERS = {
  version: 2,
  providers: [
    {
      id: "p-deepseek",
      name: "DeepSeek",
      base_url_openai: "https://api.deepseek.com/v1",
      base_url_anthropic: "https://api.deepseek.com/anthropic",
      models_url: "https://api.deepseek.com/v1/models",
      api_key: "sk-demo-deepseek",
      models: ["deepseek-chat", "deepseek-reasoner"],
      default_model: "deepseek-chat",
      sort_index: 0,
      preset_id: "deepseek",
      icon_color: "#4D6BFE",
      codex_wire_api: "responses",
      codex_auth_mode: "api_key",
    },
    {
      id: "p-kimi",
      name: "Kimi",
      base_url_openai: "https://api.moonshot.cn/v1",
      base_url_anthropic: "https://api.moonshot.cn/anthropic",
      models_url: "https://api.moonshot.cn/v1/models",
      api_key: "sk-demo-kimi",
      models: ["kimi-k2", "moonshot-v1-128k"],
      default_model: "kimi-k2",
      sort_index: 1,
      preset_id: "kimi",
      icon_color: "#5B45E0",
      codex_wire_api: "responses",
      codex_auth_mode: "api_key",
    },
  ],
  tool_activations: {
    "claude-code": {
      entries: [
        {
          provider_id: "p-deepseek",
          model: "deepseek-chat",
          settings: null,
          last_sync_at: Math.floor(Date.now() / 1000) - 3600,
        },
      ],
      active_index: 0,
    },
    codex: {
      entries: [
        {
          provider_id: "p-kimi",
          model: "kimi-k2",
          settings: { wire_api: "responses", auth_mode: "api_key" },
          last_sync_at: Math.floor(Date.now() / 1000) - 7200,
        },
      ],
      active_index: 0,
    },
  } as Record<string, unknown>,
};

export const PRESETS_FLAT = [
  {
    id: "deepseek",
    name: "DeepSeek",
    category: "domestic",
    base_url_openai: "https://api.deepseek.com/v1",
    base_url_anthropic: "https://api.deepseek.com/anthropic",
    models_url: "https://api.deepseek.com/v1/models",
    models: [],
    icon_color: "#4D6BFE",
    api_key_url: "https://platform.deepseek.com/api_keys",
    balance_endpoint: "https://api.deepseek.com/user/balance",
    balance_parser: "deepseek",
  },
  {
    id: "kimi",
    name: "Kimi",
    category: "domestic",
    base_url_openai: "https://api.moonshot.cn/v1",
    base_url_anthropic: "https://api.moonshot.cn/anthropic",
    models_url: "https://api.moonshot.cn/v1/models",
    models: [],
    icon_color: "#5B45E0",
    api_key_url: "https://platform.moonshot.cn/console/api-keys",
    balance_endpoint: "https://api.moonshot.cn/v1/users/me/balance",
    balance_parser: "kimi",
  },
  {
    id: "glm",
    name: "智谱 GLM",
    category: "domestic",
    base_url_openai: "https://open.bigmodel.cn/api/paas/v4",
    base_url_anthropic: "https://open.bigmodel.cn/api/anthropic",
    models_url: "https://open.bigmodel.cn/api/paas/v4/models",
    models: [],
    icon_color: "#3366FF",
    api_key_url: "https://open.bigmodel.cn/usercenter/apikeys",
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    category: "relay",
    base_url_openai: "https://openrouter.ai/api/v1",
    base_url_anthropic: "",
    models_url: "https://openrouter.ai/api/v1/models",
    models: [],
    icon_color: "#6366F1",
    api_key_url: "https://openrouter.ai/keys",
    balance_endpoint: "https://openrouter.ai/api/v1/credits",
    balance_parser: "openrouter",
  },
];

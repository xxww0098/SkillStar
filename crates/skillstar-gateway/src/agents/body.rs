//! Per-agent file bodies. Field names follow the magpie writer for that agent.
//! The model list is only the `model_ref` passed in.

use std::path::{Path, PathBuf};

use crate::codex::ApplyError;
use crate::PLACEHOLDER_BEARER;

use super::{Written, override_dir};

pub(super) fn files(
    agent_id: &str,
    home: &Path,
    origin: &str,
    model_ref: &str,
) -> Result<Vec<Written>, ApplyError> {
    let v1 = format!("{origin}/v1");
    let full = format!("{PLACEHOLDER_BEARER}/{model_ref}");
    let token = PLACEHOLDER_BEARER;
    match agent_id {
        "gemini" => Ok(gemini(home, origin, &full)),
        "opencode" => Ok(opencode_like(home, "opencode", &v1, model_ref, &full, token)),
        "mimocode" => Ok(opencode_like(home, "mimocode", &v1, model_ref, &full, token)),
        "pi" => Ok(pi(home, &v1, model_ref, token)),
        "crush" => Ok(crush(home, &v1, model_ref, token)),
        "dsh" => Ok(dsh(home, &v1, model_ref, token)),
        "commandcode" => Ok(commandcode(home, &v1, model_ref, &full)),
        "fx" => Ok(fx(home, &v1, model_ref)),
        "omp" => Ok(omp(home, &v1, &full, model_ref)),
        "hermes" => Ok(hermes(home, &v1, model_ref, token)),
        "cline" => Ok(cline(home, &v1, model_ref, token)),
        "qoder" => Ok(qoder(home, "qoder", ".qoder", "QODER_CONFIG_DIR", &v1, model_ref, &full)),
        "qoder-cn" => Ok(qoder(
            home,
            "qoder-cn",
            ".qoder-cn",
            "QODERCN_CONFIG_DIR",
            &v1,
            model_ref,
            &full,
        )),
        "grok" => Ok(grok(home, &v1, model_ref, &full, token)),
        "zcode" => Ok(zcode(home, origin, model_ref, token)),
        "workbuddy" => Ok(workbuddy(home, &v1, model_ref)),
        "claude" => Ok(claude_code(home, origin, model_ref)),
        _ => Err(ApplyError::NotManaged),
    }
}

fn gemini(home: &Path, origin: &str, full: &str) -> Vec<Written> {
    let dir = home.join(".gemini");
    let env = format!("GOOGLE_GEMINI_BASE_URL={origin}\nGEMINI_API_KEY={PLACEHOLDER_BEARER}\n");
    let settings = format!(
        "{{\n  \"model\": {{\n    \"name\": {full}\n  }},\n  \"security\": {{\n    \"auth\": {{\n      \"selectedType\": \"gemini-api-key\"\n    }}\n  }}\n}}\n",
        full = json(full),
    );
    vec![
        file(home, dir.join(".env"), env),
        file(home, dir.join("settings.json"), settings),
    ]
}

fn opencode_like(
    home: &Path,
    id: &str,
    v1: &str,
    model_ref: &str,
    full: &str,
    token: &str,
) -> Vec<Written> {
    let dir = home.join(".config").join(id);
    let path = json_or_jsonc(&dir, id);
    let body = format!(
        "{{\n  \"model\": {full},\n  \"provider\": {{\n    {brand}: {{\n      \"npm\": \"@ai-sdk/openai-compatible\",\n      \"name\": {brand},\n      \"options\": {{\n        \"baseURL\": {v1},\n        \"apiKey\": {token}\n      }},\n      \"models\": {{\n        {model_ref}: {{\n          \"name\": {model_ref},\n          \"variants\": {{}}\n        }}\n      }}\n    }}\n  }}\n}}\n",
        full = json(full),
        brand = json(PLACEHOLDER_BEARER),
        v1 = json(v1),
        token = json(token),
        model_ref = json(model_ref),
    );
    vec![file(home, path, body)]
}

fn pi(home: &Path, v1: &str, model_ref: &str, token: &str) -> Vec<Written> {
    let dir = home.join(".pi").join("agent");
    let models = format!(
        "{{\n  \"providers\": {{\n    {brand}: {{\n      \"name\": {brand},\n      \"baseUrl\": {v1},\n      \"api\": \"openai-completions\",\n      \"apiKey\": {token},\n      \"models\": [\n        {{\n          \"id\": {model_ref},\n          \"name\": {model_ref},\n          \"reasoning\": false\n        }}\n      ]\n    }}\n  }}\n}}\n",
        brand = json(PLACEHOLDER_BEARER),
        v1 = json(v1),
        token = json(token),
        model_ref = json(model_ref),
    );
    let settings = format!(
        "{{\n  \"defaultProvider\": {brand},\n  \"defaultModel\": {model_ref}\n}}\n",
        brand = json(PLACEHOLDER_BEARER),
        model_ref = json(model_ref),
    );
    vec![
        file(home, dir.join("models.json"), models),
        file(home, dir.join("settings.json"), settings),
    ]
}

fn crush(home: &Path, v1: &str, model_ref: &str, token: &str) -> Vec<Written> {
    let path = crush_path(home);
    let body = format!(
        "{{\n  \"models\": {{\n    \"large\": {{\n      \"model\": {model_ref},\n      \"provider\": {brand}\n    }}\n  }},\n  \"providers\": {{\n    {brand}: {{\n      \"api_key\": {token},\n      \"base_url\": {v1},\n      \"models\": [\n        {{\n          \"can_reason\": false,\n          \"context_window\": 200000,\n          \"default_max_tokens\": 16384,\n          \"id\": {model_ref},\n          \"name\": {model_ref}\n        }}\n      ],\n      \"name\": {brand},\n      \"type\": \"openai\"\n    }}\n  }}\n}}\n",
        brand = json(PLACEHOLDER_BEARER),
        v1 = json(v1),
        token = json(token),
        model_ref = json(model_ref),
    );
    vec![file(home, path, body)]
}

fn commandcode(home: &Path, v1: &str, model_ref: &str, full: &str) -> Vec<Written> {
    let dir = home.join(".commandcode");
    let settings = format!(
        "{{\n  \"model\": {full},\n  \"modelProvider\": {brand}\n}}\n",
        full = json(full),
        brand = json(PLACEHOLDER_BEARER),
    );
    let providers = format!(
        "{{\n  \"provider\": {{\n    {brand}: {{\n      \"api\": \"openai-completions\",\n      \"apiKey\": false,\n      \"baseURL\": {v1},\n      \"models\": {{\n        {model_ref}: {{\n          \"name\": {model_ref}\n        }}\n      }},\n      \"name\": {brand}\n    }}\n  }}\n}}\n",
        brand = json(PLACEHOLDER_BEARER),
        v1 = json(v1),
        model_ref = json(model_ref),
    );
    vec![
        file(home, dir.join("settings.json"), settings),
        file(home, dir.join("providers.json"), providers),
    ]
}

fn fx(home: &Path, v1: &str, model_ref: &str) -> Vec<Written> {
    let body = format!(
        "{{\n  \"models\": {{\n    {brand}: {model_ref}\n  }},\n  \"provider\": {brand},\n  \"providers\": {{\n    {brand}: {{\n      \"auth\": {{\n        \"type\": \"none\"\n      }},\n      \"base_url\": {v1},\n      \"model_metadata\": {{\n        {model_ref}: {{\n          \"supports_tool_use\": true,\n          \"supports_vision\": false\n        }}\n      }},\n      \"protocol\": \"openai-chat-completions\",\n      \"tool_choice_mode\": \"send\"\n    }}\n  }}\n}}\n",
        brand = json(PLACEHOLDER_BEARER),
        v1 = json(v1),
        model_ref = json(model_ref),
    );
    vec![file(home, home.join(".fx").join("settings.json"), body)]
}

fn qoder(
    home: &Path,
    id: &str,
    dirname: &str,
    env: &str,
    v1: &str,
    model_ref: &str,
    full: &str,
) -> Vec<Written> {
    let dir = override_dir(env, home.join(dirname));
    let token = super::token_for(id);
    let body = format!(
        "{{\n  \"model\": {{\n    \"name\": {full}\n  }},\n  \"providers\": {{\n    {brand}: {{\n      \"apiKey\": {token},\n      \"baseUrl\": {v1},\n      \"displayName\": {brand},\n      \"model\": {model_ref},\n      \"models\": [\n        {{\n          \"capabilities\": {{\n            \"tools\": true,\n            \"vision\": false\n          }},\n          \"displayName\": {model_ref},\n          \"model\": {model_ref}\n        }}\n      ],\n      \"protocol\": \"openai\"\n    }}\n  }}\n}}\n",
        full = json(full),
        brand = json(PLACEHOLDER_BEARER),
        token = json(&token),
        v1 = json(v1),
        model_ref = json(model_ref),
    );
    vec![file(home, dir.join("settings.json"), body)]
}

fn cline(home: &Path, v1: &str, model_ref: &str, token: &str) -> Vec<Written> {
    let data = override_dir("CLINE_DIR", home.join(".cline")).join("data");
    let settings = data.join("settings");
    let providers = format!(
        "{{\n  \"lastUsedProvider\": \"openai-compatible\",\n  \"modes\": {{}},\n  \"providers\": {{\n    \"openai-compatible\": {{\n      \"settings\": {{\n        \"apiKey\": {token},\n        \"baseUrl\": {v1},\n        \"headers\": {{\n          \"User-Agent\": \"cline\"\n        }},\n        \"model\": {model_ref},\n        \"provider\": \"openai-compatible\"\n      }},\n      \"tokenSource\": \"manual\"\n    }}\n  }},\n  \"version\": 1\n}}\n",
        token = json(token),
        v1 = json(v1),
        model_ref = json(model_ref),
    );
    let models = format!(
        "{{\n  \"providers\": {{\n    \"openai-compatible\": {{\n      \"models\": {{\n        {model_ref}: {{\n          \"capabilities\": [\n            \"streaming\",\n            \"tools\"\n          ],\n          \"id\": {model_ref},\n          \"name\": {model_ref}\n        }}\n      }},\n      \"provider\": {{\n        \"baseUrl\": {v1},\n        \"defaultModelId\": {model_ref},\n        \"name\": {brand}\n      }}\n    }}\n  }},\n  \"version\": 1\n}}\n",
        model_ref = json(model_ref),
        v1 = json(v1),
        brand = json(PLACEHOLDER_BEARER),
    );
    vec![
        file(home, settings.join("providers.json"), providers),
        file(home, settings.join("models.json"), models),
    ]
}

fn zcode(home: &Path, origin: &str, model_ref: &str, token: &str) -> Vec<Written> {
    let dir = home.join(".zcode").join("v2");
    let config = format!(
        "{{\n  \"provider\": {{\n    {brand}: {{\n      \"enabled\": true,\n      \"kind\": \"anthropic\",\n      \"models\": {{\n        {model_ref}: {{\n          \"limit\": {{\n            \"context\": 200000\n          }},\n          \"modalities\": {{\n            \"input\": [\n              \"text\"\n            ],\n            \"output\": [\n              \"text\"\n            ]\n          }},\n          \"name\": {model_ref}\n        }}\n      }},\n      \"name\": {brand},\n      \"options\": {{\n        \"apiKey\": {token},\n        \"baseURL\": {origin}\n      }},\n      \"source\": \"custom\"\n    }}\n  }}\n}}\n",
        brand = json(PLACEHOLDER_BEARER),
        model_ref = json(model_ref),
        token = json(token),
        origin = json(origin),
    );
    let rules = format!(
        "{{\n  \"config\": {{\n    \"modelConfigRules\": {{\n      \"manualProviderModelRules\": [],\n      \"providerModelRules\": [\n        {{\n          \"config\": {{\n            \"properties\": {{\n              \"inputFormat\": {{\n                \"supportsImage\": false\n              }}\n            }}\n          }},\n          \"modelId\": {model_ref},\n          \"providerId\": {brand}\n        }}\n      ]\n    }},\n    \"providerConfigRules\": {{\n      \"providerRules\": [\n        {{\n          \"config\": {{\n            \"access\": {{\n              \"apiKey\": {token},\n              \"type\": \"api-key\"\n            }},\n            \"api\": {{\n              \"baseUrl\": {origin},\n              \"type\": \"anthropic-messages\"\n            }},\n            \"group\": \"standard-personal\",\n            \"modelOrder\": [\n              {model_ref}\n            ],\n            \"personalModelIds\": [\n              {model_ref}\n            ]\n          }},\n          \"enabled\": true,\n          \"providerId\": {brand},\n          \"providerName\": {brand}\n        }}\n      ]\n    }}\n  }},\n  \"schemaVersion\": 1\n}}\n",
        brand = json(PLACEHOLDER_BEARER),
        model_ref = json(model_ref),
        token = json(token),
        origin = json(origin),
    );
    vec![
        file(home, dir.join("config.json"), config),
        file(home, dir.join("provider_config.json"), rules),
    ]
}

fn workbuddy(home: &Path, v1: &str, model_ref: &str) -> Vec<Written> {
    let dir = override_dir("WORKBUDDY_CONFIG_DIR", home.join(".workbuddy"));
    let url = format!("{v1}/chat/completions");
    let token = super::token_for("workbuddy");
    let body = format!(
        "[\n  {{\n    \"apiKey\": {token},\n    \"id\": {model_ref},\n    \"maxInputTokens\": 200000,\n    \"name\": {model_ref},\n    \"supportsImages\": false,\n    \"supportsReasoning\": false,\n    \"supportsToolCall\": true,\n    \"url\": {url},\n    \"vendor\": {brand}\n  }}\n]\n",
        token = json(&token),
        model_ref = json(model_ref),
        url = json(&url),
        brand = json(PLACEHOLDER_BEARER),
    );
    vec![file(home, dir.join("models.json"), body)]
}

fn dsh(home: &Path, v1: &str, model_ref: &str, token: &str) -> Vec<Written> {
    let dir = override_dir("DSH_HOME", home.join(".dsh"));
    let body = format!(
        "- id: llm-deepseek # {brand}\n  config:\n    apiKey: {token}\n    baseURL: {v1}\n    thinking: enabled\n    reasoningEffort: high\n    models:\n      - id: {model_ref}\n        name: {model_ref}\n- id: agent-loop # {brand}\n  config:\n    agents:\n      - id: main\n        provider: deepseek-official\n        model: {model_ref}\n        cwd: !!js process.cwd()\n- id: api-gateway # {brand}\n  config:\n    provider: deepseek-official\n    model: {model_ref}\n",
        brand = PLACEHOLDER_BEARER,
        token = json(token),
        v1 = json(v1),
        model_ref = json(model_ref),
    );
    vec![file(home, dir.join("config.yaml"), body)]
}

fn hermes(home: &Path, v1: &str, model_ref: &str, token: &str) -> Vec<Written> {
    let dir = override_dir("HERMES_HOME", home.join(".hermes"));
    let body = format!(
        "model:\n  provider: {brand}\n  default: {model_ref}\nproviders:\n  {brand}:\n    name: {brand}\n    base_url: {v1}\n    api_key: {token}\n    api_mode: chat_completions\n    extra_headers:\n      User-Agent: hermes-agent\n    models:\n      - {model_ref}\n",
        brand = PLACEHOLDER_BEARER,
        model_ref = json(model_ref),
        v1 = json(v1),
        token = json(token),
    );
    vec![file(home, dir.join("config.yaml"), body)]
}

fn omp(home: &Path, v1: &str, full: &str, model_ref: &str) -> Vec<Written> {
    let dir = home.join(".omp").join("agent");
    let config = format!(
        "modelRoles:\n  default: {full}\n",
        full = json(full),
    );
    let models = format!(
        "providers:\n  {brand}:\n    baseUrl: {v1}\n    api: openai-completions\n    auth: none\n    models:\n      - id: {model_ref}\n        name: {model_ref}\n        reasoning: false\n",
        brand = PLACEHOLDER_BEARER,
        v1 = json(v1),
        model_ref = json(model_ref),
    );
    vec![
        file(home, yml_or_yaml(&dir, "config"), config),
        file(home, yml_or_yaml(&dir, "models"), models),
    ]
}

fn claude_code(home: &Path, origin: &str, model_ref: &str) -> Vec<Written> {
    let body = format!(
        "{{\n  \"env\": {{\n    \"ANTHROPIC_BASE_URL\": {origin},\n    \"ANTHROPIC_AUTH_TOKEN\": {token},\n    \"ANTHROPIC_MODEL\": {model_ref},\n    \"ANTHROPIC_SMALL_FAST_MODEL\": {model_ref},\n    \"ANTHROPIC_DEFAULT_OPUS_MODEL\": {model_ref},\n    \"ANTHROPIC_DEFAULT_SONNET_MODEL\": {model_ref},\n    \"ANTHROPIC_DEFAULT_HAIKU_MODEL\": {model_ref},\n    \"ANTHROPIC_DEFAULT_FABLE_MODEL\": {model_ref},\n    \"CLAUDE_CODE_SUBAGENT_MODEL\": {model_ref}\n  }},\n  \"model\": {model_ref}\n}}\n",
        origin = json(origin),
        token = json(PLACEHOLDER_BEARER),
        model_ref = json(model_ref),
    );
    vec![file(home, home.join(".claude").join("settings.json"), body)]
}

fn grok(home: &Path, v1: &str, model_ref: &str, full: &str, token: &str) -> Vec<Written> {
    let dir = override_dir("GROK_HOME", home.join(".grok"));
    let body = format!(
        "[features]\ncampaigns = false\n\n[models]\ndefault = {full}\n\n[model.{full}]\nmodel = {model_ref}\nname = {model_ref}\nbase_url = {v1}\napi_key = {token}\napi_backend = \"chat_completions\"\n",
        full = toml(full),
        model_ref = toml(model_ref),
        v1 = toml(v1),
        token = toml(token),
    );
    vec![file(home, dir.join("config.toml"), body)]
}

fn crush_path(home: &Path) -> PathBuf {
    #[cfg(windows)]
    if !super::sandboxed()
        && let Some(app) = std::env::var_os("LOCALAPPDATA")
        && !app.is_empty()
    {
        return PathBuf::from(app).join("crush").join("crush.json");
    }
    home.join(".config").join("crush").join("crush.json")
}

fn json_or_jsonc(dir: &Path, id: &str) -> PathBuf {
    let jsonc = dir.join(format!("{id}.jsonc"));
    if jsonc.exists() {
        jsonc
    } else {
        dir.join(format!("{id}.json"))
    }
}

fn yml_or_yaml(dir: &Path, name: &str) -> PathBuf {
    let yml = dir.join(format!("{name}.yml"));
    let yaml = dir.join(format!("{name}.yaml"));
    if !yml.exists() && yaml.exists() {
        yaml
    } else {
        yml
    }
}

fn file(home: &Path, path: PathBuf, body: String) -> Written {
    let rel = path
        .strip_prefix(home)
        .unwrap_or(path.as_path())
        .to_string_lossy()
        .replace('\\', "/");
    Written { rel, path, body }
}

fn json(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn toml(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

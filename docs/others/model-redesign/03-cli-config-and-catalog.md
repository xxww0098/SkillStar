状态：research

# T3 · CLI Agent 配置格式、模型目录与路由层

> 本文是 Models 工作台重设计的**下界调研**：SkillStar 的新数据模型无论怎么设计，都必须能无损投影到本文列出的这些真实文件格式。
> 全部结论来自 2026-08-15 当天克隆的源码 / 官方 schema / 本机实际安装的产物，不依赖记忆。
> 本文只描述外部事实与由此推出的约束，不是 SkillStar 的功能文档；落地时行为变更请写进 `docs/features/models/README.md`。

---

## 0. 先看这五条

1. **Codex CLI 已经彻底删除 `wire_api = "chat"`，而 SkillStar 现在默认给所有第三方 Provider 写 `"chat"`。** 这不是"将来会坏"，是现在就坏：Codex 对该值返回的是**反序列化错误**而不是警告，整个 `config.toml` 解析失败，Codex 直接起不来。详见 [§2.2](#22-codex-cli) 与 [§7.4](#74-已知会炸的三处)。
2. **"一次配置、投影到各家"的最小公共角色抽象是 5 个角色**：必填的 `default`，加可选的 `fast` / `plan` / `vision` / `subagent`，再加一个存放工具专属角色的 `extra` 逃生舱。工具侧的角色要么是这组的子集，要么只是改了名字。详见 [§5](#5-c-路由与角色最小公共角色抽象)。
3. **models.dev 的 schema 几乎可以直接抄。** 它把"模型本身的事实"（`models/`）和"某家 Provider 怎么卖这个模型"（`providers/*/models/`）拆成两层并用 `base_model` 继承——这正是 SkillStar 现在缺的那一层。详见 [§4.1](#41-modelsdev-sstmodelsdev)。
4. **业界对模型目录的共识做法是"内置快照 + 运行时刷新 + 本地缓存"三级回退**（Crush 的 catwalk 同步器是最完整的样板），没有任何一家纯靠 `/v1/models`。建议 SkillStar 照抄这个三级结构。详见 [§4.5](#45-评估skillstar-该怎么选)。
5. **凭据落盘上有一条硬约束：Codex 没有第三方 Provider 的 key 存放位置。** `auth.json` 只有 `OPENAI_API_KEY` 一个槽位，第三方只能走 `env_key`（环境变量）或 `experimental_bearer_token`（官方明确标注"discouraged"）。SkillStar 现在选的 `env_key` 路线是对的，但要意识到它要求用户自己 export 环境变量。详见 [§6](#6-d-凭据与安全)。

---

## 1. 证据来源与版本戳

| 项目 | 获取方式 | 版本 / commit | 落地路径 |
| --- | --- | --- | --- |
| OpenCode | `git clone --depth=1 --filter=blob:none`（排除 `cloud/`、`www/`） | `e23586a` (2026-08-14) | `/tmp/model-research/opencode` |
| Codex CLI | `git clone --depth=1` | `da89849` (2026-08-14)，release 线 `rust-v0.148.0-alpha.15` | `/tmp/model-research/codex` |
| Crush | `git clone --depth=1` | `051955a` (2026-08-13) | `/tmp/model-research/crush` |
| Aider | `git clone --depth=1` | `5dc9490` (2026-05-22)，`__version__ = 0.86.3.dev` | `/tmp/model-research/aider` |
| models.dev | `git clone --depth=1` | `942682f` (2026-08-14) | `/tmp/model-research/modelsdev` |
| Claude Code | 官方文档 + JSON Schema | `json.schemastore.org/claude-code-settings.json`（2026-08-15 抓取，142 个顶层属性 / 340 个 `env` 属性） | `/tmp/model-research/claude-code-settings.schema.json` |
| LiteLLM 价格表 | `raw.githubusercontent.com` 直取 | 2026-08-15 抓取，3020 条 | `/tmp/model-research/litellm_prices.json` |
| Pi | 本机安装包（含 `.d.ts` 类型声明） | `@earendil-works/pi-coding-agent@0.84.1` | `/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent` |
| OMP | 本机安装包（**含完整 TypeScript `src/`**） | `@oh-my-pi/pi-coding-agent@17.3.2` | `~/.bun/install/global/node_modules/@oh-my-pi/pi-coding-agent` |

关于 Pi / OMP：任务书里提到"找不到就明说"。结果是**都找到了**，而且是比 GitHub 更可靠的来源——两个包都把源码或完整类型声明打进了 npm 产物。特别是 OMP 直接发布了 `src/config/*.ts`，字段定义可以逐行读。但要注意：**Pi 和 OMP 的仓库本身不公开**，能核实的只有已发布产物，所以本文对它们的结论有效期只到当前版本。

---

## 2. A. 写盘目标格式

### 2.1 OpenCode

**文件与优先级**（`packages/web/src/content/docs/config.mdx:44-56`，实现在 `packages/opencode/src/config/config.ts:250-520`）：

```
1. Remote config (.well-known/opencode)   组织默认值
2. Global  ~/.config/opencode/opencode.json
3. OPENCODE_CONFIG 指定的自定义文件
4. Project opencode.json
5. .opencode/ 目录（agents/commands/plugins）
6. OPENCODE_CONFIG_CONTENT 环境变量（内联 JSON）
7. Managed  /Library/Application Support/opencode/（macOS）
8. macOS MDM .mobileconfig                最高，用户不可覆盖
```

后面的覆盖前面的，且是**深合并（merge，不是 replace）**——这一点对 SkillStar 很关键：写全局配置不会抹掉用户的项目配置，反之项目配置会盖住 SkillStar 写的全局值。全局文件候选顺序是 `config.json` → `opencode.json` → `opencode.jsonc`（三个都会被依次合并，`config.ts:258-260`）。SkillStar 现在写 `~/.config/opencode/opencode.json`，位置正确。

**一个第三方 OpenAI 兼容 Provider + 一个模型的最小写法**（`packages/web/src/content/docs/providers.mdx:2484-2500`）：

```json
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "myprovider": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "My AI Provider Display Name",
      "options": { "baseURL": "https://api.myprovider.com/v1", "apiKey": "{env:MY_KEY}" },
      "models": { "my-model-name": { "name": "My Model Display Name" } }
    }
  },
  "model": "myprovider/my-model-name",
  "small_model": "myprovider/my-fast-model"
}
```

**Provider 块的完整字段**（权威定义：`packages/core/src/v1/config/provider.ts:82-126`）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `npm` | string | AI SDK 包名。OpenAI 兼容（`/v1/chat/completions`）用 `@ai-sdk/openai-compatible`；走 `/v1/responses` 用 `@ai-sdk/openai` |
| `name` | string | UI 显示名 |
| `api` | string | 基础 URL（另一种写法，与 `options.baseURL` 并存） |
| `env` | string[] | 触发自动启用的环境变量名 |
| `whitelist` / `blacklist` | string[] | 模型选择器过滤；whitelist 先收窄，blacklist 再剔除 |
| `options.baseURL` | string | API 端点 |
| `options.apiKey` | string | 支持 `{env:VAR}` / `{file:path}` 插值（`config.mdx:899,922`） |
| `options.headers` | map | 自定义头 |
| `options.timeout` / `headerTimeout` / `chunkTimeout` | number \| false | 超时控制 |
| `models.<id>` | Model | 见下 |

**Model 块字段**（`provider.ts:13-80`）：`id` `name` `family` `release_date` `attachment` `reasoning` `temperature` `tool_call` `interleaved` `cost{input,output,cache_read,cache_write,context_over_200k}` `limit{context,input,output}` `modalities{input,output}` `status` `provider{npm,api}` `options` `headers` `variants`。

注意 `limit.context` / `limit.output` 这两个：文档明说"标准 Provider 从 models.dev 自动拉，自定义 Provider 必须自己写，否则 OpenCode 不知道还剩多少上下文"（`providers.mdx:2555`）。**这是 SkillStar 必须携带模型元数据的第一个硬理由**——不是锦上添花，是缺了就功能降级。

**凭据**：`opencode auth login` 写 `~/.local/share/opencode/auth.json`（`packages/opencode/src/provider/auth.ts:11`），格式是 `{ "<providerID>": { "type": "api"|"oauth"|"wellknown", ... } }`，**写入时显式 `0o600`**（同文件 `writeJson(file, ..., 0o600)`）。SkillStar 已经在 `resolve_opencode_auth_path()` 里正确区分了 config 与 auth 两个文件。

**比 SkillStar 现在做法更干净的写法**：有一个。SkillStar 现在把 API key 直接写进配置。OpenCode 原生支持 `"apiKey": "{env:SKILLSTAR_MYPROVIDER_KEY}"` 和 `{file:~/.secrets/key}`，可以让明文 key 只存在一个 0600 文件里，配置文件本身可提交、可分享。见 [§6.3](#63-最小惊讶原则skillstar-写盘时应遵守的)。

### 2.2 Codex CLI

**文件**：`~/.codex/config.toml`（TOML）。**有官方 JSON Schema**：仓库内 `codex-rs/core/config.schema.json`（174 KB，由 `just write-config-schema` 从 `ConfigToml` 类型生成，见 `codex-rs/core/src/config/schema.md`）。顶层 94 个属性。

**一个第三方 Provider + 一个模型的写法**：

```toml
model = "some-model-id"
model_provider = "skillstar_ab12cd34"

[model_providers.skillstar_ab12cd34]
name = "SkillStar"
base_url = "https://api.example.com/v1"
env_key = "SKILLSTAR_AB12CD34_API_KEY"
wire_api = "responses"     # ← 唯一合法值
```

**`ModelProviderInfo` 全字段**（`codex-rs/model-provider-info/src/lib.rs:87-146`，schema 中 `additionalProperties: false`）：
`name` `base_url` `env_key` `env_key_instructions` `experimental_bearer_token` `auth{command,args,cwd,timeout_ms,refresh_interval_ms}` `aws{profile,region}` `wire_api` `query_params` `http_headers` `env_http_headers` `request_max_retries` `stream_max_retries` `stream_idle_timeout_ms` `websocket_connect_timeout_ms` `requires_openai_auth` `supports_websockets` `supports_standalone_web_search`。

#### 三个必须知道的坑

**坑 1 —— `wire_api` 只剩 `"responses"`。**

`enum WireApi` 现在只剩一个变体 `Responses`（`lib.rs:58-64`），自定义 `Deserialize`（同文件 74-85 行）对 `"chat"` 直接返回错误：

```
`wire_api = "chat"` is no longer supported.
How to fix: set `wire_api = "responses"` in your provider config.
More info: https://github.com/openai/codex/discussions/7782
```

时间线（检索 `gh api repos/openai/codex/releases` 的 release body）：`rust-v0.72.0`（2025-12-13）PR #7897 加弃用提示 → `rust-v0.95.0`（2026-02-04）PR #10157 "nuke chat/completions API" + #10498 "drop wire_api from clients" 彻底删除。

**后果**：Codex ≥ 0.95 的 `config.toml` 里只要出现 `wire_api = "chat"`，**整个文件解析失败**，Codex 无法启动。而 SkillStar `crates/skillstar-models/src/providers/crud.rs:22-28` 的 `recommended_codex_defaults()` 对所有非 `api.openai.com` 的 URL 返回 `("chat", "third_party")`——也就是**每一个第三方 Provider**。

**衍生后果，比第一个更麻烦**：删掉 `"chat"` 不是改个字符串就完事。Codex 现在**只说 Responses API**。绝大多数第三方 OpenAI 兼容端点只实现 `/v1/chat/completions`。所以"把第三方 Provider 接进 Codex"这件事，在 Codex 侧已经从"配置问题"变成了"端点能力问题"：**只有真正实现了 `/v1/responses` 的 Provider 才能接**。新数据模型必须把"该 Provider 是否支持 Responses API"作为一个可探测、可存储的能力位，而不是一个用户随便填的枚举。

**坑 2 —— 内置 Provider ID 不可覆盖。**

```rust
// lib.rs:301-303, merge_configured_model_providers
model_providers.entry(key).or_insert(provider);   // or_insert，不是 insert
```

内置 ID 是 `openai` / `amazon-bedrock` / `amazon-bedrock-runtime` / `ollama` / `lmstudio`（`lib.rs:452-479`）。用户在 `[model_providers.openai]` 里写什么都会被静默忽略（bedrock 两个是例外，允许改 `base_url`/`auth`/`http_headers`/`aws`，其它字段会**报错**）。SkillStar 用 `skillstar_<id8>` 前缀正好绕开了这个坑，是对的。

**坑 3 —— `auth.json` 只有一个 key 槽。**

```rust
// codex-rs/login/src/auth/storage.rs:38-60
pub struct AuthDotJson {
    pub auth_mode: Option<AuthMode>,
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    pub tokens: Option<TokenData>,       // ChatGPT OAuth
    pub last_refresh: Option<DateTime<Utc>>,
    pub agent_identity: Option<AgentIdentityStorage>,
    pub personal_access_token: Option<String>,
    pub bedrock_api_key: Option<BedrockApiKeyAuth>,
}
```

没有"per-provider key"的位置。第三方 Provider 的 key 只有三条路：`env_key`（环境变量名）、`experimental_bearer_token`（源码注释直接写"Use of this config is discouraged in favor of `env_key` for security reasons"）、`auth.command`（跑一条命令取 token，带 `timeout_ms` 5s / `refresh_interval_ms` 300s 默认值）。

另有 `cli_auth_credentials_store` 配置项：`file`（默认）/ `keyring`（系统钥匙串）/ `auto`。

#### 角色路由与模型目录（Codex 侧的两个惊喜）

- `review_model`：`/review` 功能专用模型覆盖。
- `agents.default_subagent_model` + `agents.default_subagent_reasoning_effort`：子 agent 默认模型。
- `agents.<role>`（`AgentRoleToml`）：每个角色可以指向一个**独立的 config 分层文件**（`config_file`），加 `description` 和 `nickname_candidates`。这是所有被调研工具里最重的角色系统——角色不只是"换个模型"，是"换一整层配置"。
- `profiles.<name>`（`ConfigProfile`）：可以整包切换 `model` / `model_provider` / `model_reasoning_effort` / `model_verbosity` / `service_tier` / `sandbox_mode` 等 27 个字段，用 `profile = "name"` 激活。**这是 SkillStar 多 Provider 共存的另一种更干净的表达方式**：与其只维护一个 `model_provider` 指针，不如给每个绑定的 Provider 生成一个 profile，用户 `codex --profile skillstar_xxx` 就能切，互不干扰。
- `model_catalog_json`：指向一个 JSON 文件，内容是 `ModelsResponse`（`ModelInfo` 数组）。加载逻辑在 `codex-rs/core/src/config/mod.rs:2028-2050`，**只在启动时读一次**，空数组会报错。`ModelInfo` 有 `slug` `display_name` `context_window` `max_context_window` `auto_compact_token_limit` `supported_reasoning_levels` `input_modalities` `service_tiers` 等 40 余字段（`codex-rs/protocol/src/openai_models.rs:385-470`）。

### 2.3 Claude Code

**文件与优先级**（官方文档 code.claude.com/docs/en/settings）：

```
1. Managed（/Library/Application Support/ClaudeCode/、/etc/claude-code/、MDM）  最高
2. 命令行参数
3. .claude/settings.local.json     （项目本地，gitignore）
4. .claude/settings.json           （项目共享）
5. ~/.claude/settings.json         （用户级）   最低
```

SkillStar 写的是最低优先级的 `~/.claude/settings.json`。**这意味着任何项目级 `.claude/settings.json` 都会静默覆盖 SkillStar 的绑定**——这是一个 UX 上必须告知用户的事实，SkillStar 现在没有做冲突检测。

**写法**：Claude Code 没有 provider 概念，全部通过 `env` 块注入环境变量。

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "https://api.example.com",
    "ANTHROPIC_AUTH_TOKEN": "sk-...",
    "ANTHROPIC_MODEL": "some-model",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL": "some-fast-model"
  }
}
```

**关键环境变量清单**（来源：schemastore 官方 schema `properties.env.properties`，共 340 条，下表是与 Provider 绑定相关的全部）：

| 变量 | 作用 |
| --- | --- |
| `ANTHROPIC_BASE_URL` | 端点覆盖，指向代理 / 网关 |
| `ANTHROPIC_AUTH_TOKEN` | 自定义 `Authorization: Bearer` |
| `ANTHROPIC_API_KEY` | 标准 Anthropic key |
| `ANTHROPIC_CUSTOM_HEADERS` | 自定义头，**换行分隔的 `Name: Value`** |
| `ANTHROPIC_MODEL` | 运行时模型覆盖 |
| `ANTHROPIC_DEFAULT_OPUS_MODEL` | Opus 档位钉死 |
| `ANTHROPIC_DEFAULT_SONNET_MODEL` | Sonnet 档位钉死 |
| `ANTHROPIC_DEFAULT_HAIKU_MODEL` | Haiku 档位钉死 |
| `ANTHROPIC_DEFAULT_FABLE_MODEL` | Fable 档位钉死 |
| `ANTHROPIC_SMALL_FAST_MODEL` | **已废弃**，schema 原文："DEPRECATED (prefer `ANTHROPIC_DEFAULT_HAIKU_MODEL`)" |
| `CLAUDE_CODE_SUBAGENT_MODEL` | 子 agent 模型 |
| `ANTHROPIC_CUSTOM_MODEL_OPTION` | **在模型选择器里加一个自定义模型条目** |
| `ANTHROPIC_CUSTOM_MODEL_OPTION_NAME` / `_DESCRIPTION` / `_SUPPORTED_CAPABILITIES` | 上一条的显示名 / 描述 / 能力（JSON 对象） |
| `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY` | **当 `ANTHROPIC_BASE_URL` 指向 Anthropic 兼容网关时，从其 `/v1/models` 发现模型** |
| `CLAUDE_CODE_MAX_CONTEXT_TOKENS` / `CLAUDE_CODE_MAX_OUTPUT_TOKENS` | 上下文 / 输出上限覆盖 |
| `ANTHROPIC_BETAS` | 逗号分隔的 beta 头 |

另有四组 `ANTHROPIC_DEFAULT_*_MODEL_NAME` / `_DESCRIPTION` / `_SUPPORTED_CAPABILITIES`，用于自定义档位在选择器里的显示。

**顶层非 env 字段**中与模型相关的：`model`（会话启动时读）、`advisorModel`、`availableModels`（限制可选模型，多层设置会合并去重）、`enforceAvailableModels`、`apiKeyHelper`（脚本输出凭据，同时作为 `X-Api-Key` 和 `Authorization: Bearer` 发送，配 `CLAUDE_CODE_API_KEY_HELPER_TTL_MS`）。

**比 SkillStar 现在做法更干净的写法**：有三处。
1. `ANTHROPIC_SMALL_FAST_MODEL` 应改为 `ANTHROPIC_DEFAULT_HAIKU_MODEL`（前者官方标废弃）。
2. `ANTHROPIC_CUSTOM_MODEL_OPTION` 系列可以让 SkillStar 绑定的第三方模型**出现在 Claude Code 的模型选择器里**，用户能自己切换，而不是只有 SkillStar 写死的那一个。这是目前 SkillStar 完全没用上的能力。
3. `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY` 可以让 Claude Code 自己去网关拉模型列表——对 CLIProxyAPI 一类的中转特别合适。

**注意一个事实**：官方 gateway 文档明确写"Anthropic doesn't support routing Claude Code to non-Claude models through any gateway"。SkillStar 把任意 OpenAI 兼容 Provider 接给 Claude Code，属于文档外用法，能不能跑取决于中转是否实现了 Anthropic Messages 协议。这个前提应该在 UI 上说清楚，而不是让用户以为随便一个 Provider 都能接。

### 2.4 Crush（charmbracelet/crush）

**有官方 JSON Schema**：仓库根 `schema.json`（`$id: https://github.com/charmbracelet/crush/internal/config/config`，线上 `https://charm.land/crush.json`）。

**文件查找顺序**（`internal/config/load.go:911-947`，后加载的优先级更高）：

```
/etc/crush/crush.json                                   系统级
$XDG_CONFIG_HOME/crush/crush.json（或 CRUSH_GLOBAL_CONFIG）   全局
  + 同目录 crushrc（shell 格式）
$XDG_DATA_HOME/crush/crush.json（或 CRUSH_GLOBAL_DATA）        机器写入的状态
向上走到 git 根为止的项目内配置，同目录内优先级：
  .crushrc > crushrc > .crush.json > crush.json
```

**写法**：

```json
{
  "$schema": "https://charm.land/crush.json",
  "providers": {
    "skillstar_ab12": {
      "name": "SkillStar",
      "base_url": "https://api.example.com/v1",
      "type": "openai-compat",
      "api_key": "$SKILLSTAR_KEY",
      "discover_models": true,
      "models": [{
        "id": "some-model", "name": "Some Model",
        "cost_per_1m_in": 0.5, "cost_per_1m_out": 1.5,
        "cost_per_1m_in_cached": 0, "cost_per_1m_out_cached": 0,
        "context_window": 200000, "default_max_tokens": 8192,
        "can_reason": true, "supports_attachments": false
      }]
    }
  },
  "models": {
    "large": { "provider": "skillstar_ab12", "model": "some-model" },
    "small": { "provider": "skillstar_ab12", "model": "some-fast-model" }
  }
}
```

**`ProviderConfig` 字段**：`id` `name` `base_url` `type` `api_key` `oauth` `disable` `system_prompt_prefix` `extra_headers` `extra_body` `provider_options` `aws_auth_refresh` `flat_rate` `discover_models` `models[]`。

`type` 是闭合枚举，共 15 个值：`openai` `openai-compat` `openrouter` `vercel` `anthropic` `google` `azure` `bedrock` `google-vertex` `hyper` `litellm` `llamacpp` `lmstudio` `ollama` `omlx`。第三方 OpenAI 兼容用 `openai-compat`。

**`Model` 的 required 字段是 10 个**（schema `$defs/Model.required`）：`id` `name` `cost_per_1m_in` `cost_per_1m_out` `cost_per_1m_in_cached` `cost_per_1m_out_cached` `context_window` `default_max_tokens` `can_reason` `supports_attachments`。**这是所有被调研工具中对模型元数据要求最严格的一个**——价格和上下文窗口是必填，不能省。SkillStar 想支持 Crush，就必须能给出这 10 个值；这是"SkillStar 必须持有模型元数据"的第二个硬理由。

**`discover_models`（默认 `true`）**：自动从 `/v1/models` 拉，"When true with existing models they are merged (yours win)"。这给了一条优雅的降级路径：SkillStar 只写 Provider 块不写 models，让 Crush 自己发现；但发现出来的模型没有价格和上下文，功能会降级。

**角色**：`models` 是 `map[SelectedModelType]SelectedModel`，`SelectedModelType` 只有两个值 `"large"` / `"small"`（`internal/config/config.go:55-56`）。Agent 定义里 `model` 字段的 enum 也只是 `large|small`（`config.go:556`）。`small` 未显式配置**且**其 provider 不在已知 provider 列表里时，回落到 `large`（`load.go:890-902`，带一条 warn 日志；已知 provider 有各自的默认 small 模型）。`SelectedModel` 除了 `provider`/`model` 还能带 `reasoning_effort`（`low|medium|high`）、`think`、`max_tokens`、`temperature`、`top_p`、`top_k`、`frequency_penalty`、`presence_penalty`、`provider_options`。

**凭据**：`api_key` 的值走**完整 shell 展开**——`$VAR`、`${VAR}`、`${VAR:-default}`、`${VAR:?msg}`、以及 `$(command)`（`internal/config/resolve.go:56-63`，底层就是 bash 工具用的同一个解释器，5 分钟超时）。这意味着 SkillStar 可以写 `"api_key": "$SKILLSTAR_KEY"` 而不落明文；但也意味着**读别人的 crush.json 时要当成可执行内容对待**。源码里明确注释了这一点：data 目录的 JSON "is writable machine state and must never be executed as Bash"（`load.go:916-918`）。

### 2.5 Aider

**没有官方 schema**。配置分三个文件，各有搜索路径（`aider/main.py:290-410`，从当前目录向上到 git 根，再到 `~`）：`.aider.conf.yml`（所有 CLI 参数的 YAML 形式）、`.aider.model.settings.yml`（`ModelSettings` 列表）、`.aider.model.metadata.json`（LiteLLM 格式的价格/上下文）。

**`ModelSettings` 字段**（`aider/models.py:128-150`，dataclass，即 yml 的合法键）：
`name` `edit_format` `weak_model_name` `use_repo_map` `send_undo_reply` `lazy` `overeager` `reminder` `examples_as_sys_msg` `extra_params` `cache_control` `caches_by_default` `use_system_prompt` `use_temperature` `streaming` `editor_model_name` `editor_edit_format` `reasoning_tag` `remove_reasoning`(废弃) `system_prompt_prefix` `accepts_settings`。

**模型元数据用的是 LiteLLM 的格式**（`.aider.model.metadata.json` 里的键值就是 `model_prices_and_context_window.json` 的条目形状），因为 Aider 底层是 LiteLLM。字段：`max_input_tokens` `max_output_tokens` `input_cost_per_token` `output_cost_per_token` `cache_read_input_token_cost` `litellm_provider` `mode` `supports_*`……注意它**允许 `//` 注释**（内置的 `aider/resources/model-metadata.json` 里就有），所以不是严格 JSON。

**Provider 概念**：Aider 没有 provider 块。第三方端点靠环境变量：`--openai-api-base`（即 `OPENAI_API_BASE`）+ `--api-key provider=KEY`（展开成 `PROVIDER_API_KEY`）+ `--set-env NAME=value`。也读 `.env`（搜索路径同上，另加 `~/.aider/oauth-keys.env`）。

**角色**：`--model`（主）/ `--weak-model`（弱，做 commit message、总结）/ `--editor-model`（编辑器模型）。三档，且 `weak_model_name` / `editor_model_name` 可以在 model-settings 里 per-model 指定默认值——**这是唯一一个把"角色映射"下沉到模型级别的设计**（"用 gpt-4o 时，weak 用 gpt-4o-mini"）。

**成熟度提示**：最后一次提交 2026-05-22，距今约 3 个月无更新。相对于其它四个（都是 1–2 天内有提交），Aider 已明显进入低活跃期。SkillStar 是否值得为它写盘，是产品优先级问题，不是技术问题。

### 2.6 Pi（`@earendil-works/pi-coding-agent@0.84.1`）

仓库不公开，但 npm 产物带完整 `.d.ts`。

**文件**（`dist/config.js:423-425` 等）：
- `~/.pi/agent/models.json` —— 自定义 Provider / 模型（`getModelsPath()`）
- `~/.pi/agent/settings.json` —— 全局设置（`getSettingsPath()`）
- `~/.pi/agent/auth.json` —— 凭据（`getAuthPath()`，本机权限 `0600`）
- `~/.pi/agent/models-store.json` —— **模型发现缓存**，不是用户配置

`models.json` 结构（`dist/core/model-config.js:179`，`dist/core/model-config.d.ts` 的 `ProviderConfigSchema`）：

顶层是 `{ "providers": { "<providerId>": ProviderConfig } }`。`ProviderConfig` 字段：`name` `baseUrl` `apiKey` `api` `oauth`(仅 `"radius"`) `headers` `authHeader` `compat`(30+ 个兼容性开关) `models[]` `modelOverrides`。
`models[]` 元素字段：`id`(必填) `name` `api` `baseUrl` `reasoning` `thinkingLevelMap{off,minimal,low,medium,high,xhigh,max → string|null}` `input[]`(`text`/`image`) `cost{input,output,cacheRead,cacheWrite,tiers[{inputTokensAbove,input,output,cacheRead,cacheWrite}]}` `contextWindow` `maxTokens` `samplingParams` `headers` `compat`。`modelOverrides` 是 `{ "<modelId>": 同上字段 }`，用于覆盖内置模型。

`settings.json` 的模型指针只有三个字段（`dist/core/settings-manager.d.ts:67-69`）：`defaultProvider` / `defaultModel` / `defaultThinkingLevel`。本机实测：

```json
{ "defaultProvider": "opencode-go", "defaultModel": "deepseek-v4-flash", "defaultThinkingLevel": "max" }
```

**Pi 没有角色路由**——只有一个全局默认模型。这是投影时最不容易失真的一家（角色多的往少的投，只保留 `default`）。

`models-store.json` 的形状是 `{ "<providerId>": { "models": [...], "checkedAt": <ms>, "lastModified": <ms>, "etag": "..." } }`——**带 ETag 的模型发现缓存**，正是 [§4.5](#45-评估skillstar-该怎么选) 要讨论的三级结构里的"缓存层"。

### 2.7 OMP（`@oh-my-pi/pi-coding-agent@17.3.2`，Pi 的分支）

**npm 包直接发布了 TypeScript 源码 `src/`**，是本次调研里字段可信度最高的非开源目标。

**文件**：
- `~/.omp/agent/models.yml` —— 自定义 Provider / 模型。加载器是 `ConfigFile("models")`（`src/config/config-file.ts:140-155`），**优先 `.yml`，回落 `.yaml`，并且会把历史遗留的 `models.json` 自动迁移成 `models.yml`**。SkillStar 现在写 `models.yml`，与实现一致。
- `~/.omp/agent/config.yml` —— 运行时设置，含 `modelRoles`
- `~/.omp/agent/models.db` —— **SQLite 模型缓存**，表 `model_cache(provider_id PK, version, updated_at, authoritative, static_fingerprint, header_omitted_model_ids, unrestorable_header_model_ids, header_restore_version, models TEXT)`

`models.yml` 的 `providers.<id>` 字段（`src/config/models-config-schema-bundle.ts:270-300`）：
`baseUrl` `apiKey` `api` `headers` `compat` `remoteCompaction` `authHeader` `auth`(`"apiKey"|"none"|"oauth"`) `discovery` `models[]` `modelOverrides` `disableStrictTools` `transport`(`"pi-native"`)。

`api` 是闭合枚举（同文件 82-84 行）：
`openai-completions` | `openai-responses` | `openai-codex-responses` | `azure-openai-responses` | `anthropic-messages` | `bedrock-converse-stream` | `google-generative-ai` | `google-gemini-cli` | `google-vertex`

`discovery.type`：`ollama` | `llama.cpp` | `lm-studio` | `openai-models-list` | `proxy` | `litellm`——**内置了"从端点自动发现模型"的六种策略**，`litellm` 是其中之一。

**校验规则**（`src/config/models-config.ts:34-100`，写盘时必须满足，否则 OMP 启动报错）：
- 定义了 `models` 就**必须**有 `baseUrl`；
- 定义了 `models` 且 `auth != "none"` 就**必须**有 `apiKey`；
- 每个 model 必须在 provider 级或 model 级有 `api`；
- 开启 provider 级 `discovery` 且 `type != "proxy"` 时必须有 `api`；
- provider 块不能是空的——至少要有 `baseUrl`/`headers`/`apiKey`/`auth:none`/`compat`/`disableStrictTools`/`remoteCompaction`/`modelOverrides`/`discovery`/`models` 之一。

**`apiKey` 的取值语法**（`src/config/model-config-values.ts` 的 `resolveConfigValue` 文档注释）：以 `!cmd` 开头则执行 shell 命令取 stdout；否则**先查同名环境变量**，查不到才当字面值。所以 `apiKey: SKILLSTAR_KEY` 会优先解析成环境变量——这是一个容易踩的坑：**明文 key 如果恰好和某个环境变量同名，会被环境变量顶掉**。

**角色（本次调研中最完整的角色系统）**，`src/config/model-roles.ts:22-64`，10 个内置角色（`角色 = 显示名`）：
`default = Default`、`smol = Fast`、`slow = Thinking`、`vision = Vision`、`plan = Architect`、`designer = Designer`、`commit = Commit`、`tiny = Tiny`、`task = Subtask`、`advisor = Advisor`。

而且**支持用户自定义角色**（`getKnownRoleIds()` 会把 `cycleOrder`、`modelRoles`、`modelTags` 里出现的任意 key 都当作角色）。角色选择器语法是 `@<role>`（旧写法 `pi/<role>`），`*` 是 default 的简写。

本机 `config.yml` 实测：

```yaml
modelRoles:
  slow: opencode-go/deepseek-v4-pro:max
  smol: opencode-go/deepseek-v4-flash:xhigh
  default: opencode-go/deepseek-v4-pro:max
  plan: aiproxy/gpt-5.6-sol:max
  vision: aiproxy/gemini-3.6-flash:high
defaultThinkingLevel: xhigh
```

**注意模型引用的形状是 `provider/model:thinkingLevel`** ——思考档位是模型引用的一部分，不是旁边的独立字段。SkillStar 的 `paths_files.rs` 注释说 "`default` / `slow` / `smol` 角色"，只列了 3 个且没提 `:level` 后缀，与实际的 10 角色 + 档位后缀有差距。写盘时如果丢掉 `:level`，用户的思考档位配置会被降级。

---

## 3. 写盘目标能力矩阵

| Agent | 配置文件路径 | 格式 | 多 Provider | 角色路由 | Key 存放位置 | 官方 schema |
| --- | --- | --- | --- | --- | --- | --- |
| **OpenCode** | `~/.config/opencode/opencode.json`（另可 `.jsonc`/`config.json`；项目级 `opencode.json`） | JSON / JSONC | ✅ `provider.<id>` map | ✅ `model` + `small_model` + `agent.<name>.model` | `~/.local/share/opencode/auth.json`（0600）；或配置内 `options.apiKey` 支持 `{env:}` / `{file:}` | ✅ `https://opencode.ai/config.json`（由 Effect Schema 生成，`packages/opencode/script/schema.ts`） |
| **Codex CLI** | `~/.codex/config.toml` | TOML | ✅ `[model_providers.<id>]`（内置 ID 不可覆盖） | ✅ `review_model` / `agents.<role>` / `agents.default_subagent_model` / `profiles.<name>` | `~/.codex/auth.json` **仅 `OPENAI_API_KEY` 一个槽**；第三方走 `env_key` 环境变量 / `experimental_bearer_token`(不推荐) / `auth.command`；可选系统 keyring | ✅ 仓库内 `codex-rs/core/config.schema.json` |
| **Claude Code** | `~/.claude/settings.json`（项目 `.claude/settings.json` 优先级更高） | JSON | ❌ 单一全局 env | ✅ `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU,FABLE}_MODEL` + `CLAUDE_CODE_SUBAGENT_MODEL` + `advisorModel` | `env.ANTHROPIC_AUTH_TOKEN` / `env.ANTHROPIC_API_KEY` 明文；或 `apiKeyHelper` 脚本 | ✅ `https://json.schemastore.org/claude-code-settings.json` |
| **Crush** | `~/.config/crush/crush.json`（+ `/etc/crush/`、`$XDG_DATA_HOME/crush/`、项目 `.crush.json`/`crush.json`） | JSON（另有 `crushrc` shell 格式） | ✅ `providers.<id>` map | ⚠️ 仅 `large` / `small` 两档 | 配置内 `api_key`，值走**完整 shell 展开**（`$VAR` / `$(cmd)`） | ✅ 仓库 `schema.json` / `https://charm.land/crush.json` |
| **Aider** | `.aider.conf.yml` + `.aider.model.settings.yml` + `.aider.model.metadata.json`（cwd→git root→`~` 逐级搜索） | YAML + JSON(带注释) | ❌ 无 provider 概念，靠 env | ✅ `--model` / `--weak-model` / `--editor-model`；可 per-model 指定 `weak_model_name`/`editor_model_name` | 环境变量 / `.env` / `~/.aider/oauth-keys.env` | ❌ |
| **Pi** | `~/.pi/agent/models.json` + `~/.pi/agent/settings.json` | JSON | ✅ `providers.<id>` map | ❌ 仅 `defaultProvider`/`defaultModel`/`defaultThinkingLevel` | `~/.pi/agent/auth.json`（0600）；或 `providers.<id>.apiKey` | ❌（npm 产物带 typebox `.d.ts`） |
| **OMP** | `~/.omp/agent/models.yml`（`.yaml` 回落，自动迁移 `models.json`）+ `~/.omp/agent/config.yml` | YAML | ✅ `providers.<id>` map | ✅ **10 内置角色 + 自定义角色**，`modelRoles.<role>: provider/model:level` | `providers.<id>.apiKey`：`!cmd …` 执行命令 / 同名环境变量优先 / 字面值 | ❌（npm 包内含 `src/config/*.ts` 源码） |

---

## 4. B. 模型目录即数据

### 4.1 models.dev（sst/models.dev）

**这是本次调研里最值得 SkillStar 直接采纳的东西。**

#### 数据组织

TOML 文件树，两层：

```
models/<vendor>/<model>.toml            模型本身的事实（与在哪买无关）
providers/<provider>/provider.toml      Provider 元信息
providers/<provider>/models/<id>.toml   这家 Provider 怎么卖这个模型
providers/<provider>/logo.svg
```

当前规模：**186 个 Provider 目录、6372 条 provider-model、327 条 provider-agnostic model**。

两层用 `base_model` 继承（`packages/core/src/generate.ts:156-190`）：

```toml
# providers/openai/models/gpt-5.toml
base_model = "openai/gpt-5"     # 指向 models/openai/gpt-5.toml
[cost]
input = 1.25
output = 10.00
[limit]
context = 200_000                # provider 覆盖 model 的默认值
```

合并规则明确：provider 字段覆盖 model 元数据；`benchmarks` / `license` / `links` / `weights` **不继承**（`inheritableModelMetadata()`）；`base_model_omit` 可以按路径删掉继承来的字段。

**这个两层拆分正是 SkillStar 现在缺的。** SkillStar 的 `ModelCatalogEntry` 是扁平的，无法表达"同一个 DeepSeek-V3 在官方和在某中转上，上下文和价格不同，但能力和知识截止日期相同"。

#### Schema（`packages/core/src/schema.ts`，Zod，全部 `.strict()`）

**Provider**（379-433 行）：`id` `name` `env: string[]`（至少 1 个）`npm`（必填）`api`（可选）`doc` `models: Record<Model>`。
有一条 refine 值得注意：`api` 字段**只有** `@ai-sdk/openai-compatible` / `@openrouter/ai-sdk-provider` / `merge-gateway-ai-sdk-provider`（必填）以及 `@ai-sdk/anthropic` / `@ai-sdk/openai` / `kiro-acp-ai-provider`（可选）允许出现，其它一律禁止。

**Model**（246-375 行）：

| 组 | 字段 |
| --- | --- |
| 标识 | `id` `name` `description` `family` `status`(`alpha\|beta\|deprecated`) |
| 能力 | `attachment` `reasoning` `tool_call` `structured_output` `temperature` `interleaved` `open_weights` |
| 推理控制 | `reasoning_options[]`：discriminated union，`{type:"toggle"}` / `{type:"effort", values:[none\|minimal\|low\|medium\|high\|xhigh\|max\|default\|null]}` / `{type:"budget_tokens", min, max}` |
| 时间 | `knowledge` `release_date` `last_updated`（`YYYY-MM` 或 `YYYY-MM-DD`，带闰年校验） |
| 限额 | `limit{context, input?, output}` |
| 模态 | `modalities{input[], output[]}`，枚举 `text\|audio\|image\|video\|pdf` |
| 价格 | `cost{input, output, reasoning?, cache_read?, cache_write?, input_audio?, output_audio?, tiers[]?, context_over_200k?}`，单位 USD / 1M token |
| 传输 | `provider{npm?, api?, shape?("responses"\|"completions"), body?, headers?}` |
| 实验 | `experimental.modes.<name>{cost?, provider{body,headers}}` |

模型元数据层（`ModelMetadata`）额外有 `license` `links[]{label,url,type}` `weights[]{label,url,format,quantization}` `benchmarks[]{name,score,metric,harness,variant,dataset,version,source,date}`。

**几条被 Zod refine 强制的一致性规则**（对 SkillStar 的校验层有直接参考价值）：
- `reasoning === true` 必须有 `reasoning_options`；
- `reasoning === false` 不许有 `reasoning_options`，也不许有 `cost.reasoning`；
- `cost.tiers` 的 `tier.size` 不许重复；
- Provider 的 `name` 全局唯一（大小写不敏感）。

#### 发布与消费

- **API**：`https://models.dev/api.json`（provider→models 全量，**3.71 MB**，gzip 后 **366 KB**）、`https://models.dev/models.json`（provider-agnostic，**267 KB**）、`https://models.dev/catalog.json`（两者合并，**3.80 MB**）、`https://models.dev/logos/{provider}.svg`、`https://models.dev/model-schema.json`（动态生成的 `provider/model` 字符串枚举，`packages/function/src/worker.ts:66-102`）。
- **更新节奏**：GitHub Actions `sync-models.yml` **每小时** cron（`17 * * * *`）跑一遍各 Provider 的同步脚本，配 `sync:auto-merge` 自动合 PR。实测最近 100 个 commit 分布在 **2 天**内——极高频。
- **部署**：push 到 `dev` 分支即部署（`deploy.yml`），SST + Cloudflare Workers。
- **官方 SDK**：npm `@opencode-ai/models`（`packages/sdk`）。
- **消费方**：OpenCode（内建，`OPENCODE_MODELS_PATH` 可指向本地 `_api.json` 做测试）；README 明说 "We also use it internally in opencode"。

**注意仓库归属**：workflow 里的 `if: github.repository == 'anomalyco/models.dev'`，SDK 的 repository URL 也是 `github.com/anomalyco/models.dev`——上游主体已经从 `sst` 迁到 `anomalyco`。SkillStar 如果要 pin 数据源，pin `https://models.dev/api.json` 这个域名比 pin GitHub repo 稳。

### 4.2 LiteLLM `model_prices_and_context_window.json`

- **URL**：`https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json`
- **规模**：3020 条，1.72 MB，gzip 后 **83 KB**（比 models.dev 小一个数量级，因为它只有价格和 token 数，没有 benchmark/描述/modalities 细节）。
- **结构**：单层扁平 map，key 是模型名（有时带 provider 前缀，如 `deepseek/deepseek-reasoner`）。**没有 provider 维度**——`litellm_provider` 只是一个字符串标签，同一个模型在不同中转的不同价格无法表达。这是它相对 models.dev 的核心短板。
- **字段**（`sample_spec` 条目自带文档）：`max_tokens`(legacy) `max_input_tokens` `max_output_tokens` `input_cost_per_token` `output_cost_per_token` `output_cost_per_reasoning_token` `input_cost_per_audio_token` `cache_read_input_token_cost` `cache_creation_input_token_cost` `litellm_provider` `mode`(`chat|embedding|completion|image_generation|...`) `deprecation_date` `supported_regions[]` `supported_endpoints[]` `supported_modalities[]` `supports_function_calling` `supports_vision` `supports_reasoning` `supports_prompt_caching` `supports_response_schema` `supports_web_search` `supports_{minimal,xhigh,none}_reasoning_effort` …
- **更新节奏**：该文件最近 100 次提交跨越 **51 天**、落在 **37 个不同的日子**——大约每天 2 次。
- **消费方式（Aider 的做法值得抄）**：`aider/models.py:161-171`

  ```python
  class ModelInfoManager:
      MODEL_INFO_URL = "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json"
      CACHE_TTL = 60 * 60 * 24        # 24 小时
      cache_dir = Path.home() / ".aider" / "caches"
  ```

  远程拉 → 落 `~/.aider/caches/` → 24h TTL → 叠加内置 `aider/resources/model-metadata.json` → 再叠加用户的 `.aider.model.metadata.json`。四层，逐层覆盖。

### 4.3 额外发现 · Catwalk（Crush 的目录）

Crush 不用 models.dev，用 charmbracelet 自己的 **catwalk**（`charm.land/catwalk` Go 模块，服务在 `https://catwalk.charm.land`，端点 `/v2/providers`）。它的三级回退结构是本文推荐 SkillStar 采纳的样板（`internal/config/provider.go:155-245`）：

`embedded`（编译进二进制的快照，`charm.land/catwalk/pkg/embedded`）→ `cache`（`$XDG_DATA_HOME/crush/providers.json`，带 etag）→ `remote`（`CATWALK_URL`，默认 `https://catwalk.charm.land`，45s 超时）。

源码注释把降级语义写得非常清楚，值得逐字借鉴：

> A returned error is advisory: it reports that the catalog could not be cached, or that an upstream returned nothing usable. **It never means that no providers are available**, so callers should surface it as a warning and keep using the returned list. A refresh that simply could not reach the network **is not an error at all**.

还有开关 `CRUSH_DISABLE_PROVIDER_AUTO_UPDATE=1` / `options.disable_provider_auto_update`，以及手动命令 `crush update-providers`（可传本地文件路径或 `embedded`）。

### 4.4 额外发现 · 各家的"发现层"

除了目录，几乎每家都还有一个"从端点直接发现"的通道，SkillStar 的新模型应该把这两者当成**两个不同的数据来源**而不是一个：

| 工具 | 发现机制 |
| --- | --- |
| Crush | `providers.<id>.discover_models`（默认 true），从 `/v1/models` 拉，与显式 `models` 合并且**用户写的赢** |
| OMP | `providers.<id>.discovery.type`：`ollama` / `llama.cpp` / `lm-studio` / `openai-models-list` / `proxy` / `litellm`，结果缓存进 `models.db`（带 `authoritative` 标志） |
| Pi | `models-store.json`，per-provider 带 `checkedAt` / `lastModified` / `etag` |
| Claude Code | `CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY=1`，从网关 `/v1/models` 发现 |
| Codex | `model_catalog_json` 指向本地 JSON 文件（**只在启动时读一次**） |

**共同点**：发现出来的模型只有 id 和名字，**没有价格、上下文窗口、能力位**。所以"目录"和"发现"是互补的，不是二选一——发现回答"有哪些模型"，目录回答"这个模型是什么样的"。

### 4.5 评估：SkillStar 该怎么选

结论：**三者都要，但职责分明。**

| 层 | 数据来源 | 更新方式 | 回答什么问题 |
| --- | --- | --- | --- |
| L0 内置快照 | 构建时冻结的 models.dev `api.json` 子集 | 随 SkillStar 版本发布 | 离线可用、首次启动即有数据 |
| L1 运行时目录 | `https://models.dev/api.json`（走 `probe_http_client`，尊重用户代理配置） | 带 ETag 的按需刷新，失败静默回落 L0 | 价格、上下文、能力、模态、推理档位 |
| L2 端点发现 | Provider 自己的 `/v1/models` | 用户点"刷新模型"时 | 这家中转**实际**开了哪些模型 |

**三条都不能单独成立**：只靠 `/v1/models` —— Crush 的 `Model` schema 有 10 个 required 字段（含价格和上下文窗口），`/v1/models` 一个都给不出来，写盘直接失败；OpenCode 也明说自定义 Provider 不写 `limit` 就没法算剩余上下文。只靠内置快照 —— models.dev 两天 100 个 commit、LiteLLM 每天 2 次，任何冻结快照在一个发布周期内就过时。只靠运行时拉 —— SkillStar 是桌面应用，用户可能离线/内网/代理受限，且 3.7 MB 首次拉取拖慢首屏。

**选 models.dev 而不是 LiteLLM 的三个理由**：
1. models.dev 有 **provider × model 二维**（同一模型在不同中转的不同价格/上下文），LiteLLM 是一维扁平的——而 SkillStar 的核心场景恰恰是"同一个模型，我从三家中转都能买到"。
2. models.dev 有 `reasoning_options` 的结构化表达（toggle / effort / budget_tokens），能直接投影到 OMP 的 `thinkingLevelMap`、Crush 的 `reasoning_levels`、Codex 的 `supported_reasoning_efforts`。LiteLLM 只有零散的 `supports_*_reasoning_effort` 布尔位。
3. models.dev 有 `modalities`、`limit.input/output` 分离、`cost.tiers` 分层定价——这三样在写 OpenCode / Crush / OMP 时都用得上。

**LiteLLM 的定位建议**：作为**补漏来源**。它有 3020 条而 models.dev 有 6372 条 provider-model，但两边覆盖的长尾不完全重合；且 LiteLLM gzip 只有 83 KB，作为兜底代价很低。

**体积决策**：内置快照不要塞 3.7 MB 全量。建议只冻结 SkillStar 预置 Provider 相关的子集 + `models.json`（267 KB，provider-agnostic 部分，这部分是"模型本身的事实"，最稳定、最值得内置）。

---

## 5. C. 路由与角色：最小公共角色抽象

### 5.1 各家角色表达一览

| 工具 | 表达方式 | 角色集合 | 模型引用形状 |
| --- | --- | --- | --- |
| OpenCode | `model` / `small_model` / `agent.<name>.model` | 主 + 小 + 任意命名 agent | `provider/model` |
| Codex | `model` / `review_model` / `agents.default_subagent_model` / `agents.<role>.config_file` / `profiles.<name>` | 主 + review + subagent + 任意命名角色（整层配置） | `model` 字符串 + 独立的 `model_provider` |
| Claude Code | `ANTHROPIC_MODEL` / `ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU,FABLE}_MODEL` / `CLAUDE_CODE_SUBAGENT_MODEL` / `advisorModel` | 主 + 4 个"档位" + subagent + advisor | 模型 ID 字符串 |
| Crush | `models.large` / `models.small` | 仅 2 个 | `{provider, model, reasoning_effort?, think?}` |
| Aider | `--model` / `--weak-model` / `--editor-model`；per-model 的 `weak_model_name` / `editor_model_name` | 主 + 弱 + 编辑器 | 模型名字符串 |
| Pi | `defaultProvider` + `defaultModel` | 仅 1 个 | provider 与 model 分开两个字段 |
| OMP | `modelRoles.<role>` | 10 内置 + 任意自定义 | `provider/model:thinkingLevel` |

### 5.2 关键观察

**观察 1：Claude Code 的"档位"不是角色。** `ANTHROPIC_DEFAULT_OPUS_MODEL` 不是"用 Opus 干什么"，而是"当有人要求 Opus 时，实际用哪个模型 ID"。它是一个**别名映射**，语义上更接近 Aider 的 `--alias ALIAS:MODEL`。把它当角色投影会失真。正确的投影是：SkillStar 的 `default` 角色 → `ANTHROPIC_MODEL`，`fast` 角色 → `ANTHROPIC_DEFAULT_HAIKU_MODEL`（因为 Claude Code 内部把 Haiku 档当作背景/低复杂度任务的槽位，schema 原文："Haiku-class model to use for background and low-complexity tasks"）。

**观察 2：角色数量差 10 倍，但语义交集很小。** OMP 的 10 个角色里，能在另外至少两家找到对应物的只有 4 个：`default`、`smol`(fast)、`plan`、`vision`。`designer` / `commit` / `tiny` / `task` / `advisor` 都是 OMP 独有或近乎独有的。

**观察 3：`slow` 是个陷阱。** OMP 有 `default` 和 `slow` 两个角色，本机配置里两者指向同一个模型。`slow` 表达的是"深思档"，但在其它工具里这不是一个模型选择问题，而是一个**推理档位**问题（Codex 的 `model_reasoning_effort`、Crush 的 `reasoning_effort`、OMP 自己的 `:max` 后缀）。**不应该把它建模成角色**，应该建模成"角色 + 推理档位"这个二元组里的第二个分量。

**观察 4：模型引用必须是三元组。** OMP 的 `provider/model:level`、Crush 的 `{provider, model, reasoning_effort}`、Codex 的 `model` + `model_provider` + `model_reasoning_effort`——三家都在传同样的三个东西。任何只传 `provider/model` 的抽象都会在写盘时丢掉档位。

### 5.3 提议的最小公共角色抽象

```
Role ::= "default" | "fast" | "plan" | "vision" | "subagent"
ModelRef ::= { provider_id, model_id, effort? }
RoleMap ::= Map<Role, ModelRef>
```

**只有 `default` 是必填**，其余四个缺省时按 `fast → default`、`plan → default`、`vision → default`、`subagent → fast ?? default` 回落（这个回落链和 Crush 的 `small → large`、OpenCode 的 `small_model` 缺省行为一致）。

**为什么是这五个**：

| 角色 | 出现在 | 不选它的代价 |
| --- | --- | --- |
| `default` | 全部 7 家 | 无法工作 |
| `fast` | OpenCode(`small_model`)、Crush(`small`)、Claude Code(`DEFAULT_HAIKU`)、Aider(`--weak-model`)、OMP(`smol`) —— 5 家 | 5 家降级到单模型 |
| `plan` | OMP(`plan`)、OpenCode(`agent.plan.model`)、Codex(`plan_mode_reasoning_effort` 至少有 plan 概念) —— 3 家 | 3 家丢失规划模型 |
| `vision` | OMP(`vision`) —— 1 家，但**这是能力驱动的路由**，不是偏好 | 多模态请求会打到不支持图片的模型上 |
| `subagent` | Codex(`agents.default_subagent_model`)、Claude Code(`CLAUDE_CODE_SUBAGENT_MODEL`)、OpenCode(`agent.<name>.model`) —— 3 家 | 3 家的子 agent 全部用主模型，成本失控 |

**为什么不加 `commit` / `designer` / `tiny` / `task` / `advisor` / `editor`**：这些各只有 1 家支持。做成一等公民会让 UI 出现 5 个对 6/7 的工具完全无意义的开关。正确做法是留一个**逃生舱**：

```
RoleMap 允许携带 extra: Map<String, ModelRef>
```

投影时，`extra` 里的 key 只会写给"支持任意命名角色"的目标（OMP 的 `modelRoles`、OpenCode 的 `agent.<name>.model`、Codex 的 `agents.<role>`），其它目标静默忽略。这样既不污染主 UI，也不丢用户已有的配置。

**为什么 `effort` 挂在 `ModelRef` 上而不是全局**：OMP 的 `slow: .../deepseek-v4-pro:max` 和 `smol: .../deepseek-v4-flash:xhigh` 证明了同一份配置里不同角色需要不同档位。全局 `defaultThinkingLevel` 只是缺省值。

**`effort` 的取值域**：取 models.dev 的 `ReasoningEffortValue` 枚举（`none|minimal|low|medium|high|xhigh|max|default|null`）作为内部规范值，因为它是最宽的超集。投影时按目标收窄：Crush 只认 `low|medium|high`（超出的向下取最近值），OMP 的 `reasoningEffortMap` 可以做任意重映射，Codex 的 `ReasoningEffort` 是自由字符串（schema 里只要求 `minLength: 1`）。

---

## 6. D. 凭据与安全

### 6.1 各家的 key 存放方式

| 工具 | 明文文件 | 环境变量 | 命令/脚本 | 系统 keychain |
| --- | --- | --- | --- | --- |
| OpenCode | `~/.local/share/opencode/auth.json`（**0600**）；配置内 `options.apiKey` | `{env:VAR}` 插值 | `{file:path}` 插值 | ❌ |
| Codex | `~/.codex/auth.json`（仅 `OPENAI_API_KEY`）；`experimental_bearer_token`（源码标注 discouraged） | `env_key` = 变量名 | `auth.command` + `args`/`cwd`/`timeout_ms`/`refresh_interval_ms` | ✅ `cli_auth_credentials_store = "keyring"\|"auto"` |
| Claude Code | `settings.json` 的 `env.ANTHROPIC_AUTH_TOKEN` / `ANTHROPIC_API_KEY` | 同左（env 块就是环境变量） | `apiKeyHelper` + `CLAUDE_CODE_API_KEY_HELPER_TTL_MS` | ❌（OAuth 走 keychain，但第三方 key 不走） |
| Crush | `providers.<id>.api_key` | `$VAR` / `${VAR:-d}` / `${VAR:?msg}` | `$(command)`（完整 shell） | ❌ |
| Aider | 不存 | `.env` / `~/.aider/oauth-keys.env` / `PROVIDER_API_KEY` | ❌ | ❌ |
| Pi | `~/.pi/agent/auth.json`（**0600**）；`providers.<id>.apiKey` | ❌（未见） | ❌ | ❌ |
| OMP | `providers.<id>.apiKey` 字面值 | **同名环境变量优先于字面值** | `!cmd <command>` 取 stdout | ❌ |

### 6.2 env 与配置文件的优先级

三种不同的语义，SkillStar 必须区分对待：

- **Codex**：`env_key` 里存的是**变量名**，不是值。Codex 运行时 `std::env::var(env_key)`，取不到就报 `EnvVarError` 并附带 `env_key_instructions` 引导用户。**配置文件永远不含 key**。
- **OMP**：`apiKey` 的值先当环境变量名查，查不到才当字面值（`resolveConfigValue`）。**同一个字段两种语义，靠"环境变量是否存在"来判**——这是个容易出事的设计，SkillStar 写字面 key 时应避免使用全大写下划线形式的值。
- **Claude Code / Crush / Pi / OpenCode**：配置里就是值（OpenCode 和 Crush 额外支持插值/展开语法）。

### 6.3 最小惊讶原则（SkillStar 写盘时应遵守的）

1. **不降低已有文件的权限。** OpenCode 和 Pi 的 auth.json 是 0600；SkillStar 重写时必须保持，不能因为用 `create_dir_all` + 默认 umask 就变成 0644。
2. **不把 key 写进用户可能提交到 git 的文件。** 具体地：Crush 的项目级 `crush.json`、Aider 的 `.aider.conf.yml`、OpenCode 的项目 `opencode.json` 都可能在仓库里。SkillStar 只写全局路径（现在已经是这样，保持）。
3. **优先用间接引用。** 能写 `{env:VAR}`（OpenCode）、`$VAR`（Crush）、`!cmd`（OMP）、`env_key`（Codex）的地方，就不要写明文。这条现在 SkillStar 只在 Codex 上做到了。
4. **不覆盖非自己管理的键。** SkillStar 的 `skillstar_<id8>` 前缀 + `is_skillstar_managed_key` 是正确做法，继续保持；但要注意 Codex 的 `model_provider`、OpenCode 的 `model`、OMP 的 `modelRoles.default` 这三个**指针字段是共享的**，改之前必须确认当前值确实指向 SkillStar 管理的条目（现有代码 `multi_provider.rs:292-296` 已经这么做了）。
5. **不静默劫持凭据通道。** Codex 的 `auth.json` 同时存 ChatGPT OAuth token 和 API key。SkillStar 现在的 `third_party` auth mode 走 `env_key` 而不碰 `auth.json`，正好保住了并存的 OAuth 登录——这个决定是对的，应该写进决策记录。
6. **告知优先级劣势。** Claude Code 的 `~/.claude/settings.json` 是**最低优先级**；项目级 `.claude/settings.json` 会盖住它。SkillStar 应该在绑定后检测项目级文件是否存在冲突的 `env.ANTHROPIC_BASE_URL`，并提示用户，而不是让用户以为绑定没生效。
7. **写 shell 可展开字段时要转义。** Crush 的 `api_key` 走完整 shell 展开。如果用户的 key 里含 `$` 或反引号（少见但可能），直接写字面值会被展开成别的东西。要么单引号包裹，要么强制走 `$VAR` 间接引用。

---

## 7. SkillStar 新数据模型的下界要求

### 7.1 字段级最小结构提案

```rust
// 一个可写盘的 Provider。
struct Provider {
    id: ProviderId,                  // 稳定 ID，用于派生 skillstar_<id8> 管理键
    name: String,
    endpoints: Endpoints,
    credential: Credential,
    headers: BTreeMap<String, String>,
    capabilities: ProviderCaps,      // 新增，当前缺失
    models: Vec<ModelEntry>,
}

// 一个 Provider 可能同时暴露多种协议端点。当前 SkillStar 只有 openai/anthropic 两个 URL 字段。
struct Endpoints {
    openai_chat: Option<Url>,        // /v1/chat/completions
    openai_responses: Option<Url>,   // /v1/responses  ← Codex ≥0.95 的唯一入口
    anthropic_messages: Option<Url>, // /v1/messages   ← Claude Code 的唯一入口
}
struct ProviderCaps { supports_responses_api: Tri, supports_models_list: Tri }  // Yes/No/Unknown，可探测

enum Credential {
    Literal(SecretString),                            // 明文写进配置或 auth.json
    EnvVar { name: String },                          // Codex env_key / OpenCode {env:} / Crush $VAR / OMP 同名 env
    File { path: PathBuf },                           // OpenCode {file:}
    Command { command: String, args: Vec<String> },   // Codex auth.command / OMP !cmd / Claude apiKeyHelper
    Oauth { /* 保留现状 */ },
}

// 模型条目 = 「模型本身的事实」+「这家 Provider 怎么卖它」，对应 models.dev 的两层拆分。
struct ModelEntry {
    id: String,                      // Provider 侧模型 ID，写盘用这个
    display_name: String,
    base_model: Option<String>,      // 指向共享的 ModelFacts，用于继承
    serving: Serving,
    facts: ModelFacts,
}

struct ModelFacts {
    family: Option<String>, knowledge_cutoff: Option<Date>, release_date: Option<Date>,
    modalities_in: Vec<Modality>, modalities_out: Vec<Modality>,   // text|image|audio|video|pdf
    tool_call: bool, attachment: bool,
    structured_output: Option<bool>, temperature: Option<bool>,
    reasoning: Reasoning, open_weights: bool,
    status: Option<ModelStatus>,     // alpha | beta | deprecated
}

enum Reasoning {
    None,
    Toggle,
    Effort { values: Vec<Effort> },                          // none|minimal|low|medium|high|xhigh|max|default
    BudgetTokens { min: Option<u32>, max: Option<u32> },
}

struct Serving {
    context: u64,                    // 必填 —— Crush required、OpenCode 需要
    max_input: Option<u64>,
    max_output: u64,                 // 必填 —— Crush required
    cost: Cost,                      // 必填 —— Crush required（4 个价格字段）
    wire_shape: WireShape,           // Responses | Completions | AnthropicMessages
    extra_body: Option<Json>,
    extra_headers: BTreeMap<String, String>,
}

struct Cost {                        // 单位 USD / 1M token，与 models.dev 一致
    input: f64, output: f64,
    cache_read: Option<f64>, cache_write: Option<f64>, reasoning: Option<f64>,
    tiers: Vec<CostTier>,            // { above_input_tokens, input, output, cache_read, cache_write }
}

// 角色路由 —— §5.3 的最小公共抽象。
struct ModelRef { provider_id: ProviderId, model_id: String, effort: Option<Effort> }
struct RoleMap {
    default: ModelRef,                          // 必填
    fast: Option<ModelRef>, plan: Option<ModelRef>,
    vision: Option<ModelRef>, subagent: Option<ModelRef>,
    extra: BTreeMap<String, ModelRef>,          // 逃生舱，只投给支持任意角色的目标
}

// 一次绑定 = 一组 Provider + 一份角色表。
struct Binding { providers: Vec<ProviderId>, roles: RoleMap }
```

**相对当前 SkillStar 数据模型的四个增量**：

1. `ModelEntry` 拆成 `facts` + `serving` 两层（现在 `ModelCatalogEntry` 是扁平的）。
2. `Endpoints` 增加 `openai_responses` 与 `ProviderCaps.supports_responses_api`（现在只有 `base_url_openai` / `base_url_anthropic`，无法回答"这家能不能接 Codex"）。
3. `Credential` 从"字符串 + auth_mode 枚举"升级成带变体的枚举（现在 `codex_auth_mode` 是 Codex 专属的字符串字段，没法复用到 OMP 的 `!cmd` 或 Claude 的 `apiKeyHelper`）。
4. `RoleMap` 是全新的（现在只有单个 active model 指针）。

**同时应删除的**：`codex_wire_api` 字段。它编码的是一个已经消失的选择——Codex 只剩 `responses`。应该被 `Endpoints.openai_responses` 的存在与否取代，因为真正的问题从来不是"我要告诉 Codex 用哪个协议"，而是"这家 Provider 支不支持 Codex 要求的协议"。

### 7.2 逐 Agent 无损投影论证

**OpenCode** —— 全字段命中，无损。
`Provider.name` → `provider.<key>.name`；`endpoints.openai_chat` → `options.baseURL`（`npm = "@ai-sdk/openai-compatible"`）；有 `openai_responses` 时改 `npm = "@ai-sdk/openai"`；`credential` 的 Literal→`options.apiKey`、EnvVar→`options.apiKey = "{env:NAME}"`、File→`"{file:path}"`（Command 变体降级为 Literal，先执行再写，或提示不支持）；`headers` → `options.headers`；`serving.context/max_output` → `models.<id>.limit.context/output`；`cost` → `models.<id>.cost`（字段名完全一致：`input`/`output`/`cache_read`/`cache_write`/`context_over_200k`）；`facts` 的 `attachment`/`reasoning`/`tool_call`/`temperature`/`family`/`release_date`/`modalities`/`status` → 同名字段（**OpenCode 的 Model schema 与 models.dev 的字段名一一对应**，这不是巧合，OpenCode 就是 models.dev 的主要消费方）；`roles.default` → `model = "<key>/<id>"`；`roles.fast` → `small_model`；`roles.plan`/`vision`/`subagent`/`extra` → `agent.<role>.model`。
**唯一损失**：`Reasoning::BudgetTokens` 的 min/max 无处安放（OpenCode Model schema 没有对应字段），可放进 `models.<id>.options`。

**Codex** —— 有损，且损失是外部造成的。
`Provider` → `[model_providers.skillstar_<id8>]`：`name`、`endpoints.openai_responses` → `base_url`、`credential::EnvVar` → `env_key`、`credential::Command` → `auth.command`/`args`、`headers` → `http_headers`、`wire_api = "responses"`（恒定）。`roles.default` → 顶层 `model` + `model_provider`；`ModelRef.effort` → `model_reasoning_effort`；`roles.subagent` → `agents.default_subagent_model`（+ `default_subagent_reasoning_effort`）；`roles.extra.<name>` → `agents.<name>`。
**结构性损失**：
- `serving.cost` / `facts.modalities` / `facts.tool_call` 等**在 config.toml 里没有位置**。想投影必须走 `model_catalog_json`：SkillStar 生成一个 `ModelsResponse` JSON 写到自己的数据目录，再让 `config.toml` 的 `model_catalog_json` 指过去。这是可行的且是 Codex 官方通道，但要注意它**只在启动时读**。
- `roles.fast` / `roles.plan` / `roles.vision` 没有直接对应。`fast` 可以借 `agents.<role>` 表达（每个角色一个 config 分层文件），但语义不完全等价。
- **`endpoints.openai_responses == None` 的 Provider 根本不能投影到 Codex。** 这不是 SkillStar 的建模缺陷，是 Codex 删掉 chat/completions 造成的既成事实。新数据模型的正确反应是：在 UI 上把这类 Provider 的 "Codex" 目标标灰，并说明原因，而不是写一个会让 Codex 起不来的配置。
- **建议同时输出 profile**：为每个绑定的 Provider 额外生成 `[profiles.skillstar_<id8>]`（含 `model` / `model_provider` / `model_reasoning_effort`），用户可 `codex --profile skillstar_<id8>` 切换。比只维护一个全局指针干净得多。

**Claude Code** —— 有损但可接受。
`endpoints.anthropic_messages` → `env.ANTHROPIC_BASE_URL`；`credential::Literal` → `env.ANTHROPIC_AUTH_TOKEN`，`credential::Command` → 顶层 `apiKeyHelper`；`headers` → `env.ANTHROPIC_CUSTOM_HEADERS`（**注意是换行分隔的 `Name: Value`，不是 JSON**）；`roles.default` → `env.ANTHROPIC_MODEL`；`roles.fast` → `env.ANTHROPIC_DEFAULT_HAIKU_MODEL`（**不要再写 `ANTHROPIC_SMALL_FAST_MODEL`**）；`roles.subagent` → `env.CLAUDE_CODE_SUBAGENT_MODEL`；`serving.context` → `env.CLAUDE_CODE_MAX_CONTEXT_TOKENS`，`serving.max_output` → `env.CLAUDE_CODE_MAX_OUTPUT_TOKENS`。
额外可选：把非 default 的模型通过 `ANTHROPIC_CUSTOM_MODEL_OPTION` + `_NAME` + `_DESCRIPTION` + `_SUPPORTED_CAPABILITIES` 送进模型选择器；`facts.modalities` 可编码进 `_SUPPORTED_CAPABILITIES`（JSON 对象）。
**结构性损失**：`cost`、`plan`/`vision` 角色、多 Provider 并存——Claude Code 只有一组全局 env，一次只能绑一个。这是 `AgentKind::Single` 的正确理由。
**新增要求**：投影后应检测项目级 `.claude/settings.json` 是否也设了 `env.ANTHROPIC_BASE_URL`，有则告警（它优先级更高）。

**Crush** —— 全字段命中，且是最严格的验收者。
`Provider` → `providers.skillstar_<id8>`：`name`、`endpoints.openai_chat` → `base_url`、`type = "openai-compat"`（有 responses 端点时可用 `"openai"`）、`credential::EnvVar` → `api_key = "$NAME"`、`credential::Command` → `api_key = "$(cmd)"`、`headers` → `extra_headers`、`serving.extra_body` → `extra_body`。
`ModelEntry` → `models[]`，**10 个 required 全部由本提案覆盖**：`id`←id、`name`←display_name、`cost_per_1m_in`←cost.input、`cost_per_1m_out`←cost.output、`cost_per_1m_in_cached`←cost.cache_write（注意语义是"写缓存"）、`cost_per_1m_out_cached`←cost.cache_read、`context_window`←serving.context、`default_max_tokens`←serving.max_output、`can_reason`←facts.reasoning≠None、`supports_attachments`←facts.attachment。另有 `reasoning_levels`←Reasoning::Effort.values、`default_reasoning_effort`。
`roles.default` → `models.large`，`roles.fast` → `models.small`，`ModelRef.effort` → `SelectedModel.reasoning_effort`（**需收窄到 `low|medium|high`**）。
**损失**：`plan`/`vision`/`subagent`/`extra` 无处安放（Crush 只有两档）；`modalities` 只能压成 `supports_attachments` 一个布尔。
**这条投影是最好的可行性检验**：如果 SkillStar 的数据模型能满足 Crush 的 10 个 required 字段，那它对其它所有目标都是够用的。建议把"能生成合法 Crush 配置"作为新模型的验收测试。

**Aider** —— 有损，但覆盖了它能表达的全部。
`endpoints.openai_chat` → `.aider.conf.yml` 的 `openai-api-base`（或 `.env` 的 `OPENAI_API_BASE`）；`credential` → `.env` / `--api-key provider=KEY`；`roles.default` → `model`，`roles.fast` → `weak-model`；`extra["editor"]` → `editor-model`。
`ModelEntry` → `.aider.model.metadata.json`（**LiteLLM 格式**）：`max_input_tokens`←serving.max_input??context、`max_output_tokens`←serving.max_output、`input_cost_per_token`←cost.input/1e6、`output_cost_per_token`←cost.output/1e6、`cache_read_input_token_cost`←cost.cache_read/1e6、`litellm_provider`←provider_id、`mode: "chat"`、`supports_function_calling`←facts.tool_call、`supports_vision`←modalities_in 含 image、`supports_reasoning`←facts.reasoning≠None。
`facts` 的行为部分 → `.aider.model.settings.yml`（`weak_model_name`、`editor_model_name`、`use_temperature`、`streaming`）。
**注意单位换算**：models.dev / Crush 用 USD/1M token，LiteLLM 用 USD/token，差 1e6。这是唯一需要单位转换的目标，容易出 bug，应该有测试。

**Pi** —— 全字段命中（Pi 的 schema 比本提案还宽）。
`Provider` → `models.json` 的 `providers.skillstar_<id8>`：`name`/`baseUrl`/`apiKey`/`api`/`headers`/`authHeader`/`compat`。`api` 取值需映射：有 responses 端点→（Pi 的 `api` 是自由字符串，取 `openai-responses`），否则 `openai-completions`。
`ModelEntry` → `models[]`：`id`/`name`/`reasoning`/`input`(←modalities_in 过滤成 text|image)/`cost{input,output,cacheRead,cacheWrite,tiers}`（**`tiers` 用 `inputTokensAbove` 键，与本提案的 `above_input_tokens` 只差命名**）/`contextWindow`/`maxTokens`/`headers`/`compat`。
`Reasoning::Effort` → `thinkingLevelMap`（把 `off|minimal|low|medium|high|xhigh|max` 映射到 Provider 侧的实际取值，`null` 表示该档不支持）。
`roles.default` → `settings.json` 的 `defaultProvider` + `defaultModel`，`ModelRef.effort` → `defaultThinkingLevel`。
**损失**：`fast`/`plan`/`vision`/`subagent` 全部丢失（Pi 无角色系统）。这是可接受的降级，不是建模错误。

**OMP** —— 全字段命中，且是唯一能吃下完整 `RoleMap`（含 `extra`）的目标。
`Provider` → `models.yml` 的 `providers.skillstar_<id8>`：`baseUrl`/`apiKey`/`api`/`headers`/`authHeader`/`auth`/`compat`/`models[]`/`modelOverrides`。`api` 必须取自那 9 个枚举值之一（`openai-completions` / `openai-responses` / `anthropic-messages` / …）——本提案的 `WireShape` 三值是它的子集，映射直接。
**必须满足 OMP 的写盘前校验**（§2.7）：有 `models` 就必须有 `baseUrl`；有 `models` 且 `auth != "none"` 就必须有 `apiKey`；每个 model 必须有 `api`。**这三条应该成为 SkillStar 侧的前置校验**，不要等 OMP 启动时才报错。
`ModelEntry` → `models[]`：`id`/`name`/`api`/`baseUrl`/`reasoning`/`thinking`/`input`/`cost`/`contextWindow`/`maxTokens`/`headers`/`compat`。
`RoleMap` → `config.yml` 的 `modelRoles`：`default`→`default`、`fast`→`smol`、`plan`→`plan`、`vision`→`vision`、`subagent`→`task`、`extra.<k>`→`<k>`。值的形状是 **`skillstar_<id8>/<model_id>:<effort>`**，`effort` 缺省时省略后缀。
**当前实现的差距**：SkillStar 的注释只提到 `default`/`slow`/`smol` 三个角色且没有 `:level` 后缀。`slow` 按 §5.2 观察 3 不应该是角色，应由 `default` + 高 effort 表达；而 `:level` 后缀丢失会让用户的思考档位被静默降级。

### 7.3 一致性小结

| 目标 | 能吃下本提案的比例 | 主要损失 |
| --- | --- | --- |
| OpenCode | ~95% | BudgetTokens 的 min/max |
| Crush | ~85% | plan/vision/subagent 角色；modalities 压成布尔 |
| OMP | ~95% | 无实质损失 |
| Pi | ~80% | 全部非 default 角色 |
| Codex | ~55% | cost/modalities（需 `model_catalog_json` 旁路）；fast/plan/vision；**不支持 responses 的 Provider 完全不可投影** |
| Claude Code | ~45% | cost；plan/vision；多 Provider |
| Aider | ~50% | 无 provider 概念；需单位换算 |

**下界的意义**：本提案的每一个字段，都至少有两个目标会真的用到它。没有一个字段是为了对称而存在的。反过来，去掉任何一个字段，都会让至少一个目标的写盘从"完整"降级为"能跑但功能缺失"。

### 7.4 已知会炸的三处

按严重程度排序，这三处是本次调研发现的、SkillStar 当前代码与外部现实之间的实际冲突：

1. **`wire_api = "chat"`（严重）** —— `crates/skillstar-models/src/providers/crud.rs:22-28` 的 `recommended_codex_defaults()` 对所有非 `api.openai.com` 的 URL 返回 `"chat"`。Codex ≥ 0.95.0（2026-02-04 起）对该值返回反序列化错误，**整个 `config.toml` 解析失败，Codex 无法启动**。影响面是"所有第三方 Provider × 所有升级过 Codex 的用户"。修复不只是改字符串——见 §7.2 Codex 段的结构性论述。
2. **`ANTHROPIC_SMALL_FAST_MODEL`（轻微）** —— 官方 schema 标注 "DEPRECATED (prefer `ANTHROPIC_DEFAULT_HAIKU_MODEL`)"。目前仍然生效，但应迁移。
3. **OMP 角色与档位（中等）** —— `paths_files.rs:93-95` 的注释只覆盖 `default`/`slow`/`smol` 三个角色，实际有 10 个内置 + 任意自定义；且模型引用的 `:thinkingLevel` 后缀未被处理，写盘会静默丢掉用户的思考档位。

---

## 8. 未解决 / 需要后续验证的

1. **Codex 的 `model_catalog_json` 是否接受 SkillStar 生成的最小 `ModelsResponse`。** `ModelInfo` 有 40 余字段，其中多少是 required 需要实测（schema 里 `#[serde(default)]` 很多，但没有全部标注）。建议写一个最小 JSON 实际喂给 Codex 验证。
2. **Claude Code 接非 Anthropic 协议的中转的实际成功率。** 官方文档明确不支持；实际取决于中转是否实现 Anthropic Messages 协议。SkillStar 的 UI 应该基于探测结果而不是假设。
3. **Pi 和 OMP 的下一个版本。** 两者都不公开仓库，本文结论只对 `pi@0.84.1` / `omp@17.3.2` 有效。建议 SkillStar 在写盘前用 schema 校验（OMP 的校验规则已在 §2.7 列出），失败时给出可读错误而不是写坏文件。
4. **`ANTHROPIC_CUSTOM_MODEL_OPTION_SUPPORTED_CAPABILITIES` 的 JSON 结构。** schema 只说 "JSON object specifying capability flags"，没给字段名。需要实测或找到更详细的文档。
5. **Aider 是否还值得维护写盘路径。** 最后提交 2026-05-22，约 3 个月无更新。这是产品决策，不是技术判断。
6. **models.dev 的仓库归属迁移。** workflow 里是 `anomalyco/models.dev`，README 和 clone URL 还是 `sst/models.dev`。如果 SkillStar 要 pin 数据源，建议 pin `https://models.dev/api.json` 域名而非 GitHub raw。

---

## 附：本文引用的关键文件

- **OpenCode**（`/tmp/model-research/opencode`）：Provider schema `packages/core/src/v1/config/provider.ts:82-126`；Config（`model`/`small_model`/`provider`）`packages/core/src/v1/config/config.ts:74-79,110-112`；Agent schema `.../agent.ts:12-40`；配置优先级 `packages/web/src/content/docs/config.mdx:44-56` 与 `packages/opencode/src/config/config.ts:250-520`；自定义 Provider 文档 `packages/web/src/content/docs/providers.mdx:2431-2555`；auth.json 0600 `packages/opencode/src/provider/auth.ts:11,78-80`；schema 生成器 `packages/opencode/script/schema.ts`。
- **Codex**（`/tmp/model-research/codex`）：`ModelProviderInfo` `codex-rs/model-provider-info/src/lib.rs:87-146`；`wire_api` 只剩 responses `同文件:54-85`；内置 Provider 不可覆盖 `同文件:288-325`；官方 schema `codex-rs/core/config.schema.json`（生成说明 `codex-rs/core/src/config/schema.md`）；`auth.json` `codex-rs/login/src/auth/storage.rs:38-60`；`model_catalog_json` `codex-rs/core/src/config/mod.rs:2028-2056`；`ModelInfo`/`ModelPreset` `codex-rs/protocol/src/openai_models.rs:216-262,385-470`；chat/completions 删除时间线 `gh api repos/openai/codex/releases`（`rust-v0.72.0` #7897、`rust-v0.95.0` #10157/#10498）。
- **Claude Code**：settings schema `https://json.schemastore.org/claude-code-settings.json`（本地副本 `/tmp/model-research/claude-code-settings.schema.json`）；优先级与设置项 `https://code.claude.com/docs/en/settings`；网关文档 `https://code.claude.com/docs/en/llm-gateway`。
- **Crush**（`/tmp/model-research/crush`）：JSON Schema 仓库根 `schema.json`（`$defs/ProviderConfig`、`$defs/SelectedModel`、`$defs/Model`）；角色只有 large/small `internal/config/config.go:55-56,556`；配置查找顺序 `internal/config/load.go:911-947,1180-1250`；small→large 回落 `同文件:890-902`；catwalk 三级回退 `internal/config/provider.go:155-245`；shell 展开 `internal/config/resolve.go:56-63`。
- **Aider**（`/tmp/model-research/aider`）：`ModelSettings` `aider/models.py:128-150`；LiteLLM 消费与 24h 缓存 `aider/models.py:161-171`；配置搜索路径 `aider/main.py:290-410`；CLI 参数 `aider/args.py:60-140,185,194`。
- **models.dev**（`/tmp/model-research/modelsdev`）：schema `packages/core/src/schema.ts`；`base_model` 继承 `packages/core/src/generate.ts:14-21,156-190`；API 路由 `packages/function/src/worker.ts:66-134`；每小时同步 `.github/workflows/sync-models.yml:3-5`；贡献规范 `README.md:49-140`。
- **Pi**（`/opt/homebrew/lib/node_modules/@earendil-works/pi-coding-agent`）：`models.json` schema `dist/core/model-config.d.ts`（`ProviderConfigSchema`）与 `dist/core/model-config.js:179`；路径 `dist/config.js:423-425`；settings 字段 `dist/core/settings-manager.d.ts:67-69`。
- **OMP**（`~/.bun/install/global/node_modules/@oh-my-pi/pi-coding-agent`）：Provider schema `src/config/models-config-schema-bundle.ts:82-100,162-200,258-306`；写盘前校验 `src/config/models-config.ts:34-100`；10 个角色 `src/config/model-roles.ts:22-64`；文件名回落与迁移 `src/config/config-file.ts:20-49,130-155`；`apiKey` 解析语义 `src/config/model-config-values.ts`。
- **SkillStar 现状**：codex 默认值 `crates/skillstar-models/src/providers/crud.rs:22-28` 与 `crates/skillstar-models/src/providers/types.rs:158-172`；codex 投影 `crates/skillstar-models/src/tool_sync/multi_provider.rs:194-345`；路径解析 `crates/skillstar-models/src/tool_sync/paths_files.rs`；agent 注册表 `crates/skillstar-models/src/tool_sync/agents.rs`。

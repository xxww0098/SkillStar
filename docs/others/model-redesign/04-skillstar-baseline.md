# SkillStar Models 现状全量盘点与痛点清单

状态：research

> 本文是「连数据模型一起重设计」的事实基线。所有结论带 `path:line` 证据；无法验证的写「未确认」。
> 审计日期：2026-08-15。审计基线 commit：`00737df`（`main`，工作区有 5 个与 Models 无关的未提交改动）。
> 本轮**未修改任何生产代码**，只新增本文件。

---

## 0. 一句话现状

后端（Rust）的 Provider/Binding 数据模型已经收敛到一个相当干净的 v3 flat store + 表驱动 Agent 注册表；
**前端生产主界面却是一份原型代码**（`components/hub/prototype/`），它只用到了后端能力的一个子集，
并且在三处关键位置（创建 Provider 的 preset 表、Claude 角色映射的持久化、多 provider 绑定的解绑语义）
与后端契约实质性脱节。约 **2900 行前端代码已无任何生产引用**，但没有任何门禁能发现它们。

---

## 1. Rust 侧数据模型（字段级）

### 1.1 `crates/skillstar-providers` — 零依赖元数据叶子

两个模块，共 314 行（`identity.rs` 223 + `balance.rs` 91）。crate 内没有任何 IO。

#### `ProviderIdentity`（`crates/skillstar-providers/src/identity.rs:21-33`）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `canonical_id` | `&'static str` | 全局唯一规范 id |
| `display_name` | `&'static str` | 展示名 |
| `catalog_id` | `Option<&'static str>` | Usage 侧 catalog 行 id；models-only provider 为 `None` |
| `preset_ids` | `&'static [&'static str]` | Models 侧 preset id 列表（可多个，如 GLM = `glm` + `glm-coding`） |

`PROVIDER_IDENTITIES` 共 **15 行**（`identity.rs:36-134`）：
`deepseek` / `kimi` / `glm` / `minimax` / `openrouter` / `siliconflow` / `grok` / `longcat` /
`xiaomi-mimo` / `claude-official` / `cursor` / `codex` / `antigravity` / `stepfun` / `opencode`。

两个查询函数：`identity_for_catalog`（`identity.rs:137-141`）、`identity_for_preset`（`identity.rs:144-148`），都是线性扫描。

值得注意的**身份粒度不对称**（这是这张表存在的全部理由，注释写在 `identity.rs:1-17`）：

- 一个 catalog id 可以对应两个 preset（`glm` → `["glm","glm-coding"]`，`identity.rs:50-55`）。
- Native Official 种子占用了 preset id：`claude-official` 的 `catalog_id` 是 `anthropic`（`identity.rs:94-101`），
  `codex` 的 `preset_ids` 是 `["codex-official"]`（`identity.rs:109-115`）。
  也就是说「原生登录种子」在 identity 层被当作普通 preset 处理，没有专门的类型位。
- `opencode`（`identity.rs:128-133`）在这里是一个 **Usage 订阅 provider**，同时又是 tool_sync 的一个 **Agent id**。
  同一个字符串在两个域指两个东西，靠上下文区分，无类型隔离。

#### `BalanceSpec`（`crates/skillstar-providers/src/balance.rs:22-34`）

| 字段 | 类型 |
| --- | --- |
| `catalog_id` / `display_name` / `endpoint` | `&'static str` |
| `auth` | `AuthScheme::{Bearer, RawHeader(&'static str)}`（`balance.rs:12-18`） |
| `auth_error_hint` | `Option<&'static str>` |

`API_KEY_BALANCE_SPECS` 只有 4 条：DEEPSEEK / KIMI / GLM / MINIMAX（`balance.rs:73`）。
响应解析**刻意不建模**（`balance.rs:5-8` 的注释），各 fetcher 自己解析。
前端另有一份余额解析表 `src/features/models/api/balance.ts`（180 行），与 preset 的 `balance_parser` 字段配对。

### 1.2 `crates/skillstar-models/src/providers/` — flat v2/v3 store

模块划分（`providers/mod.rs:14-30`，全部 `pub use *` glob 再导出）：
`crud.rs`(569) / `model_catalog.rs`(184) / `presets.rs`(374) / `store.rs`(454) / `types.rs`(322) + `tests/`(5 个 part，2352 行)。

#### 磁盘格式与版本

- 路径：`skillstar_core::infra::paths::config_dir().join("model_providers.json")`（`providers/store.rs:9-16`），
  即 `~/.skillstar/config/model_providers.json`，受 `SKILLSTAR_DATA_DIR` 覆盖。
- `FLAT_STORE_VERSION = 3`（`providers/types.rs:94`）。注意**命名与版本号不同步**：
  代码、文档、注释里到处叫「flat v2 architecture」，磁盘版本号已经是 3。
  v2→v3 的差别只是 `tool_activations` 的值从 `Option<ToolActivation>` 变成 `ToolBinding`（`types.rs:88-93`）。

#### `FlatProvidersStore`（`providers/types.rs:100-106`）

| 字段 | 类型 |
| --- | --- |
| `version` | `u32` |
| `providers` | `Vec<ProviderEntryFlat>` |
| `tool_activations` | `HashMap<String, ToolBinding>` |

`tool_activations` 的 key 是**任意字符串**，不是枚举。`resync_active_tools` 明确要跳过历史遗留 id（`tool_sync/backup_merge.rs:207-211`，注释点名已删除的 `gemini`）。

#### `ProviderEntryFlat` —— Provider 行的每一个字段（`providers/types.rs:124-166`）

| 字段 | 类型 | 默认 | 说明 / 证据 |
| --- | --- | --- | --- |
| `id` | `String` | `""` | UUIDv4，除 Native Official 种子外（`crud.rs:72-79`） |
| `name` | `String` | 必填 | 创建时校验非空（`crud.rs:63-65`） |
| `base_url_openai` | `String` | `""` | OpenAI 兼容端点 |
| `base_url_anthropic` | `String` | `""` | Anthropic 兼容端点 |
| `models_url` | `String` | `""` | 「拉取模型列表」端点，全 Agent 共用（`types.rs:132-139`） |
| `api_key` | `String` | `""` | 明文存储 |
| `models` | `Vec<String>` | `[]` | 模型 id 列表 |
| `default_model` | `String` | `""` | 激活时的兜底模型（`crud.rs:325-328`） |
| `sort_index` | `u32` | max+1 | 创建时自动分配（`crud.rs:104-115`） |
| `preset_id` | `Option<String>` | `None` | 指向 preset 注册表 |
| `icon_color` | `Option<String>` | `None` | |
| `notes` | `Option<String>` | `None` | |
| `created_at` | `Option<u64>` | now(ms) | 注意是**毫秒**（`crud.rs:97-101`），而 `ToolActivation.last_sync_at` 是**秒** |
| `meta` | `Option<serde_json::Value>` | `None` | 无 schema 的口袋，见下 |
| `codex_wire_api` | `String` | `"responses"` | Codex 专属，直接长在通用 Provider 行上 |
| `codex_auth_mode` | `String` | `"api_key"` | 同上；三态 `api_key`/`oauth`/`third_party` |

共 **17 个字段，其中 2 个是 Codex 独占的字段**。

`meta` 这个无 schema 口袋里事实上塞了 5 类东西（无集中定义，需要跨文件拼）：

| meta key | 写入方 | 读取方 |
| --- | --- | --- |
| `model_catalog` | 前端 `ClaudeMappingPanel.tsx:167-171`、`useProviderForm` | `catalog_from_meta`（`providers/model_catalog.rs:80-84`）、OpenCode writer（`tool_sync/sync.rs:200`） |
| `claude_main_model` | 前端 `AppAiModelsPicker.tsx:171`（已死代码路径见 §3） | `ai_provider/resolve.rs:68` |
| `claude_haiku_model` / `claude_sonnet_model` / `claude_opus_model` | **生产界面无写入方**（见 §5.6） | `tool_sync/sync.rs:108-119`、`ai_provider/resolve.rs:83-85` |
| `baseURL` | v1 遗留 | `ai_provider/resolve.rs:246-253`（仅 legacy 路径） |
| provider request meta（timeout 等） | `apply_provider_request_meta`（`ai_provider/resolve.rs:86`） | 同 |

常量 `MODEL_CATALOG_META_KEY = "model_catalog"` 定义在 `providers/model_catalog.rs:5`，
前端另有一份同名常量 `src/features/models/lib/providerPatch.ts:22`。两份字面量，无一致性测试。

#### `ModelCatalogEntry`（`providers/types.rs:180-196`）

`id` / `display_name` / `source_name` / `description` / `context_length` / `max_completion_tokens` / `cost: Option<Value>` / `raw: Option<Value>`。
`raw` 保存整个上游 JSON（`model_catalog.rs:111`），所以 catalog 缓存进 `meta` 后体积可以很大——
**`meta.model_catalog` 会连同整个 provider 行一起被 `serde_json::to_string_pretty` 写盘**（`providers/store.rs:81-82`）。
没有任何裁剪或体积上限。

`ModelCatalogFetchResult`（`types.rs:200-206`）：`models` / `catalog` / `metadata_sources` / `missing_cost_count`。
`metadata_sources` 在 `merge_model_catalog` 里被硬编码为 `Vec::new()`（`model_catalog.rs:74`）——**这个字段目前永远是空的**。

解析支持 3 种上游形状（`model_catalog.rs:16-44`）：CLIProxyAPI、models.dev、OpenAI 兼容。

#### `ToolActivation` —— 一条 binding entry（`providers/types.rs:216-225`）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `provider_id` | `String` | |
| `model` | `String` | |
| `settings` | `Option<Value>` | **per-entry** 设置袋，当前唯一消费者是 `CodexSettings` |
| `last_sync_at` | `Option<u64>` | Unix **秒**；外部修改检测基线 |

#### `ToolBinding` —— 一个 Agent 的全部绑定（`providers/types.rs:241-249`）

| 字段 | 类型 |
| --- | --- |
| `entries` | `Vec<ToolActivation>` |
| `active_index` | `usize` |
| `settings` | `Option<Value>` — **binding 级**设置袋，当前唯一消费者是 `OmpSettings` |

helper：`single`(251-259) / `active`(262-268，带 clamp) / `active_mut`(271-277) / `binds_provider`(280-282) / `is_empty`(284-286)。
`active_index` 越界靠 clamp 兜底而不是不变量（`types.rs:266`），说明这个指针可能失同步。

**两层设置袋并存**是当前模型最容易误用的地方：`ToolActivation.settings`（per-provider）和
`ToolBinding.settings`（per-tool），两者都是无类型 `Value`，各有一个专用写入命令
（`update_tool_settings` / `update_tool_binding_settings`），名字只差 `binding` 一个词。

#### `ProviderPatchFlat`（`providers/types.rs:293-322`）

14 个 `Option` 字段，逐个 `if let Some(..)` 应用（`crud.rs:141-183`）。
**注意缺口**：patch 里没有 `models_url` 之外的 `created_at`，也没有删除字段的语义
（`Some("")` 与 `None` 的区别是「写空串」和「不改」，没有「清除」）。
前端 `ProviderPatchFlat` 手写 mirror（`src/types/models.ts:174-188`）**少了 `preset_id`**——
Rust 有（`types.rs:314-315`），TS 没有。

#### v1 遗留类型（`providers/types.rs:11-81`）

`ModelMapping` / `ProviderSettings` / `ProviderEntry` / `AppProviders` / `ProvidersStore` 全部 `pub(crate)`。
`ProvidersStore` 固定 4 个 app bucket：`claude` / `codex` / `opencode` / `gemini`（后者注释为「仅供迁移保留」，`types.rs:77-80`）。

v1→v2 迁移：`migrate_store_if_needed`（`providers/store.rs:111-161`）
→ 按 `(base_url, api_key)` 去重合并（`convert_v1_to_v2`，`store.rs:218-339`），
只把 `claude.current` / `codex.current` 转成 binding（`store.rs:306-307`），
opencode/gemini 的 `current` **直接丢弃**。
v2→v3：`migrate_v2_to_v3`（`store.rs:183-212`）。
迁移前备份 `.json.bak`（`store.rs:165-176`），备份失败只 warn 不中止。

**这条迁移路径每次读盘都会跑**：所有写命令都调用 `migrate_store_if_needed` 而不是 `read_flat_store`
（例：`src-tauri/src/commands/models_commands/tools.rs:37`、`provider_cmds.rs:109`）。
读取路径（`detect_provider_conflicts`）用的是 `read_flat_store`（`tools.rs:379`），两条路径行为不同。

`read_flat_store` 对**任何**解析失败都返回空 store（`store.rs:54-63`）——
文件损坏时用户会看到「所有 provider 都没了」，而不是错误。

### 1.3 presets 注册表与 Official 种子

`ProviderPresetFlat`（`providers/presets.rs:79-104`）：
`id` / `name` / `category` / `base_url_openai` / `base_url_anthropic` / `models_url` /
`models` / `icon_color` / `api_key_url?` / `balance_endpoint?` / `balance_parser?` / `endpoint_candidates[]`。

`get_all_presets_flat()` 返回 **13 条硬编码 preset**（`presets.rs:110-313`，计数被 `providers/tests/part1.rs:248-251` 钉死）：

| category | preset id |
| --- | --- |
| `domestic`(8) | `deepseek` `kimi` `kimi-coding` `minimax` `glm` `glm-coding` `longcat` `xiaomi-mimo` |
| `relay`(2) | `openrouter` `siliconflow` |
| `official`(3) | `claude-official` `codex-official` `grok` |

`category: "official"` 里混了**两种完全不同的东西**（`presets.rs:266-311` 的注释自己承认）：
Native Official 种子（无 key、空端点）和 Grok（有 key 的官方厂商）。区分靠 id 白名单而不是字段。

#### Native Official 种子机制

- 常量：`CLAUDE_OFFICIAL_ID = "claude-official"`、`CODEX_OFFICIAL_ID = "codex-official"`（`presets.rs:10-12`）。
- 判定：`is_native_official_preset_id`（`presets.rs:17-19`）是一个 `matches!` 白名单；
  `is_native_official_provider`（`presets.rs:24-30`）同时看 `id` 和 `preset_id`。
- `ensure_official_providers`（`presets.rs:38-67`）：缺失时插入，已存在则跳过（不覆盖用户改名）。
  仅在 `get_providers_flat` 命令中调用（`provider_cmds.rs:84-86`），变更时写盘。
- 稳定 id：`create_from_preset_flat` 对这两个 id 不发 UUID（`presets.rs:343-348`）；
  `create_provider_flat` 对它们额外做重复检查（`crud.rs:73-76`）。
- Codex Official 强制 `codex_auth_mode = "oauth"`（`presets.rs:350-354`，`crud.rs:345-356`）。
- 激活时跳过 URL 校验（`crud.rs:295-322`）。
- Claude Official 同步 = 清空托管 env（`tool_sync/sync.rs:66-68` → `clear_claude_managed_env_at`）。
- Codex Official 同步 = 不写 auth.json，并清掉指向托管表的 `model_provider`/`model`（`multi_provider.rs:239-298`）。

**2 个种子服务 3 条 binding**：`claude-official` 同时被 `claude-code` 和 `claude-desktop` 使用
（前端 `officialProviderIdForTool`，`src/features/models/lib/officialProviders.ts:43-47`）。

### 1.4 CRUD 与激活语义（`providers/crud.rs`）

| 函数 | 行 | 语义要点 |
| --- | --- | --- |
| `recommended_codex_defaults` | 22-28 | `api.openai.com` → `("responses","api_key")`，其余 → `("chat","third_party")`。**前端有逐字重复实现**（`lib/providerPatch.ts:129-143`） |
| `create_provider_flat` | 58-121 | 校验 name/3 个 URL；Official 保稳定 id；用「字段等于 serde 默认值」推断 Codex 默认（`crud.rs:88-93`）——这意味着**用户显式选 `responses` 和「没选」不可区分** |
| `update_provider_flat` | 130-186 | 逐字段 patch |
| `delete_provider_flat` | 195-219 | 删 provider + 摘掉所有 binding entry + re-clamp active_index + 清角色 |
| `reorder_providers` | 228-244 | 按传入顺序重排 `sort_index` |
| `activate_tool` | 277-395 | 见下 |
| `agent_supports_multiple_providers` | 405-408 | 转发到 `agent_spec().kind` |
| `set_active_binding` | 415-438 | 仅移动 active 指针 |
| `remove_binding_entry` | 447-475 | 摘一条 entry + re-clamp + 清角色 |
| `prune_binding_roles_for_provider` | 482-494 | 只动 `settings.roles`，保留袋内其他 key |
| `update_tool_settings` | 505-521 | 写 active entry 的 per-entry 袋 |
| `update_tool_binding_settings` | 532-550 | 写 binding 级袋；`null` 清空 |
| `deactivate_tool` | 559-569 | 整个 binding 置空 |

`activate_tool` 的 6 步（`crud.rs:283-394`）：
① 找 provider → ② 按 `agent_spec().required_url` 校验（Official 跳过）→
③ model 兜底到 `default_model` → ④ settings 继承同 provider 的旧 settings；Codex Official 强制 oauth →
⑤ 构造 entry（`last_sync_at: None`）→ ⑥ multi 则 upsert+指针，single 则整体替换。

**注意**：步骤 ⑤ 每次激活都把 `last_sync_at` 重置为 `None`，命令层随后在同步成功时再写回
（`tools.rs:58-67`）。这是两次写盘（`tools.rs:49` 和 `tools.rs:66`），中间夹一次磁盘同步——
不是原子的。

### 1.5 `tool_sync` —— Agent 注册表与写盘

#### `AgentSpec`（`tool_sync/agents.rs:52-75`）

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `id` | `&'static str` | 与前端共享的 toolId |
| `display_name` | `&'static str` | |
| `binary_name` | `&'static str` | PATH 探测用 |
| `config_dir_probes` | `&'static [&'static str]` | home 相对目录列表 |
| `kind` | `AgentKind::{Single,Multi}`（`agents.rs:21-26`） | |
| `required_url` | `RequiredUrl::{Anthropic,Openai}`（`agents.rs:30-35`） | |
| `files` | `&'static [AgentConfigFileSpec]` | 首条是主配置文件 |
| `sync_binding` | `fn(&ToolBinding, &[ProviderEntryFlat]) -> Result<ToolSyncResultFlat>` | |
| `unsync` | `fn() -> Result<()>` | |
| `detect_provider` | `fn(&Path) -> Result<Option<String>>` | |

`AgentConfigFileSpec`（`agents.rs:38-49`）：`file_id` / `label` / `format`("json"|"toml"|"env"|"yaml") /
`resolve: fn() -> Result<PathBuf>` / `default_content`。

注意 `format` 的注释写 `"json"、"toml" 或 "env"`（`agents.rs:44`），但校验里允许 `"yaml"`
（`agents.rs:284`），且 `"env"` **没有任何 spec 在用**——是死枚举值。

#### Agent 能力矩阵（`AGENT_SPECS`，`tool_sync/agents.rs:87-225`）

| toolId | display | binary | kind | required_url | 配置文件（file_id → 路径） | 多 provider | 角色路由 | Official | writer 位置 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `claude-code` | Claude Code | `claude` | Single | Anthropic | `settings` → `~/.claude/settings.json` (json) | ✗ | ✗ | ✓ `claude-official` | `tool_sync/sync.rs:280-286` → `sync_to_claude_code` (`sync.rs:47-126`) |
| `claude-desktop` | Claude Desktop | `claude-desktop`(不探测) | Single | Anthropic | `binding` → `~/.claude-desktop/skillstar-binding.json` (json) | ✗ | ✗ | ✓ 复用 `claude-official` | `tool_sync/sync.rs:293-328`（只写标记文件） |
| `codex` | Codex | `codex` | Multi | Openai | `config` → `~/.codex/config.toml` (toml)；`auth` → `~/.codex/auth.json` (json) | ✓ | ✗ | ✓ `codex-official` | `tool_sync/multi_provider.rs:199-352` |
| `opencode` | OpenCode | `opencode` | Multi | Openai | `opencode` → `~/.config/opencode/opencode.json` (json) | ✓ | ✗ | ✗ | `tool_sync/multi_provider.rs:362-404`（走 JSON 骨架） |
| `pi` | Pi | `pi` | Multi | Openai | `models` → `~/.pi/agent/models.json`；`settings` → `~/.pi/agent/settings.json` (json) | ✓ | ✗ | ✗ | `tool_sync/multi_provider.rs:416-496`（走 JSON 骨架） |
| `omp` | Oh My Pi | `omp` | Multi | Openai | `models` → `~/.omp/agent/models.yml`；`config` → `~/.omp/agent/config.yml` (yaml) | ✓ | **✓ 唯一** | ✗ | `tool_sync/omp_provider.rs:109-204`（YAML 骨架） |

注册表顺序被 `agents.rs:250-264` 钉死，并与前端 `src/features/models/lib/__tests__/agentRegistry.test.ts:13`
的同一份字面量互锁。

`claude-desktop` 的特殊性（一行行都是特例）：
- `binary_name = "claude-desktop"` 但从不探测（`agents.rs:108-110` 注释 + `tools.rs:248-252` 硬分支）。
- 安装判定只看 macOS/Windows 桌面 app（`tools.rs:265-266`、`tools.rs:287-293`）。
- 写盘只是一个 SkillStar 自造的标记文件，**不投影任何 Claude Desktop 原生配置**
  （`tool_sync/sync.rs:288-292` 注释「native write-path TBD」，`sync.rs:319-324` 的 body）。
- `detect_provider` 读的就是自己写的标记（`sync.rs:342-353`）。

也就是说：**Claude Desktop 这一整列今天在磁盘上不产生任何对 Claude Desktop 有意义的效果。**

#### 写盘骨架

- JSON 多 provider 骨架 `sync_json_blocks_inner`（`multi_provider.rs:114-186`）：
  备份 → 读或初始化 root → `provider_map.retain(!is_skillstar_managed_key)` →
  逐 entry 写 `skillstar_<id8>` 块（跳过空 OpenAI URL）→ 回调 finish_root 写 active 指针 → 写盘。
  消费者：OpenCode(`multi_provider.rs:382-401`)、Pi(`multi_provider.rs:469-478`)。
- YAML 骨架 `sync_yaml_blocks_inner`（`omp_provider.rs:22-94`）：与上面**逐行同构**，
  只是把 `serde_json::Map` 换成 `serde_yaml::Mapping`。**两份骨架并存，没有共享抽象**。
- Codex 独立维护（`multi_provider.rs:215-352`），理由记在 `docs/decisions.md` D-012。
- 托管键规则：`skillstar_managed_key`（`multi_provider.rs:29-47`）取 provider id 前 8 字符小写、
  非字母数字转 `_`；`is_skillstar_managed_key`（`multi_provider.rs:51-56`）匹配 `skillstar` 或 `skillstar_*`。
  同一规则在 **3 处重复实现**：Rust `multi_provider.rs:29-47`、
  Rust env 变量版 `codex_env_key_for`（`tool_sync/types.rs:264-282`，大写版）、
  前端 `previewRoleValue`（`src/features/models/lib/ompRoles.ts:83-87`）。

#### retain 策略（各 Agent 的「清理什么」不一致）

| Agent | 托管块 retain | active 指针清理条件 |
| --- | --- | --- |
| Claude Code | 6 个 env key 白名单（`tool_sync/types.rs:409-416`） | 无指针概念；`env` 空则删掉 `env`（`sync.rs:145-149`） |
| Codex | `model_providers` 表内 `skillstar*` 全删重写（`multi_provider.rs:320`） | 仅当 `model_provider` 指向托管键才删（`multi_provider.rs:291-298`）；unsync 时无条件删（`multi_provider.rs:576-577`） |
| OpenCode | `provider.*` 内 `skillstar*`（骨架） | unsync 时看 `model` 前缀（`multi_provider.rs:604-608`） |
| Pi | `providers.*` 内 `skillstar*` | unsync 时看 `defaultProvider`（`multi_provider.rs:655-658`） |
| OMP | `providers.*` 内 `skillstar*` | `modelRoles` 里**每一个**值前缀是托管键的角色都删（`omp_provider.rs:307`、`374`） |

注意 Codex unsync 的 `table.remove("model_provider")` 是**无条件**的（`multi_provider.rs:576`），
与 sync 路径的条件删除（`multi_provider.rs:291`）不一致——用户自己设的 `model_provider` 会在
deactivate 时被连坐删除。**未确认**是否有测试覆盖这个差异。

#### `OmpSettings` / 角色路由（`tool_sync/types.rs:91-189`，`omp_provider.rs`）

- `OMP_MODEL_ROLES`：10 个（`types.rs:91-93`）`default smol slow plan vision designer commit tiny task advisor`。
- `OMP_THINKING_LEVELS`：9 个（`types.rs:103-105`）。
- `OmpRoleTarget { provider_id, model, thinking? }`（`types.rs:116-123`）。
  `to_role_value`（`types.rs:130-143`）渲染 `<managed_key>/<model>[:thinking]`；无 model 返回 `None`。
- `OmpSettings { roles: BTreeMap<String, OmpRoleTarget> }`（`types.rs:154-157`）。
- `is_valid_omp_role_name`（`types.rs:182-189`）：非空、≤64、不以 `@` 开头、仅 `[A-Za-z0-9_-]`。
- 写盘：`resolve_omp_roles`（`omp_provider.rs:237-270`）跳过名字非法 / provider 未写进 models.yml /
  无 model 的角色；`default` 缺失时用 active entry 兜底（`omp_provider.rs:263-267`）。
- `role_models_by_provider`（`omp_provider.rs:209-228`）把角色引用的 model 补进 models.yml 的 `models[]`。
- `set_omp_model_roles`（`omp_provider.rs:280-318`）先 `retain(!points_at_managed)` 再写当前集合。

#### conflicts（`tool_sync/conflicts.rs`）

- `ConflictType`：`ExternalModification` / `LegacyConfig` / `EnvVarOverride`（`types.rs:308-316`）。
- `detect_conflicts(tool_id, last_sync_ts)`（`conflicts.rs:27-49`）：外部修改（mtime > last_sync）+
  `claude-code` 专属的 `~/.claude.json` 检查 + 全局 env 覆盖。
- env 检查只覆盖 Claude(3 个) 和 Codex(2 个)（`conflicts.rs:10-17`）——
  **OpenCode / Pi / OMP 的 env 覆盖完全不检测**。
- 冲突描述文案是**硬编码中文**（`conflicts.rs:68-71`、`conflicts.rs:128-131`、`conflicts.rs:179-181`），
  不走 i18n。

#### backup / merge（`tool_sync/backup_merge.rs`）

- `create_rolling_backup`（11-27）：`{path}.bak.{ms}`，保留最近 5 份（`cleanup_old_backups`，30-67）。
- `merge_json_write`（73-103）：顶层字段合并。
- `merge_json_env_write`（110-162）：只动 `env` 子对象；`Value::Null` 表示「删除该 key」。
- `resync_active_tools`（185-228）：按 `AgentKind` 决定影响面——Multi 只要任一 entry 命中就整体重写，
  Single 只在 active entry 命中时重写。

#### 沙箱与测试隔离

- `TOOL_SYNC_HOME_ENV = "SKILLSTAR_TOOL_SYNC_HOME"`（`tool_sync/mod.rs:56`）。
- 解析优先级（`sandbox_home`，`mod.rs:65-81`）：`cfg(test)` thread-local override →
  环境变量 → `cfg(test)` 的 per-process 兜底 temp dir → `dirs::home_dir()`。
- thread-local override 是 2026-08-12 修并行 flaky 的方案，记在 `docs/errors.md:50-55`。
- 上游 home 覆盖 `CODEX_HOME`/`GROK_HOME` 排在沙箱之后（`mod.rs:143-147`、`sync.rs:26-34`）。

### 1.6 `ai_provider` 对上述模型的依赖

- `AiProviderRef { app_id, provider_id }`（`crates/skillstar-models/src/provider_ref.rs:12-16`），
  从 crate root 导出（`lib.rs:19`）。
- `resolve_provider_ref_parts`（`ai_provider/resolve.rs:277-305`）：
  `app_id` 只接受 `"claude"` 或 `"codex"`（`resolve.rs:285-287`）——
  **这是第三套 id 空间**，既不是 preset id 也不是 tool id。
- 先查 flat store（`resolve_from_flat_store`，`resolve.rs:34-121`），失败则**降级到 v1 legacy store**
  （`resolve.rs:303-304` → `resolve_from_legacy_store`，`resolve.rs:143-273`）。
  这是 v1 类型今天唯一的生产读取点。
- flat 路径按 `app_id` 分支硬编码字段映射（`resolve.rs:50-118`），
  硬编码兜底模型 `claude-sonnet-4-20250514`（`resolve.rs:77`）和 `gpt-5.4`（`resolve.rs:106`）。
- 空 api_key 直接 bail（`resolve.rs:52-54`、`resolve.rs:89-91`）——
  意味着 **Native Official 种子无法作为应用内 AI 的 provider**（它们没有 key）。

---

## 2. 命令面与 DTO

### 2.1 命令清单

`src-tauri/src/commands/models_commands/`：`mod.rs`(80) / `provider_cmds.rs`(180) / `tools.rs`(492) / `diagnostics.rs`(92)。
全部在 `src-tauri/src/lib.rs:408-438` 注册，写命令共享 `ProvidersWriteLock`（`mod.rs:46-52`，tokio Mutex）。

| # | 命令 | 定义 | 入参 | 返回 | 前端调用方 |
| --- | --- | --- | --- | --- | --- |
| 1 | `get_providers_flat` | `provider_cmds.rs:78` | (lock) | `FlatProvidersResponse` | `api/providers.ts:19` |
| 2 | `create_provider_flat` | `provider_cmds.rs:103` | `entry: ProviderEntryFlat` | `ProviderEntryFlat` | `api/providers.ts:39` |
| 3 | `update_provider_flat` | `provider_cmds.rs:123` | `id`, `patch` | `ProviderUpdateFlatResult` | `api/providers.ts:52` |
| 4 | `delete_provider_flat` | `provider_cmds.rs:149` | `id` | `()` | `api/providers.ts:80` |
| 5 | `reorder_providers` | `provider_cmds.rs:168` | `orderedIds` | `()` | `api/providers.ts:116`（**无 UI 触发**） |
| 6 | `get_provider_presets_flat` | `provider_cmds.rs:13` | — | `Vec<ProviderPresetFlat>` | `api/presets.ts:39` → `useProviderForm` |
| 7 | `set_app_ai_provider_ref` | `provider_cmds.rs:22` | `appId`,`providerId` | `()` | `api/appAi.ts` |
| 8 | `clear_app_ai_provider_ref` | `provider_cmds.rs:62` | — | `()` | `api/appAi.ts` |
| 9 | `activate_tool` | `tools.rs:28` | `providerId`,`toolId`,`model?`,`settings?` | `ToolSyncResultFlat` | `api/activations.ts:48` |
| 10 | `deactivate_tool` | `tools.rs:77` | `toolId` | `()` | `api/activations.ts:89` |
| 11 | `set_active_binding` | `tools.rs:103` | `toolId`,`providerId` | `ToolSyncResultFlat` | `api/activations.ts:174` → **仅死代码 `useAgentActivation.ts:103`** |
| 12 | `remove_binding_entry` | `tools.rs:135` | `toolId`,`providerId` | `ToolSyncResultFlat` | `api/activations.ts:205` → OMP 单元格 |
| 13 | `update_tool_settings` | `tools.rs:167` | `toolId`,`settings` | `ToolSyncResultFlat` | `api/activations.ts:119` → **仅死代码 `useAgentActivation.ts:114`** |
| 14 | `update_tool_binding_settings` | `tools.rs:195` | `toolId`,`settings` | `ToolSyncResultFlat` | `api/activations.ts:142` → `OmpRolePanel` |
| 15 | `get_tool_config_targets` | `tools.rs:13` | — | `Vec<ToolConfigTarget>` | **无生产调用方**（只在 devMock 和类型声明里） |
| 16 | `detect_tool_installation` | `tools.rs:238` | `toolId` | `serde_json::Value`（未类型化） | `api/install.ts` → **仅死代码** |
| 17 | `list_tool_config_files` | `tools.rs:302` | `toolId` | `Vec<ToolConfigFileInfo>` | `api/configFiles.ts` → **仅死代码** |
| 18 | `read_tool_config_file` | `tools.rs:310` | `toolId`,`fileId` | `String` | 同上 |
| 19 | `write_tool_config_file` | `tools.rs:316` | `toolId`,`fileId`,`content` | `WriteToolConfigFileResult` | 同上 |
| 20 | `format_tool_config_file` | `tools.rs:328` | `toolId`,`fileId` | `String` | 同上 |
| 21 | `push_provider_to_tool_config` | `tools.rs:334` | `providerId`,`toolId` | `ToolSyncResultFlat` | `api/configFiles.ts:101` → **仅死代码** |
| 22 | `detect_provider_conflicts` | `tools.rs:375` | `providerId` | `Vec<serde_json::Value>`（未类型化） | `ConflictWarnings.tsx:147` |
| 23 | `resync_tool` | `tools.rs:408` | `toolId` | `ToolSyncResultFlat` | `ConflictWarnings.tsx:184` |
| 24 | `test_endpoints_latency` | `diagnostics.rs:12` | `urls`,`apiKey?`,`timeoutMs?` | `Vec<EndpointLatencyResult>` | `api/diagnostics.ts:78` |
| 25 | `test_provider_latency` | `diagnostics.rs:24` | `appId`,`providerId`,`baseUrl`,`apiKey`,`timeoutMs?` | `LatencyResult` | **无生产调用方** |
| 26 | `test_provider_connection` | `diagnostics.rs:42` | `baseUrl`,`apiKey`,`model`,`format` | `ConnectionTestResult` | `api/diagnostics.ts` |
| 27 | `fetch_provider_models` | `diagnostics.rs:58` | `url`,`apiKey`,`timeoutMs?` | `Vec<String>` | `api/modelCatalog.ts` |
| 28 | `fetch_provider_model_catalog` | `diagnostics.rs:70` | `url`,`apiKey`,`timeoutMs?` | `ModelCatalogFetchResult` | `api/modelCatalog.ts` |
| 29 | `query_provider_balance` | `diagnostics.rs:84` | `presetId`,`apiKey`,`_baseUrl` | `serde_json::Value` | `api/balance.ts:130` |

外围但同域的两条（`src-tauri/src/commands/shell_rc.rs`）：
`write_codex_env_to_zshrc`(15-21) / `read_codex_env_from_zshrc`(25-28)，
只被 `CodexSettingsForm.tsx:72-81` 使用，而该组件在死代码岛内（见 §3.2）。

**结论：29 条 Models 命令里，8 条（#11 #13 #15 #16 #17 #18 #19 #20 #21 #25，共 10 条）
今天没有任何可达的生产 UI 触发路径。** 加上 shell_rc 两条则是 12 条。

### 2.2 DTO：生成类型 vs 手写 mirror

`scripts/internal/check_generated_types.sh` 存在，`src/types/generated/` 有 73 个文件。
用 `grep` 确认：**generated 里没有任何 Provider/Tool/Preset/Catalog 类型**，
只有 `McpPreset.ts`、`McpToolStatus.ts`、`CatalogEntry.ts`（Usage 的）。
`ts_rs` 在 `skillstar-models` 里只用于 `mcp/*`（`grep ts_rs crates/skillstar-models/src/` 全部命中 mcp 模块）。

| 类型 | Rust | TS | 状态 |
| --- | --- | --- | --- |
| `ProviderEntryFlat` | `providers/types.rs:124` | `src/types/models.ts:46` | 手写 mirror |
| `ProviderPatchFlat` | `providers/types.rs:293` | `src/types/models.ts:174` | 手写；**缺 `preset_id`** |
| `ProviderPresetFlat` | `providers/presets.rs:79` | `src/types/models.ts:190` | 手写 |
| `ModelCatalogEntry` / `…FetchResult` | `providers/types.rs:180/200` | `src/types/models.ts:74/85` | 手写 |
| `ToolActivation` | `providers/types.rs:216` | `src/types/models.ts:104` | 手写 |
| `ToolBinding` | `providers/types.rs:241` | `src/types/models.ts:160` | 手写 |
| `OmpRoleTarget` / `OmpSettings` | `tool_sync/types.rs:116/154` | `src/types/models.ts:133/143` | 手写 |
| `CodexSettings` | `tool_sync/types.rs:23` | `src/types/models.ts:94` | 手写；**`auth_mode` 缺 `"third_party"`** |
| `ToolSyncResultFlat` | `tool_sync/types.rs:334` | `src/types/models.ts:36`（名为 `ToolSyncResult`） | 手写；**名字都不一样** |
| `ToolConfigTarget` / `ToolConfigFileInfo` / `WriteToolConfigFileResult` | `tool_sync/types.rs:324/385/398` | `src/types/models.ts:28/213/222` | 手写 |
| `ConfigConflict` | `tool_sync/types.rs:290` | `src/lib/ipc/commands/models.ts:19`（**第三个位置**） | 手写 |
| `FlatProvidersResponse` | `models_commands/mod.rs:68` | `src/types/models.ts:168` | 手写 |
| `ProviderUpdateFlatResult` | `models_commands/mod.rs:77` | `src/types/models.ts:208` | 手写 |
| `AiProviderRef` | `provider_ref.rs:12` | `src/types/ai.ts` | 手写 |
| `OMP_THINKING_LEVELS` | `tool_sync/types.rs:103` | `src/types/models.ts:116` | 手写常量，由 `tool_sync/tests/part4.rs:925-938` + `lib/__tests__/ompRoles.test.ts` 双侧钉住 |

`FlatProvidersResponse` 刻意**不用** `rename_all = "camelCase"`，注释记录了这个坑
（`models_commands/mod.rs:60-66`：曾经因为 `toolActivations` vs `tool_activations` 导致所有 Agent 卡片显示「未接入」）。
其它 DTO 有的 camelCase（`ProviderUpdateFlatResult`，`mod.rs:76`），有的不（`ProviderEntryFlat` 是 snake_case）。
**同一域内序列化风格不统一。**

---

## 3. 前端现状

### 3.1 生产渲染路径（用引用链证明）

```
src/pages/Models.tsx:24            → <ModelsHub {...props} />
  ├─ (DEV only, ?variant=D2|D3)    → <ModelsHubPrototype/>   (Models.tsx:13-22)
  └─ components/hub/ModelsHub.tsx:80 → <VariantD1 data={data}/>
       └─ prototype/ia/VariantD1.tsx:6  export { VariantB2b as VariantD1 }   ← 纯 re-export
            └─ prototype/matrix/rich/VariantB2b.tsx:41
                 ├─ RichMatrixShell (matrix/rich/RichMatrixShell.tsx:63)
                 │    ├─ MatrixChrome  (matrix/MatrixChrome.tsx:16)
                 │    │    └─ AgentColumnCarousel (matrix/AgentColumnCarousel.tsx:15)
                 │    ├─ ClaudeSurfaceIcon / AgentToolIcon
                 │    ├─ ColumnHeader（Official 开关，RichMatrixShell.tsx:183-266）
                 │    └─ EditorPage (prototype/EditorPage.tsx:20)  ← create / app-ai / agent-settings
                 ├─ ClaudeCodeCell → ClaudeMappingPanel (matrix/rich/ClaudeMappingPanel.tsx)
                 ├─ OmpRoleCell   → OmpRolePanel (matrix/rich/OmpRolePanel.tsx) → OmpRoleRow
                 └─ InlineSelectCell（codex / opencode / pi）
  + ProviderEditorDrawer  (components/provider/ProviderEditorDrawer.tsx，ModelsHub.tsx:83)
  + DeleteProviderDialog  (components/hub/DeleteProviderDialog.tsx，ModelsHub.tsx:93)
```

**为什么生产主界面在 `hub/prototype/` 目录**：D1 IA 原型（B2b 矩阵）在评审后被直接晋升为生产，
晋升方式是加了一个 6 行的别名文件 `ia/VariantD1.tsx`（其内容只有一行 `export { VariantB2b as VariantD1 }`），
而没有把实现移出 prototype 目录。`ModelsHub.tsx:14-18` 的 doc comment 承认了这一点。
`scripts/internal/i18n_hardcoded_baseline.txt:13-19` 也记录了这次晋升，
并据此把 prototype 目录**大部分**移出了 i18n 豁免，只保留 `ModelsHubPrototype` / `PrototypeOverlays` / `VariantD2` / `VariantD3` 四个。

### 3.2 prototype 目录逐文件判定

prototype 目录共 4200 行。

| 文件 | 行数 | 判定 | 证据 |
| --- | --- | --- | --- |
| `ia/VariantD1.tsx` | 6 | **生产**（别名） | `ModelsHub.tsx:10` |
| `matrix/rich/VariantB2b.tsx` | 413 | **生产**（真实主界面） | `ia/VariantD1.tsx:6` |
| `matrix/rich/RichMatrixShell.tsx` | 312 | **生产** | `VariantB2b.tsx:21` |
| `matrix/rich/ClaudeMappingPanel.tsx` | 361 | **生产** | `VariantB2b.tsx:14-19` |
| `matrix/rich/OmpRolePanel.tsx` | 256 | **生产** | `VariantB2b.tsx:20` |
| `matrix/rich/OmpRoleRow.tsx` | 196 | **生产** | `OmpRolePanel.tsx:25` |
| `matrix/MatrixChrome.tsx` | 53 | **生产** | `RichMatrixShell.tsx:20` |
| `matrix/AgentColumnCarousel.tsx` | 96 | **生产** | `MatrixChrome.tsx:4`、`RichMatrixShell.tsx:17` |
| `matrix/ClaudeSurfaceIcon.tsx` | 64 | **生产** | `AgentColumnCarousel.tsx:7`、`RichMatrixShell.tsx:18` |
| `matrix/matrixColumns.ts` | 71 | **生产** | `usePrototypeHub.ts:6` |
| `usePrototypeHub.ts` | 157 | **生产** | `ModelsHub.tsx:12` |
| `types.ts` | 87 | **生产** | 全链共用 |
| `modelsNavBridge.ts` | — | **生产** | `Models.tsx:3` |
| `EditorPage.tsx` | 773 | **生产**（create / app-ai / agent-settings 三个 overlay） | `VariantB2b.tsx:11,67` |
| `ModelsHubPrototype.tsx` | 58 | **仅 DEV** | `Models.tsx:2`，`Models.tsx:14` `import.meta.env.PROD` 关闭 |
| `ia/VariantD2.tsx` | 251 | **仅 DEV** | `ModelsHubPrototype.tsx:3` |
| `ia/VariantD3.tsx` | 318 | **仅 DEV** | `ModelsHubPrototype.tsx:4` |
| `StateDump.tsx` | — | **仅 DEV** | 只被 `VariantD2.tsx:11`、`VariantD3.tsx:12` 引用 |
| `PrototypeOverlays.tsx` | 91 | **仅 DEV** | 只被 `VariantD2.tsx:10`、`VariantD3.tsx:11` 引用（`DeleteConfirmModal`） |
| `matrix/rich/VariantB2a.tsx` | 159 | **死代码** | 全仓无任何引用（`grep -rn "\bVariantB2a\b" src/` 只命中自身） |
| `matrix/rich/VariantB2c.tsx` | 241 | **死代码** | 同上 |

DEV-only 岛（D2/D3 + StateDump + PrototypeOverlays + ModelsHubPrototype）+ 两个死 Variant ≈ **1130 行**。

### 3.3 prototype 目录之外的死代码

用 `grep -rn "\b<Name>\b" src/` 排除自身与自身测试后为 0 引用：

| 文件 | 行数 | 状态 |
| --- | --- | --- |
| `components/agents/AgentSettingsDialog.tsx` | 369 | **死**（0 引用），是整个岛的根 |
| `components/agents/AgentHeroCard.tsx` | 298 | **死**（仅注释提及） |
| `components/agents/MultiProviderCard.tsx` | 283 | **死** |
| `components/agents/AppAiCard.tsx` | 222 | **死** |
| `components/agents/CodexSettingsForm.tsx` | 227 | 只被 `AgentSettingsDialog.tsx:37` 引用 → **传递性死** |
| `components/agents/AgentConfigFiles.tsx` | 143 | 只被 `AgentSettingsDialog.tsx:33` → **传递性死** |
| `components/agents/ClaudeModelMapping.tsx` | 96 | 只被 `AgentSettingsDialog.tsx:36` → **传递性死** |
| `components/agents/AgentLaunchCommand.tsx` | 46 | 只被 `AgentSettingsDialog.tsx:34` → **传递性死** |
| `components/agents/AgentStatusPill.tsx` | 110 | 只被上述死组件引用 → **传递性死** |
| `components/provider/PresetPicker.tsx` | 359 | **死**（0 引用） |
| `components/hub/ProviderGalleryCard.tsx` | 193 | **死**（只被自己的 `.test.tsx` 引用） |
| `hooks/useAgentActivation.ts` | 143 | 只被死组件引用 → **传递性死** |
| `hooks/useAgentHealth.ts` | 79 | 同上 |
| `lib/agentStatus.ts` | 95 | 只被死组件 + 自己的测试引用 |
| `lib/launchCommand.ts` | 58 | 只被 `AgentLaunchCommand` 引用 |
| `api/configFiles.ts` | 138 | 只被 `AgentConfigFiles` 引用 |
| `api/install.ts` | 47 | 只被 `useAgentActivation` 引用 |

**合计约 2906 行**（含各自的 `.test.tsx`）。加上 §3.2 的 1130 行，
`src/features/models/` 的 14820 行里约有 **4000 行（27%）不在生产路径上**。

这些死代码还持有 **10 条后端命令的唯一调用点**（见 §2.1），
所以「命令没有 UI」和「UI 是死代码」是同一件事的两面。

### 3.4 状态流

- 唯一数据源：`get_providers_flat` 的 query cache。
  `modelsKeys.providersFlat() = ["models","providers-flat"]`（`src/features/models/api/keys.ts:7`），
  `staleTime = 30_000`（`api/providers.ts:14`）。
- **activation map 不单独 fetch**：`useProvidersFlat` 直接从同一份 response 投影
  （`hooks/useProvidersFlat.ts:24-26`，注释在 `api/activations.ts:1-7`）。
- Query key 工厂只有 4 个 key（`api/keys.ts:5-10`）：`all` / `providersFlat` / `presets` / `install(toolId)`。
  另有独立的 `aiConfigKeys = ["ai-config"]`（`api/keys.ts:18-20`），注释解释了为什么不并入。
- Optimistic 模式统一为 `onMutate` 写缓存 → `onError` 回滚 + toast → `onSettled` invalidate
  （`api/providers.ts:24-28` 的注释；`activate`(`activations.ts:49-86`)、
  `deactivate`(88-115)、`updateBindingSettings`(140-170)、`setActiveBinding`(172-201)、
  `removeBindingEntry`(203-229) 五个 mutation 全部遵守）。
  例外：`updateSettingsMutation`（`activations.ts:117-135`）**没有 optimistic 分支**。
- `create` 用返回实体填 cache 而不是假 id（`api/providers.ts:40-47`）。
- 乐观更新逻辑与后端语义的镜像在 `lib/toolBinding.ts`：
  `upsertBindingEntry`(33-50) 镜像 `crud.rs:376-391`，
  `removeBindingEntry`(57-66) 镜像 `crud.rs:468-473`，
  `pruneRolesForProvider`(74-79) 镜像 `crud.rs:482-494`。
  **这是三份手工同步的逻辑复制**，靠 `lib/__tests__/toolBinding.test.ts`（97 行）单侧覆盖。
- autosave：`hooks/useAutosave.ts`，600ms debounce（`useAutosave.ts:15`），
  失败后不再 re-arm 直到 `changeToken` 变化（`useAutosave.ts:7-10`、`useAutosave.ts:34-40`），
  in-flight 期间的编辑会在同一个 promise 内续跑（`useAutosave.ts:51-59`），
  `flush()` 供抽屉关闭时 best-effort 落盘（`useAutosave.ts:80-84`，调用点 `ProviderEditorDrawer.tsx:80` 附近）。

### 3.5 i18n

- 两个 locale 文件：`src/i18n/locales/zh-CN.json` 与 `en.json`，顶层 41 个命名空间。
- `models.*` 下 **28 个子命名空间、429 个叶子 key**：
  `hub`(10) `card`(27) `status`(11) `dialog`(41) `appAi`(24) `drawer`(9) `tabs`(4)
  `connectionTab`(16) `modelsTab`(15) `claudeMapping`(16) `ompRoles`(44) `advancedTab`(15)
  `preset`(24) `guide`(7) `deleteDialog`(5) `gallery`(17) `save`(9) `picker`(7)
  `configFiles`(12) `launch`(2) `conflicts`(8) `diagnosticsPanel`(24) `toasts`(29)
  `errors`(4) `modelFormat`(5) `sidebar`(4) `matrix`(17) `editorPage`(23)。
- 命名空间与组件树**不对齐**。逐个 namespace 反查引用方（`grep -rl "models\.<ns>\." src/`，排除 `.test.`）后，
  以下 6 组的**全部引用方都在 §3.3 的死代码岛里**：

  | namespace | key 数 | 唯一引用方 |
  | --- | --- | --- |
  | `card` | 27 | `AgentHeroCard` / `MultiProviderCard` / `AgentSettingsDialog`（均死）+ `lib/agentRegistry.ts:41` 的 `taglineKey`（表活，但渲染方全死） |
  | `status` | 11 | `AgentStatusPill`（死） |
  | `dialog` | 41 | `AgentSettingsDialog` / `CodexSettingsForm` / `ClaudeModelMapping`（均死） |
  | `gallery` | 17 | `ProviderGalleryCard`（死） |
  | `configFiles` | 12 | `AgentConfigFiles` / `AgentSettingsDialog`（均死） |
  | `launch` | 2 | `AgentLaunchCommand`（死） |

  合计 **110 个 key（约占 `models.*` 的 26%）已经没有渲染方**，两个 locale 各一份。
  （`picker`(7) 一度看起来同类，实际仍被活代码 `ModelsTab` / `ProviderSelectPopover` / `ModelSelectPopover` 使用，不算在内。）
- 硬编码文案豁免：`i18n_hardcoded_baseline.txt:28-31` 只保留 4 个 prototype 文件。
  但生产路径上仍有裸英文字符串未走 i18n，例如 `VariantB2b.tsx:308` 的 `Bind`、
  `VariantB2b.tsx:405` 的 `title="Unbind"`、`VariantB2b.tsx:140` 的 `← Back`、
  `RichMatrixShell.tsx:82` 的 `title="Provider × Agent"`、
  `RichMatrixShell.tsx:107` 的 `Provider` 表头、`EditorPage.tsx` 的 `Back`。
  这些没有进 baseline，**说明 `check_i18n_hardcoded.sh` 的判据抓不到它们**（未确认具体判据）。

---

## 4. 测试与门禁

### 4.1 锁死当前设计的 Rust 测试

| 测试 | 位置 | 锁住了什么 | 重设计时 |
| --- | --- | --- | --- |
| `registry_covers_exactly_the_known_agents_in_order` | `tool_sync/agents.rs:249-264` | 6 个 Agent 的 id 与顺序 | **改 Agent 集合就要改它**（与前端同名测试成对） |
| `registry_kind_matches_store_layer_decision_point` | `agents.rs:343-353` | `AgentKind` ↔ `agent_supports_multiple_providers` | 必须保留（真的抓过分叉） |
| `registry_paths_match_legacy_resolvers` | `agents.rs:290-312` | `files[0].resolve` ↔ `resolve_tool_config_path` | 若删掉 legacy 解析器可一并删 |
| `registry_files_match_legacy_editor_listing` | `agents.rs:314-326` | 文件清单 ↔ `list_tool_config_files` | 同上 |
| `registry_default_content_matches_legacy_editor_defaults` | `agents.rs:328-341` | skeleton 内容 | 同上 |
| `required_url_pins_activation_validation_rules` | `agents.rs:371-380` | Anthropic/Openai 分类 | 若引入第三种 URL 需求要改 |
| `registry_display_names_match_legacy_targets` | `agents.rs:355-369` | 5 个显示名字面量（**遗漏了 omp**） | 可放宽 |
| `test_get_all_presets_flat_count` | `providers/tests/part1.rs:248-251` | preset 数量 = 13 | **加/删 preset 必红**，是纯计数断言，价值低 |
| `every_preset_id_maps_through_skillstar_providers` | `providers/tests/part3.rs:590-598` | 每个 preset 有 identity | **必须保留**（跨 crate 一致性） |
| `every_preset_id_resolves_to_a_provider_identity` | `part3.rs:571-587` | preset → entry 的 `preset_id` 回写 | 保留 |
| `native_official_preset_ids_resolve` | `skillstar-providers/src/identity.rs:208-222` | 两个 Official id 的 identity 映射 | 若重命名 Official 概念要改 |
| `granularity_mismatches_are_pinned` | `identity.rs:184-195` | glm/kimi 的 1:N 粒度 | 保留 |
| `omp_role_and_thinking_registries_match_the_frontend` | `tool_sync/tests/part4.rs:925-938` | 10 个角色 + 9 个 thinking level 的**字面量与顺序** | 与 `lib/__tests__/ompRoles.test.ts` 成对；改角色集合两侧都要改 |
| `omp_output_is_accepted_by_the_real_binary` | `part4.rs:831-` | 落盘产物被真实 `omp` 二进制接受 | 高价值，保留 |
| `test_get_tool_config_targets_returns_all_tools` | `tool_sync/tests/part1.rs:30-` | targets 数 = 6 + 各自路径子串 | 与 Agent 集合绑定 |
| `codex_binding_writes_one_table_per_provider_plus_pointer` / `..._preserves_user_provider_and_replaces_stale_managed` | `tests/part4.rs` | Codex TOML 逐字节形状 | 改写盘格式必红（这是好事） |
| `opencode_binding_writes_blocks_and_model_selector` / `pi_binding_…` / `omp_binding_…` | `tests/part4.rs` | 三家 JSON/YAML 形状 | 同上 |
| `*_unsync_leaves_user_owned_default_pointer_alone` (pi/omp) | `tests/part4.rs` | 不碰用户自有指针 | **必须保留**（用户数据安全） |
| `omp_skips_roles_that_would_dangle` / `omp_unassigning_a_role_removes_it_but_keeps_user_roles` | `tests/part4.rs` | 角色悬空防护 | 必须保留 |
| `prop_migration_preserves_all_provider_data` / `prop_migration_preserves_metadata_fields` | `providers/tests/part4.rs` | proptest：v1→v2 不丢数据 | 若删 v1 迁移可一并删 |
| `test_migrate_store_if_needed_v2_to_v3` / `_v1_*` | `providers/tests/part3.rs` 等 | 迁移路径 | 同上 |
| `test_create_provider_flat_infers_third_party_codex_defaults` 等 4 个 | `providers/tests/part2.rs` | Codex 默认推断规则 | 若把 codex 字段搬离 Provider 行要改 |
| `test_activate_claude_official_skips_url_gate` / `test_activate_codex_official_forces_oauth_settings` | `providers/tests/*` | Official 两条特例 | 重设计 Official 时改 |
| `binding_settings_survive_a_reactivation` / `entry_settings_and_binding_settings_are_independent` | `providers/tests/part5.rs` | **两层设置袋的独立性** | 若合并成一层，这两个测试就是要删的 |

统计：`providers/tests/` 5 个 part 共 2352 行、`tool_sync/tests/` 4 个 part 共 2087 行、
`agents.rs` 内联 8 个一致性测试。

### 4.2 前端测试

| 测试 | 锁住 |
| --- | --- |
| `lib/__tests__/agentRegistry.test.ts` | 6 个 toolId 字面量 + `CONFIG_FILE_TOOLS` + kind + requiredUrlField（与 Rust 成对，`agentRegistry.test.ts:5-13` 的注释明说） |
| `lib/__tests__/ompRoles.test.ts`(168) | 角色/thinking 列表与 Rust 对齐 |
| `lib/__tests__/toolBinding.test.ts`(97) | upsert / remove / prune 的镜像逻辑 |
| `lib/__tests__/providerPatch.test.ts`(175) | Codex 默认推断规则（前端副本） |
| `lib/officialProviders.test.ts`(81) | Official 判定与注入 |
| `matrix/rich/OmpRolePanel.test.tsx`(160) / `ClaudeMappingPanel.test.tsx`(49) | 生产面板行为 |
| `api/__tests__/activations.test.ts`(149) / `providers.test.tsx`(96) | mutation 行为 |
| `hooks/__tests__/useAutosave.test.tsx`(130) | autosave 状态机 |
| `components/hub/ProviderGalleryCard.test.tsx`(46) | **测试一个死组件** |
| `lib/__tests__/agentStatus.test.ts`(119) | **测试死代码岛的 lib** |

### 4.3 门禁脚本

`scripts/internal/` 共 10 个 check 脚本。与 Models 相关的现状：

| 脚本 | 对 Models 的效力 |
| --- | --- |
| `check_file_size.sh` | 已跑：**0 new over-limit**。Models 侧最大文件是 `tool_sync/tests/part4.rs`(974) 和 `EditorPage.tsx`(773)，都未超 1000 行硬线，但 part4.rs 距离 1000 只剩 26 行 |
| `check_no_orphan_modules.sh` | **只覆盖 `.rs`**（脚本头注释明写 "every `.rs` file"）。TS 死代码完全不在射程内 |
| `check_feature_imports.sh` | baseline 里**没有任何 models 条目**（`grep` 无命中） |
| `check_command_boundaries.sh` | baseline 里**没有任何 models 条目** |
| `check_generated_types.sh` | Models 类型不在生成范围内，因此该门禁对 Models 的 DTO 漂移零作用 |
| `check_i18n_hardcoded.sh` | baseline 保留 4 个 prototype 文件；生产路径上仍有未被抓到的裸英文（见 §3.5） |
| `check_error_strings.sh` / `check_clippy_ratchet.sh` / `check_dep_graph_doc.sh` / `check_workspace_deps.sh` | 通用 |

### 4.4 测试隔离约定

- **所有 tool-sync 测试必须持有 `use_sandbox_home()` 返回的 guard**（`tool_sync/tests/mod.rs:16-44`）。
  实现是 thread-local override（`tool_sync/mod.rs:89-104`），不是环境变量。
- 集成测试（在 crate 外编译）必须显式设 `SKILLSTAR_TOOL_SYNC_HOME`（`tool_sync/mod.rs:60-64`）。
- `cfg(test)` 兜底：即使忘了设，也会落到 `skillstar-toolsync-test-<pid>` 临时目录（`mod.rs:107-117`）——
  这是硬安全网，防止污染开发者真实 `~/.codex`。
- provider store 测试用 `SKILLSTAR_DATA_DIR`（`providers/store.rs:10-15` 的注释）。
- 路径可测性模式：每个 writer 都有 `_inner` / `_at` 变体接受显式路径
  （`sync_codex_binding_inner`(`multi_provider.rs:215`)、`unsync_codex_all_at`(`multi_provider.rs:559`)、
  `sync_omp_binding_inner`(`omp_provider.rs:167`)、`unsync_omp_all_at`(`omp_provider.rs:341`)、
  `unsync_pi_all_at`(`multi_provider.rs:637`)）。

---

## 5. 痛点清单

每条格式：**现象 → 证据 → 根因 → 它约束了什么**。

### 5.1 概念冗余：一个「Provider 被某 Agent 使用」的事实有 5 个名字

**现象**：讨论同一件事时必须先对齐词汇——preset / provider / binding / activation / entry / tool / official seed
到底哪个是哪个，代码里也在混用。

**证据**：
- `tool_activations` 是字段名，值却是 `ToolBinding`（`providers/types.rs:105`）——键名说 activation，类型说 binding。
- `ToolActivation` 是「一条 entry」（`types.rs:216` 的 doc 自己说 "One provider+model binding entry"），
  但类型名叫 activation。
- `activate_tool` 既做「新增绑定」又做「切换 active」（`crud.rs:376-391`），
  另有专门的 `set_active_binding`（`crud.rs:415`）和 `remove_binding_entry`（`crud.rs:447`）。
- `deactivate_tool` 不是 `activate_tool` 的逆——它清空**全部** entry（`crud.rs:559-569`）。
- `preset` 与 `official seed` 混在同一张表（`presets.rs:266-311`），靠 id 白名单区分。
- `ProviderIdentity` 是第 4 套 id（`identity.rs:21`），`ai_provider` 的 `app_id`（`"claude"|"codex"`，
  `resolve.rs:285`）是第 5 套。

**根因**：这些概念是**分三次增量加上去的**（v1 per-app → v2 单 activation → v3 binding + 两层设置袋），
每次都在旧词汇上叠加而不是重命名，磁盘字段名又必须向后兼容，于是名字被冻结在了最早那一次的语义上。

**它约束了什么**：任何重设计如果保留 `tool_activations` 这个磁盘键名，就继续背着这个错位；
如果要改，就需要一次 v3→v4 迁移。这是「能不能顺手改名」的关键分叉点。

### 5.2 Claude CLI / Desktop 的特例不是孤例，而是一类

**现象**：`claude-desktop` 这一列在注册表里长得像别的 Agent，但几乎每个列都是特例。

**证据**（逐项）：
| 维度 | 特例内容 | 位置 |
| --- | --- | --- |
| Official 种子 | 与 `claude-code` 共用 `claude-official`，不拆 `claude-desktop-official` | `officialProviders.ts:44` |
| 安装探测 | `binary_found` 强制为 false | `tools.rs:248-252` |
| 安装判据 | 只认桌面 app | `tools.rs:287-293` |
| 写盘 | 只写自造标记文件，不投影原生配置 | `sync.rs:305-328` |
| detect | 读自己写的标记 | `sync.rs:342-353` |
| unsync | 删文件而不是删字段 | `sync.rs:331-339` |

同类特例还有 3 处：
- **Codex Official 强制 oauth**：写在 `crud.rs:345-356`（activate 时）+ `presets.rs:350-354`（create 时），两处。
- **Native Official 跳过 URL 校验**：`crud.rs:295`。
- **Codex 的 `wire_api`/`auth_mode` 字段长在通用 Provider 行上**：`providers/types.rs:158-165`。

**根因**：`AgentSpec` 是「一行 spec + 三个函数指针」的形状，
凡是不能用数据表达的差异都溢出成了调用点的 `if tool_id == "..."`。
`tools.rs:248`、`tools.rs:287-293`、`conflicts.rs:39` 是三处硬编码 tool id 分支。

**它约束了什么**：新增 Agent 的成本不是「加一行 spec」，见 §5.4。
另外，「Claude Desktop 列」在重设计时应当被明确定性：
它今天要么是一个**未完成的功能**（原生投影 TBD），要么应当从矩阵移除——不能继续假装它已交付。

### 5.3 角色路由只有 OMP 有，且这是能力缺失而非刻意

**现象**：矩阵里只有 OMP 列的单元格能配置多模型角色，其他 5 列都是单模型。

**证据**：
- Rust：`ToolBinding.settings` 是通用的 binding 级袋（`providers/types.rs:247`），
  doc comment 明说「OMP's `modelRoles` map is the first consumer」（`types.rs:236-240`）——
  设计上就是可扩展的接缝。
- `docs/decisions.md` D-025 的「后果」段落自己写：
  「binding 级设置袋对未来其他"跨 entry 配置"（OpenCode 的 `small_model`、Claude 的层级模型）是现成接缝」。
- 前端：`VariantB2b.tsx:77` 用 `column.bindToolId === OMP_TOOL_ID` 硬分支决定用 `OmpRoleCell` 还是 `InlineSelectCell`。
- Claude 的层级模型（Haiku/Sonnet/Opus）**后端已经支持**（`sync.rs:108-119` 写三个 `ANTHROPIC_DEFAULT_*_MODEL`），
  但它走的是 `provider.meta` 而不是 binding settings——**同一类需求，两套存储位置**。

**根因**：Claude 的层级模型先落在 `meta` 里（因为那时还没有 binding 级袋），
OMP 的角色后落在 binding settings 里。两者从未收敛。

**它约束了什么**：重设计时「模型角色」应该是一个**跨 Agent 的一等概念**
（Claude 的 haiku/sonnet/opus、OMP 的 10 个 role、OpenCode 的 small_model 是同一个东西的不同方言），
而不是 OMP 的私有特性。这是数据模型层面最有价值的一次收敛机会。

### 5.4 新增一个 Agent 要改的地方远不止「一行 spec」

**现象**：`agents.rs:10-11` 的 doc 说「adding an agent means adding one row here plus its writer functions」，
实际清点是 **11 处**。

**证据**（以最近加的 `omp` 为参照，见 `docs/decisions.md` D-018 的「证据」段）：

| # | 位置 | 改什么 |
| --- | --- | --- |
| 1 | `tool_sync/agents.rs:198-224` | `AgentSpec` 一行 |
| 2 | `tool_sync/paths_files.rs:88-99` | 2 个路径 resolver |
| 3 | `tool_sync/omp_provider.rs` 全文（379 行） | writer + unsync + detect |
| 4 | `tool_sync/mod.rs:41-42` | `mod` + `pub use` |
| 5 | `tool_sync/agents.rs:250-264` | 一致性测试的字面量 |
| 6 | `tool_sync/agents.rs:359-365` | display name 测试字面量（omp **漏了**，见 §5.9） |
| 7 | `tool_sync/tests/part1.rs:32` | targets 计数 |
| 8 | `src/features/models/lib/agentRegistry.ts:11` + `:97-106` | `ProviderToolId` union + descriptor |
| 9 | `src/features/models/lib/agentRegistry.ts:129-136` | `CONFIG_FILE_TOOLS` |
| 10 | `src/features/models/components/hub/prototype/matrix/matrixColumns.ts:10` + `:54-59` | 列 union + 列定义 |
| 11 | `src/features/models/lib/__tests__/agentRegistry.test.ts:13` + `:28-35` | 两处字面量 |

另外还有 i18n（`models.card.taglines.<agent>`）、图标（`AgentToolIcon`）、
以及若该 Agent 有 Official 则 `officialProviders.ts:39-47` 的两个函数。

**根因**：注册表只统一了 Rust 侧的**调度**，没有统一**类型空间**——
`ProviderToolId` 是 TS union 字面量、`MatrixColumnId` 是第二份 TS union、Rust 侧是 `&'static str`。
三处必须手工同步，靠成对测试兜底。

**它约束了什么**：如果重设计想让「加 Agent」变便宜，必须先解决 toolId 的单一来源问题
（生成 TS union，或让前端从 `get_tool_config_targets` 运行时拿列表——注意后者今天恰好没有调用方）。

### 5.5 前端 create 流程自带一份与后端不一致的 preset 表（生产 bug 级）

**现象**：从生产界面「添加 Provider」创建的 DeepSeek / Kimi provider，**没有 Anthropic 端点**，
因此**永远无法绑定到 Claude CLI 或 Claude Desktop 列**。

**证据**：
- 前端硬编码 `CREATE_PRESETS`（`EditorPage.tsx:84-125`），只有 5 条：
  `deepseek`(anthropic=`""`, :91) / `kimi`(anthropic=`""`, :99) / `glm`(有 anthropic, :107) /
  `openrouter`(anthropic=`""`, :115) / `custom`。
- Rust 注册表里 DeepSeek 的 `base_url_anthropic = "https://api.deepseek.com/anthropic"`（`presets.rs:118`），
  Kimi 是 `"https://api.moonshot.cn/anthropic"`（`presets.rs:135`）。
- 激活 `claude-code` 要求 `base_url_anthropic` 非空（`crud.rs:302-309`）；
  前端单元格在 URL 为空时直接渲染「需要 Anthropic URL」占位（`VariantB2b.tsx:193-199`）。
- 创建时 `models_url` 被硬写成 `""`（`EditorPage.tsx:205`），
  而 `ClaudeMappingPanel` 的「获取模型列表」需要 `models_url` 非空（`ClaudeMappingPanel.tsx:134`）——
  所以新建 provider 的模型拉取按钮也是灰的。
- 后端 preset 命令 `get_provider_presets_flat` 有 13 条且被 `api/presets.ts:39` 正常包装，
  但**唯一消费者是 `useProviderForm`**（编辑态）和已死的 `PresetPicker`——create 流程绕开了它。
- 这直接违反 `docs/features/models/README.md:72`：「built-in preset 由 Rust command 返回，TypeScript 不复制 registry」。

**根因**：D1 原型自己写了一个轻量创建表单用于演示，晋升生产时没有换回真实 preset 源。

**它约束了什么**：这是**必须在重设计前先确认的既有行为**——
有多少用户的 store 里已经存着 `base_url_anthropic=""` 的 DeepSeek 行？
迁移时是否要按 `preset_id` 回填端点？（回填会覆盖用户手动清空的选择，不回填则用户继续绑不上 Claude。）

### 5.6 Claude 角色映射面板完全不持久化

**现象**：矩阵里 Claude 列的单元格点开是一个「显示模型 / 请求模型 / 1M」的映射面板，
填完关掉再打开就没了；磁盘上也从来没写过。

**证据**：
- 映射状态是 `VariantB2b.tsx:44` 的 `useState<Record<string, ClaudeMapState>>`，
  key 是 `${toolId}::${providerId}`（`VariantB2b.tsx:37-39`），**没有任何持久化调用**。
- `ClaudeMappingPanel` 的 `onChange` 只回调父组件的 setState（`ClaudeMappingPanel.tsx:139-141`）。
  面板里唯一真正写盘的是「获取模型列表」（`ClaudeMappingPanel.tsx:165-171`，写 `models` + `meta.model_catalog`）。
- 「一键设置」只 `onChange` + toast + `onStub`（`ClaudeMappingPanel.tsx:144-152`），
  `data.stub` 在 PROD 下是空函数（`usePrototypeHub.ts:84-89`）。
- 后端**已经准备好接收**：`sync_to_claude_code` 从 `provider.meta` 读
  `claude_haiku_model` / `claude_sonnet_model` / `claude_opus_model`（`tool_sync/sync.rs:108-119`）。
- 全仓写这三个 meta key 的地方只有 `AgentSettingsDialog.tsx:105-107`（死代码）
  和 `providerPatch.ts:232-235`（`buildProviderPatch`，但**没有任何 UI 编辑这四个表单字段**——
  `grep claudeHaikuModel src/features/models/components/provider/` 无命中，只有 `useProviderForm.ts:118-123` 的依赖数组）。
- `docs/features/models/README.md:61` 把这个状态描述为「Claude mapping UI 仍是前端本地状态（Agent 加法）」——
  文档是准确的，但用户看到的是一个看起来会保存的表单。

**根因**：原型阶段这一格只是视觉稿；晋升生产时接了 `activateTool`/`deactivateTool`（真实），
却没接映射的持久化。

**它约束了什么**：重设计必须决定这层映射的归属——
放 `provider.meta`（现状后端读的位置）还是放 `ToolBinding.settings`（与 OMP 角色同构，见 §5.3）。
两者不能都要。

### 5.7 多 provider 能力在 UI 上只对 OMP 兑现，其他三列语义是错的

**现象**：Codex / OpenCode / Pi 后端都是 `AgentKind::Multi`，可以同时绑多个 provider；
但在矩阵里，一个 multi 列**同时只有一行显示为已绑定**，且点「解绑」会把该 Agent 的**所有**绑定一起清掉。

**证据**：
- `InlineSelectCell` 判定「已绑定」用的是 `activeEntry(...)?.provider_id === provider.id`
  （`VariantB2b.tsx:360-361`）——只看 active 那一条，其余 entry 的行显示为未绑定的「Bind」按钮。
- 同一个组件的解绑按钮调 `data.deactivateTool(agent.toolId)`（`VariantB2b.tsx:407`），
  后端语义是清空整个 binding（`crud.rs:559-569`）。
- 对比 OMP 列做对了：`bound` 用 `bindsProvider(binding, provider.id)`（`VariantB2b.tsx:286`），
  解绑用 `removeBindingEntry(agent.toolId, provider.id)`（`VariantB2b.tsx:337`，注释还解释了为什么）。
- `set_active_binding` 命令存在但生产不可达（§2.1 #11）——
  所以「已绑 3 个 provider，想切到第 2 个」在生产 UI 里只能通过重新点第 2 个的模型下拉
  （`activate_tool` 会 upsert 并移动指针，`crud.rs:378-383`），无法只切指针。
- `useProvidersFlat` 根本没把 `setActiveBinding` 透出（`hooks/useProvidersFlat.ts:16`、`:28-43`）。

**根因**：OMP 列是最近一轮（D-025）新做的，做对了；另外三列还是原型的单 provider 心智。

**它约束了什么**：矩阵单元格的**状态语义必须重新定义**——
一个 (provider, agent) 格子至少要能表达：未绑 / 已绑非 active / 已绑且 active / 不兼容 / 冲突。
现在只有 3 种（不兼容 / 未绑 / 已绑=active）。

### 5.8 矩阵 IA 的伸缩性：列靠隐藏、行无过滤

**现象**：Provider 多了或 Agent 多了，矩阵就没法看了，而目前的应对手段只有「藏列」。

**证据**：
- 列控制是一个 icon carousel，只能整列显示/隐藏，至少保留 1 列（`usePrototypeHub.ts:41-50`）。
  默认全部 6 列可见（`matrixColumns.ts:62`）。
- 表格宽度：Provider 列固定 200px（`RichMatrixShell.tsx:91`），
  Claude 列 168px、其余 152px（`RichMatrixShell.tsx:93`），外层容器 `max-w-6xl`（`MatrixChrome.tsx:22`）。
  6 列全开需要 200+168×2+152×3 = 992px 加上 filler，在 1280 宽窗口下已经接近极限；
  再加 1~2 个 Agent 就必然横向滚动，而 Provider 列是 sticky 的（`RichMatrixShell.tsx:102`），
  所以横滚时列头与行头会持续遮挡。
- 行没有任何搜索/过滤/分组：`rows = matrixProviders(data.providers)`（`RichMatrixShell.tsx:79`）
  就是全量渲染，只按 `sort_index` 排序（`useProvidersFlat.ts:19-22`）。
  `MatrixChrome` 的 `toolbar` 插槽在生产变体里传的是 `legend={null}`（`VariantB2b.tsx:78`）——
  **搜索/过滤位是空的**。
- 单元格高度固定 `h-14`（`VariantB2b.tsx:196` 等），信息密度上限是两行：
  一行状态 + 一行模型名。冲突、延迟、余额、last_sync 都没有位置。
- 没有 `reorder_providers` 的 UI 触发（§2.1 #5），所以用户无法调整行顺序。

**根因**：矩阵 IA 是在「6 个 Agent × 少量 Provider」的原型语境下设计的，
没有承担过「20 个 Provider」或「加到第 8 个 Agent」的压力。

**它约束了什么**：如果重设计仍然保留矩阵，必须先回答：
行怎么收敛（分组/搜索/虚拟化）、列怎么收敛（分页/优先级/合并 Claude 两列）、
单元格要表达几种状态（见 §5.7）。如果这三个答案里有两个是「加控件」，那矩阵可能不是对的 IA。

### 5.9 生产代码放在 prototype 目录的实际代价

**现象**：目录名与生产状态不符，已经产生了具体的、可指认的损失。

**证据**：
1. **一个 6 行的别名文件成了唯一的「这是生产」标记**（`ia/VariantD1.tsx:6`）。
   任何人 grep `VariantB2b` 都会以为在看原型。
2. **i18n baseline 被迫做精细区分**（`i18n_hardcoded_baseline.txt:13-19` 用 7 行注释解释哪些 prototype 文件
   是生产、哪些不是），且**这个区分做漏了**：生产路径上的 `Bind` / `Unbind` / `← Back` /
   `Provider × Agent` 表头（§3.5）既不在 baseline 也没走 i18n。
3. **两份死 Variant 与生产 Variant 混在同一目录**（`VariantB2a.tsx` 159 行、`VariantB2c.tsx` 241 行），
   而它们和生产的 `VariantB2b.tsx` 共享 `RichMatrixShell`——
   任何对 shell 的改动都要同时不破坏两个没人用的调用方。
4. **`EditorPage.tsx`(773 行) 同时承担生产创建流程和原型编辑页**，
   它的 `detailStyle` 参数有三个取值（`"tabs"|"sections"|"split"`），
   其中 `"sections"` / `"split"` 只被 DEV-only 的 D2/D3 使用（`VariantD2.tsx:34`、`VariantD3.tsx:43`）。
   生产路径永远只走 `"tabs"`（`VariantB2b.tsx:67`）。
5. **`data.stub`**（`usePrototypeHub.ts:84-89`）是原型时代的「TODO 打印」，
   在生产路径上仍被 3 处调用（`ClaudeMappingPanel.tsx:150`、`:172`；`VariantB2b.tsx:160`、`:260`），
   PROD 下什么也不做——它标记的正是 §5.6 那些没接上的功能。
6. **`PrototypeHubData.stateDump`**（`usePrototypeHub.ts:104-135`）是一个 30 行的调试对象，
   每次 providers/activations 变化都会重算，生产路径上只有 DEV 的 D2/D3 消费它。

**根因**：晋升是通过「加别名」而不是「搬文件」完成的，因为搬文件会产生大 diff。

**它约束了什么**：这不是重设计的阻碍，而是**重设计前必须先做的清理**——
否则新旧代码将在同一个 prototype 目录下继续混居，判断「哪些能删」的成本只会更高。

### 5.10 死代码没有门禁，且拖着 10 条命令与 119 个 i18n key

**现象**：约 4000 行前端代码（27%）无生产引用，但 lint / build / test 全绿。

**证据**：
- 清单见 §3.2 + §3.3。
- `check_no_orphan_modules.sh` 脚本头注释明确写它只处理 `.rs`
  （"every `.rs` file inside a workspace member must be reachable"）。
- 死代码持有 10 条命令的唯一调用点（§2.1），因此这些命令的 Rust 侧测试仍在跑、
  仍在维护，但产品上无人能触发。
- 死代码还配着自己的测试（`ProviderGalleryCard.test.tsx` 46 行、`agentStatus.test.ts` 119 行）——
  **绿色的测试正在为不可达的代码背书**。
- i18n 侧 110 个 key（§3.5 已逐 namespace 反查）× 2 个 locale 处于同样状态。

**根因**：D1 IA 替换了旧的「Agent 卡片 + 设置对话框」IA，旧组件被留在原地而不是删除
（大概率是「万一要回滚」）。回滚窗口早已过去。

**它约束了什么**：重设计必须先做一次「哪些能删」的判定，
否则新 IA 会成为第三代，而第一代（agents 卡片）和第二代（prototype D2/D3）还在仓库里。
另外：`AgentSettingsDialog` 里其实实现了 Codex `wire_api`/`auth_mode` 切换、
配置文件编辑器、Claude 层级模型编辑——**这些能力今天在产品上不存在，但代码是写好的**。
重设计时它们是「要不要重新接回来」的候选，不是纯垃圾。

### 5.11 DTO 手写 mirror 已经产生实质漂移

**现象**：14 个跨 IPC 结构全部手抄，其中至少 3 处已经与 Rust 不一致。

**证据**：
| 漂移 | Rust | TS |
| --- | --- | --- |
| `CodexSettings.auth_mode` | 三态（`tool_sync/types.rs:31-35`） | `"api_key" \| "oauth"`（`src/types/models.ts:96`）——**少了 `third_party`**，而 `third_party` 正是所有第三方 provider 的默认值（`crud.rs:26`）。前端另有一份正确的 `CodexAuthMode`（`lib/providerPatch.ts:10`） |
| `ProviderPatchFlat.preset_id` | 有（`providers/types.rs:314`） | 无（`src/types/models.ts:174-188`） |
| `ToolConfigFileInfo.format` | 实际有 `"yaml"`（`agents.rs:210`） | 注释只写 `"json" \| "toml"`（`src/types/models.ts:217`，类型是 `\| string` 兜底） |
| `get_tool_config_targets` 入参 | 无参（`tools.rs:13`） | 声明 `{ app_id: AppId }`（`src/lib/ipc/commands/models.ts:92`） |
| `ConfigConflict` | `tool_sync/types.rs:290` | 第三份定义在 `src/lib/ipc/commands/models.ts:19-25` |
| `detect_provider_conflicts` / `detect_tool_installation` 返回值 | `Vec<serde_json::Value>` / `serde_json::Value`（`tools.rs:377`、`tools.rs:238`） | 前端定义了结构（`models.ts:19-31`）但后端不保证 |

`docs/errors.md:27` 已经记过一次同类事故（「手抄的 DTO 会把"键缺席"抄成 `null`」），
`docs/decisions.md` D-025 的「后果」段也明确承认了这个缺口未修。

**根因**：`ts-rs` 在 `skillstar-models` 里只接了 `mcp` 模块，Provider/Tool 侧从未接入。

**它约束了什么**：重设计如果要改数据模型，**先接 ts-rs 再改**，否则每改一个字段都要人肉同步 14 个类型。
这是重设计的前置条件，不是可选优化。

### 5.12 store 读写的健壮性与原子性缺口

**现象**：三个可以让用户「丢配置」或「看到错误状态」的路径。

**证据**：
1. **解析失败静默返回空 store**：`read_flat_store` 对 IO 错误和 JSON 错误都返回 `FlatProvidersStore::default()`
   （`providers/store.rs:44-63`），只 `warn!`。用户看到的是「所有 provider 消失」。
   紧接着如果触发一次写（例如 `get_providers_flat` 的 `ensure_official_providers` 判定为需要写盘，
   `provider_cmds.rs:84-86`），损坏的文件就会被**空 store + 两个 Official 种子覆盖**。
   注意 `migrate_store_if_needed` 在 JSON 解析失败时同样返回默认（`store.rs:124-132`），**且不备份**。
2. **激活是两次写盘**：`tools.rs:49`（写 binding）→ `tools.rs:54`（写磁盘配置）→
   `tools.rs:66`（写 `last_sync_at`）。中途失败会留下「store 说已绑定，`last_sync_at` 为 None」的状态，
   于是外部修改检测永远不触发（`conflicts.rs:114` 直接 `last_sync_ts?` 返回 None）。
3. **`meta.model_catalog` 无体积上限**：`ModelCatalogEntry.raw` 保存整个上游 JSON（`model_catalog.rs:111`），
   OpenRouter 的 `/models` 返回数百个模型，整体被 pretty-print 进 `model_providers.json`。
   没有裁剪、没有单独文件、没有上限。

**根因**：store 层是为「小配置文件」设计的（v1 时代只有 URL + key），
model catalog 缓存是后来塞进 `meta` 的。

**它约束了什么**：重设计应当把 catalog 缓存**移出 provider 行**（独立文件或 sqlite），
并让解析失败走「报错 + 保留原文件」而不是「静默清空」。

### 5.13 文件体积：目前无超限，但两处逼近

已跑 `bash scripts/internal/check_file_size.sh`：
```
summary: 0 new over-limit, 1 baselined debt, 2 oversized test file(s) under cap, 0 stale baseline entries.
✓ No new over-limit files.
```
唯一 baseline 债务是 `src/features/shared-channels/components/SharedChannelsContent.test.tsx`（1808 行，与 Models 无关）。

Models 侧最大的文件：

| 文件 | 行数 | 距 1000 行硬线 |
| --- | --- | --- |
| `crates/skillstar-models/src/tool_sync/tests/part4.rs` | 974 | **26 行** |
| `src/features/models/components/hub/prototype/EditorPage.tsx` | 773 | 227 行 |
| `crates/skillstar-models/src/ai_provider/skill_pick.rs` | 691 | 309 行 |
| `crates/skillstar-models/src/tool_sync/multi_provider.rs` | 667 | 333 行 |
| `crates/skillstar-models/src/providers/tests/part2.rs` | 655 | 345 行 |
| `src/features/models/components/settings/AppAiModelsPicker.tsx` | 662 | 338 行 |
| `crates/skillstar-models/src/providers/crud.rs` | 569 | 431 行 |

`tool_sync/tests/part4.rs` 是 OMP 测试的落脚点，**下一个 OMP 相关测试就会把它推过 800 行的拆分建议线**
（AGENTS.md：接近 800 行时开始拆分），再多两三个就撞 1000。

### 5.14 其他已确认的小裂缝

| # | 现象 | 证据 |
| --- | --- | --- |
| 1 | `registry_display_names_match_legacy_targets` 只断言 5 个 Agent，**omp 漏了** | `tool_sync/agents.rs:359-365` |
| 2 | `AgentConfigFileSpec.format` 允许 `"env"`，但没有任何 spec 使用 | `agents.rs:284` vs `agents.rs:87-225` |
| 3 | `ModelCatalogFetchResult.metadata_sources` 永远是空数组 | `model_catalog.rs:74` 硬写 `Vec::new()` |
| 4 | Codex unsync 无条件删 `model_provider`，sync 路径却是条件删 | `multi_provider.rs:576` vs `:291-298` |
| 5 | conflicts 的 env 检查只覆盖 Claude/Codex，OpenCode/Pi/OMP 的 env 覆盖不检测 | `conflicts.rs:10-17` |
| 6 | conflicts 描述文案硬编码中文，不走 i18n | `conflicts.rs:68-71`、`:128-131`、`:179-181` |
| 7 | `created_at` 是毫秒、`last_sync_at` 是秒，同一个 store 内两种时间单位 | `crud.rs:97-101` vs `tools.rs:364-368` |
| 8 | `create_provider_flat` 用「等于 serde 默认值」推断 Codex 默认，用户显式选 `responses` 与未选不可区分 | `crud.rs:88-93` |
| 9 | Native Official 种子没有 API key，因此**无法作为应用内 AI 的 provider**（resolve 直接 bail） | `ai_provider/resolve.rs:52-54` |
| 10 | `ai_provider` 的 `app_id` 只认 `"claude"`/`"codex"`，与 6 个 toolId 无关 | `resolve.rs:285-287` |
| 11 | v1 legacy store 的唯一生产读取点是 `ai_provider` 的 fallback | `resolve.rs:303-304` |
| 12 | `reorder_providers` 命令与 mutation 都存在，**没有任何 UI 触发** | `api/providers.ts:116` 无调用方 |
| 13 | `updateSettingsMutation` 是唯一没有 optimistic 分支的 mutation | `api/activations.ts:117-135` |
| 14 | `withEnsuredOfficialProviders` 在前端注入 `sort_index: -1` 的种子，与后端 `ensure_official_providers` 的 `max+1` 排序相反 | `officialProviders.ts:125` vs `presets.rs:57-62` |

---

## 6. 重设计的硬约束清单

### 6.1 不能动（会破坏用户已有配置文件或已发布行为）

| # | 约束 | 为什么 |
| --- | --- | --- |
| 1 | **`skillstar_` 托管键前缀语义** | 用户磁盘上的 `~/.codex/config.toml`、`opencode.json`、`~/.pi/agent/models.json`、`~/.omp/agent/models.yml` 里已经有 `skillstar_<id8>` 块。`is_skillstar_managed_key`（`multi_provider.rs:51-56`）同时匹配旧的裸 `skillstar` 和新的 `skillstar_*`——**这两个都要继续认**，否则旧块变成永久垃圾。 |
| 2 | **`skillstar_managed_key` 的推导规则** | 键是**现算**的（`multi_provider.rs:29-47`），不落盘。改规则 = 旧块认不出来 = 用户配置里留下孤儿块。若必须改，先做一次「按旧规则清理」的迁移。 |
| 3 | **retain-then-write 的写盘策略** | 五个 writer 一致：先删所有托管块再写当前集合。用户在 UI 取消绑定后磁盘上不留残留，这是已发布的行为契约（`docs/features/models/README.md:37`）。 |
| 4 | **「只碰自己管的字段」** | Claude 只动 6 个 env key（`types.rs:409-416`）；Pi/OMP 只在指针指向托管块时才清（`multi_provider.rs:655-663`、`omp_provider.rs:357-377`）。这条被多个测试锁死（`*_unsync_leaves_user_owned_default_pointer_alone`）。**不能放松。** |
| 5 | **备份先于写** | 每个写路径都先 `create_rolling_backup`（保留 5 份，`backup_merge.rs:11-27`）。用户已经依赖 `.bak.<ms>` 文件恢复。 |
| 6 | **Native Official 的两个稳定 id** | `claude-official` / `codex-official` 是磁盘上的 provider `id`（不是 UUID，`presets.rs:343-348`）。用户 store 里已经有这两行，且 `tool_activations` 里可能有指向它们的 entry。**改 id = 用户的「原生登录」绑定失效。** |
| 7 | **Claude Official 被两条 binding 共用** | `claude-code` 与 `claude-desktop` 共用同一个种子 id（`officialProviders.ts:44`）。如果重设计要拆成两个种子，需要迁移已有的 `claude-desktop` binding。 |
| 8 | **`tool_activations` 磁盘键名与 `ToolBinding` 的 serde 形状** | `entries` / `active_index` / `settings` 三个字段名已落盘（`providers/types.rs:241-249`）。改名需要 v3→v4 迁移。 |
| 9 | **`OmpSettings.roles` 的值形状** | `{ provider_id, model, thinking? }`（`types.rs:116-123`）已落盘在 `ToolBinding.settings`。 |
| 10 | **`meta` 里已有的 5 类 key** | `model_catalog` / `claude_*_model` / `baseURL`（见 §1.2 表）。读取方分散在 tool_sync 和 ai_provider，删 key 前必须同时改读取方。 |
| 11 | **`SKILLSTAR_TOOL_SYNC_HOME` 与 `SKILLSTAR_DATA_DIR`** | 测试隔离与用户沙箱都依赖它们（AGENTS.md 硬性要求；`tool_sync/mod.rs:56`）。**任何新写盘路径必须继续走 `sync_home_dir()`**，不能直接 `dirs::home_dir()`。 |
| 12 | **上游 home 覆盖 `CODEX_HOME` / `GROK_HOME`** | 用户移动过 Codex home 时，写错位置 = 凭证 CLI 读不到（`sync.rs:19-25` 的注释记录了这个坑）。 |
| 13 | **`FlatProvidersResponse` 的 snake_case 序列化** | 已经踩过 `toolActivations` vs `tool_activations` 的坑（`models_commands/mod.rs:60-66`）。 |
| 14 | **v1→v3 迁移链** | 只要还有用户可能持有 v1/v2 文件，`migrate_store_if_needed` 就不能删。要删需要先确认最低支持版本。 |
| 15 | **写命令共享 `ProvidersWriteLock`** | 并发写 `model_providers.json` 的唯一保护（`models_commands/mod.rs:44-46`）。新命令必须继续持锁。 |
| 16 | **`~/.zshrc` 只在用户显式点击时写** | `docs/features/models/README.md:25` 的明文约束；autosave 不得产生该副作用。 |

### 6.2 可以动（无外部契约，只需内部一致）

| # | 可动项 | 代价 |
| --- | --- | --- |
| 1 | **前端 `hub/prototype/` 的目录位置与文件名** | 纯内部；`i18n_hardcoded_baseline.txt` 的 4 行路径要跟着改 |
| 2 | **§3.2/§3.3 列出的约 4000 行死代码** | 删除即可；同时删对应的 i18n key（110 个 × 2 locale）和 3 个测试文件。注意 `AgentSettingsDialog` 里有产品上不存在但已实现的能力（Codex 设置、配置文件编辑器、Claude 层级模型），删前先决定是否重新接回 |
| 3 | **`EditorPage.tsx` 的 `detailStyle` 三态** | `"sections"`/`"split"` 只服务 DEV-only 的 D2/D3，随 D2/D3 一起删 |
| 4 | **`usePrototypeHub.stateDump` 与 `data.stub`** | 纯调试遗留 |
| 5 | **矩阵 IA 本身**（列/行/单元格的组织方式） | 无磁盘契约，纯 UI 决策 |
| 6 | **`ProviderEntryFlat` 上的 `codex_wire_api` / `codex_auth_mode` 迁到 per-entry settings** | 需要 v3→v4 迁移；但 per-entry `CodexSettings` 已经存在（`ToolActivation.settings`），且 `activate_tool` 已经会从 provider 行兜底（`multi_provider.rs:232-237`），迁移路径清晰 |
| 7 | **把角色路由从 OMP 专属提升为通用概念** | binding 级设置袋已经是通用的（`providers/types.rs:236-240` 的 doc 就是这么写的）；Claude 层级模型从 `meta` 迁到 binding settings 需要迁移 |
| 8 | **接入 ts-rs 生成 Provider/Tool 类型** | 一次性；`check_generated_types.sh` 已存在，`mcp` 模块是现成范式。**建议作为重设计的第一步** |
| 9 | **`get_all_presets_flat` 的 13 条内容与分类** | 内部数据；但 `test_get_all_presets_flat_count` 的计数断言要跟着改（这个断言本身价值低，可以换成「每条都有 identity」） |
| 10 | **`category` 字段把 Native Official 与 Grok 混在 `"official"`** | 可以拆成 `native_official` / `vendor_official`；`presets.rs:266-311` 的注释已经在描述这个区分 |
| 11 | **`ModelCatalogFetchResult.metadata_sources`** | 永远空，可删或实现 |
| 12 | **conflicts 的中文硬编码文案** | 应改为结构化 + 前端 i18n |
| 13 | **`AgentConfigFileSpec.format` 的 `"env"`** | 死枚举值，可删 |
| 14 | **`ProviderIdentity` 是线性扫描** | 可换 map；15 行规模下无所谓 |
| 15 | **两份写盘骨架（JSON / YAML）合并** | D-012 已给出「什么时候值得抽象」的判据：出现第二个同类 Agent 时再抽。目前 JSON 有 2 个调用方（OpenCode/Pi），YAML 有 1 个（OMP） |
| 16 | **`ai_provider` 的 `app_id`（`"claude"`/`"codex"`）** | 只在 `ai.json` 里，与 tool id 无关；若统一 id 空间需要迁移 `ai.json` |

### 6.3 重设计前必须先回答的三个问题

1. **§5.5 的 preset 漂移影响了多少存量数据？**
   有多少用户 store 里存着 `base_url_anthropic=""` 且 `preset_id="deepseek"` 的行？
   迁移时按 `preset_id` 回填端点，还是留给用户手动补？（回填会覆盖用户主动清空的选择。）

2. **Claude Desktop 列是「未完成」还是「应当移除」？**（§5.2）
   它今天在磁盘上不产生任何对 Claude Desktop 有意义的效果。
   如果保留，需要先做原生配置投影；如果移除，需要迁移已有的 `claude-desktop` binding。

3. **模型角色的归属：`provider.meta` 还是 `ToolBinding.settings`？**（§5.3 + §5.6）
   Claude 层级模型走前者（后端已实现、前端无 UI），OMP 角色走后者（两端都实现）。
   这是同一个概念的两套存储，重设计必须二选一——这个选择决定了迁移的形状。

---

## 附录 A：关键文件与行数速查

| 层 | 文件 | 行数 |
| --- | --- | --- |
| identity leaf | `crates/skillstar-providers/src/identity.rs` | 223 |
| identity leaf | `crates/skillstar-providers/src/balance.rs` | 91 |
| store | `crates/skillstar-models/src/providers/types.rs` | 322 |
| store | `crates/skillstar-models/src/providers/store.rs` | 454 |
| store | `crates/skillstar-models/src/providers/crud.rs` | 569 |
| store | `crates/skillstar-models/src/providers/presets.rs` | 374 |
| store | `crates/skillstar-models/src/providers/model_catalog.rs` | 184 |
| store tests | `crates/skillstar-models/src/providers/tests/part1..5` | 2352 |
| tool_sync | `crates/skillstar-models/src/tool_sync/agents.rs` | 387 |
| tool_sync | `crates/skillstar-models/src/tool_sync/types.rs` | 416 |
| tool_sync | `crates/skillstar-models/src/tool_sync/sync.rs` | 389 |
| tool_sync | `crates/skillstar-models/src/tool_sync/multi_provider.rs` | 667 |
| tool_sync | `crates/skillstar-models/src/tool_sync/omp_provider.rs` | 379 |
| tool_sync | `crates/skillstar-models/src/tool_sync/paths_files.rs` | 397 |
| tool_sync | `crates/skillstar-models/src/tool_sync/conflicts.rs` | 190 |
| tool_sync | `crates/skillstar-models/src/tool_sync/backup_merge.rs` | 228 |
| tool_sync tests | `crates/skillstar-models/src/tool_sync/tests/part1..4` | 2087 |
| ai | `crates/skillstar-models/src/ai_provider/resolve.rs` | 350 |
| commands | `src-tauri/src/commands/models_commands/` | 844 |
| frontend | `src/features/models/`（全部） | 14820 |
| frontend | `src/features/models/components/hub/prototype/` | 4200 |
| frontend types | `src/types/models.ts` | 248 |
| frontend ipc | `src/lib/ipc/commands/models.ts` | 135 |

## 附录 B：本轮实际执行的验证命令

```bash
bash scripts/internal/check_file_size.sh      # 0 new over-limit
```

未运行 `cargo test` / `bun run test`（本轮为只读审计，不改代码，无需回归验证）。
所有代码事实均通过直接阅读源文件 + `grep` 引用链取得，未依赖测试输出。

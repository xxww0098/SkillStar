# Models 与 AI

状态：active

本文件维护 Provider store、Agent tool sync、Models 工作台和应用内 AI 的当前契约。

## Provider 分层

- `skillstar-core::providers` 拥有 Provider identity、鉴权方案和余额 endpoint；模块本身不依赖任何产品域，Models 与 Usage 都从它派生。
- `skillstar-models::providers` 拥有 flat provider store、preset、tool binding 和 runtime resolve。
- `AiProviderRef` 从 crate root 导出，provider-ref 实现模块保持私有，调用方不依赖内部文件布局。无调用者的旧 Models circuit-breaker 不作为占位模块保留。
- Usage catalog 与 Models preset 可以不同，但都必须通过 guard test 映射到同一 Provider identity。
- 添加 Provider 从 `crates/skillstar-core/src/providers/identity.rs` 开始，再补 Models preset、余额解析 fixture 和映射测试；不得在 command/frontend 手写鉴权头。
- v1 per-app store 只是历史迁移来源：命令面与同步/CRUD 已删除，v1 类型和读取收缩为 crate 内部，仅供 v1→v2 migration 和 `ai_provider` 的 legacy provider-ref fallback 使用。新功能只能进入 flat v2 registry 和 API。

### v4 数据模型（已接入运行路径）

- v4 类型在 `providers/{provider,credential,binding,catalog}.rs`：`Provider` + `Endpoints`（每协议一个可选 URL）+ `ProviderCaps`（三态 `Tri`）+ `Credential`（判别联合）+ `AgentBinding`（含一等字段 `roles`）。裁决与理由见 [decisions.md](../../decisions.md) D-035。
- **能力位语义**：`Tri::Unknown` 是「需要检测」，**不是**「不支持」。只有探测明确返回 `No` 才允许禁用绑定入口。迁移期一律写 `Unknown`。
- **Official = `Credential::ExternalCli`**：不再靠 id 白名单分支。`claude-official` / `codex-official` 两个固定 id 保留（改 id 会让用户的原生登录绑定失效）。
- **角色路由归 `AgentBinding.roles`**：`provider.meta.claude_*_model` 与 `binding.settings.roles` 都迁到这里。键是开放 map，规范键为 `default` / `fast` / `plan` / `vision` / `subagent`，其余（含 OMP 的 `slow`、`designer`）原样保留为 extra 角色。
- **Desktop 能力以原生写盘效果判断**，不是注册表是否有行：sync 不再写 `skillstar-binding.json`。迁移报告不能把 marker 描述为已生效的原生配置，也不得擅自将 Desktop 绑定搬到 CLI。
- **磁盘格式已切换**：`model_providers.json` 现在以 v4 写盘（`version: 4`，`providers` + `bindings`）。启动入口是 `load_store_and_repair`：读→（必要时）迁移→落盘 catalog 缓存→写回 store。它不改 Agent 文件。`get_providers_flat` 是唯一调用它的命令，其余命令走 `load_store`。
- **v3 reader 拒绝未来版本**：`migrate_store_if_needed` 的 v1 分支是「排除法」到达的，而 v1 结构每个字段都有 serde 默认值——所以一个 v4 文件会被**成功**解析成四个空桶，然后覆盖用户的真实配置。现在遇到高于 `FLAT_STORE_VERSION` 的版本直接报错并保留原文件。
- **Settings 的 provider 列表仍是 v3 形状**，翻译集中在 `src-tauri/src/commands/models_commands/compat.rs`。写入是**打补丁**而非重建：v3 表达不了的 `caps` / `headers` / key 故障转移链 / `ext` 因此不会在每次保存时被清空。Models 页的 `get_models_board` 不经过这条缝，只返回 id 和 name。`compat.rs` 留到 Settings 不再调用 `get_providers_flat` 时再删。

### 迁移契约（v3 → v4）

- 纯函数 `migrate_v3_to_v4(v3, presets) -> MigrationOutcome`，无 IO、无时钟、无网络；catalog 只被**抽出**，由调用方落盘，写失败不中止（catalog 可重建，绑定不可）。
- **三条件回填**：仅当「当前 anthropic 端点为空」且「preset 有值」且「openai URL 与 preset 逐字相同」三条同时成立才回填。第三条区分「前端 bug 造成的空」与「用户主动清空」——后者回填等于覆盖用户的决定。`models_url` 同规则。回填结果必须经 modal（非 toast）告知并提供撤销。
- **备份与回滚**：见 [decisions.md](../../decisions.md) D-036。`model_providers.v3.json` 永久保留，是撤销按钮的依据。
- **单位**：v4 所有时间字段带 `_ms` 后缀。v3 的 `last_sync_at`（秒）迁移时 ×1000。

### 前端类型契约

- `ProviderDto` 定义在 `crates/skillstar-app/src/models/dto.rs`，**不带明文 key**：只有 `credential_kind` / `credential_summary`（掩码或变量名/路径/命令）/ `has_secret`。理由与后果见 D-037。
- 诊断命令收 `provider_id` 而非 key。草稿态需先落盘再探测。

## Tool binding 与写盘

- Agent binding 使用 `AgentBinding { entries, active_index, roles, settings }`；所有读写通过 helper/facade，不直接索引 `entries[active_index]`。
- **命令按职责拆分**（v3 的 `activate_tool` 一个名字干三件事、`deactivate_tool` 不是它的逆）：`bind_provider`（加一条并指向它）/ `set_active_binding`（只移动指针）/ `update_binding_entry`（只改条目，不动指针）/ `unbind_provider`（只摘一条）/ `unbind_agent`（清空，破坏性的那个现在必须点名）/ `update_binding_entry_settings`（entry 级设置袋）/ `update_agent_settings`（agent 级设置袋 + 角色）。
- **绑定按 wire protocol 校验**：注册表列从 `required_url` 换成 `required_wire`（`RequiredWire`）。`codex` 要求 `OpenaiResponses`，`claude-code` / `claude-desktop` 要求 `AnthropicMessages`，其余要求 `OpenaiChat`。`Credential::ExternalCli` 行豁免（空端点是它的语义）。`Tri::Unknown` 从不拒绝，只有探测得到的 `No` 才拒绝。
- Agent descriptor 的 `kind` 是 UI 和后端能力的共同开关：single 只激活一个 provider；multi 原生保留多个条目并维护 active 指针。
- Rust 侧 Agent 事实（binary、配置目录探测、文件清单、kind、`required_wire`、**角色清单**、sync/unsync/探测 dispatch）的 SSOT 是 `tool_sync::agents` 注册表；写盘、卸载、resync 与配置目标枚举都经它路由，新增 Agent 只加一行 spec 及其 writer。这条目标是**可证伪的**，不是口号：`a_synthetic_agent_syncs_through_the_registry_alone` 用一个 dispatch 从没见过的合成 Agent 走完整条同步路径；`agent_ids_are_spelled_out_only_in_the_registry_and_the_writers` 给注册表和 writer 之外的每个文件钉死 Agent id 字面量预算。声明面通过 `list_agent_descriptors` 命令投影为 `AgentDescriptorDto` 供前端消费（`crates/skillstar-app/src/models/agents.rs`）；前端 `agentRegistry.ts` 只保留没有后端对应物的展示项（图标、tagline、安装文档链接）。
- 托管模型配置的所有者是 `skillstar-gateway`。`tool_sync` 对六个受管 Agent 的 sync 不再写入厂商 base URL 或 API key。Codex 的环回地址只在用户保存时由网关写入。文件型 Agent 的环回地址在保存时由 `apply_gateway` 写入。哪些 id 在册，以 gateway 的 `apply_gateway_` 测试为准。Claude Code 的 `settings.json` 在保存时把 `ANTHROPIC_BASE_URL` 指到网关根，不带 `/v1`；`ANTHROPIC_AUTH_TOKEN` 是占位 `skillstar`，不是 Usage 的 access token。进程桥不读这份文件。Claude Desktop 在保存时写入原生 `claude_desktop_config.json` 和 Claude-3p 配置档，不写 `skillstar-binding.json`。OpenHanako 在跑时走它本地 API，没在跑时写 catalog 和 agent 配置；地址是网关的 `/v1`，provider 名是 `skillstar`。Alma 在跑时经 `http://localhost:23001` 写入名为 `skillstar` 的 openai provider，地址是网关的 `/v1`；没在跑时不写文件。Cindy 只生成导入链接，密钥是 `skillstar-cindy`，数据库只读。WSL 里正在运行的 Codex 以 `codex@wsl:<发行版>` 写入，路径是 `\\wsl.localhost\<发行版>`；mirrored 用 `127.0.0.1`，NAT 用发行版看到的 Windows 地址，不启动已停止的发行版。models.dev 的目录缓存在数据根的 `cache/gateway-catalog/models.dev.json`，下载走 `probe_http_client`。失败时沿用已有缓存。不写 provider 的 `model_catalog`，也不写 `cache/model_catalog/`。Providers 栏的一行显示名称和掩码后的凭据摘要，摘要来自后端。这一行没有明文密钥，也没有厂商 base URL。
- 启动和保存 provider 都不改 Agent 文件。已有文件里的厂商 URL 保持不动，直到用户保存 Codex；这次保存只改这一档声明的键。`repair_agent_configs` 不再从启动路径调用。
- unsync 仍删除 SkillStar 已经写过的托管键（Claude 托管 env、Desktop marker、`skillstar` / `skillstar_*` 块，以及指向它们的指针）。那是解除托管，不是把厂商密钥写回去。
- 所有测试把 `SKILLSTAR_TOOL_SYNC_HOME`、`SKILLSTAR_DATA_DIR` 和 `HOME` 指到临时目录。
- Claude CLI 与 Desktop 在 store 内保持独立绑定，共用 Claude Official 种子。Codex CLI、桌面体验和官方编辑器扩展仍共用一份 Codex binding。
- `wire_api = "chat"` 不再被写入。Codex ≥0.95 读到这个值会让整个 `config.toml` 解析失败。留在 `[model_providers.skillstar]` 里的旧值，要等 API 形态的保存把这张表整段换掉；其它表不动。
- Pi 与 OMP 的 sync 不再写 `models.json` / `models.yml`。停用仍只清理托管块，且仅当指针指向托管块时才连带清除。OMP 不读 Pi 的 `~/.pi/agent/*`。

### 角色路由（跨 Agent）

角色路由是**域内一等概念**，不是 OMP 的功能。词表与类型在 `providers::roles`（`RoleDef` / `RoleCapability` / `DroppedRole` / `RoleDropReason` + 五个规范角色常量 `default` / `fast` / `plan` / `vision` / `subagent`），值的形状是共享的 `ModelRef{provider_id, model, effort}`，存储位置是 `AgentBinding.roles`。

**每个 Agent 在注册表里声明自己支持哪些角色**，分三档：

| 档 | Agent | `AgentSpec.roles` |
| --- | --- | --- |
| 无角色 | `pi` / `codex` / `opencode` / `claude-desktop` | 空 slice，UI 只渲染单一 provider+model 选择 |
| 单角色 + 兜底 | `claude-code` | `default` / `fast` / `sonnet` / `opus` / `subagent`（5 条） |
| 多角色 | `omp` | 10 条完整角色面板（主要平铺 + 次要折叠） |

Codex 与 OpenCode 上游各自有一个角色概念（`default_subagent_model`、`small_model`），注册表里故意留空。

**角色留在 binding 上。** `tool_sync` 不把声明的角色写进 Agent 配置。`managed_agents_do_not_write_declared_roles` 赋满角色并断言配置文件字节不变。

- `RoleDef.agent_key` 是该 Agent 配置文件里的键名（OMP 的 `smol`、Claude 的 `ANTHROPIC_DEFAULT_HAIKU_MODEL`、OpenCode 的 `small_model`），writer 与角色面板都读它，不再各自硬编码翻译表。
- `RoleDef.inherits` 是**该 Agent 文档承认的**回落目标，UI 把它渲染成空行的 placeholder（「未配置 — 回落到 default」/「未配置 — 由该 Agent 自行选择」）。回落只在**读时**解析（`providers::roles::resolve_role`），绝不写盘：写时复制会让「显式设成同一个模型」和「继承」在磁盘上无法区分，清空字段也拿不回原值。
- Claude Code 的 sync 仍可在返回值里列出写不进去的角色（`ToolSyncResultFlat.dropped_roles`）。它不因此创建 `settings.json`。前端 `useRoleDrops` 只记住后端裁决。
- **thinking / effort 等级按模型能力裁剪**：`ModelCatalogEntry.reasoning`（`Reasoning::{None, Toggle, Effort, BudgetTokens}`）来自模型目录，`tool_sync::omp_thinking_levels_for` 据此收窄 9 元 grammar；前端 `ompThinkingLevelsFor` 是同一张表的镜像。目录**没有**该模型的数据时返回完整清单——「不知道」不能渲染成「不支持」。

#### Claude Code

角色存在 `AgentBinding.roles`。键名仍是注册表里的 env 名（`ANTHROPIC_MODEL`、`ANTHROPIC_DEFAULT_HAIKU_MODEL` 等），但 sync 不把它们写进 `~/.claude/settings.json`。unsync 按 `claude_managed_env_keys()` 删除已经存在的托管 env 键。指向其它 provider 的角色记入 `dropped_roles`，原因是 `provider_not_bound`。

#### OMP 模型角色

角色存在 binding 级 `AgentBinding.roles`，命令是 `update_agent_settings`。store 用规范键；OMP 自己的名字在 `RoleDef.agent_key`。注册表与 `migrate::omp_role_key` 由 `registry_agent_keys_match_the_migration_table` 锁定。sync 不写 `modelRoles`。unsync 只删除已经指向 `skillstar_*` 的角色，用户自己的 provider 不动。解绑或删除 provider 会清掉指向它的角色分配。SkillStar 不写 OMP 的 `cycleOrder`。

## Native Official（原生登录）

- `claude-official` / `codex-official` 是固定种子 Provider（稳定 store `id` + `preset_id`），不是 UUID 新建行。判定靠这些 id，不靠空 URL 启发式。
- **`PresetCategory` 拆开了 v3 的 `official`**：`native_login`（Claude / Codex 种子，凭据在别人的 CLI 里）与 `vendor_official`（Grok，拿 API Key 访问）是结构上不同的两件事，v3 用一个字符串表示、靠 id 白名单区分。白名单已删除，`is_native_official_preset_id` 现在查注册表。前端 `openai_compatible` 是它自己合成的模板，也在同一个枚举里，所以类型是完备的。
- `ensure_official_providers` 在缺失时插入种子行；已存在同 `id`/`preset_id` 则跳过（不覆盖用户改名）。`get_providers_flat` 会调用它并在变更时写盘。
- `create_provider_from_preset` 对这两个种子保留稳定 id 并写 `Credential::ExternalCli`；`create_provider` 不再改写调用方给的 id（v3 会覆盖成 UUID，这正是固定 slug 需要白名单的原因），重复 id 直接报错。
- 激活时跳过「必须有 anthropic/openai URL」校验。
- 激活官方登录只改 store 里的绑定。它不改 `~/.claude/settings.json`，也不改 Codex 的 `config.toml` 或 `auth.json`。解除托管仍走 unsync。官方登录的激活界面不提供 Desktop。
- Codex Official 的 `bind_provider` 强制 `auth_mode = oauth`。这不写入 `OPENAI_API_KEY`，也不改用户的 ChatGPT token。
- 停用 Official 与普通 unbind 一致（清 binding；Claude 不额外清用户自有配置）。
- Official 是连接方式，不是可创建/编辑的 API 供应商：原生种子从供应商选择与 Recent 中排除。使用原生登录通过 `bind_provider` 选择后端种子，保持它的真实 id 与 ExternalCli 凭据；不能用空 API Key 新建行冒充原生种子。

## 本机网关

官方账号经本机网关转发是当前目标。「本轮不做 proxy takeover」不再描述它。监听地址、谁启动 serve、路由表、Codex 环回写入和 `agent_stash.json` 见 [运行架构](../../architecture.md#本机模型网关)。订阅侧 Claude 的进程桥也写在那里：本机 `claude`，access token 不进子进程。

能签上游的是 Usage 里已经保存的账户，catalog 与请求头以 `skillstar-gateway` 的签名模块及其 `sign_` 测试为准。`anthropic` 不签 HTTP。Gemini CLI、Devin、WorkBuddy、Command Code 没有 Usage 凭证行，出站只用 provider 快照里的 API 密钥，不读厂商自己的认证文件。没有注入的账户快照时不请求配额，该候选保持未知。

## Models 工作台

Models 页左侧是 Agents 和 Providers 两张列表卡，右侧是一块 Gateway 面板；面板内配置列（监听方式 → 配置档 → 路由与亲和 → 分组成员）与最近调用表并排，窄屏时配置列收为单栏。这一屏不决定行密度。某一栏的读取成功而且是空的时候，句子分别是「还没有探测到可配置的 Agent」「还没有密钥」「还没有调用」。读取失败时不显示这些句子。句子不提示厂商 URL，也不给出示例密钥。这一栏有了自己的一行之后，它的句子就不再出现。

`get_models_board` 返回每一行的 id、name 和 `model_label`。`model_label` 是从该 Agent 配置文件读回的当前模型（gateway 的 `written_model_ref`，含 board 拼法到 writer 拼法的归一，如 `claude-code` 行读的是 `claude` 的 `settings.json`），读不到就是空串，页面显示「未选择模型」。Providers 行另外带掩码摘要。Agents 来自注册表，Providers 来自 `load_store()` 的名字。看板里的 gateway 列表保持为空。Gateway 栏另读进程内最近 60 条调用，含时间、Agent、模型、状态和补全 token；没有用法时 token 为空。这一栏不显示配额，也不显示上游 URL。这个读取不走 `get_providers_flat`，不读 `compat.rs`，也不读网关监听地址。缺失的 store 是空列表，不写 Agent 文件。

Gateway 面板的配置列顶部是「接入端点」：OpenAI（`/v1/chat/completions`）和 Anthropic（`/v1/messages`）两行，各带复制按钮，来源是 `get_loopback_origin` 命令。展示时去掉 `http://` 前缀，复制的是完整地址。读取失败时这一区不渲染。

点 Agents 栏的一行打开选择器：搜索框输入即过滤，ArrowUp/Down 移动高亮，Enter 保存，Esc 关闭；底部有「保存显示名」表单。列表是 models.dev 缓存和已保存分组的投影，每一项的 id 是 `provider/model` 或 `group/<id>`，当前项带勾选标记。保存走该 Agent 已经落地的 writer，成功后刷新看板并提示；失败时弹层保留。Codex 的保存走网关的字段级接管（`apply_agent_with_model`），写 `config.toml` 的 `model` 键，不整文件替换；空 ref 等于解除接管。不在这套写入里的 id，包括 Goose、Cursor CLI、Copilot CLI 和 Devin，返回 `agent_not_managed`，不写文件，也不为它们新造 URL 字段。弹出层不显示密钥，也不显示厂商 URL。

Agent 名称旁边的小字是已经写进该 Agent 文件的环回地址，形如 `127.0.0.1:21847`；名称下面一行 chip 是 `model_label`。没有写下环回地址时这一格为空。页面不使用 provider 存储里的端点来填它。

选中的提供商，以及已经保存在 `model_gateway.json` 里的分组，在 Gateway 面板的配置列里改路由和亲和。控件只提交 smart、order、rotate、usage 与 auto、session、turn、off。保存写这份文件。smart 和 auto 可以不落字段，读回来仍是这两项。这次保存不改 `model_providers.json` 的版本和列。监听转发仍打启动时的那一个上游根，不读这次写下的路由和亲和。

已保存的分组可以在 Gateway 面板里增减成员。新建、加入和移除都调用分组写入。成环或超过 8 层时文件不变，界面留下这次返回的原因。还没被保存过的自动分组不出现在这张列表里。

Gateway 面板的配置列列出已经保存的配置档名字。保存写下名字和若干 Agent 的 model ref。点一个名字就按这份档调用已有的写入。超过 64 个字或写不进去时文件不变，界面留下返回的原因。未实现的 Agent 不写文件。

Gateway 面板可以选择环回或局域网。局域网让进程听 `0.0.0.0` 和原来的端口。写给 Agent 的地址仍是 `127.0.0.1`。端口 `3425` 不会因为打开局域网而开始监听。

人可以给目录里的模型一个显示名。选择器展示这个名字，没有时展示 id。出站请求的 `model` 仍是上游 id。空名字、换行、超过 80 个标量、含 `://` 或 `sk-`、或目录里没有这个 id 时不写文件。目录缓存不被改写。

分组成员可以固定一个 effort。可选等级来自目录缓存，不在界面里另写一份。请求里的等级在出站前收成最接近的一项；目录没有这个模型时保持原样。成员上的固定值优先。

每个 Agent 可以有一份可见名单，写在 `model_gateway.json` 的 `visible`。项是家族标签、provider id 或 group id，家族写在 provider 或分组的 `family`。没有名单时看到全部模型。名单收窄这个 Agent 的 `/v1/models`、选择器，以及写进它文件的模型目录。已经保存的 model ref 留在原字段。名单外的 id 仍可以请求。

一个 `provider/model` 可以留下目录等级的子集，写在 `model_gateway.json` 的 `model_efforts`。没有这份子集时，选择器和出站收束仍用目录里的全部等级。有子集时，这两处只提供子集里的等级；未固定的请求按目录顺序收进子集。成员上的固定 effort 仍优先。

Settings 的 App AI 仍用 `get_providers_flat`。切换列表卡、点 Providers 或 Gateway 的行、或点侧栏里的最近名字，只改变当前选中。保存所选模型才写该 Agent 的配置。旧的 Claude 工作台和只被它挂上的编辑抽屉不在这条生产路径上。


## 前端状态与诊断

- 所有 Models IPC 集中在 `src/features/models/api/`；query key 由 `modelsKeys` 工厂生成。
- mutation 采用 optimistic update → error rollback/toast → settled invalidate；create 用返回实体填充 cache。
- activation map 从 provider flat cache 投影，不额外维护第二套 `tool_activations` fetch。
- built-in preset 由 Rust command 返回，TypeScript 不复制 registry。
- 余额响应的解析在 Rust preset。Models 页不保留第二份解析表。
- App AI 可以绑定 Models provider 或本地 Ollama。这个表单在 Settings，不在 Models 页。
- App AI 的完整设置区块（Models provider 选择与本地 Ollama 表单）由 `src/features/models/components/settings/` 提供，`src/pages/Settings.tsx` 只负责组合，避免 Settings feature 反向读取 Models 私有 hooks。

## 应用内 AI

- `AiProviderRef` 的 `app_id` 改名为 `agent_id`，取值来自 Agent 注册表（`claude` → `claude-code`，`codex` 不变）。这是 Models 域的第五套 id 空间，现在并入第四套。旧文件靠 serde `alias = "app_id"` 继续解析，`normalize_agent_id` 负责把旧拼法映射过来。
- Claude 的三档模型读 `claude-code` binding 的角色。sync 不把这些角色抄进 Agent 配置。
- chat、summary 的 provider resolve 与 HTTP 实现在 `skillstar-models::ai_provider`。Skill 图文教程不走 Models provider，而由 Skills 详情页调用用户配置的 ACP Agent。Skill 摘要的输出语言是当前界面语言（与教程同一套 locale），不是 `ai.json` 里的独立字段。
- 前端展示后端报告的 route/provider/fallback，不复制 provider 选择逻辑。
- provider timeout 在 resolve 时应用，不写进旧 `ai.json` 兼容格式。
- 流式 UX 的共享规范见 [../frontend/README.md](../frontend/README.md#tauri-事件与流式-ux)。

## 本地决策模型（AgentJev-0.6B）

这是 Models 域下的**本地推理能力**，不是另一个 App AI provider：App AI 生成文本（chat/summarize），决策模型不生成任何 token，只把一段状态和若干结构化问题映射成每个选项的校准概率。实现全部在 `skillstar-decision`；命令层只做 DTO/State/事件，域逻辑不回流到 `src-tauri`。

### checkpoint 契约

- 权重**不进仓库**。四个文件（`model.safetensors` 1.2 GB、`tokenizer.json`、`config.json`、`temperatures.json`）从 Hugging Face 的 pinned revision `b3bf6b6d…` 下载到 `<data_root>/models/agentjev-0.6b/`，`SKILLSTAR_DECISION_MODEL_DIR` 可整体改目录，`SKILLSTAR_HF_ENDPOINT` / `HF_ENDPOINT` 可改下载源（镜像如 `https://hf-mirror.com`）。
- 每个文件带固定字节数与 SHA-256。中断的传输写 `<file>.part` 并用 Range 续传；下载完成后逐个核对摘要，失败删除临时文件。**加载时只核对大小**（重新哈希 1.2 GB 不该出现在启动路径上），完整校验由下载流程与显式「校验完整性」命令/`--verify` 负责。
- 缺文件、长度不符、摘要不符一律 fail-closed：不加载、不用半份权重回答。

### 引擎生命周期

- 引擎在第一次提问时懒加载，之后常驻一个进程一份（`DecisionState` 持有 `Arc<DecisionEngine>`）；加载与推理都跑在阻塞线程上，前台命令不会把 async 运行时的工作线程占住。`decision_unload_engine` / 面板「释放内存」显式归还。
- 默认设备是 macOS 上的 Metal、其余平台 CPU；默认精度 f32。**f32 是刻意选择**：candle 0.11 的 Metal 后端缺 `softmax-last-dim`、`rotary-emb` 等核，f16 会逐算子回退到 CPU——`ops.rs` 用可移植算子实现了等价数值，`--dtype f16` 仍可能撞到缺失核，因此不作为默认。
- 共享前缀复用是自实现的：一个问题的 `[STATE][QUESTION]` 前缀只前向一次，每个候选作为分支读到前缀的 KV；兄弟候选之间互不可见，前缀张量只读。

### 请求/答案契约（`agentjev.decision.v1`）

- 三种原语：`boolean`（返回 true 的概率）、`choice`（2–255 个候选，返回选中项、top 概率与 margin）、`score`（2–10 个有序等级，返回 argmax 等级与期望分 `Σ i·Pᵢ`）。一次请求最多 32 个 state、128 个问题、1024 条候选路径；单条 `[STATE][QUESTION][CANDIDATE]` 路径最长 2048 token，**超出直接报错，绝不截断**。
- 校验消息与官方 `jev_service` 的 `contract.py` 逐字一致（`prepare`/`answer` 是它的移植），因为 CLI 和界面都把这句原文展示给用户。
- 概率是模型分布：`temperature` 来自 checkpoint 的 `temperatures.json`（按原语一个正标量）。README 与文档都不得把它描述成「动作成功率」——要那个语义得对每个动作问一个 boolean 并在自己的结果上校准。
- 结构化 state/候选支持对象或数组，落成紧凑 JSON；对象形态的 key 在本实现里按字典序处理（serde_json map 语义），头的置换等变性保证答案按 key 不变，但**不要**依赖返回数组的下标顺序。

### 三个入口

- CLI：`skillstar decide --file payload.json`（`-` 读 stdin），`--json` 输出结构化结果，`--status` / `--verify` / `--download` 管理 checkpoint，`--device` / `--dtype` 覆盖运行时。
- 设置页：`src/features/models/components/settings/DecisionModelSection.tsx`，经 `Settings` 的 `settings-decision` 区块渲染。显示目录/大小/下载源、下载进度（后端事件 `decision://download-progress`）、校验/加载/释放，以及一个试跑区（state + 问题 + 选项，输出每个候选的概率条与用量）。面板不伪造进度或成功：下载中的数字全部来自事件流。
- 命令：`decision_model_status` / `decision_verify_model` / `decision_download_model` / `decision_cancel_download` / `decision_engine_info` / `decision_load_engine` / `decision_unload_engine` / `decision_evaluate`；DTO 由 ts-rs 生成到 `src/types/generated/Decision*.ts`，前端 `src/types/decision.ts` 只做再导出。

### 测试

- `cargo test -p skillstar-decision`（不需要权重）：契约校验/答案整形、温度范围、目录状态机、`Send + Sync` 断言。
- golden 测试（默认 `#[ignore]`，需要 1.2 GB checkpoint）：`SKILLSTAR_DECISION_MODEL_DIR=<dir> cargo test -p skillstar-decision --profile release-fast --test golden -- --ignored --nocapture`。fixture 由官方 `jev_service`（torch CPU f32）在六个 payload 上产出，Rust 侧必须复现 token id、概率与赢家；默认设备一条单独断言 Metal/CPU 与参考一致。

### 已知边界

- 目前只做能力层，没有接进任何业务流（安装闸门、App AI 路由都未接线）；阈值与后果由未来的调用方决定，域 crate 不替业务做决定。
- 宽候选集上每个候选都要拼接一次前缀 KV，长状态 + 255 选项仍有可做的性能优化。
- 对象形态选项的字典序处理、以及 f16 在 Metal 上不可用，都是本实现的已知取舍，写在上面而不是事后解释。

## 类型生成

Models 的跨 IPC 大结构使用 ts-rs。修改 Rust 类型后运行 `bun run types:gen`，禁止手改 `src/types/generated/`。是否把小型手写 mirror 转为生成类型，以实际维护收益和既有门槛为准，不在本文复制字段清单。

## 模型 catalog 缓存

- Provider 自己 `/v1/models` 返回的目录不再存在 store 里（v3 的 `meta.model_catalog` 会把几百个模型的原始 JSON 反复重写进放着凭据的文件）。现在一 provider 一个文件，放 `<data_root>/cache/model_catalog/<provider_id>.json`，模块是 `providers::catalog_cache`。
- 读失败一律返回空表而不是报错：没有模型元数据的配置文件是降级，写不出配置文件是坏掉。
- 迁移仍把抽出的 catalog 落到缓存。它不再把 catalog 投影进 `opencode.json`。
- 三级来源策略（内置快照 / models.dev / provider 自身）属于后续工作包，本模块只拥有 provider 自身这一级。

## 验证

```bash
cargo test -p skillstar-core -p skillstar-models -p skillstar-decision
bun run test -- src/features/models
bun run types:gen
```

决策模型的数值改动必须跑上面那条 golden 命令（需要 checkpoint）；契约、温度与目录状态的改动只需 `cargo test -p skillstar-decision`。

Codex 环回写入跑 `cargo test -p skillstar-gateway codex_writer_`。启动和 provider 保存不改 Agent 文件，跑 `cargo test -p skillstar-models --test startup_agent_files`。

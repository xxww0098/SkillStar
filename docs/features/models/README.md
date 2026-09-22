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
- **Desktop 能力以原生写盘效果判断**，不是注册表是否有行：当前 writer 仍可能写 SkillStar marker；工作台的只读边界见 [Models 工作台](#models-工作台)。迁移报告不能把 marker 描述为已生效的原生配置，也不得擅自将 Desktop 绑定搬到 CLI。
- **磁盘格式已切换**：`model_providers.json` 现在以 v4 写盘（`version: 4`，`providers` + `bindings`）。启动入口是 `load_store_and_repair`：读→（必要时）迁移→落盘 catalog 缓存→修复已写坏的 Agent 配置→再次写回 store。`get_providers_flat` 是唯一调用它的命令，其余命令走不做修复的 `load_store`。
- **v3 reader 拒绝未来版本**：`migrate_store_if_needed` 的 v1 分支是「排除法」到达的，而 v1 结构每个字段都有 serde 默认值——所以一个 v4 文件会被**成功**解析成四个空桶，然后覆盖用户的真实配置。现在遇到高于 `FLAT_STORE_VERSION` 的版本直接报错并保留原文件。
- **IPC 线上形状仍是 v3**：store 与所有 writer 都是 v4，但渲染进程读到的仍是 v3 形状，翻译集中在 `src-tauri/src/commands/models_commands/compat.rs` 一处。写入是**打补丁**而非重建：v3 表达不了的 `caps` / `headers` / key 故障转移链 / `ext` 因此不会在每次保存时被清空。前端 IA 重写时删除该模块，明文 key 也随之停止过界。

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
- tool-sync 只改自己管理的字段，保留用户已有配置；写入前备份并使用原子替换。
- JSON 型 multi Agent（OpenCode / Pi）的写盘共享 `multi_provider` 内部骨架（备份 → retain 托管键 → 写块 → active 指针），各自只提供 build_block 与指针落点；Codex（TOML + auth.json 副通道）独立维护，理由见 decisions.md D-012。
- 所有测试设置 `SKILLSTAR_TOOL_SYNC_HOME` 到临时目录，绝不写真实 Agent 配置。
- Claude CLI 与 Desktop 在 store 内保持独立绑定，共用 Claude Official 种子。CLI writer 写原生 settings；Desktop writer 的 marker 不代表原生配置支持，工作台写入保护见 [Models 工作台](#models-工作台)。Codex CLI、桌面体验和官方编辑器扩展仍共用一份 Codex binding。
- **Codex 只写 `wire_api = "responses"`**：`CodexSettings` 不再有 `wire_api` 字段，provider 行也不再有 `codex_wire_api`。没有 `/v1/responses` 端点的 host **整条跳过**，不写入 `config.toml`；`base_url` 取 responses 端点而非 chat 端点。根因：Codex ≥0.95 的 `WireApi` 枚举只剩 `Responses`，而 SkillStar 对任何非 `api.openai.com` 的 base URL 都写 `"chat"`；该值反序列化失败会让**整个 `config.toml` 解析不了**，Codex 起不来——不是单个 provider 失效。自检：`grep 'wire_api' ~/.codex/config.toml`，出现 `"chat"` 即为受影响。（errors.md 的正式条目归后续工作包。）
- **迁移会修复已写坏的磁盘配置**：`tool_sync::repair_agent_configs` 在迁移那一次运行时删掉不可写的 Codex 条目（store 与 `config.toml` 两侧）、清掉 Claude Desktop 标记文件，再重投影其余绑定。被删的条目带 provider 名与模型进 `MigrationReport::codex_dropped`，UI 必须解释而不是静默。单条清理用 `unsync_codex_entry`，不是整体 unsync——修一条坏绑定不该丢掉两条能用的。
- Pi 是 multi Agent：绑定写 `~/.pi/agent/models.json` 的 `providers.skillstar_*` 块（`openai-completions`，模型条目只写 `id`，其余交给 Pi 默认值），激活条目同时把 `~/.pi/agent/settings.json` 的 `defaultProvider`/`defaultModel` 指过去；停用只清理托管块，且仅当 default 指针指向托管块时才连带清除。
- OMP（Oh My Pi）是独立产品（配置根 `~/.omp`），不读 Pi 的 `~/.pi/agent/*`。绑定写 `~/.omp/agent/models.yml` 的 `providers.skillstar_*` 块（YAML，schema 与 Pi 同构：`baseUrl` / `api: "openai-completions"` / `apiKey` / 最小 `{ id }` 模型条目）。tool-sync 对 `omp` 使用 YAML 文件规格（`format: "yaml"`，编辑器校验/格式化走 serde_yaml，保留 key 顺序）。

### 角色路由（跨 Agent）

角色路由是**域内一等概念**，不是 OMP 的功能。词表与类型在 `providers::roles`（`RoleDef` / `RoleCapability` / `DroppedRole` / `RoleDropReason` + 五个规范角色常量 `default` / `fast` / `plan` / `vision` / `subagent`），值的形状是共享的 `ModelRef{provider_id, model, effort}`，存储位置是 `AgentBinding.roles`。

**每个 Agent 在注册表里声明自己支持哪些角色**，分三档：

| 档 | Agent | `AgentSpec.roles` |
| --- | --- | --- |
| 无角色 | `pi` / `codex` / `opencode` / `claude-desktop` | 空 slice，UI 只渲染单一 provider+model 选择 |
| 单角色 + 兜底 | `claude-code` | `default` / `fast` / `sonnet` / `opus` / `subagent`（5 条） |
| 多角色 | `omp` | 10 条完整角色面板（主要平铺 + 次要折叠） |

Codex 与 OpenCode 上游各自有一个角色概念（`default_subagent_model`、`small_model`），这里**故意留空**：它们的 writer 目前只投影 active 指针。

**红线：注册表声明的角色，writer 必须写。** 声明但不写等于 UI 提供一个无效设置，正是本轮要修的缺陷。`every_declared_role_reaches_disk` 逐 Agent 赋满全部声明角色、跑真实 writer、断言每个 `agent_key` 出现在写出的字节里；加角色忘了改 writer 会直接红。

- `RoleDef.agent_key` 是该 Agent 配置文件里的键名（OMP 的 `smol`、Claude 的 `ANTHROPIC_DEFAULT_HAIKU_MODEL`、OpenCode 的 `small_model`），writer 与角色面板都读它，不再各自硬编码翻译表。
- `RoleDef.inherits` 是**该 Agent 文档承认的**回落目标，UI 把它渲染成空行的 placeholder（「未配置 — 回落到 default」/「未配置 — 由该 Agent 自行选择」）。回落只在**读时**解析（`providers::roles::resolve_role`），绝不写盘：写时复制会让「显式设成同一个模型」和「继承」在磁盘上无法区分，清空字段也拿不回原值。
- **写盘时被跳过的角色必须回报**：`ToolSyncResultFlat.dropped_roles` 带 `{role, reason, provider_id}`，reason 是 `provider_not_bound` / `provider_has_no_endpoint` / `provider_missing` / `no_model` / `role_not_supported` / `invalid_role_name`。同步成功不代表配置完整，差集只有 writer 算得出来，所以它是返回值的一部分。前端 `useRoleDrops` 只记住后端裁决，不重算规则。
- **thinking / effort 等级按模型能力裁剪**：`ModelCatalogEntry.reasoning`（`Reasoning::{None, Toggle, Effort, BudgetTokens}`）来自模型目录，`tool_sync::omp_thinking_levels_for` 据此收窄 9 元 grammar；前端 `ompThinkingLevelsFor` 是同一张表的镜像。目录**没有**该模型的数据时返回完整清单——「不知道」不能渲染成「不支持」。

#### Claude Code

`AgentBinding.roles` 直接投影到 `~/.claude/settings.json` 的 env 块，键名取自注册表：`default → ANTHROPIC_MODEL`（未分配时由 active 条目的模型兜底）、`fast → ANTHROPIC_DEFAULT_HAIKU_MODEL`、`sonnet` / `opus → ANTHROPIC_DEFAULT_{SONNET,OPUS}_MODEL`、`subagent → CLAUDE_CODE_SUBAGENT_MODEL`。托管 env key 清单由注册表派生（`claude_managed_env_keys()`），所以 unsync 不会漏清新增角色。Claude 的 env 块只有一个 base URL，因此指向**其它 provider** 的角色不写并回报 `provider_not_bound`。前端映射面板经 `update_agent_settings` 落盘——v3 它只有 `useState`，用户填的东西从未抵达后端。

#### OMP 模型角色

OMP 按任务意图把请求路由到不同模型，角色写在 `~/.omp/agent/config.yml` 的 `modelRoles`。规则如下（角色与 thinking 等级清单的 SSOT 是 `tool_sync::agents` 的 `OMP_ROLES` 与 `tool_sync::types` 的 `OMP_THINKING_LEVELS`；后端一致性测试锁定声明与实际投影）：

- 角色分配存在 **binding 级一等字段** `AgentBinding.roles`（`BTreeMap<String, ModelRef>`），不是 entry 级 `BindingEntry.settings`——一个角色可以指向任意已绑定 provider，不必是 active 的那个。写入命令是 `update_agent_settings`。
- **角色名要在落盘时翻译回 OMP 的词汇**：store 存规范键（`smol` 迁移后叫 `fast`，`task` 叫 `subagent`），OMP 不认识这两个名字。writer 查注册表的 `RoleDef.agent_key` 译回去；注册表未声明的自定义角色原样透传（`modelRoles` 是开放 map）。注册表与迁移的 `migrate::omp_role_key` 由 `registry_agent_keys_match_the_migration_table` 锁定，任何角色调用方也必须按规范键读写，否则迁移过的用户会看到角色「消失」同时旁边多出一条重复角色。写入顺序按 OMP 的角色名排序，避免内部改名让 YAML 无故重排。
- 每个已分配角色写成 `modelRoles.<role> = "skillstar_<id8>/<model>[:thinking]"`。未分配的角色**不写**：OMP 自己会让 `smol`/`slow`/`designer` 回落到 `default`，留空是安全的。
- `default` 未显式分配时由 binding 的 active 条目兜底，等同于角色功能引入前的行为。
- 角色指向未绑定、或没有 OpenAI base URL（因此没进 models.yml）、或没选模型的 provider 时跳过，不写悬空指针。角色名含 `/`、空白或以 `@` 开头（与 OMP 的 `@role` 别名语法冲突）同样跳过。**每一种跳过都进 `dropped_roles`**，角色行标黄给出原因——v3 这里是三个裸 `continue`，前端无从知道。
- 每次同步先删除所有指向 `skillstar_*` 的角色再写入当前集合，与 models.yml 托管块的 retain 策略一致：用户在 UI 取消分配后磁盘上不留残留。指向用户自有 provider 的角色永不触碰。
- 解绑 provider（`unbind_provider`）或删除 provider（`delete_provider_flat`）会连带清除指向它的角色分配；`unbind_agent` 清空整条绑定连同角色。
- Ctrl+P 只在 OMP 的 `cycleOrder`（默认 `["smol","default","slow"]`）之间循环，配置了 `plan` 等角色也不会进循环——这是 OMP 侧行为，SkillStar 不代写 `cycleOrder`。

## Native Official（原生登录）

- `claude-official` / `codex-official` 是固定种子 Provider（稳定 store `id` + `preset_id`），不是 UUID 新建行。判定靠这些 id，不靠空 URL 启发式。
- **`PresetCategory` 拆开了 v3 的 `official`**：`native_login`（Claude / Codex 种子，凭据在别人的 CLI 里）与 `vendor_official`（Grok，拿 API Key 访问）是结构上不同的两件事，v3 用一个字符串表示、靠 id 白名单区分。白名单已删除，`is_native_official_preset_id` 现在查注册表。前端 `openai_compatible` 是它自己合成的模板，也在同一个枚举里，所以类型是完备的。
- `ensure_official_providers` 在缺失时插入种子行；已存在同 `id`/`preset_id` 则跳过（不覆盖用户改名）。`get_providers_flat` 会调用它并在变更时写盘。
- `create_provider_from_preset` 对这两个种子保留稳定 id 并写 `Credential::ExternalCli`；`create_provider` 不再改写调用方给的 id（v3 会覆盖成 UUID，这正是固定 slug 需要白名单的原因），重复 id 直接报错。
- 激活时跳过「必须有 anthropic/openai URL」校验。
- Claude Official 激活 CLI 时清除 SkillStar 托管的连接与角色 env 键，让 Claude 使用原生登录；不写第三方 Base URL/Key，不启动登录、不验证账号，也不清除用户自有的其它鉴权设置。Desktop 的 marker 不具备同等语义，界面不提供其激活操作。
- Codex Official 绑定 `codex`：`bind_provider` 强制 `auth_mode = oauth`，不写 `OPENAI_API_KEY`、不触碰用户 ChatGPT token；清除指向 SkillStar 托管表的 `model_provider`/`model` 指针。
- 停用 Official 与普通 unbind 一致（清 binding；Claude 不额外清用户自有配置）。
- Official 是连接方式，不是可创建/编辑的 API 供应商：原生种子从供应商选择与 Recent 中排除。使用原生登录通过 `bind_provider` 选择后端种子，保持它的真实 id 与 ExternalCli 凭据；不能用空 API Key 新建行冒充原生种子。
- 本轮不做 proxy takeover /「官方账号路由」例外。

## Models 工作台

- `pages/Models.tsx` 只组合一个 `ModelsHub`。生产界面是 **Claude Code 专用工作台**，不再保留 Provider × Agent 矩阵、列显隐 carousel 或通用 Agent 设置原型。客户端标签与顺序以 [`claudeClients.ts`](../../../src/features/models/lib/claudeClients.ts) 及测试为准。
- 默认打开 CLI；客户端切换只改变查看对象，不触发绑定或修改用户文件。CLI 用「官方登录 / API 连接」选择连接方式，选中供应商只预览，必须显式点击应用才绑定。原生登录不代表 SkillStar 已验证订阅或登录状态。
- **Desktop 是只读说明入口**：当前后端仍注册 Desktop，并可能对自造 marker 写入返回 success，但这不等于原生 Desktop 配置生效。界面显式禁止 Desktop 的绑定、官方切换和模型映射写入，标注原生配置尚未接入；既有 Desktop 状态不显示为已同步，也不自动迁移到 CLI。
- CLI 的 API 连接优先展示 Anthropic 端点和凭据状态；没有 Anthropic 端点的旧供应商仍可查看、编辑，但不能应用到 CLI。所有 Provider、其他 Agent 的绑定、后端 writer 和诊断能力保留，界面重构不做数据清理。
- 主区域提供连接来源、当前绑定与模型映射。切换提交期间禁用重复操作；失败明确展示，不能把本地选择或乐观更新当作写盘成功。已保存绑定仍可重新应用，因为上一次落盘可能失败。读取失败提供重试，不伪装成空列表。
- 角色映射以内联面板呈现；有未确认草稿时锁定客户端/连接切换，保存确认或明确放弃草稿后解锁，避免标签的 mousedown 卸载先于输入 blur 保存。角色与 env key 从 `list_agent_descriptors` 获取；加载或空角色描述不伪造可写角色。「统一使用一个模型」仅作用于已声明角色。修改经 `update_agent_settings` 持久化，成功提示必须等待实际成功结果；跳过原因继续来自后端。
- 供应商创建是 Claude 专用表单：名称、Anthropic Base URL、API Key 为主，默认模型与模型目录地址可选；密钥默认隐藏，URL 在提交前验证。保存只创建连接，不自动绑定；随后打开已有编辑抽屉。
- Provider 编辑继续复用 tabbed drawer（autosave 600ms debounce、validation-aware re-arm、close 前 best-effort flush）。Anthropic 端点常显且优先，OpenAI 兼容端点与目录地址放入可展开的附加设置，旧字段与元数据继续以 patch 保留。
- 侧栏添加与 Recent 快捷入口继续打开创建/编辑；Official 种子不进 Recent。删除仍须确认并列出全部受影响 Agent，包括当前工作台不展示的既有绑定。
- 生产组件位于 `components/hub/claude/`，数据和导航桥接由 `hooks/useModelsData.ts` 与 `lib/navBridge.ts` 提供。旧矩阵与 OMP 专属前端面板删除；OMP 的 store 与后端写盘契约不变。App AI 的完整设置保留在 Settings，不再保留工作台内无效的原型页。
- `ProviderConfigPrimitives.tsx` 是 Models 表单视觉 SSOT：标准控件 40px、dense 控件 36px，并统一 border、focus、disabled 和 invalid 状态。


## 前端状态与诊断

- 所有 Models IPC 集中在 `src/features/models/api/`；query key 由 `modelsKeys` 工厂生成。
- mutation 采用 optimistic update → error rollback/toast → settled invalidate；create 用返回实体填充 cache。
- activation map 从 provider flat cache 投影，不额外维护第二套 `tool_activations` fetch。
- built-in preset 由 Rust command 返回，TypeScript 不复制 registry。
- probe 规则由共享 helper 决定，不在每个 panel 分叉；Models 余额响应解析表位于 `api/balance.ts`，Rust preset 和前端 parser/fixture 由双侧测试锁定。
- App AI 可以绑定 Models provider 或本地 Ollama；Models hub 只负责前者，Ollama 配置仍由 Settings 管理。
- App AI 的完整设置区块（Models provider 选择与本地 Ollama 表单）由 `src/features/models/components/settings/` 提供，`src/pages/Settings.tsx` 只负责组合，避免 Settings feature 反向读取 Models 私有 hooks。

## 应用内 AI

- `AiProviderRef` 的 `app_id` 改名为 `agent_id`，取值来自 Agent 注册表（`claude` → `claude-code`，`codex` 不变）。这是 Models 域的第五套 id 空间，现在并入第四套。旧文件靠 serde `alias = "app_id"` 继续解析，`normalize_agent_id` 负责把旧拼法映射过来。
- Claude 的三档模型同样读 `claude-code` binding 的角色，与写盘 writer 同源，两者不会漂移。
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

Models/MCP 的跨 IPC 大结构使用 ts-rs。修改 Rust 类型后运行 `bun run types:gen`，禁止手改 `src/types/generated/`。是否把小型手写 mirror 转为生成类型，以实际维护收益和既有门槛为准，不在本文复制字段清单。

## 模型 catalog 缓存

- Provider 自己 `/v1/models` 返回的目录不再存在 store 里（v3 的 `meta.model_catalog` 会把几百个模型的原始 JSON 反复重写进放着凭据的文件）。现在一 provider 一个文件，放 `<data_root>/cache/model_catalog/<provider_id>.json`，模块是 `providers::catalog_cache`。
- 读失败一律返回空表而不是报错：没有模型元数据的配置文件是降级，写不出配置文件是坏掉。
- OpenCode 的 provider 块靠它填 `name` / `limit` / `cost`，所以迁移必须先落盘 catalog 再重投影配置，否则会写出一份正确但没有模型元数据的 `opencode.json`。
- 三级来源策略（内置快照 / models.dev / provider 自身）属于后续工作包，本模块只拥有 provider 自身这一级。

## 验证

```bash
cargo test -p skillstar-core -p skillstar-models -p skillstar-decision
bun run test -- src/features/models
bun run types:gen
```

决策模型的数值改动必须跑上面那条 golden 命令（需要 checkpoint）；契约、温度与目录状态的改动只需 `cargo test -p skillstar-decision`。

写盘行为改动必须跑 `tool_sync::tests::golden` —— 它的 fixture 是在 v3 代码上**实际跑出来**的输出，不是手写的期望值。Codex 之外的任何字节差异都是回归。

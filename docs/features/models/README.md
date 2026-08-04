# Models 与 AI

状态：active

本文件维护 Provider store、Agent tool sync、Models 工作台和应用内 AI 的当前契约。

## Provider 分层

- `skillstar-providers` 是零依赖 metadata leaf，拥有 Provider identity、鉴权方案和余额 endpoint。
- `skillstar-models::providers` 拥有 flat provider store、preset、tool binding 和 runtime resolve。
- `AiProviderRef` 从 crate root 导出，provider-ref 实现模块保持私有，调用方不依赖内部文件布局。无调用者的旧 Models circuit-breaker 不作为占位模块保留。
- Usage catalog 与 Models preset 可以不同，但都必须通过 guard test 映射到同一 Provider identity。
- 添加 Provider 从 `crates/skillstar-providers/src/identity.rs` 开始，再补 Models preset、余额解析 fixture 和映射测试；不得在 command/frontend 手写鉴权头。
- v1 per-app store 只是历史迁移来源：命令面与同步/CRUD 已删除，v1 类型和读取收缩为 crate 内部，仅供 v1→v2 migration 和 `ai_provider` 的 legacy provider-ref fallback 使用。新功能只能进入 flat v2 registry 和 API。

## Tool binding 与写盘

- Agent binding 使用 `ToolBinding { entries, active_index }`；所有读写通过 helper/facade，不直接索引 `entries[active_index]`。
- Agent descriptor 的 `kind` 是 UI 和后端能力的共同开关：single 只激活一个 provider；multi 原生保留多个条目并维护 active 指针。
- Rust 侧 Agent 事实（binary、配置目录探测、文件清单、kind、必需 URL、sync/unsync/探测 dispatch）的 SSOT 是 `tool_sync::agents` 注册表；写盘、卸载、resync 与配置目标枚举都经它路由，新增 Agent 只加一行 spec 及其 writer。它与前端 `agentRegistry.ts` 各自持有同一份 toolId 清单，由两侧的表驱动一致性测试互相锁定。
- tool-sync 只改自己管理的字段，保留用户已有配置；写入前备份并使用原子替换。
- JSON 型 multi Agent（OpenCode / Pi）的写盘共享 `multi_provider` 内部骨架（备份 → retain 托管键 → 写块 → active 指针），各自只提供 build_block 与指针落点；Codex（TOML + auth.json 副通道）独立维护，理由见 decisions.md D-012。
- 所有测试设置 `SKILLSTAR_TOOL_SYNC_HOME` 到临时目录，绝不写真实 Agent 配置。
- Claude CLI（`claude-code`）与 Claude Desktop（`claude-desktop`）是**独立绑定**：各自有 `tool_activations` 条目、Official 开关和角色映射状态；共用同一条 Claude Official 种子 Provider（不拆 `claude-desktop-official`）。CLI 写 `~/.claude/settings.json`；Desktop 目前写 SkillStar 绑定标记 `~/.claude-desktop/skillstar-binding.json`（原生 Desktop 配置投影后续接入）。Codex CLI、桌面体验和官方编辑器扩展仍共用一份 Codex binding。
- Codex third-party key 只有用户明确点击时才写 `~/.zshrc`；autosave 不得产生该副作用。
- Pi 是 multi Agent：绑定写 `~/.pi/agent/models.json` 的 `providers.skillstar_*` 块（`openai-completions`，模型条目只写 `id`，其余交给 Pi 默认值），激活条目同时把 `~/.pi/agent/settings.json` 的 `defaultProvider`/`defaultModel` 指过去；停用只清理托管块，且仅当 default 指针指向托管块时才连带清除。

## Native Official（原生登录）

- `claude-official` / `codex-official` 是固定种子 Provider（稳定 store `id` + `preset_id`），不是 UUID 新建行。判定靠这些 id，不靠空 URL 启发式。
- 与 Grok 等 `category: "official"`（带 API Key 的官方大厂）不同：Native Official **无 API Key、无余额探测、空双端点**；文案上对应「原生登录 / Official」。
- `ensure_official_providers` 在缺失时插入种子行；已存在同 `id`/`preset_id` 则跳过（不覆盖用户改名）。`get_providers_flat` 会调用它并在变更时写盘。
- `create_from_preset_flat` / `create_provider_flat` 对这两个种子保留稳定 id（不发 UUID）；允许空 Key。
- 激活时跳过「必须有 anthropic/openai URL」校验。
- Claude Official 种子可分别绑定到 `claude-code` 或 `claude-desktop`：激活 CLI 时清除 SkillStar 托管 env（`ANTHROPIC_*`），让 Claude 走浏览器/客户端原生登录；不写第三方 Base URL/Key。两条绑定共用同一 Official 种子 id（不拆 `claude-desktop-official`），但开关互不影响。
- Codex Official 绑定 `codex`：`activate_tool` 强制 `auth_mode = oauth`，不写 `OPENAI_API_KEY`、不触碰用户 ChatGPT token；清除指向 SkillStar 托管表的 `model_provider`/`model` 指针。
- 停用 Official 与普通 unbind 一致（清 binding；Claude 不额外清用户自有配置）。
- Official **不是**矩阵交叉引用行：前端 `matrixProviders` 过滤种子行；Claude CLI、Claude Desktop、Codex 列表头各自提供「切回官方」开关（分别走 `claude-code` / `claude-desktop` / `codex` binding）。仅当 store 缺失时客户端注入同 id 种子作 activate fallback。开关走生产用的 `activate_tool` / `deactivate_tool`。PresetPicker 不展示这两个原生 Official 预设。
- 本轮不做 proxy takeover /「官方账号路由」例外。

## Models 工作台

- `pages/Models.tsx` 只组合一个 `ModelsHub`，不恢复旧的多子页信息架构。
- 生产主界面是 **Provider × Agent 矩阵**（原 D1 IA）：行是第三方 Provider，列是 Agent（Claude CLI / Desktop 分列且**独立绑定**）；顶部 icon carousel 控制可见列。
- Claude / Codex 列表头提供 Official（原生登录）开关；矩阵单元格负责第三方 Bind / 模型选择 / Claude mapping。
- 侧栏「添加 Provider」与 Recent 只服务第三方 Provider；Official 种子不进 Recent。
- Provider 编辑使用既有 tabbed drawer（autosave 600ms debounce、validation-aware re-arm、close 前 best-effort flush）。创建是主栏表单，创建后打开 editor drawer。
- Claude mapping UI 仍是前端本地状态（Agent 加法）；解绑/绑定走真实 `activate_tool` / `deactivate_tool`。「一键设置」把当前/默认模型广播到全部角色；「获取模型列表」走 `fetch_provider_model_catalog` 并写回该 Provider 的 `models` / `meta.model_catalog`。
- DEV 仅保留 `?variant=D2|D3` 交替 IA 原型；默认 `#models` 即生产矩阵。
- `ProviderConfigPrimitives.tsx` 是 Models 表单视觉 SSOT：标准控件 40px、dense 控件 36px，并统一 border、focus、disabled 和 invalid 状态。
- 删除必须确认并展示会断开的 Agent。

## 前端状态与诊断

- 所有 Models IPC 集中在 `src/features/models/api/`；query key 由 `modelsKeys` 工厂生成。
- mutation 采用 optimistic update → error rollback/toast → settled invalidate；create 用返回实体填充 cache。
- activation map 从 provider flat cache 投影，不额外维护第二套 `tool_activations` fetch。
- built-in preset 由 Rust command 返回，TypeScript 不复制 registry。
- probe 规则由共享 helper 决定，不在每个 panel 分叉；Models 余额响应解析表位于 `api/balance.ts`，Rust preset 和前端 parser/fixture 由双侧测试锁定。
- App AI 可以绑定 Models provider 或本地 Ollama；Models hub 只负责前者，Ollama 配置仍由 Settings 管理。
- App AI 的完整设置区块（Models provider 选择与本地 Ollama 表单）由 `src/features/models/components/settings/` 提供，`src/pages/Settings.tsx` 只负责组合，避免 Settings feature 反向读取 Models 私有 hooks。

## 应用内 AI

- chat、summary、skill pick 的 provider resolve 与 HTTP 实现在 `skillstar-models::ai_provider`。Skill 图文教程不走 Models provider，而由 Skills 详情页调用用户配置的 ACP Agent。
- 前端展示后端报告的 route/provider/fallback，不复制 provider 选择逻辑。
- provider timeout 在 resolve 时应用，不写进旧 `ai.json` 兼容格式。
- 流式 UX 的共享规范见 [../frontend/README.md](../frontend/README.md#tauri-事件与流式-ux)。

## 类型生成

Models/MCP 的跨 IPC 大结构使用 ts-rs。修改 Rust 类型后运行 `bun run types:gen`，禁止手改 `src/types/generated/`。是否把小型手写 mirror 转为生成类型，以实际维护收益和既有门槛为准，不在本文复制字段清单。

## 验证

```bash
cargo test -p skillstar-providers -p skillstar-models
bun run test -- src/features/models
bun run types:gen
```

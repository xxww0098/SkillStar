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
- tool-sync 只改自己管理的字段，保留用户已有配置；写入前备份并使用原子替换。
- 所有测试设置 `SKILLSTAR_TOOL_SYNC_HOME` 到临时目录，绝不写真实 Agent 配置。
- Claude Code CLI 与 Desktop Code 共用 `claude-code`；Codex CLI、桌面体验和官方编辑器扩展共用一份 Codex binding。
- Codex third-party key 只有用户明确点击时才写 `~/.zshrc`；autosave 不得产生该副作用。
- Pi 是 multi Agent：绑定写 `~/.pi/agent/models.json` 的 `providers.skillstar_*` 块（`openai-completions`，模型条目只写 `id`，其余交给 Pi 默认值），激活条目同时把 `~/.pi/agent/settings.json` 的 `defaultProvider`/`defaultModel` 指过去；停用只清理托管块，且仅当 default 指针指向托管块时才连带清除。

## Models 工作台

- `pages/Models.tsx` 只组合一个 `ModelsHub`，不恢复旧的多子页信息架构。
- Agent cards 是激活/停用/重同步的唯一日常入口；provider drawer 只编辑 provider 数据。
- single Agent 使用 hero card；multi Agent 展示全部绑定、active 单选、模型选择和增删。
- Agent settings dialog 处理当前 binding 的深配置、配置文件和同步。provider 切换/关闭前 flush 草稿；未保存的原始配置禁止被重载、同步或重绑覆盖。
- Provider editor 使用 tabbed drawer。autosave 600ms debounce、validation-aware re-arm、close 前 best-effort flush；X、Esc、scrim 和“完成”不能因校验/网络错误把用户困在抽屉里。
- `ProviderConfigPrimitives.tsx` 是 Models 表单视觉 SSOT：标准控件 40px、dense 控件 36px，并统一 border、focus、disabled 和 invalid 状态。
- Provider/Model picker 和 editable model combobox 复用 feature-local shared primitives，不在各卡片、drawer tab 中重造 popover/select 行为。
- 创建流程先选择 preset，再创建并进入 editor；删除必须确认并展示会断开的 Agent。

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

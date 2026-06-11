# 新增 Agent 支持指南（Adding a New Agent）

> 本文是给未来贡献者（含 AI 助手）的操作手册：在 SkillStar 中接入一个新的 Agent CLI
> 需要改哪些代码、按什么顺序、哪些是必做项、哪些是可选项。
> 架构背景见 [AGENTS.md](./AGENTS.md)；前端规范见 [AGENTS-UI.md](./AGENTS-UI.md)。

SkillStar 里"支持一个 Agent"其实是 **三条互相独立的轴**，按需选做：

| 轴 | 作用 | 必做？ |
|---|---|---|
| ① Skills 分发 | Agent 出现在 Settings / Projects / My Skills，技能可链接到它 | ✅ 核心 |
| ② Models 工具同步 | 在 Models 工作台把 Provider 配置写入该 Agent 的磁盘配置文件 | 可选 |
| ③ Usage 订阅 | 在 Usage 面板聚合该厂商的订阅 / 配额 / 余额 | 可选 |

---

## 第 0 步：先想清楚要不要写代码

**用户级自定义 Agent 是零代码路径。** 运行时即可添加：Settings → Agent 连接 →
添加自定义 Agent（后端入口 `add_custom_agent_profile`，定义在
`crates/skillstar-projects/src/projects/agents/custom.rs` 的 `CustomProfileDef`，
持久化在 `~/.skillstar/config/profiles.toml`）。自定义 Agent 支持自定义全局技能目录、
项目级相对路径和 base64 图标，但不能覆盖内置 Agent 的 id。

只有当这个 Agent 值得**开箱即用**（自动检测、官方图标、出现在所有用户的列表里）时，
才需要走下面的代码路径。

---

## 轴①：Skills 分发（核心，通常只需 2 个文件）

### 1. 在内置数据表加一行

`crates/skillstar-projects/src/projects/agents/builtin.rs` 的 `BUILTIN_AGENT_DEFS`：

```rust
// (id, display_name, icon, home_subdirs, project_skills_rel)
(
    "myagent",                    // 唯一 id，全小写
    "My Agent",                   // UI 显示名
    "agents/myagent.svg",         // 相对 public/ 的图标路径
    &[".myagent", "skills"],      // 展开为 ~/.myagent/skills（全局技能目录）
    ".myagent/skills",            // 项目级相对路径；"" = 仅全局（如 OpenClaw）
),
```

设计约束（违反会被现有测试拦截，见 `agents/mod.rs` 的测试区）：

- `project_skills_rel` 在所有 Agent 间必须**唯一**（消歧逻辑已封死，永远返回空）。
- 仅全局的 Agent 填 `""` —— 不要在代码里加 if 特判。
- 两个 Agent 可以共享 home 根目录（如 Antigravity 与 Gemini 共用 `~/.gemini/`），
  只要项目级路径不同即可；这种关系**纯数据表达**，不允许代码特例。
- 路径一律正斜杠；Windows 反斜杠输入由后端归一化。

其余全部自动生效：安装检测（`detect.rs` 会自动建目录并判断 `installed`）、
启用/禁用持久化（`profile_storage.rs`）、项目检测（`detect_project_agents`）、
同步与软链（`sync.rs`）、CLI `--agent myagent`、前端列表渲染。

### 2. 放一个图标

`public/agents/myagent.svg`。前端 `src/components/ui/AgentIcon.tsx` 会自动内联渲染 SVG；
只有像 Antigravity 那样 CSS filter 处理不了的动态 SVG 才需要在该文件加特判分支。

### 3. 检查"仅全局"语义（如适用）

项目部署选择器按 `project_skills_rel` 是否为空过滤仅全局 Agent
（`src/lib/agentProfiles.ts` 的 `supportsProjectDeploy`，被
`ProjectDeployAgentDialog` / `DeployToProjectModal` / `Projects.tsx` 共用）。
新的仅全局 Agent **不需要**改前端 —— 填 `""` 即可。

### 4. 测试与文档

- 若 Agent 有特殊性质（仅全局 / 共享 home 根），在
  `crates/skillstar-projects/src/projects/agents/mod.rs` 测试区加一条守卫测试
  （参考 `openclaw_has_no_project_level_skills_directory`）。
- 跑 `cargo test -p skillstar-projects`（`validate_project_skills_rel_rules`
  会自动校验新行的路径规则）。
- 更新 README.md 的 Agent 列表表格；如有特殊行为，在 AGENTS.md
  "Project Registration and Detection" 一节补一句。
- 检索 `src/i18n/locales/en.json` / `zh-CN.json` 中枚举 Agent 名字的提示文案
  （如 `bannerNoClis`），按需补充。

**冻结接口，勿动：** `AgentProfile` 是 8 字段冻结结构体
（`registry.rs`，前端镜像在 `src/types/index.ts`），跨 Tauri IPC 序列化 ——
新增 Agent 永远不需要改它；`AgentSpec` trait（`spec.rs`）同理。

---

## 轴②：Models 工具同步（可选）

仅当该 Agent 有自己的磁盘配置文件、且希望在 Models 工作台一键写入
Provider（Base URL / API Key / 模型）时才做。现有目标：`claude-code`、`codex`、
`opencode`、`gemini`、`claude-desktop`。

全部改动在 `crates/skillstar-models/src/tool_sync/` + 少量前端：

1. **路径解析** `paths_files.rs`：
   - `resolve_tool_config_path()` 加 `"myagent" => ...` 分支；
   - `resolve_tool_config_file_path()` 加 `("myagent", "<file_id>")` 分支
     （一个 Agent 可有多个文件，如 Codex 的 `config` + `auth`）；
   - `list_tool_config_files()` 加文件清单（驱动前端"磁盘配置文件"编辑器）。
2. **写入/卸载** `sync.rs`：实现 `sync_to_myagent()` 与对应的 unsync 函数，
   返回 `ToolSyncResultFlat`。必须遵守的语义：
   - 只增删**自己管理的字段**，保留用户已有配置（参考各 `*_MANAGED_*` 常量，
     定义在 `types.rs`；新格式则新增常量）；
   - 写前备份（backup_path 语义与现有实现一致）。
3. **沙箱安全**：所有路径必须经 `tool_sync` 的 home 解析（受
   `SKILLSTAR_TOOL_SYNC_HOME` 重定向）。**测试绝不能写真实 `$HOME`**
   —— 集成测试必须设置该环境变量（历史事故见 mod.rs 顶部注释）。
4. **前端**（注册表驱动，只需两处）：
   - `src/features/models/lib/agentRegistry.ts`：在 `PROVIDER_AGENTS` 加一条
     `AgentDescriptor`（toolId / displayName / requiredUrlField / installDocsUrl /
     tagline / disabledTooltip / configPathDisplay），并视情况扩展
     `CONFIG_FILE_TOOLS`。Agent 卡片、接入设置对话框、状态汇总、安装检测
     全部由该注册表驱动，无需新组件；
   - `src/features/models/components/shared/AgentToolIcon.tsx` 的
     `AgentToolIconId` 联合类型 + 图标分支；
   - 如支持 MCP 配置同步，另见 `src/types/index.ts` 的 `MCP_TOOL_IDS`。
5. 跑 `cargo test -p skillstar-models`（含属性测试 `tool_sync_prop_tests`）。

---

## 轴③：Usage 订阅（可选）

仅当要在 Usage 面板展示该厂商的配额/余额时才做。全部在
`crates/skillstar-usage/`：

1. `catalog.rs`：在 `catalog()` 固定目录中加一个 `CatalogEntry`
   （id、显示名、auth 模式、计费周期等）。
2. 按 auth 模式实现 fetcher 并在对应 `dispatch` 注册：
   - API Key 型 → `fetchers/api_key/myvendor.rs`；
   - OAuth 型 → `fetchers/oauth/myvendor.rs`（PKCE / Device Flow / 轮询基建在
     `oauth/` 子模块）；
   - 纯手动录入 → 不需要 fetcher。
3. 所有 HTTP 必须用 `skillstar_core::infra::http_client::probe_http_client`
   （自动走 `config/proxy.json` 代理）。
4. 凭据存储自动走 AES-256-GCM 加密（`crypto.rs` + `storage.rs`），无需额外处理。
5. 前端无需新组件：`SubscriptionEditDialog` / `UsageGrid` 按 catalog 数据驱动渲染。

> ⚠️ `fetchers/oauth/cursor.rs` 被标记为完成态，除非明确要求不要改它。

---

## 提交清单（Checklist）

```text
轴①（必做）
  [ ] builtin.rs 数据表 +1 行
  [ ] public/agents/<id>.svg
  [ ] cargo test -p skillstar-projects 全绿
  [ ] README.md Agent 表格 / i18n 枚举文案（如涉及）
  [ ] 特殊性质 → mod.rs 守卫测试 + AGENTS.md 一句话

轴②（可选）
  [ ] paths_files.rs 三处 match 分支
  [ ] sync.rs 的 sync_to_* / unsync_*（含备份 + managed-keys 语义）
  [ ] lib/agentRegistry.ts +1 AgentDescriptor + shared/AgentToolIcon.tsx 图标分支
  [ ] cargo test -p skillstar-models 全绿（测试必须走 SKILLSTAR_TOOL_SYNC_HOME）

轴③（可选）
  [ ] catalog.rs +1 entry
  [ ] fetchers/<auth_mode>/<vendor>.rs + dispatch 注册
  [ ] cargo test -p skillstar-usage 全绿
```

按 Conventional Commits 提交，scope 用 `agents`（轴①）、`models`（轴②）或
`usage`（轴③），如 `feat(agents): add MyAgent builtin profile`。

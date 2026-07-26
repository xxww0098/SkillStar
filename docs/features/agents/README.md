# 新增 Agent 支持指南（Adding a New Agent）

状态：active

> 本文是给未来贡献者（含 AI 助手）的操作手册：在 SkillStar 中接入一个新的 Agent CLI
> 需要改哪些代码、按什么顺序、哪些是必做项、哪些是可选项。
> 项目边界见 [../../boundaries.md](../../boundaries.md)；前端规范见
> [../frontend/README.md](../frontend/README.md)。

SkillStar 里"支持一个 Agent"其实是 **三条互相独立的轴**，按需选做。内置 Skills
分发注册表以 `vercel-labs/skills/src/agents.ts` 为兼容基线；同步上游时必须同时核对
Agent id、显示名和全局/项目技能目录。SkillStar 自有目标可以作为扩展保留，
但不能改变同名上游 Agent 的目录语义。

| 轴 | 作用 | 必做？ |
|---|---|---|
| ① Skills 分发 | Agent 出现在 Settings / Projects / My Skills，技能可链接到它 | ✅ 核心 |
| ② Models 工具同步 | 在 Models 工作台把 Provider 配置写入该 Agent 的磁盘配置文件 | 可选 |
| ③ Usage 订阅 | 在 Usage 面板聚合该厂商的订阅 / 配额 / 余额 | 可选 |

---

## 第 0 步：先想清楚要不要写代码

**用户级自定义 Agent 是零代码路径。** 运行时即可添加：Settings → Agent 连接 →
添加自定义 Agent（后端入口 `add_custom_agent_profile`，定义在
`crates/skillstar-skills/src/agents/custom.rs` 的 `CustomProfileDef`，
持久化在 `~/.skillstar/config/profiles.toml`）。自定义 Agent 支持自定义全局技能目录、
项目级相对路径和 base64 图标，但不能覆盖内置 Agent 的 id。

只有当这个 Agent 值得**开箱即用**（官方图标、出现在所有用户的列表里）时，
才需要走下面的代码路径。

---

## 轴①：Skills 分发（核心，通常只需 2 个文件）

### 1. 在内置数据表加一行

`crates/skillstar-skills/src/agents/builtin.rs` 的 `BUILTIN_AGENT_DEFS`：

```rust
(
    "myagent",                    // 唯一 id，全小写
    "My Agent",                   // UI 显示名
    home(&[".myagent", "skills"]), // 全局目录；也可用 config/env_or_home/openclaw/unsupported
    ".agents/skills",             // 项目级相对路径
),
```

设计约束（违反会被现有测试拦截，见 `agents/mod.rs` 的测试区）：

- 兼容 open agent skills 的 Agent 应使用共享项目路径 `.agents/skills`；只有上游要求专属目录时才填 `.claude/skills`、`.qoder/skills` 等专属值。
- 多个 Agent 共享 `project_skills_rel` 是正常情况。Project detector 会返回 ambiguous group，manifest 只选择一个 owner；sync/cleanup 必须按路径去重。
- 上游没有 `globalSkillsDir` 的 Agent 使用 `none`，并由 `has_global_skills()` 统一从全局
  选择和部署中排除；项目路径仍按上游填写。
- 两个 Agent 可以共享 home 根目录（如 Antigravity 与 Gemini 共用 `~/.gemini/`）；注册表只表达各自真实技能目标，不从共享根推断安装状态。
- 路径一律正斜杠；Windows 反斜杠输入由后端归一化。

其余全部自动生效：默认关闭与手动启用/禁用持久化（`profile_storage.rs`）、
项目检测（`detect_project_agents`）、同步与软链（`sync.rs`）、CLI `--agent myagent`、
前端列表渲染。

Settings 的 Agent 列表自带**搜索 + 状态筛选**（全部 / 已启用 / 未启用）：搜索同时匹配
显示名和 id；状态段的计数基于搜索结果；任一筛选生效时列表不再折叠成前 10 条，无匹配时
给出重置入口。纯函数在 `src/features/settings/lib/agentFilters.ts`，UI 在
`src/features/settings/components/AgentListFilterBar.tsx`，新增 Agent 无需任何改动。

### 2. 登记图标

内置 Agent 图标统一由 `src/components/ui/icons/agentIcons.ts` 映射到
`@lobehub/icons`，包内 deep import 只能出现在 `src/components/ui/icons/lobe.ts`。
Lobe Icons 有对应品牌时使用品牌 `Color`/`Mono` 组件；没有时使用 `LobeHubMono`
通用图标。不要为内置 Agent 新增 `public/agents/*.svg`。自定义 Agent 的 data URI 和
历史静态资源 fallback 仍由 `AgentIcon.tsx` 处理。

### 3. 检查共享路径或"仅全局"语义（如适用）

项目部署选择器按 `project_skills_rel` 是否为空过滤仅全局 Agent
（`src/lib/agentProfiles.ts` 的 `supportsProjectDeploy`，被
`ProjectDeployAgentDialog` / `DeployToProjectModal` / `Projects.tsx` 共用）。
新的仅全局 Agent **不需要**改前端 —— 填 `""` 即可。共享 `.agents/skills` 的 Agent 同样不需要新增前端分支；现有 disambiguation 与 canonicalization 会按路径处理。

### 4. 测试与文档

- 若 Agent 有特殊性质（仅全局 / 共享 home 根 / 非 universal 项目路径），在
  `crates/skillstar-skills/src/agents/mod.rs` 测试区加一条守卫测试
  （参考共享路径和专属路径的现有测试）。
- 跑 `cargo test -p skillstar-skills`（`validate_project_skills_rel_rules`
  会自动校验新行的路径规则）。
- 若用户可见能力变化，更新根 README 的描述，但不要复制完整 Agent 清单或数量；
  特殊行为写入本文件或 [Skills 行为文档](../skills/README.md)。
- 检索 `src/i18n/locales/en.json` / `zh-CN.json` 中枚举 Agent 名字的提示文案
  （如 `bannerNoClis`），按需补充。

**冻结接口，勿动：** `AgentProfile` 是 8 字段冻结结构体
（`registry.rs`，前端镜像在 `src/types/project.ts`），跨 Tauri IPC 序列化 ——
新增 Agent 永远不需要改它。兼容字段 `installed` 只镜像手动 `enabled`，不得重新接入安装探测；
私有 `AgentSpec` trait 只描述路径与能力，可以随域实现演进。

---

## 轴②：Models 工具同步（可选）

仅当该 Agent 有自己的磁盘配置文件、且希望在 Models 工作台一键写入
Provider（Base URL / API Key / 模型）时才做。现有目标：`claude-code`、`codex`、
`opencode`、`gemini`、`pi`。Claude Code CLI 与 Desktop Code 共用 `claude-code`，不能因运行
入口不同再增加一个 Agent 或 tool id。

全部改动在 `crates/skillstar-models/src/tool_sync/` + 少量前端：

1. **注册表** `agents.rs`：在 `AGENT_SPECS` 加一行 `AgentSpec`
   （id / display_name / binary_name / config_dir_probes / **kind** /
   required_url / 文件清单：每个文件带 file_id、label、format、沙箱 resolver
   和编辑器默认内容）。路径解析、文件清单、config target 列表、安装探测、
   激活时的 URL 校验、`agent_supports_multiple_providers` 的 kind 判定和
   deactivate 分发（`unsync_tool`）全部由这张表驱动——不再有散布的
   per-agent `match` 需要接线。表驱动一致性测试（`agents.rs` 内联）会
   自动覆盖新行。
2. **写入/卸载** `sync.rs`（single 型）或 `multi_provider.rs`（multi 型）：
   实现 writer 与对应的 unsync 函数，返回 `ToolSyncResultFlat`，并在
   `sync_tool_binding` / `resync_active_tools` / `unsync_tool` 的写盘
   dispatch 各加一个分支。必须遵守的语义：
   - 只增删**自己管理的字段**，保留用户已有配置（参考各 `*_MANAGED_*` 常量，
     定义在 `types.rs`；multi 型使用 `skillstar_<id8>` 托管键约定）；
   - 写前备份（backup_path 语义与现有实现一致）。
3. **沙箱安全**：所有路径必须经 `tool_sync` 的 home 解析（受
   `SKILLSTAR_TOOL_SYNC_HOME` 重定向）。**测试绝不能写真实 `$HOME`**
   —— 集成测试必须设置该环境变量（历史事故见 mod.rs 顶部注释）。
4. **前端**（注册表驱动，只需两处）：
   - `src/features/models/lib/agentRegistry.ts`：在 `PROVIDER_AGENTS` 加一条
     `AgentDescriptor`（toolId / displayName / requiredUrlField / **kind** /
     installDocsUrl / tagline / disabledTooltip / configPathDisplay），并视情况
     扩展 `CONFIG_FILE_TOOLS`。**`kind` 决定卡片形态与绑定语义**：
     `"single"`（全局 env，仅一个激活供应商，如 Claude Code / Gemini）渲染
     `AgentHeroCard`；`"multi"`（配置文件原生并存多个供应商 + 指针，如 Codex /
     OpenCode / Pi）渲染 `MultiProviderCard`（供应商列表 + 激活单选 + 增删）。
     前后端注册表各自钉住同一份 toolId 字面量清单
     （`agentRegistry.test.ts` ↔ `agents.rs` 一致性测试），加行时两侧同步；
     Agent 卡片、接入设置对话框、状态汇总和工具配置检查全部由该注册表驱动，无需新组件；
   - `src/features/models/components/shared/AgentToolIcon.tsx` 的
     `AgentToolIconId` 联合类型 + 图标分支；
   - 如支持 MCP 配置同步，除扩展 `src/types/mcp.ts` / Rust 侧对应的 `MCP_TOOL_IDS`
     外，还要在 `src/features/mcp/lib/agentTargets.ts` 登记 Settings Agent id → MCP
     tool id 的能力映射。MCP 卡片轮播会用这张映射与当前手动 `enabled` profiles 取交集；
     不得再叠加本机安装探测，图标与显示名直接来自 `AgentProfile`，不要另建 SVG 清单。
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
  [ ] agentIcons.ts + lobe.ts 的 @lobehub/icons 映射
  [ ] cargo test -p skillstar-skills 全绿
  [ ] README.md 用户能力描述 / i18n 枚举文案（如涉及）
  [ ] 特殊性质 → mod.rs 守卫测试 + Agents/Skills 功能文档

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

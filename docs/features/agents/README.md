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

2026-10 上游对齐要点：`grok` 与 `gemini-cli` 已被上游正式收录，从 extension 区
归位同步区（`grok` 增加 `$GROK_HOME` 支持；`gemini-cli` 项目路径改共享
`.agents/skills`）；`pi` 与 `kilo` 改为 open-skills 语义（Pi 原生读共享路径；
Kilo 全局目录为 `~/.kilo/skills`，`~/.kilocode` 仅上游用于安装检测）；`droid`
项目路径改共享 `.agents/skills`，全局目录留在 `~/.factory/skills` 供旧安装清理。
旧目录中已部署的链接不做迁移，用户重新部署即更新。`discovery` 的
`PRIORITY_SKILL_DIRS` 与上游 `AGENT_PROJECT_SKILL_DIRS` 保持超集关系，缺的容器
目录（`.grok`、`.kilo`、`.kimchi`、`.minimax`、`.posit/assistant`、`.zcode`）
随同步补齐。

**Gemini 的两轴状态各不相同**，不要当成一句话的「支持 / 不支持」：`gemini-cli` 已随上游
进入同步区（全局目录 `~/.gemini/skills`，项目级共享 `.agents/skills`）。
Antigravity 同样落在 `~/.gemini/` 下，但它是 Google Antigravity，与 Gemini CLI 是不同
产品，两个 profile 互不顶替。Antigravity 自己有三种安装状态（app / CLI / IDE），只有
**一个** Agent profile，部署时再扇出到三份 `builtin/skills`，见下面的[镜像目录](#镜像目录一个-profile多份技能目录)。Usage/Cloud Code 中的 Gemini **模型名**
与 Marketplace 的 google-gemini 技能仓库不受影响。旧 v1 provider store 的 `gemini`
字段仅用于迁移读取。

| 轴 | 作用 | 必做？ |
|---|---|---|
| ① Skills 分发 | Agent 出现在 Settings / Projects / My Skills，技能可链接到它 | ✅ 核心 |
| ② 订阅 / 账号 | 在 Accounts/Usage 面板聚合该厂商的订阅配额，必要时接入账号切换 | 可选 |

原「Models 工具同步」轴（把 Provider 配置写入 Agent 磁盘配置）已随模型域整体移除（[D-082](../../decisions.md)）。

---

## 第 0 步：先想清楚要不要写代码

**用户级自定义 Agent 是零代码路径。** 运行时即可添加：Settings → Agent 连接 →
添加自定义 Agent（后端入口 `add_custom_profile`，定义在
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
    ".agents/skills",             // 项目级相对路径（builtin 禁止空串）
),
```

设计约束（违反会被现有测试拦截，见 `builtin.rs` / `agents/mod.rs` 的测试区）：

- 兼容 open agent skills 的 Agent 应使用共享项目路径 `.agents/skills`；只有上游要求专属目录时才填 `.claude/skills`、`.qoder/skills` 等专属值。
- 多个 Agent 共享 `project_skills_rel` 是正常情况。Project detector 会返回 ambiguous group，manifest 只选择一个 owner；sync/cleanup 必须按路径去重。
- 上游没有全局技能目录的 Agent 使用 `unsupported()`（非 `none`），`has_global_skills()` /
  `supports_global()` 会把它从全局选择和部署中排除；**builtin 的项目路径仍须按上游填写且非空**
  （见 `builtin_agent_fields_are_well_formed`）。
- 两个 Agent 可以共享 home 根目录（如 Antigravity 与 Gemini CLI 都落在 `~/.gemini/` 下）；注册表只表达各自真实技能目标，不从共享根推断安装状态，更不能因为共享前缀就让一个 profile 顶替另一个产品。
- 同一产品的多个安装状态**不拆成多个 Agent**，用镜像目录表达（见下一节）。
- 加一行 builtin 时还要同步前端的图标镜像：`src/components/ui/icons/agentIcons.ts` 的 `AGENT_ICON_BY_ID` 与 `agentIcons.test.ts` 里的 `BUILTIN_AGENT_IDS`。漏掉不会报错，只会让该 Agent 静默退回 LobeHub 通用字形。
- 路径一律正斜杠；Windows 反斜杠输入由后端归一化。

其余全部自动生效：默认关闭与手动启用/禁用持久化（`profile_storage.rs`）、
项目检测（`detect_project_agents`）、同步与软链（`sync.rs`）、CLI `--agent myagent`、
前端列表渲染。

Settings 的 Agent 列表自带**搜索 + 状态筛选**（全部 / 已启用 / 未启用）：搜索同时匹配
显示名和 id；状态段的计数基于搜索结果；任一筛选生效时列表不再折叠成前 10 条，无匹配时
给出重置入口。纯函数在 `src/features/settings/lib/agentFilters.ts`，UI 在
`src/features/settings/components/AgentListFilterBar.tsx`，新增 Agent 无需任何改动。

#### 镜像目录：一个 profile，多份技能目录

同一个产品可能有多个并存的安装状态，各自读自己的技能目录。它们**不是**多个 Agent：
拆行会让用户在 Settings 里看到重复条目，还得逐个启用才能全部同步到。

`builtin.rs` 的 `GLOBAL_MIRROR_DEFS` 表达这种关系：profile 的 `global_skills_dir` 仍是
唯一记账真相（链接计数、部署状态、一键解绑都只看它），部署层在每次 link/unlink 后把它
的软链重放（reconcile）到全部镜像目录（`deployment/mirror.rs`）。重放是幂等对账，所以
后装的状态、被产品升级重新解包过的目录都会在下一次部署时自动补齐。

目前只有 Antigravity：一个 `antigravity` profile 统一支持 App、CLI 与 IDE 三种状态，
主全局目录为 `~/.gemini/antigravity/skills`，镜像到 CLI 专属的 `~/.gemini/antigravity-cli/skills`、
跨端共享的 `~/.gemini/skills`、生态配置全局 `~/.gemini/config/skills` 以及 IDE 专属的 `~/.gemini/antigravity-ide/skills`。约束：

- 镜像目录的父目录由对应产品或运行时创建，不存在即视为该状态未安装，SkillStar 不代建（`~/.gemini/skills` 的父目录为 `~/.gemini/`）。
- 绝不向产品的 `builtin/skills` 注入用户软链（那是系统自带技能空间）；部署对账时会自动自愈清理历史遗留在 `builtin/skills` 下的旧软链。
- 镜像里只删软链，绝不动用户或系统自带的真实目录 —— 产品自带的内置技能就住在旁边。
- 弃用的 per-state id（`antigravity-cli` / `antigravity-ide`）通过 `compatible_profile_id`
  与 CLI 的 `normalize_agent_ids` 折叠到 `antigravity`，旧配置和 `--agent antigravity-cli` 继续可用。

### 2. 登记图标

内置 Agent 图标统一由 `src/components/ui/icons/agentIcons.ts` 映射到
`@lobehub/icons`，包内 deep import 只能出现在 `src/components/ui/icons/lobe.ts`。
Lobe Icons 有对应品牌时使用品牌 `Color`/`Mono` 组件；没有时使用 `LobeHubMono`
通用图标。不要为内置 Agent 新增 `public/agents/*.svg`。自定义 Agent 的 data URI 和
历史静态资源 fallback 仍由 `AgentIcon.tsx` 处理。

### 3. 检查共享路径或无全局目录语义（如适用）

- **Builtin**：`project_skills_rel` 必须非空；无全局目录用 `unsupported()`（如 eve /
  promptscript），不是填 `""`。
- **自定义 Agent**：空 `project_skills_rel` 表示仅全局；前端
  `supportsProjectDeploy`（`src/lib/agentProfiles.ts`）按空串过滤，无需新分支。
- 共享 `.agents/skills` 同样不需要新增前端分支；现有 disambiguation 与
  canonicalization 会按路径处理。

### 4. 测试与文档

卡组对单个 Agent 的批量 link/unlink 必须走一次
`batch_toggle_skills_for_agent` IPC，而不是由前端循环调用单项命令。后端 tracing 以
`operation_id` 关联整批操作，并在开始、单项失败和汇总结束事件中记录 Agent、方向、总数、
成功数、失败数与耗时；批次报告保留每个失败 Skill 的完整 error chain。遇到目标位置已有
非 SkillStar 管理的真实目录时必须 fail closed、保留该目录：该项记为 `skipped`（code
`unmanaged_real_directory` + 冲突路径），不得记为 `failed`，也不得覆盖。UI 用界面语言说明
原因，并提供「打开该目录」；关闭 toast 即表示接受跳过。单项 `toggle_skill_for_agent` 仍把
同一种碰撞映射为错误，避免用户以为已经链上。

Settings 的「当前受管技能」主开关不是 Agent 的启用开关，也不是 Hub 同步。它通过
`get_agent_managed_skills_state` / `toggle_agent_managed_skills` 调用
`skillstar-app::agent_managed_skills`：暂停前先把该物理 Global skills 目录的精确活动名字
原子写入 `profiles.toml`，随后仅临时移除这些名字；恢复只尝试 journal 中仍缺失的名字。
失败、Hub 源已消失或未受管目录冲突的项会留在 journal，不得用 Hub 其他技能补齐。journal 按
解析后的目录而非 Agent id 保存，因此共享目录的所有 profile 共同显示、共同 pending；它只记录
恢复意图，绝不声称目录 entry 属于某个 profile，也不会修改冻结的 `AgentProfile` 8 字段契约。

- 若 Agent 有特殊性质（无全局目录 / 共享 home 根 / 非 universal 项目路径），在
  `crates/skillstar-skills/src/agents/builtin.rs` 测试区加一条守卫测试
  （参考 `project_only_agents_have_no_global_path` 与共享/专属路径测试）。
- 跑 `cargo test -p skillstar-skills`（`validate_project_skills_rel_rules` 与
  builtin 字段守卫会自动校验新行）。
- 若用户可见能力变化，更新根 README 的描述，但不要复制完整 Agent 清单或数量；
  特殊行为写入本文件或 [Skills 行为文档](../skills/README.md)。
- 检索 `src/i18n/locales/en.json` / `zh-CN.json` 中枚举 Agent 名字的提示文案
  （如 `bannerNoClis`），按需补充。

**冻结接口，勿动：** `AgentProfile` 是 8 字段冻结结构体
（`registry.rs`，前端镜像在 `src/types/project.ts`），跨 Tauri IPC 序列化 ——
新增 Agent 永远不需要改它。兼容字段 `installed` 只镜像手动 `enabled`，不得重新接入安装探测；
私有 `AgentSpec` trait 只描述路径与能力，可以随域实现演进。

### OMP（Oh My Pi）注册说明

OMP（`@oh-my-pi/pi-coding-agent`，命令 `omp`）与 Pi（`@earendil-works/pi-coding-agent`，
命令 `pi`）是同源但独立的产品：配置根互不读取（`~/.omp` vs `~/.pi/agent`），OMP 自带
`~/.omp/agent/config.yml`（modelRoles）、自有 models.db 目录、会话与认证状态，本机可并存。

- 注册在 `BUILTIN_AGENT_DEFS` 的 extension 区（与 `deepseek`、`workbuddy` 并列，不在
  vercel-labs 上游 id 内）：全局技能目录 `~/.omp/agent/skills`，项目级 `.omp/skills`；
  `skillstar-skills::discovery` 的优先级目录包含 `.omp/skills`。
- `~/.omp/agent/managed-skills` 是 OMP Auto-Learn 的自动生成目录（`manage_skill`
  工具写入），**不纳入** SkillStar 的发现、部署与卸载——工具生成内容不当作
  用户技能，避免噪音与误清理。
- 轴①（Skills 分发）已接入。历史上的「Models 工具同步」注入（OMP 的
  `~/.omp/agent/models.yml`、Pi 的 `~/.pi/agent/models.json` provider 块、角色路由的
  `modelRoles` 指针）已随模型域移除（[D-082](../../decisions.md)），此处仅存历史记录。
  OMP 不读 Pi 的配置，与 Pi 互不影响。

### WorkBuddy 注册说明

WorkBuddy 是腾讯的办公 Agent，技能目录与 CodeBuddy 同构（`~/.workbuddy/skills` /
`.workbuddy/skills`），但产品与配置根都独立，不能共用 `codebuddy` 那一行。它还不在
`vercel-labs/skills` 上游清单里，所以落在 `BUILTIN_AGENT_DEFS` 的 extension 区。
Lobe Icons 没有对应品牌字形，图标走通用 `LobeHubMono`。

轴①已接入。

### Devin 注册说明

Devin 是 Cognition 的软件工程 Agent，`devin` 行与 `vercel-labs/skills` 上游逐字一致：显示名
`Devin for Terminal`，全局 `~/.config/devin/skills`，项目 `.devin/skills`（上游 id 就在
`UPSTREAM_AGENT_IDS` 内，不是 extension 区）。图标走 Lobe Icons 的品牌字形 `DevinColor`。

轴①已接入。Windsurf 已改名 Devin Desktop，但同属 Cognition 不代表两个 profile 可以互相顶替。

Devin 的模型由 Cognition 托管，`~/.config/devin/config.json` 的
`agent.model` 只在自家模型名之间选择，没有 base URL / API Key 之类的自带 provider 配置。
Usage 订阅同理由公开 API 决定，未接入。

## 轴②（historical）：Models 工具同步

已随模型域整体移除（[D-082](../../decisions.md)）：`crates/skillstar-models/src/tool_sync/`
与 `src/features/models/` 均已删除。支持一个新 Agent 不再包含「写 Provider 配置」步骤。

---

## 轴②：订阅 / 账号（可选）

仅当要在 Accounts/Usage 面板展示该厂商的配额/余额（或接入账号切换）时才做。全部在
`crates/skillstar-usage/`：

1. `catalog.rs`：在 `catalog()` 固定目录中加一个 `CatalogEntry`
   （id、显示名、auth 模式、计费周期等）。
2. 按 auth 模式实现 fetcher 并在对应 `dispatch` 注册：
   - API Key 型 → `fetchers/api_key/myvendor.rs`；
   - OAuth 型 → `fetchers/oauth/myvendor.rs`（PKCE / `poll_flow` 轮询 /
     `start_info` 等基建在 `oauth/` 与 `fetchers/oauth/`；无独立 Device Flow 模块）；
   - Cookie 型 → `fetchers/cookie/myvendor.rs`（`AuthMode::Cookie`，用户粘贴
     `Cookie:` header；解析与加密见 `cookie_jar.rs`）；
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
  [ ] 特殊性质 → builtin.rs 守卫测试 + Agents/Skills 功能文档

轴②（可选，订阅/账号）
  [ ] catalog.rs +1 entry（含 AuthMode，Cookie 见 fetchers/cookie/）
  [ ] fetchers/<auth_mode>/<vendor>.rs + dispatch 注册
  [ ] cargo test -p skillstar-usage 全绿
```

按 Conventional Commits 提交，scope 用 `agents`（轴①）、`models`（轴②）或
`usage`（轴③），如 `feat(agents): add MyAgent builtin profile`。

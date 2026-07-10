# SkillStar Rust Workspace 迁移计划

> 状态：**Phases 0–5 已实施**（§11 完成定义已满足）。Phase 6（`skillstar-mcp`）仍为延后决策门，非本次目标。
>
> ## Task checklist（执行勾选）
>
> - [x] Phase 0：基线与文档决策（module-first SSOT、依赖方向、projects→skills 所有权）
> - [x] Phase 1：清理虚依赖 + DAG/feature 护栏
> - [x] Phase 2：Skills 拥有 lockfile + update detection
> - [x] Phase 3：合并 Projects 到 Skills（agents/deployment/projects/patrol/terminal）
> - [x] Phase 4：收深 facade；删除 Tauri skills pass-through + SkillManager
> - [x] Phase 5：skillstar-app library-only；单一 skillstar 二进制；CLI use case 下沉 App
> - [x] §11 完成定义与 §8.1 验证矩阵
> - [ ] Phase 6（非目标 / deferred）：`skillstar-mcp` 决策门
>
> 本文件是迁移执行计划，不是当前项目结构的 SSOT。实施过程中，技术栈、crate、目录和依赖结构仍以
> [AGENTS.md](./AGENTS.md) 为准；后端行为以 [docs/backend.md](./docs/backend.md) 为准。
> 每个迁移 PR 都必须先更新对应 SSOT，再修改代码。

## 1. 结论

当前问题不是 crate 数量本身，而是部分 module 的 seam 放错、interface 过宽，以及应用流程仍散落在
`skillstar-core`、域 crate、`skillstar-app` 和 `src-tauri` 多层。

本计划选择“局部合并 + interface 收深”，不做全 workspace 大合并：

1. 将 `skillstar-projects` 合并进 `skillstar-skills`，让技能库、Agent、项目部署、更新 reconcile 和 patrol
   形成一个内聚 module。
2. 保留 `skillstar-providers`、`skillstar-marketplace`、`skillstar-models`、`skillstar-ai`、
   `skillstar-usage`、`skillstar-fingerprint`、`skillstar-ssh`、`skillstar-sync` 的独立 crate。
3. 将 `skillstar-app` 深化为跨域 use-case module，并改成 library-only；同名可执行文件只由
   `src-tauri` package 产出。
4. 将技能域实现从 `skillstar-core` 移回 `skillstar-skills`；`core` 只保留基础设施和确实需要共享的契约。
5. 删除 `src-tauri/src/core/skills/` 中的 pass-through module；Tauri command 只承担 DTO、错误、State、
   事件和窗口 adapter。
6. MCP 是否独立成 `skillstar-mcp` 延后到单独决策门，不在本次合并中顺手实施。

## 2. 目标与非目标

### 2.1 目标

- 提升 locality：修改技能安装、更新、卸载或项目部署时，主要改动集中在一个 crate。
- 提升 depth：调用方通过少量 use-case interface 获得完整行为，不再自行拼装内部步骤。
- 消除概念循环：不再因为 `skills → projects` 而把 patrol、项目导入等流程留在 Tauri 层。
- 缩小 `skillstar-core` 的编译扇出和知识范围。
- 让 GUI 与 CLI 复用同一套应用流程。
- 保持所有持久化格式、IPC 名称、前端行为和用户数据路径兼容。
- 每个迁移 PR 都可单独验证、回滚，且不夹带产品行为变更。

### 2.2 非目标

- 不合并 `models + ai`、`usage + fingerprint`、`ssh + sync`。
- 不重写 SQLite schema、JSON/TOML 存储格式或用户目录布局。
- 不重命名现有 Tauri command、事件名或前端 feature。
- 不在结构迁移中新增 Provider、Agent、Usage fetcher 或产品能力。
- 不把 MCP registry、MCP 本地 store 和 tool projection 强行迁入新 crate。
- 不借迁移机会修改 `crates/skillstar-usage/src/fetchers/oauth/cursor.rs`。

## 3. 设计原则

### 3.1 Module 与 interface

- crate 只是 module 的一种物理形式；frontend feature 不自动对应 backend crate。
- 一个 module 通过一个明确 interface 对外提供行为，内部实现默认 `pub(crate)` 或私有。
- interface 包含方法、类型、错误、调用顺序、不变量、配置和性能约束，不只是 Rust 类型签名。
- 禁止用裸 `pub mod` 或 glob re-export 把整个实现目录当作外部 interface。

### 3.2 Depth 与 deletion test

- 删除一个深 module 后，复杂度应重新散回多个调用方；如果删除后只需改 import path，它就是
  pass-through，应移除。
- `src-tauri/src/core/skills/*` 的单行 re-export 不提供 leverage，应删除。
- 只有一个 production adapter 的 trait 是 hypothetical seam；除非存在真实 test adapter 或第二种行为实现，
  否则不保留空泛 trait。

### 3.3 依赖类别

- Skills/Projects 的核心行为是 in-process 或 local-substitutable，可以直接深化并在合并后的 interface 上测试。
- SSH、S3、Provider HTTP 等 true external 依赖继续留在各自 crate，通过真实 production adapter 与 test
  adapter 验证。
- 内部 test seam 不因测试方便而暴露为 crate 的外部 interface。

### 3.4 测试策略

- 新测试从合并后的外部 interface 观察结果，不越过 interface 断言内部状态。
- 当新的 interface 测试覆盖旧行为后，删除只验证 pass-through implementation 的旧测试，不重复分层测试。
- 所有涉及工具配置和 `$HOME` 的测试必须设置 `SKILLSTAR_TOOL_SYNC_HOME` 或数据根覆盖到临时目录。

## 4. 目标组织结构

以下是目标职责，不是当前结构清单：

```text
src-tauri                    # Tauri adapter：command、DTO、State、事件、窗口
    │
    ├── skillstar-app        # 跨域 use case、CLI 解析与应用流程；library-only
    │
    ├── skillstar-skills     # 技能库 + Agent + 项目部署 + patrol + Launch Deck
    ├── skillstar-marketplace
    ├── skillstar-models
    ├── skillstar-ai
    ├── skillstar-usage
    ├── skillstar-fingerprint
    ├── skillstar-ssh
    └── skillstar-sync
             │
             ├── skillstar-core       # 基础设施 + 共享契约
             └── skillstar-providers  # Provider identity/balance 零依赖 SSOT
```

允许保留的域间依赖方向：

```text
skillstar-ai       -> skillstar-models -> skillstar-providers
skillstar-usage    -> skillstar-fingerprint + skillstar-providers
skillstar-sync     -> skillstar-skills
skillstar-app      -> 完成跨域 use case 所需的域 crate
域 crate           -> skillstar-core（仅使用基础设施或共享契约）
src-tauri          -> skillstar-app 或域 crate 的公开 facade
```

禁止形成的方向：

```text
skillstar-skills      -X-> skillstar-marketplace
skillstar-marketplace -X-> skillstar-skills
skillstar-models      -X-> skillstar-ai
skillstar-usage       -X-> skillstar-models
任意域 crate          -X-> src-tauri
skillstar-core        -X-> 任意域 crate
```

搜索结果安装、Usage 账号写入 CLI、AI 配置解析等跨域行为由 `skillstar-app` 编排，不通过反向依赖解决。

## 5. `skillstar-skills` 目标形态

### 5.1 建议目录

为减少无意义改名，合并后的 crate 继续使用 `skillstar-skills`：

```text
crates/skillstar-skills/src/
├── lib.rs
├── library/                 # hub、installed、local、repo、bundle、pack、update
├── agents/                  # builtin/custom profile、检测、全局部署
├── projects/                # 注册、manifest、扫描、导入、刷新
├── deployment/              # project/global link-copy reconcile
├── patrol/                  # 配置、检查循环、事件无关的运行逻辑
├── terminal/                # Launch Deck 的 CLI registry/session/types
├── git/                     # git 操作
└── shared/                  # 仅 crate 内共享实现，不作为杂项公共出口
```

目录名允许在实施 PR 中按现有模块迁移成本微调，但职责不可重新混回同一大文件。

### 5.2 目标 interface

crate root 不直接公开所有实现模块，而是公开少量能力面：

- Skill library：安装、批量安装、更新、卸载、本地创作、内容与 bundle 生命周期。
- Agent registry：列出、检测、维护 builtin/custom Agent profile。
- Deployment：向全局 Agent 或项目部署、重同步、移除，并返回完整失败报告。
- Projects：注册、扫描、导入、保存、reconcile 项目技能清单。
- Patrol：启动一次检查或创建 runner；事件发送由调用方注入 adapter。
- Terminal：列出可启动 CLI、解析可执行文件和生成 session 标识。

interface 应返回结构化结果，不直接生成 Tauri toast、窗口事件或 CLI 文本。

### 5.3 行为不变量

合并后必须保持：

- 安装、更新、卸载的 lockfile 与磁盘状态一致。
- 更新技能后同时刷新 link 与 copy 部署，失败汇总而非首错中止。
- 项目部署继续使用 symlink → junction → copy 回退链。
- 本地技能删除同时清理 Agent 与项目引用。
- 项目导入继续采用 adopt + hub link + project manifest reconcile。
- Patrol 继续批量预取 repo，并区分“无更新”和“检查失败/未知”。
- Windows 路径、换行和 junction 行为保持兼容。

## 6. 文件迁移映射

### 6.1 从 `skillstar-core` 移入 Skills

| 当前路径 | 目标 | 说明 |
| --- | --- | --- |
| `crates/skillstar-core/src/types/lockfile.rs` | `skillstar-skills` 的 library/lockfile implementation | Lockfile 只属于技能生命周期 |
| `crates/skillstar-core/src/types/update_checker.rs` | `skillstar-skills` 的 library/update implementation | 删除当前双层 wrapper |
| `crates/skillstar-skills/src/lockfile.rs` | 替换为真实实现或 facade | 不再 re-export Core 实现 |
| `crates/skillstar-skills/src/update_checker.rs` | 保留单一实现入口 | 合并路径、repo 与 git 行为 |

`skillstar-core/src/types/skill.rs` 暂时保留，因为 Marketplace 与 Skills 都使用该共享契约。是否另建轻量
contracts crate 不属于本次迁移。

### 6.2 从 Projects 移入 Skills

| 当前路径 | 目标模块 |
| --- | --- |
| `skillstar-projects/src/projects/agents/` | `skillstar-skills/src/agents/` |
| `skillstar-projects/src/projects/project_manifest/` | `skillstar-skills/src/projects/` |
| `skillstar-projects/src/projects/sync.rs` | `skillstar-skills/src/deployment/` |
| `skillstar-projects/src/patrol/` | `skillstar-skills/src/patrol/` |
| `skillstar-projects/src/terminal/` | `skillstar-skills/src/terminal/` |

迁移完成后删除 `crates/skillstar-projects/`，并同步更新 workspace、AGENTS.md 的 Workspace Crates 表和项目树。

### 6.3 从 Tauri 下沉或删除

| 当前路径 | 处理方式 |
| --- | --- |
| `src-tauri/src/core/skills/*` 单行 re-export | 删除；调用方改用最终 facade |
| `src-tauri/src/core/skills/mod.rs::SkillManager` | 删除，或由真实的双 adapter seam 替换 |
| `src-tauri/src/core/patrol.rs` | 保留 Tauri State/Emitter adapter；检查和循环行为移入 Skills |
| `src-tauri/src/commands/projects.rs` 的项目导入流程 | 下沉到 Skills/`skillstar-app` use case |
| `src-tauri/src/cli/install.rs`、`manage.rs` 的跨域流程 | 下沉到 `skillstar-app` |
| `src-tauri/src/commands/usage_commands.rs` 的刷新/持久化流程 | 单域部分下沉 Usage；CLI switch 组合留在 App |

### 6.4 `skillstar-app` 清理

- 删除 `skillstar-app` 的 `[[bin]] name = "skillstar"` 和当前必然失败的 `src/bin/skillstar.rs`。
- 保留 `skillstar-app` library，拥有 CLI 解析、模式识别和跨域 use case。
- `src-tauri/src/main.rs` 不再维护一份与 Clap enum 重复的 command 字符串列表；由 App interface 判断进入 CLI
  还是 GUI。
- CLI use case 返回结构化结果，由 CLI adapter 负责 stdout/stderr 和进程退出码。

## 7. 分阶段实施

每个阶段建议独立 PR。不得在当前大规模 dirty worktree 上直接叠加结构迁移；先收束现有业务变更，建立可复现基线。

### Phase 0：基线与文档决策

目标：冻结迁移前行为，明确目标 seam。

任务：

- 收束当前未提交业务改动，迁移从干净工作树开始。
- 先更新 AGENTS.md：
  - 将“一个功能域 = 一个 crate”改为“功能先形成内聚 module，满足晋升条件后再成为 crate”。
  - 记录允许的内部依赖方向。
  - 标记 `skillstar-projects` 将并入 `skillstar-skills`。
- 更新 docs/backend.md，记录 Skills/Projects 合并后的行为所有权；不得复制可枚举清单。
- 保存迁移前 `cargo metadata --no-deps` 与 feature tree 作为评审附件，不提交生成产物。
- 运行完整基线检查，确认失败项是迁移前已存在还是迁移引入。

退出条件：

- 目标依赖图和 crate 所有权完成评审。
- 基线测试结果可复现。
- 当前业务分支与迁移分支不再同时修改同一批文件。

### Phase 1：清理虚依赖并建立护栏

目标：在移动文件前先简化依赖图。

任务：

- 验证并移除无源码引用的 `skillstar-skills → skillstar-marketplace` 依赖。
- 验证并移除无源码引用的 `skillstar-fingerprint → skillstar-core` 依赖。
- 新增内部 dependency DAG 检查，禁止未来新增反向边。
- 新增或扩展 feature-unification 检查，防止重依赖经默认 feature 被意外重新启用。
- 明确 `impersonate` 由最终 binary root 选择，避免 leaf crate 默认 feature 隐式决定最终构建图。

退出条件：

- `cargo metadata` 内部图无循环、无上述虚边。
- `cargo tree -p skillstar -e features` 与预期一致。
- workspace check/test 通过。

### Phase 2：让 Skills 拥有 lockfile 与 update detection

目标：先清理 Core 中最明显的技能域 implementation。

任务：

- 将 lockfile 与 update detection 的真实实现移动到 `skillstar-skills`。
- 在同一 PR 更新所有调用方，删除 Core 旧实现和 Skills pass-through wrapper。
- 将原实现测试迁移到最终 Skills interface；避免长期保留两套测试与兼容层。
- 保持存储文件路径、序列化格式、mutex 语义和 git 行为不变。

退出条件：

- `skillstar-core::types` 不再导出 lockfile/update checker。
- repo update、subtree hash、失败预取语义均有 interface 级测试。
- Core 与 Skills 的定向测试、workspace test 通过。

### Phase 3：合并 Projects 到 Skills

目标：消除 Skills/Projects 错置 seam 和概念循环。

建议顺序：

1. 在 Skills 内建立私有目标模块与最终 facade。
2. 移动 Agent registry 和 deployment，实现编译通过。
3. 移动 project manifest、scan、import、reconcile。
4. 移动 patrol 与 terminal。
5. 更新 `skillstar-app`、`skillstar-sync` 和 `src-tauri` 调用路径。
6. 删除 `skillstar-projects` workspace member 和目录。
7. 删除临时 re-export；不得把临时兼容 seam 带到下一阶段。

Patrol seam：

- 检查、批量预取、节流、统计属于 Skills implementation。
- Tauri event emitter 是 production adapter。
- 测试使用 recording/in-memory adapter；这使 seam 拥有两个真实 adapter。
- Tauri State 与窗口生命周期仍留在 `src-tauri`。

退出条件：

- 仓库中不存在生产代码 `skillstar_projects::` 引用。
- `crates/skillstar-projects/` 已删除，AGENTS.md 已同步。
- 安装、更新、卸载、项目导入、Agent 部署和 patrol 回归测试通过。
- 合并后的 crate root 没有把全部实现重新声明为 `pub mod`。

### Phase 4：收深域 interface，压薄 Tauri command

目标：让 crate 分层产生真实 leverage，而不是只改变 import path。

优先顺序：

1. Skills：CLI install、项目导入、更新 cascade、卸载清理。
2. Usage：create/update/refresh/reauth/persist snapshot 的单域流程。
3. SSH：connect、host-key gate、SFTP、progress 的完整远程操作。
4. MCP：read → mutate → write → tool sync 的事务顺序。

规则：

- command 函数只解析参数、映射 DTO/错误、注入 adapter、调用 use case。
- command 不直接知道 storage、fetcher、crypto、repo scanner 或 SFTP 的调用顺序。
- 以 `src-tauri/src/commands/s3_sync.rs` 调用 `push_all`/`pull_manifest`/`restore_entries` 的形态为参考。
- 新 facade 返回结构化 outcome，包括部分成功和失败明细。

退出条件：

- `src-tauri/src/core/skills/` pass-through module 全部删除。
- 单 adapter `SkillManager` 删除。
- 重点 command 的行为测试从域/App interface 驱动。
- Tauri command 层不再持有跨域事务顺序。

### Phase 5：统一 App 与 CLI 所有权

目标：只保留一个真实 `skillstar` binary，并让 GUI/CLI 复用应用流程。

任务：

- 将 `skillstar-app` 改成 library-only。
- 由 App 的 CLI interface 统一判断 CLI/GUI 模式，删除 Tauri main 中重复 command 清单。
- 将 `src-tauri/src/cli/` 的跨域行为迁入 App；Tauri package 只保留进程入口 adapter。
- 保持所有既有命令名、alias、退出码和输出语义。
- 为 headless CLI 增加测试，防止未知/新增命令误启动 GUI。

退出条件：

- workspace 只产出一个预期的 `skillstar` executable。
- `skillstar --help`、`--version` 和全部现有子命令由同一 Clap 定义驱动。
- CLI 与 GUI 调用同一 use-case implementation。

### Phase 6：MCP 决策门

本阶段不是默认实施项。只有同时满足下列条件，才创建 `skillstar-mcp`：

- MCP 本地 store/tool projection 已有小而稳定的外部 interface。
- Marketplace MCP snapshot 的数据库所有权能够迁移且不会形成 `mcp ↔ marketplace` 循环。
- MCP 变更节奏、测试范围或依赖集合证明独立编译单元能带来实际收益。
- Tauri 不再负责 Marketplace schema 与 installed MCP schema 的转换事务。
- 新 crate 通过 deletion test：删除后复杂度会明确散回多个调用方，而不只是改 import path。

若条件不满足，保持 MCP 为现有 crate 内的深 module，并只修正 interface 与文档归属。

## 8. 验证矩阵

### 8.1 每个阶段必跑

```bash
cargo check --workspace --locked
cargo test --workspace --locked
bun run lint
bun run test
```

涉及 TS 导出时额外运行：

```bash
bun run types:gen
bun run build
```

涉及依赖/feature 时额外检查：

```bash
cargo metadata --no-deps --format-version 1
cargo tree -p skillstar -e features
cargo tree -p skillstar -e features -i wreq
```

Windows 路径、junction、换行和 shell 行为由现有 Windows CI 兜底；合并前不得仅凭 macOS 本地测试宣告完成。

### 8.2 Skills/Projects 行为回归

- repo 单技能、多技能、root-first、full-depth 安装。
- hub-only 与 project-level CLI 安装。
- symlink 成功、junction/copy 回退、broken link、stale copy 刷新。
- Agent 全局 link、batch link、unlink 与失败聚合。
- 技能更新后的 sibling hash、Agent relink 与 project cascade。
- 本地技能 create/adopt/delete/reconcile。
- project register/scan/import/save/rebuild/remove。
- patrol 批量预取、取消、失败保留旧状态与事件顺序。
- S3 manifest restore 继续通过 Skills 最终 interface 安装技能。

### 8.3 CLI 回归

- `list`、`find/search`、`install/add`、`update`、`remove/rm/uninstall`、`init/create`、`publish`、
  `doctor`、`pack`、`help`、`--version`。
- 新增子命令无需同步另一份字符串清单。
- 无 GUI 的环境中 CLI 命令不会误启动窗口并挂起。

## 9. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| 当前工作树已有大规模业务改动 | 先完成或拆离当前分支；结构迁移从干净基线开始 |
| 文件移动导致 Git 历史难读 | 先做纯 move，再做小范围 interface 修改；避免同一提交大规模格式化 |
| 合并后公开面反而更大 | 先定义 facade，再移动实现；目标模块默认私有 |
| 临时 re-export 永久残留 | 临时 seam 只能存在于同一迁移 PR，合并前必须删除 |
| Core 类型移动破坏序列化 | 不改 serde 字段、默认值、文件路径和版本常量；增加旧 fixture 回归 |
| CLI binary 所有权调整影响打包 | 在移除 App bin 前先验证 Tauri package 的 GUI/CLI 双模式产物 |
| Rust feature 是加法合并 | 由最终 binary root 显式选择重 feature，并在 CI 检查 feature tree |
| 测试误操作真实 Home | 所有 tool-sync/数据根测试使用临时目录覆盖，禁止真实 `$HOME` |
| patrol 下沉后引入 Tauri 依赖 | domain runner 只依赖 sink/cancellation interface；Emitter adapter 留在 Tauri |

## 10. 建议提交序列

每个提交都必须可编译，提交信息使用英文 Conventional Commits：

1. `docs(architecture): define crate migration seams`
2. `chore(workspace): remove stale internal dependencies`
3. `refactor(skills): own lockfile and update detection`
4. `refactor(skills): absorb agent and project deployment`
5. `refactor(skills): move patrol and launch deck modules`
6. `refactor(app): centralize cross-domain workflows`
7. `refactor(commands): remove pass-through skill adapters`
8. `refactor(cli): make the application layer library-only`
9. `chore(workspace): enforce dependency and feature guards`

如果某一步需要产品行为改变，应拆成后续独立 `feat`/`fix` PR，不混入上述结构提交。

## 11. 完成定义

迁移只有在以下条件全部满足时才算完成：

- AGENTS.md、docs/backend.md 与实际 crate/目录/职责同步。
- `skillstar-projects` 已合并并从 workspace 删除。
- Skills 拥有 lockfile、update、Agent、project deployment 和 patrol 的实现。
- `skillstar-core` 不再包含技能域实现。
- `src-tauri/src/core/skills/` pass-through module 与单 adapter `SkillManager` 已删除。
- Tauri command 只承担 adapter 职责，不编排域事务。
- `skillstar-app` 是 library-only，workspace 只有一个真实 `skillstar` executable。
- 依赖图无循环、无已知虚边，重 feature 由 binary root 显式控制。
- 全部 Rust/前端测试、lint、构建及 Windows CI 通过。
- 没有提交 `target/`、`dist/`、`node_modules/` 或临时迁移产物。

## 12. 已决与延后决策

已决：

- 采用局部合并，不采用大规模 capability mega-crate。
- 合并后沿用 `skillstar-skills` 名称，减少迁移噪音。
- `skillstar-providers` 保持独立零依赖 leaf crate。
- SSH 与 S3 Sync 保持分离。
- `Skill` 共享契约暂留 Core。
- App 保留为 library-only application module；可执行文件由 Tauri package 唯一拥有。

延后：

- 是否创建 `skillstar-mcp`。
- 是否将共享契约从 Core 再拆成独立轻量 crate。
- 是否进一步统一 SSH/S3 progress primitive；至少出现第三个真实复用方后再评估。

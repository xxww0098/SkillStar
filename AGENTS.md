# 和我讲中文

# SkillStar — Agent 项目规则

SkillStar 的界面在 `gui`（基于 **`nsh`** 应用框架），域逻辑在 Rust workspace，同一个 `skillstar` 二进制提供 GUI 与 CLI。

设计、实现和界面验证均以 `nsh` 应用为准（`nsh check gui` / `nsh test gui`）；验证前核对运行进程的可执行路径。GPUI 壳已随 D-110 删除。

本文件是 Agent 的唯一规则入口。项目树和依赖见 [docs/boundaries.md](./docs/boundaries.md)，运行架构见 [docs/architecture.md](./docs/architecture.md)，界面视觉与交互见 [docs/features/frontend/README.md](./docs/features/frontend/README.md)。同一事实只写在一个地方。

## 应用框架（nsh）

- **唯一 GUI 与应用框架是 `nsh`**（位于 `gui`，当前锁定 `0.6.x`）。本项目**不使用** React、Tauri、Electron 或已移除的 GPUI（D-110）。
- **三层架构与单向依赖**：
  1. **TSX 视图与状态层（`gui/App.tsx` + `src/`）**：采用 `nsh` 细粒度信号响应式模型（`signal` / `effect` / `<If>` / `<Each>`）构建页面与 `src/chrome/` 共用控件。
  2. **原生桥接层（`gui/native`，crate `ss-gui-bridge`）**：通过 `#[native_module]` 将 `crates/ss-*` 域 facade 暴露给 TSX；除纯同步判定外一律走 blocking 池。
  3. **原生宿主层（`gui/host`，crate `ss-gui-host`）**：桌面 AppView 宿主，macOS AppKit 与 Windows Win32 各一份（`host/build.rs` 监听 `App.tsx`、`src/`、`native/` 变更并在构建时执行 `nsh build` 内嵌产物），由仓库根 `src/main.rs`（package `ss-bin`，产出二进制 `skillstar`）调用 `ss_gui_host::run` 启动。关闭主窗口只隐藏窗口，进程和后台唤醒继续（D-115）。
- **`nsh` 关键开发纪律**：
  - **组件体只执行一次**：`nsh` 无虚拟 DOM 重渲染；严禁在组件函数顶层解构响应式 `props` 或写 `const x = sig()` 做一次性求值，所有信号与动态 `props` 必须放在 **JSX 表达式**或**访问器函数 `() => ...`** 内读取。
  - **Keep-alive 与页面隔离**：`Shell` 对页面单次挂载并按 `display` 切换保活；`src/pages/*` 之间禁止互相 import，跨页跳转与数据失效通知统一走 `src/shell/events.ts`。
  - **主题与零帧交互**：色彩一律经 `paletteToken()` 读取 `gui/theme.json`（`current` 深色 / `paper` 浅色双主题），常规悬停走 `hoverStyle` / `activeStyle` 即时换色，不做颜色弹簧过渡。

## 修改前先确定唯一文档落点

- 目录、crate、所有权或依赖方向：先更新 `docs/boundaries.md`。运行拓扑、数据所有权或技术选择同时更新 `docs/architecture.md`。
- 功能行为或 UX：先更新对应的 `docs/features/<域>/README.md`。
- 长期架构选择：`docs/decisions.md`。
- 根因不直观、可能复发的故障：`docs/errors.md`。
- 用户安装、能力或 CLI 用法：`README.md`。
- 文档与代码同一变更序列完成。

可枚举清单和计数以代码注册表及其测试为准。文档写规则并链接代码。

## 架构红线

- GUI 经 nsh 桥 crate（`gui/native`）调用域 facade。展示层不直接读业务文件或业务网络；除纯判定外的调用一律走 blocking 池。
- 进程分派只放仓库根 `src/main.rs`（package `ss-bin`，产出二进制 `skillstar`）。市场快照接线和频道周期唤醒放 `ss-app`；共享频道远程编排在 `ss-channels`，账号、OAuth 与额度在 `ss-accounts`，本机项目技能 MCP 在 `ss-mcp`，其余域逻辑进入对应 `crates/ss-*`。GUI 宿主在 `gui/host`（`ss-gui-host`）。
- MCP serve（`ss_mcp::serve`）在 askpass 和市场快照初始化之前返回，且不调用 `ss_app::bootstrap::prepare_process`。
- 跨域 use case 进入 `ss-app`。域 crate 不靠反向依赖完成编排。
- 远程 HTTP 走 `ss_core::infra::http_client::probe_http_client`，并遵守用户代理配置。
- 新功能先成为既有内聚 crate 的私有 module 和窄 facade。只有独立变更节奏、依赖集合或 deletion test 证明有收益时才新增 crate。
- 新逻辑放进该 crate 已有的私有 module，不堆进杂项公共出口。
- 页面私有组件默认留在页面目录。第二个页面要复用、且能独立命名时，才抽到 `src/chrome/`。目录以 boundaries 的「GUI 壳（nsh）」为准。

## 安全与实现约束

- 新 Rust 依赖用 `cargo add`。workspace 版本归一化在根 `Cargo.toml`。
- 单个源文件不超过约 1000 行；接近 800 行时按职责拆开。
- 测试不写真实 `$HOME`。tool-sync 测试把 `SKILLSTAR_TOOL_SYNC_HOME` 设到临时目录。
- 数据目录经 `ss-core` 解析。`SKILLSTAR_DATA_DIR`、`SKILLSTAR_HUB_DIR` 等覆盖继续生效。
- `target/` 和 `.codegraph/` 不是项目结构，也不进提交。

## 常用验证

```bash
nsh check gui
nsh test gui
cargo check --workspace --locked
cargo test --workspace --locked
```

按风险先跑最小相关测试，再跑上面的完整门槛。完整门禁清单以 `.github/workflows/ci.yml` 为准。结构改动再跑：

```bash
bash scripts/internal/check_workspace_deps.sh
bash scripts/internal/check_file_size.sh
bash scripts/internal/check_error_strings.sh
bash scripts/internal/check_dep_graph_doc.sh
```

首次 clone 先装 git hooks，见 [README](./README.md#git-hooks)。修改 workflow 前先读文件顶部的 `Failure lessons`。依赖变化只更新 `Cargo.lock`。

## 文档索引

| 文档 | 唯一职责 |
| --- | --- |
| [README.md](./README.md) | 面向用户的产品、安装、使用和 CLI |
| [docs/boundaries.md](./docs/boundaries.md) | 项目树、目录所有权、依赖方向和接缝 |
| [docs/architecture.md](./docs/architecture.md) | 运行拓扑、数据所有权、不变量与技术选择 |
| [docs/decisions.md](./docs/decisions.md) | 长期架构决策及其后果 |
| [docs/errors.md](./docs/errors.md) | 可复发故障、根因和自检 |
| [docs/features/](./docs/features/) | 随实现变化的行为、契约和 UX |
| [docs/storage-layout.md](./docs/storage-layout.md) | `~/.skillstar` 存储分类、默认位置与迁移（[D-087](./docs/decisions.md)） |

功能入口：

- [Agents](./docs/features/agents/README.md)
- [界面](./docs/features/frontend/README.md)
- [Skills](./docs/features/skills/README.md)
- [Marketplace](./docs/features/marketplace/README.md)
- [MCP](./docs/features/mcp/README.md)
- [Project skills MCP](./docs/features/project-skills-mcp/README.md)
- [Accounts](./docs/features/accounts/README.md)
- [Usage](./docs/features/usage/README.md)
- [Team](./docs/features/team/README.md)
- [Platform](./docs/features/platform/README.md)

## Agent skills

### Issue tracker

Issues 和 PRD 在本仓库 GitHub Issues，用 `gh` 操作。见 `docs/agents/issue-tracker.md`。

### Triage labels

五个标签：needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix。见 `docs/agents/triage-labels.md`。

### Domain docs

领域词汇在根目录 `CONTEXT.md`。架构决策在 `docs/decisions.md`。见 `docs/agents/domain.md`。

## 提交规范

英文 Conventional Commits：`type(scope): description`。常用 type：`feat`、`fix`、`docs`、`refactor`、`test`、`chore`。

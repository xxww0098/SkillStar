# 和我讲中文

# SkillStar — Agent 项目规则

SkillStar 的界面在 `crates/ss-gpui`，域逻辑在 Rust workspace，同一个 `skillstar` 二进制提供 GUI 与 CLI。

本文件是 Agent 的唯一规则入口。项目树和依赖见 [docs/boundaries.md](./docs/boundaries.md)，运行架构见 [docs/architecture.md](./docs/architecture.md)，界面视觉与交互见 [docs/features/frontend/README.md](./docs/features/frontend/README.md)。同一事实只写在一个地方。

## 修改前先确定唯一文档落点

- 目录、crate、所有权或依赖方向：先更新 `docs/boundaries.md`。运行拓扑、数据所有权或技术选择同时更新 `docs/architecture.md`。
- 功能行为或 UX：先更新对应的 `docs/features/<域>/README.md`。
- 长期架构选择：`docs/decisions.md`。
- 根因不直观、可能复发的故障：`docs/errors.md`。
- 用户安装、能力或 CLI 用法：`README.md`。
- 文档与代码同一变更序列完成。历史稿只放 `docs/others/` 并标记 `historical`。

可枚举清单和计数以代码注册表及其测试为准。文档写规则并链接代码。

## 架构红线

- GUI 经 `spawn_domain` 调用域 facade。展示层不直接读业务文件或业务网络。
- 进程分派只放 `crates/skillstar`。市场快照接线和频道周期唤醒放 `ss-app`。域逻辑进入 `crates/ss-*`。
- MCP serve 在 askpass 和市场快照初始化之前返回，且不调用 `prepare_process`。
- 跨域 use case 进入 `ss-app`。域 crate 不靠反向依赖完成编排。
- 远程 HTTP 走 `ss_core::infra::http_client::probe_http_client`，并遵守用户代理配置。
- 新功能先成为既有内聚 crate 的私有 module 和窄 facade。只有独立变更节奏、依赖集合或 deletion test 证明有收益时才新增 crate。
- 新逻辑放进该 crate 已有的私有 module，不堆进杂项公共出口。
- GPUI 能力内部默认私有。第二个调用方出现、且能独立命名时，才抽到 `skill_card/` 或 `chrome/`。目录以 boundaries 的「GPUI 壳模块」为准。

## 安全与实现约束

- 新 Rust 依赖用 `cargo add`。workspace 版本归一化在根 `Cargo.toml`。
- 单个源文件不超过约 1000 行；接近 800 行时按职责拆开。
- 测试不写真实 `$HOME`。tool-sync 测试把 `SKILLSTAR_TOOL_SYNC_HOME` 设到临时目录。
- `crates/ss-usage/src/fetchers/oauth/cursor.rs` 只在用户明确要求时修改。
- 数据目录经 `ss-core` 解析。`SKILLSTAR_DATA_DIR`、`SKILLSTAR_HUB_DIR` 等覆盖继续生效。
- `target/` 和 `.codegraph/` 不是项目结构，也不进提交。

## 常用验证

```bash
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
| [docs/others/README.md](./docs/others/README.md) | 活动路线图和冻结历史 |

功能入口：

- [Agents](./docs/features/agents/README.md)
- [界面](./docs/features/frontend/README.md)
- [Skills](./docs/features/skills/README.md)
- [Marketplace](./docs/features/marketplace/README.md)
- [Project skills MCP](./docs/features/project-skills-mcp/README.md)
- [Accounts](./docs/features/accounts/README.md)
- [Usage](./docs/features/usage/README.md)
- [Sync](./docs/features/sync/README.md)
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

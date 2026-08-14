# 和我讲中文

# SkillStar — Agent 项目规则

SkillStar 是 Tauri v2 桌面应用：React SPA 负责界面，Rust workspace 负责域逻辑；同一个 `skillstar` 二进制同时提供 GUI 与 CLI。

本文件是 Agent 的唯一规则入口。完整项目树和依赖边界见 [docs/boundaries.md](./docs/boundaries.md)，运行架构见 [docs/architecture.md](./docs/architecture.md)。不要在 README、CLAUDE 或功能文档复制这两类事实。

## 修改前先确定唯一文档落点

- 目录、crate、所有权或依赖方向变化：先更新 `docs/boundaries.md`；运行拓扑、数据所有权或技术选择变化时同时更新 `docs/architecture.md`。
- 功能行为或 UX 变化：先更新对应的 `docs/features/<域>/README.md`。
- 长期有效的架构选择：记录到 `docs/decisions.md`。
- 根因不直观、可能复发的重要故障：记录到 `docs/errors.md`。
- 用户安装、能力或 CLI 用法变化：更新 `README.md`。
- 文档与代码同一变更序列完成，不留“待补”。历史稿只放 `docs/others/` 并标记 `historical`。

同一事实只允许一个 SSOT。可枚举清单和计数以代码注册表及其测试为准，文档只描述规则并链接代码，不手抄数量。

## 架构红线

- 前端只能通过 Tauri `invoke()` 和事件访问后端，不直接访问文件系统或业务网络接口。
- `src-tauri/src/commands/` 只做命令注册、DTO、State、错误和事件适配；域逻辑进入 `crates/*`，Tauri 专用胶水进入 `src-tauri/src/core/`。
- 跨域 use case 进入 `skillstar-app`，禁止靠域 crate 反向依赖完成编排。
- 所有远程 HTTP 必须通过 `skillstar_core::infra::http_client::probe_http_client`，遵守用户代理配置。
- 新功能先成为既有内聚 crate 的私有 module 和窄 facade；只有独立变更节奏、依赖集合或 deletion test 证明有收益时才新增 crate。
- 不在既有 crate 的杂项公共出口或命令包装层堆放新逻辑。
- 前端 feature 内部实现默认私有；跨 feature 复用优先提升到 `src/components/shared/` 或 `src/lib/`，并通过 `scripts/internal/check_feature_imports.sh`。

## 安全与实现约束

- 新 Rust 依赖用 `cargo add`；workspace 版本归一化在根 `Cargo.toml` 维护。不要直接手写新增依赖。
- 不让单个源文件超过约 1000 行；接近 800 行时开始拆分。
- 测试不得写真实 `$HOME`。tool-sync 测试必须设置 `SKILLSTAR_TOOL_SYNC_HOME` 到临时目录。
- 除非用户明确要求，不修改 `crates/skillstar-usage/src/fetchers/oauth/cursor.rs`。
- 不手改 `src/types/generated/`；修改 Rust 来源后运行 `bun run types:gen` 并提交生成结果。
- 不绕过后端解析真实数据目录；`SKILLSTAR_DATA_DIR`、`SKILLSTAR_HUB_DIR` 等覆盖必须继续生效。
- 不把 `target/`、`dist/`、`node_modules/` 或 `.codegraph/` 当作项目结构或提交内容。

## 常用验证

```bash
bun run lint
bun run build
bun run test
cargo check --workspace --locked
cargo test --workspace --locked
```

按风险优先运行最小相关测试，再运行上面的完整门槛。结构改动还应运行：

```bash
bash scripts/internal/check_workspace_deps.sh
bash scripts/internal/check_feature_imports.sh
bash scripts/internal/check_file_size.sh
bash scripts/internal/check_command_boundaries.sh
```

首次 clone 先装 git hooks，让上述门禁在提交和推送时自动执行：见 [README](./README.md#git-hooks)。

CI 由 `.github/workflows/ci.yml`、`windows-ci.yml` 和 `release.yml` 负责。修改 workflow 前先阅读文件顶部的 `Failure lessons`。本地使用 Bun，但 Windows CI 使用 npm；依赖变化必须同步 `bun.lock` 与 `package-lock.json`。

## 文档索引

| 文档 | 唯一职责 |
| --- | --- |
| [README.md](./README.md) | 面向用户的产品、安装、使用和 CLI 入口 |
| [docs/boundaries.md](./docs/boundaries.md) | 完整项目树、目录所有权、依赖方向和接缝 |
| [docs/architecture.md](./docs/architecture.md) | 运行拓扑、数据所有权、不变量与技术选择 |
| [docs/decisions.md](./docs/decisions.md) | 长期架构决策及其后果 |
| [docs/errors.md](./docs/errors.md) | 可复发故障、根因和自检方法 |
| [docs/features/](./docs/features/) | 随功能实现变化的行为、契约和 UX |
| [docs/others/README.md](./docs/others/README.md) | 活动路线图和冻结历史的决策表 |

功能入口：

- [Agents](./docs/features/agents/README.md)
- [Frontend](./docs/features/frontend/README.md)
- [Skills](./docs/features/skills/README.md)
- [Marketplace](./docs/features/marketplace/README.md)
- [MCP](./docs/features/mcp/README.md)
- [Models](./docs/features/models/README.md)
- [Usage](./docs/features/usage/README.md)
- [Sync](./docs/features/sync/README.md)
- [Platform](./docs/features/platform/README.md)

## Agent skills

### Issue tracker

Issues 和 PRD 跟踪在本仓库的 GitHub Issues，通过 `gh` CLI 操作。见 `docs/agents/issue-tracker.md`。

### Triage labels

使用默认五个 triage 标签（needs-triage / needs-info / ready-for-agent / ready-for-human / wontfix）。见 `docs/agents/triage-labels.md`。

### Domain docs

Single-context：根目录 `CONTEXT.md` + `docs/adr/`。见 `docs/agents/domain.md`。

## 提交规范

使用英文 Conventional Commits：`type(scope): description`。常用 type 为 `feat`、`fix`、`docs`、`refactor`、`test`、`chore`。

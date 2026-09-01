# RUST.md — 项目工程画像（/rust-skills:rust 系列命令的状态文件）
<!-- rust-skills:managed:start schema=1 -->
## Facets

- 默认：`artifact=lib, maturity=production`（9 个 `crates/skillstar-*` 成员均只交付 lib target）。
- 覆盖：`src-tauri`（package `skillstar`）=`artifact:desktop, maturity:production`；它是默认成员和 Tauri 组合根，同一二进制也分派 CLI 子命令。
- 成熟度证据：仓库通过非 prerelease 的多平台安装包、签名 updater 与 tag `v*` 发布；没有待确认项。

## 基线

- Workspace：虚拟根，10 个成员；`default-members = ["src-tauri"]`。所有成员均为 edition 2024、MSRV 1.94.1、resolver 3；日常构建工具链固定为 1.97.1。
- 规范：rust-skills v0.0.11（112 条分级规则）。
- Features：10 个 package 均未声明 package feature；依赖 feature 由根 `[workspace.dependencies]` 与成员清单选择。
- Lints：10 个成员均继承 workspace lint；`clippy::todo`、`unimplemented`、`dbg_macro` 为 deny，其他存量问题使用项目门禁/基线收紧。
- Profiles：`release` 使用 thin LTO、1 codegen unit、strip symbols；`release-fast` 关闭 LTO 并使用 16 codegen units；没有通配 package `opt-level`。
- Lock：应用型 workspace 的 `Cargo.lock` 已跟踪；`cargo metadata --no-deps --format-version 1 --locked` 成功。
- 风险扫描（排除 `cfg(test)` 项、文件型测试模块与 integration roots）：生产路径有 13 个裸 `.unwrap()`；`src-tauri` 有 6 个 `unsafe` 构造和 4 处 extern ABI 语法。172 个 print 宏均位于 CLI/askpass 用户协议输出。唯一无界 channel 位于 `release_scanner.rs:201`，sender 只发送一次完成结果，队列实际上限为 1。生产清单没有通配 `opt-level`。

## Crate 图

消费者到全部内部依赖（`→`；normal 边由 `Cargo.toml` 决定，0 条 dev-only、0 条 build）：

- `skillstar → {app, channels, sync, skills, models, marketplace, usage, git, core}`
- `skillstar-app → {channels, skills, models, marketplace, usage, git, core}`
- `skillstar-channels → {skills, git, core}`；`skillstar-sync → {core}`
- `skillstar-skills → {git, core}`
- `skillstar-models → {core}`；`skillstar-usage → {core}`
- `skillstar-marketplace → core`；`skillstar-git → core`
- 内部依赖出度 0：`skillstar-core`

最长链（叶子在左，5 条边）：

`skillstar-core ← skillstar-git ← skillstar-skills ← skillstar-channels ← skillstar-app ← skillstar`

## 域划分

- `src-tauri`：GUI/CLI 组合根；`commands/` 仅做 Tauri DTO、State、错误与事件适配，`core/` 保存 Tauri 生命周期胶水。
- `skillstar-app`：跨域 use case 与共享 CLI 解析；`skillstar-core`：路径、配置、共享契约与基础设施。
- 业务域：`skillstar-skills`（技能/项目/部署/Agent profile/GitHub App 身份）、`skillstar-channels`（共享频道/patrol）、`skillstar-marketplace`（市场/MCP catalog）、`skillstar-models`（provider/AI/MCP/tool sync）、`skillstar-usage`（订阅/OAuth/配额）、`skillstar-sync`（SSH/SFTP）。
- 叶子能力：`skillstar-git`（Git transport/ops）、`skillstar-core::providers`（Provider identity/balance 元数据，无产品域依赖）。完整所有权与依赖红线的 SSOT 是 `docs/boundaries.md`，运行和数据所有权的 SSOT 是 `docs/architecture.md`。
- 布局以业务域 crate 为主，crate 内再按内聚模块拆分；不是横跨 workspace 的技术层目录。
- 测试：单元测试主要贴近实现或放同模块文件；有 6 个 package-level integration test roots，无 `tests/common.rs`/`tests/common/mod.rs`。项目模块门禁检查 435 个 `.rs`，结果为 0 个新孤儿、0 个基线孤儿、0 个过期基线项。

## 债务清单

- [ ] `debt:ERR-03:crates/skillstar-marketplace` · production facet 下仍有 2 个裸 unwrap（`remote/publisher_repos.rs:161,424`）；ERR-03 要求生产路径不用裸 unwrap。
- [ ] `debt:ERR-03:crates/skillstar-models` · production facet 下仍有 1 个裸 unwrap（`tool_sync/omp_provider.rs:198`）。
- [ ] `debt:ERR-03:crates/skillstar-skills` · production facet 下仍有 1 个裸 unwrap（`skill_group.rs:166`）。
- [ ] `debt:ERR-03:crates/skillstar-usage` · production facet 下仍有 1 个裸 unwrap（`oauth/local_server.rs:269`）。
- [ ] `debt:ERR-03:src-tauri` · production facet 下仍有 2 个裸 unwrap（`core/acp_client/runner.rs:158,159`）。
- [ ] `debt:UNSAFE-01:Cargo.toml` · workspace 尚未统一设置 `unsafe_code = "deny"` 并只对确需 FFI 的 crate 定点放开；当前生产 unsafe 集中在 `src-tauri`。
- [ ] `debt:UNSAFE-02:src-tauri` · `main.rs` Windows FFI 与 `core/dock_menu.rs` macOS runtime hook 共 6 个 unsafe 构造，尚未逐块用精确 `// SAFETY:` 前置条件覆盖。

## 最近评审

- 无；`document` 只投影当前状态，不生成 review 历史快照。
<!-- rust-skills:managed:end -->

<!-- rust-skills:human:start -->
## 人工上下文

领域术语、取舍与无法从代码推导的约束。
<!-- rust-skills:human:end -->

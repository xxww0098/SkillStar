# RUST.md — 项目工程画像（/rust-skills:rust 系列命令的状态文件）
<!-- rust-skills:managed:start schema=1 -->
## Facets

- 默认：`artifact=lib`。`crates/ss-*` 成员交付 library。
- 覆盖：仓库根 package `skillstar`（`src/main.rs`）=`artifact:bin`；它是默认成员和进程入口，同一二进制分派 CLI 与 GPUI。`ss-gpui` 另有一个调用 `run()` 的薄 bin。
- 成熟度：Tauri 安装包和签名 updater 已退役（D-091）。tag `v*` 只上传 `skillstar` 二进制。下面的 unwrap/unsafe 计数来自退役前的审计，不要当成当前数字。

## 基线

- Workspace：根 `Cargo.toml` 同时是 package `skillstar` 与 workspace；成员是 `.` 与 `crates/*`；`default-members = ["."]`。edition、MSRV 和工具链以根 `Cargo.toml` 与 `rust-toolchain.toml` 为准。
- 规范：rust-skills v0.0.11（112 条分级规则）。
- Features：10 个 package 均未声明 package feature；依赖 feature 由根 `[workspace.dependencies]` 与成员清单选择。
- Lints：10 个成员均继承 workspace lint；`clippy::todo`、`unimplemented`、`dbg_macro` 为 deny，其他存量问题使用项目门禁/基线收紧。
- Profiles：`release` 使用 thin LTO、1 codegen unit、strip symbols；`release-fast` 关闭 LTO 并使用 16 codegen units；没有通配 package `opt-level`。
- Lock：应用型 workspace 的 `Cargo.lock` 已跟踪；`cargo metadata --no-deps --format-version 1 --locked` 成功。
- 风险扫描（排除 `cfg(test)` 项、文件型测试模块与 integration roots）：下面的 unwrap 计数来自退役前的审计，不要当成当前数字。172 个 print 宏均位于 CLI/askpass 用户协议输出。唯一无界 channel 位于 `release_scanner.rs:201`，sender 只发送一次完成结果，队列实际上限为 1。生产清单没有通配 `opt-level`。

## Crate 图

消费者到全部内部依赖（`→`；normal 边由 `Cargo.toml` 决定，0 条 dev-only、0 条 build）：

- 依赖图的唯一事实源是 `docs/boundaries.md` 的 mermaid。不要在这里维护第二份。
- `skillstar` 只依赖 `ss-gpui`、`ss-app`、`ss-git`。`ss-gpui` 调用域 facade。

## 域划分

- 仓库根 `src/main.rs`：进程分派。`ss-gpui`：窗口和展示。`ss-app`：跨域 use case、CLI、进程启动和频道唤醒。
- `ss-core`：路径、配置、共享契约与基础设施。
- 业务域：`ss-skills`（技能/项目/部署/Agent profile/GitHub App 身份/共享频道）、`ss-marketplace`（市场快照）、`ss-usage`（订阅/OAuth/配额/账号切换）、`ss-sync`（SSH/SFTP）。模型域 crate 已随 D-082 移除。
- 叶子能力：`ss-git`（Git transport/ops）、`ss-core::providers`（Provider identity/balance 元数据，无产品域依赖）。完整所有权与依赖红线的 SSOT 是 `docs/boundaries.md`，运行和数据所有权的 SSOT 是 `docs/architecture.md`。
- 布局以业务域 crate 为主，crate 内再按内聚模块拆分；不是横跨 workspace 的技术层目录。
- 测试：单元测试主要贴近实现或放同模块文件；有 6 个 package-level integration test roots，无 `tests/common.rs`/`tests/common/mod.rs`。项目模块门禁检查 435 个 `.rs`，结果为 0 个新孤儿、0 个基线孤儿、0 个过期基线项。

## 债务清单

- [ ] `debt:ERR-03:crates/ss-marketplace` · production facet 下仍有 2 个裸 unwrap（`remote/publisher_repos.rs:161,424`）；ERR-03 要求生产路径不用裸 unwrap。
- [ ] `debt:ERR-03:crates/ss-skills` · production facet 下仍有 1 个裸 unwrap（`skill_group.rs:166`）。
- [ ] `debt:ERR-03:crates/ss-usage` · production facet 下仍有 1 个裸 unwrap（`oauth/local_server.rs:269`）。
- [ ] `debt:UNSAFE-01:Cargo.toml` · workspace 尚未统一设置 `unsafe_code = "deny"` 并只对确需 FFI 的 crate 定点放开。`src-tauri` 已删除，不要把那边的 unsafe 计回当前债务。

## 最近评审

- 无；`document` 只投影当前状态，不生成 review 历史快照。
<!-- rust-skills:managed:end -->

<!-- rust-skills:human:start -->
## 人工上下文

领域术语、取舍与无法从代码推导的约束。
<!-- rust-skills:human:end -->

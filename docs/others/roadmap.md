# 结构治理路线图

状态：active

本文件只维护尚未完成的结构债、顺序和验收。当前项目树与允许依赖以 [../boundaries.md](../boundaries.md) 为准；已完成的 workspace 迁移在本目录的历史稿中冻结。

基线日期：2026-07-14。数字只表示本次代码审计快照，实施时必须重跑对应命令，不能把本页当成永久计数 SSOT。

## 已完成基线

- [x] Skills/Projects 所有权并入 `skillstar-skills`，删除旧 projects crate。
- [x] fingerprint 并入 Usage、AI 并入 Models、SSH 并入 Sync。
- [x] 建立 workspace dependency、文件大小、i18n、feature import、error string 和 clippy 棘轮。
- [x] 文档从根级大杂烩重构为 boundaries/architecture/features/others 信息模型。

## P1：阻止当前边界继续恶化

### 前端 feature 越界

状态：completed

2026-07-14 的 `check_feature_imports.sh` 报告存量基线、一个新增越界和一个过期基线。先修复新增与 stale，再逐域缩减基线。

- [x] 将 MCP 与 Marketplace 共用的 `PublisherAvatar` 提升到 `src/components/shared/`，调用者只依赖 identity + size interface。
- [x] 删除已经失效的 baseline 行。
- [x] Settings 只组合 Models、S3、Usage/Fingerprint 各域公开 section，删除反向深层依赖。
- [x] 将远程技能工作区迁入 `my-skills/remote/`，SSH 只公开主机、连接与远程操作接口，依赖收敛为 `my-skills → ssh`。
- [x] 清空 feature import baseline；公开根 `index.ts` 是允许接缝，跨域深层 reach-in 继续由闸门拒绝。

验收：

```bash
bash scripts/internal/check_feature_imports.sh
```

### 单一 Cargo lockfile

状态：completed

根 `Cargo.lock` 与 `src-tauri/Cargo.lock` 同时被跟踪且内容不同，而 Cargo metadata 以根 workspace 为准。

- [x] 确认 Cargo metadata、CI、release 与 package scripts 都从 workspace 根消费 lockfile。
- [x] 删除无消费者的 nested lockfile，并由 `.gitignore`、workspace guard、Linux CI 防止重生。

验收：`git ls-files '*Cargo.lock'` 只返回 workspace 权威 lockfile，CI 的 `--locked` 全绿。

### `Skill` 契约重复

状态：completed

`skillstar-core` 与 `skillstar-marketplace` 各存在一个公开 `Skill` 结构，名称相同但语义边界不清。

- [x] 核对字段、构造器和消费者：Marketplace 副本与 core 契约完全相同，运行代码已消费 core 类型。
- [x] 删除 Marketplace 中的重复类型和 helper，Marketplace crate root 只重导出 core 契约。
- [x] 将 Marketplace `models` 模块设为私有，并用类型同一性测试锁定公开契约，不增加转换链。

## P2：让 facade 比实现更窄

状态：completed

当前域 crate 存在较多公开 symbol，Tauri command 也直接依赖多个域 facade。目标不是减少文件，而是减少调用方必须理解的概念。

- [x] 以 `src-tauri` 和 `skillstar-app` 为消费者审计各域实际使用的 public surface，删除无消费者的 terminal、circuit-breaker 和 Tauri 直通模块。
- [x] 将只在 crate 内消费的实现模块改为 private 或 `pub(crate)`；Marketplace 的 `Skill` 契约继续复用 core 类型，不用重导出伪装收敛。
- [x] 建立 Skills 内容/更新/adoption/share/deploy-status facade，以及 App 的 Usage、技能组部署和 storage maintenance 跨域 use case。
- [x] 增加 `check_command_boundaries.sh`：command 新增直接文件系统/path ownership 即失败，存量只允许缩减；复杂事务在域/app 测试中直接验证。

验收：`cargo check --workspace --locked` 与 `cargo test --workspace --locked` 全绿；结构棘轮无新增债务。

## P2：持续文档治理

- [ ] 把 `xxww-docs` 的确定性审计接入 CI，确保 `CLAUDE.md`、核心宪章、状态标记和 docs 根布局不回退。
- [ ] active 功能文档只描述稳定契约；catalog、Agent、Publisher、command 等清单继续由代码和测试看门。
- [ ] 每季度复核 `docs/others/README.md`，已无保留价值的 historical 文档由用户拍板删除。

## 执行纪律

- 每个整改独立提交，先补能证明边界的测试，再移动实现。
- 不以 crate 数、目录数或文件数作为成功指标。
- 新功能先进入现有内聚 module；是否拆 crate 由变更节奏、依赖集合和 deletion test 决定。
- 完成一项后更新本文件的状态和可重跑证据，不留下失真的旧数字。

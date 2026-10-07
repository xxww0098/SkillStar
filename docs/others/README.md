# Others 文档决策表

状态：active

本目录只保存活动计划和冻结历史，不承载当前架构或功能契约。当前事实从根 [AGENTS.md](../../AGENTS.md)、[boundaries.md](../boundaries.md)、[architecture.md](../architecture.md) 和 `docs/features/` 进入。

| 文档 | 状态 | 决策 | 理由 / 当前 SSOT |
| --- | --- | --- | --- |
| [roadmap.md](./roadmap.md) | active | 保留 | 尚未完成的结构债、顺序和验收；结构事实仍归 boundaries |
| [crate-domain-redesign-2026-10.md](./crate-domain-redesign-2026-10.md) | historical | 冻结 | 已实施的 D-086 crate 域重设计执行记录（迁移映射、不变量、验证）；当前结构见 boundaries |
| [rust-engineering-audit-2026-08.md](./rust-engineering-audit-2026-08.md) | historical | 冻结 | 2026-08 对照《Rust 大型项目开发宝典》的一次性全仓审计快照；只保留**未落地**的量化发现、实测推翻宝典的结论与方法学边界。已落地部分见 git 历史；dev profile / `build-override` / `target/` 搬迁的决定见 `decisions.md` D-032 |

2026-10 清理：已删除功能的一次性调研、原型与 spec（模型域 D-082、MCP D-074、Learn/教程 D-053、`skill_hooks`）及已完成的迁移稿已移除；历史见 git。

## 维护规则

- historical 文档只增加状态/来源说明，不随当前实现重写。其中的 `src/`、`src-tauri/` 和 React 组件是当时的路径，不是当前代码。
- 对应功能已被决策移除（D-053 / D-074 / D-082 等）的调研、原型、归档 spec 不再冻结保留：直接删除，Git 历史即归档。
- active 计划完成后，要么删除（Git 历史即归档），要么经用户确认冻结；不能继续冒充当前 SSOT。
- 新的临时文档进入本目录时，必须在上表增加一行“保留 / 合并 / 删除”决策。
- 每季度复核一次；未被稳定索引引用且没有决策价值的历史稿应由用户拍板删除。

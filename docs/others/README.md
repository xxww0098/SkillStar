# Others 文档决策表

状态：active

本目录只保存活动计划和冻结历史，不承载当前架构或功能契约。当前事实从根 [AGENTS.md](../../AGENTS.md)、[boundaries.md](../boundaries.md)、[architecture.md](../architecture.md) 和 `docs/features/` 进入。

| 文档 | 状态 | 决策 | 理由 / 当前 SSOT |
| --- | --- | --- | --- |
| [roadmap.md](./roadmap.md) | active | 保留 | 尚未完成的结构债、顺序和验收；结构事实仍归 boundaries |
| [workspace-migration-wave1.md](./workspace-migration-wave1.md) | historical | 冻结 | 已完成的 Skills/Projects、facade、单 binary 迁移；当前结构见 boundaries |
| [workspace-migration-wave2.md](./workspace-migration-wave2.md) | historical | 冻结 | 已完成的 fingerprint/AI/SSH crate 吸收；当前结构见 boundaries |
| [usage-card-refactor-2026-07.md](./usage-card-refactor-2026-07.md) | historical | 冻结 | 已实施设计与审查过程；当前 Usage 契约见 `features/usage` |
| [mcp-modern-design-research.md](./mcp-modern-design-research.md) | historical | 冻结 | 2026-08 一次性外部调研快照（MCP 2026-07-28 规范、官方 registry、客户端配置矩阵）；被采纳的结论进入 `features/mcp` 与 `decisions.md` |
| [mcp-current-state-audit.md](./mcp-current-state-audit.md) | historical | 冻结 | 2026-08 一次性代码盘点快照；其 B.4-a/F1/F2/A.3-f 与 R1 第 1 条已被 P0 修复实现，当前 MCP 契约见 `features/mcp` |
| [rust-engineering-audit-2026-08.md](./rust-engineering-audit-2026-08.md) | historical | 冻结 | 2026-08 对照《Rust 大型项目开发宝典》的一次性全仓审计快照；只保留**未落地**的量化发现、实测推翻宝典的结论与方法学边界。已落地部分见 git 历史；dev profile / `build-override` / `target/` 搬迁的决定见 `decisions.md` D-032 |

## 维护规则

- historical 文档只增加状态/来源说明，不随当前实现重写。
- active 计划完成后，要么删除（Git 历史即归档），要么经用户确认冻结；不能继续冒充当前 SSOT。
- 新的临时文档进入本目录时，必须在上表增加一行“保留 / 合并 / 删除”决策。
- 每季度复核一次；未被稳定索引引用且没有决策价值的历史稿应由用户拍板删除。

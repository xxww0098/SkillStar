# Learning

状态：active

本文件是私人教程、学习身份与 Guide/进度契约的单一事实来源。Skill 安装、快照与本地 sidecar 见 [Skills](../skills/README.md)。crate 边界见 [boundaries.md](../../boundaries.md)，运行时数据位置见 [architecture.md](../../architecture.md)，冻结决定见 [D-051](../../decisions.md#d-051来源复合身份精确内容修订与-skillstar-learning)。

## 所有权

- `skillstar-learning` 拥有 `SkillIdentity` / `SkillRevision`、私人 CSP-strict HTML tutorial、Guide / GuideRevision / GuideStep、LearningProgress、GuideDraft，以及各自的 freshness 与原子持久化。
- `skillstar-app::learning` 拥有 `Skill → ResolvedSkill` 投影和跨域生成 use case。`Skill.name` 只是当前安装表中的临时查找句柄，绝不进入稳定 key。
- `skillstar-skills` 拥有只读快照、lockfile/Git HEAD 事实和本地 UUID sidecar；`skillstar-channels` 拥有 numeric repository identity 与 subscription provenance。learning 不依赖这两个 crate。
- Tauri command/core 只保留 ACP 进程、State、事件和 DTO 适配。Learn UI（#47）、Block JSON 转换（#44）与社区发布不在本域本阶段。

## 身份与 revision

三类来源，解析优先级 `channel > local > ordinary Git`：

| 来源 | Identity | Revision 内容绑定 | 说明 |
| --- | --- | --- | --- |
| Git | canonical repository + ref selector + content root | HEAD commit/tree + 当前 v2 content hash | 本地编辑保持同一 identity，只产生新 revision |
| Local | UUID sidecar | 当前 v2 content hash | 改名不改 identity；复制/外部 adopt 必须 mint 新 UUID |
| Channel | GitHub numeric `repository_id` + content root | 不可变 release commit + 当前 v2 content hash | 仓库改名不改 identity；可选 release 标签不是 key 的一部分 |

稳定 key 是 domain-separated SHA-256，不把 URL 或路径塞进文件名。`name`、展示名、URL alias、安装/生成时间均不参与 key。解析缺失、重复或互相矛盾时 fail closed，绝不退回 name key。

同名不同 repo、同 repo 不同 folder、同 repo/folder 不同 ref selector 都是不同 identity，学习记录不得合并。

## 私人教程

- 存储根：`~/.skillstar/learning/tutorials/<identity-key>/`，路径由 `skillstar-core::infra::paths` 解析并尊重 `SKILLSTAR_DATA_DIR`。
- **双读单写**：先读 identity 路径；缺失时只读旧 `~/.skillstar/tutorials/<name-key>/`。所有新写入只写 identity 路径。P0 不删除、重命名或覆盖旧目录。
- 旧 artifact 没有来源证据，即使同名或 content hash 碰巧相同也不能自动绑定到新 identity，也不能转为 Guide Draft。用户重新生成后才得到 bound artifact。
- HTML 必须是完整文档，带精确 CSP meta，禁止脚本、表单、iframe、外链；必须覆盖快照中的每个源文件，并包含至少一个可访问的内联 SVG。
- freshness：generated revision 与当前 revision 不同即 content stale；generator fingerprint（prompt bundle hash + artifact schema）不同即 generator stale。兼容旧 IPC 时按既有优先级投影一个 reason。
- 落盘使用跨进程文件锁、同步写入、staging/backup 目录替换。ACP 失败、校验失败或生成期间内容变化时，最后一个可用 artifact 保持不变。启动读取会恢复中断窗口留下的 backup。

## 窄 facade

生产调用只走：

- `load_private_tutorial(resolved, generator_fingerprint)`
- `commit_private_tutorial(resolved, inventory, generator_fingerprint, raw_html)`
- `list_guides` / `get_guide`
- `load_progress` / `save_progress`（key 必含 Guide revision）
- `create_guide_draft_from_tutorial`（只接受 bound、已校验 artifact）

存储路径、lock、HTML parser 与 Block parser 保持私有。source resolution 在 app adapter，不在本 crate。

## 验证

```bash
cargo test -p skillstar-learning --locked
cargo test -p skillstar-skills --lib tutorial --locked
cargo test -p skillstar-app --lib learning --locked
cargo check --workspace --locked
```

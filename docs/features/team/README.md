# Team intelligence

状态：active

本文件是本机团队智能（recall、skill health、friction notes、digest）的单一事实来源。这不是已删除的 Learn/教程域（[D-053](../../decisions.md#d-053移除学习功能与-skillstar-learning)）：不读写 `~/.skillstar/learning/`，不生成 Guide HTML，不启动 ACP。

架构选择见 [D-056](../../decisions.md#d-056团队智能留在-skillstar-skills私有-module)。

## 所有权

- `skillstar-skills::team` 拥有语料枚举、BM25、friction 评分、learnings/usage/recall 持久化与 health/digest 投影。
- 路径只通过 `skillstar-core::infra::paths::team_store_path()`（`state/team.json`）。
- CLI 适配在 `skillstar-app::cli::team`；本切片不增加 Tauri command 或 GUI 页。
- `skillstar find` 仍只搜 Marketplace 快照。`skillstar team recall` 只搜已安装 Skill 与本地 notes。

## 闭环

蒸馏自 teamai-cli 的 Context + Improvement，映射到 SkillStar 已有 Execution（install / deploy / channels）：

1. Agent 使用已安装 Skill（`team used`）。
2. 任务开始前用 `team recall` 检索本机技能正文与 notes（BM25 + 邻接 boost）。
3. 会话摩擦（打断、拒工具、重试、纠正）记入 `team friction`；超过阈值提示 `team share`。
4. `team health` / `team digest` 投影 usage × freshness × recall coverage，标出 silent Skill。

## Recall

- 语料：Hub（含指向 local 的 link）与 `hub/local` 下可读的 `SKILL.md`，加上 store 里的 notes。
- 分词：拉丁词（长度 ≥ 2，去掉短停用词）+ CJK 单字与二元组。
- 排序：BM25（k1=1.2, b=0.75）；name 加权重复索引。命中的 learning 若绑定 Skill，给该 Skill 邻接 +1.5。
- 空 query 返回空列表，不写 store。成功命中会追加 recall event，供 health 使用。
- 只读 `content::read_raw` / 本地 `SKILL.md`，不 materialize Git worktree。

## Health 与 digest

对每个已安装 Skill：

- `usage_count` / `recall_count` 来自本地 store。
- `freshness = exp(-days_since_last_signal / 30)`，signal 取 last used、last recalled、`SKILL.md` mtime 的最晚者；全无则按 90 天。
- `score = 0.45·usage_norm + 0.35·freshness + 0.20·recall_norm`。
- 从未使用且从未被召回 → `silent`。其余按分数分为 healthy / aging / stale。

Digest 汇总 skill 数、note 数、coverage（至少召回过一次的比例）、friction 次数、worth-documenting 次数、silent 名单、用量最高的 Skill、最近 notes。

## Friction 与 notes

- 分数 = `2*interrupts + 2*rejects + retries + 2*corrections`。阈值 3（与 teamai-cli「值得记下的会话」同一直觉）。
- `team share` 写入本地 note（title/body 必填）；可选 `--skill` 与 `--tags`。这不是教程，也不会晋升为 `SKILL.md`（promote 不在本切片）。
- 未来 schema 的 `state/team.json` fail-closed：拒绝读与写，避免猜测迁移。
- store 裁剪：notes 最多 200，usage/recall/friction 事件各 500，按时间丢掉最旧。

## CLI

```bash
skillstar team recall "pull request tests"
skillstar team health
skillstar team digest
skillstar team friction --interrupts 2 --retries 8 --task "Fix hook injection"
skillstar team share --title "…" --body "…" --skill pr-review --tags ci
skillstar team notes
skillstar team used pr-review
```

`--json` 供 Agent 消费。精确参数以 `skillstar team --help` 为准。

## 非目标（本切片）

- GUI 页、Tauri IPC、ts-rs DTO。
- 把 notes 推到 GitHub 共享频道或开 PR（teamai 的 contribute/MR 流程）。
- codebase graph / teamwiki。
- 复活 `skillstar-learning` crate 或 ACP 教程。

## 验证

```bash
cargo test -p skillstar-skills team
cargo test -p skillstar-app --lib cli::mode_tests
cargo test -p skillstar-core --lib infra::paths
```

# 06 codex / opencode / pi / omp parser（P2）

usage 线；依赖 05 的 trait 与 checkpoint。每 parser 一个 commit。

## 解锁的契约

四个 parser 落地，覆盖受管 6 agent 的其余全部（codex、opencode、pi、omp）。

## 各 parser 契约要点（magpie 实证，逐条测试）

**codex**（rollout，`$CODEX_HOME/sessions/**/rollout-*.jsonl`）：
- plain 与 `.jsonl.zst` 并存时**只信 plain**（app 压缩到一半双格式同在；magpie sessions.go:393-398 + codex_zst_test.go）；
- `token_count` 事件按累计差分计增量（total 回退/重置时取 last；magpie codex.go:271-300）；
- input 含 cached token 的口径按 codex 自身语义剥离（codex.go:47-53）；
- 依赖：`cargo add ruzstd`（choices C6；纯 Rust decode-only，Windows CI 无 C 工具链风险）。

**opencode**（SQLite，`$XDG_DATA_HOME/opencode/opencode.db`）：
- `hasTable(session_v2)` + kv `migration.v1-v2{phase:completed}` 判别 v1/v2 双表；v1 查询排除已迁移行（magpie opencode.go:202-250）；另有旧版 JSON 文件形态兜底；
- rusqlite 只读打开（先例 gateway agents/cindy.rs）；
- fixture 在测试内建临时 db，绝不 copy 开发者库。

**pi**（`~/.pi/agent/sessions/**/*.jsonl`，jsonc 容忍）：注意 token 口径 input **不含** cache（与 codex 相反——口径矩阵写进模块文档与测试）。

**omp**（`~/.omp/agent/sessions/`，pi 超集）：title 行、model_usage、artifacts 子会话。

## 人能看/跑什么

```bash
cargo test -p skillstar-usage --locked sessions
# codex: 双格式并存只计一次；zst golden；total 回退用例
# opencode: v1 库 / v2 库 / 迁移半程库 三 fixture
```

## 必须保持绿

usage crate 全量；`check_workspace_deps.sh`（ruzstd 归一化进根 Cargo.toml）。

## 委托给实现者的自由

每 parser 的内部状态形状（agent_state JSON）；omp 子会话的展开粒度。

## 检查点（非阻塞）

ruzstd 解压实测失败 → 依 choices C6 降级链（zstd → .zst 不支持 + docs/errors.md 记录）。

# SkillStar Workspace 迁移计划 — Wave 2（继续收敛 crate 数量）

> 状态：**Wave 2A 已实施**（fingerprint→usage，ai→models → 9 crates）。2B 仍可选。  
> 前置：Wave 1（`plan.md`）已完成 — `skillstar-projects` 并入 `skillstar-skills`，当前 **11** 个域 crate 文件夹。  
> 本文件是 Wave 2 的执行计划，不是 SSOT。实施时仍先改 [AGENTS.md](./AGENTS.md) / [docs/backend.md](./docs/backend.md)，再改代码。

## 1. 为什么还是 11 个

Wave 1 的目标是 **错置 seam 修复**（skills/projects），不是「数字最小化」。  
刻意保留的独立 crate 在 Wave 1 §2.2 里写明了非目标：`models+ai`、`usage+fingerprint`、`ssh+sync`。

Wave 2 专门回答：**在 locality / 编译边界可接受的前提下，继续把 11 → 9（推荐）或 8（可选）**。

## 2. 结论（推荐路径）

### 2.1 必做（Wave 2A）→ **9 个文件夹**

| 合并 | 保留 crate 名 | 被吸收 | 理由 |
| --- | --- | --- | --- |
| A | `skillstar-usage` | `skillstar-fingerprint` | 已有单向依赖 `usage → fingerprint`；TLS 指纹是 usage/OAuth 的支撑实现，不是独立产品域 |
| B | `skillstar-models` | `skillstar-ai` | 已有单向依赖 `ai → models`；推理是「Provider 解析 + 调用」的延伸，调用方几乎总是两者一起碰 |

### 2.2 可选（Wave 2B）→ **8 个文件夹**

| 合并 | 建议 | 被吸收 | 门槛 |
| --- | --- | --- | --- |
| C | `skillstar-sync` 吸收 `skillstar-ssh`，或新建 `skillstar-remote` 两者皆 module | 另一侧删除 | **仅当** 出现第三个真实复用方的 progress/credential 抽象，或维护成本证明「远程传输」内聚收益 > 把 `skills` 依赖扇出到 SSH 路径 |

**默认不在 2A 做 C**：ssh 只依赖 core；sync 依赖 skills。硬并会让「只碰 SSH」的编译图带上 skills，deletion test 偏弱。

### 2.3 目标树（2A 完成后）

```text
src-tauri                         # 唯一 skillstar 二进制 + Tauri adapter
└── skillstar-app                 # library-only 跨域 use case + CLI
    ├── skillstar-skills          # 技能库 + agents + projects + patrol + terminal
    ├── skillstar-marketplace
    ├── skillstar-models          # Provider store/tool-sync + AI 推理（原 ai）
    ├── skillstar-usage           # 订阅配额 + TLS fingerprint（原 fingerprint）
    ├── skillstar-ssh             # （2B 前仍独立）
    └── skillstar-sync
         ├── skillstar-core
         └── skillstar-providers  # 零依赖 leaf，永不并入
```

**禁止并入 `skillstar-providers`**：它是 identity/balance 的零依赖 SSOT，被 models 与 usage 两侧 guard 测试钉死；合并会破坏「叶子无依赖」不变量。

**仍不默认做**：`skillstar-mcp`（沿用 Wave 1 Phase 6 决策门）。

## 3. 依赖与规模（现状快照）

| Crate | 约行数 | 当前依赖（域） | 生产消费者 |
| --- | ---: | --- | --- |
| models | ~16k | core, providers | ai, app, src-tauri |
| ai | ~5k | core, models | src-tauri |
| usage | ~10k | core, fingerprint, providers | app, src-tauri |
| fingerprint | ~2.6k | （无 skillstar 域依赖） | usage, src-tauri |
| ssh | ~4k | core | sync(?)/src-tauri |
| sync | ~1.6k | core, skills | src-tauri |

允许方向（2A 后）：

```text
skillstar-models  (含 ai)  -> skillstar-providers
skillstar-usage   (含 fp)  -> skillstar-providers
skillstar-sync             -> skillstar-skills
skillstar-app              -> 跨域 crate
域 crate                   -> skillstar-core
src-tauri                  -> app / 域 facade
```

禁止：

```text
usage  -X-> models
models -X-> usage
skills -X-> marketplace
core   -X-> 任意域 crate
```

## 4. 合并 A：fingerprint → usage

### 4.1 物理落点

```text
crates/skillstar-usage/src/
├── fingerprint/          # 原 skillstar-fingerprint 源码（默认私有或 pub 窄 re-export）
│   ├── client.rs
│   ├── ide_projector.rs
│   └── ...
├── fetchers/
├── catalog/
└── lib.rs                # 对外：订阅/配额 facade；fingerprint 仅在需要处 pub use
```

### 4.2 Feature

- `impersonate` 继续挂在 **usage**（转发到内部 fingerprint 的 wreq）。
- **default = []**（与 Wave 1 护栏一致）。
- **binary root（src-tauri）** 显式 `skillstar-usage = { features = ["impersonate"] }`。
- 删除独立 `skillstar-fingerprint` 后，src-tauri **不再**直接依赖 fingerprint 包（除非仍需 poc bin — poc 迁到 usage 的 `[[bin]]` + `required-features`，或删掉）。

### 4.3 调用方

| 旧 | 新 |
| --- | --- |
| `skillstar_fingerprint::*` | `skillstar_usage::fingerprint::*`（或更窄路径） |
| `Cargo.toml` path dep fingerprint | 删除；只留 usage |

### 4.4 退出条件

- [x] 无 `crates/skillstar-fingerprint/`
- [x] 无生产 `skillstar_fingerprint::` 引用
- [x] `check_workspace_deps.sh`：fingerprint 包不存在；usage default 不含 impersonate；src-tauri 仍 opt-in
- [ ] `cargo test -p skillstar-usage` + workspace test 绿

## 5. 合并 B：ai → models

### 5.1 物理落点

```text
crates/skillstar-models/src/
├── providers/            # 既有
├── tool_sync/            # 既有
├── mcp/                  # 既有
├── ai/                   # 原 skillstar-ai（推理、translate、summarize、skill pick）
└── lib.rs
```

### 5.2 Feature 策略（控制编译扇出）

合并后 models 变重。用 feature 避免「只做 tool-sync 的测试」强行链 HTTP/推理栈：

```toml
# skillstar-models/Cargo.toml（示意）
[features]
default = []
inference = []   # 打开 ai/ 模块与其额外依赖（若有）
```

- src-tauri / 需要 AI 命令的路径：`skillstar-models = { features = ["inference"] }`（或 default 含 inference 若几乎总开）。
- **推荐默认**：`default = ["inference"]` 对桌面应用更简单；若 CI 想轻量，再拆。  
  以「实现时 `cargo tree -p skillstar` 可解释」为准，写入 AGENTS.md。

### 5.3 调用方

| 旧 | 新 |
| --- | --- |
| `skillstar_ai::*` | `skillstar_models::ai::*` |
| src-tauri `commands/ai/*` | 改 import，逻辑仍薄 |
| app 若将来碰 AI | 只依赖 models |

### 5.4 退出条件

- [x] 无 `crates/skillstar-ai/`
- [x] 无生产 `skillstar_ai::` 引用
- [ ] providers 仍为零依赖 leaf
- [ ] AI 流式事件名 / command 名不变
- [ ] workspace test 绿

## 6. 可选合并 C：ssh ↔ sync（2B）

**仅当满足至少两条再开 PR：**

1. 已有稳定的共享 progress / credential / host-key 抽象，且 ≥2 个真实调用方（不仅是「看起来像」）。
2. 产品上把「远程技能 + 云同步」当成同一设置域，UI/命令也准备一起改文档。
3. 合并后能通过 deletion test：删掉合并 crate 会把复杂度散回多个调用方，而不是改 import。

建议命名（二选一，实施前定一种）：

- **C1**：`skillstar-sync` 内 `mod ssh` + `mod s3`（名称略偏 S3，但改名噪音大）
- **C2**：新建 `skillstar-remote`，ssh + s3 均为私有模块，再删两个旧 crate（一次改名，干净）

**依赖约束**：remote/sync 可以依赖 skills（装包）；**不得** skills → remote。

## 7. 分阶段实施

每阶段独立可编译 PR；不夹带产品行为；先 SSOT 后代码。

### Phase 0 — 文档冻结

- [ ] 更新 AGENTS.md Workspace Crates 表：去掉 fingerprint/ai（及可选 ssh）行；扩写 usage/models 职责
- [ ] 更新 docs/backend.md 所有权句（fingerprint 归 usage；AI 归 models）
- [ ] 基线：`cargo test --workspace --locked`、`bun run lint`、`bun run test` 记入 scratch
- [ ] 扩展 `scripts/internal/check_workspace_deps.sh` 禁止图：
  - 不得再出现 `skillstar-fingerprint` / `skillstar-ai` package（2A 完成后）
  - 保持 `usage -X-> models`、`providers` 零依赖

### Phase 1 — fingerprint → usage

- [ ] git mv 源码进 `usage/src/fingerprint/`
- [ ] 改所有 import + Cargo.toml；删 fingerprint crate
- [ ] poc bins 迁 usage 或删除
- [ ] feature 护栏单测/脚本
- [ ] `cargo test -p skillstar-usage --locked` + workspace

### Phase 2 — ai → models

- [ ] git mv 进 `models/src/ai/`
- [ ] 定 inference feature 策略并写进 AGENTS.md
- [ ] 改 commands/ai、删除 ai crate
- [ ] 流式事件 / IPC 回归（名称不变）
- [ ] workspace test

### Phase 3（可选）— ssh/sync

- [ ] 满足 §6 门槛后再开
- [ ] 选 C1 或 C2；更新 SSOT；删旧 crate

### Phase 4 — 收尾

- [ ] AGENTS.md 项目树与 crate 表与磁盘一致
- [ ] §8 验证矩阵全绿
- [ ] 本文件 checklist 勾完；`plan.md` 可链到 Wave 2

## 8. 验证矩阵

每阶段必跑：

```bash
cargo check --workspace --locked
cargo test --workspace --locked
bun run lint
bun run test
bash scripts/internal/check_workspace_deps.sh
cargo tree -p skillstar -e features | head
```

行为回归焦点：

- Usage：OAuth/API key 刷新、CLI switch、fingerprint impersonate 开关路径
- Models/AI：provider CRUD、tool sync、summarize/translate 流式事件
- （若 2B）SSH 推送与 S3 push/pull 互不回归

## 9. 建议提交序列

```text
docs(architecture): define wave2 crate consolidation seams
refactor(usage): absorb skillstar-fingerprint
refactor(models): absorb skillstar-ai
chore(workspace): enforce wave2 package absence in dep guard
# optional:
refactor(remote): merge ssh and s3 sync
```

## 10. 完成定义（Wave 2A）

- [ ] 磁盘上域 crate 文件夹 = **9**（无 fingerprint、无 ai；有 providers/core/skills/marketplace/models/usage/ssh/sync/app）
- [ ] AGENTS.md / backend.md 与树一致
- [ ] providers 仍为零依赖
- [ ] usage/models 的 default feature 策略有护栏
- [ ] IPC / 持久化 / CLI 表面无产品变更
- [ ] 全量 check/test/lint 绿
- [ ] Phase 6 MCP、providers 合并、skills↔marketplace 仍不做

## 11. 明确不做

- 不把 providers 并进 models 或 usage  
- 不把 marketplace 并进 skills  
- 不把 app 并进 src-tauri  
- 不借迁移改 cursor fetcher、usage 产品模型、UI  
- 不在脏产品 WIP 上叠结构 PR（先 stash/分分支）

## 12. 决策记录

| 决策 | 选择 | 原因 |
| --- | --- | --- |
| 2A 合并对 | usage←fp，models←ai | 已有单向依赖；概念内聚；删除 test 成立 |
| 保留 providers leaf | 是 | identity SSOT + 双侧 guard |
| ssh+sync | 默认 2B 可选 | 依赖不对称（skills） |
| 目标数量 | 9（可选 8） | 可解释、可回滚、编译边界清晰 |

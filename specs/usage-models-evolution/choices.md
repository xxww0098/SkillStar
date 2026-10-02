# choices.md — 草稿分歧裁决记录

三份独立草稿（A 最少切片=6 / B 风险优先=17 子片 / C 接缝质量=13）各自侦查后合并。收敛事实直接采纳；分歧裁决如下，实现者不重开。

## C1 「生产网关无 upstream」如何影响排序（三草稿共同发现）

route/sign 未接线（serve.rs:105-123 恒 None）意味着 P4 是首次建造。裁决：P4 拆成两片——10 接线（纯建造，fake upstream 验证）、11 401 自愈（行为叠加）；10 与 11 之间是天然人工检查点（真实流量开始流动）。P1 账本不因「暂无真实流量」推迟：测试用 fake upstream（tests/serve.rs 既有模式）验证，接线后自然有数据。

## C2 鉴权模型：全量严格校验 vs magpie LAN 模式

- A/C：入站（除 claude callback）全部验 key，PLACEHOLDER_BEARER 删除，十余处 writer 改写。
- B：loopback 放行任意 token（magpie lan.go:20-27 模式），仅非 loopback 强制 key。
- **裁决：B。** 第一性理由：key 文件 `config_dir()/gateway.key` 是 0600 用户可读——同用户的本机攻击者直接读 key 即可绕过，严格校验 loopback 的真实增益≈0；而它把四条无辜通道炸进爆炸半径：`skillstar-<agent>` 归因 bearer（rules.rs:176-180）、`skillstar/<model_ref>` 模型选择值（codex.rs:187、body.rs:19）、omp `auth: none`（body.rs:308-329，无 key 字段）、本地 GET 面。非 loopback（LAN、WSL NAT）的攻击者是不同用户，key 是真防线——P0-1（LAN 无鉴权）由此修复。
- 附带收益：PLACEHOLDER_BEARER 保留，树 spec D3「pub use 逐字不变」无冲突（草稿 C 的 CP-1 消解）。

## C3 切片数：6 vs 17 vs 13

- A 的合并逻辑（同 phase 同风险同验证栈）用于 P2 内部：5 个 parser 不按 agent 拆片，按「地基+claude」与「其余四个」两片。
- B 的风险分界用于 P4（接线/自愈分片）与 P0/P1 内部（key 与账本分开、append 与读面分开——后者因 ⛩tree-07 前置被迫分离，正好合理）。
- 终局 13 片；每片独立验证命令族 + 可单独 revert。

## C4 会话归因：apply_gateway 注入 vs 原生头优先

用户拍板「网关下发会话标识」。但侦查发现 affinity.rs:28-35 的 `session_id()` 已实现 X-Skillstar-Session → 原生头（x-claude-code-session-id / x-opencode-session / session_id 等 magpie 同款清单）→ body 派生的三级回退，只是 dispatch 没提取 session 头。
**裁决：管道优先。** 切片 03 先补 dispatch 的 session 头提取（零写盘改动，claude/codex/opencode 原生可用）；apply_gateway 注入仅作为**实测无原生头 agent**（pi/omp 候补）的补充手段，注入不了就留空，由切片 07 的会话文件交叉归因兜底。拍板意图（账本有会话维度）被完整保留，实现路径从「改 6 个 writer」收缩为「改 1 个 dispatch + 按需注入」。

## C5 受管第 6 agent（草稿 C 猜 grok，错）

AGENT_SPECS 钉死的 6 个（skillstar-models/tool_sync/agents.rs:332-348）：claude-code、claude-desktop、codex、opencode、pi、omp。claude-desktop 与 claude-code 共用 projects JSONL 解析家族（magpie calls.go:22,440 先例），路径若不同则独立 discovery、共享 parser。zcode 出范围（口径校准成本，树 spec 开放问题 2 已载）。

## C6 zstd 依赖选择

workspace 无 zstd。zstd-sys 需 C 工具链（Windows CI/npm 链风险）；本场景只需**解压**。裁决：`cargo add ruzstd`（纯 Rust decode-only）优先；不行再 zstd；再不行 .zst 首期不支持（记 docs/errors.md，plain rollout 已覆盖绝大多数会话）。

## C7 checkpoint 设计：磁盘 offset vs 内存 memo

草稿 B 引 filememo 说 magpie「无磁盘 offset」——那是 logins.json 的防抖，sessions 的增量 state **确实落盘**（magpie sessions.go:95-125 Head/ContentHash/Size/Off + saveCache 到 sessions.json，草稿 C 已核）。裁决：采用 magpie sessions state 形状（version + head_hash 判替换 + prefix 采样哈希 + size/offset + per-parser 私有 JSON），落 `data_root()/sessions/index.json`，atomic_write。截断/替换检测（head hash 不符→全量重读）是第一类测试。

## C8 prices 键落点（草稿 C 的 CP-3）

`model_gateway.json` 顶层 `prices` 键、经树 spec store lens 读写。理由：与 routing/affinity/listen 同文件有先例；独立文件制造第二个 config 真相与第二套原子写。前置 tree-03；向 store/doc.rs 提 typed 字段而非绕开（本 spec 是 store 的第一个外部消费者，验证树 spec 的扩展性承诺）。

## C9 账本位置

草稿 A 提议 config_dir()/usage/（与 usage snapshot 同域）；B/C 提议 data_root()/gateway/。裁决：**data_root()/gateway/usage.jsonl**——账本是网关的派生数据（可丢弃可重建），不是用户配置；usage/config 目录属用户可备份配置。轮转下限强制（README D10）。

## C10 AllowanceSnapshot 口径改动的时机

草稿 B 警告「若成本依赖跨 provider 可比则提前」。裁决：不提前——成本（08/09）按 catalog 分组天然可比，无需跨 provider 排序；口径收敛（12）保持 P5，与交叉视图（13）同期收口。rest.rs:44-49 已有的 renews 旁路注入在 12 里统一进 `AllowanceSnapshot{percent, renews_at}`。

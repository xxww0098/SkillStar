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

## S1 切片 01 落地时的实现裁决（banked 2026-10-02）

- **无路径白名单**：非 loopback 对所有非 callback 请求强制 key（`GET /`、`/api/hello`、`/v1/models` 一并拦）。切片给的自由度里选了 magpie lanGuard 同款全拦——最简模型，无白名单漏洞面。验证方（另一台机）也必须带 key，属预期。
- **`gateway_key() -> io::Result<String>`**：切片草图的 `-> String` 与「失败即拒绝」的已定决策矛盾，改 Result；serve/save_listen 各自映射为自己的变体。
- **key 写入照抄 redact::write_key 先例**（OpenOptions mode 0600 + set_permissions，非 temp+rename）。切片说「0600 原子写」但援引的先例本身非原子；按先例实现，崩溃留下的最长风险是一个截断文件——读回时 <32 字符视为无效会自动重新生成，语义自愈。
- **无进程级 key 缓存**：每次非环回请求读一次 key 文件（loopback 根本不读）。换取测试沙箱可隔离（OnceLock 会串沙箱），LAN 流量低下成本可忽略。
- **常数时间比较**（长度差折进同一累加器）与 **精确 `Bearer ` 前缀剥离**（大小写敏感，magpie callerKey 同款）。
- **读回校验**：trim 后 <32 字符视为无效（生成侧 64 hex）；`check_inbound` 只读不建，缺失即拒。
- **WSL NAT bearer 收敛在 codex.rs**：`apply_codex_full` 按 origin 是否 loopback 决定占位/实际 key，key 写不出则整次接管失败（不写一个过不了门禁的配置）。wsl.rs 只传 origin。
- **注释语言**：仓库代码注释一律英文（中文只在 Display/UI 字符串），切片 01 的中文注释在集成时已归一。
- **omp 检查点关闭**：LAN 下不可用已按 D2 默认接受，写进 models README 与 D-078 承担段。

## S5 切片 05 落地时的实现裁决（banked 2026-10-02）

- **read_calls 返回全量视图**（每次经 `replay` 从 checkpoint 重建 + 跨文件 msg-id 去重），不是累计增量流；消费方（切片 07 合并）可幂等整体替换。`parse` 返回 delta，delta 自身按 msg id 幂等。
- **trait 增补**：`PARSER_VERSION` 关联常量（checkpoint 全量重读的判据）与 `replay(checkpoint)` 方法（全量重建通道；`SessionCall` 无 msg 字段，msg id 经 replay 的返回对传递）。trait 带关联常量不可 dyn → 对象安全方法面 `SessionParserMethods` + blanket impl 分发，注册表在 `mod.rs::parsers()` 一处。
- **checkpoint 指纹编码**：`head_hash = v1:<长度>:<hex>`（长度参与编码：短文件增长后头窗口变长，按记录长度截断比较）；`prefix_hash = sample-v1:<hex>`（≤64KiB 全量，否则 16×4KiB 均匀采样，magpie prefixHash 先例）。索引单文件 `data_root()/sessions/index.json`，store version + 每 parser version 双层失效。
- **claude-desktop 归因走 entrypoint 前缀**：本机实测 Desktop 内嵌 Claude Code 会话在 `Claude-3p/title-gen/...` 且行内 `entrypoint: "claude-desktop-3p"`（magpie 未记录的后缀），归因用 `starts_with("claude-desktop")` 不依赖目录；Cowork glob 本机不命中但 discovery 保留。Desktop 主 Code 标签正文文件本机未找到——已知未知，不阻塞。
- **未增长且头一致时不打开正文**（同长度替换被 head 校验拦下走全量）；正在写的半行不消费、offset 不推进。
- **无 message id 的行不进 msgs map**（独立计数 calls_seen，不参与跨文件去重）；usage 缺失/全零的 assistant 行不算调用；`model_asked` 无 identity 信息时回落 `model_answered`（均 magpie 先例，测试钉死）。

## S2 切片 02 落地时的实现裁决（banked 2026-10-02）

- **facade 形状**：自由函数 `signing_material` + 子模块 `usage_switch/signing.rs`（主文件已 717 行，避免逼近 800）。
- **无 CLI target 的 catalog（含 IDE adapter）直接行回退 + Row**：没有 live 文件可以 disagreement，行即唯一真相；同时避开 IDE adapter reconcile 的读修复副作用。
- **Live/Diverged 材料从 authoritative root 提取**（target.external_root 优先、否则 live 文件），而非 snapshot——CLI rename 顶掉链接并轮换 token 的场景下 live 才是 CLI 真正在发的 token。authoritative_root 判定按 target trait 在 signing.rs 内重现（custody.rs 当时在别人文件集；后续如动 custody 可考虑上提）。
- **probe 出错（AmbiguousOwner 等）降级行回退 + tracing warn**，与 reconcile_cli_accounts 的降级模式一致。
- **Row 态 subscription_id=Some(行 id)**（账本归因用），仅 Diverged 恒 None；freshness/subscription_id 不进 AccountSnapshot——后续账本切片需要归因时读 signing_material 而非快照。
- **Custody::probe 纯读实测**：无副作用（open 只拼路径、keychain 只读查询、read_dir 容缺失），无需走 reconcile 轻量子集；「不建 CLI 家目录」纪律已有测试钉死。

## S3 切片 03 落地时的实现裁决（banked 2026-10-02）

- **note_turn 签名**：`note_turn(&TurnFacts, status, body, upstream_raw)`——响应侧字段作参数而非塞进 facts（forward_turn 局部响应体的生命周期短于请求侧借用，单生命周期 struct 装不下）。
- **Turn 带上游原始字节**（`upstream: Option<Bytes>`，回译前保留）：Anthropic 入站被 translate 重建 stream、回译失败 502 时 agent 无回复但 token 已耗——账本从 raw 字节记账（有测试钉死）；该字段 None 同时是 classify 的 `local` 判据。
- **轮转**：按大小 5MB，活文件 rename 到 `usage.<n>.jsonl` 最低空位；load 按编号序+活文件读完整行（torn tail 丢弃）。进程内 static Mutex 与 O_APPEND 双保险防并发交错。
- **account 语义（未接线世界）**：四槽位第一个非空 key；`skillstar`/`skillstar-<agent>`/`skillstar/<model>` 通道记空，真 key 记 `key:<sha256 前 8 hex>`；切片 10 后由胜选 candidate 的订阅 id 替换。
- **ErrorKind::Quota/Verify 暂不产出**（区分需 rest.rs 私有词表，不在文件集）；schema 一次定形含全部 8 变体，切片 10 状态机细化。
- **gateway 新增依赖 tracing + sha2**（cargo add，workspace 归一）：防火墙 1 要求 tracing warn 而 gateway 原无任何日志手段；指纹用 sha256（custody orphan_id 先例）。
- **内存环每 turn 都喂**（UI 尾缓存连续），append 失败时 warn + 环即降级面。
- **Anthropic 回译 SSE 仍是 502**（既有 bug 未修，需 translate.rs SSE 聚合，不属本片）；本片保证该场景 usage 不丢。

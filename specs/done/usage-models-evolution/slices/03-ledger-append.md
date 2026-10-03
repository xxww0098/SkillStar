# 03 账本 append 热路径（P1 核心）

## 解锁的契约

每次完成的转发 turn 追加一行 JSONL 到 `data_root()/gateway/usage.jsonl`（README D10）；append 失败绝不打断 turn（防火墙 1）；usage 提取同时支持 JSON body 与 SSE 尾帧（translate.rs:5 声明 Anthropic 入站会被重建成 stream 请求——现状 trace.rs:86-93 只解析 JSON，SSE 下 completion 恒空）。

## Record schema（一次定形，硬切）

```rust
// crates/skillstar-gateway/src/ledger/record.rs
pub struct Record {
    pub at: i64,                  // unix millis，turn 开始
    pub agent: String,            // rules::request_agent 归一化 id
    pub session: String,          // affinity::session_id 三级回退；空 = 未知
    pub model_asked: String,      // 入站 body.model
    pub model_answered: String,   // 响应命名的模型；空 = 未命名
    pub catalog: String,          // 胜选 candidate 的 catalog_id（未接线前空）
    pub account: String,          // 订阅 id 或 "key:<sha256 前 8 hex>"；空 = 未知
    pub tokens: TokenCounts,      // input/output/cache_read/cache_write/reasoning，缺失即 0
    pub status: u16,
    pub latency_ms: u64,
    pub error_kind: Option<ErrorKind>,  // RateLimit|Quota|Credit|Verify|Auth|Upstream|BadBody|Rejected
    pub endpoint: String,         // 入站 path
}
```

secret 红线：无任何 key/token 字段；`account` 只落指纹（先例 custody.rs:668-677）。

## 接缝

- `ledger/{record,append}.rs`：`append(record: &Record)`（O_APPEND 单行单 write，失败吞错 + tracing warn + 降级喂内存环）、`load(since)` 只收完整行、轮转下限强制。
- 挂点：serve.rs:338 `forward_turn` 的 `note_forward` 同位延展为 `note_turn(&TurnFacts)`；trace.rs 内存环保留为 UI 尾缓存与 append 失败降级目标。
- **session 头管道**（README D4）：dispatch 补提取 session 头传入 forward_turn——复用 `affinity.rs` 的 `session_id()` 逻辑（需从 affinity 导出或新增 `session_from_headers`），原生头（x-claude-code-session-id / x-opencode-session / session_id 等）优先，X-Skillstar-Session 次之，body 派生兜底。
- RecentCall DTO 不动（读面换源在切片 04）。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
# tests/serve.rs fake upstream：200/4xx/502 三种 turn 各落一行；SSE 尾帧 usage golden
# 只读目录注入：append 失败、turn 不失败
# 双线程并发 append：无交错半行
```

## 必须保持绿

trace.rs 全部测试（含 `note_forward_drops_the_bearer_secret`——给 ledger 复制同款 Debug 断言）；tests/serve.rs、tests/surface.rs 既有断言。

## 委托给实现者的自由

JSONL 键风格（snake_case）；轮转策略（大小上限或按月分文件）；TurnFacts 与 Record 的装配位置。

## 会改变本片的反馈

用户若要账本同时记流式中间事件（首 token 延迟 TTFT）→ Record 加 `ttft_ms: Option<u64>` 一次定形，不做第二套流事件文件。

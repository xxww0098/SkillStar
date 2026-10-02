# 01 gateway key 与 LAN 门禁（P0）

## 解锁的契约

非 loopback 的入站请求（LAN、WSL NAT 形态）必须携带安装级 gateway key，否则 401；loopback 行为零变化（归因/选择通道、omp、本地 GET 面、claude callback 全不动）。这是「门先于线」的防火墙 6 前提。

## API 接缝

```rust
// crates/skillstar-gateway/src/access.rs（新私有 module，lib.rs 窄导出）
pub fn gateway_key() -> String;   // 惰性生成：config_dir()/gateway.key，0600 原子写
                                 // （先例 redact.key，redact/mod.rs:724），≥32 字节随机
/// 非 loopback 校验：Authorization Bearer / x-api-key / x-goog-api-key / ?key= 任一槽
/// 匹配即过（magpie internal/gateway/lan.go:255-265 模式）。loopback 由调用方跳过。
pub fn check_inbound(peer: &SocketAddr, authorization: &str, api_key: &str, goog_key: &str, query_key: &str) -> bool;
```

- 挂点：`dispatch()`（serve.rs:244）在 claude callback 分支（serve.rs:251-259，自有 token + loopback 双门，**不动**）之后、`surface::plan` 之前；`peer: SocketAddr` 已传入（serve.rs:247）。
- LAN 门禁双保险：`save_listen("lan")` 在 key 不可生成/不可读时拒绝（`SaveListenError` 新变体，过 `check_error_strings.sh`）；`serve()` 对 `listen_is_lan()` 且 key 不可读返回新 `ServeError::LanNeedsKey`。
- WSL NAT：codex 的 WSL writer 需要带 key（NAT 形态 peer 非 loopback）——`tests/wsl_codex.rs` 改断言；mirrored 形态用 127.0.0.1 不受影响。
- `PLACEHOLDER_BEARER`、`token_for`、`named_bearer`、本地 GET 面全部不动（README D2）。
- 依赖：`cargo add rand --workspace`（gateway 未引，workspace 已有）。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
# 实机：开 LAN 后另一台机无 key curl /v1/models 得 401、带 key 得 200
```

## 必须保持绿

tests/lan.rs（4）、tests/serve.rs、tests/surface.rs、tests/rules.rs 的 request_agent 族、tests/apply_gateway.rs、tests/codex_writer.rs、`startup_agent_files`（启动不写 agent 文件）。

## 委托给实现者的自由

key 字符集与长度下限之上的细节；401 响应体文案；`GET /` 与 `/api/hello` 是否对非 loopback 放行（建议放行——无敏感数据，`/v1/models` 必须拦）。

## 人工检查点（非阻塞）

omp 在 LAN 模式下不可用（`auth:none` 无法带 key）——确认可接受后写进 models README 与 decisions.md（「omp 仅 loopback」）；若不可接受，回 choices C2 重议。

## 文档（同一提交）

docs/architecture.md:142-146（bearer 系列声明加 key 段）、docs/features/models/README.md LAN 段、docs/decisions.md（D2 模型及理由）、docs/errors.md（P0 根因：占位 bearer + 0.0.0.0）。

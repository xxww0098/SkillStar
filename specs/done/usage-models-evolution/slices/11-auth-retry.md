# 11 401 自愈（P4 后半）

⛩ 前置：tree-09 + 切片 10。安全敏感片。

## 解锁的契约

上游 401 时：进该 catalog 的 serialization domain → adopt（吸收 CLI 轮换的新 token）→ 刷新 → 重签重试**恰好一次**；仍 401 透传给 agent 并落账本 + 该候选进 auth rest。三限（README D9）：仅 401、仅一次、仅未 committed。

## 接缝

```rust
// gateway/src/sign.rs —— trait 扩展（默认实现 None，测试假 book 免改）
pub trait AccountBook {
    /* 既有两个方法不动 */
    /// 401 自愈钩子：app 实现 = reconcile 三态消费 + adopt + refresh + 回写；
    /// 返回新材料供本 turn 重签重发一次；None = 放弃，透传 401。
    fn reauthorize(&self, catalog_id: &str) -> Option<AccountSnapshot> { None }
}
// forward.rs 的 turn 编排：
struct TurnAuth { retried: bool }          // 栈变量，循环在结构上不可能
// 401 && !retried && !committed → reauthorize → Some 则 retried=true 重发一次
// gateway 侧永远只调钩子；adopt/refresh/锁全在 app 实现里
```

- app 侧 `reauthorize` 实现锁序：**必须复用** `refresh_guard` 的 catalog lock + CLI refresh lease + `adopt_active_cli_session_before_refresh` → refresh → `sync_refreshed_active_subscription`（service.rs:363-400 是唯一模板；tree-09 后落在拆出的子模块）——禁止新造锁（防火墙 7，历史故障：后台刷新踢掉 Codex CLI）。
- `UpstreamFailure` 加 `Auth` 变体 → `AUTH_REST`（建议 30 分钟，与 VERIFY_REST 同档）；rest.rs:38-50 现无 auth 类。
- 403/429/5xx 永不触发（`request.rs is_auth_error()` 仅 401 是既有裁决）。
- **跨 runtime 探针**：网关 serve 自建 runtime（serve.rs:181-186）调 usage 异步锁——先写探针测试；死锁/长阻塞则降级为「落自愈请求到共享队列、当次 turn 透传 401、下一次调用恢复」（README 已知未知 #2）。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
# fake upstream 第一次 401 第二次 200：自愈成功；401→401：恰一次重试；403：不自愈；committed：不重试；锁被占：超时退避透传
cargo test -p skillstar-app --locked
# 人工（灰度）：人为使 codex token 过期，观察一次 adopt+重试；确认 CLI 不被踢下线
```

## 必须保持绿

sign_ 族（默认实现不破坏假 book）；custody/refresh 既有测试；10 的接线测试。

## 委托给实现者的自由

AUTH_REST 时长；自愈结果是否发 usage 事件（若发，事件名进 frontend README 事件表）。

## 文档（同一提交）

docs/features/usage/README.md:15（网关读凭证的通道声明改写）、docs/errors.md（自愈路径与降级语义）。

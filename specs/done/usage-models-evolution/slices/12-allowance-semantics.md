# 12 AllowanceSnapshot 口径收敛（P5 前半）

⛩ 前置：tree-04（route.rs 已进 route/）。

## 解锁的契约

消灭两处分叉的余量口径：`AllowanceSnapshot { used: f64 }`（语义漂移——字段名是 used，装的是 percent，route.rs:19-22）与 rest.rs:44-49 的独立 `renews` 旁路注入，统一为 `{ percent, renews_at }`：

- **percent**：同 provider 窗口内百分比，语义钉死「跨 provider 不可比」——route_smart/usage_order 只用它做同 provider 内排序；
- **renews_at**：重置时间——rest 坐窗条件（used≥98 且 renews 在未来，rest.rs:256-264）统一从 snapshot 读，旁路注入删除。

## 接缝与触点（五处，一片内每处一 commit）

1. `route.rs`（tree-04 后在 route/）：类型改名 + 字段拆分；
2. `rest.rs`：UpstreamFailure 的 renews 注入口删除，改读 snapshot；
3. `app/models/account_book.rs`（tree-07 后在 gateway/）：填充值带窗口标签；
4. app 投影 DTO + `types:gen`；
5. 前端 `RoutingControl.tsx`（若显示余量）。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
cargo test -p skillstar-app --locked
bun run types:gen && git diff --exit-code src/types/generated/
```

## 必须保持绿

tests/route.rs（9）、tests/rest.rs 全族——按新口径改写断言（先跑旧断言记录行为，再改写，不盲改）。

## 委托给实现者的自由

percent 的取值口径细节（多窗口时取最紧张窗口——延续 account_book.rs:67-73 的 max 语义但带标签）。

## 会改变本片的反馈

若切片 10 实测发现路由需要跨 provider 可比才能工作（如 failover 场景）→ 本片提前到 10 之前（choices C10 的例外触发）。

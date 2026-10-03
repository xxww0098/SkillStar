# 10 upstream 接线与 turn 状态机（P4 前半，全 spec 最大工程片）

⛩ 前置：tree-04 + tree-05（route/决策组与 store lens 改道完成，避免与树 spec 手术正面相撞）。防火墙 6：**禁止先于切片 01 合并**。

## 解锁的契约

生产 dispatch 首次真正编排：路由（route_smart/候选展开）→ 上游解析（provider endpoint）→ 签名（sign_upstream + 02 的 live-first book）→ 转发 → 记账（03 的 ledger）。之前所有「已测试未接线」的纯函数进入运行路径。

## 接缝

```rust
// crates/skillstar-gateway/src/forward.rs（新；serve.rs 723 行预警带，编排抽出）
/// 全部经注入，gateway 不 import usage/models（README D7）
pub struct UpstreamEnv {
    pub resolve: Box<dyn Fn(&ModelRef) -> Vec<RouteCandidate>>,  // app 实现：provider store endpoint + visible/catalog 归属
    pub book: Box<dyn AccountBook>,                              // app 实现：UsageAccountBook（02 已 live-first）
}
// serve dispatch 的 forward_body 路径改经 forward::turn(&env, …)：
//   候选排序（route_smart）→ 逐候选 sign_upstream → send → rest/坐窗既有语义 → ledger::append
// translate/hold/surface 既有协议面不动。
```

- 装配点：`start_desktop_gateway`（app/cli/gateway.rs:53）与 CLI 同路注入 `UpstreamEnv`。
- anthropic 进程桥不受影响（sign.rs:63-67 `bridge:true` 分支照旧）。
- tests/serve.rs 的 fake upstream 全套改走注入 resolve（既有模式扩展）。
- 401 重试**不在本片**（11）；本片 401 = 透传 + 账本 error_kind=Auth。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
cargo test -p skillstar-app --locked
# 端到端：fake provider + fake book，断言签名头、路由顺序、账本行
# 人工（灰度）：真实 agent 走一轮对话，账本出现第一行真实 catalog/account 归因
```

## 必须保持绿

tests/serve.rs / tests/surface.rs / tests/route.rs / tests/rest.rs 全族（纯函数行为在接线后不变）；sign_ 族；`check_workspace_deps.sh`（不新增违规依赖边）。

## 委托给实现者的自由

UpstreamEnv 的具体形状（trait vs 闭包集）；候选循环与 rest/坐窗的复用方式（rest.rs 的 UpstreamFailure 注入口既有）。

## 人工检查点（非阻塞）

**接线完成即真实暴露面成立**——确认切片 01 已在 main、LAN 模式实测 401/200 各一次；这是 10 与 11 之间的天然停点。

## 文档（同一提交）

docs/architecture.md:139（签名通道声明）、docs/features/models/README.md:99/113（「监听转发仍打启动时的那一个上游根」等声明改写）。

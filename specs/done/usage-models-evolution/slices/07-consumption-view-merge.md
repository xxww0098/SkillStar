# 07 ConsumptionView 合并去重（P2/P3 之间，全 spec 最难接缝）

依赖：03 的 Record + 05/06 的 SessionCall 数据形状。归属：app（唯一能同时 import gateway ledger 与 usage sessions 的层，AGENTS「跨域编排进 skillstar-app」）。

## 解锁的契约

账本（网关见过且带归因的调用）⊕ 会话文件（**全部**调用，含 agent 绕过网关直连的）合并成一个消耗视图，同一次调用不双计。

## 去重语义（移植 magpie internal/usage/ledger.go:214-280 `gatewayMatches`）

匹配优先级：

1. `request_id` 相等——主键，先消费；
2. 否则 `(agent, native_session)` + in/out/cache_read/cache_write 四元组完全相等 + 会话文件时间落在 record 结束 ±2s + 失败语义一致（同错或同成功）+ 空成功调用不参与 + 该配对在两侧唯一（一对多即放弃、保持两侧可见）。

一个网关 Record 恰好消费一个 SessionCall；未匹配的会话文件行 = `CallSource::SessionFile`（magpie `Row.Source=="log"`：「网关没见过的调用」），catalog 记 `"session-unknown"`。

## 接缝

```rust
// crates/skillstar-app/src/usage/consumption.rs（新）
pub enum CallSource { Gateway, SessionFile }
pub struct UnifiedCall { /* Record 与 SessionCall 的公共字段投影 + request_id */ }
pub struct ConsumptionRow { pub call: UnifiedCall, pub source: CallSource }
pub struct Window;   // Today | Week | Month | All；网关侧多读 24h 缓冲（跨界 turn）
pub fn gateway_matches(records: &[Record], calls: &[SessionCall]) -> MatchMap;
pub fn consumption_view(records: &[Record], calls: &[SessionCall], window: Window) -> ConsumptionView;
```

纯函数：时钟注入、无 IO——模型 id 的归一（requested/served 的 vendor 前缀、version 尾巴等价，magpie served.go 正则族）作为本片的辅助纯函数先行，golden 测试钉住已知等价对（如 `gpt-5` ↔ `gpt-5-2025-08-07`）。

## 人能看/跑什么

```bash
cargo test -p skillstar-app --locked consumption
# golden 四类样本：纯网关调用 / 纯绕行调用（SessionFile）/ 双侧同 call（log 行被吞）/ 歧义双 call（保留可见）
# ±2s 时钟边界用例
```

## 必须保持绿

app crate 既有测试（新文件，不碰 service.rs——tree-09 前置在 09 才生效）。

## 委托给实现者的自由

MatchMap 内部结构；归一正则的覆盖面（已知等价对清单之外的扩展）。

## 会改变本片的反馈

用户若要「绕行流量也按账号归因」→ SessionFile 行的 catalog 恒 `session-unknown`，账号归因只能到会话级——这是数据本身的边界，不扩。

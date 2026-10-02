# 09 Summarize + 今日消耗（P3 后半）

⛩ 前置：tree-09（app/usage/service.rs 拆分完成后再动 usage 服务面）。依赖 07/08。

## 解锁的契约

读时计价的汇总纯函数 + 「今日消耗」命令与前端呈现：Usage 页新增每订阅/每 provider 的今日 token 与成本（ ConsumptionView 数据，标注「经网关 / 全部」口径）。

## 接缝

```rust
// app/src/usage/consumption.rs（07 的文件内）
pub enum Period { Today, Week, Month, All }
pub enum Dimension { Agent, Model, Account, Session }
pub struct Totals { calls, errors, input, output, cache_read, cache_write, reasoning,
                    cost_usd: f64, unpriced: u64, mean_latency_ms: f64 }
pub struct SummarizeInput<'a> { pub now_ms: i64, pub rows: &'a [ConsumptionRow],
    pub price: &'a dyn Fn(&str, &str) -> Option<ModelCost> }   // 时钟与价格全注入
pub fn summarize(input: &SummarizeInput<'_>, period: Period) -> Summary;
// Summary { totals, series(小时|天|周自动分桶), by: BTreeMap<Dimension, Vec<Group>> }
```

- 命令：`get_consumption_summary(window)` 挂 usage_commands（DTO ts-rs）；`UsageSummary`（dto.rs:366）或新 DTO 带今日字段——落在 tree-09 拆出的服务子模块。
- 前端：`UsageSpendSummary.tsx` 今日行；卡片主体今日消耗行（Operate 纪律：等宽数字大号、成本恒标「估算」、空态「还没有记录」句式、`prefers-reduced-motion`）。
- 读时计价：价格表会变，改价重述历史——语义接受并写进文档（README D8）。

## 人能看/跑什么

```bash
cargo test -p skillstar-app --locked consumption
bun run types:gen && git diff --exit-code src/types/generated/
bun run lint && bun run build && bun run test -- src/features/usage
bash scripts/internal/check_i18n_hardcoded.sh   # 新文案
# 人工：拿已知价目模型（deepseek）对照官方账单核对数量级
```

## 必须保持绿

UsageSpendSummary/UsageGrid 既有前端测试；usage service 既有测试（tree-09 拆分后归位的）。

## 委托给实现者的自由

Group label 拼装；series 分桶粒度；今日行的信息密度（一屏内不得推走配额主视觉——卡片节奏规约延续）。

# 13 交叉视图与文档收口（P5 收尾）

依赖：04/07/09/12 的数据与口径就绪。

## 解锁的契约

把度量面成果接到两个工作台，形成「配额（厂商口径）× 实测消耗（网关+会话口径）× 成本」的三角视图；全部文档声明收口。

## 接缝（DTO 形状）

```rust
// app 侧 ts-rs DTO（落 src/types/generated）
pub struct TodayConsumptionDto { totals, by_agent, chips: Vec<SessionChipDto> }
pub struct SessionChipDto {            // 会话维度入口（usage 卡 → 会话 → agent）
    agent, session, title: Option<String>, last_active,
    tokens, cost_usd: Option<f64>, via_gateway: bool,   // Gateway 来源占比 >0
}
pub struct RouteComparisonDto {        // 同一模型各 catalog 对照（可比路由口径的可视化）
    model: String, candidates: Vec<RouteCostDto>,
}
pub struct RouteCostDto {              // 全部来自账本 Record 聚合，口径一致
    catalog, calls, error_rate, p50_latency_ms, p95_latency_ms,
    tokens, cost_usd: Option<f64>, resting: bool,
}
```

- 命令：`get_today_consumption`、`get_route_comparison`（usage_commands / models_commands 各归各）。
- 前端：Usage 页顶部今日行（09 已有）+ 会话 chip；ModelsHub Gateway 面板候选 chip（`route_smart` 排序 + allowance 首次上 UI）；usage 卡 → 网关候选 → 选择器三角导航（点配额卡跳「这个账号正在服务哪些 agent」）。
- UX 纪律（Operate）：扫描性优先；chip 不新增真相（全部派生视图）；CSS 叠加柱状图（input 底/output 顶，magpie 同款，不引图表库）；等宽数字；成本恒标「估算」；空态「还没有调用」句式；`prefers-reduced-motion`。
- **文档收口（同序列完成，不留待补）**：models/README.md:46/99/105/113/119、usage/README.md:15、architecture.md:128/139/142-146 全部按最终行为改写；decisions.md 记 gateway key（D2 模型）与账本（度量面）两个长期决策。

## 人能看/跑什么

```bash
cargo test -p skillstar-app --locked
bun run types:gen && git diff --exit-code src/types/generated/
bun run lint && bun run build && bun run test
bash scripts/internal/check_feature_imports.sh && bash scripts/internal/check_i18n_hardcoded.sh
# 全量门槛：cargo test --workspace --locked
```

## 人工检查点（非阻塞）

同一模型在三个来源（配额卡 / 账本 / 会话文件）的数字人工对一次账——这是 P5 的验收定义本身；对不上的口径差记进 docs/features/usage/README 的口径矩阵。

## 委托给实现者的自由

chip 与对照卡的信息密度、排序、折叠策略；三角导航的具体路由形态。

## 会改变本片的反馈

用户若要 Tray/Dock 也显示今日消耗 → 另立小切片（dock_usage 纯函数已有先例），不扩本片。

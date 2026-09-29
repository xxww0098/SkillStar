# 03 — 一条路由决定

## 契约

注入一份用量快照后，smart 路由对一组候选给出顺序。用量达到 `usedShare`（98）的候选靠后。没有快照的候选是 unknown，排在有余量的候选后面，但不当成耗尽。本档只有这一种决定。空字符串路由模式留到 09 档。

## 缝

`skillstar-gateway` 的纯函数：候选列表加 `AllowanceSnapshot` 进，排序后的 id 列表出。快照类型定义在这个 crate。测试自己构造快照。不引用 `skillstar-usage` 或 `skillstar-models`。

夹具：`crates/skillstar-gateway/tests/fixtures/magpie/route/smart-one.json`，含候选、快照、期望顺序。

## 人可以运行

```bash
cargo test -p skillstar-gateway route_smart_
```

## 验证

- `route_smart_puts_used_share_behind`
- `route_smart_unknown_snapshot_is_not_exhausted`

## 可改

快照结构里与本档断言无关的字段名。

## 不可改

98 的阈值、unknown 的含义、函数不读磁盘也不发 HTTP。

## 必须保持绿

01、02 的测试。依赖守卫仍然认为 gateway 只依赖 core。

## 会改这一档的反馈

期望顺序和 magpie `routing_test.go` 里对应的 smart 用例不一致。

## 决定

- 本档不计算亲和，不读 `model_gateway.json`。

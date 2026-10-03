# 08 价格源（P3 前半）

⛩ 前置：tree-03（store/doc.rs）+ tree-06（catalog typed parse）。

## 解锁的契约

`effective_price(catalog, model)` 三级生效价：model_gateway.json 顶层 `prices` 键（用户覆盖，经 store lens）> models.dev 缓存 cost > None（unpriced）。本片是树 spec store 的**第一个外部消费者**——验证其扩展性承诺（向 store/doc.rs 提 typed 字段，不绕开）。

## 接缝

```rust
// gateway/src/store/doc.rs 增（树 spec 形状之外的新 typed 字段）：
pub struct PriceRow {
    pub id: String,   // "<catalog>/<model>" 精确 或 "<catalog>" 通配
    pub input: f64, pub output: f64, pub cache_read: f64, pub cache_write: f64,  // USD/Mtok
}
// gateway/src/cost.rs（新）：
pub struct ModelCost { /* 同四元组 */ }
impl ModelCost {
    pub fn cost(&self, t: &TokenCounts) -> f64;   // Σ(token×单价)/1e6，reasoning 已含在 output（magpie catalog.go:83 同式）
}
pub fn effective_price(catalog: &str, model: &str) -> Option<ModelCost>;
```

- 树 spec 06 的 `CatalogModel.cost`（models.dev 的 `cost{input,output,cache_read,cache_write}`）补进 typed parse。
- 显式 0 价是有效价（记 priced）；`prices` 键往返不丢未知键（树 spec D5 flatten 保证，加钉子测试）。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked cost
# 三态：覆盖价 > 目录价 > 缺失；prices 行往返保留
```

## 必须保持绿

树 spec 01 的行为锁矩阵（prices 是新顶层键，flatten 进 rest 前先跑锁确认不吞）；catalog 既有测试。

## 委托给实现者的自由

PriceRow 的 serde 键名；是否支持未来币种字段（预留 `currency: Option<String>` 默认 USD 可接受，不强制）。

## 会改变本片的反馈

用户若要求按上游 relay 折扣价管理多套价格 → PriceRow 加 scope 字段的扩展另立，本片先落地单套。

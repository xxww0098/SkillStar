# 06 catalog typed parse：models.dev 收口

依赖：02（catalog/ 目录已成形）。与 03-05 并行可行（catalog 读的是 models.dev 缓存，不是 model_gateway.json）。

## 解锁的契约

`catalog/` 提供 models.dev api.json 的唯一 typed parse，消灭仓库内三份重复解析（`visible.rs` 的 catalog_serves/catalog_ids、`names.rs:95-110` catalog_lists、`effort.rs` 的 catalog_model/catalog_levels）与 app 侧一份（`app/models/picker.rs:55-77` 自带 `serde_json::Value` 目录解析）。

## API 接缝

```rust
// crates/skillstar-gateway/src/catalog/schema.rs
#[derive(Deserialize)]
pub(crate) struct Catalog {
    #[serde(flatten)]
    providers: BTreeMap<String, CatalogProvider>,
}
#[derive(Deserialize)]
pub(crate) struct CatalogProvider {
    #[serde(default)] models: BTreeMap<String, CatalogModel>,
}
#[derive(Deserialize, Clone)]
pub(crate) struct CatalogModel {
    #[serde(default)] pub id: String,
    #[serde(default)] pub name: String,
    #[serde(default)] pub family: Option<String>,   // catalog 侧 family（与行上的 owner family 是两个概念，见 10）
    #[serde(default)] pub limit: Option<CatalogLimit>,
    #[serde(default)] pub cost: Option<CatalogCost>,
    #[serde(default)] pub reasoning_options: Vec<ReasoningOption>,
}

// catalog/ids.rs —— 单一实现，替换 visible.rs 与 picker.rs 两份
pub fn catalog_ids() -> Vec<String>;                       // "provider/model" 扁平化
// catalog/mod.rs
pub fn serves(provider: &str, model: &str) -> bool;        // 存在性检查，替换三处
pub fn entry(provider: &str, model: &str) -> Option<CatalogModel>;
pub fn effort_values(provider: &str, model: &str) -> Vec<String>;  // reasoning_options[type=effort].values
```

语义保持（现状 silent-skip，不许漏）：

- 空 provider、model id 含 `/`、models 非对象 → 跳过；
- 缓存缺失/损坏 → 空 Catalog（现状 `unwrap_or_default`）；
- `ModelEntry` 只声明消费者要的字段（catalog 是只读缓存不是回写文档，未知字段忽略即可，无 flatten 保留义务）。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
cargo test -p skillstar-app --locked models
grep -c "get(\"models\")" crates/skillstar-gateway/src crates/skillstar-app/src  # 只剩 catalog/
```

## 必须保持绿

- `tests/models_dev.rs`（3，缓存只读、失败沿用）、`tests/visible.rs`（5）、`tests/effort.rs`（8，含 `effort_unknown_model_passes_through`）。
- app 侧 picker：`choices_are_provider_or_group_ids_and_carry_no_secret`（**泄密锁，typed 化后必须仍绿**）、`a_missing_catalog_still_lists_groups`。

## 委托给实现者的自由

- CatalogModel 字段富余度（允许多声明未来要用的：name/family/limit/cost 已含）。
- `effort.rs` 的 `catalog_levels/catalog_model` 内部改写方式。

## 会改变本片的反馈

若 picker 投影需要 catalog 条目的更多字段（如 label 回退用 name）→ 在本片 lens 上加，不另起第四份解析。

# 03 store/doc.rs：typed schema owner

依赖：02（store/ 目录已成形）。

## 解锁的契约

`ModelGatewayDoc` 成为 `model_gateway.json` 的唯一 typed 视图：已知顶层键与 providers/groups 行的共享字段 typed 承载，未知键整段保留。本片**零迁移**——现有读写方不动，只证明容器正确。04/05 才把读写方改道过来。

## API 接缝（签名级契约）

```rust
// crates/skillstar-gateway/src/store/doc.rs
pub(crate) struct ModelGatewayDoc {
    providers: Vec<OwnerRow>,
    groups: Vec<OwnerRow>,
    model_names: BTreeMap<String, String>,
    model_efforts: BTreeMap<String, Vec<String>>,
    profiles: Vec<ProfileRow>,
    listen: Option<String>,
    visible: BTreeMap<String, Vec<String>>,
    /// 本 crate 不解释的顶层键（redact_*、vision、手编键）：整段保留。
    #[serde(flatten)]
    rest: BTreeMap<String, Value>,
}

/// providers 与 groups 共用的行形状。行内未知键（note、auto、classifier、
/// rules、未来的 family 扩展）进 extra 原样回写。
pub(crate) struct OwnerRow {
    pub id: String,
    pub members: Vec<String>,       // 仅 groups 有意义；providers 恒空
    pub routing: Option<String>,
    pub affinity: Option<String>,
    pub family: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl ModelGatewayDoc {
    /// 缺文件 = 空 doc（不创建）；存在但非 JSON object = Err。写路径用。
    pub(crate) fn open() -> Result<Self, DocStoreError>;
    /// 坏文件 = 空 doc（默认值）。读路径用，对齐现状 read_doc 语义。
    pub(crate) fn open_lenient() -> Self;
    /// to_vec_pretty + atomic_write，一次全量原子替换。
    pub(crate) fn save(&self) -> Result<(), DocStoreError>;
}
```

护栏（README D5、choices C7）：

- 全字段 `#[serde(default)]`，无 `deny_unknown_fields`，序列化不 skip——「Smart/Auto = 删键」语义由 lens setter 负责。
- 行查找一律 first-match-by-id；重复 id 行、无 id 行原样保留不丢弃。
- `model_efforts`/`visible` 无代码写方，只进 typed 读视图，本片不提供 setter。

## 新测试（tests/store_doc.rs，对齐可证伪命名）

- `a_typed_round_trip_sorts_keys_the_way_the_value_path_did` —— 同一含手编键的 fixture 经旧 Value 路径与新 typed 路径各写一次，parse-back 相等。
- `a_missing_file_reads_as_defaults_and_is_not_created`（与 01 的同名测试互补：本片锁 doc 容器行为）。
- `a_broken_file_refuses_open_but_lenient_reads_defaults`。
- `owner_rows_keep_duplicate_and_missing_ids_as_they_are`。

fixture 复用 01 的 `tests/fixtures/gateway/handwritten.json`。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked store_doc
```

## 必须保持绿

01 全套（**旧读写方还在跑旧路径**，gateway_roundtrip 必须原样绿）；`cargo check --workspace --locked`；clippy ratchet。

## 委托给实现者的自由

- `DocStoreError` 的变体命名与 Display 文案（过 `check_error_strings.sh` 门禁即可）。
- lens 函数本片不建（04/05 按需加），`store/mod.rs` 是否只 re-export doc。

## 人工检查点（非阻塞）

schema 形状是全阶梯唯一不可逆决策（一旦 04/05 改道完成，行内边界再动就是二次迁移）。打开 `store/doc.rs` 让用户过目 OwnerRow 的 typed 字段集与 `rest`/`extra` 边界；用户沉默则按本契约继续。

## 文档（同一提交）

- `docs/decisions.md` 新条目：schema-owner 决策（含「为什么 family 落在 gateway 的 store/ 而不是 models crate」与「仍非事务」声明 D7）。
- `store/mod.rs` 模块文档写明：唯一 open 点（04/05 完成后）、豁免清单为空、非事务。

## 会改变本片的反馈

用户若要求 store 顺手提供进程内锁（消灭并发写窗口）→ D7 作废需重立决策；本片拒绝夹带，锁是行为变化。

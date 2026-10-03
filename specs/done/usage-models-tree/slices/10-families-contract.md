# 10 families 落点契约（只写契约，不实现）

随时可做；实现时另立 spec 引用本文件，签名零决策。

## 契约目的

family 一等化实施时**只动四个文件**，不触碰 route/、catalog/、app/ 任何代码——这是整条阶梯把 visible 数据面提前压进 store/ 的全部回报。历史契约背景：`specs/done/models-gateway/slices/37-visible-families.md`（可见名单项 = family 标签 | provider id | group id，大小写不敏感）。

## 两个 family 概念的边界（必须写进契约文档）

- **owner family**：`model_gateway.json` 里 providers/groups 行上的 `family` 字符串标签（本契约的对象）；
- **catalog family**：models.dev 模型条目的 `family` 字段（`CatalogModel.family`，目录侧事实）。
- 两者互不相干；本契约不给 catalog family 建定义表。

## 数据形状（model_gateway.json 新可选键 families；旧文件无该段 = 无家族，语义完整）

```json
{
  "families": [ { "id": "relay", "name": "Relay 系列" } ],
  "providers": [ { "id": "relayco", "family": "relay" } ],
  "groups": [ { "id": "fast", "family": "relay", "members": ["relayco/m1"] } ],
  "visible": { "opencode": ["relay"] }
}
```

- `FamilyRow { id: String /* 必填、非空、无 '/' */, name: Option<String> }`。
- 现状不变式：行上的 family 标签继续被 visible 匹配（37 号切片契约）；一等化后标签必须指向 `families[]` 已定义行。
- schema 变更面：`families` 键从 `ModelGatewayDoc.rest` 提升为 typed 字段——**这是 store/doc.rs 的唯一 schema 改动**。

## 文件清单与签名（实现时零决策）

```rust
// crates/skillstar-gateway/src/store/families.rs（新建）
pub struct FamilyRow { pub id: String, pub name: Option<String> }
pub fn defined_families(doc: &ModelGatewayDoc) -> Vec<String>;
pub fn owner_family(doc: &ModelGatewayDoc, owner: RouteOwner, id: &str) -> Option<String>;
pub fn members_of_family(doc: &ModelGatewayDoc, family: &str) -> Vec<String>;
pub fn save_families(rows: &[FamilyRow]) -> Result<(), SaveFamilyError>;
enum SaveFamilyError { Duplicate, Store }   // Display: "family_duplicate" / "family_store"

// crates/skillstar-gateway/src/store/visible.rs（扩展一个函数）
pub(crate) fn visible_family_refs(doc: &ModelGatewayDoc, agent: &str) -> Vec<String>;
// agent 名单项中指向 family 的项；配合 defined_families 做引用完整性
```

命令/DTO：`app/models/gateway/families.rs` 投影 + ts-rs DTO；前端 `api/families.ts` + `components/hub/FamilyControl.tsx`（挨着 GroupMembers，同属目录治理控件）。

## 引用完整性测试（tests/families.rs，命名即规格）

1. `a_visible_name_points_at_a_defined_family_provider_or_group` —— 悬空 family 引用可被 `visible_family_refs` ∖ `defined_families` 测出。
2. `an_owner_family_tag_points_at_a_defined_family` —— 行标签 ∖ 定义集 的差集非空可测出。
3. `a_family_without_members_is_still_a_definition` —— 空 family 合法，`members_of_family` 返回空。
4. `saving_a_family_keeps_every_foreign_key_and_row_field` —— 复用 store_doc roundtrip fixture。
5. `duplicate_family_ids_are_refused_and_the_file_stays` —— `SaveFamilyError::Duplicate`，字节不变。

## 验证（本片只交付文档）

- 契约文档落点：本文件即 SSOT；实现 spec 引用它。
- `docs/features/models/README.md` 加一行指针（行为未变，不抄契约内容）。

## 人工检查点（非阻塞）

契约的 FamilyRow 字段集（要不要 members 内联）给用户过目一次；默认**不要**——成员关系由行上标签表达，families[] 只做定义处，避免双向冗余真相。

## 会改变本片的反馈

用户若要 family 直接当路由候选（family → 展开成 provider 集合参与排序）→ 那是 route/ 的新行为，属于实现 spec 的切片，本契约的签名已预留（members_of_family）。

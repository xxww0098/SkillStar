# 02 account_book 改道 custody（P0-2 前半）

无 tree 前置（account_book.rs 原地改；tree-07 之后顺带搬进 gateway/）。

## 解锁的契约

`UsageAccountBook::account()` 不再从存储行解密 token 作为真相源——改为 live-first：custody 判定归属的 snapshot/live 优先，存储行降级为回退。止住「绕过 custody 签名」这条错路（401 自愈的另一半在切片 11）。

## API 接缝

```rust
// crates/skillstar-usage/src/usage_switch.rs —— 新公开窄口（CliCredentialTarget 保持 pub(super)）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness { Live, Row, Diverged }
pub struct SigningMaterial {
    pub access_token: Option<String>, pub account_id: Option<String>,
    pub api_key: Option<String>,
    pub subscription_id: Option<String>,   // 账本归因用；Diverged 时 None
    pub freshness: Freshness,
}
/// 纯读，不进任何锁：Custody::probe（custody.rs:232）→
///   LinkedTo → snapshot 直读；Diverged → orphan 材料（subscription_id=None）；
///   Missing  → 存储行回退（现 account_book.rs:16-23 逻辑）+ Freshness::Row
pub fn signing_material(catalog_id: &str) -> Option<SigningMaterial>;
```

app 侧：`UsageAccountBook::account()` 改调 `signing_material`，映射进既有 `AccountSnapshot`（字段不动，sign.rs trait 契约不变）。`allowance()` 维持读 usage snapshot（不涉密，不改）。

## 人能看/跑什么

```bash
cargo test -p skillstar-usage --locked   # 新测试：三态 → 三种 Freshness
cargo test -p skillstar-app --locked     # usage_account_book_reads_the_pinned_row 改写后仍证明 pinned 语义
```

## 必须保持绿

`sign_` 测试族（trait 契约零变化）；custody 既有全部测试（新测试进新文件，不进 custody_tests.rs——它正被 tree-08 拆分）；account_book 现有测试按新通道改写（改写前先跑旧测试记录 pinned 语义）。

## 委托给实现者的自由

facade 形状（函数 vs struct）；`signing_material` 放 usage_switch.rs 还是子模块。

## 会改变本片的反馈

若 `Custody::probe` 的纯读路径实测会创建目录/文件（违反「reconcile 不取锁不建家」的既有纪律，README:93）→ 改走 `reconcile_cli_accounts()` 的轻量子集，多花一次读但不留副作用。

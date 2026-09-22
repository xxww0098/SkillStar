# 04 · `usage_switch::ide` 适配器注册表 + 导入 dispatch 表化

## 解锁的契约

IDE 凭据写回从「antigravity/cursor 两个硬编码 if」推广为注册表；`local_import` 白名单
函数化；`import_subscription_token` 命令落地（消费切片 01 的接缝）。

## API 接缝

```rust
// crates/skillstar-app/src/usage_switch/ide.rs
pub trait IdeCredentialAdapter: Send + Sync {
    fn catalog_id(&self) -> &'static str;
    fn available(&self) -> bool;                 // live 存储存在（zed 非 macOS 恒 false）
    fn activate(&self, sub_id: &str) -> UsageResult<(Subscription, SwitchOutcome)>; // 备份→写→回读→pin 最后
    fn sync(&self, sub: &Subscription) -> UsageResult<SwitchOutcome>;             // refresh 后投影回 live
    fn reconcile(&self) -> UsageResult<Option<CliAccountState>>;                  // 读 live→三态
    fn adopt_before_refresh(&self, sub: &mut Subscription) -> UsageResult<()>;    // 吸收工具侧轮换
    fn forget(&self, sub_id: &str) -> UsageResult<()>;
}
```

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/{mod,antigravity,cursor}.rs` | 既有逻辑包薄壳成 trait impl，**行为零变化**（keychain 优先读取、protobuf 写回、回读校验全部保留） |
| app | `usage_switch.rs` | `supports_cli_switch` → `supports_switch = target_for(catalog) \|\| ide_adapter_for(catalog)`；`CliRefreshLease` 从 antigravity/cursor 双 bool 推广为 `ide: Option<&dyn IdeCredentialAdapter>`；`acquire_cli_refresh_lease`/`adopt_active_cli_session_before_refresh`/`sync_refreshed_active_subscription`/`forget_subscription_session`/`reconcile_cli_accounts`/`activate`/`resync` 全部走注册表；OAuth finalize 后「active 行回投」的硬编码名单改为「有 adapter 且目标是 active」 |
| 域 | `local_import.rs` | `matches!` 白名单改 `LOCAL_IMPORTERS` dispatch 表（每 provider 一个 `import_from_local()` 入口）；本文件退化为调度+锁域，provider 实现住进各自 fetcher 文件（防爆行） |
| 域 | `crates/skillstar-usage/src/token_import.rs`（新） | `token_import_supported(catalog_id)` + `import_subscription_from_token(catalog_id, payload, target_subscription_id)`：`with_catalog_lock` 内调 provider `import_from_token(payload)`→归一化→加密→upsert→立即 refresh 一次验证活性（失败拒建行）；粘贴内容永不进日志/事件/DTO |
| app | `usage/service.rs` + `src-tauri/src/commands/usage_commands.rs` | `import_subscription_token` 命令 + DTO；create/update 对 token-import 的拒绝改指向此命令 |
| 前端 | `api.ts` + `devMock` | 新命令签名；`TokenImportFields`（粘贴 textarea + provider 提示）进 `SubscriptionEditDialog` |

## 人能看见

既有行为不变（antigravity/cursor 切号回归即证明）；`import_subscription_token` 对未注册
catalog 返回明确「不支持」。

## 验证

- `custody_tests` 全绿（迁移不破行为）。
- 注册表 conformance 测试：每个 `supports_switch==true` 的 catalog 都有 adapter；无 adapter 的 catalog 在 reconcile map 中缺席（pin 回退语义不变）。
- `token_import`：未注册 catalog fail-closed；锁域断言；`target_subscription_id` 原位替换 + metadata 保留；导入后 refresh 失败→拒建行。
- `bun run test -- src/features/usage`（TokenImport 表单出现/提交字段）。
- 结构门禁：`check_file_size.sh`（local_import.rs 应瘦下来）、`check_command_boundaries.sh`。

## 委托给实现者的决定

- trait 对象存放形态（`&'static dyn` 数组 vs `fn` 表）。
- `import_from_token` 的 payload 类型（不透明 `String`，provider 内部 serde）。

## 必须保持绿

- pin/三态/reconcile 语义、active 回投时机、回滚 Stage 语义一字不动。
- `SKILLSTAR_TOOL_SYNC_HOME` 沙箱下 keychain 整体关闭的既有行为。

## 会改变本片的人类反馈

- 若不希望触碰 antigravity/cursor 现有适配器（保持双轨而非统一注册表），本片缩为「新增注册表但不迁移存量」——默认仍推荐统一，双轨留作备选记录。

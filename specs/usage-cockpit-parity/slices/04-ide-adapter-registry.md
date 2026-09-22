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

## 结果

IDE 切号和本机导入都走表，不再写死 antigravity/cursor。生产环境没有 token importer，所以 `import_subscription_token` 对每个 catalog 都返回「不支持」，并且不写订阅。

落地：

- `usage_switch/ide.rs` 放 `IdeCredentialAdapter` 和 `&'static dyn` 表。impl 是既有 `usage_switch/antigravity.rs`、`usage_switch/cursor.rs` 里的单元结构体，函数体没搬。keychain 优先读、protobuf 写、回读校验、成功后才 pin，都还在原来的函数里。
- `supports_switch = target_for || ide_adapter_for`。`supports_cli_switch` 只是这个谓词的旧名字，DTO 字段名没改。
- `CliRefreshLease` 去掉两个 bool，改成 `ide: Option<&'static dyn IdeCredentialAdapter>`。activate / resync / reconcile / forget / refresh lease / adopt / sync 都先查这张表。
- OAuth 完成回投不再匹配 `"xai" | "antigravity"`。现在是「`supports_switch` 且这行已经是 pin」才 `activate_subscription`。
- `local_import.rs` 只留 `LOCAL_IMPORTERS`、catalog 锁，以及三家共用的 `upsert_oauth_subscription`。codex / antigravity 的读取进了各自 fetcher 的 `import_from_local`。Cursor 放在新文件 `fetchers/oauth/cursor_import.rs`，`fetchers/oauth/cursor.rs` 没改。
- `token_import.rs`：`token_import_supported` + `import_subscription_from_token`。`TOKEN_IMPORTERS` 是空表。查找、解析、归一化、加密、upsert、验证 refresh 都在 `with_catalog_lock` 里。refresh 失败不 upsert。粘贴是 `String`，不进日志、事件或 DTO。没有新的 ts-rs 结构，所以没跑 `types:gen`。
- 命令 `import_subscription_token` 在 `usage/token_import.rs`（`service.rs` 仍是 820 行）和 `usage_commands.rs`。create/update 拒绝文案仍指向这个命令。
- `TokenImportFields` 只在 auth mode 为 `token-import` 时出现。提交走 `importSubscriptionToken`，不走 `createSubscription` 的 api key。没有 catalog 提供这个模式，生产对话框看不到它。devMock 返回一张卡片，不回显粘贴。

测试：

- `cargo test -p skillstar-usage --locked --lib -- local_import token_import`：12 passed（含 local_import 4 个、token_import 5 个；另外 3 个是名字里带 token_import 的既有测试）。
- `cargo test -p skillstar-app --locked --lib -- usage_switch usage::`：86 passed，含 `custody_tests` 和「无 adapter 的 catalog 不进 reconcile map」。
- `bun run test -- src/features/usage`：145 passed，含 `SubscriptionEditDialog.tokenImport.test.tsx`。

静默决定：

- trait 对象是 `&'static dyn` 常量数组，不是函数表。适配器文件留在 `usage_switch/` 下，没有再套一层 `ide/` 目录。
- IDE `forget` 仍是空操作。删卡片不会把 IDE 登出，因为本来就没有按账号存的快照。
- `available()` 表示 `state.vscdb` 路径解析得出来。桌面三个系统上都为 true，所以库文件不存在时 reconcile 仍是 `Missing`（在 map 里），不是缺席。缺席只留给没有 adapter 的 catalog，pin 回退不变。
- 回投旧名单其实是 `xai | antigravity`，不是 antigravity/cursor。改成 `supports_switch` 之后，xAI 仍在（它是 CLI target）；cursor、codex、opencode 在「这行已经是 pin」时也会重新 activate。时机仍是 OAuth 完成之后、且只针对当前 pin，pin 仍写在写回成功之后。
- 共用的 upsert 留在 `local_import.rs`，避免三份 refresh-then-save，也避免把持久化拆进每个 fetcher。
- 测试用 `cfg(test)` 注册表模拟 importer，并按 catalog 替换 refresh，不打网络。未注册 catalog（包括现在的 cursor）在锁内直接「不支持」，不写行。`target_subscription_id` 用 `carry_over_user_metadata` 保留价格、备注、排序和其余用户字段。目标行不存在就 `NotFound`，catalog 对不上也拒绝，两种都不写。
- 对话框只提交 catalog id、trim 后的粘贴、以及编辑时的目标 id。创建路径带不走表单里的价格/备注，因为 token-import 行不能再走 update。替换已有行时，库里的价格/备注/排序保留。
- access token 和 `provider_state` 都空才拒绝。没有写 copilot 解析。
- IDE reconcile 失败的日志收成一句，catalog id 仍在 tracing 字段上。

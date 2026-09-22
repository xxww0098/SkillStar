# 10 · windsurf — 配额 + 登录 + 导入（L0–L3）

## 解锁的契约

`windsurf` catalog 上线：OAuth 浏览器腿 + TokenImport（apiKey/JSON）+ 本机导入；
配额显示 plan + User Prompt credits + Add-on prompt credits + 周期。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` + `identity.rs` | `windsurf` 行（tier=OAuth，`[OAuth, TokenImport]`，brand `09B6A2`，url=`https://windsurf.com`）+ identity |
| 域 | `fetchers/oauth/windsurf.rs`（新，超 800 行即拆 `windsurf/{mod,login,quota}.rs`） | `start_login`：`windsurf.com` 系 OAuth → firebase id_token → Connect-RPC `SeatManagementService`：`RegisterUser`→`apiKey`+`apiServerUrl`→`GetOneTimeAuthToken`→`GetCurrentUser`/`GetPlanStatus`（`register.windsurf.com`/`server.codeium.com`，Connect-RPC = JSON POST）；implicit 回调走 fragment——`manual_callback` 已有 fragment→query 合并测试可复用；Devin auth 链 `auth1_token` 长凭证入 `provider_state_encrypted` |
| 域 | quota 解析 | cockpit 在 `windsurf_devin_oauth.rs` 自实现 protobuf varint 解析（无 prost 依赖）——直接移植；保留 raw status + per-field 降级 |
| 域 | `local_import.rs` dispatch + `fetchers/oauth/windsurf.rs::import_from_local` | 读 `windsurf_state_db_path()` 的 `windsurfAuthStatus` + `windsurf_auth-*` 键（明文 ItemTable 键；`secret://` 值如需读依赖切片 05 结论） |
| 前端 | brandThemes/logo/i18n/devMock | — |

## 字段落法

`access_token_encrypted`=authToken/session；`refresh_token_encrypted`=firebase refresh（若有）；
`provider_state_encrypted`=`{apiKey, apiServerUrl, auth1_token?}`；`oauth_account_id`=email。

## 人能看见

Windsurf 卡三入口（OAuth/粘贴/本机导入）；配额条显示 credits 构成。

## 验证

- Connect-RPC 响应 fixture 解析（varint/proto JSON 两形态）；401→AuthRequired 分类。
- implicit/fragment 回调解析测试；**若 `windsurf://` 深链出现则降级 `manual_callback` 粘贴**（已知未知，README 已记）。
- 本机导入 fixture：临时 vscdb 含 windsurfAuthStatus → 建行。
- 真实联调一次（人工记录）。
- **视觉门禁**：卡片截图 → screenshot-critique。

## 委托给实现者的决定

- protobuf 手写解析 vs 引入轻量 prost（优先照抄 cockpit 手写解析，零新依赖）。

## 必须保持绿

- Connect-RPC 非 2xx 走统一 `UsageError::http_status` 分类。

## 会改变本片的人类反馈

- Windsurf Devin-auth 迁移期旧 `windsurf_auth` 与 `auth1` 并存策略（默认：都读，新的优先）。

## 结果

实现在 `crates/skillstar-usage/src/fetchers/oauth/windsurf/`（`mod` / `login` / `quota` / `import`，测试在 `quota_tests.rs`）。`fetchers/oauth/mod.rs` 只加了 `pub mod windsurf;`，没有 `dispatch` / `start_login` 臂。catalog、identity、`token_import.rs`、`local_import.rs` 都没改。

真实浏览器登录：**未验证**。cockpit 源码里没有 `windsurf://`，没有发明自定义 scheme，也没有降级。`OAuthFlow::LocalCallback`，`redirect_parameters_type=query`，回调路径 `/windsurf-auth-callback`。共享 `local_server` 只收 `code`，implicit 的 `access_token` 用本模块自己的 loopback。粘贴的 fragment 仍走既有 `manual_callback`（先并进 query 再 GET）。

### 集成者要贴的行

`catalog.rs`，用已有的 `entry(...)` 和 `OAUTH_TOKEN_IMPORT`：

```rust
entry(
    "windsurf",
    "Windsurf",
    "Codeium Windsurf",
    CatalogTier::OAuth,
    OAUTH_TOKEN_IMPORT,
    "09B6A2",
    "USD",
    "https://windsurf.com",
),
```

`crates/skillstar-core/src/providers/identity.rs`：

```rust
ProviderIdentity {
    canonical_id: "windsurf",
    display_name: "Windsurf",
    catalog_id: Some("windsurf"),
    preset_ids: &[],
},
```

`fetchers/oauth/mod.rs` 的 `dispatch`：

```rust
"windsurf" => windsurf::fetch(subscription).await,
```

同文件的 `start_login`（浏览器腿，不是 `dispatch`）：

```rust
"windsurf" => windsurf::start_login(region, target_subscription_id).await,
```

`token_import.rs` 的 `TOKEN_IMPORTERS`：

```rust
TokenImporter {
    catalog_id: "windsurf",
    import_from_token: crate::fetchers::oauth::windsurf::import_from_token,
},
```

`local_import.rs` 不能复用 `upsert_oauth_subscription`：那个函数把 `provider_state_encrypted` 写成 `None`，apiKey 会丢。贴这个：

```rust
fn import_windsurf() -> LocalImportFuture {
    Box::pin(async {
        let imported = crate::fetchers::oauth::windsurf::import_from_local()?;
        let sub = crate::fetchers::oauth::windsurf::oauth_row_from_imported(imported)?;
        crate::storage::upsert_subscription(sub)
            .map_err(|err| crate::UsageError::Other(format!("Windsurf 订阅保存失败：{err}")))
    })
}

LocalImporter {
    catalog_id: "windsurf",
    import_from_local: import_windsurf,
},
```

这条只建 OAuth 行，不打配额。`dispatch` 接上之后，若要和 codex 一样先 refresh，在 `upsert` 前调用 `windsurf::fetch`。

### 配额字段

都落在现有 `SubscriptionUsage` 上。缺字段就省略该条，不补 0。接口里的 0 会保留。

| 来源 | 落点 |
| --- | --- |
| `planInfo.planName` / `plan_name` / `teamsTier` / `teams_tier` | `plan_name`。空串和 `Unknown` 省略 |
| `usedPromptCredits` + `monthlyPromptCredits`，或 available+used，或 monthly−available | `monthly`，label `User Prompt credits`。`reset_at` = `planEnd`（数字或 `{seconds}`） |
| flex / add-on / top-up 的 available、used、monthly | 上一条的 `breakdown`，label `Add-on prompt credits`。没有 prompt 条时，这条自己当 `monthly` |
| `dailyQuotaRemainingPercent`（及 `dailyRemainingPercent`） | `hourly`，label `Daily`。`used = 100 - remaining`，`total = 100`。字段不在就不画 |
| `weeklyQuotaRemainingPercent`（及 `weeklyRemainingPercent`） | `weekly`，label `Weekly`。同样只在字段存在时画 |
| `GetCurrentUser` / `userStatus` 的 email | 空的 `oauth_account_id`，以及占位标题 `Windsurf` |

只有 `available*`、没有 used 也没有 monthly 时，不造 `used: 0` 的条。周期单独存在、没有额度数字时，不造空条。

数字按接口原值，不除 100（cockpit 展示也是原值；`× 100` 只是 Devin 注释，未用真账号核对）。

会话（access token，且不是 `sk-ws-` apiKey）走 `GetPlanStatus` + `GetCurrentUser`。只有 apiKey 时走 `GetUserStatus`（cockpit 的 apiKey 腿；`GetPlanStatus` 要 authToken）。`RegisterUser` 只在浏览器交换里用。Auth1 protobuf 的 `GetPlanStatus` 按 cockpit 字段号手写解析（field 1 planStatus，2 planName，3 planEnd，14/15 日/周剩余百分比，17/18 reset）。没有新 prost 依赖。HTTP 只经 `fetchers::http_client()`。401 或 body `invalid_grant` → `AuthRequired`；429/5xx/传输 → `Transient`；403 和其他 → `Fetcher`。

### 凭据

`ImportedToken.provider_state` 是明文 JSON，由 token-import 管道加密。不是事先加密的密文，也不是裸 apiKey 字符串（裸 `sk-ws-` 粘贴会收成这个 JSON；读的时候两种都认）：

```json
{"apiKey":"...","apiServerUrl":"...","auth1Token":"..."}
```

缺的键省略。`access_token` = `authToken` / session（没有就空串）。`refresh_token` = firebase refresh（有才填）。`oauth_account_id` = email。`devin-session-token$` 同时放进 access token，`apiServerUrl` 用 `https://server.self-serve.windsurf.com`。`auth1_` 只进 `auth1Token`，不跑 Devin 四步 protobuf 交换；这种行刷新会得到 Fetcher「缺少 apiKey 或 session」，不是 `AuthRequired`。implicit 的 `expires_in` 不落库（没有 refresh 交换，过期时间戳会被当成死会话）。

本机导入只读 `windsurf_state_db_path()` 的 `windsurfAuthStatus`，以及 `secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.sessions"}` 和同扩展的 `windsurf_auth.apiServerUrl`。不读 `windsurf_auth-*` usage 缓存。`windsurfAuthStatus` 里已有的 apiKey / email 优先，secret 只填空缺。`secret://` 用 `tool_store::safe_storage`，密钥由调用方注入；`import_from_local()` 传 `None`，不读钥匙串。密文认标准 base64，也认 cockpit 的 `{"type":"Buffer","data":[...]}`。测试用 `SKILLSTAR_TOOL_SYNC_HOME` 和临时 sqlite，密码 `injected-password`。

### 测试

`cargo test -p skillstar-usage --locked --lib -- windsurf`：29 passed。无真实 Windsurf 请求。mock HTTP 覆盖 JSON 配额、401/`invalid_grant`、403、GetUserStatus、RegisterUser 交换。protobuf fixture 覆盖日/周百分比。本机导入覆盖明文 `windsurfAuthStatus`、`secret://` 往返、错误密码、以及没有密钥时拒绝读钥匙串。

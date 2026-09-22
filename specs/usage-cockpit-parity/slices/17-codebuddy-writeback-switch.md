# 17 · codebuddy×2 — 切号写回（L4）

> 依赖：16 + 04 + 05。

## 解锁的契约

切号写回两端各自的 `state.vscdb` `secret://` 键：
国际版 `planning-genie.new.accessToken`，CN 版 `…accessTokencn`（后缀差异集中进参数表）。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/codebuddy.rs` | 一个 adapter 实现 + domain/key 参数表；注册 `codebuddy` + `codebuddy-cn` 两 catalog |
| app | `usage_switch.rs` | 注册 ×2 |

## 人能看见

两卡各自切号 + 三态 badge；真机重启换号（人工记录 ×2）。

## 验证

- safe-storage 键写回回环 ×2 变体（key 名差异断言钉死——**写串后缀是已知坑**）。
- reconcile 三态 ×2。
- 人工 smoke 记录。

## 委托给实现者的决定

- 两变体共享 impl 的参数表形状。

## 必须保持绿

- 各写各的目录，`CodeBuddy` 与 `CodeBuddy CN` 不互踩。

## 会改变本片的人类反馈

- 无。

## 结果

适配器在 `crates/skillstar-app/src/usage_switch/codebuddy.rs`，`GLOBAL_ADAPTER` 与 `CN_ADAPTER` 注册在 `usage_switch/ide.rs`，跟 Antigravity、Cursor、Windsurf、Kiro、Qoder 并列。没有再套 `ide/` 目录（切片 04 已经这么放）。两端不是两份实现：`Profile` 表带 catalog id、展示名、完整 secret 键、session id、口令环境变量、`state.vscdb` 路径函数。`supports_switch("codebuddy")` 与 `supports_switch("codebuddy-cn")` 为 true。`oauth_completion_rewrites_live_store` 对这两个 catalog 因为 IDE adapter 存在而为 true。

官方 CodeBuddy / CodeBuddy CN 重启：**未验证**。没有对真机写过，也不能说官方 app 接受了这次写回。不因此把 `supports_switch` 设成 false。

### 写入的键（只动 auth）

路径只认 `tool_paths::codebuddy_state_db_path()` 与 `codebuddy_cn_state_db_path()`。库不存在就不建、不复制。事务里只 upsert 这一条 `secret://`。回读不一致则把 rolling backup 拷回去，pin 不动。两端各写各的目录。

| catalog | ItemTable 键 | 密文里的 `id` |
| --- | --- | --- |
| `codebuddy` | `secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessToken"}` | `Tencent-Cloud.genie-ide` |
| `codebuddy-cn` | `secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessTokencn"}` | `Tencent-Cloud.genie-ide-cn` |

CN 键是表里的完整字面量 `planning-genie.new.accessTokencn`，不是写的时候再追加后缀。`accessToken` 是 `accessTokencn` 的前缀，测试用整键相等，不用 `contains`。

明文是 cockpit `build_default_client_session_json` 那一包：`token` 为 access token，有 uid 时 `accessToken` 为 `{uid}+{token}`，`refreshToken` / `expiresAt` / `domain`，`converted: true`，`account`（uid、nickname、enterpriseId、enterpriseName、`pluginEnabled`、`lastLogin`），`auth`（accessToken、refreshToken、`tokenType: Bearer`、domain、expiresAt、expiresIn、refreshExpiresIn `0`、`lastRefreshTime`）。企业字段来自 `provider_state`。nickname 只用非邮箱、非产品名的 `display_name`。没有 uid 时不写前导 `+`。缺的 refresh / 企业 / domain 写成空串，缺的 expires 写成 `0`，以便形状对齐 cockpit；对账不会用 `0` 或空 refresh 盖掉卡上已有的值。

`forget` 只删这一个 auth 键。不删库，不动无关行，也不动另一端的键。只有 live 的 access token（或 uid）对得上这张卡才清；删另一张卡不会把 IDE 登出。对不上、密文解不开、或库不存在则不动文件。

`reconcile`：路径能解析但没有库、库在但没有 auth 键、或解开后没有 token，是 `Missing`（不是缺 adapter）。token 对上某张同 catalog 订阅是 `LinkedTo`（文件赢，不必是 pin）；对不上 token 但 uid 对上，也是 `LinkedTo`，并把新 token / 非空 refresh 吸回订阅，pin 不动。有 token 但对不上、或密文解不开：`Diverged`。比对先看规范化后的 access token（`uid+token` 会拆开），再看 uid。不看邮箱，cockpit 这份 session JSON 里没有 email。

`sync` 把刷新后的 token 投影回 live db，不改 pin。

### 测试

`cargo test -p skillstar-app --locked --lib -- usage_switch::codebuddy`：11 passed。临时 sqlite，`SKILLSTAR_TOOL_SYNC_HOME` / `SKILLSTAR_DATA_DIR` 沙箱。国际版口令 `SKILLSTAR_CODEBUDDY_SAFE_STORAGE_PASSWORD`，CN 口令 `SKILLSTAR_CODEBUDDY_CN_SAFE_STORAGE_PASSWORD`，测试值都是 `injected-password`。没有碰真实 CodeBuddy 目录，也没有调用钥匙串。

覆盖：键名后缀钉死、写→回读解密、无关行和另一条 catalog 的 decoy 键保留、两端目录互不覆盖、两账号切换、空 uid 不写 `+token`、Missing / LinkedTo / Diverged、文件赢过 pin、uid 相同 token 轮换后吸回、cockpit `{"type":"Buffer","data":[...]}`、口令错误是 Diverged 不是 Missing、forget 只清本 catalog 的 auth 键且不删库、无库或无口令或没有 access token 时 pin 不动且不建库、CN 不借用国际版口令、回读失败恢复备份且 pin 不动、sync 投影新 token 但不改 pin。

### 静默决定

- Safe Storage 口令按 catalog 分开注入。不读 macOS 钥匙串、Linux secret-tool 或 Windows DPAPI。没注入口令时切换直接失败，pin 不动。
- 密文算法跟宿主走，和 Qoder / Windsurf 写回相同：macOS `KeyMaterial::macos_v10`（PBKDF2 1003），Linux `linux_v10`，Windows 把口令 SHA-256 成 32 字节 `OsCryptKey`（本构建没有 DPAPI，这不是 CodeBuddy.exe 的密钥）。写入值是 `encrypt_secret` 的标准 base64，不是 cockpit 的 Buffer JSON。读的时候两种都认。
- 不复制 cockpit 实例注入里的「目标库不存在时复制默认库」，也不做会话合并。`codebuddy_state_db_path` / `codebuddy_cn_state_db_path` 只给路径，不建文件。库不存在就切换失败，reconcile 为 Missing。
- 订阅没有 `token_type`。`auth.tokenType` 固定写 `Bearer`。
- cockpit 在 uid 为空时仍写 `+{token}`。这里不写这个前导加号，否则导入侧的 `uid+token` 切分会把加号留在 token 里，回读对不上。
- `expiresIn` 跟 cockpit 一样写成和 `expiresAt` 同一个数，不是一段秒数。


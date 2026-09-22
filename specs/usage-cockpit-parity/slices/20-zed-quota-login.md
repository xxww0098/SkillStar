# 20 · zed — 配额 + 登录 + 导入（L0–L3）

> 依赖：02（local_server 参数 map）+ 07（RSA 解密已验证）。

## 解锁的契约

`zed` catalog 上线：`native_app_signin` OAuth（本地监听收 RSA 加密凭据）+ TokenImport
（`{user_id, access_token}` JSON 或裸 token）+ 本机导入（macOS Keychain 读）；
配额显示订阅状态 / Edit Predictions / Token Spend / Spend Limit / 账期结束。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` + `identity.rs` | `zed` 行 + identity |
| 域 | `fetchers/oauth/zed.rs` | `start_login`：生成一次性 RSA-2048 对 → `zed.dev/native_app_signin?native_app_port=<p>&native_app_public_key=<pk>` → loopback 收 `user_id`+加密 `access_token` → 解密（07 的成果）→ 建卡；quota=`cloud.zed.dev/client/users/me`，**`Authorization: "<user_id> <token>"` 自定义 scheme**；billing 端点一律不调（对有效桌面凭据也 401——误置 reauth 的坑） |
| 域 | 本机导入 | `security find-internet-password -s https://zed.dev`（meta 取 account=user_id，`-w` 取 token）；非 macOS 明确「仅支持 macOS 本机导入」 |

## 字段落法

`access_token_encrypted`=access token；`oauth_account_id`=user_id；无 refresh（token 长效）。

## 人能看见

Zed 卡点登录→浏览器授权→自动完成；本机导入读 Keychain；卡片显示 spend/limit。

## 验证

- RSA 回调解密测试（07 的 fixture 直接复用）；quota `Authorization` 头格式断言。
- 401→AuthRequired 分类；非 macOS 本机导入降级文案测试。
- 真实联调一次。
- **视觉门禁**：卡片截图 → screenshot-critique。

## 委托给实现者的决定

- 本地监听端口分配复用 `local_server` 还是独立 socket（倾向复用）。

## 必须保持绿

- 不调 `/frontend/billing/*`；不尝试刷新 zed token（无 refresh 腿，失效走 reauth）。

## 会改变本片的人类反馈

- 无。

## 结果

`zed` 已进 catalog（OAuth + TokenImport，USD，品牌色 `#2E6BE6`，续费页 `https://zed.dev/account`）和 identity。配额、RSA 登录、令牌导入、本机导入都在 `fetchers/oauth/zed/`。`dispatch`、`start_login`、`TOKEN_IMPORTERS`、`LOCAL_IMPORTERS` 各加一条。catalog 22→23，OAuth tier 15→16。没有进 `usage_switch`，也没有进实例注册表。

登录是 `OAuthFlow::LocalCallback`。每次生成一次性 RSA-2048（PKCS#1）。浏览器打开 `https://zed.dev/native_app_signin?native_app_port=<p>&native_app_public_key=<pk>`，公钥是 URL-safe base64、无 padding。`local_server::wait` 仍要求 `code` + 匹配的 `state`（02 把无 code 的回调留到本片）。Zed 走新的 `wait_for_keys`：query 里有非空 `user_id` 和 `access_token` 才结束；缺键继续等；`error=` 或空值结束并报错。密文用已有的 `zed_token::decrypt_zed_token`（OAEP-SHA256，失败再 PKCS#1 v1.5）。`access_token_encrypted` 是解开的 access token，`oauth_account_id` 是 `user_id`，没有 refresh token。监听 `0` 时记下操作系统分配的端口，签到 URL 和取消才打得到真正的 socket。

配额只 `GET https://cloud.zed.dev/client/users/me`。`Authorization` 是 `"<user_id> <token>"`，不是 Bearer。不调用 `/frontend/billing/*`。401 → AuthRequired；403 → Fetcher；429 / 5xx / 传输 → Transient。有 `used` 才建窗口，显式 0 保留。Token Spend 有正的美分上限时，窗口标签用现成的 `Monthly credits`，卡片按美元画；没有上限就标 `Token Spend` 且不补 total。Edit Predictions 有数字上限时占另一条窗口（月槽已被 Token Spend 占用时用周槽，标签仍是 Edit Predictions）。`unlimited` 不画空条，写成 credit `N / unlimited`。Spend Limit 是 credit 行（`$10.00` 这种），不假装成已用额度。账期结束写在窗口的 `reset_at`。套餐来自 `plan.plan_v3`（`zed_pro` → `Zed Pro`）。`has_overdue_invoices` 在套餐名后加 ` · overdue`。缺字段就不建对应窗口。

令牌导入只接受 JSON `{user_id, access_token}`。`user_id` 可以是数字。裸 token、缺字段、空字符串、垃圾都拒绝。没有 refresh。本机导入在 macOS 上先 `find-internet-password -s https://zed.dev` 取出 account，再 `find_internet_password("https://zed.dev", account)`。`SKILLSTAR_TOOL_SYNC_HOME` 在 spawn `security` 之前返回原来的沙箱错误。`local_import_available("zed")` 在非 macOS 上是 false，导入文案是「Zed 本机导入仅支持 macOS」。测试注入假的 account/password，或只测解析和解密，不调用 `/usr/bin/security` 打真实登录钥匙串。

`lobe.ts` 没有导出 Zed 图标，卡片用字母 Z。主题色用上面的 `#2E6BE6`（cockpit 没有色值）。`LOCAL_IMPORT_CATALOG_IDS` 加了 `zed`。

没有调用 zed.dev。不降级。

测试：`cargo test -p skillstar-usage --locked --lib -- zed catalog::` 39 passed；`cargo test -p skillstar-core --locked --lib -- identity` 9 passed。

静默决定：

- 无 `code` 的回调没有放宽 `wait` 本身，避免既有授权码登录把缺 `code` 当成成功。Zed 用同一监听器上的 `wait_for_keys`。
- 登录超时沿用监听器默认 300 秒，不是 cockpit 的 600 秒。
- 品牌色 `#2E6BE6` 是本片记下的稳定蓝，不是从 cockpit 抄来的。
- 裸 token 不能导入：配额头需要 user id 和 access token 两段。
- Token Spend 用 `Monthly credits` 只是为了走现成的美分美元条，没有新的展示字段。Edit Predictions 在月槽被占用时放进周槽，标签不变。
- Spend Limit 与 Token Spend 的 limit 不是同一个字段；前者始终是 credit 行。
- 本机导入先无 `-a` 读 account，再按规格调用带 account 的 `find_internet_password`。沙箱优先于「仅 macOS」文案，这样测试在任何系统上都不会启动 `security`。
- 登录后的配额刷新失败不丢掉刚拿到的 token。没有 refresh 腿。

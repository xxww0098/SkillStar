# 18 · trae×4 — 配额 + 登录 + 导入（L0–L3）

> 依赖：06（device-proof + byte_crypto 已验证）。**全计划最重的 provider**——
> 4 catalog 共享一套 `TraePlatformKind` 参数化实现。

## 解锁的契约

`trae`/`trae-solo`/`trae-cn`/`trae-solo-cn` 四条目上线：浏览器登录
（`GetLoginGuidance` → loopback callback → `ExchangeToken`）+ TokenImport（refresh token
或凭据 JSON）+ 本机导入（storage.json iCube 键）；配额显示套餐原始值 + 美元消耗/总额度 +
重置时间。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 依赖 | `cargo add ring aes cbc`（若 06 未引入） | ECDSA P-256 + AES-128-CBC |
| 域 | `catalog.rs` + `identity.rs` | 4 行 catalog（`auth_modes=&[OAuth, TokenImport]`）+ 4 identity |
| 域 | `fetchers/trae/{mod,platform,login,exchange,quota,storage}.rs`（预置目录防爆行） | `TraePlatformKind` 参数表（provider_key/display/client_id/auth_domain/app_support_dir/app_name/region hosts：`api.trae.ai`/`trae.cn`/`trae.com.cn`/`marscode.com`/`traeapi.us` 路由）；login：`GetLoginGuidance`→浏览器→loopback→`ExchangeToken`（`ClientID`+`ClientSecret`+`RefreshToken`+`DeviceInfo.PublicKey`+`DeviceProof` ECDSA 签名，message 拼法逐字节对齐 06）；quota=`GetUserInfo`+entitlement/usage |
| 域 | `token_import.rs` trae 腿 | 裸 refresh_token（生成新 P-256 对，**UI 文案导向本机导入**——服务端可能校验设备绑定）或凭据 JSON（含 auth_raw/deviceKeyPair） |
| 域 | 本机导入 | `trae_storage_path_for(kind)` 读 `storage.json` 的 `iCubeAuthInfo://icube.cloudide` 等 `byte_crypto` 键 |

## 字段落法

`access_token_encrypted`/`refresh_token_encrypted` 标准槽；
`provider_state_encrypted`=`{deviceKeyPair{privateKeyPEM,publicKeyPEM},clientId,loginHost,authDomain}`——
**refresh 每次签名要用私钥**；`oauth_region`=loginRegion；`oauth_account_id`=user id。

## 人能看见

侧栏四张 Trae 变体卡；登录/粘贴/本机导入三入口；配额条。

## 验证

- ExchangeToken mock（DeviceProof 字段断言）；region 路由表测试。
- storage.json byte_crypto 读 fixture（依赖 06 结论）。
- TokenImport 两态输入解析测试。
- 真实联调一次（**若设备绑定拒签，TokenImport 降级为「仅本机导入」并在表单文案说明**）。
- **视觉门禁**：四卡截图 → screenshot-critique。

## 委托给实现者的决定

- 目录内文件划分（mod/platform/login/exchange/quota/storage 为建议形状）。

## 必须保持绿

- token 单次使用 → refresh/切号全在 `with_catalog_lock`。
- 4 变体差异只许出现在 platform 表。

## 会改变本片的人类反馈

- 是否砍掉 `trae-solo-cn` 等低价值变体——默认全做。

## 结果

`trae` / `trae-solo` / `trae-cn` / `trae-solo-cn` 已进 catalog（OAuth + TokenImport，USD，续费页分别是 `https://www.trae.ai` 与 `https://www.trae.cn`）和 identity。配额、设备交换、令牌导入、本机导入都在 `fetchers/oauth/trae/`，由 `TraePlatformKind` 参数化。四条 dispatch、`start_login`、token-import、local-import 臂调用同一套函数。签名仍用已有的 `fetchers/trae/device.rs`，没有再写一份。

登录流选择：**`OAuthFlow::Immediate`，本机 `storage.json` 领养，不打开浏览器，绑定当时不调用 ExchangeToken。** cockpit 的浏览器 URL 来自运行时 `GetLoginGuidance`，不是固定地址；`provider-reference` 和本片契约写的是无浏览器腿。`OAuthStartInfo.auth_url` 只放产品首页（`www.trae.ai` / `www.trae.cn`，已在 `region_hosts` 里），Immediate 面板不会打开它。真正的 refresh token 登录发生在刷新和令牌导入的校验里：一次 `POST {loginHost}/trae/api/v3/oauth/ExchangeToken`。

- 请求体对齐 src-tauri 官方 DeviceProof 刷新：`ClientID`、`ClientSecret`（空字符串，不是旧接口的 `"-"`）、`RefreshToken`、`DeviceInfo`（`DevicePublicKey` / `PlatformCode=IDE_PC` / `DeviceType=PC` / `ClientVersion`）、`DeviceProof`（`Signature` + `Timestamp` + `Nonce`）、`IDEVersion=3.5.66`。签名 message 是 `POST\n/trae/api/v3/oauth/ExchangeToken\n<clientId>\n<refreshToken>\n<ts>\n<nonce>`。
- Refresh token 单次使用：一次刷新只 POST 这一次。失败不回退旧的 `/cloudide/api/v3/trae/oauth/ExchangeToken`，也不按 region host 列表重放同一枚 token。配额才按 `loginHost` 优先、然后 `TraePlatformKind::region_hosts()` 换站。CN 的 pay/usage 先 v2 再 v1；usage 没有可用响应时再试 `user_current_entitlement_list`。
- 交换成功后如果配额失败，新的 access/refresh 留在订阅上，刷新返回 `Ok` 且 `usage.error` 写原因。这样令牌导入的校验不会把已经轮换的 refresh token 丢掉。没有 refresh token 时只拉配额，不交换。本机导入和 Immediate 登录也不交换，避免绑卡时花掉 IDE 手里的那枚 token。
- 若响应没带新的 refresh token，保留原来的。服务端如果已经作废旧 token 又没发新的，下一次刷新会失败；这和 cockpit 一样，没有第二枚可存。
- 配额：`GetUserInfo`（`Bearer` + `x-cloudide-token`）+ `ide_user_pay_status` / `ide_user_ent_usage`（`Cloud-IDE-JWT`）。套餐名优先用接口原文 `user_pay_identity_str`（或 pack 的 `identity_str`），没有字符串才按 product_type 落到 Ultra/Pro+/Pro/Lite/Free/CNExpress。美元窗口是 `Monthly credits`，单位是美分（`basic_usage_amount` / `basic_usage_limit`），卡片按 `$used / $total` 画。缺 used 或缺 total 就不建窗口，不补 0；接口给出的 0 保留。重置时间是 pack `end_time` 归一到秒再 +1。有 basic 窗口时 bonus 走 credit 行；没有 basic 但 bonus 两边都有时，bonus 成为那一条窗口。`pay_go_amount > 0` 记成 On-demand credit，不进额度条。
- `provider_state` 明文 JSON 是 `{deviceKeyPair:{privateKeyPEM,publicKeyPEM},clientId,loginHost,authDomain}`，由导入管线加密。不写 `platform_token_encrypted`。`oauth_account_id` 是 user id。`oauth_region` 是 loginRegion（`china-north`→`cn` 等）。catalog `regions` 为空。
- 令牌导入：凭据 JSON（含 `deviceKeyPair` / iCube 存储形）或至少 20 字符、无空白的裸 refresh token。裸 token 会新生成一把 P-256，catalog warning 写明设备绑定可能失败、请优先本机导入。本机导入走 `tool_paths::trae_storage_path_for`，读 `storage.json` 的 `iCubeAuthInfo://icube.cloudide` 和 `iCubeAuthInfo://icube-dc:*`，值是标准 base64 的 `byte_crypto` blob。测试把 `SKILLSTAR_TOOL_SYNC_HOME` 指到临时目录。cockpit 形密文能往返；翻转一个密文字节会因完整性失败，没有跳过。
- 错误：401 / `invalid_grant` / `invalid_token` → AuthRequired；429 / 5xx / 传输 → Transient；其它含 403（即使 body 写了 invalid_grant）→ Fetcher。HTTP 200 且数字 `code != 0` 是 Fetcher。

真实 ExchangeToken 未调用。不降级。

测试：`cargo test -p skillstar-usage --locked --lib -- trae catalog::` 26 passed；`cargo test -p skillstar-core --locked --lib -- identity` 8 passed。

静默决定：

- 不实现 `GetLoginGuidance` 浏览器腿，也不发明登录 URL。
- 官方 DeviceProof 交换的 `ClientSecret` 用空字符串。旧接口的 `"-"` 和不带证明的第二次交换都没有做。
- 配额多 host 时，Transient 优先于 AuthRequired，避免一台 500、另一台 401 时把账号打成需要重新登录。交换成功后的配额失败不返回 `Err`，以免丢掉新 refresh token。
- 套餐展示保留接口原文，product_type 名字只是没有字符串时的兜底。美元条用现成的 `Monthly credits` 美分格式，而不是新的字段。
- 没有把 device key 写进 `platform_token_encrypted`。
- 字符串字段经 `trim` 后入库，PEM 末尾换行会去掉；签名解析不依赖这一个换行。密文往返测的是解密后的密钥材料，不是逐字节保留 PEM 空白。
- 侧栏图标用已有的 `TraeColor`。四张卡的主题色不同。本机导入按钮把四个 id 加进 `LOCAL_IMPORT_CATALOG_IDS`。没有做切号（slice 19）。


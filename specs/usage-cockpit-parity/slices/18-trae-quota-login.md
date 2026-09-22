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

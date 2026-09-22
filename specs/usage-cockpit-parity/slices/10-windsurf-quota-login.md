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

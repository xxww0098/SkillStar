# Provider 机制参照表（cockpit-tools 调研结果）

本文件是切片实现的事实底座。来源：`/tmp/cockpit-tools`（GitHub `jlcodes99/cockpit-tools`
稀疏克隆，`crates/cockpit-core` + `src-tauri/src`）。注意 cockpit-tools 存在两套树——
`crates/cockpit-core/src/modules/`（抽取中）与 `src-tauri/src/modules/`（更全，例如 Kiro
IDC device flow 只在后者）。**冲突时以 `src-tauri` 版本为准**，core 版本用于对照。

## 每 provider 机制速查

| provider | catalog id | 认证腿 | 配额端点 | 本机真实存储（导入/切号落点） | 切号 | 实例候选 |
| --- | --- | --- | --- | --- | --- | --- |
| GitHub Copilot | `github-copilot` | Web PKCE OAuth（`github.com/login/oauth/authorize`，`redirect_uri=vscode.dev/redirect`，`state`=本地回调 URL，`client_id=01ab8ac9400c4e429b23`）+ TokenImport（`gho_`/`ghp_`/`github_pat_`） | `api.github.com/copilot_internal/v2/token`（auth scheme 是 `token <gh>`，**不是 Bearer**）→ `copilot_internal/user`（quota_snapshots / limited_user_quotas）；plan 识别 Free/Pro/Business/Enterprise | **无本地凭据** | ✗ | ✗（VS Code profile 是另一机制，不冒充） |
| Windsurf | `windsurf` | 浏览器 OAuth → firebase id_token → Connect-RPC `SeatManagementService`：`RegisterUser`→`apiKey`+`apiServerUrl`→`GetOneTimeAuthToken`→`GetCurrentUser`/`GetPlanStatus`（`register.windsurf.com` / `server.codeium.com`）；TokenImport=apiKey 或 JSON | 同上 Connect-RPC JSON POST：plan + prompt credits + add-on credits + 周期 | `…/Windsurf/User/globalStorage/state.vscdb`：`windsurfAuthStatus` + `windsurf_auth-*` 键 | ✓ vscdb 键写回 | Windsurf.app（预期可交付） |
| Kiro | `kiro` | 双腿：(a) `app.kiro.dev/signin` PKCE+本地回调→`prod.us-east-1.auth.desktop.kiro.dev/oauth/token`；(b) AWS IDC device flow：`oidc.{region}.amazonaws.com/client/register`→device auth→轮询 `/token` | `q.{region}.amazonaws.com` CodeWhisperer runtime 用量 | `~/.aws/sso/cache/kiro-auth-token.json` + IdC 注册缓存 + `~/.kiro/profile.json` + Kiro `state.vscdb` | ✓ AWS 文件 + vscdb | Kiro.app（预期可交付） |
| Qoder | `qoder` | machine info/machine token → `qoder.com/device/selectAccounts?…` URL → 轮询 `openapi.qoder.sh/api/v1/deviceToken/poll`（无 user_code，nonce+machine headers）；JSON/本机导入 | `openapi.qoder.sh`（`/api/v1/userinfo` 等，machine headers） | Qoder `state.vscdb`（`User/globalStorage/` 多候选路径，`ensure_state_db_path_for_user_data_dir`） | ✓ vscdb | Qoder.app（预期可交付） |
| Trae ×4 | `trae`,`trae-solo`,`trae-cn`,`trae-solo-cn` | **无浏览器腿**：refresh_token → `…/trae/api/v3/oauth/ExchangeToken`（`ClientID`+`ClientSecret`+`RefreshToken`+`DeviceInfo`+**`DeviceProof` ECDSA-P256 签名**）；本机/JSON 导入为主 | `GetUserInfo`（`/cloudide/api/v3/trae/GetUserInfo`）+ entitlement/usage raw；按 `api.trae.ai`/`trae.cn`/`trae.com.cn`/`marscode.com`/`traeapi.us` 区域路由 | `…/<App>/User/globalStorage/storage.json`：iCube `byte_crypto` AES-128-CBC 加密值（随机密钥内嵌 blob，SHA512 完整性） | ✓ storage.json ×4 | Trae / TRAE SOLO / Trae CN / TRAE SOLO CN（预期可交付） |
| Zed | `zed` | `zed.dev/native_app_signin?native_app_port=<p>&native_app_public_key=<pk>` → 本地监听收 `user_id`+**RSA-OAEP 加密** access_token（一次性 RSA-2048 对）；JSON/本机导入 | `cloud.zed.dev/client/users/me`，`Authorization: "<user_id> <token>"`（**非 Bearer**） | macOS Keychain `security find-internet-password -s https://zed.dev`（account=user_id） | ✓ Keychain 写回（macOS-only） | ✗ Zed.app 无 `--user-data-dir` |
| ZCode | `zcode` | **内嵌 webview**：authorize URL 内导航，拦截 `zcode://oauth/callback` / `zcode://zai-auth/callback`；token 交换 `zcode.z.ai/api/v1/oauth/token`；provider=zai/bigmodel | z.ai `chat.z.ai/api/oauth/userinfo`；bigmodel `open.bigmodel.cn/api/biz/customer/getCustomerInfo`；zcode JWT bearer | `~/.zcode/v2/credentials.json`：AES-256-GCM，key=SHA256(`zcode-credential-fallback:{platform}:{home}:{user}` 或 `ZCODE_CREDENTIAL_SECRET`)；`settings.json` 可改 `dataBaseDir` | ✓ 加密 credentials.json 写回 | ZCode.app（预期可交付） |
| CodeBuddy | `codebuddy` | `POST {api}/api/v1/auth/state?platform=…` → `state`+`authUrl` → 浏览器登录 → **服务端轮询** session（loginSessionId）→ access+refresh；refresh=`/auth/token/refresh`（`X-Refresh-Token` 头，非标准 OAuth） | `POST /v2/billing/meter/get-dosage-notify`、`get-payment-type`、user-resource（`Bearer` + `X-User-Id`/`X-Enterprise-Id`/`X-Domain`） | `…/CodeBuddy/User/globalStorage/state.vscdb` | ✓ vscdb | CodeBuddy.app（预期可交付） |
| CodeBuddy CN | `codebuddy-cn` | 同上，`www.codebuddy.cn` | 同上 .cn 域 | `…/CodeBuddy CN/…state.vscdb` | ✓ vscdb | CodeBuddy CN.app（预期可交付） |

## 易踩坑（实现时用测试钉住）

- **GitHub Copilot**：`copilot_internal` 的 auth scheme 是 `token <gh>`，不是 `Bearer`；copilot token 短效（`refresh_in`），不落库，每次 refresh 现场二级交换。
- **Zed**：`Authorization: "<user_id> <token>"` 自定义 scheme；`/frontend/billing/*` 端点对有效桌面凭据也会 401——配额只走 `/client/users/me`，不要顺手加 billing 端点否则误置 reauth。
- **CodeBuddy**：业务错误在 JSON body 的 `code`/`message` 里（HTTP 200）——不得把业务失败误归 Transient。
- **Trae**：device keypair 与登录绑定——裸 refresh token 粘贴若服务端校验设备绑定会失败，UI 文案须导向本机导入；`DeviceProof` message 拼法（`POST\n<path>\n<clientId>\n<refreshToken>\n<ts>\n<nonce>`）逐字节对齐 cockpit；refresh token 单次使用。
- **Kiro IDC**：`clientId/clientSecret` 必须持久化否则 refresh 无腿 → `platform_token_encrypted` JSON blob；注册缓存路径 `~/.aws/sso/cache/<clientIdHash>.json` 的 hash 算法照抄 `kiro_account.rs` `idc_client_registration_path`。
- **ZCode**：`dataBaseDir` 覆盖必须先读 `settings.json` 再定位 `credentials.json`；`zcode://` 拦截只认注册的 host 白名单，拒绝任意 scheme 导航。
- **Windsurf**：`windsurf_auth-*` 键含本地 usage 缓存——写回只动 auth 键，别清 usage 键。

## `Subscription` 字段映射（不加 provider 私有字段的落法）

| provider | access_token | refresh_token | id_token | platform_token（加密 blob） | oauth_account_id | oauth_region |
| --- | --- | --- | --- | --- | --- | --- |
| github-copilot | GitHub token（长效，每次 refresh 重新二级交换） | — | — | — | github login | — |
| windsurf | authToken/session | firebase refresh（若有） | — | windsurf apiKey | email | — |
| kiro | access token | refresh token | — | IDC 上下文 `{clientId,clientSecret,region,startUrl}` | user id | region |
| qoder | qoder token | 若有 | — | machine `{machineToken,machineId,machineType}` | user id | — |
| trae ×4 | access token | refresh token | — | auth 上下文 `{deviceKeyPair{privateKeyPEM,publicKeyPEM},clientId,loginHost,authDomain}` | user id | loginRegion |
| zed | access token | — | — | — | user_id | — |
| zcode | provider access | provider refresh | zcode JWT | — | user id | `zai`/`bigmodel` |
| codebuddy(-cn) | access token | refresh token | — | enterprise 上下文（如需） | uid | `cn`/`global` |

→ 需要 `platform_token_encrypted` 语义放宽为「provider 私有刷新上下文加密 blob」，
并把该字段纳入 `patch_oauth_credentials`/`patch_fetcher_state` 的窄 patch 范围。

## cockpit-tools 参照文件清单

| provider | 文件 |
| --- | --- |
| github-copilot | `crates/cockpit-core/src/modules/github_copilot_{oauth,account,instance}.rs`、`src-tauri/src/modules/github_copilot_oauth.rs`；`vscode_paths.rs`/`vscode_inject.rs` **不移植**（SafeStorage 注入超出切号最小承诺） |
| windsurf | `crates/cockpit-core/src/modules/windsurf_{oauth,account,instance}.rs` + `windsurf_devin_oauth.rs`；src-tauri 同名更全 |
| kiro | **`src-tauri/src/modules/kiro_oauth.rs`**（IDC device flow）+ `crates/cockpit-core/src/modules/kiro_{account,instance,oauth}.rs` |
| qoder | `crates/cockpit-core/src/modules/qoder_{oauth,account,instance}.rs` |
| trae ×4 | `crates/cockpit-core/src/modules/trae_account_core_{import,injection,platform_storage,product_paths,refresh}.rs` + `trae_oauth.rs` |
| zed | `crates/cockpit-core/src/modules/zed_{oauth,account,instance}.rs` |
| zcode | `src-tauri/src/modules/zcode_{oauth,account,instance}.rs` + `src-tauri/src/commands/zcode.rs` + `src-tauri/src/models/zcode.rs` |
| codebuddy(+cn) | `crates/cockpit-core/src/modules/codebuddy_{oauth,account,instance}.rs` + `codebuddy_cn_{oauth,account,instance}.rs` |
| 唤醒网关 | `/tmp/cockpit-tools/crates/cockpit-core/src/modules/wakeup_gateway.rs` + `src-tauri/src/modules/wakeup_*.rs` |

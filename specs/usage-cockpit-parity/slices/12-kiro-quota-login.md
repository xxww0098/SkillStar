# 12 · kiro — 配额 + 登录 + 导入（L0–L3）

## 解锁的契约

`kiro` catalog 上线：双腿登录（Kiro portal PKCE + AWS IDC device flow）+ TokenImport +
本机导入；配额显示 credits / freeTrial / reset 周期。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` + `identity.rs` | `kiro` 行（tier=OAuth，`[OAuth, TokenImport]`）+ identity |
| 域 | `fetchers/oauth/kiro.rs`（拆 `kiro/{mod,portal,idc,quota}.rs`，参照 cockpit `src-tauri/src/modules/kiro_oauth.rs`——**IDC 腿只在 src-tauri 版**） | portal 腿：`app.kiro.dev/signin` PKCE + loopback（path 白名单 `/oauth/callback`、`/signin/callback`）→ `prod.us-east-1.auth.desktop.kiro.dev/oauth/token`；IDC 腿（flow=`RemotePoll`+user_code）：`oidc.{region}.amazonaws.com/client/register`→device authorization→轮询 `/token`（device_code grant，尊重 `interval`/`slow_down`/`authorization_pending`）；回调参数 `loginOption`=`builderid/awsidc/internal` 无 code 时给明确报错文案 |
| 域 | quota | `q.{region}.amazonaws.com` CodeWhisperer runtime：`getUsageLimits?profileArn` 等，多路径 credits 解析（`estimatedUsage`/`usageBreakdowns`/`freeTrialInfo`/`resetOn`） |
| 域 | `local_import` dispatch + kiro import | 读 `~/.aws/sso/cache/kiro-auth-token.json` + IdC 注册缓存 `~/.aws/sso/cache/<clientIdHash>.json`（hash 算法照抄 cockpit `idc_client_registration_path`）+ `~/.kiro/profile.json` + Kiro `state.vscdb` |

## 字段落法

`access_token_encrypted`/`refresh_token_encrypted` 标准槽；
`provider_state_encrypted`=`{idc:{clientId,clientSecret,region,startUrl}, profileArn}`——
**IDC 注册不持久化则 refresh 无腿**；`oauth_region`=region；`oauth_account_id`=user id。

## 人能看见

Kiro 卡选「社交登录（浏览器）」或「AWS IDC（复制验证码→打开页面→等待）」——
第一片消费切片 02 的 RemotePoll 面板。

## 验证

- device flow mock 测试：register→device→poll 序列；`authorization_pending`/`slow_down` 语义。
- region 三级回落测试（callback→profile arn→默认 us-east-1）。
- 本机导入 fixture：临时 `.aws/sso/cache` 双文件 → 建行。
- 真实联调一次（人工记录两条腿各自的通过/失败）。
- **视觉门禁**：device 面板 + 卡片截图 → screenshot-critique。

## 委托给实现者的决定

- portal/IDC 双腿在 UI 上的呈现顺序与默认项。

## 必须保持绿

- IDC `/token` 是非标准 JSON token 腿：实现放 kiro.rs，错误分类语义与 `post_token` 对齐（`invalid_grant`→AuthRequired）。

## 会改变本片的人类反馈

- 若只想要 portal 腿（砍掉 IDC）——说，本片可减半。

## 结果

`kiro` 已进 catalog（OAuth + TokenImport，品牌色 `14B8A6`，续费页 `https://app.kiro.dev/signin`）和 identity。配额、两条登录腿、令牌导入、本机导入都在 `fetchers/oauth/kiro/`。

测试里两条腿都走到了，没有打真实 AWS：

- Portal / Builder ID：PKCE 授权 URL、`/oauth/callback` 与 `/signin/callback` 白名单、`loginOption=builderid|awsidc|internal` 无 code 的明确报错、本地监听、JSON `/oauth/token` 的 `invalid_grant` → AuthRequired。`start_login` 默认 `LocalCallback`。
- AWS IDC：mock 上 `client/register` → `device_authorization` → `/token`。`authorization_pending` 保持间隔，`slow_down` +5s，然后发 token。`OAuthFlow::RemotePoll` 带 user code。403 不是 AuthRequired；401 / `invalid_grant` 是。

本机导入在 `SKILLSTAR_TOOL_SYNC_HOME` 下读 `~/.aws/sso/cache/kiro-auth-token.json` 和 `<clientIdHash>.json`，并在存在时读 `profile.json` 与 `state.vscdb`。令牌导入接受 refresh token 或 JSON，拒绝垃圾。IDC 的 `clientId` / `clientSecret` / `region` / `startUrl`（以及有则带上的 `profileArn`）在 `provider_state` 明文 JSON 里，由导入管线加密。不写 `platform_token_encrypted`。

注册缓存路径照抄 cockpit `idc_client_registration_path`：40 位小写 hex，否则拒绝，不扫描目录。文件名哈希是 cockpit `compute_idc_client_id_hash`：start URL 的 SHA-1。目录是 `tool_paths::aws_sso_cache_dir()`。

真实登录未联调。

静默决定：

- 默认腿是 portal（社交登录）。`idc` / `aws-idc` 或 AWS region（如 `eu-central-1`）才走 device flow。卡片上 portal 在前。
- IDC 只用 Builder ID start URL `https://view.awsapps.com/start`，没有 Enterprise URL 输入。
- `provider_state` 是扁平 JSON，不是 `{idc:{...}}`。
- Credits 进 monthly 窗口，Free trial 进 weekly；缺字段就不建窗口。`resetOn` 只挂到已有窗口。
- Portal token 是 JSON；IDC `/token` 是 src-tauri 的 form snake_case。响应同时认 camelCase 和 snake_case。
- OIDC 客户端名是 `SkillStar Kiro`。不探测 `mwinit`。回调成功页是本地 200，不 302 回 portal。
- 裸 refresh token 记下 region `us-east-1`。没有 client 凭据时刷新走 portal `refreshToken`。`oauth_account_id` 是 user id，邮箱只做标题。

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

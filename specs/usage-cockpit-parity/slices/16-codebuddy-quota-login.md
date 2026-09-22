# 16 · codebuddy + codebuddy-cn — 配额 + 登录 + 导入（L0–L3）

## 解锁的契约

`codebuddy`（`www.codebuddy.ai`）与 `codebuddy-cn`（`www.codebuddy.cn`，D13 已裁决非
WorkBuddy）两条目共享一套参数化实现；服务端 state 轮询登录（flow=`RemotePoll`）+
TokenImport + 本机导入；配额显示套餐/用量/加量包。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` + `identity.rs` | 两行：`codebuddy`/`codebuddy-cn`（各自固定 domain，`oauth_region` 存 `global`/`cn`） |
| 域 | `fetchers/oauth/codebuddy.rs`（domain 参数表集中管理，**两端差异只许出现在表里**） | `POST {api}/api/v1/auth/state?platform=…` → `state`+`authUrl` → 浏览器登录 → 轮询 `auth/token?state=`（loginSessionId）→ access+refresh；refresh=`/auth/token/refresh`（**`X-Refresh-Token` 头 + `X-Auth-Refresh-Source: ide-main`，非标准 OAuth**）；quota=`/v2/billing/meter/get-dosage-notify` + `get-payment-type` + user-resource（`Bearer`+`X-User-Id`/`X-Enterprise-Id`/`X-Domain` 头） |
| 域 | 本机导入 | 两变体 `…/CodeBuddy [CN]/User/globalStorage/state.vscdb` 读 auth 键 |

## 易踩坑（钉测试）

- **UA 头必须**：缺 UA → 403/`code=10085`。
- **业务错误在 body**：HTTP 200 但 `code`/`message` 报错——映射 `Fetcher`，不得误归 Transient/AuthRequired。
- 企业账号头（`X-Enterprise-Id`/`X-Tenant-Id`）若参与配额，存 `provider_state_encrypted`。

## 字段落法

`access_token_encrypted`/`refresh_token_encrypted` 标准槽；`provider_state_encrypted`=企业上下文（如需）；`oauth_account_id`=uid；`oauth_region`=`cn`/`global`。

## 人能看见

两张 CodeBuddy 卡（全球/CN）；RemotePoll 面板显示 authUrl+等待；本机导入读已登录客户端。

## 验证

- auth/state→poll mock 测试；refresh 头断言；body-内 `code` 业务错误分类测试（**多 response shape 全覆盖 parser fixture**）。
- 两 domain 参数表一致性测试（除表内字段外代码路径同一）。
- 真实联调一次。
- **视觉门禁**：卡片截图 → screenshot-critique。

## 委托给实现者的决定

- response shape 漂移的解析兜底粒度。

## 必须保持绿

- 非标准 refresh 腿（`X-Refresh-Token`）错误分类与 `post_token` 语义对齐。

## 会改变本片的人类反馈

- 无。

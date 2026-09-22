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

## 结果

`codebuddy` 与 `codebuddy-cn` 已进 catalog（OAuth + TokenImport，品牌色 `6C4DFF`，续费页分别是 `https://www.codebuddy.ai` 与 `https://www.codebuddy.cn`）和 identity。配额、RemotePoll 登录、令牌导入、本机导入都在 `fetchers/oauth/codebuddy/`。CN 不是第二份模块：差异只在 `Host` 表（origin、`oauth_region`、secret 键、`state.vscdb` 路径函数）。两条 catalog 走同一套 `endpoint` / 请求 / 解析。Logo 用 lobe 已导出的 `CodeBuddyColor`。登录面板没有 user code：`OAuthStartInfo::remote_poll`，`interval_secs` 为 2。

测试里登录、刷新和配额都走到了，没有打真实 `www.codebuddy.ai` / `www.codebuddy.cn`：

- 授权：`POST {origin}/v2/plugin/auth/state?platform=ide`，空 JSON，带 `X-No-Authorization` / `X-No-User-Id` / `X-No-Enterprise-Id` / `X-No-Department-Info` 和浏览器 `User-Agent`。响应取 `data.state` + `data.authUrl`（也接受 `auth_url` / `url`）。没有 authUrl 时回退 `{origin}/login?state=`。浏览器 URL 追加 `loginSessionId`，不探测客户端版本。
- Poll：`GET {origin}/v2/plugin/auth/token?state=`。先 200 且业务 `code` 不是成功、没有 access token，继续等；随后 200 带 token 完成。超时文案是「CodeBuddy 登录已超时」；取消是「用户取消登录」。401 / `error=invalid_grant|invalid_token` 立刻 AuthRequired。HTTP 200 且 `code=10085` 是 Fetcher（缺 UA）。其它 200 业务码在 poll 上保持等待，与 cockpit 的轮询循环一致。403 不是 AuthRequired。429/5xx 先记住，到截止时间仍以 Transient 结束。
- Refresh：`POST {origin}/v2/plugin/auth/token/refresh`，无 form body。头是 `Authorization: Bearer`、`X-Refresh-Token`、`X-Auth-Refresh-Source: ide-main`，有 domain 时再加 `X-Domain`。错误分类与 `post_token` 对齐：401 / `invalid_grant` / `invalid_token` → AuthRequired；429/5xx/传输失败 → Transient；其它非 2xx（含 403）→ Fetcher。HTTP 200 且业务 `code` 不是 0/200/ok/success → Fetcher，不是 Transient，也不是 AuthRequired（即使 `code` 字符串是 `invalid_grant`）。2xx 但没有 access token → AuthRequired。有 refresh token 的配额刷新会先走这条腿，失败则整次刷新失败，不吞掉。
- 配额：`POST /v2/billing/meter/get-dosage-notify`、`get-payment-type`，个人账号再 `get-user-resource`（body 含 `ProductCode=p_tcaca`、`Status=[0,3]`）。有 `enterpriseId` 时改为 `get-enterprise-user-usage`，再折成同一套资源包解析。头是 `Bearer`，以及存在时的 `X-User-Id` / `X-Enterprise-Id` / `X-Tenant-Id`（与 enterprise id 相同，不另存）/ `X-Domain`。缺字段不发空头。每个请求都带同一个 Mozilla UA。
- 窗口：一个 `monthly`。主包优先 Pro，然后 Basic、Enterprise、其它、Add-on；其余包进 breakdown。已知套餐码映射为 Usage / Add-on / Basic / Pro / Enterprise / Activity（中英 i18n）。没有 used 就不建窗口，不补 0；接口给出的 0 保留。`limit=-1` 或 `Unlimited` 只显示 used，不写 total。Status 不是 0/3 的包省略。套餐名优先 payment type，其次包标签，最后 dosage 文案。
- 令牌导入接受账号 JSON、`{accounts:[...]}` / `{items:[...]}` 的第一条可用记录，也接受至少 20 字符、无空白、含字母的 token 字符串，以及 `uid+token`。垃圾拒绝。
- 本机导入走 `tool_paths::codebuddy_state_db_path()` 与 `codebuddy_cn_state_db_path()`。键是 `secret://{"extensionId":"tencent-cloud.coding-copilot","key":"planning-genie.new.accessToken"}`，CN 的 key 是 `planning-genie.new.accessTokencn`。`secret://` 只用注入的 `KeyMaterial` 解密；没有密钥时报错并写明不会读系统钥匙串。测试把 `SKILLSTAR_TOOL_SYNC_HOME` 指到临时目录，不碰真实 home。

`provider_state` 是明文 JSON `{enterpriseId, enterpriseName, domain}`，由登录和导入管线加密。不写 `platform_token_encrypted`。`oauth_account_id` 是 uid。`oauth_region` 在建卡时写成 `global` 或 `cn`，catalog 的 `regions` 为空，用户不用选。access / refresh 走标准加密槽。

真实登录未联调。

静默决定：

- 切片简写 `{api}/api/v1/auth/state` 与 cockpit 不符。实现跟 cockpit：auth 在 `/v2/plugin`，配额在 `/v2/billing/meter`。仓库里没有 `api/v1` 这条路径。
- 两端共用一条浏览器 UA，不复制 cockpit 里两段略有差别的 Chrome 字符串。契约是必须带 UA。
- Poll 上非 10085 的 HTTP 200 业务码继续等；auth/state、refresh、配额上同一形状是 Fetcher。没有 `code` 字段时，单靠 `message` 不把 HTTP 200 判失败。
- 不读 `product.json`，登录 URL 不加 `version`。
- 企业用量走 `get-enterprise-user-usage`。`X-Tenant-Id` 等于 enterprise id。user-resource 也带 `X-Domain`（cockpit 那个私有 helper 丢了这个头，切片要求有就带）。
- 配额三条腿是严格的：任一条业务错误失败整次刷新。登录路径上，配额 AuthRequired 不保存卡片；其它配额错误仍保存刚换到的 token。
- 用量只做一个 monthly 窗口加 breakdown，不拆 5h/7d。不实现 CN 签到。
- 两端默认货币都是 USD，与其它 IDE OAuth 行一致。
- 生产本机导入不会读系统钥匙串。加密的 `secret://` 必须由调用方注入 `KeyMaterial`；当前 UI 入口传的是 `None`，因此真实客户端密文在接到钥匙串注入之前会明确失败，而不是静默去读钥匙串。
- `patch_fetcher_state` 不写 `oauth_region`。region 在登录和导入插入整行时写入。
- CN 卡片用 lobe 渐变里的青色停点，和全球卡的紫色停点区分；图标仍是同一个 `CodeBuddyColor`。

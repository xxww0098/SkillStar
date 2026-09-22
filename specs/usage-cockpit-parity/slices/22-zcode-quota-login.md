# 22 · zcode — 配额 + 登录 + 导入（L0–L3）

> 依赖：02（`SchemePaste` flow）+ 08（enc:v1 解密，仅本机导入需要）。

## 解锁的契约

`zcode` catalog 上线：Z.ai/BigModel 双上游 OAuth（**SchemePaste**——浏览器授权后浏览器跳到
`zcode://…` 打不开属正常，用户粘贴该 URL，provider 进程内解析 `code`+`state`）+
TokenImport（oauth token 或 api key）+ 本机导入（credentials.json 解密）；
配额显示订阅套餐 + 按模型额度。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` + `identity.rs` | `zcode` 行，`regions=&["zai","bigmodel"]`（region 参数承载双上游选择，复用既有 region UI）+ identity |
| 域 | `fetchers/oauth/zcode.rs` | `start_login`：按 region 构造 authorize URL（zai：`chat.z.ai/api/oauth/authorize`→`zcode://zai-auth/callback`；bigmodel：`bigmodel.cn/login`→`zcode://oauth/callback`），flow=`SchemePaste{scheme_prefix:"zcode://"}`；`submit_callback_input`：校验 scheme/path/state 白名单（只认 `oauth/callback`、`zai-auth/callback`）→ manual inbox → worker 用 `zcode.z.ai/api/v1/oauth/token` 换 token（**不发 HTTP 到 zcode://**）；quota：z.ai `chat.z.ai/api/oauth/userinfo`、bigmodel `open.bigmodel.cn/api/biz/customer/getCustomerInfo` + `zcode-plan/billing/balance`（需 zcode JWT） |
| 域 | `local_import` | `zcode_home()`（先读 `settings.json` 的 `dataBaseDir`）→ `credentials.json` `enc:v1:` 解密（08 成果） |
| 域 | `token_import.rs` zcode 腿 | oauth token 或 api key 分流（api key 写 `api_key_encrypted`，`provider_state_encrypted.kind` 区分） |

## 字段落法

`access_token_encrypted`=provider access；`refresh_token_encrypted`=provider refresh；
`id_token_encrypted`=zcode JWT；`oauth_region`=`zai`/`bigmodel`；`oauth_account_id`=user id。

## 人能看见

ZCode 卡：选上游（zai/bigmodel）→登录→浏览器跳转后粘贴 `zcode://` URL→自动完成；
本机导入读 `~/.zcode/v2/credentials.json`。

## 验证

- SchemePaste 白名单测试：合法 `zcode://oauth/callback?code&state` 通过；非白名单 host/错误 scheme/篡改 state 拒绝。
- 双上游 token 交换 mock；quota 双端点 fixture。
- 本机导入 fixture（08 的加密向量）。
- 真实联调一次。
- **视觉门禁**：SchemePaste 面板截图 → screenshot-critique。

## 委托给实现者的决定

- `zcode://` 粘贴框的 UX 文案（解释「浏览器打不开这个链接是正常的，复制过来」）。

## 必须保持绿

- `manual_callback` loopback-only 校验**不放宽**——SchemePaste 是独立分流（02 已建好）。
- api key 与 oauth token 的存储分流忠实 cockpit 语义（不同字段不同文件）。

## 会改变本片的人类反馈

- 内嵌 webview 自动拦截 `zcode://`（Tauri `on_navigation`）作为可选增强——本片完成后由你拍板是否立项；基线不靠它。

## 结果

SchemePaste 登录、双上游换票、配额、令牌导入和 `enc:v1` 本机导入已接上 catalog `zcode`。没有打真实 z.ai / BigModel，也没有读 `~/.zcode`。官方是否接受这套回调和配额：**未验证**。不降级。没有做内嵌 webview。

接受的回调 host（cockpit `is_zcode_callback_url`，host + path，不是空 host 下的路径）：

- `zcode://oauth/callback`（bigmodel）
- `zcode://zai-auth/callback`（zai）

其它 host、其它 path、`https://`、以及 `zcode:///oauth/callback` 都拒绝。host 还要和这次登录的上游一致，state 必须等于 authorize URL 里的 state。`code` 与 `authCode` 都可以。粘贴走 manual inbox，不对 `zcode://` 发 HTTP。

换票是 `POST https://zcode.z.ai/api/v1/oauth/token`。zai 再 `POST https://api.z.ai/api/auth/z/login`，落库的 access token 是业务 token，不是信封里的 oauth access。JWT 放 `id_token_encrypted`。`oauth_region` 是 `zai` 或 `bigmodel`。

配额：zai `GET chat.z.ai/api/oauth/userinfo` 用 `Bearer` access token；bigmodel `GET open.bigmodel.cn/api/biz/customer/getCustomerInfo` 的 `Authorization` 是原始 access token（没有 `Bearer`）。额度是 `GET https://zcode.z.ai/api/v1/zcode-plan/billing/balance`，`Bearer` zcode JWT。数字窗口只来自 `balances`；缺 `used` 就不画窗口，不补 0。多个带名字的 balance 进 breakdown。

颜色：`000000`，与 lobe ZAI `COLOR_PRIMARY` 相同。卡片图标用已有的 `ZAIMono`，不是字母兜底。

静默决定：

- 未选 region 时默认 `zai`。
- 登录超时 300 秒，和 cockpit 一样。一次粘贴失败就结束这次登录，不等下一次。
- z.ai token 信封必须是整数 `code == 0`；bigmodel 允许没有 `code` 或 `0`，`200` 不算成功。业务 token 接受 `0` / `200` / 空 / `"0"` / `"200"`。billing 必须是整数 `0`。HTTP 200 的业务 `code` 失败是 Fetcher。纯 403 是 Fetcher，不是 auth。401 和 `invalid_grant` / `invalid_token` 才是 auth。成功信封里缺少 provider access token 是 auth；缺少 `data.token` 是 Fetcher。
- 配额请求不写 device mid，也不调用 `sw_vers`。`app_version` 固定 cockpit 默认 `3.10.2`。
- 凭据密钥的 home 是操作系统 home；测试里 `SKILLSTAR_TOOL_SYNC_HOME` 代替它。`dataBaseDir` 只改 `credentials.json` 的位置，不进密钥。文件名仍是 `setting.json`。
- 本机导入只读 oauth `credentials.json`，不读 `config.json` 里的 API key，也不为了导入去打网络。API key 走令牌 JSON：`api_key_encrypted` + `provider_state.kind = api_key`，`auth_mode` 仍是 TokenImport。一次粘贴只收一个账号。
- 浏览器打不开 `zcode://` 的说明写在 SchemePaste 面板上。开始登录后会打开 https 授权页，并仍可复制链接。

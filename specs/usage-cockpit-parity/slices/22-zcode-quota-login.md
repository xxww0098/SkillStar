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

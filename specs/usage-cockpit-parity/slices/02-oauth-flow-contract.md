# 02 · OAuthFlow 完成契约（四态）

## 解锁的契约

OAuth 完成机制成为显式多态：`OAuthStartInfo.flow` 声明本次登录如何完成，前端按 flow
渲染而不是按 catalog_id 分支。四种形态覆盖 cockpit 全部现实：

| flow | 含义 | 谁会用 |
| --- | --- | --- |
| `LocalCallback` | loopback 监听器就位；粘贴的回调 URL 经 HTTP 重放兜底（现状） | 现有全部 + copilot/windsurf/kiro-portal/trae/zed |
| `RemotePoll` | 后端轮询 provider 端点，用户没有可粘贴物 | qoder、codebuddy×2、kiro IDC device |
| `SchemePaste` | 自定义 scheme 回调（`zcode://`），provider 进程内解析参数，不发请求 | zcode |
| `Immediate` | 就地完成（本机凭据采纳） | anthropic（现状语义） |

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `crates/skillstar-usage/src/fetchers/oauth/start_info.rs` | `pub enum OAuthFlow { LocalCallback, RemotePoll, SchemePaste{scheme_prefix}, Immediate }`（ts-rs 导出）；`OAuthStartInfo` 加 `flow` + `user_code: Option<String>` + `verification_uri: Option<String>` + `interval_secs: Option<u32>`；`browser()` 保留=LocalCallback；新增 `device(...)`/`remote_poll(...)`/`scheme_paste(...)` 构造器 |
| 域 | `crates/skillstar-usage/src/oauth/pending_state.rs` | `PendingLogin` 加 `flow` + `manual_inbox_tx: Option<mpsc::UnboundedSender<String>>`（SchemePaste 的投递通道）；`register` 旧签名默认 `LocalCallback` |
| 域 | `crates/skillstar-usage/src/oauth/local_server.rs` | `wait()` 返回值从 `code: String` 泛化为参数 map（zed 回调带 `user_id`+加密 token、windsurf 带 fragment token、copilot 经 `state` 带回 loopback URL）；既有调用方取 `code` 键 |
| 域 | `crates/skillstar-usage/src/oauth/manual_callback.rs` | 保持 loopback-only 校验；新增 `deliver_manual_input(pending_id, input)` 按 `PendingLogin.flow` 分流：LocalCallback→现有重放（先过 provider 的 `normalize_callback_input`，默认恒等）；SchemePaste→格式校验后送 `manual_inbox`；RemotePoll/Immediate→明确报错「此登录无需手动回调」 |
| 域 | `crates/skillstar-usage/src/fetchers/oauth/mod.rs` | `submit_callback(pending_id, input)` 分发入口；provider 可注册 `normalize_callback_input`（copilot 用它把 `vscode.dev/redirect?state=<localhost-url>` 解开）与 `submit_callback_input`（zcode 解析 zcode://） |
| app | `crates/skillstar-app/src/usage/dto.rs` + service | `OAuthStartDto` 加 `flow`/`user_code`/`verification_uri`/`interval_secs`；`submit_oauth_callback` 改走新分发 |
| 前端 | `components/subscriptionEdit/oauth/OAuthLoginPanel.tsx` | 按 `flow` 四态渲染：LocalCallback=现状三步；RemotePoll=device 面板（user_code 大字+复制+verification_uri 链接+倒计时+等待动画，**无粘贴框**）；SchemePaste=粘贴完整 URL 框+格式即时校验；Immediate=不渲染 |

## 人能看见

既有 provider 行为完全不变；devMock 给 `start_oauth_login` 加 `flow` 字段后可在
无后端情况下看到四种面板形态——**本片的人可玩检查点 = 前端面板四态预览**。

## 验证

- pending_state 单测：flow 往返、manual inbox 收发、cancel/timeout 清理、旧 `register` 签名默认 LocalCallback。
- `deliver_manual_input` 分流测试：RemotePoll/Immediate 拒绝、SchemePaste 格式校验失败即时报错、LocalCallback 走重放。
- `local_server::wait` 参数 map 化后 codex/cursor/xai 现有测试回归。
- 前端 panel 四态组件测试 + i18n key。
- **视觉门禁**：四态面板截图 → screenshot-critique 终审。
- `bun run types:gen` + `cargo test -p skillstar-usage -p skillstar-app`。

## 委托给实现者的决定

- 参数 map 的具体类型（`HashMap<String,String>` 或小型 struct）。
- `manual_inbox` 用 `mpsc::UnboundedSender` 还是 oneshot 数组（zcode 只需一次投递）。

## 必须保持绿

- `manual_callback` 的 loopback-only 校验与既有测试（拒绝非本机 host）一字不动。
- anthropic 的 `Immediate` 语义只是显式化，行为不变。

## 会改变本片的人类反馈

- 若 device/poll 面板想要不同的 UX 结构（如二维码），在 review 截图时提。

## 结果

OAuth 完成方式变成显式四态。现有浏览器登录仍是 `LocalCallback`，行为不变；anthropic 采纳本机凭据标成 `Immediate`。没有 `expires_in_secs`。

落地：

- `OAuthFlow` 从 `start_info.rs` 用 ts-rs 导出。serde 外部标签 + kebab-case：`"local-callback"` / `"remote-poll"` / `"immediate"` 是字符串，`SchemePaste` 是 `{"scheme-paste":{"scheme_prefix":"..."}}`。
- `OAuthStartInfo` 与 `OAuthStartDto` 增加 `flow`、`user_code`、`verification_uri`、`interval_secs`。`browser()` 仍是 LocalCallback。构造器：`device`、`remote_poll`、`scheme_paste`，另加 `immediate`。
- `PendingLogin` 增加 `flow` 与 `manual_inbox_tx`。`register` / `register_with_callback_port` 签名不变，默认 LocalCallback。`register_scheme_paste` 把接收端交回调用方。`register_with_flow` 用于 RemotePoll / Immediate。
- `local_server::wait` 返回 `CallbackParams`（`HashMap<String, String>` 的别名，避免 `Result<HashMap<String, String>>` 误伤错误字符串棘轮）。`wait_for_callback` 仍返回 `code` 字符串，xai / antigravity 不动。codex 从 map 读 `code`。未改 `cursor.rs`。
- `deliver_manual_input` 按 flow 分流。LocalCallback 先过 `fetchers/oauth/mod.rs` 的 `normalize_callback_input`（空函数指针表，缺省恒等，本片没有 provider 登记）再走原来的 loopback 重放。SchemePaste 校验前缀后送 inbox。RemotePoll / Immediate 报「此登录无需手动回调」。非本机 host 仍拒绝。
- `submit_oauth_callback` 改走 `fetchers::oauth::submit_callback`。
- `OAuthLoginPanel` 按 `flow` 渲染：LocalCallback 保持原来三步；RemotePoll 是用户码、复制、验证页链接、`interval_secs` 倒计时、等待态，没有粘贴框；SchemePaste 是一个 URL 文本框，前缀不对就拒绝提交；Immediate 不渲染。
- i18n 加在 `usage.oauth*`。devMock `start_oauth_login` 默认 `flow: "local-callback"`，并接受 `flow` 参数预览另外三态，不新增 catalog。

测试：

- `cargo test -p skillstar-usage --locked --lib -- oauth::`：107 passed（含 pending_state、local_server、manual_callback，以及 codex / xai / antigravity）。
- `cargo test -p skillstar-app --locked --lib usage::`：36 passed。
- `bun run types:gen` 与 `bash scripts/internal/check_generated_types.sh` 通过。生成物只有 `OAuthFlow.ts` 与 `OAuthStart.ts`。
- `bun run test -- src/features/usage`：144 passed，其中面板四态 5 个。

静默决定：

- 参数 map 的具体类型是 `CallbackParams` = `HashMap<String, String>`，只收 query。fragment 仍由 `manual_callback` 在重放前并进 query；浏览器不会把 fragment 送到 loopback。
- 监听成功仍要求 `code` + 匹配的 `state` + 没有 `error`。没有 `code` 的请求继续等，不结束本次登录。无 code 的 zed 回调留给后面的片放宽。
- inbox 用规格点名的 `mpsc::UnboundedSender`，发送后不取走，改错的粘贴可以再发。zcode 只需要收一次。
- `device()` 是带 user_code、verification_uri（同时写入 `auth_url`）和 interval 的 RemotePoll。`remote_poll()` 是没有用户码的 RemotePoll。
- 规格的构造器列表没有 `immediate()`，但 anthropic 必须显式标 Immediate，所以加了。
- 超时清理仍是 worker 把超时送进 completion、await 侧 `remove`。没有新的扫表。cancel 仍删会话、丢掉 inbox、叫醒等待方。
- SchemePaste 先 trim 再对前缀做 `starts_with`，送出的是 trim 后的字符串。空前缀直接当格式错误。
- start 之前 catalog 不声明 flow，空闲面板仍是现在的本地三步。start 返回后才按 flow 画。
- 倒计时是轮询间隔 `interval_secs`，归零后按同一间隔重计，不是会话过期。
- devMock 的 `await_oauth_completion` 不 resolve，浏览器预览里面板不会一闪就关。`submit_oauth_callback` / `cancel_oauth_login` 是空操作。
- 四态由组件测试锁住。本轮没有跑截图终审。

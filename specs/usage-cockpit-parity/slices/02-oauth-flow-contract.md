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

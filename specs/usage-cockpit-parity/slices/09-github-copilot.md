# 09 · github-copilot — 配额 + OAuth + TokenImport（L0–L2）

> 第一个新 provider：跑通切片 01/02 接缝的最小竖切。**无本地 IDE 凭据 → 无切号（D9）。**

## 解锁的契约

`github-copilot` catalog 上线：OAuth（浏览器）与 TokenImport 双入口，配额卡片显示
Inline Suggestions / Chat messages 用量与重置时间，plan 徽章识别
Free/Individual/Pro/Business/Enterprise。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `catalog.rs` | 新行：`github-copilot`，tier=OAuth，`auth_modes=&[OAuth, TokenImport]`，brand_color=`24292F`，subscription_url=`https://github.com/settings/copilot` |
| 域 | `crates/skillstar-core/src/providers/identity.rs` | canonical `github-copilot`，`catalog_id=Some("github-copilot")`，`preset_ids=&[]` |
| 域 | `fetchers/oauth/github_copilot.rs`（新） | `start_login`：GitHub `login/oauth/authorize` + PKCE，`redirect_uri=https://vscode.dev/redirect`、`state=<本地回调URL>`（cockpit `client_id=01ab8ac9400c4e429b23`）；`normalize_callback_input`：粘贴的 vscode.dev URL → 解 `state` → 重放 localhost URL（走 02 的接缝）；`finalize`：code→token→`fetch_github_user`/`emails`→建卡（`access_token_encrypted`=GitHub token 长效；copilot token 短命不落库）；`fetch`：`token <gh>` scheme 调 `copilot_internal/v2/token` → 用 copilot token 调 `copilot_internal/user` 取 `quota_snapshots`/`limited_user_quotas` |
| 域 | `fetchers/oauth/mod.rs` | dispatch/start_login 加 `"github-copilot"` arm |
| 域 | `token_import.rs` | copilot 腿：`build_from_github_token`——裸 `gho_`/`ghp_`/`github_pat_` 或 JSON，验证后建行 |
| 前端 | `brandThemes.ts` + `ProviderLogo` + i18n + devMock | 品牌色/图标/i18n key/mock 行 |

## 字段落法

`access_token_encrypted`=GitHub OAuth token；`oauth_account_id`=github login；
`provider_state_encrypted` 暂不需要（copilot token 每次 refresh 现场二级交换）。

## 人能看见

Usage 页出现 GitHub Copilot；对话框 OAuth 走浏览器→配额出现；TokenImport 粘贴 token→配额出现。

## 验证

- fetcher mock 测试：`token <gh>` scheme 断言（**不是 Bearer**，钉住）；`copilot_internal` 401→`AuthRequired`、429→`Transient`；quota_snapshots 缺字段→对应窗口缺失而非整卡失败（沿用 anthropic 粒度先例）。
- vscode.dev `state` 解包归一化测试。
- TokenImport：裸 token/JSON/坏 JSON 三态。
- catalog 计数测试 +1；identity conformance 自动覆盖。
- 真实联调一次（人工记录：OAuth 登录→quota 条渲染）。
- **视觉门禁**：卡片截图 → screenshot-critique。

## 委托给实现者的决定

- quota_snapshots 内部字段的取舍（展示哪些 quota 桶）。

## 必须保持绿

- 三态错误表；refresh 窄 patch；既有 provider 回归。

## 会改变本片的人类反馈

- 若想要 VS Code profile 注入（实例侧 copilot 多开）——见切片 24 的可选项，不占本片。

## 结果

`github-copilot` 进 catalog（OAuth + TokenImport，`24292F`，USD，`https://github.com/settings/copilot`）。identity `canonical_id=github-copilot`，`preset_ids` 空。无切号、无实例。真实浏览器 OAuth 未联调，不降级。

落地：

- 登录：`login/oauth/authorize` + PKCE，`client_id=01ab8ac9400c4e429b23`，`redirect_uri=https://vscode.dev/redirect`，`state` 是本机 `http://127.0.0.1:{port}/callback?nonce=…`。`start_login` 走现有 loopback，返回 `LocalCallback`。
- `normalize_callback_input` 只改 `https://vscode.dev/redirect`：按 vscode.dev 的 302 把 `state` 解成本机 URL，并带上外层 `code`、把 `state` 再写回 query，这样现有 listener 的 `code` + `state` 校验能收。其它粘贴原样返回。
- code 交换是带 `Accept: application/json` 的 form POST（GitHub 默认 body 不是 JSON，`post_token` 设不了这个头），错误分类走 `parse_token_body`。长效 GitHub token 进 `access_token_encrypted`；`oauth_account_id` 是 github login；不写 `provider_state_encrypted`，不落短命 copilot token。
- 配额：`Authorization: token <github>` 调 `copilot_internal/v2/token`，再用返回的 copilot token 以 `Bearer` 调 `copilot_internal/user`。
- TokenImport：裸 `gho_` / `ghp_` / `github_pat_`，或 JSON 里的 `github_access_token` / `access_token` / `token`。坏 JSON 和随机字符串拒绝，错误文案不回显粘贴。

配额字段：

- `quota_snapshots.completions` → Inline Suggestions；`chat` → Chat messages；`premium_models` 优先，否则 `premium_interactions` → Premium requests。用量是 `entitlement - remaining`，百分比是 `100 - percent_remaining`。`unlimited` 或负 entitlement 画成 0% 已用。entitlement ≤ 0，或没有 entitlement 且 `has_quota=false`，该行省略。
- `quota_snapshots` 对象缺席时，才用 `limited_user_quotas.completions` / `chat`，总量取 copilot token 的 `cq` / `tq`。快照在场时，缺的键不回退到 limited。
- 重置时间：`quota_reset_date_utc`，否则 `quota_reset_date`，否则 `limited_user_reset_date`。卡片是一个月度 `Copilot` 窗口，桶在 breakdown 里；缺桶只少一行，不让整卡失败。
- plan：`copilot_plan`，否则 token `sku`，否则 token 串里的 `sku=`。`free*` → Free，`individual` / `monthly_subscriber` → Individual，`pro` / `individual_pro` → Pro，`business` → Business，`enterprise` → Enterprise。未识别的字符串做成 Title Case，不丢。

测试：

- `cargo test -p skillstar-usage --locked --lib -- github_copilot catalog:: token_import`：31 passed。
- 同包 `normalize_callback_input_is_identity` 与 `start_login_is_refused`：2 passed。
- `cargo test -p skillstar-core --locked --lib -- identity`：6 passed。
- `bun run test -- src/features/usage`：145 passed。

静默决定：

- vscode.dev 粘贴不只返回裸 `state`。对照过一次无登录的 `vscode.dev/redirect` 302：它会把 `code` 接到 loopback，并把原来的 `state` 写回 query。归一化照这个形状，否则现有 listener 会一直等 `code`。
- 用户信息这一跳用 `Bearer <copilot token>`。`token <github>` 只钉在 `v2/token`。`/user` 与 `/user/emails` 用 GitHub `Bearer`，只为拿 login / 主邮箱。
- 快照在场时以快照为全集；limited 只在没有 `quota_snapshots` 时补 Inline 和 Chat。Premium 是第三行。
- `individual` 显示 Individual，不并进 Pro。
- user 端点的 403 以及其它非 401、非瞬时的 Fetcher 不让整卡失败，有限配额和 sku 仍可画。401 仍是 AuthRequired，429/5xx/传输仍是 Transient。token 端点的 403 是 Fetcher，不是 auth。
- 登录时配额失败仍保存 GitHub 账号（与 Codex 相同）。邮箱 401/403/404 不中断登录。
- 卡片标题优先已验证主邮箱，否则 github login。client id / secret 固定为 VS Code 公开客户端，没有 env 覆盖。
- 父窗口的已用百分比取可见桶里的最大值。

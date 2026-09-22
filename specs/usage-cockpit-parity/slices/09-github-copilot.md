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

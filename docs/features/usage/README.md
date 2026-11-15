# Usage（额度读取）

状态：active

本文件是额度能力的契约：读谁、怎么读、失败怎么分类。账号的记录、存储、登录、切号以及新增受管 Agent 的完整改动清单见
[Accounts](../accounts/README.md#新增受管-agent-统一配方recipe)。

## 所有权

额度抓取作为 `crates/ss-accounts` 的内部私有模块 `usage`（[`crates/ss-accounts/src/usage/`](../../../crates/ss-accounts/src/usage/mod.rs)）运行，拥有额度快照模型（`UsageSnapshot` / `UsageWindow` / `Credits` / `UsageUnit`）与每家 agent 的额度抓取。它不持久化任何东西：快照由 `ss-accounts::service` 存在账号记录旁。读之前若令牌临近过期，
`usage::fetch::tokens_for_read` 调 `crate::oauth::refresh` 换新，并把轮换后的 `TokenSet` 与 `provider_state` 通过
`UsageOutcome` 回传给服务层落盘——`usage` 不直接写凭据文件。

当前覆盖范围与 `ss_accounts::Agent::supported()` 完全一致（见 [`crates/ss-accounts/src/agents.rs`](../../../crates/ss-accounts/src/agents.rs) 的单一静态描述表 `spec()`）。新增抓取器的具体改动步骤与文件清单见 [Accounts 新增受管 Agent 统一配方](../accounts/README.md#新增受管-agent-统一配方recipe)。

## 模型不变量

- `UsageWindow.remaining_percent` 永远是**剩余**百分比（0..=100）；已用比例不得写进这个字段。
- `used` / `total` 的含义随 `unit`：`Percent` 时留空、`Count` 时是数量、`UsdCents` 时是**美分**。
- `resets_at` 是 unix 秒；`fetched_at` 也是 unix 秒。
- `UsageSnapshot.error` 只在「保留了旧窗口但这次读失败」时填原因（Transient 路径）。
- `Credits.unlimited` 为真时 `remaining` 为空，卡片显示「无限」。

## 每家端点与解析

| agent | 端点 | 鉴权 | 窗口 | 备注 |
| --- | --- | --- | --- | --- |
| codex | `GET https://chatgpt.com/backend-api/wham/usage` | `Authorization: Bearer <at>` + 有账号 id 时 `ChatGPT-Account-Id` | `rate_limit.primary_window` / `secondary_window`（`used_percent` 是**已用**、`limit_window_seconds`、`reset_at`） | `limit_window_seconds >= 86400` 记周窗口，否则 5 小时窗口；label 由窗口秒数换算（Nd/Nh/Nm/Ns）；`credits.unlimited` 或 `credits.balance` 给点数 |
| claude-code | `GET https://api.anthropic.com/api/oauth/usage` | `Bearer` + `anthropic-beta: oauth-2025-04-20` | 优先 `limits[]`：`session` -> 5 小时、`weekly_all` -> 周、`weekly_scoped` -> 模型名；回退顶层 `five_hour` / `seven_day` | 百分比取 `percent`，缺则 `utilization`（可能是数字字符串）；`resets_at` 支持 RFC3339 / 秒 / 毫秒 |
| antigravity | `POST https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist` → `…:retrieveUserQuotaSummary`（404/405 才 fallback `…:fetchAvailableModels`） | `Bearer` + UA `antigravity/{ver} {os}/{arch}` + `x-goog-api-client: gl-node/22.21.1` | `groups[].buckets[]` 的 `remainingFraction`（**剩余**）、`resetTime`（RFC3339）；per-model 在 `models[*].quotaInfo` | base 顺序 daily → daily sandbox → production；project id 兼容 `{id}` 对象并缓存在 `provider_state`，带缓存 400 时去缓存重试；plan = `paidTier.id` → `currentTier.id` → `allowedTiers`；credits = `paidTier.availableCredits`；`is_gcp_tos` 进快照的 `gcpTos` |
| gemini-cli | Cloud Code Assist：`loadCodeAssist` → `onboardUser` → `retrieveUserQuota`（缺则 `fetchAvailableModels`） | `Bearer`（Google） | 窗口百分比 + 模型明细 | Google 系与 antigravity 同源不同 client |
| grok | `cli-chat-proxy.grok.com` 的 `/v1/billing`（`?format=credits`）与 `/v1/settings` | `x-xai-token-auth` | 计费窗口 + credits | gRPC-Web 重置额度未移植 |
| kiro | `POST https://q.{region}.amazonaws.com/getUsageLimits` | `Bearer`（CodeWhisperer） | 用量上限 | region 取自账号 |
| zcode | `billing/balance`（JWT + `X-Device-Mid`）+ 上游 `quota/limit` | `Bearer` | 余额 + 比例窗口 | MCP 用量窗口与重置卡未移植 |
| devin-desktop | `SeatManagementService/GetUserStatus` | `Bearer` | 座位额度 | 只有导入的账号可用 |
| cursor | `GET https://cursor.com/api/usage-summary` | Cookie `WorkosCursorSessionToken=<user_id>%3A%3A<access_token>`（user_id 取 access token JWT `sub` 中 `|` 之后、须 `user_` 开头）+ 固定 UA | `autoPercentUsed` / `apiPercentUsed` / `totalPercentUsed`（**已用**）；金额 `used` / `totalSpend` / `includedSpend` 与 `limit` 是**美分** | 套餐由 `GET https://api2.cursor.sh/auth/full_stripe_profile` 给（enterprise / team / 个人档），失败不影响额度 |

所有抓取器都走 `ss_core::infra::http_client::probe_http_client`（超时 20s），即用户代理配置生效。

## 错误分类（唯一实现：`ss-accounts::oauth` 与 `ss-accounts::usage::UsageError`）

| 上游 | 分类 | 账号层动作 |
| --- | --- | --- |
| 401、`invalid_grant` 及其同类 | `AuthRequired` | 置 `requires_reauth`、保留账号行、卡片给「重新授权」 |
| 429 / 5xx / 传输失败 | `Transient` | 保留上次快照，`UsageSnapshot.error` 记原因，不置 reauth |
| 其它 4xx、解析失败、形状变化 | `Unreadable` | 视为本次读取失败，不置 reauth |

**不得**把 5xx 当掉登录处理；也**不得**在 403 上置 reauth（403 语义随 provider 而异）。

## 验证

- `cargo test -p ss-accounts --locked`：每家都有纯函数解析（响应文本 -> 快照）与 fixture 测试，覆盖正常、缺字段回退、
  401、429、垃圾响应。
- 真实端点联调**未验证**：本版只锁协议形状与错误分类，联调需要真实账号，未做。

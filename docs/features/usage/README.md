# Usage 与 OAuth

状态：active

本文件维护订阅、配额、账号切换和 Usage UI 的当前契约。

## 所有权与存储

- `ss-usage` 拥有 catalog、OAuth/API-key/TokenImport fetcher、token 加密、subscription storage 和 refresh guard。
- subscription/usage snapshot 位于 `~/.skillstar/secrets/accounts/usage/`（含加密 token，整体按秘密保护；刷新与存储锁在 `runtime/locks/accounts/`，布局见 [../../storage-layout.md](../../storage-layout.md)）。catalog 和条目数量以 `crates/ss-usage/src/catalog.rs` 及其测试为准。
- `AuthMode::Cookie` / `Manual` 仅保留旧数据兼容，当前账号目录不提供入口。`AuthMode::TokenImport` 接受裸 token 或凭据 JSON，不走创建表单，只走 `import_subscription_token`。添加账号对话框能驱动哪些模式以 `crates/ss-gpui/src/accounts/dialog.rs` 为准，不是后端枚举的镜像。不要恢复 `authModes.ts`。
- Provider 私有刷新上下文放在 `Subscription.provider_state_encrypted`（AES-GCM 的版本化 JSON）。它不进 DTO，也不复用旧兼容字段 `platform_token_encrypted`。refresh 的窄 patch 会轮换这个字段。
- 目录外家族的旧记录只从列表、汇总与后台刷新中过滤；读取或新增账号不删除其磁盘记录。当前家族范围见 [accounts README](../accounts/README.md#家族范围)。
- 远程请求统一使用 `ss_core::infra::http_client::probe_http_client`。
- 度量面是会话文件单源（D-082 移除了网关与它的账本合并、路由对比和 401 自愈）：各 Agent 会话文件解析出的调用直接供给今日行与今日汇总（见「今日消耗与汇总」）。账号的增删改切与消费展示都在 Accounts 工作台（见 [accounts README](../accounts/README.md)）；Usage 顶层模式已移除（[D-085](../../decisions.md)），Usage 域只作为数据与卡片的提供方。
- 除非用户明确要求，不修改完成态的 `fetchers/oauth/cursor.rs`。

## DSH 家族补齐

- OpenCode Go 使用独立 `opencode-go` 家族与 API Key 添加入口；先以 Bearer 查询 `/console/api/go/status`，失败时回退 `/zen/go/v1/usage`，展示 5 小时、周、月窗口。月窗口标签是这一账期的天数（`28d`–`31d`）：重置时刻等于 `access.endsAt` 时用 `access.startsAt` 到该时刻的间隔；只有重置时刻时，按 UTC 回退到上个月同一钟点，短月把日夹到月末，与 OpenCode `getMonthlyBounds` 相同。没有可用间隔时标签仍是 `Monthly`。控制台 meter 带 `usedMicroCents` / `limitMicroCents` 时，`used` / `total` 记成美分，`unit` 为 `usd-cents`（微美分 ÷ 1e6）。卡片写成 `$已用 / $上限 · 剩余 n%`，例如 `$3.00 / $12.00 · 剩余 75%`。只有百分比的回退窗口 `unit` 仍是 count，只显示剩余百分比和重置时间。无有效额度字段报错，不显示虚假的零用量。本次不恢复旧 OpenCode/Zen Cookie 刮页入口，也不自动转换旧 `opencode` 账号。

- ChatGPT 与 Codex 是独立家族。ChatGPT 当前通过 TokenImport 接收 Sign in with ChatGPT 的完整会话（`accessToken`、`refreshToken`、已签发的 `oaiapp_` clientId、`expiresAt`、`scopes`），只接受已授予 `chatgpt.tokens.use.direct` 的记录，按原 clientId 刷新，并通过公共 `/v1/models` 验证凭据。该流程没有额度接口，不把 Codex 额度当成 ChatGPT 额度；用量链接指向 ChatGPT Settings → Usage。
- Cline 当前通过 TokenImport 接收 accessToken 或会话 JSON，使用 `workos:` Bearer，读取账号、微美元余额、套餐与滚动窗口；带 refreshToken/expiry 的会话到期前刷新。没有套餐时允许只显示余额。
- Command Code 使用 API Key，按 whoami 的组织读取 credits、subscriptions 与 usage summary，展示余额和 5 小时 / 周窗口。
- Kimi 读取 Kimi Code `/coding/v1/usages` 与 `/me`，不再把 Moonshot 平台余额当成 Kimi Code 额度。已有 Moonshot key 需换成 Kimi Code key。
- 新增家族的导入/密钥入口复用现有添加对话框；尚未提供这些家族的浏览器登录或本机凭证切换。

## OAuth 与刷新

- OAuth 启动返回 `OAuthStartInfo.flow`，登录界面按 flow 渲染，不按 catalog id 分支。四态是 `LocalCallback`（loopback，粘贴回调可 HTTP 重放）、`RemotePoll`（后端轮询，没有粘贴框）、`SchemePaste`（自定义 scheme，进程内解析，不发 HTTP）和 `Immediate`（就地完成）。pending 登录仍是进程内存态，重启后作废。
- 编辑既有 subscription 发起 OAuth 时，pending state 带原 subscription id；**每个 OAuth catalog 都必须把它传到 finalize**，成功后原位替换并保留用户 metadata/sort order，不新增重复卡片。用户自定义的卡片标题优先于登录带回的邮箱，只有占位标题会被升级。
- 标准 form-grant token 交换走 `oauth::token_endpoint::post_token`。非标准 token 腿（例如 Kiro IDC、ZCode JSON）留在各自 fetcher，但错误分类必须与 `post_token` 同一张表。
- refresh 只用窄 patch 更新 fetcher-owned runtime 字段，不能用网络请求开始时的旧整行覆盖用户刚修改的 metadata 或凭证。
- 客户端自动同步以 adopt 之后、网络请求之前的凭据为基线；只有查询实际改变了令牌或有效期才写回，名称、套餐、额度、身份补全和相同明文的重新加密不触发写回。写回仍须确认客户端服务的是这次查询的原会话；客户端已切号、退出或自行轮换时，不用旧会话覆盖。轮换只修改认证字段，保留同账号的客户端私有资料；显式切号/重新同步继续走各自完整写回。
- OAuth finalize 与 `local_import` 都在对应 catalog 的 `refresh_guard` 锁内完成写入，和 refresh 同属一个 serialization domain。
- Antigravity 的“从本地导入”与切号使用同一读取优先级：macOS 当前桌面版本优先读 `gemini` / `antigravity` Keychain，旧版或未检测到系统凭据时再读 `state.vscdb`。导入复制当前 access/refresh，不向 Google 兑换刷新令牌，也不改 Keychain 或 `state.vscdb`。
- Devin Desktop、Kiro、ZCode 的本地导入在写入前同样尽力核对一次额度，核对只用复制来的访问令牌，不兑换刷新令牌。核对失败仍保存凭据行，卡片可以之后再刷新。粘贴导入（`import_subscription_from_token`）必须刷新成功才落盘。
- 可覆盖 OAuth client credential 的 provider 按 env → compile-time → `oauth_clients.json` → built-in fallback 解析；要求外部凭证的 provider 保留自己的专用配置模块。
- Antigravity 使用参考项目同源的公开 Google OAuth 客户端作为内置 fallback；仍可用 `SKILLSTAR_ANTIGRAVITY_CLIENT_ID` / `SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET` 或 `~/.skillstar/config/antigravity_oauth.json` 覆盖。这里的 client secret 属于桌面 OAuth 客户端标识，不是用户账号凭证；账号的 access/refresh token 仍只进加密 subscription storage。
- Antigravity 额度先调用 `loadCodeAssist` 获取 plan、credits 和 `cloudaicompanionProject`，再把项目 ID 传给 Cloud Code。项目字段同时兼容字符串和 `{ "id": ... }` 对象。Cloud Code 请求按 daily → daily sandbox → production 回退；优先使用 `retrieveUserQuotaSummary` 返回的用户可见 5h/weekly buckets，只有摘要接口没有可用窗口时才回退到 `fetchAvailableModels` 的模型 quota。汇总卡按最紧张（消耗百分比最高）的窗口计算，`UsageWindow.used` 与 `percent` 始终表示已消耗比例，不能把剩余比例写入 `used`。
- Antigravity 模型列表不是固定枚举：已知模型按产品分组，新增的 Gemini/Claude/GPT/Image 模型只要带 `quotaInfo.remainingFraction` 也必须显示；无法抓取模型额度时保留 plan/credits，同时在 `usage.error` 显示原因，401 仍按认证失效处理。配额刷新优先调用 `retrieveUserQuotaSummary`；只有该 endpoint 明确返回 404/405 时才回退 `fetchAvailableModels`，有效但为空的 summary 不再额外发起模型目录请求。卡片上的 `Weekly Limit` 与 `Five Hour Limit` 随界面语言显示，中文是「周额度」和「5 小时额度」，模型组名保持抓取原文；重置倒计时与进度条同行，避免四个窗口把卡片纵向拉长。
- Cursor 额度来自 `GET https://cursor.com/api/usage-summary`。计划节点按 `individualUsage.plan`、`individual_usage.plan`、`planUsage`、`plan_usage` 依次解析。包含额度的 `used` / `limit`（以及更早的 `breakdown.total` 上限）是美分，窗口标签 `Included`，`unit` 为 `usd-cents`；已用和上限都在时，`percent` 按金额四舍五入。只有百分比时 `unit` 仍是 count，总量为 100。`autoPercentUsed` 与 `apiPercentUsed` 只记百分比，标签分别是 `Auto + Composer` 和 `API`。旧的 `fastRequests` 计数仍是 count，标签 `30d`。重置倒计时用紧凑单位，中文不写「后重置」。

### Codex 特例

- 额度端点 `GET https://chatgpt.com/backend-api/wham/usage`，头 `Authorization: Bearer <access_token>` + `ChatGPT-Account-Id`（取自 id_token claims）。`plan_type` 最大只到 `pro`：OpenAI 不在该响应暴露 Pro 20x/5x 档位，界面上的 PRO 徽章即来自这里；上游哪天加档位字段再接入。
- **窗口槽位与标签由 `limit_window_seconds` 决定，不写死窗口位置**：ChatGPT Pro 账号实测把 7 天预算放在 `primary_window` 且 `secondary_window` 为 null，按位置写死会把周窗标成 "5h"，造出「5 小时窗口 3 天后重置」的矛盾卡。时长 ≥24h 进 weekly 槽（`604800` 标 "7d"），否则进 hourly 槽（`18000` 标 "5h"）；字段缺失回退位置槽位（primary→5h、secondary→7d）。界面按 `Nh`/`Nd` 时长标签本地化，不写死窗口位置。
- 同一响应的 `credits.balance` 画在额度条下的「额度点数」行（英文 `Credits`）。落盘保留上游原文，包括 `0`；界面最多保留两位小数并去掉尾随 0。`credits.unlimited` 为真时这一行是「无限」/ `Unlimited`，不再显示点数。没有 `credits`，或余额为空且非无限时，不显示这一行。重置卡仍来自 `rate-limit-reset-credits`，与这笔点数无关。

### 错误分级（唯一裁决点）

失败必须先归类再落库，三类互斥：

| 上游 | `UsageError` | `requires_reauth` | 已有额度快照 |
| --- | --- | --- | --- |
| 401，或 OAuth `error` 为 `invalid_grant` / `invalid_token`（Google 用 **400** 报撤销） | `AuthRequired` | 置位 | 清空，换成“登录已失效”卡 |
| 429 / 5xx / 传输失败 | `Transient` | 不动 | **保留**，只追加错误文案并保持原 `fetched_at` |
| 其它非 2xx、解析失败 | `Fetcher` | 不动 | 清空 |

- 只有 `AuthRequired` 能置 `requires_reauth`，且它**只**由 `oauth::token_endpoint` 和 fetcher 里显式的 401 判定产生。
- 403 不是认证失败：Cloudflare / 地域拦截对有效凭证同样返回 403，而 API Key 模式根本没有“重新授权”可做。
- 已置位的 reauth latch 不会被一次瞬时失败清掉。
- 分级实现见 `UsageError::{http_status, transport, is_transient}`、`request::RequestError::{is_auth_error, is_transient}` 与 `SubscriptionUsage::from_refresh_error`。

### Ollama Cloud

- 本机 `localhost:11434` 没有额度。这张卡读的是 ollama.com Cloud，密钥来自 [Settings → Keys](https://ollama.com/settings/keys)。
- `GET https://ollama.com/api/usage`，`Authorization: Bearer <key>`。公开的 chat/generate 文档不暴露账号额度；该路径无密钥返回 401 `invalid credentials`，有密钥时社区已对过真实账号：`limits.session.usage` / `limits.weekly.usage` 是 0–1 已消耗比例；`limits.session.models` / `limits.weekly.models` 是 `{ name, request_count }` 列表（请求次数，不是 token、也不是额度份额）。
- 模型行解析进对应窗口的 `breakdown`，不写 `percent`/`total`。无法解析的模型行丢弃，不影响窗口本身。卡片把这些行画成本时段 / 本周「用过的模型」列表（色点 + 名称 + 次数），不走分类额度条。
- API 不返回重置时间。5h 窗口按 Unix epoch 对齐的 5 小时网格本地推算；周窗口是每周一 00:00 UTC。解析失败按窗口降级，两个窗口都没有才算账号失败。
- 401 走 `BalanceSpec.auth_error_hint`：卡片显示「请填 Cloud API Key」而不是通用「重新授权」——API Key 没有 reauth 流。
- 卡片画百分比、重置和「用过的模型」列表，不画分类额度条。

### ZCode 特例：billing 请求必须带设备标识

- ZCode 额度查询不会轮换令牌，刷新额度后不写客户端凭据或设置文件。显式切号和手动重新同步才写回：`user_info` 必须含字符串 `id`、`username`、`displayName`；同一账号已有完整资料时保留原文（含 `rawProfile`、头像等客户端字段），不能用额度卡的摘要覆盖。不同账号不得沿用上一账号资料；缺少用户 ID 时写回失败，不覆盖客户端文件。
- 配额数据链是三段：provider profile（zai 走 `chat.z.ai/api/oauth/userinfo` Bearer，BigModel 走 `open.bigmodel.cn/api/biz/customer/getCustomerInfo` 裸 token）+ billing 余额（`zcode.z.ai/api/v1/zcode-plan/billing/balance`，Bearer zcode JWT）+ 编程套餐三条额度（`GET {api.z.ai|open.bigmodel.cn}/api/monitor/usage/quota/limit`，`Authorization: Bearer` 上游 access token）。
- 三条额度是 5 小时、每周、ZCode MCP。`limits[]` 里 `TOKENS_LIMIT` / `CREDIT_LIMIT` 用 `unit` 区分窗口：`unit` 3（`number` 缺省或 5）是 5 小时，`unit` 6（`number` 缺省或 1）是每周；`TIME_LIMIT` 是工具调用窗，只在 MCP 用量接口没有返回时占第三条。`percentage` 是已消耗比例。卡片只写剩余百分比，不把 `currentValue` / `usage` 画成已用 / 总量。`usageDetails` 里次数大于 0 的工具画在这条下面，不另作额度条。同一窗口出现两次，或 `unit` 对不上，就省略该窗，不按数组位置猜。没有 `limits` 时才读旧字段 `fiveHourPercent` / `weeklyPercent` / `monthlyMCPUsage`。这次请求失败时保留已经取到的 billing 余额。billing 只有一条余额、总量大于 100、且已消耗比例和 5 小时窗一致时，把绝对数量补进 5 小时条，不再另画一条 Quota；卡片仍只写剩余百分比。
- 第三条 **ZCode MCP** 不在 `limits[]` 里。它是 `GET https://zcode.z.ai/api/v1/mcp/usage`：`Authorization: Bearer` zcode JWT，`X-Bigmodel-Authorization: Bearer` 上游 access token，`Bigmodel-Target-Type: PERSONAL`。`data.total_usage` 的 `used` / `limit` / `remaining` 决定这条条的剩余比例，文案只写剩余百分比，`next_refresh_at` 是重置时刻的 Unix 秒。业务 `code` 必须是 0。这条成功时盖过 `TIME_LIMIT`，因为界面上的第三条是 ZCode MCP，不是工具调用窗。失败则保留已有的月窗。
- billing/balance 强制 `X-Device-Mid` 头与 `app_version` query：缺设备标识时服务端返回 400 业务码 3001 "parameter error"（服务端只验存在性，见 errors.md 2026-10-05 条目）。profile 两个端点不带该头。
- 设备标识解析（`fetchers/oauth/zcode/device.rs`）：优先**只读**官方客户端 `{zcode_home}/v2/telemetry-state.json` 的 `deviceMid`（不创建、不轮换，与 usage_switch 的承诺一致），否则回退 `state/usage/zcode-device-mid` 里持久化的自有 UUID v4（每机一份，落点具名函数在 `ss_core::infra::paths::zcode_device_mid_path`），全无则生成一个并尽力落盘——解析失败不让整卡失败。
- 仿客户端版本号写死在 zcode fetcher 的 `APP_VERSION`（当前 3.14.3，与 magpie 一致）；服务端将来校验版本时会以 400/3001 同款症状复发，届时对照参考实现升版本并补头。

## 本地工具账号切换：CLI 软链与 IDE 适配器

切号的本质是「让目标应用下次读凭证时读到另一个账号的凭证」。SkillStar **不持有凭证**，CLI
账号使用凭证文件快照和软链；CLI 自己轮换 token 时直接写穿到快照，所以不存在「陈旧拷贝」。
Antigravity 和 Cursor 不适合这套整文件软链模型，分别写入它们真实使用的 OAuth 存储。路线选择与后果见
[../../decisions.md](../../decisions.md)。

- `ss_usage::usage_switch` 是账号切换引擎本体（D-077 迁入 usage crate；它对 models 的旧依赖已收敛为 core 的滚动备份/沙箱原语与本 crate 的 `tool_paths` 解析）；壳和 CLI 不直接理解 provider 凭证文件 schema。
- 快照落点 `~/.skillstar/secrets/accounts/cli/<catalog_id>/<subscription_id>.json`，权限 0600，走后端解析真实数据目录（`SKILLSTAR_DATA_DIR` 等覆盖继续生效）。**一份快照是整个 CLI 凭证文件**，不是其中一个账号的片段 —— 软链只能整文件替身。
- live 路径必须是 CLI 自己读的那个文件，并尊重上游 env 覆盖：`CODEX_HOME`、`GROK_HOME`。`SKILLSTAR_TOOL_SYNC_HOME` 沙箱优先级最高，测试不得逃逸。
- 支持哪些 catalog 由切换适配器推导，不是 UI 手抄白名单：CLI 账号走 `usage_switch::target_for`，IDE 账号走 `usage_switch::ide` 的 `IdeCredentialAdapter` 注册表。Antigravity 和 Cursor 是最初的两个实现；其后的 IDE 只加注册表项，不改切号顺序。Antigravity 的“当前账号”优先读取 macOS Keychain 的 `gemini` / `antigravity` 条目；没有该条目时读取 `state.vscdb` 中 `antigravityUnifiedStateSync.oauthToken`。Cursor 的当前账号读取其 `state.vscdb` 的 `cursorAuth/accessToken`、`cursorAuth/refreshToken` 和 `cursorAuth/cachedEmail`，都不是 Usage 的 active pin。
- 已登录且这张卡本来就是 pin 时，OAuth 完成会重写本机存储的范围是：有 IDE 适配器，或者 catalog 是 `xai`。Codex 的登录路径自己写 CLI 文件，不在完成时再写一遍。
- 本地路径、`state.vscdb` 通用写、原子 JSON 和 macOS internet-password 在 `ss-usage` 的 `tool_paths` / `tool_store`。测试必须走 `SKILLSTAR_TOOL_SYNC_HOME`，不得碰真实 `$HOME` 或登录钥匙串。
- 编辑器改名只迁 Electron user-data 目录，不动 `~/.codeium`：Windsurf 编辑器 2026-06 OTA 改名 Devin Desktop 后，配置与 skills 仍在 `~/.codeium/windsurf`，但 macOS `Application Support`（及各 OS 对应目录）下的 user-data 目录从 `Windsurf` 变成 `Devin`。`tool_paths::devin_desktop_state_db_path` 先取 `Devin` 再回退 `Windsurf`，未升级的旧安装继续可用。
- 同一次改名把 catalog id 从 `windsurf` 换成 `devin-desktop`（`devin` 是 Devin for Terminal，不冲突）。旧值在读取边界迁移，不丢用户数据：订阅行的 `catalog_id` 与 pin map 的 key 在 `storage` 加载时改写回存（`storage::CATALOG_ID_RENAMES` 是唯一映射表）；实例记录靠 `DesktopAppId::DevinDesktop` 的 serde alias 与 `instances::store` 的 profile 目录改名；skills 的 enabled 偏好在 `profile_storage` 键迁移。OAuth 端点仍挂在 windsurf.com 域名，vscdb 存储键（`codeium.windsurf` 等）不变。 Rust 模块、类型、函数及提示统一使用 Devin Desktop 命名；既有回调路径与 `SKILLSTAR_WINDSURF_SAFE_STORAGE_PASSWORD` 环境变量保留兼容。
- 浏览器登录的 loopback `access_token` 有两种。Firebase ID token 仍走 `RegisterUser`，拿到 apiKey 之后 `GetOneTimeAuthToken` / `GetCurrentUser` 失败（含 401）只少会话和邮箱，不取消这次登录。当前 windsurf.com `/editor/auth-success` 放进回调的是浏览器里已经换好的会话 `authToken`。`RegisterUser` 对它返回 401 时按会话保存；`GetCurrentUser` 和 `GetPlanStatus` 也都拒绝，才是 `AuthRequired`。
- 各 provider 的私有协议（Copilot 的 `token` scheme、Windsurf Connect-RPC、Kiro 双登录腿、ZCode `enc:v1`）以对应 fetcher 和其测试为准，不在这里抄字段表。缺字段省略窗口，不补成 0。
- Antigravity 切换顺序：取得 catalog 锁 → 读取并解密目标账号 → 写入并验证 macOS Keychain（当前桌面版本）或生成官方 Unified OAuth protobuf、在 SQLite 事务内写入 `state.vscdb`（旧版/其它平台）→ 回读并校验 refresh token → 最后才落 active pin。目标存储不存在、无法写入或回读不一致时，pin 保持旧值并明确显示“切换未生效”。
- Antigravity OAuth 登录完成后，如果目标卡原本就是 active，会立即按同一适配器把新凭证投影回 IDE；普通刷新也会把 active 卡的 token rotation 投影回 IDE，避免 Usage 与 IDE 分叉。
- Cursor 切换顺序：取得 catalog 锁 → 解密目标 access/refresh token → 在 SQLite 事务内同时写入 `cursorAuth/*` 和 Cursor 镜像 key → 回读 access/refresh/email 校验 → 最后才落 active pin。Cursor 缺少本地 `state.vscdb` 或凭证不完整时不会只改 pin，而是明确返回“切换未生效”。Cursor 本地导入也读取同一组 key；一次 OAuth 会话读取在同一 SQLite 连接和查询中取得四个字段，避免跨连接快照。
- Cursor 的 IDE 进程可能自行轮换 token；Usage 刷新前先采纳仍属于当前卡的本地新 token，刷新后再把新 token 投影回 `state.vscdb`，避免 refresh token 双花和卡片/IDE 分叉。
- 每个 catalog 使用进程内 async mutex + CLI 自己的 OS file lock（Grok 用官方 `auth.json.lock` 并回写 `PID:秒` holder 行；无官方锁的 CLI 用私有 `<file>.skillstar.lock`）。软链消灭的是陈旧拷贝，**不是** refresh token 单次使用的双花竞态，所以锁必须保留。
- activate 顺序：取锁 → 捕获 live 现有凭证（备份 + 归属判定 + 吸收）→ 准备快照 → 原子替换 live 为软链 → 回读校验 → **最后**才落 pin。任何一步失败都保留旧 pin 与旧 live 文件；软链已换但后续失败时回滚到原来的软链或备份。CLI 托管不写 macOS keychain；Codex 的 keychain 只在对账时读取。
- 「失败发生在替换之前还是之后」不靠调用方猜：`usage_switch::error` 里 `ActivationError` 带一个 `Stage`（`BeforeReplace` / `AfterReplace`），回滚与否只看它。替换前失败时旧凭证原封不动，回滚才是破坏。
- 托管层的失败面是**真枚举不是字符串**：`CustodyError`（路径解析 / 锁 / 读 / 写 / 原子替换 / 软链 / 回读校验 / 归属冲突 / 快照缺失损坏）、`MaterializeError`（订阅行凑不出 CLI 能用的凭证）。面向用户的中文文案由变体生成，命令层只做 `SwitchOutcome.error` 字符串适配。计数门禁见 `scripts/internal/check_error_strings.sh`。
- 对账判据是**内容不是文件类型**：三态 `LinkedTo / Diverged / Missing`；CLI 比 access_token 字符串，Antigravity 比 refresh_token，不比整个 JSON（CLI 会加自己的字段）。CLI 用 `rename()` 把软链冲成实体文件、内容却一致时判 `LinkedTo` 并静默重建软链。
- pin（`active_per_catalog.json`）是这个三态的缓存，不是第二个真相源；`reconcile` 随时可以从磁盘重建它。UI 的「当前」badge 读 `reconcile_cli_accounts` 命令而不是读 pin（见下面「Usage 卡片与 active 状态」）。
- `reconcile_cli_accounts` 在每个 catalog 自己的 serialization domain 里跑，且对没有本地工具凭证的机器不取 CLI 锁 —— 取锁会为了确认“什么都没有”而先把 CLI 的家目录和锁文件创建出来。
- 删除 subscription 时先 `forget_subscription_session`：删快照，且若它正是当前 live 的软链目标，先把 live 还原成一份实体文件拷贝 —— 悬空软链不是「已登出」，是「登不上」。
- 不向通用 `Subscription` 增加 provider-specific 字段；CLI provider 实现 `CliCredentialTarget`（路径、锁、access_token 提取、身份、materialize、absorb、可选的只读第二存储 `external_root`），IDE provider 使用独立适配器。

### 软链盖不住的三个洞

- **Codex 在 macOS 以 `auth.json` 为准，不向系统 Keychain 写入任何凭据**（已遵循全局本地加密存储策略停用系统钥匙串写入，仅保存在应用内加密数据与本地工具配置中）。
- **CLI 用 `rename()` 原子写会把软链换成实体文件**，见上面的三态判据；reconcile 负责重建。
- **Windows 无软链权限时降级为拷贝**，reconcile 每次双向同步。降级是显式且**用户可见**的：`LinkMode::Copy` 经 `SwitchOutcome.link_mode` 透到 DTO 和 UI（切换/重新同步成功后弹降级提示，浮窗常驻一条说明），不是只写 warn 日志。拷贝语义下 CLI 自己轮换的 token 不再自动回流，这件事必须说出口。

### Grok/xAI 特例

- 交互登录走 OIDC 设备码：`POST https://auth.x.ai/oauth2/device/code`（`client_id` + CLI scopes，含 `workspaces:read` / `workspaces:write`），浏览器打开 `verification_uri_complete`，后台按 `interval` 轮询 `https://auth.x.ai/oauth2/token` 的 `urn:ietf:params:oauth:grant-type:device_code`。设备端点返回 404 时才回退 `127.0.0.1:56121/callback` 的授权码流程。这与 Grok CLI 在远程 `device_flow=true` 时的登录一致。
- credits endpoint 决定当前 weekly/monthly period；weekly 是严格 percent-only，不能用 calendar-month 金额伪造绝对周额度。每月额度的 `used` / `total` 是美分，`unit` 为 `usd-cents`。上限为 0 的明确月额度仍写入 `percent: 0`，账号卡显示剩余百分比，但不画每月额度进度条。窗口标签在数据里固定为 `Weekly credits` / `Monthly credits`；账号卡按界面语言显示，见 [Accounts](../accounts/README.md)。
- 套餐徽章读 `GET /v1/settings` 的 `subscription_tier_display`（例如 `SuperGrok Heavy`、`X Premium+`），失败时用 access token 的 JWT `tier` 数字映射到同一套显示名。不要把供应商品牌 `Grok` 写进 `plan_name`。三次代理请求都带 `x-xai-token-auth: xai-grok-cli`。默认 billing 传输失败时，只要 `?format=credits` 已经给出周窗口，卡片仍更新，不把整次刷新判失败。
- calendar-month spend 作为次级 credit 展示，不再生成第二条“monthly quota”。零 on-demand cap 不显示。
- 周额度本轮缺失时可携带上次已知 weekly window，避免闪回错误月视图。
- 重置卡是账号级、显式确认的上游 mutation，不能以刷新代替。Codex 使用 wham `rate-limit-reset-credits` 查询和 `/consume` 消耗，同一请求的 `redeem_request_id` 与 `idempotencyKey` 相同；Grok 使用 `ConsumerUiSvc.GetRemainingResets` / `RedeemReset`，优先最早过期的 token；ZCode（GLM）使用对应区域的 `customer-package-reset/list` / `use`，分别选择 `FIVE_HOUR` 与 `WEEK`，校验业务成功信封。同一卡的 GLM 请求 ID 在网络结果不明时保留用于重试。BigModel 无时区到期时间按 UTC+8 解释，Z.ai 按 UTC 解释（沿用参考实现）。
- 卡库通过 `subscription::ResetWindow` 指定的 `CreditInfo` 记录投影张数与到期时间，保留现有 Grok 记录兼容；token / record ID 不落盘。卡库读取失败保留上次已知卡库，不冒充零张；消费成功后读取失败则清除旧卡库并提示刷新，防止再次消费过时卡。过期卡在本地时钟推进时移除；GUI 几何和交互见 [Accounts](../accounts/README.md)。
- CLI snapshot 用稳定 subject/user identity 归属；冲突 identity fail closed。token 必须满足 Grok CLI scopes，写入前后均验证，并保护外部进程并发改写。
- 「从本地导入」（`local_import` 注册表含 `xai`，与切换引擎共用 `tool_paths::switch_grok_auth_path` 的路径解析）**只读** `~/.grok/auth.json`（尊重 `GROK_HOME`），不写文件：CLI 自己轮转 token，导入只是把当前 token 快照成 subscription 行并刷新一次额度，也不在完成时重写 live——行的内容本来就来自 live。条目选择优先本 crate 的 scope key（`https://auth.x.ai::<client_id>`，`xai::scope_key` 是唯一格式化点，切换引擎同用），否则按 magpie 的读取语义取排序后第一个带非空 `key` 的条目；`expires_at` 取存储 RFC3339 与 JWT `exp` 的较早者，身份字段镜像切换引擎的读取（`user_id` / `principal_id` / `sub` 与 `email`，JWT claims 兜底）。

## Agent 会话解析（sessions）

- `ss-usage::sessions` 只读解析受管 Agent 自己的会话文件（`crates/ss-usage/src/sessions/`），给度量面提供本地调用的 token 事实。**绝不写 Agent 目录**（该目录归 Agent 自己与账号切换引擎）；解析器自身唯一落盘是 SkillStar 数据根下的增量索引 `data_root()/sessions/index.json`（`atomic_write`，删掉只是下次全量重读）。
- 入口是 `read_calls(home, since)`：每次调用返回全量视图（消费方可幂等整体替换），`since` 为 epoch 毫秒下界。跨文件 message-id 去重按「最早文件优先」——resumed 会话拷贝旧文件内容，同一 message 只计一次。
- 增量语义按文件 checkpoint（`FileCheckpoint`）：文件头指纹判「替换 vs 增长」、已读前缀采样哈希防原地改写，任一失配即从零重读；未增长且头一致时不再打开正文。解析器版本或索引版本变化同样全量重读，不做迁移。
- 覆盖哪些 Agent 家族以 `crates/ss-usage/src/sessions/mod.rs` 的 `parsers()` 注册表及其测试为准（SSOT），文档不手抄清单；受管 Agent 的本地会话文件都走同一注册表，新增解析器只加注册表行。Claude 的行级规则由测试钉死：同 message id 后块 usage 覆盖前块且 `from` 取首块位置、synthetic 行只有 API 错误才算调用、行内 `entrypoint` 前缀 `claude-desktop` 归因 Desktop（含 `claude-desktop-3p`）。
- discovery 遵守 `SKILLSTAR_TOOL_SYNC_HOME` 沙箱（沙箱优先于 `$CLAUDE_CONFIG_DIR`）；claude-desktop 的 Cowork 目录布局（`local-agent-mode-sessions/*/*/local_*/.claude`）作为接口保留，本机未验证到该布局实际存在，Desktop 归因目前主要靠行内 entrypoint。

## 今日消耗与汇总（consumption summary）

- 数据链：`get_consumption_summary(window)` → `ss_usage::accounts::service::summary`（会话单源：`sessions::read_calls` → `consumption_view` 投影）→ `accounts::consumption::summarize` 纯函数（时钟与价格全注入）。DTO 见 `ConsumptionSummary`（`period / totals / series / by_agent / by_model / by_session`，ts-rs 生成；`by_account`/`by_catalog` 随网关归因一并移除）。
- **UTC 日界**：`Today` 的起点是 `now` 所在 UTC 日的 00:00（`Week`/`Month` 为截至今天的 7/30 个 UTC 日历日）。这是 wire 契约；服务层从 `period_floor_ms` 起读会话文件，可见窗口由 `summarize` 的 UTC 过滤裁定。
- **读时计价**（spec D8）：会话行只存 token，价格读当前表（`ss-usage::pricing`：遗留 `model_gateway.json` 顶层 `prices` 覆盖 > models.dev 缓存，按模型 id 反查 provider）。价格表变更会在下一次读取时重述历史，UI 恒标「估算」；价格表查不到的调用计入 `unpriced`（未知，不是免费）。缓存不再在线刷新，价格冻结在最后一次 models.dev 同步（D-082 的已知承担）。
- 会话行没有 provider 归属，计价按模型 id 在价格表内反查（`effective_price_by_model`）；查不到即 `unpriced`，不猜 provider。
- 计价与分组的模型 id 取「应答模型优先、请求模型回退」；`errors` = 有错误类别或 HTTP ≥400；`mean_latency_ms` 只对带时长的调用取均值。`series` 分桶自动切换：Today 按小时、Week/Month 按 UTC 天、All 按 ISO 周（周一 00:00 UTC 对齐）。
- 没有独立 Usage 页。若账号页展示今日消耗，成本必须标估算。不要恢复 `UsageSpendSummary` / `TodayConsumptionLine`。

## 会话 chip（slice 13）——后端契约，当前无界面消费方

- 数据链：`get_today_consumption` → 同一份会话单源组装 → `accounts::consumption::crossview` 纯函数（时钟与价格全注入）。DTO 见 `TodayConsumption`（`totals / by_agent / chips`，chip 含 `agent / session / title / last_active / tokens / cost_usd`，ts-rs 生成；`via_gateway` 随网关移除）。UTC 日界与读时计价与汇总同口径。
- chip 是派生视图，不新增真相：按 `(agent, session)` 折叠今日调用，最新活动在前；无会话 id 的调用只进 `totals` 不出 chip；`title` 取该会话调用最多的应答模型；`cost_usd` 为 `null` 表示没有一条可计价（未知，不是免费）。
- 会话 chip 条随 Usage 顶层模式一起删除（[D-085](../../decisions.md)）。今天没有任何界面渲染 `TodayConsumption`：这个 DTO 是休眠契约，重新启用前不要假设它在页面上出现。不要恢复已删除的 Usage 页组件。

## Usage 卡片与 active 状态

- **「当前」badge 的真相是对账结果，不是 pin。** pin（`get_active_subscriptions`）记录用户点过哪张卡；`reconcile_cli_accounts` 返回每个 catalog 的三态，是 CLI 下次实际会读到的东西。两者冲突时文件赢。
- 三态在 UI 上各自有话说，**不折成布尔**：`LinkedTo` → 绿色「当前」；`Diverged` → 琥珀「本地工具非此账号」并说明 CLI/IDE 现在用的不是这张卡（浮窗还给一条“点重新同步把它指回来”）；`Missing` → 灰色「本地工具未登录」。Diverged/Missing 不得静默渲染成“未激活”。
- pin 说 A 而 live 是 B 时，绿色 badge 挂在 B 上；A 只有在自己被 pin 时才显示 Diverged。卡片高亮环同样跟随对账结果。
- 没有切换适配器的 catalog（纯 API key 等）不在对账 map 里，回落到 pin —— 那里没有文件可以反驳它，pin 就是全部真相。Antigravity 虽然是 IDE，但有独立的 state.vscdb 对账适配器；首帧对账未回来时同样回落到 pin，不会先喊一声“没有当前账号”。
- `setActive` 的返回值是后端真相：只有返回行 `is_active=true` 时，界面才 demote 同 catalog sibling。
- CLI 切换被拒绝时，保留旧 badge，并使用“switch not applied”反馈；不能乐观宣称目标已激活。切换后紧跟一次对账，所以被拒时旧 badge 是被**重新确认**的，不只是没被改。
- 账号卡片的现行布局在 `crates/ss-gpui/src/accounts/`。邮箱不得用省略号截成看不清的身份；套餐徽章保留上游原文，不自行缩写成 `MAX` 或 `PRO`。
- 不要恢复 React 的 `UsageCard`、`bodyRegistry.ts`、`brandThemes.ts` 或独立浮窗。

## 请求构建

- 所有 fetcher 走 `ss-usage::request::Req`：统一附带 header/bearer/body、把非 2xx 归一为 `RequestError::HttpStatus`，让各 provider 只写响应解析。
- 底层 client 一律由 `crate::http_client::usage_http_client()` 提供，透传 `probe_http_client` 的代理设置；fetcher 不自建 `reqwest::Client`。

## 类型与刷新

- 账号和用量形状以 Rust DTO 为准。没有 TypeScript 生成物，不要恢复 `src/types/generated/`。
- `SubscriptionDto.oauth_region` 带回已存储的区域。`UpdateSubscriptionInput` 仍无该字段，所以编辑对话框不能修改区域。
- 账号页用已落地的缓存上屏，刷新结果原位更新。只有冷启动首次读取尚未落地时才显示等待态。此后刷新失败保留已显示的数据。
- 写路径、refresh 与 CLI 对账必须进入后端同一 catalog serialization domain，禁止页面级临时锁。
- `refresh_all_subscriptions` 接受可选 `catalogId`：只刷该 catalog 的行，其余行按存储快照原样返回。单供应商刷新必须传，避免一次点击横扫所有厂商端点。
- 加载、切换、对账和用量错误分层展示，不把所有失败折成“暂无数据”。
- 不要恢复 `useUsageData`、`UsageDataProvider` 或独立 Usage 页。

## Dock 与 Tray 菜单额度显示

- macOS 右键点 Dock 图标，或在系统状态栏 Tray 小图标菜单中，列出各订阅额度：每行 `<账号> · <额度状态>`（如 `剩余 N%`、`余额 $M`、`剩余 K 积分`、Codex `剩余 K 点`、`未同步`），按最紧张（剩余百分比最少）在前排序。N% 是该订阅「消耗最高的那条额度窗口」的剩余份额。Codex 点数用与账号卡相同的两位小数格式。
- 额度行的纯函数仍是 `ss_usage::dock_usage::snapshot_menu_summary`，拼行在 `ss_usage::accounts::dock_menu_lines_for_lang`。Dock 菜单和 Tray 随 Tauri 删除，GPUI 目前不展示这两处菜单。

## 桌面应用多开

Usage 卡片仍是订阅配额 + 默认 live 工具切号，**不是**启动器。`open_usage_card_window` / `open_external_url` 只开浮窗或浏览器，不会拉起 IDE。

实例清单由 `ss-usage::instances` 拥有，独立于 `Subscription` 数据。家族范围限 Accounts 目录中有独立 IDE 的应用，唯一注册表与验证状态见 [`instances/apps.rs`](../../../../crates/ss-usage/src/instances/apps.rs)，对应回归测试见 [`instances/tests.rs`](../../../../crates/ss-usage/src/instances/tests.rs)。插件、CLI 和聊天桌面应用不进入多开范围。

- 只有通过隔离验证的应用才显示入口、允许创建和启动；其余保留 Pending，不能因家族在目录内而绕过验证。
- Profile 根经 `ss-core` 路径解析，尊重 `SKILLSTAR_DATA_DIR`。Start 必须用独立 profile 拉起本机应用，不改默认 profile。Antigravity 必须使用 `--user-data-dir=<dir>` 等号形式；空格形式会丢失隔离参数。
- 移除的家族不再解析、列出、创建或启动。旧实例记录写回时保留，已有 profile 不自动删除，也不影响保留家族的清单读取。
- Stop 只终止 cmdline 含该实例目录的 PID，不动默认 profile 或其它实例。默认 live 切号仍只作用于各工具自己的默认存储。
- Windows / Linux 列出与创建可以工作；Start / Stop 返回明确的「仅支持 macOS」。

### 创建隔离实例

1. 从已验证 IDE 的多开入口创建实例，例如 `Work` 与 `Personal`。
2. 分别点 Start，以各自独立的 profile 启动；默认 IDE profile 保持不动。
3. Stop 只停该实例目录对应的进程。切号与配额刷新不会被实例 Start 覆盖。

## 验证

```bash
cargo test -p ss-usage -p ss-app
```

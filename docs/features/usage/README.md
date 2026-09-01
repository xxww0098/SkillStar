# Usage 与 OAuth

状态：active

本文件维护订阅、配额、账号切换和 Usage UI 的当前契约。详细的 Usage 卡片拆分过程已冻结在 [../../others/usage-card-refactor-2026-07.md](../../others/usage-card-refactor-2026-07.md)。

## 所有权与存储

- `skillstar-usage` 拥有 catalog、OAuth/API-key/Cookie fetcher、token 加密、subscription storage 和 refresh guard。
- subscription/usage snapshot 位于 `~/.skillstar/config/usage/`。catalog 和条目数量以 `crates/skillstar-usage/src/catalog.rs` 及其测试为准。
- Auth 模式含 `AuthMode::Cookie`（用户粘贴浏览器 `Cookie:`）；fetcher 入口在 `fetchers/cookie/`，解析见 `cookie_jar.rs`。前端能驱动哪些 auth 模式由 `src/features/usage/lib/authModes.ts` 的 `selectableAuthModes` 单独回答——它是**表单能力**声明，不是后端枚举的镜像，所以不放在 `types.ts`。目前 Cookie 放行、Manual 仍无表单。
- 不支持的旧 auth-mode 行在 load migration 中清理；文档不保留已删除 catalog 清单。
- 远程请求统一使用 `skillstar_core::infra::http_client::probe_http_client`。
- 除非用户明确要求，不修改完成态的 `fetchers/oauth/cursor.rs`。

## OAuth 与刷新

- OAuth 启动返回 auth URL 和 pending id；前端展示后轮询/回调完成。所有 provider 都走浏览器跳转，没有 device code 流（`OAuthStartInfo::browser` 是唯一构造函数）。
- 编辑既有 subscription 发起 OAuth 时，pending state 带原 subscription id；**每个 OAuth catalog 都必须把它传到 finalize**，成功后原位替换并保留用户 metadata/sort order，不新增重复卡片。用户自定义的卡片标题优先于登录带回的邮箱，只有占位标题会被升级。
- 所有 OAuth token 交换与刷新走 `oauth::token_endpoint::post_token`，fetcher 不自建 token POST 与错误映射。
- **不是每个 OAuth catalog 都有 token 交换腿。** `anthropic` 全程只读 Claude Code 自己的登录态，从不 POST token、从不写回凭证，因此 `token_endpoint` 不在它的路径上（见下节）。
- refresh 只用窄 patch 更新 fetcher-owned runtime 字段，不能用网络请求开始时的旧整行覆盖用户刚修改的 metadata 或凭证。
- OAuth finalize 与 `local_import` 都在对应 catalog 的 `refresh_guard` 锁内完成写入，和 refresh 同属一个 serialization domain。
- Antigravity 的“从本地导入”与切号使用同一读取优先级：macOS 当前桌面版本优先读 `gemini` / `antigravity` Keychain，旧版或未检测到系统凭据时再读 `state.vscdb`；导入本身会刷新一次 Google token 后保存为 SkillStar subscription。
- 可覆盖 OAuth client credential 的 provider 按 env → compile-time → `oauth_clients.json` → built-in fallback 解析；要求外部凭证的 provider 保留自己的专用配置模块。
- Antigravity 使用参考项目同源的公开 Google OAuth 客户端作为内置 fallback；仍可用 `SKILLSTAR_ANTIGRAVITY_CLIENT_ID` / `SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET` 或 `~/.skillstar/config/antigravity_oauth.json` 覆盖。这里的 client secret 属于桌面 OAuth 客户端标识，不是用户账号凭证；账号的 access/refresh token 仍只进加密 subscription storage。
- Antigravity 额度先调用 `loadCodeAssist` 获取 plan、credits 和 `cloudaicompanionProject`，再把项目 ID 传给 Cloud Code。项目字段同时兼容字符串和 `{ "id": ... }` 对象。Cloud Code 请求按 daily → daily sandbox → production 回退；优先使用 `retrieveUserQuotaSummary` 返回的用户可见 5h/weekly buckets，只有摘要接口没有可用窗口时才回退到 `fetchAvailableModels` 的模型 quota。汇总卡按最紧张（消耗百分比最高）的窗口计算，`UsageWindow.used` 与 `percent` 始终表示已消耗比例，不能把剩余比例写入 `used`。
- Antigravity 模型列表不是固定枚举：已知模型按产品分组，新增的 Gemini/Claude/GPT/Image 模型只要带 `quotaInfo.remainingFraction` 也必须显示；无法抓取模型额度时保留 plan/credits，同时在 `usage.error` 显示原因，401 仍按认证失效处理。配额刷新优先调用 `retrieveUserQuotaSummary`；只有该 endpoint 明确返回 404/405 时才回退 `fetchAvailableModels`，有效但为空的 summary 不再额外发起模型目录请求。卡片在窄宽度下把已知的 Gemini、Claude/GPT 周额度与 5 小时额度压缩为可扫描的短标签，并保留完整原始标签作为悬浮说明；重置倒计时与进度条同行，避免四个窗口把卡片纵向拉长。

### Anthropic Claude 特例：只读采纳，不参与轮换

- 凭证来源是 Claude Code 自己的登录态：macOS 读钥匙串 `Claude Code-credentials`（account = `$USER` / `$LOGNAME`，取不到时回退字面量 `claude-code-user`），其它平台读 `~/.claude/.credentials.json`。取 JSON 的 `claudeAiOauth.accessToken`。
- **`claudeAiOauth.expiresAt` 是 epoch 毫秒**，与本 crate 其它所有 provider 的秒不同；统一经 `expires_at_seconds()` 归一，禁止直接使用该字段。
- SkillStar **不刷新、不写回**这份凭证。Anthropic 的 refresh token 一次性，谁先刷谁让对方失效；自己去刷会把用户的 Claude Code 登出。每次 refresh 重读本地存储、采纳更新的那一份，本地存储不可用时才回落到绑定时捕获的 token。
- 因为不写回，也就不存在钥匙串 read-modify-write：同一条目里的 `mcpOAuth` 不会被覆盖，用户不会被登出所有 MCP server。
- 钥匙串读取 shell out 到 `/usr/bin/security` 而不是链 `security-framework`：钥匙串 ACL 授权绑定调用方二进制签名，重新编译后授权会失效。
- 没有浏览器腿。`start_login("anthropic")` 就地采纳本地凭证并立即兑现 pending-login 通道；没有本地登录态时在 `start_login` 当场失败，用户看到“先跑一次 `claude`”而不是卡住的等待面板。
- 额度端点 `GET https://api.anthropic.com/api/oauth/usage`，头 `Authorization: Bearer <access_token>` + `anthropic-beta: oauth-2025-04-20`。非公开文档端点，schema 会漂：`limits[]`（`kind` 为 `session` / `weekly_all` / `weekly_scoped`）是权威来源，顶层 `five_hour` / `seven_day` 是逐窗口回落，两者都读。
- **解析失败的粒度是「这条窗口缺失」，不是「这个账号失败」**：未知 `kind`、读不出的百分比、形状不定的 `used_dollars` / `limit_dollars`、无法解析的 `resets_at`，都只让对应的那一条额度条消失。
- 同 host 相邻请求间隔 ≥5s，由 fetcher 内的进程级 gate 串行化（这层在 `refresh_guard` 的 per-catalog 间隔之上）。

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
- 没有 token 交换腿的 fetcher（`anthropic`）在额度请求上复用同一张表的另一半：`Resp::is_auth_error()` 判 401，其余非 2xx 交给 `UsageError::http_status`，传输失败交给 `UsageError::transport`。不允许再写第二套状态码匹配。

### Ollama Cloud

- 本机 `localhost:11434` 没有额度。这张卡读的是 ollama.com Cloud，密钥来自 [Settings → Keys](https://ollama.com/settings/keys)。
- `GET https://ollama.com/api/usage`，`Authorization: Bearer <key>`。公开的 chat/generate 文档不暴露账号额度；该路径无密钥返回 401 `invalid credentials`，有密钥时社区已对过真实账号：`limits.session.usage` / `limits.weekly.usage` 是 0–1 已消耗比例。
- API 不返回重置时间。5h 窗口按 Unix epoch 对齐的 5 小时网格本地推算；周窗口是每周一 00:00 UTC。解析失败按窗口降级，两个窗口都没有才算账号失败。
- 401 走 `BalanceSpec.auth_error_hint`（与 MiniMax Token Plan Key 同一条路径）：卡片显示「请填 Cloud API Key」而不是通用「重新授权」——API Key 没有 reauth 流。
- 卡片走 `DefaultUsageBody`（百分比 + 重置），不进 `bodyRegistry`。

## 本地工具账号切换：CLI 软链与 IDE 适配器

切号的本质是「让目标应用下次读凭证时读到另一个账号的凭证」。SkillStar **不持有凭证**，CLI
账号使用凭证文件快照和软链；CLI 自己轮换 token 时直接写穿到快照，所以不存在「陈旧拷贝」。
Antigravity 和 Cursor 不适合这套整文件软链模型，分别写入它们真实使用的 OAuth 存储。路线选择与后果见
[../../decisions.md](../../decisions.md)。

- `skillstar-app::usage_switch` 是唯一跨 Usage/Models 的账号激活 facade；Tauri command 不直接理解 provider 凭证文件 schema。
- 快照落点 `~/.skillstar/accounts/<catalog_id>/<subscription_id>.json`，权限 0600，走后端解析真实数据目录（`SKILLSTAR_DATA_DIR` 等覆盖继续生效）。**一份快照是整个 CLI 凭证文件**，不是其中一个账号的片段 —— 软链只能整文件替身。
- live 路径必须是 CLI 自己读的那个文件，并尊重上游 env 覆盖：`CODEX_HOME`、`GROK_HOME`、`XDG_DATA_HOME`（OpenCode 用 `$XDG_DATA_HOME/opencode/auth.json`，不是 config 目录）。`SKILLSTAR_TOOL_SYNC_HOME` 沙箱优先级最高，测试不得逃逸。
- 支持哪些 catalog 由切换适配器推导，不是 UI 手抄白名单：CLI 账号走 `usage_switch::target_for`，Antigravity 和 Cursor 走独立的 IDE 凭证适配器。Antigravity 的“当前账号”优先读取 macOS Keychain 的 `gemini` / `antigravity` 条目；没有该条目时读取 `state.vscdb` 中 `antigravityUnifiedStateSync.oauthToken`。Cursor 的当前账号读取其 `state.vscdb` 的 `cursorAuth/accessToken`、`cursorAuth/refreshToken` 和 `cursorAuth/cachedEmail`，都不是 Usage 的 active pin。
- Antigravity 切换顺序：取得 catalog 锁 → 读取并解密目标账号 → 写入并验证 macOS Keychain（当前桌面版本）或生成官方 Unified OAuth protobuf、在 SQLite 事务内写入 `state.vscdb`（旧版/其它平台）→ 回读并校验 refresh token → 最后才落 active pin。目标存储不存在、无法写入或回读不一致时，pin 保持旧值并明确显示“切换未生效”。
- Antigravity OAuth 登录完成后，如果目标卡原本就是 active，会立即按同一适配器把新凭证投影回 IDE；普通刷新也会把 active 卡的 token rotation 投影回 IDE，避免 Usage 与 IDE 分叉。
- Cursor 切换顺序：取得 catalog 锁 → 解密目标 access/refresh token → 在 SQLite 事务内同时写入 `cursorAuth/*` 和 Cursor 镜像 key → 回读 access/refresh/email 校验 → 最后才落 active pin。Cursor 缺少本地 `state.vscdb` 或凭证不完整时不会只改 pin，而是明确返回“切换未生效”。Cursor 本地导入也读取同一组 key；一次 OAuth 会话读取在同一 SQLite 连接和查询中取得四个字段，避免跨连接快照。
- Cursor 的 IDE 进程可能自行轮换 token；Usage 刷新前先采纳仍属于当前卡的本地新 token，刷新后再把新 token 投影回 `state.vscdb`，避免 refresh token 双花和卡片/IDE 分叉。
- 每个 catalog 使用进程内 async mutex + CLI 自己的 OS file lock（Grok 用官方 `auth.json.lock` 并回写 `PID:秒` holder 行；无官方锁的 CLI 用私有 `<file>.skillstar.lock`）。软链消灭的是陈旧拷贝，**不是** refresh token 单次使用的双花竞态，所以锁必须保留。
- activate 顺序：取锁 → 捕获 live 现有凭证（备份 + 归属判定 + 吸收）→ 准备快照 → 原子替换 live 为软链 → 回读校验 → keychain 等副作用 → **最后**才落 pin。任何一步失败都保留旧 pin 与旧 live 文件；软链已换但后续失败时回滚到原来的软链或备份。
- 「失败发生在替换之前还是之后」不靠调用方猜：`usage_switch::error` 里 `ActivationError` 带一个 `Stage`（`BeforeReplace` / `AfterReplace { target_installed }`），回滚与否只看它。替换前失败时旧凭证原封不动，回滚才是破坏；`target_installed=false`（bind 自己失败）时第二存储从未被写过，回滚不再多跑一次 keychain。
- 托管层的失败面是**真枚举不是字符串**：`CustodyError`（路径解析 / 锁 / 读 / 写 / 原子替换 / 软链 / 回读校验 / 归属冲突 / 快照缺失损坏）、`MaterializeError`（订阅行凑不出 CLI 能用的凭证）、`ExternalStoreError`（macOS keychain）。面向用户的中文文案由变体生成，命令层只做 `SwitchOutcome.error` 字符串适配。计数门禁见 `scripts/internal/check_error_strings.sh`。
- 对账判据是**内容不是文件类型**：三态 `LinkedTo / Diverged / Missing`；CLI 比 access_token 字符串，Antigravity 比 refresh_token，不比整个 JSON（CLI 会加自己的字段）。CLI 用 `rename()` 把软链冲成实体文件、内容却一致时判 `LinkedTo` 并静默重建软链。
- pin（`active_per_catalog.json`）是这个三态的缓存，不是第二个真相源；`reconcile` 随时可以从磁盘重建它。UI 的「当前」badge 读 `reconcile_cli_accounts` 命令而不是读 pin（见下面「Usage 卡片与 active 状态」）。
- `reconcile_cli_accounts` 在每个 catalog 自己的 serialization domain 里跑，且对没有本地工具凭证的机器不取 CLI 锁 —— 取锁会为了确认“什么都没有”而先把 CLI 的家目录和锁文件创建出来。
- 删除 subscription 时先 `forget_subscription_session`：删快照，且若它正是当前 live 的软链目标，先把 live 还原成一份实体文件拷贝 —— 悬空软链不是「已登出」，是「登不上」。
- 不向通用 `Subscription` 增加 provider-specific 字段；CLI provider 实现 `CliCredentialTarget`（路径、锁、access_token 提取、身份、materialize、absorb、可选的第二存储），IDE provider 使用独立适配器。

### 软链盖不住的三个洞

- **Codex 在 macOS 以 keychain 为准**，`auth.json` 只是 fallback。activate 时写 keychain，reconcile 时从 keychain 读回并吸收进快照；写入是 **read-modify-write**（`security add-generic-password -U` 会替换整个 secret，直接覆盖会连带清掉 CLI 放在同一 blob 里的其它键）。`SKILLSTAR_TOOL_SYNC_HOME` 已设置时 keychain 托管整体关闭，测试不碰真实登录钥匙串。
- **CLI 用 `rename()` 原子写会把软链换成实体文件**，见上面的三态判据；reconcile 负责重建。
- **Windows 无软链权限时降级为拷贝**，reconcile 每次双向同步。降级是显式且**用户可见**的：`LinkMode::Copy` 经 `SwitchOutcome.link_mode` 透到 DTO 和 UI（切换/重新同步成功后弹降级提示，浮窗常驻一条说明），不是只写 warn 日志。拷贝语义下 CLI 自己轮换的 token 不再自动回流，这件事必须说出口。

### Grok/xAI 特例

- credits endpoint 决定当前 weekly/monthly period；weekly 是严格 percent-only，不能用 calendar-month 金额伪造绝对周额度。
- calendar-month spend 作为次级 credit 展示，不再生成第二条“monthly quota”。零 on-demand cap 不显示。
- 周额度本轮缺失时可携带上次已知 weekly window，避免闪回错误月视图。
- Grok 卡片页脚的“重置额度”是账号级、显式确认的上游 mutation：先调用 `ConsumerUiSvc.GetRemainingResets` 选择最早过期的未过期 token，再调用 `ConsumerUiSvc.RedeemReset(token_id)` 消耗一次真实 reset credit；成功后等待账单投影收敛并重新读取该卡。卡片同时展示这份接口返回的未过期 token 数量（`剩余 N 次`）；没有可用 token 时显示 `剩余 0 次` 并禁用操作。它不等同于刷新，也不允许在供应商分组层面模糊选择账号；没有可用 token 时直接报错且不改本地额度。
- CLI snapshot 用稳定 subject/user identity 归属；冲突 identity fail closed。token 必须满足 Grok CLI scopes，写入前后均验证，并保护外部进程并发改写。

## Usage 卡片与 active 状态

- **「当前」badge 的真相是对账结果，不是 pin。** pin（`get_active_subscriptions`）记录用户点过哪张卡；`reconcile_cli_accounts` 返回每个 catalog 的三态，是 CLI 下次实际会读到的东西。两者冲突时文件赢。
- 三态在 UI 上各自有话说，**不折成布尔**：`LinkedTo` → 绿色「当前」；`Diverged` → 琥珀「本地工具非此账号」并说明 CLI/IDE 现在用的不是这张卡（浮窗还给一条“点重新同步把它指回来”）；`Missing` → 灰色「本地工具未登录」。Diverged/Missing 不得静默渲染成“未激活”。
- pin 说 A 而 live 是 B 时，绿色 badge 挂在 B 上；A 只有在自己被 pin 时才显示 Diverged。卡片高亮环同样跟随对账结果。
- 没有切换适配器的 catalog（纯 API key 等）不在对账 map 里，回落到 pin —— 那里没有文件可以反驳它，pin 就是全部真相。Antigravity 虽然是 IDE，但有独立的 state.vscdb 对账适配器；首帧对账未回来时同样回落到 pin，不会先喊一声“没有当前账号”。
- `setActive` 的返回值是后端真相：只有返回行 `is_active=true` 时，前端才 demote 同 catalog sibling。
- CLI 切换被拒绝时，保留旧 badge，并使用“switch not applied”反馈；不能乐观宣称目标已激活。切换后紧跟一次对账，所以被拒时旧 badge 是被**重新确认**的，不只是没被改。
- card 使用 shell + body registry + primitives；特化 catalog 在 `bodyRegistry.ts` 明确注册 ownsBalance/ownsCredits/reset ownership。
- 卡片空间节奏（Operate）：品牌带只负责认出是哪张卡（logo / 标题 / 本地工具状态 / plan），不是视觉高潮。`UsageCardMetaStrip` 只在有例外时出现（需重登 / 无凭证 / 未由 meter 拥有的 reset / 备注），不是常驻第三段。剩余额度的数字 + 进度条是主阅读目标。费用/续费是页脚工具带里的一行可选事实，与操作同一行，不再做成 KPI 瓷砖；同步时间挂在刷新按钮的 title。主卡与占位卡共享 `usageCardSlotClassName`。供应商网格里卡片吃满 `minmax(280px, 1fr)`，占位卡单独限宽。
- 卡片品牌带必须完整显示账号身份与套餐语义：邮箱标题不得用省略号截断，允许在窄卡内换行；右上角 plan 徽章原样保留上游等级和倍数（例如 `MAX20X` 就显示 `MAX20X`），不得只显示 `MAX` 或 `PRO`，也不得自行插入空格或改大小写。
- 卡片品牌带的账号标题预留两行高度，下面的本地工具状态预留一行固定槽；因此同一供应商内某张卡显示「当前」或邮箱恰好换行时，不会把重置条和额度主体单独推低。超过两行的异常长身份仍允许自然撑开，不能用省略号伪造完整邮箱。
- Usage 页头部提供持久化的邮箱隐私开关；开启后所有卡片只显示遮罩身份，原始邮箱仍留在订阅数据中，不能影响切号、刷新、对账或后端请求。
- 同一供应商的横向卡片使用最高卡片的高度作为行高，较短卡片的主体和页脚保持底部对齐；邮箱换行不会让同一行的操作位置参差不齐。
- 所有额度条共享 `UsageMeter` primitive（标签+已用徽章 / 大号等宽数字 / `ProgressTrack` / 脚注+重置芯片）；货币/绝对/百分比额度读作同一套语法，各渲染器只组合它，不自绘嵌套卡片。鉴权方式、CLI 能力徽标、计费周期和「即将重置」提示文案不占卡片：前三项在编辑对话框，重置紧急度由倒计时着色和卡片描边承担。
- 重置倒计时归属唯一律：meter 只在 `windowRendersOwnReset(window)` 为真时渲染自身重置芯片，否则由 card MetaStrip 顶部显示；二者互补，同一 reset 绝不出现两次。
- Cursor 的 Total/分类明细只保留卡片级主 reset，分类行不重复显示同一个 7d 倒计时；DeepSeek 的余额卡以可用余额为唯一主视觉，状态、分析提示和余额构成作为无嵌套卡片的次级分组。
- 主卡与独立窗口共享逻辑 body，不共享 chrome。浮窗使用 dark chrome + `LightBodySurface`，compact body 的品牌 CSS vars 必须来自 `brandThemes.ts`。
- 每个 catalog 的品牌 header、bar 和 glow 只在 `src/features/usage/lib/brandThemes.ts` 定义；卡片内不得硬编码另一套颜色。
- 有品牌主题 ≠ 有特化 body。只输出「百分比 + 重置时间」这种限流窗口的 catalog（`codex`、`anthropic`）走 `DefaultUsageBody` → `UsageWindowBar` → `UsageMeter`，**不在 `bodyRegistry` 注册**；`bodyRegistry.test.ts` 把特化集合锁死，就是为了让“新增 catalog 顺手造一个 `DefaultUsageBody` 克隆”这件事失败。
- 费用/renew、delete 和 API key action 按 surface matrix 决定；浮窗不暴露 API key copy bar。

## 请求构建

- 所有 fetcher 走 `skillstar-usage::request::Req`：统一附带 header/bearer/body、把非 2xx 归一为 `RequestError::HttpStatus`，让各 provider 只写响应解析。
- 底层 client 一律由 `crate::http_client::usage_http_client()` 提供，透传 `probe_http_client` 的代理设置；fetcher 不自建 `reqwest::Client`。

## 前端类型契约

- `/usage` 的所有后端形状由 ts-rs 从 Rust 生成，落在 `src/types/generated/`；`src/features/usage/types.ts` 只是 re-export barrel，不再手写任何字段。改 Rust 结构后跑 `bun run types:gen` 并提交生成结果，`scripts/internal/check_generated_types.sh` 会在产物落后于 Rust 源时让构建失败。具体清单以 `#[derive(TS)]` 为准，文档不抄字段表。
- 纯数据、无行为的类型（`skillstar-usage` 的枚举与 usage 快照树）直接 derive `TS`——它们本来就是 wire 形状。有自己重构节奏的域类型不直接暴露给 ts-rs，改由 `skillstar_app::usage::dto` 的投影拥有前端契约，理由与后果见 [../../decisions.md](../../decisions.md) D-034。
- 编辑既有订阅时 `SubscriptionDto.oauth_region` 把已存 region 带回前端。补上这个字段之前，编辑对话框只能回落到默认 region，region-aware provider 一旦出现就会在编辑时静默丢失授权区域。`UpdateSubscriptionInput` 目前仍无该字段，因此“编辑时改 region”尚未支持。

## 前端刷新与错误

- `useUsageData` 管理 list、refresh、focus 和 active-changed 的 cache 更新；window 自己保持生命周期，不挂主窗口 Context。
- 写路径、refresh 与 CLI 对账读取共用 `useUsageData` 内单一 FIFO 队列（`enqueue`）：任何写 `subscriptions`/`alerts`/`cliAccounts` 的调用都排在此前所有 list/refresh/mutation 之后，避免在途 refresh 用旧列表（或旧对账结果）覆盖刚完成的写入。队列链保持不 reject，单次失败不会把队列卡死。队列内部的调用只能走非入队的私有实现，再入队会等一个永远开不了的槽。
- refresh、OAuth finalization、metadata 编辑、删除和切换必须进入后端同一 catalog serialization domain。
- `refresh_all_subscriptions` 接受可选 `catalogId`：只刷该 catalog 的行，其余行按存储快照原样返回（返回值始终是完整列表）。单 provider 页面必须传，避免一次点击横扫所有厂商端点。
- load error、switch error、cliFailed 和 usage.error 分层展示，不把所有失败折成“暂无数据”。

## Dock 与 Tray 菜单额度显示

- macOS 右键点 Dock 图标，或在系统状态栏 Tray 小图标菜单中，列出各订阅额度：每行 `<账号> · <额度状态>`（如 `剩余 N%`、`余额 $M`、`剩余 K 积分`、`未同步`），按最紧张（剩余百分比最少）在前排序。N% 是该订阅「消耗最高的那条额度窗口」的剩余份额。
- 分层：纯函数 `skillstar_usage::dock_usage::snapshot_menu_summary`（对快照，支持多语言与全品类配额）→ 读存储+拼行的 `skillstar_app::usage::dock_menu_lines_for_lang` → Tauri 胶水 `src-tauri` `core::dock_menu` 与 `core::app_shell::build_tray_menu`。
- macOS Dock 菜单经 app delegate 的 `applicationDockMenu:` 提供，通过 objc2 动态注入 Tauri delegate 类；Tray 菜单在系统状态栏常驻。
- 触发点：启动（`setup_tray` 与 `dock_menu::refresh`）、订阅增删改、排序、刷新、导入与 CLI 切换时统一通过 `refresh_tray_and_dock_menu(&app)` 刷新。

## 验证

```bash
cargo test -p skillstar-usage -p skillstar-app
bun run test -- src/features/usage
```

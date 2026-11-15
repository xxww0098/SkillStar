# Accounts（账号工作台）

状态：active

账号工作台按 agent 管理多份登录凭据，并把选中的那份写进本机客户端，让 CLI/IDE 真的换成它。
界面只存在于 nsh 应用（`gui/`）。产品二进制直接启动 nsh 宿主，账号入口是侧栏顶部的模式胶囊「技能 / 账号」，切到账号后左栏列出 `ss-skills` 注册表与 `ss_accounts::Agent::supported()` 的交集 agent。

## 范围

- 当前覆盖九家 agent，接 `ss-skills` 注册表的 id：`codex`（Codex）、`claude-code`（Claude Code）、`cursor`（Cursor）、`antigravity`（Antigravity）、`gemini-cli`（Gemini CLI）、`grok`（Grok）、`kiro`（Kiro）、`zcode`（ZCode）、`devin-desktop`（Devin Desktop）。受管清单与侧栏顺序以 [`crates/ss-accounts/src/lib.rs`](../../../crates/ss-accounts/src/lib.rs) 的 `Agent::supported()` 及 [`crates/ss-accounts/src/agents.rs`](../../../crates/ss-accounts/src/agents.rs) 的 `spec()` 为唯一代码真相；各家认证腿与落点见下「每家认证腿与落点」。
- 登录令牌（`oauth`）、额度抓取（`usage`）、账号库与切号写回（`store` / `switch` / `service`）统一内聚在 `crates/ss-accounts`。
  UI（经 `ss-gui-bridge`）只面对 `ss-accounts` 的顶层 facade 与 `AccountView`，凭据不出 crate。

## 页面

主导航固定在应用左侧栏（`Sidebar.tsx` 的 `AccountsProviderMenu`），列 `ss-skills` 注册表与 `ss_accounts::Agent::supported()` 的交集（复用 `src/chrome/agentRail.tsx` 的 `AgentIcon` 品牌 SVG 图标 + 名称 + 该 agent 的账号数；与技能导航一致，选中行采用 `panel_active` 底色 + `1px accent` 边框 + `accent_fg` 文字，未选中无底色，由 `agent_icons.rs` 在浅深主题下统一处理单色/彩色 SVG 对比度）。账号页主面板顶部**不设独立页头栏，也不在页头或正文重复渲染切换胶囊或纵向窄栏**，直接以悬浮圆角工具栏起始；工具栏复用 `PageBar` 导出的同一组几何常量（`TOOLBAR_H` 带、顶端 10px、46px 卡片、14 圆角、12px 内边距），与其余五个页面同高同位。页面本身不画自己的底色，直接坐落在壳面板的 `panel` 表面上——和技能页同一底色，两侧留白不再比技能页深一档。卡片区仍受 `CARD_ROW_W`（三列 `SKILL_CARD_W`）宽度约束。

主区由上至下依次为：

1. **悬浮圆角工具栏**：左侧含实时搜索框（匹配名称、邮箱、ID 与套餐）、三态视图切换胶囊（紧凑 `compact` / 列表 `list` / 卡片网格 `grid`）、套餐与异常筛选下拉（全部、各套餐档位、`需重新授权`、`额度异常`）以及排序菜单（默认当前优先 / 按综合配额 / 按最近使用 / 按添加时间）配升降序切换按钮；右侧含主色 `+` 添加账号按钮、全部刷新按钮（仅正在刷新的卡旋钮）与隐私打码开关（对邮箱与显示名称脱敏，不影响搜索匹配）。
2. **全选批量操作条**（当前 agent 存在账号时显示）：左侧全选复选框、已选计数与取消选择；居中显示「全部 N」计数胶囊与快捷 `+` 添加按钮；选中任意账号后右侧显现「刷新选中」与「删除选中」（走共享确认框）。
3. **可关闭状态/错误横幅**：切换账号成功（展示「已切换至 <账号>」，开启隐私模式时同步脱敏）或动作出错时出现在列表上方，可手动关闭。
4. **多视图内容区**：首次加载且尚无数据时展示与卡片网格同几何的 `AccountsSkeletonGrid` 骨架占位；无账号时居中展示空态引导与「添加账号」按钮（已有账号后不在网格末尾常驻虚线占位卡）；筛选无结果时展示无匹配提示与「清除筛选」按钮。
   空态的虚线面板**占满工具栏以下的整个窗口**：滚动内容至少与视口等高（`Scroll` 的 `contentStyle` 带 `flexGrow: 1`），页内容列与空态根节点随之 `flexGrow: 1`，`minHeight: 240` 只作极矮窗口的下限；引导图标、标题、说明与「添加账号」按钮始终在该面板内垂直居中，面板下方不再留空白带。

添加对话框采用顶部三 Tab（`OAuth 授权` / `Token / JSON` / `本地导入`），不再使用二次下钻入口页；其中 OAuth 段按 `LoginFlow` 分支渲染：

| flow | 界面 |
| --- | --- |
| `LocalCallback` | 授权链接展示 + 「复制链接」+ 「在浏览器打开」+ 可粘贴回调链接或授权码的输入行 |
| `RemotePoll` | 设备码（支持复制）+ 验证地址 + 轮询等待（没有粘贴框） |
| `SchemePaste` | 提示粘贴 scheme 协议回调 URL |
| `Immediate` | 无用户操作，立即完成（`supportsOauth = false` 的本地客户端走这条，如 Claude Code、Devin Desktop；页内默认落到「本地导入」Tab 并提示改用「本地导入」或「Token / JSON」） |

等待态显示旋转指示与「取消」。

## 卡片

复刻 cockpit-tools 的账号卡解剖（D-109 及其后续修订），数字固定，颜色一律走 `paletteToken()`，兼容深色/浅色双主题：

- 卡根（网格视图）：纵向 flex、gap 10、padding 12 14、圆角 16、1px 描边、卡面底色、`min-height` 228、宽固定 `SKILL_CARD_W`。
  hover 只换底色与描边色——本仓库的状态切换零过渡契约继续有效，不做位移、半透明模糊与阴影过渡。
- 身份行（`card-top`，横 flex，gap 8）：勾选复选框（18×18 热区）→ 账号名 14/600 单行省略（`flex:1`，受隐私开关脱敏）→ 当前生效胶囊（`ok` 底白字 11/600、padding 3 10、圆角 999）→ 错误胶囊（额度读取失败时 `danger` 软底、`danger` 图标/短标 + Tooltip 详述）→ 套餐徽标 `margin-left:auto` 靠右（11/600 大写、letter-spacing .05em、padding 4 10、圆角 999）。
  套餐配色：plus 绿（`ok_bg`/`ok`）、team 紫、pro 蓝底白字（`accent`/`on_accent`）、ultra 琥珀金（`amber_bg`/`amber_fg`）、enterprise 暖橙（`warn_bg`/`warn`）、free/其它中性灰（`well`/`fg_muted`）。
- 登录行：当与主名称不同时显示 11px 次要色脱敏邮箱或 account id；认证失效时换成 `danger` 文案「登录已失效，需重新授权」并在行内给一个 24 高的描边「重新授权」按钮。
- 额度内框（`well` 软底圆角 10 容器，padding 10、1px `border_soft` 描边、纵向 gap 8）：当窗口数 >= 2 时按**双列并排**布局（如 `5h` 与 `Weekly` 并列一行），每个窗口三层——顶行（label 11/600 次要色 + 可选 chip 10/600 + 右对齐剩余百分比 11/700 tabular-nums，按**剩余**率分段：>=80 `ok`、>=40 `warn`、>=10 橙 `amber`、<10 `danger`）、中行（轨道高 4px、圆角 999）、底行（10px 弱化色紧凑倒计时，如 `4h 54m`，完整重置时刻挂在 Tooltip，已重置显示「已重置」）。顺序固定：`five_hour` -> `week` -> 其它窗口 -> 内框底部分隔后的 Credits 行（credits 无进度条，只显示数值或「无限」）。没有额度时居中写 12px「暂无额度数据」。
- 底部操作行：`margin-top:auto`，上方 1px 分隔线（`padding-top` 10），左侧展示最近使用/创建时间（`lastUsedAt ?? createdAt`），右侧排布 28×28 图标按钮（圆角 8、图标 14）：切换（非当前用 `play` 绿色图标；当前生效展示禁用 `checkCircle`）、单卡刷新（仅刷新中的卡旋钮）、删除。
- 列表视图与紧凑视图：列表视图以圆角表格展示复选框、身份与徽标、横向额度条、最近使用时间与操作按钮；紧凑视图沿用三列窄卡展示勾选、脱敏身份、彩色圆点百分比胶囊与快速切换按钮。
- 卡内只放 身份 / 套餐 / 额度 / 时间 / 操作。cockpit 卡内的代理、API 服务、速度档、会话、备注/2FA 等子系统不复刻。

「当前生效」是磁盘真相，不是缓存 pin：list 渲染时读 `ss_accounts::reconcile()` 的客户端实况（D-109）。

## 添加与登录

- **OAuth**：`start_login` 返回 `LoginStart`（flow / auth_url / user_code / 验证地址 / 间隔 / 过期），
  页面按 flow 渲染；`submit_login_callback` 投递粘贴的回调；`finish_login` 阻塞拿到结果并落库。
  同一 agent + 同一身份的旧行**原地替换**，不产生重复卡。
- **Claude Code 没有浏览器登录腿**：`start_login` 返回 `Immediate`，`finish_login` 直接以 `AuthRequired` 拒绝并提示
  「请用从本机导入或粘贴令牌」。这是如实能力，不是缺陷：Claude Code 的凭据只从本机存储或粘贴令牌进入账号库。
- **从本机导入**：读客户端当前凭据组装账号行。Codex 读 `~/.codex/auth.json`（尊重 `CODEX_HOME` 与
  `SKILLSTAR_TOOL_SYNC_HOME` 覆盖）。
- **粘贴令牌**：接受裸 access token，或含 `access_token` / `refresh_token`（或 Cursor 的驼峰
  `accessToken` / `refreshToken`）的 JSON。落库前尽力核对一次额度：`AuthRequired` 拒绝落库，
  `Transient` 允许落库但额度留空。

## 切号写回

切号 = 把账号库里的令牌写进该 agent 的客户端凭据，然后读回校验。写回失败不移动 `last_used_at`、不改索引，
并把旧凭据写回（不留半截）。

| agent | v1 落点 | 说明 |
| --- | --- | --- |
| codex | `<codex home>/auth.json` | 写 `OPENAI_API_KEY: null` + `tokens{id_token,access_token,refresh_token,account_id}` + `last_refresh`；v1 **不写钥匙串**，macOS 上 Codex 可能优先读 `Codex Auth` 钥匙串条目，如实标注 |
| claude-code | `<claude config dir>/.credentials.json` | 只读改写 `claudeAiOauth` 段（兄弟键如 `mcpOAuth` 原样保留），`expiresAt` 是**毫秒**；macOS 钥匙串 `Claude Code-credentials` 不桥接 |
| cursor | `<Cursor userData>/User/globalStorage/state.vscdb` | `ItemTable` 单事务 upsert `cursorAuth/accessToken`、`cursorAuth/refreshToken`、`cursorAuth/cachedEmail`、`cursorAuth/authId`，并镜像 `cursor.accessToken` 与 `cursor.email`；写完回读校验 |
| antigravity | macOS 钥匙串 `gemini` / `antigravity` + `~/.gemini/jetski-standalone-oauth-token`（+ `antigravity-cli/` 存在时同目录 token 文件）+ `<userData>/User/globalStorage/state.vscdb` | 三处都写成功才算切换：钥匙串值 `go-keyring-base64:` 前缀；vscdb 键 `antigravityUnifiedStateSync.oauthToken`（base64+protobuf，sentinel `oauthTokenInfoSentinelKey`）；写完解出同一 refresh_token 才成功，任一步失败按快照回滚（D-114） |
| gemini-cli | `~/.gemini/oauth_creds.json` + `~/.gemini/google_accounts.json` | Google 授权码 + 回环 `/oauth2callback`（端口 8085，PKCE）；**不写钥匙串** |
| grok | `~/.grok/auth.json`（`GROK_HOME` 可覆盖）的 scope 条目 | xAI 设备码（`auth.x.ai`，404 时退回环 56121）；**不写钥匙串** |
| kiro | `~/.aws/sso/cache/kiro-auth-token.json` + `<sha1(startUrl)>.json` + Kiro `profile.json` | 门户 PKCE 或 AWS IdC 设备码（注册信息进 `provider_state`）；**不写钥匙串** |
| zcode | `~/.zcode/v2/credentials.json`（`enc:v1:` 自加密）+ `setting.json` 家族模式 | `zcode://` 深链粘贴（zai / bigmodel），换票 `zcode.z.ai/api/v1/oauth/token`；**不写钥匙串** |
| devin-desktop | `state.vscdb`（`Devin`，回退 `Windsurf`）三行 | **没有浏览器登录腿**：只能从本机导入或粘贴 apiKey |

## 存储与安全

- 路径：`~/.skillstar/secrets/accounts/accounts.json`（索引，明文元数据）+
  `~/.skillstar/secrets/accounts/records/<id>.json`（整条记录，令牌字段 AES-256-GCM 密文），
  文件权限 0600，写锁 `~/.skillstar/runtime/locks/accounts/storage.lock`。
- 加密：key = SHA-256(machine uid)，12 字节随机 nonce，存 `base64(nonce || ciphertext)`（沿用旧实现的方案，
  换成别的派生会让已有记录全部解不开）。
- 明文令牌不出 crate：`AccountView` 与任何 Debug 或序列化输出都不得含令牌。
- 路径语义全部走 `ss_core::infra::paths`，`SKILLSTAR_DATA_DIR` 覆盖继续生效。

## 验证

- `cargo test -p ss-accounts`（登录、额度抓取、加密存储、切号与错误分类）。
- `nsh check gui` 与 `nsh test gui`（页面与桥；假桥注入 AccountView fixture）。
- 真实客户端重启认写回 **未验证**：v1 只证明「写-回读-校验」在文件/SQLite 层成立。

## 未覆盖（如实登记）

- 多开实例、重置卡、API-key 家族、GitHub Copilot、ChatGPT 登录：本版不做（D-108）。
- macOS 钥匙串写回：不做，见上表。
- cockpit 的拖拽排序、账号备注/2FA、标签/分组持久化、明文凭据导出：不复刻。

## 已知缺口

- LLM 翻译的端点与密钥缺少设置页写入入口：当前大模型翻译只从 `ss_core::translation` 的手动 key 存储读取（`save_api_key` 全仓尚无前端设置入口），尚未与当前账号工作台的凭据接通；如需打通，可在设置页翻译分区补「端点 + 密钥」输入框或对接账号读取。

## 每家认证腿与落点（九家速查）

| agent | 登录腿 | 额度来源 | 导入 | 对账判据 |
| --- | --- | --- | --- | --- |
| codex | OpenAI 回环（可粘贴） | `chatgpt.com/backend-api/wham/usage` | 本机 `~/.codex/auth.json` / 粘贴令牌 | account id，其次指纹 |
| claude-code | 无（只能导入/粘贴） | `api.anthropic.com/api/oauth/usage` | `~/.claude/.credentials.json` | 令牌指纹 |
| cursor | 远程轮询 | `cursor.com/api/usage-summary` | `state.vscdb` | 指纹，其次 email |
| antigravity | Google 回环（localhost:51121） | Cloud Code `loadCodeAssist` 系 | Keychain（只读优先）/ vscdb | email，其次指纹 |
| gemini-cli | Google 回环 `/oauth2callback:8085` | Cloud Code Assist（`loadCodeAssist` → `onboardUser` → `retrieveUserQuota`） | 两份 `~/.gemini` 文件 | email，其次指纹 |
| grok | xAI 设备码（退回环 56121） | `cli-chat-proxy.grok.com` 的 `/v1/billing` 与 `/v1/settings` | `~/.grok/auth.json` | user_id，其次 email |
| kiro | 门户 PKCE 或 AWS IdC 设备码 | `q.{region}.amazonaws.com/getUsageLimits` | AWS SSO 缓存 token 文件 | userId / email，其次指纹 |
| zcode | `zcode://` 深链粘贴 | `billing/balance`（JWT + `X-Device-Mid`）+ 上游 `quota/limit` | `~/.zcode/v2/credentials.json` | userInfo userId，其次 JWT sub |
| devin-desktop | **无**（只导入/粘贴） | `SeatManagementService/GetUserStatus` | `state.vscdb` | email |

keychain 口径：只有 antigravity 写系统钥匙串（D-114，客户端只认钥匙串）；其余八家一律不写，读得到就读。


## 新增受管 Agent 统一配方（Recipe）

新增一个支持账号管理的 Agent（设注册 ID 为 `<id>`，Rust 模块蛇形命名为 `<snake>`）时，按下面清单改造。前端工作台、图标着色、空态与添加弹窗均由后端 `Accounts.agents()` 下发的能力元数据自动装配，**不要在 UI 或 Facade 层新增按 `<id>` 命名的分支或品牌图标表**。

### 一、下次新增改哪些文件（按执行顺序）

| 顺序 | 文件路径 | 目标符号 / 产物 | 具体改动 |
| --- | --- | --- | --- |
| 1 | [`crates/ss-skills/src/agents/builtin.rs`](../../../crates/ss-skills/src/agents/builtin.rs) | `BUILTIN_AGENT_DEFS` | 若该 Agent 尚未在 `ss-skills` 注册为内置 Agent，先按 [`agents/README.md`](../agents/README.md) 轴①补齐一行定义（`ss_accounts::Agent::id()` 须与已注册的 `AgentProfile.id` 一致，或可在 `ss_skills::agents::compatible_profile_id` 找到对应 profile） |
| 2 | `gui/assets/agents/<id>.svg` | 品牌 SVG 文件 | 放入紧凑居中的矢量图标（规范见下「图标规范」） |
| 3 | [`gui/native/src/agent_icons.rs`](../../../gui/native/src/agent_icons.rs) | `agent_icon_svg` / `normalize_agent_icon_id` | 在 `agent_icon_svg` 的 `match` 增加 `"<id>" => include_bytes!("../../assets/agents/<id>.svg")`；仅当 SVG 文件名与技能 profile ID 不同时才在 `normalize_agent_icon_id` 增加别名映射（如 `"claude-code"` 共用 `"claude"`） |
| 4 | [`crates/ss-accounts/src/lib.rs`](../../../crates/ss-accounts/src/lib.rs) | `pub enum Agent` / `Agent::supported()` | 新增枚举变体（如 `MyAgent`），并在 `Agent::supported()` 返回切片中的目标位置插入 `Self::MyAgent`（该切片决定侧栏展示顺序与 `reconcile` 遍历顺序） |
| 5 | `crates/ss-accounts/src/oauth/agents/<snake>.rs` + [`mod.rs`](../../../crates/ss-accounts/src/oauth/agents/mod.rs) | `start` / `refresh`（或复用 `immediate_auth_required`） | 新建登录与令牌刷新模块，并在 `oauth/agents/mod.rs` 声明 `pub(crate) mod <snake>;` |
| 6 | `crates/ss-accounts/src/usage/fetch/<snake>.rs` + [`mod.rs`](../../../crates/ss-accounts/src/usage/fetch/mod.rs) | `pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome>` | 新建额度抓取与纯函数解析模块（响应复杂时拆 `<snake>_parse.rs`），并在 `usage/fetch/mod.rs` 声明 `pub mod <snake>;` |
| 7 | [`crates/ss-accounts/src/switch/paths.rs`](../../../crates/ss-accounts/src/switch/paths.rs) + `crates/ss-accounts/src/switch/<snake>.rs` + [`mod.rs`](../../../crates/ss-accounts/src/switch/mod.rs) | 路径函数 + `credential_path` / `write` / `read` / `snapshot` / `live_identity` | 在 `paths.rs` 新增基于 `&PathRoots` 的路径解析；新建 `<snake>.rs` 与 `<snake>_tests.rs` 实现本机读取、切号写回与回滚，并在 `switch/mod.rs` 声明 `pub mod <snake>;` |
| 8 | [`crates/ss-accounts/src/agents.rs`](../../../crates/ss-accounts/src/agents.rs) | `spec(agent: Agent) -> AgentSpec` / `SUPPORTED_IDS` | 在 `match agent` 的编译器穷尽分支补齐 `AgentSpec`，并同步更新同文件 `#[cfg(test)] pub(crate) const SUPPORTED_IDS` 长度与数组项 |
| 9 | 文档速查表 | 本页两表 + [`usage/README.md`](../usage/README.md) | 在本文件「切号写回」「每家认证腿与落点」及 [`usage/README.md`](../usage/README.md)「每家端点与解析」各补一行协议事实 |

### 二、零改动层（无需修改、禁止添加 Agent 分支）

- **域编排与存储层**：[`crates/ss-accounts/src/service.rs`](../../../crates/ss-accounts/src/service.rs)、[`reconcile.rs`](../../../crates/ss-accounts/src/reconcile.rs)、`store/`、`record.rs`、[`oauth/mod.rs`](../../../crates/ss-accounts/src/oauth/mod.rs)、[`usage/mod.rs`](../../../crates/ss-accounts/src/usage/mod.rs)、[`token_import.rs`](../../../crates/ss-accounts/src/token_import.rs) 全部通过 `Agent::supported()` 与 `agents::spec(agent)` 动态派发。
  - *仅在两种特殊协议下才需扩展共用原语*：① 客户端粘贴格式不是裸 token 也不是 JSON、而是类似 `go-keyring-base64:` 的专有外层封装时，在 `token_import.rs` 扩展解码入口；② 单次切号需跨多种全新存储介质做原子回滚时，在 `switch/mod.rs` 的 `ClientSnapshot` 增加枚举变体（单文件 JSON 用 `ClientSnapshot::File`，多文件用 `ClientSnapshot::Files`，VS Code 系数据库用 `ClientSnapshot::Items`）。
- **原生 GUI 桥接**：[`gui/native/src/facades/accounts.rs`](../../../gui/native/src/facades/accounts.rs) 的 `Accounts::agents()` 自动遍历 `Agent::supported()` 并序列为 `{ id, displayName, supportsOauth, localSource }`，**无需修改任何方法或单测**。
- **前端账号页与文案**：[`gui/src/pages/accounts/`](../../../gui/src/pages/accounts/index.tsx) 全部视图组件（`rail.tsx`、`index.tsx`、`state.ts`、`views.tsx`、`states.tsx`、`addDialog.tsx`、`logic.ts`、`meters.tsx`、`bridge.ts`）与 [`gui/src/services/translation.ts`](../../../gui/src/services/translation.ts) **零改动**。

### 三、各改动点实现契约与可复用基建

#### 1. 图标来源与浅深主题规范（`assets/agents/<id>.svg` + `agent_icons.rs`）

- **素材与 `viewBox`**：从官方或开源社区图标库（如 Simple Icons / 厂商仓库）取 SVG，精简冗余元数据并居中到紧凑正方形 `viewBox`。
- **单色与彩色着色规则**：
  - 单色剪影图标统一将浅色主题下需跟随前景色反转的 `fill` / `stroke` 写成 `#e8eef8`（与 `agent_icons.rs` 的 `DARK_MONO_INK` 精确匹配）；浅色主题（`paper`）下 `agent_icon_data_uri` 会自动将 `#e8eef8` 替换为 `#18181b`（`PAPER_MONO_INK`）。
  - 品牌多色图标保留原色即可，但必须同时在深色（`current`）与浅色（`paper`）侧栏的 `panel_active` 选中底色下清晰可辨。
- **统一渲染**：侧栏与空态引导统一复用 [`src/chrome/agentRail.tsx`](../../../gui/src/chrome/agentRail.tsx) 的 `<AgentIcon id={...} />`，禁止在 `pages/accounts/` 自建 Canvas `Path2D` 图标表。

#### 2. OAuth 登录与刷新模块（`crates/ss-accounts/src/oauth/agents/<snake>.rs`）

模块需对 `crate::oauth::agents` 暴露两个入口（`ProviderState` 如 Kiro 需要传额外注册状态时 `refresh` 可直接收第 2 个 `Option<&Value>` 参数，否则用 `agents.rs` 闭包忽略）：

```rust
pub(crate) fn start(login_id: &str, opts: &StartOptions) -> OAuthResult<Started>
pub(crate) fn refresh(tokens: &TokenSet) -> OAuthResult<TokenSet> // 或带 Option<&serde_json::Value>
```

按客户端支持的登录方式选择对应复用基建：

| 登录形态 | `AgentSpec.oauth.supported` | 复用基建与参考实现 |
| --- | --- | --- |
| `LoginFlow::LocalCallback` | `true` | `oauth::loopback`、`oauth::pkce`、`oauth::agents::code_from_query`；参考 [`codex.rs`](../../../crates/ss-accounts/src/oauth/agents/codex.rs)、`gemini_cli.rs` |
| `LoginFlow::RemotePoll` | `true` | 轮询 / 设备码流程；参考 [`cursor.rs`](../../../crates/ss-accounts/src/oauth/agents/cursor.rs)、`grok.rs`、`kiro.rs` |
| `LoginFlow::SchemePaste` | `true` | 自定义协议深链解析；参考 `zcode.rs` |
| `LoginFlow::Immediate` | `false` | 直接调用 `oauth::agents::immediate_auth_required(login_id, OAuthError::AuthRequired(...))`；参考 [`claude_code.rs`](../../../crates/ss-accounts/src/oauth/agents/claude_code.rs)、`devin_desktop.rs` |

#### 3. Usage 额度模块（`crates/ss-accounts/src/usage/fetch/<snake>.rs`）

- 暴露唯一入口：`pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome>`。
- 除「刷新会作废桌面 IDE 授权」等特殊情况外（见 `antigravity.rs`），读取前先调用 `super::tokens_for_read("<id>", creds)?` 自动在临近过期时走 `oauth::refresh`。
- 所有 HTTP 请求必须通过 [`usage/fetch/mod.rs`](../../../crates/ss-accounts/src/usage/fetch/mod.rs) 的 `super::http_get` 或 `super::http_post_json`（底层调用 `ss_core::infra::http_client::probe_http_client`，强制遵守用户代理配置与 20s 超时）；非 2xx 状态码统一经 `super::classify_status` 映射（401 → `AuthRequired`，429/5xx → `Transient`，其余 → `Unreadable`）。
- 窗口不变量：`UsageWindow.remaining_percent` 必须填写**剩余百分比**（`0..=100`）；`five_hour` / `week` 等键名规范见 [`usage/README.md`](../usage/README.md)。

#### 4. Switch 切号与本机读取（`crates/ss-accounts/src/switch/paths.rs` + `switch/<snake>.rs`）

- **路径解析（`switch/paths.rs`）**：路径函数只能基于注入的 `&PathRoots` 计算，且 `roots.sandbox_home`（由 `SKILLSTAR_TOOL_SYNC_HOME` 控制）必须永远优先于用户目录环境变量与真实 `$HOME`。若客户端自身支持专属环境变量（如 `CODEX_HOME`、`CLAUDE_CONFIG_DIR`、`GROK_HOME`），在 `PathRoots` 中增加对应字段并在 `PathRoots::from_env()` 读取。
- **五个必需函数（对应 `SwitchDispatch`）**：
  ```rust
  pub fn credential_path(roots: &PathRoots) -> PathBuf
  pub fn write(roots: &PathRoots, request: &SwitchRequest<'_>) -> AccountResult<SwitchOutcome>
  pub fn read(roots: &PathRoots) -> AccountResult<Option<LocalCredential>>
  pub fn snapshot(roots: &PathRoots) -> AccountResult<ClientSnapshot>
  pub fn live_identity(identity: &Identity) -> Option<String>
  ```
- **三套可直接复用的写回/快照原语**：
  1. **单 JSON 文件**：复用 `switch::mod` 的 `read_json_file`、`write_restricted`（原子写入 + `0600` 权限）、`snapshot_file`（生成 `ClientSnapshot::File`），写时只更新凭据字段并保留客户端其它键；参考 `codex.rs`、[`claude_code.rs`](../../../crates/ss-accounts/src/switch/claude_code.rs)。
  2. **多文件客户端**：快照按写入顺序记录多个 `(PathBuf, Option<Vec<u8>>)` 到 `ClientSnapshot::Files`，由 `switch::restore` 自动逆序放回；参考 `gemini_cli.rs`、`kiro.rs`、`zcode.rs`。
  3. **VS Code / Electron 系 `state.vscdb`（`ItemTable`）**：
     - 路径复用 `switch/paths.rs` 的 `ide_user_data_dir(roots, "<AppDir>")` 定位 `User/globalStorage/state.vscdb`。
     - 数据库操作复用 [`switch/vscdb.rs`](../../../crates/ss-accounts/src/switch/vscdb.rs) 的 `open_existing`、`read_value`、`read_values`、`apply_rows`、`write_rows`（快照使用 `ClientSnapshot::Items`，`switch::restore` 自动通过 `vscdb::restore_items` 回滚）；参考 `cursor.rs`、[`devin_desktop.rs`](../../../crates/ss-accounts/src/switch/devin_desktop.rs)。

#### 5. 单表接线（`crates/ss-accounts/src/agents.rs`）

在 `spec(agent: Agent) -> AgentSpec` 新增分支，填写：
- `id`、`display_name`、`local_source`（添加到弹窗「本地导入」Tab 展示的客户端凭据位置说明）。
- `oauth`（`supported`、`refresh_skew_seconds`、`start`、`refresh`）、`usage`、`switch` 三个子域分派器。
- `provider_account_id`：粘贴令牌导入时从 token 提取上游账号 ID；不需要的可填模块私有函数 `no_account_id`，需要解 JWT 的可复用 [`identity.rs`](../../../crates/ss-accounts/src/identity.rs) 的 `jwt_subject_tail` / `jwt_nested_claim`。
- 同步将新 `<id>` 加入 `#[cfg(test)] pub(crate) const SUPPORTED_IDS`。

#### 6. GUI 自动装配与设计骨架（无需改前端代码）

接入后端表后，GUI 的五层骨架会按 `Accounts.agents()` 下发的 `AgentRow`（`{ id, displayName, supportsOauth, localSource }`）自动呈现与现有 Agent 一致的外观与交互：
1. **左侧品牌导航行**（[`rail.tsx`](../../../gui/src/pages/accounts/rail.tsx)）：`AccountsProviderMenu` 渲染 `<AgentIcon>` + `displayName` + 实时账号计数，选中态与 `SkillsNav` 统一为 `panel_active` 底色 + `1px accent` 边框 + `accent_fg` 文字。
2. **悬浮工具栏**（[`index.tsx`](../../../gui/src/pages/accounts/index.tsx)）：统一复用 `PageBar` / `CARD_ROW_W` 尺寸，提供搜索、三态视图切换、筛选与排序。
3. **三态数据视图**（[`views.tsx`](../../../gui/src/pages/accounts/views.tsx) + [`logic.ts`](../../../gui/src/pages/accounts/logic.ts)）：网格卡片（`AccountCardView`）、紧凑三列卡（`AccountCompactView`）与列表行（`AccountListView`），自动复用 `planKind` 徽标配色、`remainingTone` 剩余百分比阈值着色与 `formatCreditValue` 额度格式化。
4. **加载骨架与空态**（[`states.tsx`](../../../gui/src/pages/accounts/states.tsx)）：首帧加载自动渲染与三列卡片同尺寸的 `AccountsSkeletonGrid` 脉冲占位；空列表时在全高虚线容器内居中显示 `<AgentIcon>`、`displayName` 与「添加账号」按钮；筛选无匹配时显示 `AccountsNoMatchState`。
5. **添加/重授权弹窗**（[`addDialog.tsx`](../../../gui/src/pages/accounts/addDialog.tsx) + [`state.ts`](../../../gui/src/pages/accounts/state.ts)）：根据 `supportsOauth` 自动选择默认 Tab（`true` → `OAuth 授权`，`false` → `本地导入`）并在 `OAuth` Tab 展示不可用说明与引导，在 `本地导入` Tab 展示后端下发的 `localSource` 路径提示。

### 四、必补测试与验证门禁

1. **新 Agent 必补两类离线单测**：
   - 在 `crates/ss-accounts/src/usage/fetch/<snake>.rs`（或 `<snake>_parse.rs`）补充纯函数 fixture 测试，覆盖正常额度、缺字段回退、401 `AuthRequired`、429/5xx `Transient` 与畸形响应。
   - 新建 `crates/ss-accounts/src/switch/<snake>_tests.rs`，完全使用 `TempDir` + `PathRoots`（禁止读写宿主真实 `$HOME` 或系统钥匙串）验证 `read`、`write` 回读校验、`snapshot` 与失败回滚 `restore`。
2. **自动生效的全仓一致性守卫（无需另写配置）**：
   - `specs_match_supported_agents_order_and_uniqueness`（`crates/ss-accounts/src/agents.rs`）：校验 `Agent::supported()`、`SUPPORTED_IDS`、`from_id`、`display_name`、`local_source` 完整唯一。
   - `every_account_agent_has_dedicated_icon_and_skills_profile` 与 `agents_list_the_supported_rows`（`gui/native/src/facades/accounts.rs`）：强制校验每个受管 Agent 均拥有非 `_fallback.svg` 的专属图标、在 `ss-skills` 注册表存在有效 `AgentProfile`，且桥接正确透传四项能力字段。
   - `all_embedded_svgs_decode_as_valid_data_uris`（`gui/native/src/agent_icons.rs`）：校验新 SVG 在深色与浅色主题下均可正常生成 `data:image/svg+xml;base64,…`。
3. **验证命令**：

```bash
cargo test -p ss-accounts --locked
cargo test -p ss-gui-bridge --locked
nsh check gui
nsh test gui
```

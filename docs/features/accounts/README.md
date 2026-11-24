# Accounts（账号工作台）

状态：active

账号工作台按 agent 管理多份登录凭据，并把选中的那份写进本机客户端，让 CLI/IDE 真的换成它。
界面只存在于 nsh 应用（`gui/`）。产品二进制直接启动 nsh 宿主，账号入口是侧栏顶部的模式胶囊「技能 / 账号」，切到账号后左栏列出 `ss_accounts::Agent::supported()` 中的 agent。

## 范围

- 受管清单与侧栏顺序以 [`crates/ss-accounts/src/lib.rs`](../../../crates/ss-accounts/src/lib.rs) 的 `Agent::supported()` 及 [`crates/ss-accounts/src/agents.rs`](../../../crates/ss-accounts/src/agents.rs) 的 `spec()` 为唯一代码真相；各家认证腿与落点见下「每家认证腿与落点」。
- 登录令牌（`oauth`）、额度抓取（`usage`）、账号库与切号写回（`store` / `switch` / `service`）统一内聚在 `crates/ss-accounts`。
  UI（经 `ss-gui-bridge`）只面对 `ss-accounts` 的顶层 facade 与 `AccountView`，凭据不出 crate。
- Gemini CLI 的账号管理（登录、额度、切号写回、本机导入）已移除（[D-119](../../decisions.md#d-119移除-gemini-cli-账号支持)）；其存量账号记录在下次读取列表时从磁盘删除（见下「存储与安全」），技能分发与 Antigravity 不受影响（Gemini CLI 的 MCP 支持亦已按 [D-122](../../decisions.md#d-122移除-gemini-cli-mcp-支持) 移除）。

## 页面

主导航固定在应用左侧栏（`Sidebar.tsx` 的 `AccountsProviderMenu`），列账号域受管客户端（复用 `src/chrome/agentRail.tsx` 的 `AgentIcon` 品牌 SVG 图标 + 名称 + 该 agent 的账号数；与技能导航一致，选中行采用 `panel_active` 底色 + `1px accent` 边框 + `accent_fg` 文字，未选中无底色，由 `agent_icons.rs` 在浅深主题下统一处理单色/彩色 SVG 对比度）。账号页主面板顶部**不设独立页头栏，也不在页头或正文重复渲染切换胶囊或纵向窄栏**，直接以悬浮圆角工具栏起始；工具栏复用 `PageBar` 导出的同一组几何常量（`TOOLBAR_H` 带、顶端 10px、46px 卡片、14 圆角、12px 内边距），与其余五个页面同高同位。页面本身不画自己的底色，直接坐落在壳面板的 `panel` 表面上——和技能页同一底色，两侧留白不再比技能页深一档。卡片区仍受 `CARD_ROW_W`（三列 `SKILL_CARD_W`）宽度约束。

主区由上至下依次为：

1. **悬浮圆角工具栏**：每个控件都用 [界面约定](../frontend/README.md)的工具栏尺度（`chrome/toolbar.tsx` 的 `BAR_*`），页面不再自绘按钮：左侧含实时搜索框（`BarSearchInput`，匹配名称、邮箱、ID 与套餐）、三态视图切换分段（`BarSegmented`：紧凑 `compact` / 列表 `list` / 卡片网格 `grid`，当前模式用 `card` 底描边）、套餐与异常筛选下拉（`BarMenuTrigger` + `BarMenuPanel`：全部、各套餐档位、`需重新授权`、`额度异常`）以及排序菜单（默认当前优先 / 按综合配额 / 按最近使用 / 按添加时间）配升降序图标按钮；右侧含主色 `+` 添加账号按钮（该条唯一的主色动作）、全部刷新按钮（仅正在刷新的卡旋钮）、自动刷新周期菜单（见下「自动刷新（错峰轮询）」）与隐私打码开关（开启时走 `accent_soft` 激活态，对邮箱与显示名称脱敏，不影响搜索匹配）。
2. **全选批量操作条**（当前 agent 存在账号时显示）：左侧全选复选框、已选计数与取消选择；居中显示「全部 N」计数胶囊与快捷 `+` 添加按钮；选中任意账号后右侧显现「刷新选中」与「删除选中」（走共享确认框）。
3. **可关闭错误横幅**：动作出错时出现在列表上方，可手动关闭；单卡刷新的失败在横幅右侧给「重试」。成功一律不在正文里插提示：切换（「已切换至 <账号>」，开启隐私模式时同步脱敏）、添加账号、两种导入、删除与批量删除、刷新与批量刷新都只发页面顶部那条短 toast（约 3 秒自动消失），列表不被顶下去、布局不跳动。添加/导入的失败留在对话框内的行内文案上。
4. **多视图内容区**：首次加载且尚无数据时，按[界面约定的等待](../frontend/README.md#等待)展示与卡片网格同几何的 `AccountsSkeletonGrid`；已有卡片时刷新保留卡片。有数据时，网格（`AccountGridView`）、紧凑（`AccountCompactView`）与列表（`AccountListView`）视图均走 `<List>` 行虚拟化 + 按 index/id 复用的 `<Each>`，仅挂载视口附近行并用行级 `computed` 与 `ReadonlySet` 收窄刷新/勾选扇出；页面处于切走隐藏态（`currentPage() !== "accounts"`）时，`AccountsWorkbench` 通过 `pageActive()` 门控跳过后台账号与刷新队列推送，仅在重新切回工作台时恢复同步。无账号时居中展示空态引导与「添加账号」按钮（已有账号后不在网格末尾常驻虚线占位卡）；筛选无结果时展示无匹配提示与「清除筛选」按钮。
   空态的虚线面板**占满工具栏以下的整个窗口**：外层 `<Scroll>` 的 `contentStyle` 带 `flexGrow: 1`，页内容列与空态根节点随之 `flexGrow: 1`，`minHeight: 240` 只作极矮窗口的下限；引导图标、标题、说明与「添加账号」按钮始终在该面板内垂直居中，面板下方不再留空白带。

添加对话框显示顶部 Tab（`OAuth 授权` / `Token / JSON` / `本地导入`；当客户端元数据 `supportsOauth` 为 `false` 时禁用「OAuth 授权」入口并默认切到「本地导入」）。OAuth 段按后端 `Agents` 下发的 `LoginFlow` 与可选 `oauthModes` 配置渲染（[`addDialogControls.tsx`](../../../gui/src/pages/accounts/addDialogControls.tsx)）：

| flow | 界面 |
| --- | --- |
| `LocalCallback` | 授权链接展示 + 「复制链接」+ 「在浏览器打开」+ 可粘贴回调链接或授权码的输入行 |
| `RemotePoll` | 设备码（支持复制）+ 验证地址 + 轮询等待（没有粘贴框）；支持 `oauthModes` 切换（如 Kiro 的 `portal` / `idc`，选 `idc` 时展示 `idcRegion` 与 `idcStartUrl` 输入项） |
| `SchemePaste` | 提示粘贴 scheme 协议回调 URL；支持 `oauthModes` 切换（如 ZCode 的 `zai` / `bigmodel` 端点模式） |
| `Immediate` | 无用户操作，立即完成（`supportsOauth = false` 的本地客户端走这条，如 Claude Code、Devin Desktop；页内默认落到「本地导入」Tab 并提示改用「本地导入」或「Token / JSON」） |

等待态显示旋转指示与「取消」。

## 卡片

复刻 cockpit-tools 的账号卡解剖（D-109 及其后续修订），数字固定，颜色一律走 `paletteToken()`，兼容深色/浅色双主题：

- 卡根（网格视图）：纵向 flex、gap 10、padding 12 14、圆角 16、1px 描边、卡面底色、`min-height` 228、宽固定 `SKILL_CARD_W`。
  hover 只换底色与描边色——本仓库的状态切换零过渡契约继续有效，不做位移、半透明模糊与阴影过渡。
- 身份行（`card-top`，横 flex，gap 8）：勾选复选框（18×18 热区）→ 账号名 14/600 单行省略（`flex:1`，受隐私开关脱敏）→ 当前生效胶囊（`ok` 底白字 11/600、padding 3 10、圆角 999）→ 错误胶囊（额度读取失败时 `danger` 软底、`danger` 图标/短标 + Tooltip 详述）→ 套餐徽标 `margin-left:auto` 靠右（11/600 大写、letter-spacing .05em、padding 4 10、圆角 999）。
  套餐配色按**等级**上色，不分厂商，同一等级在各家账号上同色：

  | 档位 | 认的词根（示例） | 配色 |
  | --- | --- | --- |
  | 顶档 | `ultra` `max` `heavy`（Cursor/Antigravity Ultra、Claude Max、GLM Coding Max、SuperGrok Heavy） | 琥珀金 `amber_bg`/`amber_fg`/`amber_border` |
  | 次顶 | 顶档带 `lite` 或 `5x`（Ultra 5X、Max 5x）、pro 带加号（Pro+、PRO_PLUS） | 冷蓝 `info_bg`/`info`/`info_border` |
  | pro | `pro` `professional` `supergrok` | 主色实底白字 `accent`/`on_accent` |
  | pro 降配 | `prolite` `pro lite` / pro 带 `5x`（Codex Pro Lite、legacy `pro 5x`） | 同色系软底 `accent_soft`/`accent_fg`/`accent_soft_edge` |
  | plus | `plus` `premium`（含 `Premium+`） | 绿 `ok_bg`/`ok`/`ok_border` |
  | 入门付费 | `lite` `basic` `starter` `standard` `go`（GLM Coding Lite、standard-tier、ChatGPT Go） | 青 `cyan_bg`/`cyan`/`cyan_border` |
  | 团队 | `team` `business` | 紫 `violet`/`violet_fg` |
  | 企业 | `enterprise` | 暖黄 `warn_bg`/`warn`/`warn_border` |
  | 免费/未知 | `free` 与认不出的套餐名 | 中性灰 `well`/`fg_muted` |

  词根表在 [logic.ts](../../../gui/src/pages/accounts/logic.ts) 的 `planKind`（按顶档到免费的顺序认词，先命中先定档；切词而非子串匹配，`elite` 不会被当成 `lite`），配色在 [card.tsx](../../../gui/src/pages/accounts/card.tsx) 的 `planColors`；一处一档一色由单测钉住。筛选下拉的 `ULTRA` 同时覆盖 20X 与 5X，`LITE` 只在有账号时出现。

  **能分到哪一层看上游给什么**：分档只吃套餐字符串。Codex 的 `plan_type`（wham/usage）与 JWT 的 `chatgpt_plan_type` 是同一个值，只给档位 slug（`plus` / `pro` / `prolite` / `team` / `business_*` / `go` / `free` …），**不含倍率**：Pro 的 20x 与 50x 若都报 `pro`，徽标只能是同一个 `PRO`；只有 slug 真的不同（`prolite` 即 Pro Lite / 5x）才分得出色。Antigravity 相反，代号带 `LITE`，所以 20X 与 5X 能分。
- 所有视图的身份区统一以用户名为主标题、邮箱为副标题，并受隐私开关脱敏；没有用户名或邮箱时分别显示「暂无用户名」或「暂无邮箱」，不把客户端名或账号 ID 冒充身份。认证失效时在邮箱下另显示 `danger` 文案「登录已失效，需重新授权」及「重新授权」按钮。
- 导入或刷新时尽力从凭据的用户资料中读取用户名和邮箱，保留用户自定义名称；仅有邮箱时不把邮箱当成用户名。
- Grok 从本机 CLI 凭据的 `first_name` / `last_name` 读取姓名；与当前 CLI 登录匹配的旧账号可立即显示，并在刷新时保存。单独存在且等于邮箱前缀的 `first_name` 不作为用户名。
- Antigravity 导入或刷新时，缺少账号身份则尽力读取 Google 用户名和邮箱，保留用户自定义名称。套餐徽标按表把 tier 代号换成用户认得的那一层：`G1-ULTRA-TIER` → `ULTRA 20X`（琥珀）、`G1-ULTRA-LITE-TIER` → `ULTRA 5X`（冷蓝），两档不同色；表里没有的代号去掉世代前缀 `G1` 与后缀 `TIER`、分隔符换成空格再大写（`standard-tier` → `STANDARD`），已是人话的套餐名（如 `PRO`）与其他 Agent 的套餐名原样显示。归一在 `planLabel`（[logic.ts](../../../gui/src/pages/accounts/logic.ts)），徽标文案对所有 Antigravity 账号说同一套话。
- 额度内框（`well` 软底圆角 10 容器，padding 10、1px `border_soft` 描边、纵向 gap 8）：当窗口数 >= 2 且相邻两项顶行文案紧凑时按**双列并排**布局（如 `5h` 与 `Weekly` 并列一行），长标签或带用量 chip 超宽的窗口自动占满单列，避免模型组长名截断丢掉周期后缀。每个窗口三层——顶行（label 11/600 次要色 + 可选 chip 10/600 + 右对齐剩余百分比 11/700 tabular-nums，按**剩余**率分段：>=80 `ok`、>=40 `warn`、>=10 橙 `amber`、<10 `danger`）、中行（轨道高 4px、圆角 999）、底行（10px 弱化色紧凑倒计时，如 `4h 54m`，已重置显示「已重置」；额度数据行不挂 Tooltip，避免悬停气泡盖住卡片读数）。顺序固定：`five_hour` -> `week` -> 其它窗口 -> 内框底部分隔后的 Credits 行（credits 无进度条，只显示数值或「无限」）-> 可选重置卡行（`resetCredits` 非空时显示剩余重置卡张数，如 Codex 的 `2 张` / `0 张`）。无可用窗口、点数与重置卡数据时，若 `usage.error` 非空则在槽内直接展示最多 2 行可读错误原因（完整信息挂在顶部错误角标 Tooltip），否则居中写 12px「暂无额度数据」。
- 底部操作行：`margin-top:auto`，上方 1px 分隔线（`padding-top` 10），左侧展示最近使用/创建时间（`lastUsedAt ?? createdAt`），右侧排布 28×28 图标按钮（圆角 8、图标 14）：切换（非当前用 `play` 绿色图标；当前生效展示禁用 `checkCircle`）、单卡刷新（仅刷新中的卡旋钮）、删除。
- 列表视图与紧凑视图：列表视图（`AccountListView`）固定表头下挂 `<List>` 行，展示复选框、身份与徽标、横向额度条、最近使用时间与操作按钮；紧凑视图（`AccountCompactView`）沿用按窗口自适应列数的 `<List>` 网格，展示勾选、脱敏身份、彩色圆点百分比胶囊与快速操作按钮。
- 卡内只放 身份 / 套餐 / 额度 / 时间 / 操作。cockpit 卡内的代理、API 服务、速度档、会话、备注/2FA 等子系统不复刻。

「当前生效」是磁盘真相，不是缓存 pin：list 渲染时读 `ss_accounts::reconcile()` 的客户端实况（D-109）。

## 全局额度下拉窗（状态栏 / 托盘）

为支持主窗口隐藏或处于「技能 / 市场 / 卡组 / 项目 / MCP」任意页面时快速查看配额与切号，[`gui/src/pages/accounts/quotaPopover.tsx`](../../../gui/src/pages/accounts/quotaPopover.tsx) 提供 `340pt` 宽的三段式紧凑面板 `QuotaPopoverPanel`，只挂在系统状态栏 / 托盘的弹出窗（[`gui/src/shell/residentWindow.ts`](../../../gui/src/shell/residentWindow.ts) + [`trayQuotaWindow.tsx`](../../../gui/src/shell/trayQuotaWindow.tsx)），与账号工作台共享同一份 `sharedAccountsState` 单例（零重复轮询、零跨进程状态同步）。侧栏底栏不设额度按钮：入口只有这个弹出窗与原生菜单的额度速览项（`⇧⌘U` / `Ctrl+Shift+U`）。

- **系统状态栏 / 托盘弹出窗**：`tray_quota_enabled` 开启时，macOS 菜单栏挂载无底板单色系统模板图标（`assets/tray-icon.svg`，仅显示图标、不显示百分比数值），`Tray.set` 仅订阅布尔 `hasAccounts`（是否存在账号）而非整份 `accounts()` 快照；应用启动后装载本地快照、对账在役状态并静默预热 `<Window id="quota-popover" kind="popup">`（常驻保活，失焦或按 `Escape` 时仅 `hide()` 隐藏而不销毁重建），弹出窗通过 `active={trayPopoverOpen()}` 门控在隐藏期间冻结后台信号订阅与 `IconSpin` 动画；点击菜单栏/托盘图标先按屏幕边界夹紧坐标零延迟 `setBounds` + 呈现面板，再异步触发带 15 秒冷却节流的在役对账与过期账号静默补刷。

面板交互规则：
- **仅展示已配置或在役 Agent**：按 `Agent::supported()` 顺序筛选至少存有 1 个账号或 `reconcile()` 探测到在役身份的客户端，未配置客户端不渲染空行；全库无账号时展示精简空态与「打开工作台添加」入口。
- **手风琴单开备选切号**：每个 Agent 默认展示当前在役账号（或首张可用卡）的脱敏身份、套餐徽章与紧凑窗口剩余进度条（卡片头行在品牌图标右侧直接展示当前账号用户名/脱敏身份，不显示 Agent 名称；进度条遵循 `five_hour` → `week` → 其它 → `Credits` → `重置卡` 顺序，认证失效时原位展示「重新授权」入口）；拥有多个账号的 Agent 点击卡片展开按综合剩余配额降序排列的备选行，点右侧绿色 `play` 按钮即调 `activate` 写回客户端并回读校验。
- **顶栏、错误条与底栏联动**：顶栏含异常计数胶囊、与账号页实时同步的隐私脱敏开关（`privacyMode`）、全库一键刷新与跳转工作台按钮；刷新或切号动作出错时在内容区上方渲染可手动关闭的紧凑错误条（`PopoverErrorStrip`，[`quotaPopover.tsx`](../../../gui/src/pages/accounts/quotaPopover.tsx)，支持 `dismissError()` 与可选单账号 `retryId` 重试）；底栏左侧可点击一键开启或切换错峰自动刷新周期（1 / 5 / 15 分钟 / 关），右侧提供「`+ 添加`」与「工作台」快捷唤起动作。
- **只展开菜单，不激活应用**：macOS 上状态栏唤起时面板按 `presentTrayQuotaPopover()` 呈现——应用已在最前才 `focus()`；应用不在最前时只 `show()`，让面板浮在最前而不抢键盘，主窗口不会随之上浮。面板展开（`trayPopoverOpen()` 为 `true`）期间由 [`gui/native/src/popover_dismiss.rs`](../../../gui/native/src/popover_dismiss.rs) 挂载 macOS 全局与本地 `NSEvent` 鼠标按下监听，点击面板之外的任意位置（其它 App、桌面、Dock、其它菜单栏区域或 SkillStar 主窗口）立即收起面板，同时忽略面板自身、原生右键菜单以及主按钮点击 SkillStar 自身状态栏图标（交由其 `LeftMouseUp` 统一切换开关避免按下收起后松开重开）；用户回到 SkillStar（点主窗口 / Dock 图标）时 `installDesktopFeatures` 的 lifecycle 也会兜底收起。Windows 平台 `presentTrayQuotaPopover()` 直接走 `focus()` 并依赖原生失焦收起。
- **内容区始终可滚**：卡片列表一律挂在 `<Scroll>` 里，视口框写明确上限 `QUOTA_POPOVER_BODY_MAX_H`（392pt），面板上限由常量推导 `QUOTA_POPOVER_MAX_H`（40 顶栏 + 392 + 34 底栏 + 2 描边 = 468pt）。组数少时视口收缩到内容高度、弹出窗不留空白；组数多时滚轮、触控板与拖动都在列表内翻阅。**不要按组数阈值决定挂不挂滚动容器**——组数少但单卡窗口多的列表会被面板 `overflow: hidden` 裁掉且无处可滚（[errors](../../errors.md)）。

## 自动刷新（错峰轮询）

工具栏右侧的「自动」菜单按**单个可用账号的刷新周期** T 工作（关 / 30 秒 / 1 / 2 / 5 / 15 分钟，未配置时默认开启 **5 分钟**档），对全库所有受管 Agent 的可用账号统一调度（在役账号优先），实际步进按当前全局可用账号数 N 动态切分：

- 步进 `Δt = max(T / N, 2s) × (1 ± 15%)`，抖动让多实例不会在同一秒一起打接口（分母 `N` 按当前未冷却、未禁用的可用账号数计算，不扣除当次在途账号）；每次只挑**一个**账号发请求，选的是「最久未尝试或刷新」的那个（从未尝试且未刷新过的排最前，同批次内在役主账号优先，其次按 `max(lastAttemptedAt, lastRefreshedAt)`，最后回退到快照的 `fetchedAt`；失败尝试同样后移至队尾，防止单个异常账号霸占队首饿死其余账号）。
- 与手动刷新及展开补刷共享 `refreshingIds` 单飞锁：在途账号这一轮跳过，不重复请求。页面正文正在删除、或批量动作进行时整轮让路。
- 失败保留旧配额并局部冷却，不把卡片清空：`429`（限流）按 60s 起指数退避、封顶 10 分钟；其它瞬时错误按 15s 起指数退避、封顶 10 分钟；当所有可用账号均在冷却中时，调度器定时在最早冷却到期时刻自动唤醒恢复、不永久停摆；仅 `401` / `AuthRequired`（需要重新授权）直接把该账号钉住，直到它下一次刷新成功（例如重新授权后手动刷新一次）才解除。`403` / `forbidden` 属于 `Unreadable` 读取失败，**不得**置 `requiresReauth`。错误匹配见 `classifyRefreshError`（[autoRefresh.ts](../../../gui/src/pages/accounts/autoRefresh.ts)）。
- 菜单里的预览文案「N 个可用 · 约每 M 秒轮询 1 个」用的就是上面这两个数（`autoRefreshEligibleCount` / `autoRefreshStepSeconds`）。

周期跨重启保存在 `~/.skillstar/config/gui_prefs.json` 的 `accounts_auto_refresh_ms`（与设置页的语言、背景同一份文件；缺省键默认 `300000` 即 5 分钟，显式关闭写 `null` 以跨重启保持关闭状态）。读写走 `Settings` facade 的 `load_accounts_auto_refresh` / `save_accounts_auto_refresh`（[settings.rs](../../../gui/native/src/facades/settings.rs)），页面侧接缝是 [prefs.ts](../../../gui/src/pages/accounts/prefs.ts)。两个页面写同一份文件，所以两边的写入都是「先读整张表、只替换自己的键」。

窗口隐藏不暂停轮询：nsh 的 JS 环境没有 DOM，且关闭主窗口只隐藏窗口、进程与后台唤醒继续（[D-115](../../decisions.md)）——自动刷新属于该继续跑的工作。

## 添加与登录

- **OAuth**：`start_login` 返回 `LoginStart`（flow / auth_url / user_code / 验证地址 / 间隔 / 过期），
  页面按 flow 渲染；`submit_login_callback` 投递粘贴的回调；`finish_login` 阻塞拿到结果并落库。
  同一 agent + 同一身份的旧行**原地替换**，不产生重复卡。
- **Antigravity OAuth**：默认使用 [Cockpit Tools 公开的桌面客户端配置](https://github.com/jlcodes99/cockpit-tools/blob/main/src-tauri/src/modules/oauth.rs)登录和刷新；需要时可通过 `SKILLSTAR_ANTIGRAVITY_CLIENT_ID` / `SKILLSTAR_ANTIGRAVITY_CLIENT_SECRET` 或 `config/antigravity_oauth.json` 覆盖。自定义客户端 ID 需要提供对应的 secret。
- **Claude Code 没有浏览器登录腿**：`start_login` 返回 `Immediate`，`finish_login` 直接以 `AuthRequired` 拒绝并提示
  「请用从本机导入或粘贴令牌」。这是如实能力，不是缺陷：Claude Code 的凭据只从本机存储或粘贴令牌进入账号库。
- **从本机导入**：读客户端当前凭据组装账号行。Codex 读 `~/.codex/auth.json`（尊重 `CODEX_HOME` 与
  `SKILLSTAR_TOOL_SYNC_HOME` 覆盖）。
- **粘贴令牌**：接受裸 access token，或含 `access_token` / `refresh_token`（或 Cursor 混合存储的驼峰
  `accessToken` / `refreshToken`）的 JSON。落库前尽力核对一次额度：`AuthRequired` 拒绝落库，
  `Transient` 允许落库但额度留空，`Unreadable`（如上游配额通道停服或缺 GCP 项目）允许落库并在 `usage.error` 记录诊断原因。

## 切号写回

切号 = 将账号库中选定的登录身份写回客户端并读回校验。写回失败不移动 `last_used_at`、不改索引，并恢复旧状态。

| agent | v1 落点 | 说明 |
| --- | --- | --- |
| codex | `<codex home>/auth.json` | 写 `OPENAI_API_KEY: null` + `tokens{id_token,access_token,refresh_token,account_id}` + `last_refresh`；v1 **不写钥匙串**，macOS 上 Codex 可能优先读 `Codex Auth` 钥匙串条目，如实标注 |
| claude-code | `<claude config dir>/.credentials.json` | 只读改写 `claudeAiOauth` 段（兄弟键如 `mcpOAuth` 原样保留），`expiresAt` 是**毫秒**；macOS 钥匙串 `Claude Code-credentials` 不桥接 |
| cursor | `<Cursor userData>/User/globalStorage/state.vscdb` | `ItemTable` 单事务 upsert `cursorAuth/accessToken`、`cursorAuth/refreshToken`、`cursorAuth/cachedEmail`、`cursorAuth/authId`，并镜像 `cursor.accessToken` 与 `cursor.email`；写完回读校验（禁止写入 `cursor-safe-storage:v1:` 密文） |
| antigravity | macOS 钥匙串 / Windows Credential Manager `gemini` / `antigravity`（权威）+ `~/.gemini/jetski-standalone-oauth-token`（+ `antigravity-cli/` 存在时同目录 token 文件）+ `<userData>/User/globalStorage/state.vscdb`（**旧版 IDE 才有**） | 2.x 桌面端只读系统凭据、不再创建 `state.vscdb`，因此投影按「文件在场」决定写不写：不在场既不是失败，也不由我们建库；系统凭据值带 `go-keyring-base64:` 前缀（Windows 经 [`antigravity_wincred.rs`](../../../crates/ss-accounts/src/switch/antigravity_wincred.rs) 调用 `advapi32` 读写 `gemini:antigravity`），并按客户端原载荷合并（保留 `writer_id` / `token.expires_in` 等客户端自有字段，`id_token_email` 指到这次写入的账号）；vscdb 在场时另写键 `antigravityUnifiedStateSync.oauthToken`（base64+protobuf，sentinel `oauthTokenInfoSentinelKey`）；写完回读同一 refresh_token 才成功（vscdb 不在场时以系统凭据回读为准），任一步失败按快照回滚（D-114） |
| grok | `~/.grok/auth.json`（`GROK_HOME` 可覆盖）的 scope 条目 | xAI 设备码（`auth.x.ai`，404 时退回环 56121）；**不写钥匙串** |
| kiro | `~/.aws/sso/cache/kiro-auth-token.json` + `<sha1(startUrl)>.json` + Kiro `profile.json` | 门户 PKCE 或 AWS IdC 设备码（注册信息进 `provider_state`）；**不写钥匙串** |
| zcode | `~/.zcode/v2/credentials.json`（`enc:v1:` 自加密）+ `setting.json` 家族模式 | `zcode://` 深链粘贴（zai / bigmodel），换票 `zcode.z.ai/api/v1/oauth/token`；**不写钥匙串** |
| devin-desktop | `state.vscdb`（`Devin`，回退 `Windsurf`）三行 | **没有浏览器登录腿**：只能从本机导入或粘贴 apiKey |

### 切号不碰客户端进程

切号只写凭据，**不探测、不退出、不拉起客户端**：重启客户端的时机由用户自己决定（[D-121](../../decisions.md)）。目标客户端属于用户的桌面会话，不由本应用代管。

- Antigravity 桌面端只在**启动时**读凭据（会话在常驻 sidecar `language_server` 的内存里）：外部改写系统凭据（macOS 钥匙串或 Windows WinCred）它不会热加载，运行中还会把内存里的旧会话写回系统凭据。所以 macOS / Windows 上切号后账号页只报「已切换至 <账号名>」——客户端里显示的还是旧账号，直到用户自己重启它；
- `ActivateOutcome` 只带 `account` / `previous` / `target`：切号结论由磁盘回读决定，与客户端进程是否在跑、跑的是哪个实例无关；
- 需要重启时用客户端自己的退出（macOS ⌘Q、Windows 关窗口），**不要**用 `kill -TERM` / `kill -KILL` 结束 Antigravity：被强杀之后 `open -a` 会拉不起来（[errors.md](../../errors.md)）。

## 存储与安全

- 路径：`~/.skillstar/secrets/accounts/accounts.json`（索引，明文元数据）+
  `~/.skillstar/secrets/accounts/records/<id>.json`（整条记录，令牌字段 AES-256-GCM 密文），
  文件权限 0600，写锁 `~/.skillstar/runtime/locks/accounts/storage.lock`。
- 加密：key = SHA-256(machine uid)，12 字节随机 nonce，存 `base64(nonce || ciphertext)`（沿用旧实现的方案，
  换成别的派生会让已有记录全部解不开）。
- 明文令牌不出 crate：`AccountView` 与任何 Debug 或序列化输出都不得含令牌。
- 路径语义全部走 `ss_core::infra::paths`，`SKILLSTAR_DATA_DIR` 覆盖继续生效。
- 已下线客户端的账号记录不留存：每次读取列表（`ss_accounts::list()`）先在存储锁内删掉这些记录文件并重建索引，旧令牌不再留在 `secrets/accounts/` 里；客户端自己的文件不属于 SkillStar，保持原样（不会因此删客户端数据）。

## 验证

- `cargo test -p ss-accounts`（登录、额度抓取、加密存储、切号与错误分类）。
- `nsh check gui` 与 `nsh test gui`（页面与桥；假桥注入 AccountView fixture）。
- 客户端的真实写回尚未全面验证。Antigravity 2.x 的写回已在本机按「同一账号再切一次」验证过：钥匙串写-回读、合并后保留 `writer_id`、两份 `~/.gemini` 文件都成功；切成另一个账号也已在真机切通（`target = gemini:antigravity (macOS Keychain)`、`lastUsedAt` 与钥匙串 `mdat` 对得上）。
- 切号不碰客户端进程（D-121）：真机上切号只更新钥匙串与 `~/.gemini` 文件，`pgrep -x Antigravity` 的进程号在切号前后不变。
- 仍未确认：运行中的客户端在用户重启它之前会不会把内存里的旧会话写回系统凭据（它自己刷新令牌时就会写回）——这正是切号后必须由用户手动重启客户端才生效的原因；Windows 平台 Antigravity 2.x 的 WinCred（`gemini:antigravity`，`advapi32`）读写、载荷合并、回读校验与失败回滚已通过跨平台 `WinCredStore` 抽象与单元测试落地，实际桌面重启联动待真机端完整验证。

## 未覆盖（如实登记）

- 多开实例、主动消耗重置卡、API-key 家族、GitHub Copilot、ChatGPT 登录：本版不做（D-108；Codex 仅只读展示重置卡数量）。
- 其余 7 家客户端的系统钥匙串写回：不做，见上表与下述口径。
- cockpit 的拖拽排序、账号备注/2FA、标签/分组持久化、明文凭据导出：不复刻。
- **翻译独立于账号工作台**（刻意边界）：设置页「翻译」已为大模型翻译提供独立的端点（`llm_base_url`）、模型 ID 与手动 API Key（落盘至 `secrets/translation/api_key`）配置入口，不与 Accounts 工作台的订阅凭据耦合或互通。

## 每家认证腿与落点

| agent | 登录腿 | 额度来源 | 导入 | 对账判据 |
| --- | --- | --- | --- | --- |
| codex | OpenAI 回环（可粘贴） | `chatgpt.com/backend-api/wham/usage` | 本机 `~/.codex/auth.json` / 粘贴令牌 | account id，其次指纹 |
| claude-code | 无（只能导入/粘贴） | `api.anthropic.com/api/oauth/usage` | `~/.claude/.credentials.json` | 令牌指纹 |
| cursor | 远程轮询 | `cursor.com/api/usage-summary` | `state.vscdb`（`cursorAuth/accessToken` 为 `cursor-safe-storage:v1:` 密文时回退明文镜像 `cursor.accessToken`，并忽略加密的 `cursorAuth/refreshToken`） | 指纹，其次 email |
| antigravity | Google 回环（localhost:51121） | Cloud Code `loadCodeAssist` 系 | Keychain（只读优先）/ vscdb | email，其次指纹 |
| grok | xAI 设备码（退回环 56121） | `cli-chat-proxy.grok.com` 的 `/v1/billing?format=credits` 与 `/v1/settings` | `~/.grok/auth.json` | user_id，其次 email |
| kiro | 门户 PKCE 或 AWS IdC 设备码 | `q.{region}.amazonaws.com/getUsageLimits` | AWS SSO 缓存 token 文件 | userId / email，其次指纹 |
| zcode | `zcode://` 深链粘贴 | `billing/balance`（JWT + `X-Device-Mid`）+ 上游 `quota/limit` | `~/.zcode/v2/credentials.json` | `customerNumber` / `userId` / `id`，其次 JWT `customer_id` / `sub`；显示名优先读取 `customerName` / `displayName` / `username` / `name` |
| devin-desktop | **无**（只导入/粘贴） | `SeatManagementService/GetUserStatus` | `state.vscdb` | email |

keychain 口径：只有 antigravity 写系统钥匙串（D-114，客户端只认钥匙串）；其余七家一律不写，读得到就读。


## 新增受管 Agent 统一配方（Recipe）

新增一个支持账号管理的 Agent（设注册 ID 为 `<id>`，Rust 模块蛇形命名为 `<snake>`）时，按下面清单改造。前端工作台、图标着色、空态与添加弹窗均由后端 `Accounts.agents()` 下发的能力元数据自动装配，**不要在 UI 或 Facade 层新增按 `<id>` 命名的分支或品牌图标表**。

### 一、下次新增改哪些文件（按执行顺序）

| 顺序 | 文件路径 | 目标符号 / 产物 | 具体改动 |
| --- | --- | --- | --- |
| 1 | [`crates/ss-skills/src/agents/builtin.rs`](../../../crates/ss-skills/src/agents/builtin.rs) | `BUILTIN_AGENT_DEFS` | 若该客户端也支持技能同步，按 [`agents/README.md`](../agents/README.md) 注册对应 profile；仅管理账号的客户端不注册技能目标 |
| 2 | `gui/assets/agents/<id>.svg` | 品牌 SVG 文件 | 放入紧凑居中的矢量图标（规范见下「图标规范」） |
| 3 | [`gui/native/src/agent_icons.rs`](../../../gui/native/src/agent_icons.rs) | `agent_icon_svg` / `normalize_agent_icon_id` | 在 `agent_icon_svg` 的 `match` 增加 `"<id>" => include_bytes!("../../assets/agents/<id>.svg")`；仅当 SVG 文件名与技能 profile ID 不同时才在 `normalize_agent_icon_id` 增加别名映射（如 `"claude-code"` 共用 `"claude"`） |
| 4 | [`crates/ss-accounts/src/lib.rs`](../../../crates/ss-accounts/src/lib.rs) | `pub enum Agent` / `Agent::supported()` | 新增枚举变体（如 `MyAgent`），并在 `Agent::supported()` 返回切片中的目标位置插入 `Self::MyAgent`（该切片决定侧栏展示顺序与 `reconcile` 遍历顺序） |
| 5 | `crates/ss-accounts/src/oauth/agents/<snake>.rs` + [`mod.rs`](../../../crates/ss-accounts/src/oauth/agents/mod.rs) | `start` / `refresh`（或复用 `immediate_auth_required`） | 新建登录与令牌刷新模块，并在 `oauth/agents/mod.rs` 声明 `pub(crate) mod <snake>;` |
| 6 | `crates/ss-accounts/src/usage/fetch/<snake>.rs` + [`mod.rs`](../../../crates/ss-accounts/src/usage/fetch/mod.rs) | `pub fn fetch(creds: &Credentials) -> UsageResult<UsageOutcome>` | 新建额度抓取与纯函数解析模块（响应复杂时拆 `<snake>_parse.rs`），并在 `usage/fetch/mod.rs` 声明 `pub mod <snake>;` |
| 7 | [`crates/ss-accounts/src/switch/paths.rs`](../../../crates/ss-accounts/src/switch/paths.rs) + `crates/ss-accounts/src/switch/<snake>.rs` + [`mod.rs`](../../../crates/ss-accounts/src/switch/mod.rs) | 路径函数 + `credential_path` / `write` / `read` / `snapshot` / `live_identity` | 在 `paths.rs` 新增基于 `&PathRoots` 的路径解析；新建 `<snake>.rs` 与 `<snake>_tests.rs` 实现本机读取、切号写回与回滚，并在 `switch/mod.rs` 声明 `pub mod <snake>;` |
| 8 | [`crates/ss-accounts/src/agents.rs`](../../../crates/ss-accounts/src/agents.rs) | `spec(agent: Agent) -> AgentSpec` / `SUPPORTED_IDS` | 在 `match agent` 的编译器穷尽分支补齐 `AgentSpec`，并同步更新同文件 `#[cfg(test)] pub(crate) const SUPPORTED_IDS` 长度与数组项 |
| 9 | 文档速查表 | 本页两表 + [`usage/README.md`](../usage/README.md) | 在本文件「切号写回」「每家认证腿与落点」及 [`usage/README.md`](../usage/README.md)「每家端点与解析」各补一行协议事实 |

### 二、常规接入复用层

- **域编排与存储层**：[`crates/ss-accounts/src/service.rs`](../../../crates/ss-accounts/src/service.rs)、[`reconcile.rs`](../../../crates/ss-accounts/src/reconcile.rs)、`store/`、`record.rs`、[`oauth/mod.rs`](../../../crates/ss-accounts/src/oauth/mod.rs)、[`usage/mod.rs`](../../../crates/ss-accounts/src/usage/mod.rs)、[`token_import.rs`](../../../crates/ss-accounts/src/token_import.rs) 全部通过 `Agent::supported()` 与 `agents::spec(agent)` 动态派发。
  - *仅在两种特殊协议下才需扩展共用原语*：① 客户端粘贴格式不是裸 token 也不是 JSON、而是类似 `go-keyring-base64:` 的专有外层封装时，在 `token_import.rs` 扩展解码入口；② 单次切号需跨多种全新存储介质做原子回滚时，在 `switch/mod.rs` 的 `ClientSnapshot` 增加枚举变体（单文件 JSON 用 `ClientSnapshot::File`，多文件用 `ClientSnapshot::Files`，VS Code 系数据库用 `ClientSnapshot::Items`）。
- **原生 GUI 桥接**：[`gui/native/src/facades/accounts.rs`](../../../gui/native/src/facades/accounts.rs) 的 `Accounts::agents()` 自动透传展示名、登录能力和本地来源。
- **前端账号页与文案**：[`gui/src/pages/accounts/`](../../../gui/src/pages/accounts/index.tsx) 默认复用通用组件；特殊登录方式以能力字段控制，勿在页面添加客户端 id 分支。

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
| `LoginFlow::LocalCallback` | `true` | `oauth::loopback`、`oauth::pkce`、`oauth::agents::code_from_query`；参考 [`codex.rs`](../../../crates/ss-accounts/src/oauth/agents/codex.rs) |
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
  2. **多文件客户端**：快照按写入顺序记录多个 `(PathBuf, Option<Vec<u8>>)` 到 `ClientSnapshot::Files`，由 `switch::restore` 自动逆序放回；参考 [`kiro.rs`](../../../crates/ss-accounts/src/switch/kiro.rs)、[`zcode.rs`](../../../crates/ss-accounts/src/switch/zcode.rs)。
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

接入后端表后，GUI 的五层骨架会按 `Accounts.agents()` 下发的 `AgentRow` 能力字段呈现与现有 Agent 一致的外观与交互：
1. **左侧品牌导航行**（[`rail.tsx`](../../../gui/src/pages/accounts/rail.tsx)）：`AccountsProviderMenu` 渲染 `<AgentIcon>` + `displayName` + 实时账号计数，选中态与 `SkillsNav` 统一为 `panel_active` 底色 + `1px accent` 边框 + `accent_fg` 文字。
2. **悬浮工具栏**（[`index.tsx`](../../../gui/src/pages/accounts/index.tsx)）：复用 `chrome/toolbar.tsx` 的工具栏尺度（`PAGE_BAR_*` 卡片几何 + `BAR_*` 控件尺度）与 `CARD_ROW_W` 宽度，提供搜索、三态视图切换、筛选与排序。
3. **三态数据视图**（[`views.tsx`](../../../gui/src/pages/accounts/views.tsx) + [`logic.ts`](../../../gui/src/pages/accounts/logic.ts)）：网格卡片（`AccountGridView`）、紧凑多列卡（`AccountCompactView`）与列表行（`AccountListView`），统一基于 `<List>` 虚拟化，自动复用 `planKind` 徽标配色、`remainingTone` 剩余百分比阈值着色与 `formatCreditValue` 额度格式化。
4. **加载骨架与空态**（[`states.tsx`](../../../gui/src/pages/accounts/states.tsx)）：首次且列表还空时，按[界面约定的等待](../frontend/README.md#等待)渲染与三列卡片同尺寸的 `AccountsSkeletonGrid`；已有卡片的刷新不换骨架。空列表时在全高虚线容器内居中显示 `<AgentIcon>`、`displayName` 与「添加账号」按钮；筛选无匹配时显示 `AccountsNoMatchState`。
5. **添加/重授权弹窗**（[`addDialog.tsx`](../../../gui/src/pages/accounts/addDialog.tsx) + [`state.ts`](../../../gui/src/pages/accounts/state.ts)）：根据 OAuth 能力选择默认 Tab，显示本地来源；本机导入自动识别账号。

### 四、必补测试与验证门禁

1. **新 Agent 必补两类离线单测**：
   - 在 `crates/ss-accounts/src/usage/fetch/<snake>.rs`（或 `<snake>_parse.rs`）补充纯函数 fixture 测试，覆盖正常额度、缺字段回退、401 `AuthRequired`、429/5xx `Transient` 与畸形响应。
   - 新建 `crates/ss-accounts/src/switch/<snake>_tests.rs`，完全使用 `TempDir` + `PathRoots`（禁止读写宿主真实 `$HOME` 或系统钥匙串）验证 `read`、`write` 回读校验、`snapshot` 与失败回滚 `restore`。
2. **自动生效的全仓一致性守卫（无需另写配置）**：
   - `specs_match_supported_agents_order_and_uniqueness`（`crates/ss-accounts/src/agents.rs`）：校验 `Agent::supported()`、`SUPPORTED_IDS`、`from_id`、`display_name`、`local_source` 完整唯一。
   - `every_account_agent_has_dedicated_icon_and_skills_profile` 与 `agents_list_the_supported_rows`（`gui/native/src/facades/accounts.rs`）：校验每个受管客户端都有品牌图标，技能同步客户端有对应 profile，且桥接正确透传能力字段。
   - `all_embedded_svgs_decode_as_valid_data_uris`（`gui/native/src/agent_icons.rs`）：校验新 SVG 在深色与浅色主题下均可正常生成 `data:image/svg+xml;base64,…`。
3. **验证命令**：

```bash
cargo test -p ss-accounts --locked
cargo test -p ss-gui-bridge --locked
nsh check gui
nsh test gui
```

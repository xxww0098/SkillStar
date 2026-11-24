# 界面约定

状态：active

本文件只记 SkillStar 桌面端（基于 `nsh` 的 `gui/` 应用）自身的视觉与交互契约；目录与三层边界见 [boundaries.md](../../boundaries.md#gui-壳nsh)。
各页产品行为留在相邻功能文档：[Skills](../skills/README.md)、[Marketplace](../marketplace/README.md)、[Accounts](../accounts/README.md)。

## 壳

- **应用位置与主题字典**：界面在 `gui/`，配色由 [`gui/theme.json`](../../../gui/theme.json) 与 [`gui/src/services/themeMode.ts`](../../../gui/src/services/themeMode.ts) 定义，文案在 [`gui/src/services/translation.ts`](../../../gui/src/services/translation.ts)（`en` 与 `zh-CN` 同步维护）。持久化主题读取 `gui_prefs.json` 的 `background_style`（`"current"` 深色 / `"paper"` 浅色）。
- **组件模型纪律**：`nsh` 无虚拟 DOM 重渲染，组件函数体只执行一次；所有信号（`signal()`）与动态 `props` 必须置于 JSX 表达式或访问器函数 `() => ...` 中读取，禁止在组件体顶层解构响应式属性。
- **沉浸式无系统标题栏与窗口拖拽**：`gui/nsh.json` 通过 `"window.titleBar": { "style": "hidden", "height": 44 }` 隐藏原生标题栏，[`gui/host/src/lib.rs`](../../../gui/host/src/lib.rs) 在建窗前写入窗口配置。macOS 左上角红绿灯留白（`36pt`）与 Windows 右上角标题栏按钮留白由 [`gui/src/shell/caption.ts`](../../../gui/src/shell/caption.ts) 的 `trafficLightLane` / `bandPadRight` 协调。由于 `nsh` 命中测试沿父链查找 `desktop={{ windowDrag }}`，顶栏拖拽必须直接挂在祖先容器（`Shell` 顶带、`Sidebar` 顶区、`PageBar` 外层 `56px` 容器、`Accounts` 顶壳），复合控件与可点击按钮显式设 `desktop={{ windowDrag: false }}` 隔离拖拽；全局通知容器 `NoticeBand` 外层使用 `pointerEvents: "box-none"`。
- **侧栏导航与模式切换**：模式胶囊（「技能 / 账号」）置于左栏顶部。技能模式主导航为：技能（`Sparkle`）、市场（`Store`）、卡组（`GalleryVerticalEnd`）、项目（`Briefcase`）、MCP（`Server`）；账号模式展示受管 Agent 列表（[`gui/src/pages/accounts/rail.tsx`](../../../gui/src/pages/accounts/rail.tsx)）。展开态顶部品牌行居中展示应用图标、`SkillStar` 字标与标语，点击在系统浏览器打开 GitHub 仓库主页。侧栏底栏（`SidebarFooter`）提供折叠、主题切换与设置三枚图标按钮，其上方为紧凑的 GitHub 认证状态入口（`SidebarGitHubEntry`，支持未登录 / 等待授权 / `@login` / 已过期四态与设备授权/刷新/跳转频道工作区）；账号额度入口在系统状态栏 / 托盘弹出窗与原生菜单（`⇧⌘U` / `Ctrl+Shift+U`），不占侧栏。
- **悬浮圆角工具栏（`chrome/toolbar.tsx`）**：`PageBar` 在 `TOOLBAR_H`（56px）总高与 `PAGE_PAD`（20px）水平边距内渲染 `height: 46px`、`borderRadius: 14`、`card` 底色的圆角工具栏，账号页复用同一组几何常量（`PAGE_BAR_BAND_TOP` / `PAGE_BAR_CARD_H` / `PAGE_BAR_RADIUS` / `PAGE_BAR_PAD_X`）。
- **工具栏只有一套控件尺度**：七个页面（技能 / 市场 / 卡组 / 项目 / MCP / 账号 / 发布者）的每个控件都是 `BAR_H` 32 高、`BAR_RADIUS` 10 圆角、`BAR_PAD_X` 10 水平内边距、`BAR_GAP` 6 图文间距、`BAR_ICON` 14 字形、`BAR_FONT` 12 标签、`BAR_CHEVRON` 11 菜单箭头；分段轨道同样 32 高，槽 `BAR_SLOT_H` 26 / `BAR_SLOT_RADIUS` 7，纯字形槽宽 `BAR_SLOT_W` 28。搜索框一律 `BAR_SEARCH_W` 190 宽（各页不再传宽度），按钮与字体尺寸不得在页面里另写。原语：`BarSearchInput`、`BarMenuTrigger`（图标 + 文字 + 箭头）、`BarSegmented` + `BarSegmentSlot`、`BarViewToggle`、`BarCount` / `BarTextChip`、`BarSecondary` / `BarPrimary`、`BarIconButton`（`tone="primary"` 是该条唯一的主色动作，`active` 表示筛选开启；主色按钮的实心色块按 `BAR_H - 2` 内缩 1px、圆角 `BAR_RADIUS - 1`，命中区仍是 `BAR_H` 见方，因此与描边按钮的可见色块等大，不与它抢视觉大小）。下拉族在 [`chrome/barMenu.tsx`](../../../gui/src/chrome/barMenu.tsx)：`BarMenuTrigger` + `BarMenuPanel` + `BarMenuRow`（`BAR_MENU_W` 196 宽、行 `BAR_MENU_ROW_H` 32 高、行前 `BAR_MENU_GLYPH` 13）。激活只有两种写法：**筛选生效**用 `accent_soft` 底（触发钮、agent 筛选槽、隐私开关），**当前模式**用 `card` 底 + `border` 描边（视图切换、分段槽）。
- **分段滑块胶囊（`chrome/segmented.tsx`）**：选中块为双层叠放绝对定位胶囊——外层控制位移（`translateX` / `translateY`，`--ease-in-out`，`--duration-sm`），内层控制相邻切槽的拉伸形变（朝目标延伸 `12%`，比位移稍慢收回）；系统开启减少动态效果时清空过渡，直接落槽。工具条里的 `ScopeSwitch`（「本地 / 频道」）用 `slotRadius` / `trackRadius` 把半径钉成工具条尺度（10 轨道 / 7 槽）；其余用法不给这两项，保持按高度取半圆的胶囊。
- **操作反馈与浮层锚定**：通知条见[通知](#通知)。悬停提示（`chrome/tooltip.tsx`）统一黑底白字。下拉菜单与锚定气泡统一使用单层 `<Overlay anchor={...} onPressOutside={...} onCancel={...}>`（不叠加冗余全屏遮罩层），键盘导航焦点采用 `focusVisibleStyle` 绘制 `accent` 描边环；账号工作台卡片/行与额度下拉窗卡片统一经 [`pages/accounts/accountContextMenu.ts`](../../../gui/src/pages/accounts/accountContextMenu.ts) 接入 `Menu.popup` 原生右键上下文菜单（支持切号、刷新、重授权、复制账号标识与跳转工作台）。确认弹窗与定制弹窗分别走 [`chrome/alerts.tsx`](../../../gui/src/chrome/alerts.tsx) 与 [`chrome/dialog.tsx`](../../../gui/src/chrome/dialog.tsx)（`ConfirmCard` 绑定 `onCancel` 原生响应 Escape）。确认弹窗（420px 卡）在遮罩上**水平与垂直都居中**：卡落在窗口正中，遮罩内层是全视口的内联弹性盒（`alignItems` 管横轴、`justifyContent` 管纵轴），不与顶栏往下数十分之一的老位置；需要整卡居中的表单类定制弹窗（如添加账号）落在同一位置，但两者不复用同一套定位实现。
- **退出拦截与任务确认（`shell/desktopShell.ts`、`shell/residentWindow.ts`）**：关闭主窗口的行为受「设置 → 常规」的「后台运行」开关（`gui_prefs.json` 的 `background_run`，默认关闭）控制：开启时关闭主窗口仅隐藏窗体保活，关闭时则调用 `requestAppQuit()` 走退出流程；仅当存在未完成的可取消后台任务（`!job.done && job.cancellable`）时订阅原生 `quitRequest`（`{ system: true, mainWindowClose: false }`），并与状态栏 / 托盘「退出 SkillStar」共用 `requestAppQuit()`。无活跃可取消任务时直接调用 `Application.quit()`；存在时聚焦主窗口并弹出破坏性 `ConfirmCard`（最多预览 3 项进行中任务及剩余计数），点击「中止并退出 / Cancel & Quit」逐项触发 `job.onCancel?.()` 后退出，取消或关闭弹窗则调用 `Application.cancelQuitRequest()` 放弃退出；等待期间若可取消任务全部结束则自动关闭确认弹窗。
- **错误边界与原地重试（`App.tsx`、`shell/Shell.tsx`、`shell/trayQuotaWindow.tsx`）**：`<Shell />` 根壳、各保活页按需挂载子树及托盘额度弹窗 `<QuotaPopoverPanel>` 均包裹 `<ErrorBoundary>`（`<TrayQuotaWindow />` 外层另设静默兜底），渲染异常时原位展示主题感知的 `ShellErrorFallback` 错误卡（带错误说明与「重试」按钮，点击 `onReset()` 原地重置恢复），单页异常不波及壳层与其他保活页。

## 通知

这是通知条的表面规范。停留时长在[等待](#等待)。页内提示仍用 [`chrome/alerts.tsx`](../../../gui/src/chrome/alerts.tsx)，不套这套表面。窗口标题保持 SkillStar，不加 Dock 角标。颜色只在渲染、访问器或回调里经 `paletteToken()`、`hexAlpha()`、`isLight()` 读取。

通知条挂在主面板内、页面之上，对话框（`DialogHost`）更在其上。叠放上通知条是 `zIndex` 50，任务卡片 40。

### 通知条

[`gui/src/shell/notify.tsx`](../../../gui/src/shell/notify.tsx)。一次一条，水平居中，顶边在顶栏下 12px（`TOOLBAR_H + 12`）。设置页没有顶栏，仍用这一位置。长文案换行，最宽 `NOTICE_MAX_W`（560）。新的一条换掉当前这条，钉住的也换掉。同一个 `replaceKey` 保持序号并原位换文案，宽度不动画。

卡片最小高 40，圆角 16，左内边距 8、右 6、上下 6。底是 `card` 到 `panel` 的纵向渐变，边框 `border`。阴影用 `scrim` 的两层：`0 16px 36px` 于 0.08，`0 3px 10px` 于 0.05，比抽屉轻。

图标井 24×24，圆角 8，距卡片左缘 8，与正文间隔 8。正文 14/20，色 `fg`。按钮与关闭都是高 28、圆角 10，二者间隔 4，关闭距右缘 6。操作按钮无填充，字 `accent_fg`，悬停填 `accent_soft`。关闭是 28×28 的 `x`（14），悬停 `panel_hover`。

| 状态 | 井 | 形 |
| --- | --- | --- |
| 成功、已恢复 | `accent_soft` | 勾，`accent_fg` |
| 失败 | `danger_bg` | `x`，`danger_fg` |
| 警告 | `warn_bg` | 三角，`warn` |
| 信息 | `info_bg` | 信息图标，色 `info` |
| 进行中 | `accent_soft` | `accent_fg` 圆环，按 `SPIN_MS` 旋转，弧长约 0.28 |
| 撤销 | `accent_soft` | 倒计时环。秒数是环上的等宽 10px 文字，不画进画布 |

撤销环只出现在未钉住、尚未恢复的操作条上。环随剩余时间变短，轨道是 `accent_fg` 的 0.28。进行中在 `pending` 不为 false 时不离开、也不扫光；结束后停 `NOTICE_PROGRESS_TTL_MS`（3 秒）并扫光。带 `undo` 的按钮跑完后，文案换成 `notify.restored`（已恢复 / Restored），停 `NOTICE_RESTORED_MS`（1.5 秒）再离开，并扫光。其余停留、暂停和钉住见[等待](#等待)。

扫光只在有结果时播放：状态进入、进度结束、已恢复。等选择的操作条不扫。一条 96px 宽的 `accent_fg` 高光在 900ms 内以 `--ease-out` 扫过；浅色峰值约 0.2，深色约 0.1。减少动态效果时不扫。

动效只动 `transform`、`opacity` 和 `filter`。进入从上方 10px、96%、模糊 8px 到静止（`--ease-out`，`--duration-lg`）；透明度更短，用 `--duration-md`。离开向上 6px、模糊 6px（`--ease-in`，`--duration-sm`）。同键换文案时，旧行 160ms 上移 5px 并模糊离开（`--ease-in`），新行晚 100ms，用 `--duration-md` 从下方 5px 进入（`--ease-out-strong`）。图标单独从 −40°、一半大小、模糊 4px 落入（`--ease-out-strong`，`--duration-lg`）。减少动态效果时只淡透明度 150ms，无位移、缩放、模糊和扫光。模糊是这条表面批准使用 `filter` 的地方。

`nsh` 没有 `status` 角色。正文用 `summary`，Android 为 polite。出现时不夺焦点。按钮可以用键盘到达。Escape 只在焦点位于这条内部时关闭当前一条，失焦即解绑，不抢走对话框和页面的 Escape。

## 动画

SkillStar 的桌面操作台是 **Operate + Read**。动效只说明一件东西从哪来、到哪去。全产品一个角色一条曲线，不按组件单独调。

规范的字面量在 [`gui/src/chrome/motion.ts`](../../../gui/src/chrome/motion.ts)，由 [`gui/src/chrome/motion.test.ts`](../../../gui/src/chrome/motion.test.ts) 锁住。nsh 自己解析时间函数，不解析 `var()`；`theme.json` 会丢弃非颜色、非字体、非长度的值。所以这些 token 是同名常量，不是 CSS 变量。调用走该文件的助手（`phaseTransition`、`fadeTransition`、`motionTransition`、`stretchTransition`、`springTransition`、`settleTransition`、`pressTransition`），不在调用点写曲线。

### 曲线

只用下表。CSS 默认 `ease`（`cubic-bezier(0.25, 0.1, 0.25, 1)`）不是选项；曲线没写就等于用了它，要补上。

| Token | 值 | 用于 |
| --- | --- | --- |
| `--ease-out` | `cubic-bezier(0, 0, .58, 1)` | 进入：通知、菜单、popover、对话框、抽屉 |
| `--ease-out-strong` | `cubic-bezier(.22, 1, .36, 1)` | 点击之后才落定的小零件：图标、标签、选中胶囊。手感最跟手 |
| `--ease-in` | `cubic-bezier(.42, 0, 1, 1)` | 只用于离开：通知到时、对话框或抽屉关闭、菜单收起、列表项被划掉。先慢后满速；用在进入上会显得晚，再猛地停住 |
| `--ease-in-out` | `cubic-bezier(.42, 0, .58, 1)` | 留在画面上，只改位置或尺寸：页签或分段指示、轮播翻页、重排让位、侧栏改宽、滚到锚点。不用于进入，慢启动会像迟滞 |
| `--ease-overshoot` | `cubic-bezier(.34, 1.56, .64, 1)` | 越过大约 10% 再回来。64px 以下小确认（勾选、喜欢、计数弹出等；开关滑块已改用 `--ease-spring`）。不用在对话框、抽屉，以及大约 64px 以上的物体上，也不用在一天会反复做的动作上 |
| `--ease-spring` | `spring(1480, 46)` | `nsh` 0.6+ 原生物理弹簧（`ζ ≈ 0.598`，约 9.6% 过冲，约 300ms 自动落稳并忽略 `transitionDuration`）。用于开关滑块（`settleTransition()`）与 64px 以下小确认及松手落定；CSS `linear()` 仍不受支持 |
| `linear` | `linear` | 只用于运转：旋转、进度、跑马。不用于位移 |

### 时长

| Token | 取值 | 区间 |
| --- | --- | --- |
| `--duration-sm` | 200ms | 小界面进入（通知、提示、菜单、勾选）：150–250ms |
| `--duration-sm` 的离开 | 160ms | 小界面离开：120–200ms，短于进入 |
| `--duration-md` | 300ms | 对话框、带内容的 popover、抽屉：250–350ms。其离开用 `--duration-sm` |
| `--duration-lg` | 400ms | 整屏或页面过渡：350–500ms。再长就显慢。其离开用 `--duration-md`。通知条进入也用它 |
| 悬停 / 按下 | 120ms | 100–150ms，曲线 `--ease-out` |
| 回弹 / 弹簧 | 300ms | 大约是对应平移动画的 1.5 倍，收回需要时间 |
| 减少动态效果 | 150ms | 只淡透明度 |

### 按角色

- **进入**：`--ease-out`。从它所属的一边来（通知从顶边，菜单从触发点），距离 8–24px。可以从 96% 放大，透明度比位移略早完成。通知可以带大约 8px 的模糊，落地时清掉。
- **离开**：`--ease-in`，同一条路径反过来，更短。
- **画面内移动**：`--ease-in-out`。
- **刚出现之物内部的图标、文字或计数**：可以再晚 100–200ms，用 `--ease-out-strong`，分成两步落地。
- **手势**：指针或手指移动时 1:1 跟随，不缓动。松开后用弹簧，库能吃释放速度就喂给它。
- **悬停与按下**：120ms `--ease-out`。悬停不动画布局。卡片网格仍走下面的通道一，不用这条。
- **属性**：只动画 `transform` 和 `opacity`。`filter` 少用。不动画 `top`、`left`、`width`、`height`，那些每帧都触发布局和重绘。

### 一致与可访问

- 一个角色一条曲线。
- 系统开启减少动态效果时：保留透明度变化，去掉位移和缩放；带弹簧或回弹、并且本来就有透明度的动画，改成 150ms `--ease-out` 淡入淡出。只有 `transform` 的落定直接到位。循环动画停止调度，停在终态。
- 动画不挡住输入。新动作从当前帧打断旧动画。

### 本产品现状

新动画按上面的角色接入。下面是已经接上的落点，避免把「还没有的位移」补进旧界面。

- **通道一 · 即时状态（0ms）**：Hover 与 Active 用 `hoverStyle` / `activeStyle` 瞬间换色，不用颜色弹簧，避免扫过卡片网格时连续重绘。详情抽屉与主面板同帧退让，抽屉表面（`chrome/sheet.tsx`）出现时不滑入。唯一的颜色过渡是详情「卸载」按钮（`pressTransition()`，背景与边框）。
- **通道二 · 留在画面上的位移**：侧栏选中框与分段滑块用 `motionTransition()`（只 `transform`，`--ease-in-out`，`--duration-sm`）。分段拉伸用 `stretchTransition()`，仍只朝目标延伸 12%，比位移多 50ms，位移结束时开始收回。开关滑块用 `settleTransition()`（经 `springTransition("transform", EASE_SPRING)` 走原生 `spring(1480, 46)`，约 300ms 自动落稳；减少动态效果时返回空样式直接落位）。
- **通道三 · 出现、离开与运转**：通知条的进入、离开、原位换行和扫光见[通知](#通知)。对话框与浮层的进入声明走 `fadeTransition()`（`--ease-out`，`--duration-md`）。其中卡组分享会从 -8px 淡入；其余浮层的 `translateY` 停在 0，关闭仍是直接卸下。阅读器正文在浮层打开后再等 320ms 挂上（`ENTRANCE_MS`），减少动态效果时首帧就挂。转圈、脉冲、扫光何时出现、停多久，以及任务卡片怎么放，见[等待](#等待)。
- **切换不重建页体**：两态分段控件（我的技能页的 本地 / 频道）两侧页体都常驻，切换只换 `opacity`、命中与 `zIndex`（[`ScopePane`](../../../gui/src/pages/my_skills/canvas.tsx)），和 `Shell` 保活页面同一条纪律。回到本地时重建卡板要一次挂上 3600 多个 view：实测一次提交 59.7ms，其后还有三次约 9ms 的行批，合计约 88ms，正好压在滑块 200ms 的滑行上，用户看到的就是掉帧；保活后同一动作一次提交 5.7ms（`nsh test --trace` 的 `nsh::perf commit`）。会重建大列表的开关都按这条办，不用「动画结束后再换内容」绕。
- **引擎滚动**：回到顶部和智能体底栏翻页走 nsh `scrollTo({ animated: true })`。引擎内部是 300ms 的三次缓出，调用处不能换成 `--ease-in-out`。
- **尚未使用的 token**：`--ease-overshoot`（开关滑块已改用 `--ease-spring`）。`--ease-out-strong` 用于通知原位换行和图标落入；`--duration-lg` 用于通知进入。没有拖拽松手弹簧，也没有整屏过渡。

## 等待

一次等待只选一种反馈。变长时在同一处升级（阶段句，然后右上角任务卡片），不另把页面或选择栏变灰。本地搜索和筛选、保活切页、已经在画面上的列表，保持原样。

计时只有一处：[`gui/src/chrome/delayedFlag.ts`](../../../gui/src/chrome/delayedFlag.ts)。`APPEAR_MS` 等于动画的 `--duration-md`（300ms），`HOLD_MS` 500ms，`TURN_MS` 800ms，`SLOW_MS` 10 秒。[`delayedFlag.test.ts`](../../../gui/src/chrome/delayedFlag.test.ts) 锁住这四个数。

### 种类

| 种类 | 用在 | 画面 |
| --- | --- | --- |
| 无指示 | 短于出现延迟就结束的结果；预取、自动检查更新、本地搜索和筛选、保活切页 | 画面不动。按下的操作图标可以立刻转 |
| 转圈 | 位置已知、大约从出现延迟到数秒的动作 | 原地一个指示。标签和按钮宽度不变 |
| 骨架 | 版式已知，第一次还没有内容 | 晚 `APPEAR_MS` 出现，至少停 `HOLD_MS`；更快则不画。壳和已有数据留下。形状是 `Pulse` |
| 进度 | 调用方真的报了完成数，或本来就有具名阶段 | 只画真分数或阶段名。调用能取消才放取消 |
| 乐观 | 点一下就能改回来的本地开关 | 先翻转。失败滚回，重试留在原处 |
| 逐段 | 结果真的一块块到达 | 按到达的块补上。整段返回的译文保持整段 |

### 共用规则

- **一个指示**。布局不跳。失败留在触发它的控件上，并给出重试。会自己消失的通知不代替这一行。行内保存是 [`gui/src/chrome/saveStatus.tsx`](../../../gui/src/chrome/saveStatus.tsx)：`wait.saving`（正在保存）、`wait.saved`（已保存，约 1.6 秒后收起）、`wait.couldntSave`（未能保存）加原地重试。文案键 `wait.*` 在 [`translation.ts`](../../../gui/src/services/translation.ts) 的 `zh-CN` 与 `en` 一起维护。技能卡和卡组卡的卡面都不放这一行，见[技能卡](#技能卡)。
- **转圈**在 [`gui/src/chrome/feedback.tsx`](../../../gui/src/chrome/feedback.tsx) 的 `IconSpin`。一圈等于 `TURN_MS`，线性，停在这一圈结束。刷新、更新、勾选这类操作图标从按下就开始转。名为 `loader` 的通用指示等 `APPEAR_MS` 才出现，出现前是同尺寸空盒，至少再停 `HOLD_MS`。减少动态效果时帧跟着忙闲走，不再旋转。
- **骨架**用 `Pulse`（2s `--ease-in-out`），占真实卡片的尺寸（技能卡高 `SKILL_CARD_H` 160、圆角 16）。不加快闪带。
- **通知**经 [`gui/src/shell/events.ts`](../../../gui/src/shell/events.ts) 的 `toast()` / `publishToast()`。一次一条。成功和信息 `TOAST_SHORT_TTL_MS`（3 秒）。警告沿用 `NOTICE_TTL_MS`（5 秒）。没写 `ttlMs` 的状态通知按文案长度从 3 秒到 5 秒。带操作的未钉住通知至少 5 秒，操作本身不钉住。`toast()` 的失败钉住，直到关闭或被下一条换掉；显式 `pinned: true` 同样不自动消失，但下一条会换掉它。指针悬停或焦点在通知内时，剩余时间暂停。表面与停留见[通知](#通知)。
- **任务卡片**是 [`gui/src/shell/jobBoard.ts`](../../../gui/src/shell/jobBoard.ts) 与 [`jobBand.tsx`](../../../gui/src/shell/jobBand.tsx)，由 `Shell` 挂载，右上角，宽 320，圆角 16，表面色 `card`。顶边在顶栏下 12px。人可以离开、并且跑过大约 `SLOW_MS` 的任务才用它。只有调用方报了完成数才画比例。不能取消的任务不放取消。卡片出现不足 `APPEAR_MS` 就直接收起；已展示的任务完成后展示勾选反馈并停留 `JOB_DONE_TTL_MS`（2 秒）自动收起。仍在跑且超过 `SLOW_MS` 时，卡片上加 `wait.takingLonger`（比平时更久）。
- **乐观更新**只用于技能卡和详情里的智能体开关、仓库收藏、卡组的智能体开关、描述翻译的语言选择。先翻转本地状态，失败滚回。智能体开关与仓库收藏的保存状态行写在详情列/来源行上，技能卡与卡组卡的卡面不写（见[技能卡](#技能卡)）；描述翻译的语言选择采用静默乐观更新与失败回滚，不在描述下方显示行内 `SaveStatus`（`正在保存`/`已保存`）。卸载、删除账号、删除卡组、市场安装、保存并同步保持确认后的原状态，直到调用返回。
- **译文和 `SKILL.md`** 由桥一次返回整段。原文留下。`IconSweep` 每秒一次，表示正在翻译。引擎未配置完整（如 LLM 缺模型或密钥）时，在描述/正文下方显示前往「设置 → 翻译」的配置提示；引擎返回空结果或传输失败时停止 `IconSweep` 并在段落下提供原地重试；翻译设置（含 API Key 提交/清除）保存后经 `translationPrefsChanged` 清空失败态并重译。减少动态效果时阅读器立刻挂上正文；脉冲行仍遵守出现延迟。
- **减少动态效果**保留忙闲，转圈和脉冲停止。正在等待的区域使用 `accessibilityState` 的 `busy`。结果仍由通知带说出。等待开始时焦点留在原地。

### 各页

- **我的技能、市场、发布者、卡组、项目栏、账号、设置存储、市场详情、`SKILL.md`**：第一次还没有内容时用骨架。市场搜索只在技能和发布者两列都空时才考虑骨架。已有列表或概览的刷新留着旧数据，只转按下的图标。卡组第一次打开先空白，慢了才出骨架，空状态只在加载结束后出现。项目栏不先闪空列表。详情和阅读器用几行脉冲，不写「正在阅读」。
- **自动检查更新**不转动刷新图标。手动刷新我的技能只转该图标，成功是短通知。
- **市场和发布者同步快照**：只转刷新图标，成功短通知，不另写一行刷新状态。大约 `SLOW_MS` 后升到任务卡片。
- **单个技能更新**：只转该图标。失败留在卡上并可重试。结果由一次通知说明，不出任务卡片。
- **批量更新技能**：只转工具栏的更新按钮、选择条和卡面上的更新图标。可以离开。不画假的「2/8」。网格不换骨架。结果由一次通知说明，不出任务卡片。
- **来源重装**：转按下的图标，大约 `SLOW_MS` 后升到任务卡片。
- **市场安装**：标签和宽度不变。先转圈，大约 3 秒后加 `wait.installing`（正在安装该技能），大约 `SLOW_MS` 后任务卡片。多个技能同时在装时各转各的卡，任务卡片一次安装一张；忙碌记录见 [Marketplace](../marketplace/README.md#技能搜索与导入)。这一次安装是一个请求，不能取消就不放取消。卸载在确认后只转按下的控件。榜单卡不订阅 `skillstar://git-progress`；导入对话框仍用自己的 progress sink。
- **账号**：单个刷新从按下就转刷新图标。成功短通知，失败横幅留下并可重试。切换、添加账号、本机导入、令牌导入、删除与批量删除的成功同样只发短通知（「已切换至 <账号>」在隐私模式下同步脱敏），正文不放成功横幅——横幅只报错误。批量刷新的卡片写 `wait.refreshingAccounts`（已完成 / 总数），分数来自循环里数过的账号。选择栏不变灰，删除不锁。取消停掉还没轮到的账号。单个失败留在该账号上。
- **删除账号、批量删除、卸载技能、删除卡组**：确认之后只转按下的控件。不先从列表拿掉。
- **卡组**：新建用晚出现的 `loader`，提交中仍不能关闭。复制和删除转那一张卡。导出时标签和宽度不变，路径留在原处。
- **项目**：保存并同步转勾选图标，结果留在状态行。检查副本的指示晚出现，失败留在该区块。
- **设置**：镜像探测快于出现延迟不闪，失败保持断开图标。翻译模型列表、关于里的检查更新、存储的清理、修复、删除和恢复，都留着闲置文案，只转图标。关于页不另放一枚「正在检查」。存储删除仍先确认；调用没有字节进度就不画进度条。第一次扫描用区块骨架，再次扫描留着已有概览。网络诊断只在面板行上一个指示，按钮保持活动图标。大约 `SLOW_MS` 后加「比平时更久」。诊断一次返回，不拆成假阶段。
- **GitHub 登录**的开始阶段晚 `APPEAR_MS` 才出 `loader`。设备码等待阶段保持现有面板。添加账号的开始、令牌和导入同样晚出 `loader`。复制分享码和设备码：成功是短通知，失败钉住。
- **技能导入**：保留现有阶段名和取消。大约 `SLOW_MS` 后可以「继续在后台」（`wait.continueInBackground`），任务进右上角，页面可继续用，取消仍然有效。还没放后台时，关闭对话框会取消。扫描进入选择、分享或打包时，把对话框打开回来。完成或失败后结束这张卡片。
- **卡组导入**：同样可以放到后台。这次调用不能取消，卡片上不放取消。放到后台后卡片仍留在树上。还没放后台时关掉浮层，不取消已经发出的导入。

## 色彩系统与排版

采用与黑洞引力透镜应用图标同源的 **Obsidian Event Horizon（`current` 深色，默认）** 与 **Stardust Alabaster（`paper` 浅色）** 双主题语言，由 [`gui/src/services/themeMode.test.ts`](../../../gui/src/services/themeMode.test.ts) 守卫亮度单调递进与 WCAG AA 对比度（正文与 `fg_muted` ≥ 4.5:1，控件/Accent ≥ 3.0:1）：

- **表面层级与微光**：
  - `current`（深色）：`bg` `#06070a` < `panel` `#0c0e14` < `card` `#141720`；悬停 `card_hover` `#1b1f2b` / `edge` `#464c5c`；选中透出光子环暖琥珀微光（`card_active` `#211910`、`card_active_hover` `#2a2014`、`panel_active` `#241b10`）。
  - `paper`（浅色）：`bg` `#ebe8e2` < `panel` `#f7f6f2` < `card` `#ffffff`，主墨色 `fg` `#18181b`；悬停与选中呈现香槟暖金微光（`card_hover` `#fbf7f0`、`card_active` `#fcf4e4`、`panel_active` `#f5e8cf`）。
- **品牌 Accent 与语义色谱**：
  - 引力琥珀 `accent`（DARK `#ba5a08` / LIGHT `#b45309`，配 `on_accent` `#ffffff`）、前景金 `accent_fg`（DARK `#f6b84c` / LIGHT `#9a4406`）。
  - 语义分类：`ok` 翡翠（`#2dd4bf` / `#0f766e`）、`warn` 亮金（`#eab308` / `#a16207`）、`danger` 绯红（`#f85149` / `#dc2626`）、`info` 星光蓝（`#60a5fa` / `#1d4ed8`）、`violet`（`#a78bfa` / `#7c3aed`）、`cyan`（`#22d3ee` / `#0891b2`）、`amber`（`#f97316` / `#c2410c`）。
- **主题响应式铁律**：
  - 应用表面颜色**只能在渲染/访问器/回调内调用 `paletteToken()` 读取**，严禁固化在模块级常量或静态样式表（否则切换浅色主题时会滞留深色字色/底色）。如排版样表类整组样式必须导出为工厂函数（如 `readerTypography()`）。
  - `setThemeMode()` 同步调用 `setColorScheme()`，驱动引擎内建 TextView 代码高亮与原生选区同步切换 `theme.json` 的 `light` / `dark` 块；根组件通过 `installScrollbarTheme()`（`Scrollbar.setTheme`）让全局 `<Scroll>` 与 `<List>` 悬浮滚动条随 `current` / `paper` 主题实时换色。
- **排版标尺**：默认无衬线字体 `DM Sans`（CJK 回退系统原生字体），路径/命令/哈希/数值一律使用 `JetBrains Mono` 与 `tabular-nums`；字阶严格使用整数字号（`10px` 徽标、 `12px` 说明、 `14px` 卡片与控件正文、 `16px` 弹窗标题、 `18–20px` 摘要指标），禁止浮点半步字号；双语对照译文保留充足行高。

## 技能卡

- **统一卡片骨架（`chrome/cardShell.tsx`）**：`SkillCard`、`MarketCard`、`DeckCard` 共用最小列宽 `SKILL_CARD_W`（336px）、固定高度 `SKILL_CARD_H`（160px）与 `borderRadius: 16`。网格模式通过 [`gui/src/pages/my_skills/grid.ts`](../../../gui/src/pages/my_skills/grid.ts) 的 `cardTrackWidth` 按 `repeat(auto-fill, minmax(336px, 1fr))` 均分剩余宽，默认窗口宽 `WINDOW_W` 下关抽屉为三列（1040px），打开 `DETAIL_COLUMN_W`（352px）详情轨道时同帧收为两列（688px）；列表模式铺满 100% 宽度。描述限 `numberOfLines: 2`。
- **仓库头像与来源链接（`chrome/repoAvatar.tsx` + `native/src/repo_avatars.rs`）**：卡片左上角展示 `32×32` 圆角 `9` GitHub 拥有者头像（并发限流 `4` 路拉取至 `cache/avatars/{owner}.png`，离线回退 `gitBranch`；`local` 技能显示 `hardDrive`），兼作悬停勾选框热区。副标题优先解析可点击 GitHub/HTTP 页面，不可解析或本地技能保留纯文本，点击不再冒泡打开抽屉。
- **智能体底栏开关（`chrome/agentRail.tsx` + `native/src/agent_icons.rs`）**：`SkillCard` 与 `DeckCard` 底部为 `38px` 内缩发丝线底栏，`28×28` 开关内嵌 `16×16` 由 `ss-gui-bridge` 编译期打包的 83 枚智能体品牌 SVG（单色线稿 `#e8eef8` 在 `paper` 模式自动重着色为 `#18181b`，多色与 `data:image/` 图标透传）。视口默认显示 7 槽（`RAIL_MAX_VISIBLE = 7`，`232px`）；超出 7 槽自动挂载左右步进箭头与横向 `<Scroll>`。`MarketCard` 底栏展示来源元数据与星标/安装量，不渲染 Hot/Popular 营销徽标。底栏开关按下即翻转链接态；**技能卡（`SkillCard`）与卡组卡（`DeckCard`）卡面都不写保存状态行**——保存中、已保存、失败都不占卡身的一行，失败由钉住的通知带说明，再点同一个图标就是重试。
- **行虚拟滚动与节点稳定复用**：`my_skills`、`marketplace`、`marketplace/publisher`、`skill_cards` 统一采用 `nsh` `<List>` 按行虚拟化（`scrollEventThrottle={32}`），滚动偏移超出阈值显示 [`BackToTopButton`](../../../gui/src/chrome/scrollTop.tsx)。卡片可变区域使用 `<If>` 稳定占位，避免滚动复用单元格时重建 SVG 节点；超过阈值的弹出列表框（如最近仓库名单、来源筛选、卡组成员选择）必须在定高视口外包裹 `<Scroll>` 以免被 `overflow: "hidden"` 静默裁切。

## 交互

- **详情读取与 `SKILL.md` 阅读器**：详情请求绑定当前选定目标及代际，晚到响应不覆盖新选择；长任务经 `DomainJob`（`src/services/domainJob.ts`）流式上报进度并支持取消。`SKILL.md` 阅读器（`my_skills/skillReaderSlot.tsx`）头部仅绘制技能名与可用时的样式/翻译图标，开启翻译开关或手动点译时按段落下方追加对照译文，缓存命中即即时渲染。
- **设置页（`pages/settings/`）**：跨模式入口（保留 Skills / Accounts 之前所处模式与位置上下文），不设顶部 `PageBar`，宽窗下采用左侧六大精简次级导航（智能体 / 通用 / 网络 / 翻译 / 存储 / 关于），导航卡片在内容区整高内**上下居中**（不顶部贴齐），左右留白与主侧栏对称。「常规」分区提供 `tray_quota_enabled` 与 `background_run`（「后台运行」）开关；「翻译」分区（`sections/translation.tsx`）提供 Google 机翻及手动配置的大模型端点（`llm_base_url`）、API Key（落盘至 `secrets/translation/api_key`，读取时只展示掩码状态）与模型 ID（含可选 `{base}/models` 拉取）。该分区默认即「描述」「SKILL.md」两个面开关开启、译文语言简体中文、引擎 Google 机翻；盘上已保存的显式值优先，新装不必先进设置页。设置开关复用 `SettingsSwitch`（`ok` 绿开启 / `edge` 关闭，滑块 `settleTransition()` 回弹落位）。路径与延迟数字一律等宽展示。
- **项目页（`pages/projects/`）**：左侧项目列表 + 右侧主详情区（存在未过期项目技能 MCP 计划时在列表上方展示 `PendingSkillPlanSection`，下方为按智能体管理技能 → 部署方式图例 → 检测规则文件）；未保存更改前「保存并同步项目」呈静默描边且不可提交。注册项目与更改路径弹窗（`pages/projects/dialogs.tsx`）在路径输入框旁提供原生目录选择按钮（`folderOpen` 图标，带悬停提示与防重入锁），经 `Projects.pickFolder` 呼出系统目录选择器回填路径并清除校验错误。

## 验证与文案术语

```bash
nsh check gui
nsh test gui
cargo test -p ss-gui-bridge --locked
```

新增或修改界面文案时，必须同步更新 [`gui/src/services/translation.ts`](../../../gui/src/services/translation.ts) 中的 `zh-CN` 与 `en` 两套字典。常用术语保持单一名词：

| 概念 | EN | ZH |
| --- | --- | --- |
| 技能单元 | Card / Skill | 技能 |
| 技能分组 | Deck | 卡组 |
| 技能目录 | Marketplace | 市场 |
| Agent CLI / IDE | Agent | 智能体 |
| GitHub 共享频道 | Channel | 频道 |
| 本机技能库 | Hub | Hub |
| 卸载 / 删除 / 部署 / 链接 | Uninstall / Delete / Deploy / Link | 卸载 / 删除 / 部署 / 链接 |

破坏性确认需明确对象与后果；报错遵循「先说明发生了什么，再给出恢复路径」。

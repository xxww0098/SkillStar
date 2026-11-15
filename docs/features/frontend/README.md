# 界面约定

状态：active

本文件只记 SkillStar 桌面端（基于 `nsh` 的 `gui/` 应用）自身的视觉与交互契约；目录与三层边界见 [boundaries.md](../../boundaries.md#gui-壳nsh)。
各页产品行为留在相邻功能文档：[Skills](../skills/README.md)、[Marketplace](../marketplace/README.md)、[Accounts](../accounts/README.md)。

## 壳

- **应用位置与主题字典**：界面在 `gui/`，配色由 [`gui/theme.json`](../../../gui/theme.json) 与 [`gui/src/services/themeMode.ts`](../../../gui/src/services/themeMode.ts) 定义，文案在 [`gui/src/services/translation.ts`](../../../gui/src/services/translation.ts)（`en` 与 `zh-CN` 同步维护）。持久化主题读取 `gui_prefs.json` 的 `background_style`（`"current"` 深色 / `"paper"` 浅色）。
- **组件模型纪律**：`nsh` 无虚拟 DOM 重渲染，组件函数体只执行一次；所有信号（`signal()`）与动态 `props` 必须置于 JSX 表达式或访问器函数 `() => ...` 中读取，禁止在组件体顶层解构响应式属性。
- **沉浸式无系统标题栏与窗口拖拽**：`gui/nsh.json` 通过 `"window.titleBar": { "style": "hidden", "height": 44 }` 隐藏原生标题栏，[`gui/host/src/lib.rs`](../../../gui/host/src/lib.rs) 在建窗前写入窗口配置。macOS 左上角红绿灯留白（`36pt`）与 Windows 右上角标题栏按钮留白由 [`gui/src/shell/caption.ts`](../../../gui/src/shell/caption.ts) 的 `trafficLightLane` / `bandPadRight` 协调。由于 `nsh` 命中测试沿父链查找 `desktop={{ windowDrag }}`，顶栏拖拽必须直接挂在祖先容器（`Shell` 顶带、`Sidebar` 顶区、`PageBar` 外层 `56px` 容器、`Accounts` 顶壳），复合控件与可点击按钮显式设 `desktop={{ windowDrag: false }}` 隔离拖拽；全局通知容器 `NoticeBand` 外层使用 `pointerEvents: "box-none"`。
- **侧栏导航与模式切换**：模式胶囊（「技能 / 账号」）置于左栏顶部。技能模式四项主导航为：技能（`Sparkle`）、市场（`Store`）、卡组（`GalleryVerticalEnd`）、项目（`Briefcase`）；账号模式展示受管 Agent 列表（[`gui/src/pages/accounts/rail.tsx`](../../../gui/src/pages/accounts/rail.tsx)）。展开态顶部品牌行居中展示应用图标、`SkillStar` 字标与标语，点击在系统浏览器打开 GitHub 仓库主页。
- **悬浮圆角工具栏（`chrome/toolbar.tsx`）**：`PageBar` 在 `TOOLBAR_H`（56px）总高与 `PAGE_PAD`（20px）水平边距内渲染 `height: 46px`、`borderRadius: 14`、`card` 底色的圆角工具栏，账号页复用同一组几何常量（`PAGE_BAR_BAND_TOP` / `PAGE_BAR_CARD_H` / `PAGE_BAR_RADIUS` / `PAGE_BAR_PAD_X`）。内置 `BarSearchInput`、`BarViewToggle`、`BarSecondary`、`BarPrimary`、`BarIconButton`；「我的技能」`ScopeSwitch` 为双态胶囊（「本地 / 频道」）。
- **分段滑块胶囊（`chrome/segmented.tsx`）**：选中块为双层叠放绝对定位胶囊——外层控制位移（`translateX` / `translateY`，约 `80ms`），内层控制相邻切槽的拉伸形变（朝目标延伸 `12%` 后约 `120ms` 收回）；系统开启减少动态效果时清空过渡，直接落槽。
- **操作反馈与通知带（`shell/notify.tsx` + `chrome/tooltip.tsx`）**：操作结果经 `NoticeBand` 悬浮在顶部居中（最多堆叠 4 条，同 `replaceKey` 覆盖，常规 5 秒自动收起，带 `.action` 或错误通知手动关闭）。悬停提示统一黑底白字；确认弹窗与定制弹窗分别走 [`chrome/alerts.tsx`](../../../gui/src/chrome/alerts.tsx) 与 [`chrome/dialog.tsx`](../../../gui/src/chrome/dialog.tsx)。

## 动画

SkillStar 的桌面操作台定位为 **Operate + Read**，拒绝纯装饰性“动效债务”。

- **通道一 · 即时状态切换（0 ms）**：Hover 与 Active 直接走 `hoverStyle` / `activeStyle` 瞬间换色，禁止使用颜色弹簧，避免鼠标扫过卡片网格引发连续全屏重绘；详情抽屉与主面板退让同步同帧落位。
- **通道二 · 空间位移与拉伸（`chrome/motion.ts`）**：侧栏选中框、分段滑块、开关滑块与高危操作过渡采用 `motionTransition()`（仅 `transform`，`80ms`）与 `stretchTransition()`（`120ms`）。
- **通道三 · 纯视觉微动作（`chrome/feedback.tsx`）**：旋转加载指示 `IconSpin`（0.8s 匀速）与翻译扫光 `IconSweep`（亮芯 + 尾迹单程掠过）基于原生 Animated `translateX` / `rotate` 驱动，不触发布局尺寸变化；状态切换通过 `<If>` 挂载在 JSX 表达式内。
- **减少动态效果（Reduce Motion）**：遵循系统降动效偏好，`motionTransition()` / `stretchTransition()` 返回空样式，周期光束与步进动画停止调度、直接停留在终态。

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
  - `setThemeMode()` 同步调用 `setColorScheme()`，驱动引擎内建 TextView 代码高亮与原生选区同步切换 `theme.json` 的 `light` / `dark` 块。
- **排版标尺**：默认无衬线字体 `DM Sans`（CJK 回退系统原生字体），路径/命令/哈希/数值一律使用 `JetBrains Mono` 与 `tabular-nums`；字阶严格使用整数字号（`10px` 徽标、 `12px` 说明、 `14px` 卡片与控件正文、 `16px` 弹窗标题、 `18–20px` 摘要指标），禁止浮点半步字号；双语对照译文保留充足行高。

## 技能卡

- **统一卡片骨架（`chrome/cardShell.tsx`）**：`SkillCard`、`MarketCard`、`DeckCard` 共用最小列宽 `SKILL_CARD_W`（336px）、固定高度 `SKILL_CARD_H`（160px）与 `borderRadius: 16`。网格模式通过 [`gui/src/pages/my_skills/grid.ts`](../../../gui/src/pages/my_skills/grid.ts) 的 `cardTrackWidth` 按 `repeat(auto-fill, minmax(336px, 1fr))` 均分剩余宽，默认窗口宽 `WINDOW_W` 下关抽屉为三列（1040px），打开 `DETAIL_COLUMN_W`（352px）详情轨道时同帧收为两列（688px）；列表模式铺满 100% 宽度。描述限 `numberOfLines: 2`。
- **仓库头像与来源链接（`chrome/repoAvatar.tsx` + `native/src/repo_avatars.rs`）**：卡片左上角展示 `32×32` 圆角 `9` GitHub 拥有者头像（并发限流 `4` 路拉取至 `cache/avatars/{owner}.png`，离线回退 `gitBranch`；`local` 技能显示 `hardDrive`），兼作悬停勾选框热区。副标题优先解析可点击 GitHub/HTTP 页面，不可解析或本地技能保留纯文本，点击不再冒泡打开抽屉。
- **智能体底栏开关（`chrome/agentRail.tsx` + `native/src/agent_icons.rs`）**：`SkillCard` 与 `DeckCard` 底部为 `38px` 内缩发丝线底栏，`28×28` 开关内嵌 `16×16` 由 `ss-gui-bridge` 编译期打包的 83 枚智能体品牌 SVG（单色线稿 `#e8eef8` 在 `paper` 模式自动重着色为 `#18181b`，多色与 `data:image/` 图标透传）。视口默认显示 7 槽（`RAIL_MAX_VISIBLE = 7`，`232px`）；超出 7 槽自动挂载左右步进箭头与横向 `<Scroll>`。`MarketCard` 底栏展示来源元数据与星标/安装量，不渲染 Hot/Popular 营销徽标。
- **行虚拟滚动与节点稳定复用**：`my_skills`、`marketplace`、`marketplace/publisher`、`skill_cards` 统一采用 `nsh` `<List>` 按行虚拟化（`scrollEventThrottle={32}`），滚动偏移超出阈值显示 [`BackToTopButton`](../../../gui/src/chrome/scrollTop.tsx)。卡片可变区域使用 `<If>` 稳定占位，避免滚动复用单元格时重建 SVG 节点；超过阈值的弹出列表框（如最近仓库名单、来源筛选、卡组成员选择）必须在定高视口外包裹 `<Scroll>` 以免被 `overflow: "hidden"` 静默裁切。

## 交互

- **详情读取与 `SKILL.md` 阅读器**：详情请求绑定当前选定目标及代际，晚到响应不覆盖新选择；长任务经 `DomainJob`（`src/services/domainJob.ts`）流式上报进度并支持取消。`SKILL.md` 阅读器（`my_skills/skillReaderSlot.tsx`）头部仅绘制技能名与可用时的样式/翻译图标，开启翻译开关或手动点译时按段落下方追加对照译文，缓存命中即即时渲染。
- **设置页（`pages/settings/`）**：跨模式入口（保留 Skills / Accounts 之前所处模式与位置上下文），不设顶部 `PageBar`，宽窗下采用左侧六大精简次级导航（智能体 / 通用 / 网络 / 翻译 / 存储 / 关于）。设置开关复用 `SettingsSwitch`（`ok` 绿开启 / `edge` 关闭 / `80ms` 位移）。路径与延迟数字一律等宽展示。
- **项目页（`pages/projects/`）**：左侧项目列表 + 右侧主详情区（按智能体管理技能 → 部署方式图例 → 检测规则文件）；未保存更改前「保存并同步项目」呈静默描边且不可提交。

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

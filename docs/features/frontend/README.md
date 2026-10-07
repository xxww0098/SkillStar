# 界面约定

状态：active

本文件只记 SkillStar 桌面壳自己的视觉与交互契约。目录和依赖见 [boundaries 的 GPUI 壳模块](../../boundaries.md#gpui-壳模块)。通用 GPUI Kit 界面判断以 [Design Guides](https://gpui-kit.com/docs/design-guides.md) 为准，这里不复述。

各页的产品行为留在相邻功能文档。技能页见 [Skills](../skills/README.md)，市场见 [Marketplace](../marketplace/README.md)，账号布局见 [Accounts](../accounts/README.md)。

## 壳

- 界面在 `crates/ss-gpui`。配色在 `src/theme.rs`，文案在 `assets/locales/`。`en` 与 `zh-CN` 同步。
- 技能模式侧栏四项的图标在 `NavPage::icon`：技能 `Sparkle`，市场 `Store`，卡组 `GalleryVerticalEnd`（卡面在下，后卡边从下往上露出），项目 `Briefcase`。收起后只剩图标，四枚轮廓必须能分开。卡组空状态、项目空状态，以及技能空状态里去市场的按钮，用同一枚。
- 这四项共用一块选中框。切换时框滑到新的一项，文字和图标颜色跟着走。打开设置或发布者详情时框在原位淡出，四项都不再显示选中。系统开启减少动态效果时框直接落在目标项上。
- 页面在 `render` 里读 `theme::palette()`。新状态色先在 `Palette` 加字段，并配齐 `DARK` 与 `LIGHT`。`gui_prefs.json` 的 `background_style`（`"current"` 深、`"paper"` 浅）是主题持久化的唯一来源。侧栏主题按钮和 Settings › Appearance 都走 `theme::set_mode` / `toggle`。
- 提示遮罩用 `theme::prompt_veil`，后面的页面保持可见。Kit 对话框读 `ThemeColor.overlay`，页内提示走 `chrome::prompt_mask`。
- 悬停提示底色是黑色，文字是白色，两种模式一样，画在 `chrome::tooltip`。菜单、弹层、通知和搜索仍用卡片色：它们和提示共用 kit 的 popover 色，改那个字段会把这些一起涂黑。按钮的 `tooltip(字符串)` 不经过这个函数，提示挂在元素自己的 `.tooltip` 上。
- 刷新图标只在该次请求进行中旋转，采样跟显示器刷新走（120Hz 屏幕上就是 120 帧）。旋转是字形上的绘制变换。技能网格、市场榜、发布者列表和账号额度区是重放视图：这些帧重绘图标和顶栏，不重新排版后面的卡片。骨架脉冲和重置卡闪烁仍是 20 帧。更新省略号大约每 400ms 一步。单张额度卡自己的刷新，以及重置卡的弹簧和闪烁，仍会重排额度区，因为额度卡高度不固定，不能按张缓存。
- 悬停和按下在指针状态变化时重绘一次。颜色不走弹簧：弹簧每帧重绘整个窗口，鼠标划过卡片时会一直掉帧。详情列的「卸载」是唯一例外，见 [动画](#动画)。模式胶囊、技能侧栏选中框和重置卡的位移仍用临界阻尼弹簧，大约 80ms 内停住。
- 顶栏空白可以拖动窗口。拖拽层铺在整行背后；按钮、分段轨道和搜索框调用 `occlude()`，按下它们不会拖动窗口。`overflow` 包装的弹性子项写 `w_auto()`，中间的空白才留得出来。
- 顶栏搜索框点到框外会失焦，框内没有菜单和补全时按 Escape 也会失焦。这次失焦不拦截鼠标按下，空白处仍能拖动窗口。
- 确认框在窗口正中，由 `chrome/dialog.rs` 绘制。按钮种类见 [platform](../platform/README.md)。打开时卡片仍从窗口顶部滑到这个位置。壳和主面板里的页面都是重放视图，这段动画不会把侧栏和后面的卡片网格每帧重排。

## 动画

120 帧是显示器的刷新信号：这一帧的场景要在约 8ms 内交出去。GPUI 没有合成器补间。`div` 的位置会改布局边界；视图缓存命中时只按原坐标重放上一帧场景，不会把位移交给 GPU。`with_max_fps(120)` 改走定时器，比刷新信号更漂，不要用来追 120 帧。

新动画先选一条通道：

- 透明度、颜色、字形旋转走 `with_animation`，不设 `with_max_fps`。旋转只用 `icon_spin`（`chrome/mod.rs`），它是字形上的 `Transformation::rotate`。
- 位置必须改布局时（选中框、对话框入场）把时长压短。位移弹簧用 `motion_spring`，临界阻尼，大约 80ms 内停。不动的重子树用 `replay_view`，或带确定宽高的 `.cached()`。对话框入场会改子树的布局边界，缓存键里有这块边界，滑动期间重放命中不了。整份 Markdown 要等滑动停再挂上，SKILL.md 悬浮窗就是这样。
- 呼吸和省略号不需要每帧一个新样子。脉冲用 `pulse`（20 帧）。省略号大约 400ms 一步，继续 `with_max_fps`。
- 详情列底部的「卸载」悬停时，底色从 `danger_bg` 走到 `danger_hover`，边框走向 `danger`，文字走向 `danger_fg`，垃圾桶图标轻轻抬起、放大并转动。进度用 `motion_spring`，大约 80ms 内停，指针离开时从当前进度返回。系统开启减少动态效果时直接落到终态。这是唯一走弹簧的悬停色，不要抄到技能卡或其它按钮上。卡片仍按张缓存，这次重绘不重新排版网格，也不走 `revise`。

`with_animation` 和 `with_spring` 标脏的是正在绘制的那个视图，祖先一起标脏，子孙和兄弟不会。重内容必须是这个视图的兄弟或子孙，并且自己是实体。整块区域用 `replay_view`：父级 `relative`，子视图绝对定位并铺满。固定宽高的卡片按张 `.cached()`，不要写成 `absolute().size_full()`，否则会盖住网格。不要在 `render` 里 `cx.new` 动画实体，每帧都会把动画重开。

数据变化走页面的 `revise`，把代际加一。动画帧只 `notify` 页面，不走 `revise`。重放视图观察代际，不一致才 `notify` 自己。重放视图在页面的 `render` 里创建，这一帧页面已经被租走，构造函数接收页面上已经拿到的代际。重放视图的根写 `size_full`，铺满这块视图的边界；单独的 `flex_1` 在视图根上高度是 0，滚动容器会把卡片裁没。系统开启减少动态效果时，弹簧和 `with_animation` 直接落到终态，不要再自己调度帧。

回归看 `shell/dialog_motion.rs`：旁边有循环动画时，重视图的绘制次数不跟着帧数涨。根因和复发见 [errors](../../errors.md)。

## 技能卡

技能、市场、卡组各画自己的卡片内容（`my_skills/skill_card/`、`marketplace/market_card.rs`、`skill_cards/group_card.rs`）。窗口、技能卡和额度卡的宽度在 `layout.rs`；技能卡外框经 `skill_card/size.rs` 转出。外框只有 `skill_card/shell.rs` 的 `card_shell`：圆角和阴影按种类收在这里（技能卡 16px 带阴影，市场卡圆角 xl 带阴影，卡组圆角 xl 不带阴影），页面只传入选中与否。加载占位走 `card_placeholder`，不另画边框和底色。

三张卡共用市场卡原来的蓝光悬停：边框变成强调色，底色换成 `card_hover`。打开详情、批量勾选、市场卡被点开、卡组展开仍是强调色边框，底色用 `card_active`；这时再悬停只把底色加深到 `card_active_hover`，边框保持强调色。

技能网格、市场榜单、发布者详情和账号额度卡共用 `skill_card/grid.rs` 的轨道：列数按卡片宽和间距从面板宽度算出；技能卡保持 `SKILL_CARD_W`，额度卡保持 `QUOTA_CARD_W`，都不随窗口变宽；末行用占位保持和完整行相同的卡宽。详情列打开时，先从面板内容宽度里减去列宽再算列数。主面板左右边框算在内容宽度之外，最右一张卡的边框和阴影留在滚动区域里面。技能页的列表模式不走这套轨道，每张卡占内容区一整行，见 [Skills](../skills/README.md)。技能页本地列表和市场榜按行虚拟化，只构建视口内的行。技能、市场和卡组工具栏右侧的网格/列表图标悬停分别是「网格」和「列表」。卡组用技能卡外框，自己换行，不走这套网格。额度卡的图例面在 `accounts/frame.rs`，不走技能卡外框。排布见 [Accounts](../accounts/README.md)。

技能网格卡片只放身份、一条决策证据、一个主动作和例外状态。库内已安装、运输类型、runtime、版本和仓库链接不重复画在卡片上。界面语言为中文时，这条描述若是英文，换成已缓存的译文并套用设置里的译文样式；译文还没回来时仍显示原文。

来源 chip 只有 `skill_card/source_chip.rs`：仓库路径优先，作者 handle 兜底。点击先 `stop_propagation`，再由调用方打开链接。

技能卡正文保持箭头光标。底栏 Agent 轮播的每一枚品牌 SVG 都是独立点击目标，指针停在这枚 SVG 上时改用手型光标。

技能卡右上角正在更新时，文案取 `common.updating` 去掉末尾省略号，再按 `.`、`..`、`...` 循环。三个点的位置一直留着，徽标宽度不变。系统开启减少动态效果时停在三个点，不调度动画帧。工具栏「更新」和选择栏批量更新按点击时的可更新名单让这些卡进入同一状态；`busy` 在这两条路径上是批次记号，不是技能名。

## 交互

- 详情读取绑定当前所选资源，只采纳这次选择的结果。
- 长任务可以取消。失败要显示原因。
- 设置是跨模式入口：打开设置页时保留当前 Skills / Accounts 上下文。设置按钮的选中态只表示当前是设置页。宽窗口下分区导航在内容列左侧；导航与侧栏的间隙等于导航与分区内容的间隙，内容列不随导航移动。
- 后端解析的路径直接展示。可编辑 Agent 路径显示平台分隔符；持久化的 `project_skills_rel` 仍规范为 `/`。
- 次级文本和 disabled 用独立的前景色，读得出来。
- 浮在可悬停内容上的菜单、胶囊、回到顶部、对话框遮罩和弹出列表面板要挡住后面的指针。普通命中盒不会挡住后面的元素，指针在浮层上时背后的卡片仍会进入悬停。这些浮层调用 `occlude()`。贴在可滚动列表上、滚轮仍应滚动列表的小控件（选择胶囊、回到顶部）调用 `block_mouse_except_scroll()`。Kit 的 Popover、菜单和对话框已经挡住。
- 设置里拉取的远端清单（如翻译 LLM 的模型列表）只填充下拉，不把整份列表内联铺进设置卡片；选择经下拉菜单完成，当前值不在清单中时保留为显式首项。
- 本机 Agent 的注册、启用和 rail 可见性见 [Skills](../skills/README.md#agent-注册手动启用与项目检测)。界面按 `enabled` 投影。品牌图标在 `agent_icons.rs`。

当前壳没有托盘、深链、签名更新、命令面板、SSH 远端页和共享频道管理页。补页面时放进已有能力目录。运行侧的缺口见 [architecture](../../architecture.md#壳运行模型) 和 [platform](../platform/README.md)。

## 验证

```bash
cargo test -p ss-gpui --lib
```

新增或修改文案时同步 `crates/ss-gpui/assets/locales/en.json` 与 `zh-CN.json`。

## 文案术语

同一概念只用一个名词和一个动词。

| 概念 | EN | ZH |
| --- | --- | --- |
| 已安装或可安装的技能单元 | Card | 技能 |
| 技能分组 | Deck | 卡组 |
| 技能目录 | Marketplace | 市场 |
| Agent CLI / 桌面端 | Agent | 智能体 |
| 模型供应商 | Provider | 供应商 |
| 用量账号 | Subscription | 订阅 |
| GitHub 共享频道 | Channel | 频道 |
| 本机技能库 | Hub | Hub |
| 从库中移除技能 | Uninstall | 卸载 |
| 销毁卡组 / 订阅 / 供应商 / 项目登记 | Delete | 删除 |
| 写入项目 | Deploy | 部署 |
| 对某个 Agent 启用 | Link | 链接 |

文件格式、协议和仓库路径仍用 `SKILL.md`、`skills/`。破坏性按钮写出对象和后果。错误先说发生了什么，再说怎么恢复。

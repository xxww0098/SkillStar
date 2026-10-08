# 界面约定

状态：active

本文件只记 SkillStar 桌面壳自己的视觉与交互契约。目录和依赖见 [boundaries 的 GPUI 壳模块](../../boundaries.md#gpui-壳模块)。通用 GPUI Kit 界面判断以 [Design Guides](https://gpui-kit.com/docs/design-guides.md) 为准，这里不复述。

各页的产品行为留在相邻功能文档。技能页见 [Skills](../skills/README.md)，市场见 [Marketplace](../marketplace/README.md)，账号布局见 [Accounts](../accounts/README.md)。

## 壳

- 界面在 `crates/ss-gpui`。配色在 `src/theme.rs`，应用文案在 `assets/locales/`。`en` 与 `zh-CN` 同步。Kit 组件自带的字符串按 [I18n](https://gpui-kit.com/docs/i18n/) 用 `locales/ui.yml` 覆盖；应用代码不直接查 `gpui_component.*`。
- 技能模式侧栏四项的图标在 `NavPage::icon`：技能 `Sparkle`，市场 `Store`，卡组 `GalleryVerticalEnd`（卡面在下，后卡边从下往上露出），项目 `Briefcase`。收起后只剩图标，四枚轮廓必须能分开。卡组空状态、项目空状态，以及技能空状态里去市场的按钮，用同一枚。
- 这四项共用一块选中框。切换时框滑到新的一项，文字和图标颜色跟着走。打开设置或发布者详情时框在原位淡出，四项都不再显示选中。系统开启减少动态效果时框直接落在目标项上。
- 页面在 `render` 里读 `theme::palette()`。新状态色先在 `Palette` 加字段，并配齐 `DARK` 与 `LIGHT`。`gui_prefs.json` 的 `background_style`（`"current"` 深、`"paper"` 浅）是主题持久化的唯一来源。侧栏主题按钮和 Settings › Appearance 都走 `theme::set_mode` / `toggle`。
- 提示遮罩用 `theme::prompt_veil`，后面的页面保持可见。Kit 对话框读 `ThemeColor.overlay`，页内提示走 `chrome::prompt_mask`。
- 悬停提示底色是黑色，文字是白色，两种模式一样，画在 `chrome::tooltip`。菜单、弹层和搜索用卡片色：它们和提示共用 kit 的 popover 色，改那个字段会把这些一起涂黑。按钮的 `tooltip(字符串)` 不经过这个函数，提示挂在元素自己的 `.tooltip` 上。富内容提示（重置卡的过期时间列表）也走 kit：`Tooltip::element` 喂行内容，外面用 `Styled` refine 成卡片色浮层，定位交给 tooltip overlay，不用页面状态和前置层。kit 组件内置的 `.tooltip(text)`（如 `Switch`）没有 builder 通道，保持 kit 默认 popover 色，归入卡片色一类。
- 操作结果走通知带（`notify.rs` 的 `NoticeBoard`），不再用 kit 的 Notification 浮层：页面调 `notify::toast(Notice)`，Shell 把告警画成 kit `Alert`（info/success/warning/error 四色，`small` 尺寸，带关闭按钮）停靠在主面板底部内容下方，占布局位、不遮内容。未钉住的 5 秒后自动收走；钉住的（`.pinned()`）只能手动关闭；带 `.action(label, run)` 的自动钉住并在 Alert 右侧画一枚 kit `Button`。同 `replace_key` 的通知互相顶替（家族语义），无 key 自由堆叠，同时最多 4 条、挤掉最旧未钉住的。通知带在通知增减时重画整个壳场景，频率低，不参与 120 帧动画通道。
- 控件优先用 gpui-component：设置开关是 kit `Switch`；互斥的分段胶囊切换分两类——滑动滑块与原地变色。滑块式走 `chrome::segmented`（`slider_segmented`）：等宽槽位、一枚绝对定位的 thumb（`card` 底、描边、`shadow_sm`）用 `motion_spring` 在槽间滑动，槽位透明、文字/图标随弹簧交叉淡化（图标与文字都可选，纯文字槽位如设置页译文语言五行），`reduce_motion` 直接落位；侧边栏模式切换（含收起后的纵向形态，经 `slider_segmented_at` 挂在自己的弹簧闭包里）、技能页 scope 三项、设置页译文语言五行（72px 槽位容最宽 endonym，`id` 直接用语言 code）共用这一份代码，轨道整体 `occlude()`。原地变色式是 kit `TabBar` segmented（每个 `Tab` 设 `flex_1` 均分，全宽均分只有 origin 菜单一处；图标 Tab 自己挂 `.tooltip`，悬停文案仍是「网格」和「列表」）。选择框是 kit `Checkbox`（嵌在可点行里的实例，点击回调先 `stop_propagation`，避免行和框双触发）；空态是 kit `Empty`（媒体圆片作为自定义 child 喂给 media 槽，动作按钮走 kit `Button`）；导入弹窗的状态 chip 与计数 pill（`phases::badge` / `count_pill`）是 kit `Tag::custom` 软底配色的薄包装。保留手绘的有明确理由：骨架 `pulse`（kit `Skeleton` 缺 `repeat_synced` 与 20 帧采样，换掉即回退跨条同步与省电）；技能页 agent 品牌筛选条带（固定六槽、点击已选项清除，不是互斥分段）；导入行的问题 pill（提示挂在 tooltip 上，kit `Tag` 不是交互元素）；卡内计数徽章与成员 chips（差异化卡面的一部分）。
- 刷新图标只在该次请求进行中旋转，旋转是 kit `Spinner` 的字形绘制变换（经 `chrome::icon_spin`，0.8s 一圈），静态时退回原字形。技能网格、市场榜、发布者列表和账号额度区是重放视图：这些帧重绘图标和顶栏，不重新排版后面的卡片。骨架脉冲和重置卡闪烁仍是 20 帧。更新省略号大约每 400ms 一步。单张额度卡自己的刷新，以及重置卡的弹簧和闪烁，仍会重排额度区，因为额度卡高度不固定，不能按张缓存。
- 悬停和按下在指针状态变化时重绘一次。颜色不走弹簧：弹簧每帧重绘整个窗口，鼠标划过卡片时会一直掉帧。详情列的「卸载」是唯一例外，见 [动画](#动画)。模式胶囊、技能侧栏选中框和重置卡的位移仍用临界阻尼弹簧，大约 80ms 内停住。
- 顶栏空白可以拖动窗口。拖拽层铺在整行背后；按钮、分段轨道和搜索框调用 `occlude()`，按下它们不会拖动窗口。`overflow` 包装的弹性子项写 `w_auto()`，中间的空白才留得出来。
- 顶栏命令区（搜索框、筛选组、中间空白、操作组）在 `PageBar::build` 里包进一个 kit `Toolbar`：控件全走 `content` 通道保持自己的像素，bar 层提供 ARIA toolbar 角色和左右方向键在可聚焦控件间的轮转（搜索框保留光标行为）。筛选和操作各是一个 `ToolbarGroup`，无障碍标签是 `toolbar.filtersGroup` / `toolbar.actionsGroup`。各页 `drag_id` 唯一，Toolbar 的焦点状态以它派生。
- 顶栏搜索框点到框外会失焦，框内没有菜单和补全时按 Escape 也会失焦。这次失焦不拦截鼠标按下，空白处仍能拖动窗口。
- 确认类弹窗（消息确认、注册项目、改路径、卡组导入、重置额度）都走 kit 的 AlertDialog（`chrome::open_confirm` / `chrome::open_form_dialog` 是薄封装）：kit 自己画标题、说明或表单字段和居中排布的「取消」与确认按钮，卡片停在视口上方十分之一处（kit 契约），入场是落点上方 8px 的原地弹出，与 popover 同一语言，[D-101](../../decisions.md#d-101gpui-component-以-vendored-patch-引入悬浮窗入场统一为原地弹出)。按钮种类见 [platform](../platform/README.md)。自带按钮体系的自定义弹窗（导入框、分享、阅读器、新建卡组、登录）仍由 `chrome/dialog.rs` 量高后放到窗口正中。壳和主面板里的页面都是重放视图，这段动画不会把侧栏和后面的卡片网格每帧重排。

## 动画

120 帧是显示器的刷新信号：这一帧的场景要在约 8ms 内交出去。GPUI 没有合成器补间。`div` 的位置会改布局边界；视图缓存命中时只按原坐标重放上一帧场景，不会把位移交给 GPU。`with_max_fps(120)` 改走定时器，比刷新信号更漂，不要用来追 120 帧。

新动画先选一条通道：

- 透明度、颜色、字形旋转走 `with_animation`，不设 `with_max_fps`。加载旋转统一用 `icon_spin`（`chrome/mod.rs`）：kit `Spinner` 的薄包装，旋转是字形上的 `Transformation::rotate`。
- 位置必须改布局时（选中框、对话框入场）把时长压短。位移弹簧用 `motion_spring`，临界阻尼，大约 80ms 内停。不动的重子树用 `replay_view`，或带确定宽高的 `.cached()`。对话框入场会改子树的布局边界，缓存键里有这块边界，滑动期间重放命中不了。整份 Markdown 要等滑动停再挂上，SKILL.md 悬浮窗就是这样。
- 呼吸和省略号不需要每帧一个新样子。脉冲用 `pulse`（20 帧）。省略号大约 400ms 一步，继续 `with_max_fps`。
- 详情列底部的「卸载」悬停时，底色从 `danger_bg` 走到 `danger_hover`，边框走向 `danger`，文字走向 `danger_fg`，垃圾桶图标轻轻抬起、放大并转动。进度用 `motion_spring`，大约 80ms 内停，指针离开时从当前进度返回。系统开启减少动态效果时直接落到终态。这是唯一走弹簧的悬停色，不要抄到技能卡或其它按钮上。卡片仍按张缓存，这次重绘不重新排版网格，也不走 `revise`。

`with_animation` 和 `with_spring` 标脏的是正在绘制的那个视图，祖先一起标脏，子孙和兄弟不会。重内容必须是这个视图的兄弟或子孙，并且自己是实体。整块区域用 `replay_view`：父级 `relative`，子视图绝对定位并铺满。固定宽高的卡片按张 `.cached()`，不要写成 `absolute().size_full()`，否则会盖住网格。不要在 `render` 里 `cx.new` 动画实体，每帧都会把动画重开。

数据变化走页面的 `revise`，把代际加一。动画帧只 `notify` 页面，不走 `revise`。重放视图观察代际，不一致才 `notify` 自己。重放视图在页面的 `render` 里创建，这一帧页面已经被租走，构造函数接收页面上已经拿到的代际。重放视图的根写 `size_full`，铺满这块视图的边界；单独的 `flex_1` 在视图根上高度是 0，滚动容器会把卡片裁没。系统开启减少动态效果时，弹簧和 `with_animation` 直接落到终态，不要再自己调度帧。

回归看 `shell/dialog_motion.rs`：旁边有循环动画时，重视图的绘制次数不跟着帧数涨。根因和复发见 [errors](../../errors.md)。

## 技能卡

技能、市场、卡组各画自己的卡片内容（`my_skills/skill_card/`、`marketplace/market_card.rs`、`skill_cards/group_card.rs`）。窗口、技能卡和额度卡的宽度在 `layout.rs`；技能卡外框经 `skill_card/size.rs` 转出。外框只有 `skill_card/shell.rs` 的 `card_shell`：圆角和阴影按种类收在这里（技能卡 16px 带阴影，市场卡圆角 xl 带阴影，卡组圆角 xl 不带阴影），页面只传入选中与否。加载占位走 `card_placeholder`，不另画边框和底色。底栏 Agent 轮播只有 `skill_card/agent_rail.rs` 一份，配 `agent_footer_bar` 的 42px 底栏：技能卡和卡组卡共用同一排品牌图标槽（可上链接的 agent 才占槽，见 `targetable_agent_profiles`），技能卡左侧带星标，卡组卡左侧带一键部署；槽的点击语义由页面闭包给出（技能卡安装/解绑/挂链，卡组卡切换整组链接）。

卡组卡与技能卡同骨架：其他卡渲染描述的位置，卡组卡渲染成员技能名字 chips（收起 4 枚加「+N more」，展开全部、正文区可滚）；右上角一组是 `N skills` 计数徽章、分享、复制、删除，徽章占复制按钮左侧的位置；缺装警告留在正文顶部。分享按钮打开 `skill_cards/share_sheet.rs` 的对话框，两条出路：分享码（`encode_share_code` 生成 `agd-` 码，后台读 hub git remote 兜底 `skill_sources`，本地成员计数提示改用打包，复制走系统剪贴板并以底部通知带确认）与打包（`export_deck_bundle` 导出 `.agd` 安装包到下载目录——远程成员只记来源链接、仅本地成员打包内容，成功后显示路径并可打开所在文件夹；接收方经工具栏「导入」或「文件」入口安装，链接成员与直接用链接安装等价）。

三张卡共用市场卡原来的蓝光悬停：边框变成强调色，底色换成 `card_hover`。打开详情、批量勾选、市场卡被点开、卡组展开仍是强调色边框，底色用 `card_active`；这时再悬停只把底色加深到 `card_active_hover`，边框保持强调色。

技能网格、市场榜单、发布者详情和账号额度卡共用 `skill_card/grid.rs` 的轨道：列数按卡片宽和间距从面板宽度算出；技能卡保持 `SKILL_CARD_W`，额度卡保持 `QUOTA_CARD_W`，都不随窗口变宽；末行用占位保持和完整行相同的卡宽。详情列打开时，先从面板内容宽度里减去列宽再算列数；列宽是一列卡加一档列距（`DETAIL_COLUMN_W`），打开正好少一列。默认窗口宽度（`layout.rs` 的 `WINDOW_W`）按壳层、面板边框和共用页边距推出正好一行三列：关详情列三列整、开详情列两列整，都不多不少；这条关系由 `layout.rs` 的常量断言和 `skill_card/grid.rs`、市场详情列的测试锁住，改任何一环都会编不过或测红。主面板左右边框算在内容宽度之外，最右一张卡的边框留在滚动区域里面。技能页的列表模式不走这套轨道，每张卡占内容区一整行，见 [Skills](../skills/README.md)。技能页本地列表和市场榜按行虚拟化，只构建视口内的行。技能、市场和卡组工具栏右侧的网格/列表图标悬停分别是「网格」和「列表」。卡组用技能卡外框，自己换行，不走这套网格。额度卡的图例面在 `accounts/frame.rs`，不走技能卡外框。排布见 [Accounts](../accounts/README.md)。

技能网格卡片只放身份、一条决策证据、一个主动作和例外状态。库内已安装、运输类型、runtime、版本和仓库链接不重复画在卡片上。界面语言为中文时，这条描述若是英文，换成已缓存的译文；译文还没回来时仍显示原文。描述译文不套用设置里的译文样式，按普通文字绘制；译文样式只画在 SKILL.md 阅读器的译文行上，见 [Skills](../skills/README.md)。

来源 chip 只有 `skill_card/source_chip.rs`：仓库路径优先，作者 handle 兜底。点击先 `stop_propagation`，再由调用方打开链接。

技能卡和卡组卡正文保持箭头光标。底栏 Agent 轮播的每一枚品牌 SVG 都是独立点击目标，指针停在这枚 SVG 上时改用手型光标。

技能卡右上角正在更新时，文案取 `common.updating` 去掉末尾省略号，再按 `.`、`..`、`...` 循环。三个点的位置一直留着，徽标宽度不变。系统开启减少动态效果时停在三个点，不调度动画帧。工具栏「更新」和选择栏批量更新按点击时的可更新名单让这些卡进入同一状态；`busy` 在这两条路径上是批次记号，不是技能名。

## 交互

- 详情读取绑定当前所选资源，只采纳这次选择的结果。
- 长任务可以取消。失败要显示原因。
- 设置是跨模式入口：打开设置页时保留当前 Skills / Accounts 上下文。设置按钮的选中态只表示当前是设置页。宽窗口下分区导航在内容列左侧；导航与侧栏的间隙等于导航与分区内容的间隙，内容列不随导航移动。
- 设置页的控件按语义取色：开关是 kit `Switch`，关态轨道与滑块色由 theme 投影给出（关态 `edge`——`border` 在卡片上不可见；滑块 `on_accent`，两主题都是白）；例行维护（清理缓存）用中性描边按钮，两步确认的强制删除才用 danger；分区内唯一主动作（开始诊断）用 accent 描边；列表展开脚手（展开其余智能体）用 accent 文案。
- 后端解析的路径直接展示。可编辑 Agent 路径显示平台分隔符；持久化的 `project_skills_rel` 仍规范为 `/`。
- 次级文本和 disabled 用独立的前景色，读得出来。
- 浮在可悬停内容上的菜单、胶囊、回到顶部、对话框遮罩和弹出列表面板要挡住后面的指针。普通命中盒不会挡住后面的元素，指针在浮层上时背后的卡片仍会进入悬停。这些浮层调用 `occlude()`。贴在可滚动列表上、滚轮仍应滚动列表的小控件（选择胶囊、回到顶部）调用 `block_mouse_except_scroll()`。Kit 的 Popover、菜单和对话框已经挡住。
- 滚动盒（`overflow_y_scrollbar` 等滚动包装）的外层用明确像素高度的视口框，行数超过阈值才滚动，少时高度跟内容走；不要给滚动包装写 `max_h` 或把它撑成 `flex_1`。滚动包装把调用方的高度限制留在内容节点上，内容被截到和视口一样高，滚轮位移夹回 0——看见的裁切来自外层，不是有溢出的滚动盒。范例是 `my_skills/import_modal/phases.rs` 的最近仓库列表和 `skill_cards/create_group.rs` 的成员选择列表。
- 设置里拉取的远端清单（如翻译 LLM 的模型列表）只填充下拉，不把整份列表内联铺进设置卡片；选择经下拉菜单完成，当前值不在清单中时保留为显式首项。模型下拉是 kit `Combobox`（`searchable`）：面板里可键词过滤，面板高度封顶约 280px、超出滚轮滚动；面板底部常显清单总数，拉取后设置卡片的提示行也报出数量。清单与当前值在渲染期同步进 `ComboboxState`（拉取回调拿不到 `Window`），同步键见 `sync_llm_model_state`。
- 间距只写整数步 helper（`p_2`、`gap_1`、`gap_4`…）。`*_1_5`、`*_2_5`、`*_0_5` 在本版 gpui 里不是半步语义，单个值会膨胀到 ~100–300px，禁止使用；事故与自检见 [errors](../../errors.md)。
- 项目详情页（`projects/`）：主任务「按智能体管理技能」卡在最上，部署方式卡只做每行模式胶囊的静态图例（当前模式看行内胶囊，不做汇总徽章），检测规则卡垫底。agent 行的部署路径在右侧安静展示为 mono 文本，不做输入框样式的底框。行内技能徽章、卡片角标与底部统计都走 locales；「保存并同步项目」在没有未保存更改时静默描边且不可点。
- 本机 Agent 的注册、启用和 rail 可见性见 [Skills](../skills/README.md#agent-注册手动启用与项目检测)。界面按 `enabled` 投影。品牌图标在 `agent_icons.rs`。

当前壳没有托盘、深链、签名更新、命令面板、SSH 远端页和共享频道管理页。补页面时放进已有能力目录。运行侧的缺口见 [architecture](../../architecture.md#壳运行模型) 和 [platform](../platform/README.md)。

## 验证

```bash
cargo test -p ss-gpui --lib
```

新增或修改应用文案时同步 `crates/ss-gpui/assets/locales/en.json` 与 `zh-CN.json`。要改输入框菜单、下拉占位这类 Kit 组件上的字，改 `crates/ss-gpui/locales/ui.yml` 里同名 key。

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

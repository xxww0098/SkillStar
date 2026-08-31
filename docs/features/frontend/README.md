# Frontend 约定

状态：active

本文件是 React 前端的共享边界、交互范式和视觉约定。具体 Models、Usage、Skills 等产品行为由相邻功能文档维护；完整目录树见 [../../boundaries.md](../../boundaries.md)。

## 结构与数据流

- `src/pages/*.tsx` 是薄路由壳；页面目录目前只有 `src/pages/settings/`，不要在文档虚构不存在的 page 子目录。
- `src/pages/Settings.tsx` 只组合各域公开的 settings section；section 及其数据 hooks 仍归 Models、S3、Usage 等所属 feature，不在 `features/settings` 建跨域实现副本。
- 产品实现放 `src/features/<domain>/`，内部 `api/`、`hooks/`、`lib/`、`components/` 默认私有。
- 后端已归属某域的子系统在前端也作为该域子模块组织，不建立仅为目录对称而存在的平级 feature。
- 跨 feature 复用的无业务展示组件放 `src/components/shared/`；原子 UI 放 `src/components/ui/`；纯工具放 `src/lib/`。
- 前端业务数据只通过 `src/lib/ipc/` 中的 wrapper 调 Tauri `invoke()`，不得直接访问业务文件或远程 API。
- 服务端状态使用 TanStack Query；本地组合状态使用 React hooks。没有经过决策记录，不引入另一个全局状态库。
- 真正跨页的 deploy/detail/navigation 状态由 `App.tsx` 统一持有。

`scripts/internal/check_feature_imports.sh` 阻止新增跨 feature 深层导入，但允许从目标 feature 根 `index.ts` 导入。存量基线只能减少；跨域协作优先由 page 组合，确需依赖时只消费目标 feature 的公开入口；若组件确实无业务语义且通用，应先提升到 shared/lib，再改调用方。

## Tauri 事件与流式 UX

- 生命周期型订阅使用 `src/hooks/useTauriEvent`，由它处理 `listen()` promise 与卸载 cleanup 的竞争。
- 单次请求流可以由对应 hook 管理监听，但必须处理 start、delta、complete、error 和中断。
- AI 摘要展示后端返回的 route/provider/fallback 元数据，不在前端猜测路由。
- Skill 教程是后端持久化状态；前端不计算目录 hash、不缓存另一份 HTML，也不渲染 ACP 的半成品输出。fresh artifact 直接打开，stale artifact 同时提供旧版查看与更新动作。
- 模型生成 HTML 只能放入不带 `allow-scripts`/`allow-same-origin` 的 sandbox iframe；禁止用 `dangerouslySetInnerHTML` 注入应用 DOM，也不直接交给系统浏览器执行。
- 安全扫描应区分文件准备和 AI chunk 进度，不能压成一个模糊 spinner。

## 视觉系统

精确 token 在 `src/index.css` 和 Tailwind theme 中维护，本文只维护不变量：

- 默认视觉是深色精密玻璃界面；卡片可以半透明，正文密集的 modal 必须使用 `.modal-surface` / `.modal-surface-subtle` 的近实色表面，避免退回低对比度 `bg-card/95`。
- 浅色由应用内的 `data-bg-style="paper"` 驱动，与系统色彩偏好无关。需要随主题换色的状态色写 `text-amber-400 paper:text-amber-700` 这类 `paper:` variant（`@custom-variant paper`），不要用 `dark:`——它匹配的是系统偏好，在应用内主题切换时不生效。
- 小字号次级文本优先使用有足够对比度的 `text-foreground/60–75`。disabled 状态不能只靠 `opacity-50`。
- 大容器使用统一大圆角尺度；紧凑控件使用较小尺度。
- 动效只表达进入、退出、层级和直接反馈；尊重 `prefers-reduced-motion`。
- 所有正文、焦点、禁用和错误状态满足 WCAG AA。
- 模型与核心行为字段提供 `InfoTip`；枚举选项说明使用可解析的 `Label: explanation` 行，不在每个表单重造提示样式。

产品视觉方向是 Precise、Unified、Effortless；避免纯装饰 dashboard、过度霓虹、低对比度 glass 和无意义 motion。

## 组件约定

- 样式使用 Tailwind utilities；不新增 CSS Modules 或 styled-components。
- 优先复用 `src/components/ui/`。需要焦点管理、Esc、portal 的组件使用 Radix primitive。
- 居中 modal 使用 `ModalShell`、`ModalHeader`、`ModalCloseButton`；Radix `AlertDialog` 和确有独特 surface 的对话框除外。
- 抽屉使用 `DrawerShell`，不要各自实现 overlay、Esc 和 focus 行为。
- 外链元素使用 `ExternalAnchor`；按钮/程序化跳转使用 `openExternalUrl`，避免业务页面直接写 `<a target="_blank">`。
- Marketplace 与 MCP 共用的 Publisher avatar 是无业务语义的展示 module，归 `src/components/shared/PublisherAvatar.tsx`；两个 feature 都只能依赖该 shared interface。
- 动态颜色无法用 utility 表达时才使用 inline style。
- 侧边栏导航的选中态由带 `layoutId` 的 motion 元素承载，切换时弹簧滑动；收起态改为静态高亮，不做滑动。新增导航区沿用这条约定，不要再写第三种选中态实现。
- Skill / MCP 网格卡片只承载身份、一条决策证据、一个主动作和例外状态。库内已安装、运输类型文字、runtime、版本、仓库链接和「详情」不重复画在卡片上；这些信息留在筛选、图标、详情抽屉或安装向导。
- 卡片列表（`.ss-cards-grid` / `.ss-cards-list`）第 13 项起由 CSS `content-visibility: auto` 跳过屏幕外的样式、布局和绘制；卡片高度由内容决定，`SkillGrid` 量出首张卡片写入 `--ss-card-h` 供 `contain-intrinsic-size` 占位。新增卡片列表沿用这两个类，不要自己写 JS 虚拟滚动。

## Agent 手动激活投影

- 本机 Agent 的注册、手动启用和 rail 可见性规则由 [Skills 行为文档](../skills/README.md#agent-注册手动启用与项目检测) 维护；前端统一通过 `selectTargetableAgentProfiles` 按 `enabled` 投影，并由共享 rail 再次过滤。前端不得根据 binary、应用、目录或冻结兼容字段 `installed` 推断可用性。
- 内置 Agent 的品牌图标统一通过 `src/components/ui/icons/agentIcons.ts` 投影到
  `@lobehub/icons`；deep import 只允许出现在 `icons/lobe.ts`。包内无专属品牌时使用
  `LobeHubMono`，不得为同步上游清单批量复制本地 SVG。
- 项目级能力再通过 `supportsProjectDeploy` 判断；不要硬编码 global-only Agent id。
- 全局能力通过后端 `has_global_skills()` 判断；空全局路径是“不支持全局部署”的能力标记，
  不能被解释为当前工作目录。
- Skills、Deck 和 MCP 的 Agent rail 复用 `AgentTargetCarousel`，图标和名称来自 `AgentProfile`。传给 Skill 卡的 `onInstall` 必须接受并转发 `(url, name, agentId?)`；只接 `url` 会让已安装卡的灰图标点了没反应。未隐藏的箭头才 `pointer-events: auto`，不要把箭头叠在图标上。
- MCP 等能力消费者可以叠加静态能力映射，但不得再用本机安装探测隐藏用户已手动启用的 Agent；执行时的真实失败由对应 mutation 显式反馈。
- Claude Settings profile `claude` 映射到唯一能力 id `claude-code`；不要生成第二张 Claude 卡。
- SSH 远端 Agent 由远端 discovery 决定，不复用本机 rail。

## 桌面交互

- destructive action 使用明确确认组件，不调用浏览器 `confirm()`。
- 后端解析的路径直接展示；不要在浏览器重建数据目录。可编辑 Agent 路径显示平台分隔符，持久化的 `project_skills_rel` 仍规范为 `/`。
- tray 与 Settings 的后台运行开关消费同一状态和事件；动作标签必须反映 Start/Stop 当前状态。
- GitHub 账户是全局身份，不是一条设置项：登录入口常驻侧边栏底部工具区（设置/背景/收起之上），展示当前账户与状态，点击打开设备授权面板。需要登录的界面调用 `openGithubAccountMenu()` 打开同一面板，不再跳转 Settings section。入口与面板共享同一个 `useGitHubAuth` 实例，避免两份独立轮询的登录状态。
- Marketplace、Models、Usage 等跨页面 request 使用带 nonce 的显式导航事件，避免用不可观察的模块变量传递。

## 生成类型

`src/types/generated/` 由 Rust 的 ts-rs 生成，禁止手改。修改来源结构体后执行：

```bash
bun run types:gen
```

当前生成来源位于 `skillstar-models`、`skillstar-marketplace` 和 `src-tauri` package。CI 的 generated-types 检查负责发现漂移；精确类型清单以 Rust `#[derive(TS)]` 和生成脚本为准。

## 验证

```bash
bun run lint
bun run build
bun run test
bash scripts/internal/check_feature_imports.sh
bash scripts/internal/check_i18n_hardcoded.sh
bash scripts/internal/check_ts_orphan_modules.sh
```

新增或修改文案时同步 `src/i18n/locales/en.json` 与 `zh-CN.json`。

## 文案术语

同一概念只用一个名词和动词。界面不靠文学同义替换换词。

| 概念 | EN | ZH |
| --- | --- | --- |
| 已安装或可安装的技能单元 | Card | 技能 |
| 技能分组 | Deck | 卡组 |
| 技能目录 | Marketplace | 市场 |
| MCP 目录 | Catalog | 目录 |
| Agent CLI / 桌面端 | Agent | 智能体 |
| 模型供应商 | Provider | 供应商 |
| 用量账号 | Subscription | 订阅 |
| GitHub 共享频道 | Channel | 频道 |
| 本机技能库 | Hub | Hub |
| 从库中移除技能 | Uninstall | 卸载 |
| 销毁卡组 / 订阅 / 供应商 / 项目登记 | Delete | 删除 |
| 写入项目 | Deploy | 部署 |
| 对某个 Agent 启用 | Link | 链接 |

文件格式、协议和仓库路径仍用 `SKILL.md`、`skills/`。破坏性按钮写出对象和后果，不用 Yes / OK / Confirm。错误先说发生了什么，再说怎么恢复。

`check_ts_orphan_modules.sh` 从 `src/main.tsx` 与 `src/pages/` 出发做可达性分析，`src/features/` 下的孤儿文件直接失败。注意 `check_i18n_hardcoded.sh` 只判定 CJK 字面量，裸英文文案不在它的覆盖范围内，需要人工走查。

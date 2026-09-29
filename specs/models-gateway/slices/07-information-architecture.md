# 07 — 信息架构

## 契约

Models 页只有三栏：Agents、Providers、Gateway。原来的 Claude 专用工作台（`components/hub/claude/` 那条生产路径）从页面上消失。本档只评信息架构：人先看到这三栏，并知道每一栏在决定什么。栏内可以是空列表。不评行的密度、选择器、路由控件或空态文案。

## 缝

页面在 `src/features/models/`。数据经 Tauri `invoke` 到 `skillstar-app`，再进 `skillstar-gateway` 与现有的 provider 列表命令。新命令的 DTO 定义在 `skillstar-app`，不带明文密钥。新页面不调用 v3 flat bind，不读 `compat.rs`。

Settings 里的 App AI 和 Decision 模型不动。`get_providers_flat` 还在，只给 Settings。

`docs/features/models/README.md` 的「Models 工作台」改成这三栏。`bun run types:gen` 在 DTO 变化时运行，生成物提交。

## 人可以运行

启动桌面应用，打开 Models。三栏都在。切换到 Settings 的 App AI，原来的入口还在。

## 视觉

变量只有一个：三栏的信息架构。

裁剪：整页的栏标题和栏的主区域。忽略栏内第一行的具体文字。

本档不评：密钥是否掩码（26）、选择器（27）、次要字段（28）、最近请求（29）、路由控件（30）、空态句子（31）、颜色和品牌色。

接受前：

1. 跑 screenshot-critique。批评者只拿到 1440×900 与 1280×800 的截图和验收句，不拿实现说明。
2. 参照作物放 `specs/models-gateway/assets/magpie/07-ia.png`，从本机 magpie 的主列表截。有图就跑 compare-screenshots，裁剪只盖住栏的划分。截不到就在 `choices.md` 记下缺图，仍用 README 的五问。不从 `usemagpie.ai` 下载。
3. 用 preview-shots 打开截图，大约等 5 分钟。没有回复就按证据在 `choices.md` 写下一句决定和理由，关掉窗口，继续。

验收句：这一屏分成 Agents、Providers、Gateway 三栏；看不到 Claude 专用工作台，也看不到厂商密钥或厂商 URL。

## 验证

- 组件测试或页面探针：三栏标题存在，旧 hub 的根组件不被渲染。
- 浏览器或桌面里点过三栏，确认切换栏不会写 Agent 文件（临时 `HOME`）。
- Settings 的 App AI 仍能打开。

## 可改

栏的左右顺序以外的间距。顺序固定为 Agents、Providers、Gateway。

## 不可改

三栏、旧工作台离开生产路径、DTO 无明文密钥、不调用 `compat.rs`。

## 必须保持绿

`bun run lint` 与本页相关的前端测试。`cargo check -p skillstar-app --locked`。

## 会改这一档的反馈

截图里仍是 Claude 工作台，或三栏里有一栏在视觉上不成立。

## 决定

- 栏内的第一屏允许空。空态句子留到 31 档。

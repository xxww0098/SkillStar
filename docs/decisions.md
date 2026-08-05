# 架构决策记录

状态：active

这里只记录长期有效、影响多个改动的选择。当前结构以 [boundaries.md](./boundaries.md) 为准；实现行为以对应功能文档和代码为准。

## D-001：GUI 与 CLI 共享一个二进制和域实现

- 日期：2026-07-10
- 状态：accepted
- 背景：独立 CLI package 会复制入口、依赖和跨域流程。
- 决策：可执行文件只由 `src-tauri` package 产出；启动时识别 CLI/GUI 模式。CLI 和 Tauri command 调用同一域 facade 或 `skillstar-app` use case。
- 后果：`skillstar-app` 保持 library-only；不得重新添加第二个 `skillstar` binary。
- 证据：`src-tauri/src/main.rs`、`crates/skillstar-app/src/cli/`，提交 `77ed14c`。

## D-002：module-first，满足晋升条件后才拆 crate

- 日期：2026-07-10
- 状态：accepted
- 背景：“一个前端 feature 一个 crate”造成浅模块、编译扇出和双向依赖压力。
- 决策：新能力先进入最内聚的现有 crate，以私有 module + 窄 facade 暴露。只有独立变更节奏、依赖集合或 deletion test 证明收益时才建立新 crate。
- 后果：前端切片与 Rust crate 不要求一一对应；crate 数不是架构质量目标。
- 证据：Workspace Wave 1/2 迁移，提交 `77ed14c`、`871d0c6`、`3ea5f24`。

## D-003：命令层保持薄，跨域编排归 `skillstar-app`

- 日期：2026-07-10
- 状态：accepted
- 背景：把 use case 放在 `src-tauri/src/commands/` 会让 CLI 无法复用，也容易制造 `usage → models` 等反向依赖。
- 决策：command 只做框架适配；单域逻辑归域 crate；多域事务归 `skillstar-app`。
- 后果：新增 Tauri command 不代表新增业务实现；代码审查应检查 helper 和事务是否下沉到正确层。
- 证据：`crates/skillstar-app/src/usage_switch*`、`src-tauri/src/commands/usage_commands.rs`。

## D-004：Provider 元数据使用零依赖叶子

- 日期：2026-07-10
- 状态：accepted
- 背景：Models preset 与 Usage catalog 都需要 Provider identity/鉴权事实，但二者不能相互依赖。
- 决策：`skillstar-providers` 只保存 canonical identity、鉴权和余额端点元数据；Models 与 Usage 分别从它派生自己的产品注册表，并用测试锁定映射。
- 后果：添加 Provider 先修改 identity；不得在命令层或前端复制鉴权规则。
- 证据：`crates/skillstar-providers/src/`、Models/Usage guard tests。

## D-005：技能部署采用 link-first、copy fallback

- 日期：2026-06-10
- 状态：accepted
- 背景：symlink 能保持项目干净和自动跟随更新，但 Windows 权限或文件系统可能不允许。
- 决策：部署按 symlink → junction → copy 的能力阶梯执行；reconcile 和更新必须认识实际部署类型。
- 后果：文档和 UI 不能宣称“纯 symlink”；copy 需要 stale hash 刷新，失败不能破坏现有部署。
- 证据：`crates/skillstar-skills/src/deployment/`、提交 `7fde474`。

## D-006：文档按变化速率分层，并保持单一入口

- 日期：2026-07-14
- 状态：accepted
- 背景：AGENTS、CLAUDE、README、backend 和计划文档重复维护项目树与规则，且 `docs/` 曾被整体忽略。
- 决策：`AGENTS.md` 是唯一 Agent 规则入口，`CLAUDE.md` 仅委托；项目树归 `boundaries.md`，运行蓝图归 `architecture.md`，功能行为归 `docs/features/`，历史归 `docs/others/`。
- 后果：同一事实只在一个主文档维护；文档目录必须进入 Git；移动文档时同步修复索引与链接。
- 证据：2026-07-14 `/xxww-docs refactor` 审计与确认的迁移表。

## D-007：Skill 安装采用 universal project surface 与 Agent path ownership

- 日期：2026-07-14
- 状态：accepted
- 背景：逐 Agent 强制唯一项目路径会复制同一 Skill，也与 open agent skills 生态的 `.agents/skills` 共享约定不兼容；CLI 的 `--all`、通配、scope 与 copy 语义也不能只停留在参数外形。
- 决策：对兼容 Agent 使用共享 `.agents/skills` project surface；专属路径只保留给上游明确要求的 Agent。项目扫描按路径产生唯一或 ambiguous 结果，manifest 为共享路径选择单一 owner，部署/清理按路径去重。CLI `install/add` 对齐 `npx skills add` 的来源、通配、`--all`、scope 与 symlink/copy 语义；隐式目标改由 D-009 的手动激活状态提供。
- 后果：多个 Agent profile 可以映射到同一路径；任何 sync、scan、rebuild、remove 实现都不能假设 `project_skills_rel` 唯一。Global 与 Project 都从 SkillStar hub 部署，但写入各自真实目标并保留 SkillStar 的 lock/manifest 数据。
- 证据：`crates/skillstar-skills/src/agents/`、`crates/skillstar-skills/src/projects/`、`crates/skillstar-app/src/cli/` 及对应测试；上游设计参考 `vercel-labs/skills` 的 `add`、`agents`、`installer`。

## D-008：内置 Agent 以 vercel-labs 注册表为兼容基线，品牌图标统一投影

- 日期：2026-07-14
- 状态：accepted
- 背景：逐个手工接入 Agent 会让路径、能力和图标清单各自漂移；部分上游 Agent 只有项目级目录，不能伪造全局目标。
- 决策：`skillstar-skills` 的内置注册表同步 `vercel-labs/skills/src/agents.ts` 的 Agent 能力，并以测试锁定上游 id 覆盖；SkillStar 既有持久化 id 通过 CLI 兼容别名承接。内置品牌图标只通过 `@lobehub/icons` 的集中适配层渲染，无专属品牌时使用 LobeHub 通用图标。
- 后果：上游新增/修改 Agent 时必须在同一变更中同步路径、别名、图标映射与文档；项目级 Agent 会被全局操作明确拒绝。前端不维护第二份 SVG 资产目录，8 字段 `AgentProfile` IPC 保持稳定。
- 证据：`crates/skillstar-skills/src/agents/builtin.rs`、`src/components/ui/icons/agentIcons.ts` 及覆盖测试。

## D-009：本机 Agent 采用纯手动激活，不推断系统安装状态

- 日期：2026-07-14
- 状态：accepted
- 背景：binary、桌面应用、配置根和 skills 目录都不是可靠的 Agent 身份证据；尤其多个 Agent 共享 `~/.agents/skills` 时，部署残留会造成误发现、误启用和卡片 rail 泄漏。
- 决策：删除本机 Agent 安装探测及其注册表元数据。所有 profile 默认关闭，Settings 持久化开关是本机 Agent 激活的唯一来源；CLI 隐式目标与所有本机 rail 都只消费该状态。冻结 `AgentProfile.installed` 在兼容期镜像 `enabled`，不再承载安装事实。
- 后果：SkillStar 不会替用户判断 Agent 是否存在；用户可提前启用目标，实际部署或同步失败在动作边界显式返回。共享目录不再影响 profile 可见性，新增 Agent 也无需维护探测规则。
- 证据：`crates/skillstar-skills/src/agents/`、`src/lib/agentProfiles.ts`、Settings 与 rail 回归测试。

## D-010：Skill 教程使用 ACP 全目录快照与版本化 HTML artifact

- 日期：2026-07-14
- 状态：accepted
- 背景：只翻译 `SKILL.md` 无法解释 scripts、references、assets 等完整 Skill 行为，provider 翻译缓存也不能表达“教程是否仍对应当前目录版本”。模型输出 HTML 又不能直接进入应用 DOM。
- 决策：移除 SKILL.md 翻译功能。教程生成以 `skillstar-skills::content` 的完整递归快照和确定性内容 hash 为输入，通过用户显式配置的 ACP Agent 分析；后端只接受自包含、无脚本、覆盖全部文件清单的 HTML，并与 hash、教程风格、完整 prompt bundle hash/schema 版本 metadata 一起原子持久化。风格来自 Settings 中的受控注册表，每种风格使用独立 prompt 片段；前端用 sandbox iframe 展示。
- 后果：教程能覆盖整个 Skill 并跨重启复用；任何内容、规范化界面语言、所选风格或生成契约变化都会产生 stale 提醒，刷新失败仍保留旧版。生成成本和时延高于翻译，编辑器必须先保存，ACP 未启用时不能生成新教程。
- 证据：`crates/skillstar-skills/src/{content,tutorial}.rs`、`src-tauri/src/core/skill_tutorial.rs`、`src-tauri/prompts/acp/skill_tutorial.md`、Skill 教程面板回归测试。

## D-011：删除设备指纹功能，Usage 请求回归统一代理 client

- 日期：2026-07-23
- 状态：accepted
- 背景：设备指纹（TLS/HTTP2 伪装、浏览器 preset、IDE telemetry 投影、订阅级 fingerprint 绑定）横跨 usage crate、Tauri 命令、Settings 与订阅编辑四层，并通过 `impersonate` feature 引入 `wreq`/`wreq-util` 两个 rc 版依赖。它伪装的是客户端身份而非解决额度抓取本身的问题，收益不足以支撑这条贯穿全栈的接缝。
- 决策：整体删除 `skillstar-usage::fingerprint`、`src-tauri/src/commands/fingerprints.rs`、`src/features/usage/fingerprints/` 及 `Subscription.fingerprint_id`。原 `fingerprint::request` 的请求构建器保留为 `skillstar-usage::request`（去掉 wreq 分支），所有 fetcher 统一走 `http_client::usage_http_client()`。`impersonate` feature 与 wreq 依赖一并移除，`check_workspace_deps.sh` 中相关守卫同步删除。
- 后果：额度抓取以 reqwest 默认 ClientHello 出网，若某 provider 将来按 TLS 指纹拦截，需要另行决策而不是恢复本模块；`~/.skillstar/config/fingerprints.json` 成为孤儿文件，不再读写，也不做迁移删除。构建图少两个 rc 依赖，Usage 的请求路径只剩一条。
- 证据：`crates/skillstar-usage/src/request.rs`、`crates/skillstar-usage/src/http_client.rs`、`scripts/internal/check_workspace_deps.sh`。

## D-012：multi-provider 写盘骨架只覆盖 JSON 型 Agent，Codex 与 unsync 不进抽象

- 日期：2026-07-26
- 状态：accepted
- 背景：Codex/OpenCode/Pi 三个 multi-provider writer 的骨架（备份 → 读取/初始化 → retain 托管键 → 逐条写 `skillstar_*` 块 → active 指针 → 写盘）逐字同构，修一处写盘语义要改三处（spec #1 阶段三，票 #5）。
- 决策：把骨架下沉为 `tool_sync::multi_provider::sync_json_blocks_inner`（internal seam，不进公共出口），OpenCode 与 Pi 只保留 `build_block` 与指针落点两个 adapter。Codex 触发止损不进骨架：其 TOML 文档、`auth.json` 副通道和 per-entry wire settings 会让 adapter 接口超过被取代实现的复杂度。三份 unsync 各约 30 行且指针清理语义各异（同文件 selector / 双文件条件清理 / TOML+auth），抽象后逻辑被切碎，同样保持现状。
- 后果：JSON 型 multi Agent 的写盘语义修一处即全修，新 JSON 型 Agent 只写 build_block + 指针落点；Codex 的写盘语义变化仍需单独维护；未来若出现第二个 TOML 型 multi Agent，再评估 TOML 骨架（届时有两个 adapter 证明 seam）。
- 证据：`crates/skillstar-models/src/tool_sync/multi_provider.rs`（`sync_json_blocks_inner` 及两个调用方），`tool_sync/tests/part4.rs` 的逐字节断言测试在重构前后原样通过。

## D-013：私有共享身份采用 GitHub App 设备流与系统凭据存储

- 日期：2026-08-05
- 状态：accepted
- 背景：私有共享频道需要用户身份和可撤销的 GitHub 权限，但要求用户粘贴 PAT、共享仓库凭据或依赖机器上预先配置的 `gh` 都会扩大秘密暴露面，并让 GUI、CLI 与 Git 传输使用不同身份来源。
- 决策：第一版只支持 `github.com`，使用注册的 SkillStar GitHub App 设备授权流获取用户 access/refresh token。公开的 App client ID 由构建配置提供；桌面应用不携带 client secret、App private key 或 PAT。token 与 GitHub 返回的到期元数据只写入 OS 系统凭据存储，设备码和解析后的用户身份只存在进程内。认证 facade 以 GitHub gateway、credential store 和 clock 为测试接缝；生产 HTTP 每次通过 `probe_http_client` 获取当前代理配置。
- 后果：发布构建必须配置已启用 Device Flow 的 GitHub App client ID；缺失时登录动作明确不可用，但已有凭据仍可登出。GitHub App 安装范围与仓库权限继续由 GitHub 控制，SkillStar 不建立第二套身份或 ACL。GitHub Enterprise Server、PAT 和全局 `gh` credential 不进入第一版认证路径。

## D-014：私有 Git 认证采用操作级 askpass session

- 状态：已接受（2026-08-05）
- 背景：私有仓库需要让现有 Git 扫描、安装和升级复用 GitHub App 用户身份，同时不能把 token 写进 remote URL、Git config、命令参数、日志或 IPC。直接复用全局 credential helper 会让 GUI/CLI 身份漂移；将认证 header 写入 `git -c` 或 `GIT_CONFIG_*` 仍会把秘密放进 Git 配置通道；通过第三方镜像转发认证则扩大信任边界。
- 决策：`skillstar-skills` 为每次远程 Git 动作建立唯一 operation session，并以当前 SkillStar 可执行文件作为临时 `GIT_ASKPASS`。token 只存在于该 Git 进程及 askpass 子进程继承的专用环境变量，永不进入 argv 或持久文件；Git 强制关闭终端和 credential-manager 交互，操作完成或取消后随进程环境销毁。认证仅适用于规范化的 `https://github.com/` 远端，认证操作禁用 GitHub 镜像但临时注入当前 SkillStar 代理。session 的进度、错误和调试表示统一脱敏。
- 后果：同一 Skills 域 facade 可供 GUI 与未来 CLI 使用，并可用 fake credential/transport 验证凭据生命周期。SkillStar 可执行文件必须保留内部 askpass 入口；所有新增远程 Git 动作必须通过 operation session，不能直接启动裸 Git 网络命令。进程环境本身属于敏感边界，崩溃报告和诊断不得采集该专用变量。
- 证据：issue #20、`crates/skillstar-skills/src/github_auth/`、`src-tauri/src/commands/github/auth.rs`、Settings GitHub 登录测试。

## D-015：组织私有共享频道采用 repository ID 与可恢复两阶段绑定

- 状态：已接受（2026-08-05）
- 背景：共享频道需要先创建组织私有仓库，再把它加入 GitHub App 的 selected-repository 授权范围。owner/name/URL 会随仓库转移或重命名变化；远端创建与 App 授权又无法成为单个 GitHub 原子事务。若只在全部成功后登记，授权中断会留下无法识别的孤儿仓库；若按名称重试创建，则可能重复建仓或误绑同名仓库。
- 决策：数字 repository ID 是频道稳定远程键，owner/name/URL 只作可刷新路由元数据。创建前校验目标组织已采用 selected-repository 安装 SkillStar GitHub App，并授予 Administration/Contents write；随后由 App 用户身份创建私有仓库，依赖 GitHub 将 App 创建的新仓库自动加入该安装范围。创建成功后立即原子持久化版本化 pending descriptor，再以只读 API 校验 App 可访问该 repository ID，最后转 active；绝不使用 GitHub App 用户令牌不支持的安装范围写接口，恢复也只接受 registry 中的 repository ID。仅允许组织私有 `github.com` 仓库，权限按 Admin→owner、Maintain/Write→publisher、Read→subscriber 投影。本地 registry 不含凭据，REST 访问复用当前 GitHub App 用户身份和统一代理 client。
- 后果：pending descriptor 落盘后，用户能在详情中完成 App 安装/授权并安全续接；仓库重命名不会改变频道身份，后续同步必须先按 ID 校验并刷新路由元数据。GitHub `201 Created` 与首次本地原子写之间仍是不可消除的跨系统故障窗口：若进程终止或磁盘写入失败，SkillStar 不按名称猜测、不自动删除仓库，组织所有者需在 GitHub 手动处理该孤儿仓库。
- 扩展：已有组织私有仓库不直接绑定，而先建立进程内、ID-bound 的 registration session。扫描以当前 revision 的完整 tracked tree 披露全部 Skill、非 Skill 文件以及整段历史可读边界，不以稀疏工作树代表远端库存；generation tombstone 丢弃取消后的晚到结果，确认原子 claim 预览并在落 registry 前重新按 numeric ID 校验远端与重复绑定。确认失败保留 session 以便恢复；GitHub 登出、进程重启、取消或成功后清除，且不持久化 checkout 路径或任何凭据。
- 扩展：频道发布以 `channel-vNNNNNN` annotated tag message 中的版本化 canonical manifest 和同名 GitHub Release 为唯一远端版本边界；branch commits 永远只是草稿。manifest 绑定 stable repository ID、精确 commit、发布者、时间和全量 Skill snapshot hash，并显式携带 added/updated/unchanged/removed。revision 只从已验证的远端 tags 单调派生；本地不预增计数。发布预览用短生命周期 session 固定 commit，确认时 HEAD 漂移、权限变化、schema/identity 不符或远端拒绝均 fail-closed。

## 新增记录格式

```text
## D-NNN：标题

- 日期：YYYY-MM-DD
- 状态：proposed | accepted | superseded
- 背景：为什么必须做选择
- 决策：选择了什么
- 后果：获得什么、承担什么
- 证据：代码、测试、issue 或提交
```

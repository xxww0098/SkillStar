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
- 状态：accepted（crate 形状被 [D-049](#d-049吸收通不过-deletion-test-的浅-crate) 吸收；元数据 SSOT 与「无产品域依赖」不变量仍有效）
- 背景：Models preset 与 Usage catalog 都需要 Provider identity/鉴权事实，但二者不能相互依赖。
- 决策：canonical identity、鉴权和余额端点元数据只保存在一处；Models 与 Usage 分别从它派生自己的产品注册表，并用测试锁定映射。该处最初是零依赖 crate `skillstar-providers`；D-049 把它收成 `skillstar-core::providers`，模块本身仍不依赖任何产品域。
- 后果：添加 Provider 先修改 identity；不得在命令层或前端复制鉴权规则。
- 证据：`crates/skillstar-core/src/providers/`、Models/Usage guard tests。

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

## D-013：私有共享身份采用 GitHub App 设备流与应用私有文件存储

- 日期：2026-08-05
- 状态：accepted
- 背景：私有共享频道需要用户身份和可撤销的 GitHub 权限，但要求用户粘贴 PAT、共享仓库凭据或依赖机器上预先配置的 `gh` 都会扩大秘密暴露面，并让 GUI、CLI 与 Git 传输使用不同身份来源。
- 决策：第一版只支持 `github.com`，使用注册的 SkillStar GitHub App 设备授权流获取用户 access/refresh token。公开的 App client ID 由构建配置提供；桌面应用不携带 client secret、App private key 或 PAT。token 与 GitHub 返回的到期元数据写入 `SKILLSTAR_DATA_DIR/state/github_auth.json`，首次创建和每次更新保持 Unix `0600`；设备码和解析后的用户身份只存在进程内。认证 facade 以 GitHub gateway、credential store 和 clock 为测试接缝；生产 HTTP 每次通过 `probe_http_client` 获取当前代理配置。GitHub 认证不访问 OS 系统钥匙串，避免应用启动触发系统密码授权。
- 后果：发布构建必须配置已启用 Device Flow 的 GitHub App client ID；缺失时登录动作明确不可用，但已有凭据仍可登出。现有钥匙串凭据不自动迁移，切换后需要重新登录一次；之后启动只读取本地私有文件。GitHub App 安装范围与仓库权限继续由 GitHub 控制，SkillStar 不建立第二套身份或 ACL。GitHub Enterprise Server、PAT 和全局 `gh` credential 不进入第一版认证路径。

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
- 扩展：成员资格与 open invitation 继续完全采用 GitHub collaborator/invitation API，不建立 SkillStar ACL、share code 或邀请历史。管理动作以当前 GitHub 有效 Admin 为门槛；subscriber/publisher 分别使用 GitHub read/write，已有直接、继承或 pending 权限时不重复邀请。接受邀请前仅以 repository ID、路由和目标角色写入 `awaiting_invitation_acceptance` 恢复 descriptor，GitHub 接受后转 active；最终落盘失败，或网络/5xx 令接受结果不确定时保留 marker，并从当前身份可见的私有仓库库存按 repository ID 恢复，避免远端 invitation 已消费而本地入口丢失。GitHub invitation 不支持自定义来源 metadata，因此 inbox 公开事实是“组织私有 GitHub 仓库邀请”，用户显式确认是否导入为 SkillStar 频道；GitHub REST 无独立 resend，重邀是明确的 cancel-and-create 非原子序列。
- 扩展：接受 invitation 与订阅/安装是两个独立同意边界。订阅 facade 以最新已验证 Release manifest 为评审 SSOT，并在确认时再次校验 stable repository ID 与精确 revision/tag/commit；Git scanner 固定到 commit，逐项验证 content root/hash 后才复用 staged batch installer。选择、release target、安装 baseline 与无凭据 provenance 保存到独立版本化本地 store，新增 Skill 不自动扩展选择；未知 schema 只读展示并 fail-closed。这样 GitHub 继续独占访问控制，SkillStar 只拥有本机消费意图和可回滚安装事务。
- 扩展：频道升级默认自动检查、手动应用，并按 Skill 独立提交而非整包原子覆盖。最新 Release 只提供目标事实；每个已选择 Skill 以自身 baseline、release hash 与 provenance 决定能否前进，因此干净项可成功、分歧或失败项仍留在旧 commit，频道状态由各项推导。新增项只通知，removed 项不静默删除；分歧复用统一 `.local` 保留/丢弃动作。最近已验证的检查与结果保存在本地订阅 descriptor 中，网络失败不能抹掉旧可用状态。
- 扩展：后台检查采用固定一小时到期窗口并覆盖所有订阅；自动应用则采用“按频道显式开启 + 复用手动事务”的保护模式。调度器只为已开启频道筛选可证明未修改的已订阅项，不通过自动提供分歧 resolution；pinned、removed、权限/完整性异常和未解决失败均形成持久暂停证据。新增项不会被自动确认掉。到期判断与执行结果属于版本化 subscription descriptor，Tauri 后台任务只是可替换的唤醒器，这让 fake gateway 与固定时间可以验证策略，也避免 UI 生命周期成为自动升级的数据真相。
- 扩展：历史回滚是逐 Skill 安装事实的反向移动，不是频道 release target 的整体倒退。候选历史必须来自同一 stable repository ID 的已验证 manifest，并将当前和目标都绑定到精确 commit/content root/hash；应用复用现有 staged transaction 与部署补偿。成功后的 per-Skill pin 是本机消费意图，与订阅一起持久化，同时排除手动和自动批量升级；“恢复跟随”只清 pin 并重算最新计划，不隐式改写安装内容。
- 扩展：发布者移除 Skill 只改变频道可跟踪集合，不授权订阅端自动删除。本地项进入 `removed_from_channel` 并保留内容/部署；卸载或转本地必须是用户动作，后者以完整快照和冲突安全名称建立新的本地所有权。未处理的 removal tombstone 不会因远端同名重加而退回普通更新；处理后从 tracked/known/pin 移除身份，因此未来重加只会产生显式安装选择，不把远端同名解释为可覆盖本地副本的恢复授权。移除事务在共享 update lock 下把 Hub/lockfile staging 与 subscription metadata 绑定：metadata 失败回滚，metadata 成功后的清理失败不得重新跟踪。
- 扩展：成员撤销不维护 SkillStar 成员表，也不尝试修改 GitHub 的 Team、组织 membership 或 base permission。owner 端删除 direct collaborator 后必须以 effective permission 复查结果作为结论；subscriber 端把明确失权持久化为远程状态并 fail closed，停止后续内容 mutation，但本地已下载内容继续归用户控制。暂时网络/代理/API 错误只保留上次已知状态；权限恢复必须先通过新的仓库身份与读取权限验证。
- 扩展：订阅远程状态采用五态投影而不是一个 revoked 布尔值：明确删除/失权为 `revoked`，网络/代理为 `offline`，未登录/限流/暂时协议错误为 `recoverable_failure`，stable ID 或组织漂移、未知 schema、tag/commit/path/hash 解绑为 `integrity_error`，全链验证通过才为 `active`。所有非 active 状态都冻结远端 mutation 并保留本地内容与最后快照；恢复探测不能跳过任何完整性校验。仓库同组织改名只按数字 repository ID 刷新本地路由，跨组织转移绝不自动跟随。

## D-016：移除 S3 云同步，保留 SSH 与 GitHub 共享频道

- 日期：2026-07-10
- 状态：accepted
- 背景：S3（跨设备技能同步）与 GitHub 共享频道（组织协作）功能重叠；维护三个传输后端（SSH/S3/GitHub）成本高于收益，产品定位同时覆盖个人与团队。
- 决策：删除 `skillstar-sync` 的 S3 全部代码（client/store/manifest/local_pack/sync/types、`s3_sync.rs` 命令、`src/features/s3/`、IPC 契约与 i18n）。跨设备/团队共享统一走 GitHub 共享频道；SSH 保留为个人服务器部署路径。
- 后果：个人多设备同步依赖 GitHub 仓库/组织（无 GitHub 场景失去该能力）；S3 兼容 endpoint（MinIO/R2/OSS）不再可用；`skillstar-sync` 成为 SSH-only crate。不再新增 S3 类对象存储后端。
- 证据：crates/skillstar-sync/src/（仅 ssh）、docs/features/sync/README.md、commit 待定。

## D-017：OMP（Oh My Pi）注册为独立内置 Agent，仅 Skills 分发

- 日期：2026-08-10
- 状态：accepted
- 背景：OMP（`@oh-my-pi/pi-coding-agent`，命令 `omp`）与 Pi（`@earendil-works/pi-coding-agent`，命令 `pi`）是同源但独立的产品：配置根 `~/.omp` 与 `~/.pi` 互不读取，OMP 的模型配置是 `~/.omp/agent/config.yml`（modelRoles）+ 自有 models.db 目录，不读 `~/.pi/agent/models.json`/`settings.json`，技能位置是 `~/.omp/agent/skills`（全局）与 `.omp/skills`（项目）。此前 SkillStar 只注册 Pi，OMP 用户的技能发现、链接与部署全部失效。
- 决策：在 `skillstar-skills::agents::builtin` 的 extension 区（与 `grok` 并列，不在 vercel-labs 上游 id 内）注册 `omp`（显示名 Oh My Pi）：全局 `~/.omp/agent/skills`，项目 `.omp/skills`；`skillstar-skills::discovery` 优先级目录加入 `.omp/skills`。`~/.omp/agent/managed-skills`（OMP Auto-Learn 自动生成）不纳入发现与部署。轴②（Models 工具同步）暂不接入——OMP 的 provider 注入 schema（config.yml modelRoles / models.db）与 Pi 不同，待调研后另行设计。
- 后果：OMP 用户可手动激活并在 Settings / Projects / My Skills 中链接技能；OMP 不进模型绑定矩阵（`ProviderToolId` / AGENT_SPECS）；与 `grok` 同为 SkillStar extension，同步上游注册表时不受影响。
- 证据：`crates/skillstar-skills/src/agents/builtin.rs`、`crates/skillstar-skills/src/discovery.rs`、`src/components/ui/icons/agentIcons.ts` 及覆盖测试；本机 `~/.omp/agent/`（config.yml、models.db、managed-skills）与 `~/.pi/agent/` 布局实证。

## D-018：OMP 模型绑定采用 models.yml providers 块 + config.yml modelRoles 指针

- 日期：2026-08-10
- 状态：accepted
- 背景：D-017 暂缓了 OMP 的轴②（Models 工具同步）。调研 OMP 源码（`@oh-my-pi/pi-coding-agent`）确认其自定义 provider 注入机制：`~/.omp/agent/models.yml`（YAML；models.yaml/models.json 为兼容回退）的 `providers.<key>` 块支持 `baseUrl` / `api` / `apiKey` / `models[]`，schema 与 Pi 的 models.json 同构；活动指针是 `~/.omp/agent/config.yml` 的 `modelRoles.default`（`provider/model` 串，可带 `:thinking` 后缀）。OMP 的 API key 解析（`resolveConfigValue`）支持 `!cmd` / env 名 / 字面量三种来源。tool-sync 现有 JSON/TOML 骨架无法覆盖 YAML。
- 决策：在 `skillstar-models::tool_sync` 注册 `omp`（kind Multi、RequiredUrl::Openai），新增 YAML 文件规格（`format: "yaml"`，编辑器校验与格式化走 serde_yaml，保序），写 `~/.omp/agent/models.yml` 的 `providers.skillstar_*` 块（`api: "openai-completions"`、明文 `apiKey`、最小 `{ id }` 模型条目）与 `config.yml` 的 `modelRoles.default` 指针；停用只清理托管块与托管 default 指针，`slow`/`smol` 角色和用户其余设置保留。与 Pi 绑定互不影响（不同配置根）。
- 后果：OMP 用户可在 Models 工作台绑定第三方 Provider；models.yml 由 SkillStar 以 YAML 写入（OMP 原生偏好，与用户手写格式一致）；YAML 注释不保留（与 OMP 自身写入行为一致）。
- 证据：`crates/skillstar-models/src/tool_sync/{agents.rs,multi_provider.rs,paths_files.rs}`、`src/features/models/lib/agentRegistry.ts`、`matrixColumns.ts`、`tool_sync/tests/part4.rs` OMP 测试、`docs/features/models/README.md`。

## D-019：Skill 安装/打包/采用采用 frontmatter 质量门禁

- 日期：2026-08-10
- 状态：accepted
- 背景：open Agent Skills 生态（agentskills.io 规范、`npx skills`、Anthropic skill-creator）以 `name` + `description` 为 SKILL.md 必填字段，`description` 驱动 agent 决定何时触发技能；SkillStar 此前接受无 description 的 SKILL.md，产生空描述卡片和无法触发的安装项。同时本地目录采用路径只复制 SKILL.md，静默丢失 scripts/references/assets（数据丢失 bug）。
- 决策：新增 `skillstar-skills::validation` 单一 frontmatter 解析/校验实现（discovery 复用同一解析器）。阻塞级问题：description 缺失/非字符串/超 1024 字符/含尖括号、name 超 64 字符、frontmatter 缺失或 YAML 损坏 —— 在 repo-scan 安装（`scan_install`，覆盖 GUI/CLI/marketplace/频道/卡组）、pack 安装、bundle 导出/导入、本地目录采用（`adopt_folder`，改为完整目录复制）处 fail-closed，错误列出全部失败技能与可行动原因。咨询级问题（name 缺失回退目录名、非 kebab-case）不阻塞，经 `DiscoveredSkill.frontmatter_issues` 传入前端展示。本地创作/分歧副本（`local_skill::create`/`create_from_snapshot`）不做门禁，避免阻止用户对已损坏内容的保存流程。CLI 本地目录安装删除重复实现，改用 `adopt_folder` facade。
- 后果：新装/打包/采用技能保证可触发、有描述；批量安装中单个无效技能按项失败且不阻断其余；GUI 扫描预览对无效技能显示警告标记。既有已安装的无描述技能不受影响（门禁只作用于写入路径）。adoption 现在保留完整技能目录而非仅 SKILL.md。
- 证据：`crates/skillstar-skills/src/{validation,plugin_manifest,discovery,scan_install.rs,skill_pack.rs,skill_bundle.rs,local_skill.rs}.rs`、`crates/skillstar-app/src/cli/install.rs`、`src/features/my-skills/components/import-modal/SelectSkillsPhase.tsx` 及覆盖测试。

## D-020：更新检测采用 GitHub API 快速路径，凭据不出 Git session

- 日期：2026-08-10
- 状态：accepted
- 背景：patrol/批量刷新每小时对每个唯一 repo 执行 `git fetch`，只为判断是否有技能内容变化（fetch 后还需本地 rev-parse 对比 subtree hash）。`npx skills` 用 GitHub Trees API 的目录 tree SHA 直接做远程对比，避免包传输。SkillStar 的 Git 认证纪律（D-014）要求 token 只在 askpass 子进程环境内存在。
- 决策：新增 `skillstar-skills::update_api`：对 `github.com` 来源且能本地解析远端 ref（pinned ref 或 `origin/HEAD` symbolic ref）的 repo，每个 cycle 至多 40 个 repo、并发 8、超时 10s，以 `GET /repos/{o}/{r}/git/trees/{ref}`（非递归）获取顶层树；目录条目 sha 即本地对比所需的 subtree hash。成功 → 该 repo 跳过 prefetch fetch，`check_update_local_with_api` 对比本地 HEAD subtree hash 与 API hash（缺失目录 → None 保留徽标）。任何失败（私有仓库 404、限流 403、网络、truncated）→ 回退既有 git fetch 路径。HTTP Bearer 只在用户**已经**持有 SkillStar GitHub App session 时附带，把额度从匿名 60/h/IP 提到认证 5000/h；未登录保持匿名。凭据不出 Git session（D-014）。`X-RateLimit-Remaining: 0` / HTTP 429 写入进程内冷却至 `X-RateLimit-Reset`，本 cycle 未发出的 API 调用直接跳过；401/403/404/限流是设计内回退，记 debug 不记 warn。`prefetch_unique_repos_in_session_skipping` 与 `check_update_local_with_api_entry` 保持既有 None/Some 契约与 revision 裁决。
- 后果：github.com 公共来源的更新检测从网络 fetch 降为一个轻量 API 调用；已登录用户不再把同 IP 的匿名额度打爆；私有/非 github 来源继续走 git fetch。API 故障只影响速度不影响正确性。匿名限流（60/h/IP）由每 cycle 40 repo 上限、认证回退与冷却窗口共同兜底。
- 证据：`crates/skillstar-skills/src/{update_api.rs,update_checker.rs,installed_skill.rs}` 及 `api_remote_hashes_drive_update_detection_without_fetching` 等测试。

## D-021：仓库发现对齐生态容器目录深度与 Claude Code 插件清单

- 日期：2026-08-10
- 状态：accepted
- 背景：SkillStar 的 priority 容器目录只扫描一层直接子目录，`skills/<category>/<name>/SKILL.md` 这类 catalog 布局在已有扁平技能时被漏掉（`npx skills` 走 3 层且浅层技能遮蔽嵌套）；Claude Code 插件生态（anthropics/skills、daymade、Sylph 官方仓库）用 `.claude-plugin/marketplace.json`/`plugin.json` 声明技能路径，SkillStar 完全不读。
- 决策：`discovery::scan_priority_skill_dirs` 改为对每个容器目录最多走 3 层，含 SKILL.md 的目录遮蔽其下内容；仓库根保持 1 层。新增 `skillstar-skills::plugin_manifest`：读取 `.claude-plugin/marketplace.json`（`pluginRoot` + 本地 `./` 前缀 `source`/`skills[]`，跳过远程 source）与 `plugin.json`，在路径包含性与 `./` 前缀校验后，把声明的技能父目录以 depth-1 加入扫描。
- 后果：catalog 布局仓库在 root-first 模式即完整发现；Claude Code 插件市场仓库的声明技能可被扫描、预览与安装。manifest 只读取技能位置，不执行插件安装逻辑；越界/非 `./` 路径被拒绝。
- 证据：`crates/skillstar-skills/src/{discovery.rs,plugin_manifest.rs}`、`depth_and_plugin_tests` 与 `plugin_manifest` 测试。

## D-022：门禁补齐、死代码清理与单一名称解析（编排审查轮）

- 日期：2026-08-10
- 状态：accepted
- 背景：三名只读审查 agent（安装路径/死代码/解耦）确认 D-019 门禁在 scan_install/bundle/pack/adopt_folder/频道安装处生效，但发现三个可绕过入口：`install_skill` 的直接 clone 回退把门禁失败当 Ok(None) 后整库克隆且不校验；share_install embedded 走未门禁的 `local_skill::create`；projects/import 直接调 `adopt_existing_dir_locked` 不校验。同时：CLI 的 `find_target_skill_preview` 与域版存在已证实的语义漂移（显式 name 不匹配时预览误报 would-be-installed），`derive_name_hint` 双实现，`src-tauri/Cargo.toml` 把 4 个无条件 import 的内部 crate 声明成 macOS-only 目标依赖（Linux/Windows CI 提交后必红）。
- 决策：① 三个绕过入口全部接入 `validation::ensure_installable`：直接 clone 后校验、失败删克隆并返回可行动错误；embedded 创建后校验、失败回滚删除；项目导入逐技能校验、失败跳过并告警。② 删除经全仓 grep 证实的死代码：`adopt_existing_dir`/`validate_agent_ids`/`card_window_labels`/patrol 两个 sessionless wrapper/`normalize_repo_url` shim/S3 路径 helper/`TutorialLoadResult` 别名/`SkillCandidate.skill_md_path` 死字段/`shared.rs` 与 `src-tauri/src/core/path_env.rs` 两个一行垫片/根 re-export 裁剪（仅留 `Skill`/`SkillContent`/`discover_skills`）。③ 名称解析单一化：`source_resolver::derive_skill_name_hint`（Source-aware）成为唯一实现，域安装与 CLI 共用；`find_target_skill` 提升为 pub，CLI 删除内联副本。④ 4 个内部 crate 依赖移入通用 `[dependencies]`。
- 后果：门禁对任何安装入口都 fail-closed；预览与实装不再出现结论分歧；非 macOS 平台构建不再缺 crate；`skillstar-skills` 对外根路径只剩 3 个 re-export。死代码删除均为全仓零调用验证，不影响行为；`AgentProfile.installed`、`skillstar-git` sessionless wrappers、repo_history 写路径、ACP full-access 分支按审查结论保留待后续确认(2026-08-27 对抗审查轮已处理其中两项:sessionless wrappers 经全仓零调用复核后删除;repo_history 写路径在 `scan_github_repo` 成功分支接线,历史不再只读不写)。
- 证据：`crates/skillstar-skills/src/{skill_install,share_install,validation,source_resolver,discovery,local_skill}.rs`、`crates/skillstar-skills/src/projects/import.rs`、`crates/skillstar-app/src/cli/`、`crates/skillstar-channels/src/patrol/`、`src-tauri/Cargo.toml`、`direct_clone_gate_tests` 等测试；编排 run `run_3c969aa725f6`（REVIEW-A2/B/C2）。

## D-023：拉取多源化：Git mirror 候选链 + Marketplace host 链 + 内容寻址增量

- 日期：2026-08-10
- 状态：accepted
- 背景：对抗审查场景下任何单一远端都是单点故障：`skills.sh` 或 `github.com` 被 DNS 污染/SNI 阻断后，商店拉取与技能安装/更新整条链路不可用；已有 `github_mirror` 只支持一个 mirror，失败仅回退直连。同时快照同步每次全量 delete+reinsert，无法判断"远程内容是否变化"，也没有来源记录可供审计。
- 决策：① Git mirror 从单值扩展为候选链：`candidate_mirror_urls()` 返回"custom → 选中 preset → 其余 presets（去重、规范化）"，`skillstar-git` 的 transport/ops 对每个候选逐个尝试，全部失败才回退直连 GitHub；带凭据操作仍禁止走 mirror。② Marketplace 拉取增加 host 链：`remote::marketplace_hosts()` 以 `https://skills.sh` 为首，按 `config/marketplace_mirror.json` 追加镜像；`fetch_with_failover` 按序尝试、失败降级，并返回 `FetchMeta{payload_sha256, source_host, etag}`。③ sync_state schema v11 新增 `source_host`/`payload_sha256`/`etag` 列；快照同步与 MCP registry 同步接入内容寻址增量写：payload 未变化（304 或 sha256 相同）时只更新时间戳、保留旧数据与指纹，跳过全量重写。
- 后果：任何单个 mirror/host 失效都能自动降级到下一个候选，技能安装/更新与商店拉取在审查环境下可恢复；快照写放大显著下降且可审计数据来源。副作用：同步语义从"总是重写"变为"内容寻址增量"，依赖指纹正确性（sha256 冲突风险可忽略）；host 链按序尝试增加了失败时的延迟。
- 证据：`crates/skillstar-core/src/config/github_mirror.rs`、`crates/skillstar-git/src/{transport,ops}.rs`、`crates/skillstar-marketplace/src/remote/mod.rs`、`crates/skillstar-marketplace/src/snapshot/{sync,sync_state,migrations}.rs`、`v11_migration_adds_content_addressing_columns` 等测试。

## D-024：共享 skills 目录塌缩为部署目标，归属改由链接指向 hub 推导

- 日期：2026-08-12
- 状态：accepted（决策已定，落地未开始；本条只记录选择与约束）
- 背景：`BUILTIN_AGENT_DEFS` 的 74 个内置 Agent 中，多组解析到**同一个物理目录**：Global 侧 `~/.agents/skills`（cline/dexto/kimi-code-cli/loaf/warp/zed）、`<config>/agents/skills`（amp/replit/universal）、`~/.zencoder/skills`（zencoder/zenflow）；Project 侧 `.agents/skills` 被 18 个 Agent 共用，另有 `.qoder/skills`、`.trae/skills`、`.zencoder/skills` 各 2 个。D-007 已为 Project 侧选择"manifest 单一 owner + 按路径去重"，但只读取证证明该模型两侧都未兑现，且失败形状同源：**磁盘上不存在"这条 entry 是谁装的"这一信息，代码在需要它时一律退化为"看起来像技能就删"**。Global 侧更彻底——`deployment/` 下没有任何归属记录，`unlink_skill_from_agent`(`deployment/mod.rs:305-336`) 直接删共享目录里的条目，其余 5 个仍启用的 Agent 静默失去该技能；Project 侧 `remove_skill_from_all_projects`(`projects/sync.rs:132-157`) 与 `clear_project_symlinks`(`projects/helpers.rs:36-39`) 会删掉从未登记进 `skills-list.json` 的目录，与 `sync.rs:103-106` 自己的注释直接矛盾。根本困难在于 per-agent 归属是产品虚构：`~/.agents/skills` 是生态共享约定，zed 事实上就能加载 cline 部署的技能，任何试图记录归属的方案在存量磁盘上都没有正确的起手（已有部署无归属记录，记为无主/归给全部/归给第一个三种起手都错）。
- 决策：① **目录即部署单元**：把 canonical 目录键提升为一等"部署目标"，N 个解析到同一目录的 Agent 在部署模型与 UI 上塌缩为 1 项并列出成员 Agent；不记录 per-agent 归属，因为它在物理上不存在。Agent 的**启用开关仍是 per-agent 的**（D-009 不变），只塌缩部署目标，不塌缩激活状态。② **归属零状态推导**：一条 entry 属于 SkillStar，当且仅当其链接目标落在 `hub_skills_dir()` 之下——已验证 5 个全局写入点（`deployment/mod.rs:165/406/532/536/631`）的 src 全是 hub 绝对路径，且 `read_link_resolved`(`fs_ops.rs:174-184`) 确定只解一跳，hub→repo cache 的两跳链返回中间的 hub 路径。③ **容器判定复用 `repo_link::is_inside`**(`repo_link.rs:65-77` 及其 `normalize`:79-93) 的形态（双侧 canonicalize with fallback + 分隔符归一 + Windows 小写折叠），提升为可复用实现，并把 `local_skill.rs:174`、`git/gh_manager.rs:432`、`storage_maintenance.rs:183` 三处裸 `starts_with` 一并收敛；`repo_link.rs:4-9` 已记录过"两份实现分叉导致 Windows junction 误判"的同类事故，不制造第四份。④ **目录身份键**用 `fs_ops::canonicalize_existing_prefix`（`skillstar-core`；`skill_update` 仍保留同名包装）处理"目录尚不存在"，与 ③ 的容器判定是两个不同问题，不合并。键只在每次 `list_profiles()` 快照内重算，**不持久化**——openclaw(`builtin.rs:487-494`) 与 5 个 env 驱动 Agent(`builtin.rs:102/115/152/237/294`)、`XDG_CONFIG_HOME`(`builtin.rs:473-478`) 的目录会随环境与磁盘状态漂移。⑤ **copy 形态用 sibling marker**（无链接可读），判定顺序是先试 link 谓词、`is_link` 为假才查 marker。⑥ **拒绝 project root 等于或包含任一 agent global 目录**：`ensure_project_root_exists`(`projects/types.rs:74-85`) 与 `cli/install.rs:132` 今天只检查 `is_dir()`，HOME 可被注册成 project 从而让 project 部署写进 global 共享目录，此时两个 surface 的 src 同为 hub、谓词无法区分。⑦ 归属判定**不复用** `acquire_skill_mutation_lease`：它是不可重入的进程级 `Mutex`(`skill_update/transaction.rs:4-7`)，三个 resync 入口已在其内，deployment 层再 acquire 会自死锁；改为按目录键的独立同步，并覆盖今天完全无锁的 5 个 Tauri 命令（`commands/skills.rs:91`、`commands/agents.rs:28/44/64/76`）。
- 后果：获得——存量**零迁移**（磁盘即真相，不需要任何归属回填或"收养"决策）；崩溃后重扫即一致（不引入第二份可与磁盘分叉的状态）；4 处计数串台（`installed_skill.rs:532-548`、`registry.rs:131-153`、`deployment/mod.rs:280-302`、`global_deploy.rs:23-38`）结构性消失而非逐个打补丁；CLI 已有的 `seen_dirs` 去重(`deployment/mod.rs:476,491`)从特例**泛化**为全局不变量。承担——Settings 里共享目录的多行塌缩为一行是**可见的 UX 退让**，需文案说明"这是这些 Agent 自己的生态约定，非 SkillStar 决定"；`AgentProfile` 是冻结的 8 字段 IPC（`registry.rs:16-18`、本文件 D-008），塞不进第 9 个字段，必须新开 `list_deploy_targets` IPC 而非扩展它；unlink 语义从"从某个 Agent 移除"变为"从某个目录移除（影响其全部成员 Agent）"，这是**如实陈述**而非降级——旧语义在磁盘上从未成立。本条扩展 D-007：D-007 的"按路径去重"方向正确但只在 `build_path_plans`(`projects/sync.rs:57-98`) 与 `add_skills_to_project_with_mode`(`sync.rs:419-465`) 两处兑现，scan/rebuild/cleanup 三处未兑现，本条把该不变量的适用范围扩展到 Global 侧并要求两侧同源实现。落地必须先于模型改动修掉三条既有 bug：`swap_in_fresh_deploy` 与 unlink 之间的 lost update（`mod.rs:635-640` + `mod.rs:102`，用户看到"已取消部署"但 rename 把技能复活）、`toggle_skill_for_agent` enable 分支先删后建不回滚（`mod.rs:154-173`，应复用 `mod.rs:619-654` 的先建后换）、以及 ⑦ 的无锁命令同步。
- 证据：只读取证编排 run `run_f21c372c2a96`（Global 取证 / Project 取证 / lease 与碰撞面 / 候选模型对抗评估 / provenance 验证，5 个 Task）。代码依据见 `crates/skillstar-skills/src/agents/builtin.rs`、`crates/skillstar-skills/src/deployment/mod.rs`、`crates/skillstar-skills/src/projects/{sync,helpers,rebuild,scan}.rs`、`crates/skillstar-skills/src/repo_link.rs`、`crates/skillstar-core/src/infra/fs_ops.rs`。可复发根因与自检见 [errors.md](./errors.md) 同日条目。

## D-025：OMP 模型角色存在 binding 级设置袋，未分配角色不写盘

- 日期：2026-08-13
- 状态：accepted
- 背景：D-018 只写了 `modelRoles.default`，而 OMP 的核心差异化能力恰恰是按任务意图路由的多角色系统（`default` 正常编码 / `smol` 廉价子代理 fan-out / `slow` 深度推理 / `plan` 规划模式，命令行对应 `--model` / `--smol` / `--slow` / `--plan`，另有 vision/designer/commit/tiny/task/advisor 六个）。用户要在 SkillStar 内完成这层配置，就必须能为**不同角色指定不同 provider**（典型配置是 default 用便宜的快模型、slow 用推理模型、smol 用最便宜的），因此角色不能挂在任何单个 provider 条目上。既有扩展点 `ToolActivation.settings` 是 per-entry（per-provider）的：`activate_tool` 在重新激活同一 provider 时按 provider 继承它（`crud.rs:331-342`），active 指针一变角色就会跟着漂。对 OMP v17.2.15 源码与二进制的实证确认：`modelRoles` 是**无 schema 校验**的开放 string map（`settings-schema.ts:569` 的 `type: "record"`，config.yml 整体不过 arktype），值语法为 `provider/model[:thinkingLevel]`，`@role` 是角色别名前缀，`smol`/`slow`/`designer` 未配置时由 OMP 自己回落到 `default`（`shouldInheritDefaultBeforePriority`）。
- 决策：在 `ToolBinding` 上新增**binding 级** `settings: Option<Value>`，作为 `ToolActivation.settings` 的工具级兄弟，首个消费者是 `OmpSettings { roles: BTreeMap<String, OmpRoleTarget> }`（`OmpRoleTarget { provider_id, model, thinking }`，存 SkillStar provider id 而非磁盘上的 `skillstar_*` 键，键在写入时由 `skillstar_managed_key` 现算，避免两处规则分叉）。写入命令 `update_tool_binding_settings` 与既有 `update_tool_settings` 对称。落盘策略与 models.yml 托管块的 retain 一致：**先删除全部指向 `skillstar_*` 的角色，再写当前集合**，因此 UI 取消分配等价于磁盘删除；指向用户自有 provider 的角色永不触碰。**未分配的角色不写**（OMP 自身的回落机制比我们写一个冗余指针更正确），`default` 缺失时由 active 条目兜底以保持 D-018 行为。悬空防护：角色指向未绑定 / 无 OpenAI base URL / 未选模型的 provider 一律跳过；角色名含 `/`、空白或以 `@` 开头（撞 OMP 别名语法）一律跳过。解绑与删除 provider 连带清除其角色分配（`prune_binding_roles_for_provider` 只操作 `roles` 键，保留设置袋内其他键）。
- 后果：获得——用户在 Models 矩阵内即可完成 OMP 的全部角色路由，无需手写 YAML；binding 级设置袋对未来其他"跨 entry 配置"（OpenCode 的 `small_model`、Claude 的层级模型）是现成接缝。承担——`ToolBinding` 新增字段需要前端手写类型 `src/types/models.ts` 同步（该文件不在 `types:gen` 覆盖范围内，是既有分歧，本条未修复）；不代写 OMP 的 `cycleOrder`，因此用户配置的 `plan` 角色不会进 Ctrl+P 循环（默认只循环 smol/default/slow），这是 OMP 侧行为，由 UI 文案说明而非替用户改设置。
- 证据：`crates/skillstar-models/src/tool_sync/{types.rs,omp_provider.rs}`、`crates/skillstar-models/src/providers/{types.rs,crud.rs}`、`src-tauri/src/commands/models_commands/tools.rs`、`src/features/models/lib/{ompRoles.ts,toolBinding.ts}`、`tool_sync/tests/part4.rs` 与 `providers/tests/part5.rs` 覆盖测试；schema 依据为 OMP v17.2.15 包内源码（`src/config/{model-roles,settings-schema,model-resolver,models-config-schema-bundle}.ts`）；落盘结果经真实 `omp` 二进制验证（`omp models --json` 零告警、`omp -p --model @slow/@smol/@plan` 均正常解析，未配置角色对照组报 `Model not found`）。

## D-026：官方 MCP Registry 为一等主源，GitHub registry 降级为增强镜像

- 日期：2026-08-13
- 状态：accepted
- 背景：MCP catalog 此前只有一个源 `api.mcp.github.com`，实测已从 `/v0` 返回 `Deprecation: true`，且其条款没有任何公开的再分发说明——我们却把它整表镜像进本地 SQLite 快照并长期保留。同期官方 registry `registry.modelcontextprotocol.io/v0.1` 已可用，实测 21338 条（`version=latest`），而 GitHub 侧只有 218 条；官方 registry 的 Terms of Service 明确把数据置于 **CC0 1.0**，是当前唯一在许可上允许长期本地镜像的源。两者字段互补：GitHub 侧额外带 stars / license / readme，官方侧没有。
- 决策：官方 registry 成为 `priority: 0` 的一等主源且 `mirrorable: true`；GitHub registry 降为 `priority: 10` 的**展示增强镜像**（`mirrorable: false`），只在合并时补 stars/license/readme 这类官方源不携带的字段，不再定义"有哪些 server"。`priority` 语义被明确为**权威性**而非偏好：用户自定义源起始 `priority: 50`，永远输给内置源，但可以补充无人收录的 server。分页熔断按源设置而非全局——主源被截断会让 catalog 永久性不完整（`max_pages: 400`），而受 `x-ratelimit-limit: 10` 限制的镜像宁可截断也不该被反复敲打（`max_pages: 50`）。
- 后果：获得——catalog 从 218 条扩到 21363 条（跨源合并 189、deprecated 243）；镜像行为落在明确许可的源上；单源故障不再等于 catalog 归零。承担——本地快照体量与同步耗时上一个数量级，因此**全量未分页读取不再是可用的浏览路径**（见 D-027 的分页要求）；聚合 fingerprint 必须改为按 `id=hash` 排序哈希，否则新增/移除一个源或某源答 304 都会产生假"已变更"。
- 证据：`crates/skillstar-marketplace/src/mcp_remote/{sources.rs,fetch.rs,merge.rs}`、`crates/skillstar-marketplace/src/mcp_snapshot/mod.rs`；调研与实测端点见 `docs/others/mcp-modern-design-research.md` §3.1/§3.7/§4.1（抓取 2026-08-13）。

## D-027：跨源合并主键是 `server.json` 的 `name`，不是各源的 id

- 日期：2026-08-13
- 状态：accepted
- 背景：接入第二个源之后必须回答"两行是不是同一个 server"。每个 registry 都发自己的 id，同一个 server 在官方源和 GitHub 源下 id 不同；用 id 做主键会把同一个 server 重复上架两次，用户看到两张卡片、装出两份配置。`server.json` 的 `name` 是反向域名全名（如 `io.github.netdata/mcp-server`），schema 要求发布者证明命名空间所有权，是唯一跨源稳定的身份。
- 决策：合并以 `name` 为主键；同名多版本按 registry 的 `isLatest` 收敛。安装时记录到 `McpServerEntry::registry_name` 的也是这个 `name`，与写进工具配置的 sanitized key 明确区分——后者是配置键，会因为字符清洗而丢失身份信息。
- 后果：获得——跨源去重正确，"已安装"判定与更新检测可以摆脱 server 名字字符串的模糊匹配。承担——`name` 缺失或畸形的源行会被合并逻辑跳过；这是刻意的，一个连身份都没有的条目也无法被安全地安装或更新。
- 证据：`crates/skillstar-marketplace/src/mcp_remote/merge.rs`、`crates/skillstar-marketplace/src/mcp_models/mod.rs` 的 `namespace` 字段文档、`crates/skillstar-app/src/mcp/draft.rs` 的来源指纹映射。

## D-028：不接入 PulseMCP 与 mcp.so

- 日期：2026-08-13
- 状态：accepted
- 背景：调研阶段评估了六个第三方 MCP 目录作为潜在补充源。两个必须明确拒绝，否则日后会被反复重新提出。
- 决策：**不接入 PulseMCP**——其 API 公告 2026-09 全量日落，且在此之前已强制 API key；接一个已宣布死期的源，是在为一次确定的返工付费。**不接入 mcp.so**——其 `robots.txt` 明确禁止 `/api/`，抓取它等于无视站点的明示意愿，与我们对自己数据源的要求不一致。
- 后果：获得——避免重复调研，避免把工程投入押在确定会消失的端点上。承担——放弃这两家独有的收录条目。Smithery（`useCount` 热度、预抓取 `tools[]`）与 Glama（`spdxLicense`）未被拒绝，但只可能作为**运行时代理查询**的展示补充，不做长期镜像：两家都没有公开的再分发条款，与 D-026 的镜像许可要求冲突。
- 证据：`docs/others/mcp-modern-design-research.md` §4.3/§4.5/§4.8/§7 P2-7（端点与 robots.txt 抓取于 2026-08-13）。

## D-029：MCP 运行时形态优先 remote，且以本机实际可用运行时为准

- 日期：2026-08-13
- 状态：accepted
- 背景：一个 `server.json` 可以同时给出 `remotes[]` 与 `packages[]`。此前的选择逻辑是"有 packages 就取 `packages[0]`，否则取 `remotes[0]`"——由数组顺序决定在用户机器上执行什么，既不是安全判断也不是可用性判断。
- 决策：优先级为 `remotes[streamable-http]` > `remotes[sse]`（可用但传输已弃用，必须标注） > `packages[oci]`（容器隔离，最安全的本地形态） > `packages[mcpb]` > `packages[npm/pypi/nuget/cargo]`。remote 优先的理由是它零工具链依赖、零本地代码执行，且规范把 OAuth 授权明确限定为 HTTP 传输的能力。**rank 不是最终答案**：`runtimeHint` 只是提示，每个 stdio 候选都要对真实 `PATH` 探测一次（复用与真正启动进程相同的 resolver），不可用的候选排在所有可用候选之后——没装 Docker 的机器上 npm 包胜过 OCI 包。选择器返回**全部候选 + 推荐项**而非单一结果，用户可以覆盖。
- 后果：获得——默认选择同时是安全排序和可用性排序；"为什么推荐这个"可以在 UI 上解释（rank + 可用性 + 阻塞原因）。承担——`mcpb` 候选被列出但标为不可安装（SkillStar 没有"下载并校验 `fileSha256`"这一步，而 registry 明确不做校验），无 `runtimeHint` 的 `cargo` 候选同样不可安装（`cargo install` 是持久安装而非一次性运行器）；这两条是如实陈述能力边界，旧行为在这两种形态上本来也跑不起来，只是失败得更晚更难懂。
- 证据：`crates/skillstar-app/src/mcp/runtime.rs` 与 `crates/skillstar-app/src/mcp/tests.rs`（注入式 `PATH` 探测，排序规则逐条 pin）；依据见 `docs/others/mcp-modern-design-research.md` §6.4。

## D-030：Deck 自持 Agent 链接，不从成员 Skill 推导

- 日期：2026-08-13
- 状态：accepted
- 背景：Deck 卡片底部的 Agent rail 原本是纯派生量——卡组内已安装 Skill 全部链接到某 Agent 就点亮。自从 GUI 安装会把新装 Skill 部署到全部已启用 Agent（见 `crates/skillstar-app/src/global_deploy.rs`），任何新建卡组一出生就全亮，rail 不再表达任何用户意图，"取消链接"成了唯一可用操作。
- 决策：`SkillGroup` 增加 `agent_links: Option<Vec<String>>`，语义是**用户为这个卡组显式认领的 Agent**。新建卡组为空集，rail 全灰；点亮才批量链接成员 Skill。派生量降级为漂移指示：已认领但成员并非全部实际链接时显示 mixed，未认领一律不点亮。`None` 表示该卡组早于此字段，由 `skillstar-app::skill_group_links` 按旧派生规则一次性回填并落盘，避免升级瞬间抹掉用户既有的 rail。
- 后果：获得——新建卡组的默认状态回到"什么都没做"，rail 重新可读为意图；卡组与 Skill 两级链接语义分离，安装期全局部署不再污染卡组视图。承担——两级状态可能漂移，必须靠 mixed 显式暴露，不能静默取整；回填规则是一次性近似（以回填当刻的磁盘状态为准），此后不再重算。
- 证据：`crates/skillstar-skills/src/skill_group.rs` 与 `crates/skillstar-app/src/skill_group_links.rs` 的测试；行为见 `docs/features/skills/README.md`。

## D-031：技能发布链路并入单一 GitHub App 身份，`gh` CLI 退出远程路径

- 日期：2026-08-14
- 状态：accepted
- 背景：`skillstar-skills::git::gh_manager` 是全仓最后一处绕过 SkillStar 身份与代理策略的远程链路。它用 `gh auth status`/`gh api user`/`gh repo list`/`gh api contents`/`gh repo create --push` 完成 REST，用裸 `git` 完成 clone/pull/push。后果有三：发布用的是机器上的全局 `gh` 登录而不是 D-013 的 App 身份；裸 Git 继承启动进程的 `HTTP_PROXY`，而 D-014 的 transport 路径是显式清空再按 SkillStar 配置重设，同一个应用出现两套代理行为；`gh repo list <login>` 只能列个人仓库，组织仓库根本无法作为发布目标。
- 决策：发布链路全部改用 App 凭据。REST 移入新的 `skillstar-skills::git::gh_rest`：`GET /user`、`GET /user/repos?affiliation=owner,collaborator,organization_member`（分页）、`GET /repos/{owner}/{name}/contents/skills`、`POST /user/repos`，凭据取自 `GitHubAuthFacade::api_credential()`，客户端一律由 `probe_http_client` 构造并对齐 `Accept: application/vnd.github+json` 与 `X-GitHub-Api-Version: 2022-11-28`。发布入口是同步的（Tauri `spawn_blocking`、CLI 主线程），因此在同步上下文内自建 current-thread runtime 驱动 async 客户端，而不是引入 `reqwest::blocking` 绕开统一 client。远程 Git（clone / pull --rebase / push）改走 `skillstar_git::transport::execute_remote_command` 与一次性 operation session；本地 Git（init/add/commit/remote add/rev-parse）保持裸子进程，因为它们不接触远端。`GhStatus` 形状不变但语义重映射为「发布所需的 `git` 未安装 / SkillStar 未登录或凭据过期 / 就绪且带 App login」，前端三个分支零改动继续有效。`gh` 只保留 Settings 的 `check_gh_installed` 环境探测。
- 后果：获得——发布与扫描/安装/更新使用同一身份、同一代理和同一脱敏边界，token 不进 argv、git config、remote URL 或日志；组织仓库首次成为可选发布目标；401/403/404/422/429 有可行动分类而不是 `gh` stderr 裸串。承担——发布现在要求用户在 SkillStar 内登录 GitHub，光有全局 `gh` 登录不再够用；新建仓库只能建在当前用户名下（组织建仓属于共享频道路径，见 D-015）；`~/.agents/.publish-repos/` 下的既有缓存 remote 仍是历史 `gh` 写入的 URL，靠 session push 时重新认证而不是重写 remote。
- 证据：`crates/skillstar-skills/src/git/{gh_rest.rs,gh_manager.rs}` 与 `gh_publish_tests.rs`（三态重映射、分页与 affiliation、凭据只出现在 Authorization header、stub git 的 argv/输出无 token）。

## D-032：dev 构建不加 `[profile.dev]` / `build-override` / package 热路径清单，`target/` 也不搬内置盘

- 日期：2026-08-14
- 状态：accepted
- 背景：通用 Rust 构建指南普遍要求给 dev profile 加一套模板——`[profile.dev.build-override] opt-level = 3` 让 proc-macro 与 build script 按 -O3 编译，再配一份 20–40 个热路径包的 `[profile.dev.package.<name>] opt-level = 3`，并把 `target/` 放到内置 SSD。该建议在样本仓库上实测有 3.4× 提速，因此每隔一段时间就会有人提议照抄进本仓库根 `Cargo.toml`。本仓库此前从未加过这些覆盖（根 `Cargo.toml` 只有 `[profile.release]`），需要一个实测结论来判断这个"缺失"是疏漏还是正确状态。
- 决策：**不加 `[profile.dev]`，不加 `[profile.dev.build-override]`，不加 package 热路径清单，也不把 `target/` 搬到内置卷。**实测（12 核 Apple Silicon，每组全新 `CARGO_TARGET_DIR`，用 `cargo --config` 注入而非改 manifest，相邻背靠背配对以排除机器漂移）：照抄模板后冷 `cargo check --workspace` 从 54–62 s 劣化到 140–168 s（2.4–2.8×）；冷 `cargo build --workspace` 从 95.83 s 到 187.37 s（1.96×）；再叠加 41 个热路径包覆盖到 255.16 s（4.08×）。`build-override` 的 `opt-level` 0/1/2/3 分别为 58 / 104 / 127 / 140–168 s，**单调递增、无甜点**，因此不存在"取中间档"的折中方案。`target/` 位置同样实测无差异：冷 `build` 写 8.3 G 时外置卷 71.68 s，夹在两次内置卷 69.66 / 78.73 s 之间。
- 后果：获得——dev 迭代保持当前速度，且这条"不做"有了实测依据，不必每次重新辩论；`target/` 可以继续留在外置卷，省掉一次没有收益的搬迁。承担——放弃了该模板在其它仓库确实兑现过的收益，若本仓库的 derive 密度将来大幅上升，这条结论需要重测而不是继续引用。根因决定了何时该重测：**收益与 derive 点数量成正比，成本几乎固定**——`syn`×2 / `serde_derive` / `ts-rs-macros` / `tauri-codegen` / `tauri-build` / `tauri-utils` 按 -O3 编一遍的代价与仓库大小无关，而本仓库只有 756 个 serde + ts-rs derive 点，是样本仓库（4090 个）的 18%，摊不薄这笔固定成本。`--timings` 逐单元差分给出的直接证据是成本 +980 unit-seconds、收益仅 137 unit-seconds。另有一个只在 `cargo check` 主导的迭代循环里出现的陷阱：`package.<name>.opt-level` 会经 `OPT_LEVEL` 环境变量传给该包的 `build.rs`，`cc` 会照着它编捆绑的 C——`libsqlite3-sys`（bundled SQLite）3.67 s → 71.72 s、`aws-lc-sys` 67.62 s → 109.47 s，而这两个包在 `cargo check` 下根本不产出被检查的代码，是纯成本。
- 证据：根 `Cargo.toml`（保持只有 `[profile.release]`，无 `[profile.dev]`）；2026-08-14 构建速度审计的 A/B/C/D/BO1/BO2 对照组与 `--timings` 逐单元差分；derive 普查以 `#\[derive\((.*?)\)\]` 全仓扫描交叉验证（Serialize 332 / Deserialize 376 / TS 48）。

## D-033：账号切换用「软链直通快照」，SkillStar 不把凭证拷进 CLI 的 live 文件

- 日期：2026-08-14
- 状态：accepted
- 背景：原实现把订阅行的凭证**拷贝**进 CLI 的 live 凭证文件。CLI 自己也会轮换 token 并写回同一个文件，于是两份拷贝必然发散；Grok 那条链路里的 lease / sha256 乐观并发 / 临时 pin / 回读逐字段比对 / 提交回滚，全部是在管理这个发散 —— 而发散是这个抽象自己造出来的。同一时期 codex 与 opencode 只是裸写：codex 无任何锁与回滚（后台刷新会踢掉 Codex CLI 的登录），opencode 硬要 `api_key_encrypted` 而它的 catalog auth mode 是 Cookie|Manual，那条切号链路 100% 走 fail 分支，UI 却常驻一个永远点不通的入口。
- 决策：live 路径不再持有凭证，**它是指向快照的软链**；快照 `~/.skillstar/accounts/<catalog_id>/<subscription_id>.json` 是唯一真相。一份快照是**整个** CLI 凭证文件（软链只能整文件替身），不是其中一个账号的片段。三家 CLI catalog 共用一套 custody 引擎（capture → prepare → 换软链 → 回读 → 副作用 → 落 pin），各自只实现 `CliCredentialTarget`：路径、锁、access_token 提取、身份、materialize、absorb。对账比**内容**不比文件类型，三态 `LinkedTo / Diverged / Missing`，只比 access_token 字符串。`supports_cli_switch` 由 target 注册表推导而不是手抄白名单。Cursor / Antigravity 不强行塞进软链引擎：它们通过独立 IDE adapter 事务性写入并回读验证各自的 `state.vscdb`/系统凭证。
- 后果：获得——CLI 轮换 token 时写穿到快照，快照永远新鲜，「检测轮换再回抄」这类代码整体消失；`auth_mode` 与「能否切号」解耦（opencode 只要 CLI 里登录过就能切，不再需要 SkillStar 持有 API Key），那条死链路自然消失；pin 降级为可由 `reconcile` 重建的缓存，不再是第二个真相源；codex 第一次获得与 grok 同级的锁、备份、回读与回滚，「切换被拒时保留旧 badge」从只对 xai 成立变成全域成立。承担——(1) **整文件快照意味着同一个文件里其它 provider 的登录会跟着账号一起切**：opencode 的 `auth.json` 是 `providerID → 凭证` 的扁平表，在账号 A 期间登录的 anthropic 会留在 A 的快照里，切到 B 后不可见（切回 A 即恢复，不会丢失）。做成「只换本 provider 那一段」就必须回到拷贝语义，也就把发散请回来了，因此不做。(2) 软链盖不住三个洞，必须显式处理：macOS Codex 以 keychain 为准（activate 写、reconcile 吸收、写入改成 read-modify-write）；CLI 用 `rename()` 会把软链冲成实体文件（内容一致即判 `LinkedTo` 并静默重建）；Windows 无软链权限时降级为拷贝并在日志显式标注 `LinkMode::Copy`。(3) refresh token 单次使用的双花竞态**不会**因软链消失 —— 软链消灭的是陈旧拷贝，不是「谁先刷谁让对方失效」—— 所以 CLI 自己的文件锁（Grok 官方 `auth.json.lock` 及其 `PID:秒` holder 行）、刷新前 adopt、刷新后回投这三件事全部保留，只是从 xai 专属变成全域通用。
- 证据：`crates/skillstar-app/src/usage_switch/{custody.rs,cursor.rs,target.rs,keychain.rs,target/*}`、`crates/skillstar-usage/src/vscdb.rs` 与 `custody_tests.rs`（实体文件内容一致判 LinkedTo、CLI 轮换后快照自动新鲜、Cursor 两账号切换真实写入并回读 state.vscdb、IDE 切换失败时不移动 pin、opencode 无 API Key 也能切）；行为契约见 [features/usage/README.md](./features/usage/README.md)。

## D-034：DTO 投影拥有前端契约，有重构节奏的域类型不直接暴露给 ts-rs

- 日期：2026-08-14
- 状态：accepted
- 背景：`/usage` 的前端形状长期是 `src/features/usage/types.ts` 里手抄的 `usage/dto.rs` 镜像，已在六个方向漂移，其中两类靠 review 抓不住：带 `skip_serializing_if` 的字段 `None` 时**整个键从 JSON 消失**，手抄凭 `Option<T>` 直觉写成 `| null`，于是 `x === null` 分支从来没被触发过；永远序列化的 bool 是必填，手抄写成可选。同期把 usage DTO 接进 ts-rs 时发现，`usage_switch::SwitchOutcome` 若自己 derive `TS`，切换域的重构会直接震动前端契约。
- 决策：`skillstar-usage` 的 `subscription.rs` / `catalog.rs` 里**纯数据、无行为**的类型（枚举、usage 快照树）直接 derive `TS`——它们本来就是 wire 形状，再套一层投影只会制造两份要同步的定义。有自己重构节奏的域类型不上生成面：`skillstar_app::usage::dto` 定义 `SwitchOutcomeDto` 等投影，接缝落在 AGENTS.md 已经指定的跨域聚合 crate。`types.ts` 退化为纯 re-export barrel，只保留没有 Rust 对应物的前端自有物（表单能力声明、事件契约、筛选哨兵）。
- 后果：获得——手抄漂移这一整类缺陷被门禁消灭（`check_generated_types.sh`），且 ts-rs 读 serde 属性，上面两种错误它都不会犯。承担——(1) `impl From<SwitchOutcome> for SwitchOutcomeDto` 用**完全解构**而不是 `..` 展开，上游加字段会在这里产生编译错误，这正是希望的：新字段要么显式进入前端契约，要么显式被丢弃，不会静默消失。(2) ts-rs 默认把 `i64`/`u64` 映射成 `bigint`，而 Tauri IPC 经 `JSON.parse` 只产出 `number`，64 位字段必须按仓库既有惯例标 `#[ts(type = "number")]`。
- 证据：`crates/skillstar-app/src/usage/dto.rs`、`src/types/generated/`、`src/features/usage/types.ts`；契约描述见 [features/usage/README.md](./features/usage/README.md) 的「前端类型契约」。

## D-035：Provider store 四层分离（Catalog / Provider / Credential / AgentBinding）

- 日期：2026-08-15
- 状态：accepted
- 背景：v3 的 `ProviderEntryFlat` 把四件不同的事塞进一张扁平行。三个后果各自独立地伤人：①协议是端点的属性而不是 Provider 的属性——同一个中转常同时开 `/v1/chat/completions` 与 `/anthropic`（`deepseek` preset 即如此），两个硬编码 URL 字段撑不住第三种协议；②「没探测过」与「不支持」不可区分——Codex 默认靠「字段是否等于 serde 默认值」推断，于是用户显式选择与从未触碰长得一样；③凭据只能是一个 `String`，但 Codex 的 `env_key` 存的是变量名、OpenCode 支持 `{env:}`/`{file:}`、Claude 的 `apiKeyHelper` 是一条命令，四种语义不同的通道被压成一种。另外 `created_at` 是毫秒而 `last_sync_at` 是秒，同一个文件两种单位。
- 决策：v4 拆成四个 < 400 行的模块。`Provider` 持 `Endpoints`（每协议一个 `Option<String>`）+ `ProviderCaps`（三态 `Tri`）；`Credential` 是判别联合，`ExternalCli` 变体取代 v3 靠 id 白名单在六处分支的 Native Official 特例；`AgentBinding.roles` 把角色路由从设置袋提升为一等字段（v3 里 Claude 层级模型在 `provider.meta`、OMP 角色在 `binding.settings`，同一概念两套存储）；catalog 独立成型且移出 store 文件。所有时间字段带 `_ms` 后缀。
- 后果：获得——能力位可表达「未知」，因此迁移永远写 `Unknown` 而非 `No`，升级不会让用户已有绑定突然失效；Official 从「六处 id 比较」变成「一次 `matches!`」；角色路由可跨 Agent 推广。承担——(1) 需要 v3→v4 迁移与永久备份（见 D-036）；(2) `providers/types.rs` 降级为只供迁移读的历史类型，`crud`/`tool_sync` 的 v4 化是独立工作包，在那之前 v4 是已落地但未接入运行路径的一层。
- 证据：`crates/skillstar-models/src/providers/{provider,credential,binding,catalog}.rs`；`docs/others/model-redesign/05-redesign-proposal.md` §2.3。

## D-036：迁移必须双备份、写后读回，备份失败即中止

- 日期：2026-08-15
- 状态：accepted
- 背景：v3 的 `backup_and_write` 在备份失败时只打一条 warn 就继续迁移，`read_flat_store` 对任何解析失败一律返回空 store「保证应用总能启动」。两条合起来构成一个静默毁数据的路径：一次读失败会让损坏的 store 看起来和首次运行一模一样，紧接着的写盘就用空 store 覆盖掉用户的全部 provider 与绑定。而备份失败的场景（磁盘满、无权限）恰恰是写盘最可能出问题的场景。
- 决策：`load_or_migrate_store_v4` 先取两份备份——rolling `.bak.<ms>` 和**永不参与清理**的 `model_providers.v3.json`——任一失败即返回 `StoreError::BackupFailed` 并保持 v3 运行；已存在的 v3 快照绝不被二次迁移覆盖。写盘后立即读回比对，不一致就从永久备份还原。`read_store_v4` 对损坏文件返回 `StoreError::Corrupted` 并原样保留文件，由命令层交给用户决定「打开文件 / 从备份恢复 / 重置」。
- 后果：获得——迁移不可能在无备份的情况下发生，损坏文件不再被静默替换，迁移报告的「撤销」按钮有真实依据。承担——首次 v4 启动多出两次文件拷贝；调用方必须处理 `StoreError` 的四个变体而不能再假设读取总成功。
- 证据：`crates/skillstar-models/src/providers/store_v4.rs` 与 `providers/tests/store_v4.rs`（`corrupted_store_returns_error_and_keeps_file`、`migration_aborts_when_the_backup_cannot_be_written`）。

## D-037：诊断类命令按 `provider_id` 取凭据，明文 key 不过 IPC

- 日期：2026-08-15
- 状态：accepted
- 背景：`test_provider_connection` / `fetch_provider_models` / `fetch_provider_model_catalog` / `query_provider_balance` / `test_endpoints_latency` / `test_provider_latency` 都把明文 API key 当参数收。这意味着渲染进程必须持有 key、把它放进 query cache、并在每次探测时经 IPC 送回后端——每一处都是 key 可被观测的地方，而后端本来就拥有 key 所在的 store。同时前端还自行拼 `models_url` 的兜底规则，与后端各写一份。
- 决策：这六条命令改收 `provider_id`，由 `providers::resolve_connection` 在后端解析端点与凭据。`test_endpoints_latency` 保留 `urls` 参数——它的语义就是比较「行当前并未指向的候选 URL」。`EnvVar`/`File`/`Command` 三种间接凭据在此**不展开**：展开等于把本机环境烤进一次可能在别处执行的探测，那是 writer 在同步时该做的事。
- 后果：获得——明文 key 不再离开拥有它的进程；`models_url` 兜底规则只剩一份实现；前端传了一把与磁盘漂移的 key 这类 bug 被消除。承担——(1) 探测的是**已保存**的连接，因此草稿态必须先落盘再探测（`AppAiModelsPicker` 已按此顺序调整），这与 §4.5「凭据显式提交」的保存策略一致；(2) 未保存的行无法被探测，这是有意的。
- 证据：`crates/skillstar-models/src/providers/secret_resolve.rs`、`src-tauri/src/commands/models_commands/diagnostics.rs`、`crates/skillstar-app/src/models/dto.rs`（`provider_dto_never_contains_plaintext_secret`）。

## D-038：Codex 绑定按能力位门禁，迁移主动修复已写坏的磁盘配置

- 日期：2026-08-15
- 状态：accepted
- 背景：Codex ≥0.95 从 `WireApi` 枚举里删掉了 `Chat`，只剩 `Responses`。SkillStar 的 `recommended_codex_defaults` 对任何不含 `api.openai.com` 的 base URL 返回 `("chat", "third_party")`，并且有八个真实 provider 的测试在锁死这个行为。后果不是「某个 provider 用不了」——`wire_api = "chat"` 反序列化失败会让整个 `config.toml` 解析不了，Codex 完全起不来，而用户在 SkillStar 里唯一能操作的杠杆正是那条产生它的绑定。
- 决策：三件事一起做。(1) 删除 `codex_wire_api` 字段与 `CodexSettings.wire_api`——它编码的是一个已经不存在的选择；writer 只会写 `responses`。(2) 「这家能不能接 Codex」变成关于 host 的事实，存在 `Provider.endpoints.openai_responses` 与 `ProviderCaps.responses_api` 上，由注册表的 `required_wire` 在绑定时门禁；没有 responses 端点的 host 在写盘时**整条跳过**而不是降级写入。(3) 迁移那一次运行主动清理磁盘：删掉不可写的 Codex 表（`unsync_codex_entry`，单条而非整体），把被删条目连同 provider 名与模型记进 `MigrationReport::codex_dropped`。
- 后果：获得——存量用户升级后 Codex 能重新启动；「不支持」从此可表达，UI 能在绑定前挡住不可能的组合而不是让用户事后发现。承担——(1) 之前「绑上了」的第三方 Codex 列变成禁用，这是修 bug 不是破坏，但必须由迁移报告解释清楚；(2) 判定依据是端点存在性，而 `Tri::Unknown` **从不**拒绝（迁移给每一行写的都是 `Unknown`，把「没探测过」当「不支持」会在升级时静默解绑所有人）；探测把 `Unknown` 变成 `Yes` 后即可恢复绑定，这是 WP-2B 的事。
- 证据：`crates/skillstar-models/src/tool_sync/migrate_configs.rs`、`crates/skillstar-models/src/tool_sync/tests/part6.rs`、`crates/skillstar-models/src/providers/crud_v4.rs`（`check_bindable`）。

## D-039：写盘行为的对照基线是旧代码实际跑出的字节，不是手写期望值

- 日期：2026-08-15
- 状态：accepted
- 背景：v3→v4 把 provider 行、绑定、角色和模型目录四处数据全部换了形状和存放位置，而这些数据的唯一用途是投影成六个 Agent 的配置文件。「换存放位置不改写盘结果」这句话，用手写断言是证不出来的：手写断言记录的是作者**以为**旧代码做了什么。
- 决策：从 v3 的最后一个提交拉一个 worktree，用同一份 fixture 跑真实 writer，把产物原样存进 `tool_sync/tests/golden_v3/`；新测试拿同一份 fixture 走真实迁移再走新 writer，逐字节比对。Codex 是唯一豁免（它的输出必须变，见 D-038），豁免范围收窄到「本来就写 `responses` 的 `api.openai.com` 行仍然逐字节相同」，其余变化在 `part6` 里按行为单独断言。fixture 目录从 formatter 的管辖范围里排除——格式化它就等于销毁它的用途。
- 后果：获得——三个真实回归当场暴露：OMP 角色名 `smol` 被规范化成 `fast` 后会写进 OMP 不认识的键、角色写入顺序随内部改名而重排、模型目录移出 store 后 OpenCode 块丢失 `limit`/`cost`。这三个都不会被任何手写断言发现。承担——fixture 与其构造函数必须逐字保持一致，否则比对失去意义；构造函数因此在测试里完整写出而不是复用 helper。
- 证据：`crates/skillstar-models/src/tool_sync/tests/golden.rs`、`crates/skillstar-models/src/tool_sync/tests/golden_v3/`。

## D-040：frontmatter 门禁以公开 Agent Skills 规范为准

- 日期：2026-08-19
- 状态：accepted
- 背景：D-019 把 Anthropic `quick_validate.py` 的尖括号限制当成通用生态规则，导致 Vercel 官方技能因 description 中合法的 `` `<ViewTransition>` `` 文本被拒绝；公开 Agent Skills 规范只要求 description 非空且不超过 1024 字符。扫描 UI 又把任何 issue code 固定解释成“缺少 name/description”，掩盖了真实原因。
- 决策：尖括号不再产生 frontmatter issue，也不阻断安装；其余门禁保持不变。扫描预览直接把后端 issue code 映射为具体本地化原因，不再重建或概括后端规则。此决策仅取代 D-019 的“description 含尖括号”条款。
- 后果：符合公开规范且描述中含 JSX、HTML 或占位符的技能可以安装；具体元数据问题仍 fail-closed，并在 UI 中准确显示。
- 证据：`crates/skillstar-skills/src/validation.rs`、`src/features/my-skills/components/import-modal/SelectSkillsPhase.tsx` 及对应回归测试。

## D-041：上游移除/更名在检查期可见，处理复用移除流程，迁移是一等操作

- 日期：2026-08-21
- 状态：accepted
- 背景：作者会删除、改名或把 Skill 移到别的桶（mattpocock/skills 的 in-progress 明说"可能随时变动或消失"）。此前这只在用户恰好更新同仓库别的 Skill、pull 之后才以阻塞对话框出现；改名则表现为"一个被删 + 一个新技能"，用户得自己卸旧装新并重配 Agent。
- 决策：更新检查把 tracked ref 上的路径消失记为 `upstream_change: removed`，并用 `git diff -M` / frontmatter `name` 判定后继；`update_state` 仍是唯一所有者，`update_available` 语义不变。移除的处理入口直接复用既有「来源已不再提供」对话框与 `resolve_skill_update`；改名由 `skillstar-app::skill_migration` 作为跨域 use case 一步完成（安装后继、沿用 Agent/项目部署、卸载旧条目）。不新增第二套"待处理"对话框，也不把不可更新项混进「更新 N 项」。
- 后果：用户在卡片上就能看到并处理上游变动；后继判定是启发式（`-M` 相似度或同名），判错时用户仍可走"移除 + 从 ghost 安装"的手动路径。迁移不是单事务：install 与 uninstall 各自持更新锁，中间失败按步骤报告，下一次检查会把残留旧条目标为 `removed` 供用户收尾。
- 证据：`crates/skillstar-skills/src/update_checker.rs`（`UpstreamStatus`）、`crates/skillstar-skills/src/update_state.rs`、`crates/skillstar-app/src/skill_migration.rs`、`src/features/my-skills/`。

## D-042：移除 skill-pack 功能残骸（CLI、store、读侧），而不是补全安装链

- 日期：2026-08-28
- 状态：accepted
- 背景：2026-08-27 对抗审查证实 pack 安装链(`install_pack`/`detect_pack`)全仓不可达且 `git log -S "PackAction::Install"` 为空——没有任何已发布版本写过 `packs.json`。删除安装链后,README 记录的 `skillstar doctor` / `pack list` / `pack remove` 只能对一个永远为空的 store 打印 "No packs installed."。二选一:补全安装链让功能成立,或整体移除。
- 决策：整体移除。删 CLI 三命令(`Doctor`/`Pack` clap 变体、`cmd_doctor`/`cmd_pack_list`/`cmd_pack_remove`)、`skill_pack` 模块读侧(`list_packs`/`remove_pack`/`doctor_*` + store 类型)、`paths::packs_path` 与 legacy `packs.json` 迁移行、README 条目。理由:功能从未对用户成立(store 从未被写入),不存在破坏;真正的技能打包分发已由 bundle(.agd)与 share code 覆盖。
- 后果：`skillstar` CLI 少三个从未产生过效果的子命令;`.claude-plugin` 式 pack 若将来要做,从 git 历史(本决策前一提交)取回并重新设计,而不是在空 store 上续写。`marketplace_pack*` 表的 DDL 按迁移不可删原则保留为惰性 schema。
- 证据：`crates/skillstar-app/src/cli/{mod,manage}.rs`、已删除的 `crates/skillstar-skills/src/skill_pack*`、`README.md`、2026-08-27/28 对抗审查记录(errors.md 同日条目)。

## D-043：Agent 技能主开关持久化恢复意图，而不持久化 Agent 归属

- 日期：2026-08-30
- 状态：accepted
- 背景：Settings 的旧「所有已安装技能」主开关用当前 Hub inventory 推导动作：部分状态会补齐 Hub 缺口，全量关闭会清空当前链接。它既不能表达「只暂时停用本目录原有集合」，也在刷新或重启后不知道该恢复哪些名字。把集合按 Agent id 保存同样是错误模型：多个 profile 可以合法解析到同一个物理 Global skills 目录，磁盘没有 entry 的 per-Agent ownership 事实（D-024）。
- 决策：`profiles.toml` 增加 recovery-only journal，键是暂停当刻解析到的物理 Global skills 目录，值是**仍待恢复**的排序去重 Skill 名称。`skillstar-app::agent_managed_skills` 在首次停用前原子落盘精确活动集合，再逐项临时移除；完成后只保留实际已消失的名字。若 journal 存在，恢复只尝试其中目前仍缺失的名字；成功或用户手动放回的名字删除，Hub 源缺失、受保护冲突或失败则保留。当前活动集合永远从磁盘重读，journal 不作为链接真相，也不把目录 entry 归给任一 profile。路径后来解析到其他地方时不得按 id 迁移或重新部署旧 journal。
- 后果：获得——暂停/恢复跨刷新与重启保持精确集合，且恢复路径没有枚举 Hub inventory 的入口；共享目录天然共用状态和 pending 范围。承担——恢复源已从 Hub 删除时会保留可重试项而非“看似完成”；journal 是 D-024「磁盘即真相」的狭窄例外，只表达用户主动发起的恢复意图，因此必须在每个动作后以磁盘状态收敛，不能扩张为 ownership/provenance store。
- 证据：`crates/skillstar-skills/src/agents/profile_storage.rs`、`crates/skillstar-app/src/agent_managed_skills.rs`、`src-tauri/src/commands/agents.rs`、`src/features/settings/lib/agentSkillSync.ts`。

## D-044：pack 根目录 SKILL.md 垫片不是安装单元

- 日期：2026-08-30
- 状态：accepted
- 背景：impeccable 风格的技能包（如 `xxww0098/rust-skills`）把正文放在 `skills/<name>/`，同时在仓库根放一份同 identity 的 `SKILL.md`，好让一层扫描器把整仓当作技能目录。SkillStar 的 root-first 发现把根 `SKILL.md` 当成唯一技能，`folder_path` 为空就会把整个仓库（测试、脚本、各 harness 副本）链接进 Hub。全深度扫描也会因根路径优先级 4 赢过 `skills/`。
- 决策：发现阶段先剥掉「根 SKILL.md 与同 identity 嵌套 catalog **或** harness 副本」的垫片，再执行 root-first / 去重。`skills/` 与 `source/skills/` 同为规范目录，优先级高于 `.cursor/skills`、`.dsh/skills`、`.claude/skills` 等 harness 副本。真正的单技能仓库（根 SKILL.md 没有同名嵌套副本）行为不变。`.claude-plugin/plugin.json` 的 `skills` 同时接受字符串路径和数组。
- 后果：`skillstar add xxww0098/rust-skills` 安装 `skills/rust`（若 catalog 存在），不再把整仓当技能。只有 harness 副本、没有 catalog 时也安装该副本而不是整仓。一层扫描器仍可继续使用根垫片。
- 证据：`crates/skillstar-skills/src/{pack_layout.rs,discovery.rs,plugin_manifest.rs}` 及 `pack_root_shim_installs_canonical_skills_folder` / `root_shim_plus_harness_copies_does_not_install_the_repo_root` 测试。

## D-045：多 harness 技能包按 `.<harness>/` 安装

- 日期：2026-08-31
- 状态：accepted
- 背景：rust-skills / impeccable 这类包在每个 Agent 目录下各放一份独立技能。卡片 SVG 轮播原先只 `toggle` 已安装链接，Install 按钮走 `install_skill(url, name)` → 发现后 `global_deploy` 到全部已启用 Agent。发现层又漏了 `.cursor/skills` 与 `.dsh/skills`，根垫片会把 `source_folder` 写成空，Hub 链整仓。
- 决策：安装单元是含 `SKILL.md` 的 harness 技能目录（或 harness 根，若那才是单元）。未指定 harness 时 catalog `skills/<name>/` 优先；点轮播图标或 CLI 显式单个 `--agent` 时该 harness 文件夹赢。没有该文件夹时的回退见 [D-046](#d-046已安装轮播从-repo-cache-部署且缺-harness-时回退)。稀疏检出保留全部嵌套 `SKILL.md` 父目录，不再按 basename 丢掉 `.agent` / `.agents`。
- 后果：同一 Hub 名仍只有一条 lock，`source_folder` 跟随最近一次明确请求的 harness。复用仅当现有 `source_folder` 已经是该次解析到的文件夹；否则从同一 clone 改指向（不二次 clone），并先把已链到其他 Agent 的链接钉到当前 payload。轮播未链接图标走 `install_skill(url, name, agentId)`，不得只 `toggle` 当前 Hub。缺 harness 文件夹的行为已由 D-046 修正，不再 fail-closed。
- 证据：`resolve_install_skills`、`existing_same_repo_action`、`pin_existing_global_links_to_current_source`、`install_skill(..., agentId)`、`AgentTargetCarousel` 接线测试、`stale_dsh_link_is_rewritten_to_requested_harness`。

## D-046：已安装轮播从 repo-cache 部署且缺 harness 时回退

- 日期：2026-08-31
- 状态：accepted
- 背景：D-045 让未链接轮播图标走完整 `install_skill`。`clone_or_fetch` 在 cache 已有 `.git` 时仍 `git fetch --depth 1` + reset，已装 rust-skills / impeccable 点第二个图标像重装。同时 D-045 对缺 `.<harness>/` fail-closed，impeccable 没有 `.dsh` 时点 DeepSeek 报错，用户无法把技能落到 `~/.dsh/skills/<id>`。
- 决策：hub 已装且 repo-cache 已有 clone 时，轮播 / 显式单个 `--agent` 只扫描现有 checkout（`cached_repo_dir_if_present`），不 clone、不 fetch；`source_folder` 没变就不改 lock。cache 缺失才 fetch。请求的 harness 文件夹不存在时按顺序回退：规范 `skills/<name>/` 或 `source/skills/` → 已装则用现有 hub `source_folder` → 同 identity 的另一份嵌套 harness 副本。把该 payload 部署到被点 Agent。禁止 `source_folder: None` 整仓，禁止静默 no-op。只有完全没有嵌套 `SKILL.md` 才失败。
- 后果：已装卡的常见轮播点击是 cache-local 部署/改指向。Impeccable 点 DeepSeek 会把已有 skill 文件夹链到 `~/.dsh/skills/impeccable`，不再报「没有 `.dsh`」。首次安装和 cache 被删后的重装仍走网络。
- 证据：`scan_repo_preferring_local_cache_in_session`、`resolve_install_skills` 回退、`installed_rust_skills_deepseek_retargets_from_cache_without_clone`、`installed_impeccable_deepseek_falls_back_to_a_skill_folder`、`missing_git_cache_still_fetches_for_harness_install`。

## D-047：技能安装是 vercel-skills 五步管线，harness 文件夹是 identity 别名

- 日期：2026-08-31
- 状态：accepted
- 背景：CLI、Tauri、轮播、batch 和整仓 clone 回退各自选文件夹，shim / catalog / harness 扫描器重复。用户要的是 `npx skills add` 那条管线，不是第六条路径。
- 决策：所有 git/local 安装走同一入口 `skill_install::install_from_source`：1. `Source::parse` 解析 `owner/repo`、URL、tree URL、本地路径；2. 发现含 `SKILL.md` 的目录；3. Hub 只链所选文件夹；4. Agent 目录 symlink（Windows 必要时 copy）；5. 调用方决定 project vs global。`.<harness>/skills/<name>` 与 `skills/<name>/` 是同一 identity：`--agent X` 优先该 harness，否则 catalog，否则现有 hub，否则另一份 harness 副本。没有 `SKILL.md` 才失败。禁止整仓 clone 回退。
- 后果：本地路径和 Git URL 产物一致。rust-skills / impeccable / ui-ux-pro-max-skill 都装得上。已删除 git ref 与 prefetch 失败仍按 D-046 / errors.md 处理。share-code 的 embedded 分支不是第六条 git 安装。
- 证据：`install_from_source`、`resolve_install_skills`、`install_pipeline_table_chooses_harness_or_fallback_folder`。

## D-049：吸收通不过 deletion test 的浅 crate

- 日期：2026-09-01
- 状态：accepted
- 背景：D-002 规定 crate 只在独立变更节奏、依赖集合或 deletion test 证明有收益时才拆出。审查时 `skillstar-agents`（约 1.7k 行，只与 skills 同编译）、`skillstar-github-auth`（约 1.9k 行，skills 与 channels 都已依赖 skills）和 `skillstar-providers`（约 327 行静态表，models/usage 本就依赖 core）都通不过 deletion test：没有独立第三方依赖墙，也没有环。另有 `skillstar-sync → skillstar-skills` 幽灵 path dep（源码零引用）已由前置提交拆除。`skillstar-channels` 与 `skillstar-git` 仍独立——前者用 `SkillMutationPolicy` 打破环，后者把 `gix` 挡在 skills/sync 编译单元之外。
- 决策：`agents` 与 `github_auth` 收进 `skillstar-skills` 的公开模块；Provider identity/balance 收进 `skillstar-core::providers`。对外路径改为 `skillstar_skills::agents`、`skillstar_skills::github_auth`、`skillstar_core::providers`。`check_workspace_deps.sh` 拒绝这三个旧包名再现。不把 channels 或 git 并进 skills。
- 后果：workspace 从 13 个成员减到 10 个；channels 不再直连认证叶子；models/usage 不再多一跳 providers crate。D-004 的 SSOT 与「无产品域依赖」不变量保留在 `skillstar-core::providers` 模块（模块本身仍无产品域依赖，只是不再是独立 crate）。新增浅 crate 必须先过 deletion test。
- 证据：`crates/skillstar-skills/src/{agents,github_auth}/`、`crates/skillstar-core/src/providers/`、`scripts/internal/check_workspace_deps.sh`、本决策对应提交。

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

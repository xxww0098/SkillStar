# 架构决策记录

状态：active

这里只记录长期有效、影响多个改动的选择。当前结构以 [boundaries.md](./boundaries.md) 为准；实现行为以对应功能文档和代码为准。

D-091 之前的条目可能引用 `src/`、`src-tauri/` 或 Tauri command。那些路径已经删除，只作为当时的证据，不是当前代码。

## D-001：GUI 与 CLI 共享一个二进制和域实现

- 日期：2026-07-10
- 状态：superseded by [D-091](#d-091退役-tauri-与-react全面转向-gpui)
- 背景：独立 CLI package 会复制入口、依赖和跨域流程。
- 决策：可执行文件只由一个 package 产出；启动时识别 CLI/GUI 模式。CLI 和 GUI 调用同一域 facade 或 `ss-app` use case。产出 package 在仓库根，入口是 `src/main.rs`。
- 后果：`ss-app` 保持 library-only；不得重新添加第二个产品二进制。
- 证据：`src/main.rs`、`crates/ss-app/src/cli/`。

## D-002：module-first，满足晋升条件后才拆 crate

- 日期：2026-07-10
- 状态：accepted
- 背景：“一个前端 feature 一个 crate”造成浅模块、编译扇出和双向依赖压力。
- 决策：新能力先进入最内聚的现有 crate，以私有 module + 窄 facade 暴露。只有独立变更节奏、依赖集合或 deletion test 证明收益时才建立新 crate。
- 后果：前端切片与 Rust crate 不要求一一对应；crate 数不是架构质量目标。
- 证据：Workspace Wave 1/2 迁移，提交 `77ed14c`、`871d0c6`、`3ea5f24`。

## D-003：命令层保持薄，跨域编排归 `ss-app`

- 日期：2026-07-10
- 状态：superseded by [D-091](#d-091退役-tauri-与-react全面转向-gpui)
- 背景：把 use case 放在壳的适配层会让 CLI 无法复用，也容易制造反向依赖。
- 决策：壳只做展示和调用；单域逻辑归域 crate；多域事务归 `ss-app`。
- 后果：新增 GPUI 页面不代表新增业务实现；代码审查应检查事务是否下沉到正确层。
- 证据：`crates/ss-app/src/`、`crates/ss-gpui/src/`。

## D-004：Provider 元数据使用零依赖叶子

- 日期：2026-07-10
- 状态：accepted（crate 形状被 [D-049](#d-049吸收通不过-deletion-test-的浅-crate) 吸收；元数据 SSOT 与「无产品域依赖」不变量仍有效）
- 背景：Models preset 与 Usage catalog 都需要 Provider identity/鉴权事实，但二者不能相互依赖。
- 决策：canonical identity、鉴权和余额端点元数据只保存在一处；Models 与 Usage 分别从它派生自己的产品注册表，并用测试锁定映射。该处最初是零依赖 crate `skillstar-providers`；D-049 把它收成 `ss-core::providers`，模块本身仍不依赖任何产品域。
- 后果：添加 Provider 先修改 identity；不得在命令层或前端复制鉴权规则。
- 证据：`crates/ss-core/src/providers/`、Models/Usage guard tests。

## D-005：技能部署采用 link-first、copy fallback

- 日期：2026-06-10
- 状态：accepted
- 背景：symlink 能保持项目干净和自动跟随更新，但 Windows 权限或文件系统可能不允许。
- 决策：部署按 symlink → junction → copy 的能力阶梯执行；reconcile 和更新必须认识实际部署类型。
- 后果：文档和 UI 不能宣称“纯 symlink”；copy 需要 stale hash 刷新，失败不能破坏现有部署。
- 证据：`crates/ss-skills/src/deployment/`、提交 `7fde474`。

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
- 证据：`crates/ss-skills/src/agents/`、`crates/ss-skills/src/projects/`、`crates/ss-app/src/cli/` 及对应测试；上游设计参考 `vercel-labs/skills` 的 `add`、`agents`、`installer`。

## D-008：内置 Agent 以 vercel-labs 注册表为兼容基线，品牌图标统一投影

- 日期：2026-07-14
- 状态：accepted
- 背景：逐个手工接入 Agent 会让路径、能力和图标清单各自漂移；部分上游 Agent 只有项目级目录，不能伪造全局目标。
- 决策：`ss-skills` 的内置注册表同步 `vercel-labs/skills/src/agents.ts` 的 Agent 能力，并以测试锁定上游 id 覆盖；SkillStar 既有持久化 id 通过 CLI 兼容别名承接。内置品牌图标只通过 `@lobehub/icons` 的集中适配层渲染，无专属品牌时使用 LobeHub 通用图标。
- 后果：上游新增/修改 Agent 时必须在同一变更中同步路径、别名、图标映射与文档；项目级 Agent 会被全局操作明确拒绝。前端不维护第二份 SVG 资产目录，8 字段 `AgentProfile` IPC 保持稳定。
- 证据：`crates/ss-skills/src/agents/builtin.rs`、`src/components/ui/icons/agentIcons.ts` 及覆盖测试。

## D-009：本机 Agent 采用纯手动激活，不推断系统安装状态

- 日期：2026-07-14
- 状态：accepted
- 背景：binary、桌面应用、配置根和 skills 目录都不是可靠的 Agent 身份证据；尤其多个 Agent 共享 `~/.agents/skills` 时，部署残留会造成误发现、误启用和卡片 rail 泄漏。
- 决策：删除本机 Agent 安装探测及其注册表元数据。所有 profile 默认关闭，Settings 持久化开关是本机 Agent 激活的唯一来源；CLI 隐式目标与所有本机 rail 都只消费该状态。冻结 `AgentProfile.installed` 在兼容期镜像 `enabled`，不再承载安装事实。
- 后果：SkillStar 不会替用户判断 Agent 是否存在；用户可提前启用目标，实际部署或同步失败在动作边界显式返回。共享目录不再影响 profile 可见性，新增 Agent 也无需维护探测规则。
- 证据：`crates/ss-skills/src/agents/`、`src/lib/agentProfiles.ts`、Settings 与 rail 回归测试。

## D-010：Skill 教程使用 ACP 全目录快照与版本化 HTML artifact

- 日期：2026-07-14
- 状态：superseded（被 [D-053](#d-053移除学习功能与-skillstar-learning) 取代）
- 背景：只翻译 `SKILL.md` 无法解释 scripts、references、assets 等完整 Skill 行为，provider 翻译缓存也不能表达“教程是否仍对应当前目录版本”。模型输出 HTML 又不能直接进入应用 DOM。
- 决策：移除 SKILL.md 翻译功能。教程生成以 `ss-skills::content` 的完整递归快照和确定性内容 hash 为输入，通过用户显式配置的 ACP Agent 分析；后端只接受自包含、无脚本、覆盖全部文件清单的 HTML，并与 hash、教程风格、完整 prompt bundle hash/schema 版本 metadata 一起原子持久化。风格来自 Settings 中的受控注册表，每种风格使用独立 prompt 片段；前端用 sandbox iframe 展示。
- 后果：教程能覆盖整个 Skill 并跨重启复用；任何内容、规范化界面语言、所选风格或生成契约变化都会产生 stale 提醒，刷新失败仍保留旧版。生成成本和时延高于翻译，编辑器必须先保存，ACP 未启用时不能生成新教程。`AiConfig` 不再保存 `target_language` / `short_text_priority`；Skill 摘要与教程一样只消费当前界面语言，旧 `ai.json` 里的这两个字段读入时忽略、下次保存时丢弃。
- 证据：`crates/ss-skills/src/{content,tutorial}.rs`、`src-tauri/src/core/skill_tutorial.rs`、`src-tauri/prompts/acp/skill_tutorial.md`、`skillstar-models::ai_provider::{summarize_text,language_display_name}`、Skill 教程面板回归测试。

## D-011：删除设备指纹功能，Usage 请求回归统一代理 client

- 日期：2026-07-23
- 状态：accepted
- 背景：设备指纹（TLS/HTTP2 伪装、浏览器 preset、IDE telemetry 投影、订阅级 fingerprint 绑定）横跨 usage crate、Tauri 命令、Settings 与订阅编辑四层，并通过 `impersonate` feature 引入 `wreq`/`wreq-util` 两个 rc 版依赖。它伪装的是客户端身份而非解决额度抓取本身的问题，收益不足以支撑这条贯穿全栈的接缝。
- 决策：整体删除 `ss-usage::fingerprint`、`src-tauri/src/commands/fingerprints.rs`、`src/features/usage/fingerprints/` 及 `Subscription.fingerprint_id`。原 `fingerprint::request` 的请求构建器保留为 `ss-usage::request`（去掉 wreq 分支），所有 fetcher 统一走 `http_client::usage_http_client()`。`impersonate` feature 与 wreq 依赖一并移除，`check_workspace_deps.sh` 中相关守卫同步删除。
- 后果：额度抓取以 reqwest 默认 ClientHello 出网，若某 provider 将来按 TLS 指纹拦截，需要另行决策而不是恢复本模块；`~/.skillstar/config/fingerprints.json` 成为孤儿文件，不再读写，也不做迁移删除。构建图少两个 rc 依赖，Usage 的请求路径只剩一条。
- 证据：`crates/ss-usage/src/request.rs`、`crates/ss-usage/src/http_client.rs`、`scripts/internal/check_workspace_deps.sh`。

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
- 决策：`ss-skills` 为每次远程 Git 动作建立唯一 operation session，并以当前 SkillStar 可执行文件作为临时 `GIT_ASKPASS`。token 只存在于该 Git 进程及 askpass 子进程继承的专用环境变量，永不进入 argv 或持久文件；Git 强制关闭终端和 credential-manager 交互，操作完成或取消后随进程环境销毁。认证仅适用于规范化的 `https://github.com/` 远端，认证操作禁用 GitHub 镜像但临时注入当前 SkillStar 代理。session 的进度、错误和调试表示统一脱敏。
- 后果：同一 Skills 域 facade 可供 GUI 与未来 CLI 使用，并可用 fake credential/transport 验证凭据生命周期。SkillStar 可执行文件必须保留内部 askpass 入口；所有新增远程 Git 动作必须通过 operation session，不能直接启动裸 Git 网络命令。进程环境本身属于敏感边界，崩溃报告和诊断不得采集该专用变量。
- 证据：issue #20、`crates/ss-skills/src/github_auth/`、`src-tauri/src/commands/github/auth.rs`、Settings GitHub 登录测试。

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
- 决策：删除 `ss-sync` 的 S3 全部代码（client/store/manifest/local_pack/sync/types、`s3_sync.rs` 命令、`src/features/s3/`、IPC 契约与 i18n）。跨设备/团队共享统一走 GitHub 共享频道；SSH 保留为个人服务器部署路径。
- 后果：个人多设备同步依赖 GitHub 仓库/组织（无 GitHub 场景失去该能力）；S3 兼容 endpoint（MinIO/R2/OSS）不再可用；`ss-sync` 成为 SSH-only crate。不再新增 S3 类对象存储后端。
- 证据：crates/ss-sync/src/（仅 ssh）、docs/features/sync/README.md、commit 待定。

## D-017：OMP（Oh My Pi）注册为独立内置 Agent，仅 Skills 分发

- 日期：2026-08-10
- 状态：accepted
- 背景：OMP（`@oh-my-pi/pi-coding-agent`，命令 `omp`）与 Pi（`@earendil-works/pi-coding-agent`，命令 `pi`）是同源但独立的产品：配置根 `~/.omp` 与 `~/.pi` 互不读取，OMP 的模型配置是 `~/.omp/agent/config.yml`（modelRoles）+ 自有 models.db 目录，不读 `~/.pi/agent/models.json`/`settings.json`，技能位置是 `~/.omp/agent/skills`（全局）与 `.omp/skills`（项目）。此前 SkillStar 只注册 Pi，OMP 用户的技能发现、链接与部署全部失效。
- 决策：在 `ss-skills::agents::builtin` 的 extension 区（与 `grok` 并列，不在 vercel-labs 上游 id 内）注册 `omp`（显示名 Oh My Pi）：全局 `~/.omp/agent/skills`，项目 `.omp/skills`；`ss-skills::discovery` 优先级目录加入 `.omp/skills`。`~/.omp/agent/managed-skills`（OMP Auto-Learn 自动生成）不纳入发现与部署。轴②（Models 工具同步）暂不接入——OMP 的 provider 注入 schema（config.yml modelRoles / models.db）与 Pi 不同，待调研后另行设计。
- 后果：OMP 用户可手动激活并在 Settings / Projects / My Skills 中链接技能；OMP 不进模型绑定矩阵（`ProviderToolId` / AGENT_SPECS）；与 `grok` 同为 SkillStar extension，同步上游注册表时不受影响。
- 证据：`crates/ss-skills/src/agents/builtin.rs`、`crates/ss-skills/src/discovery.rs`、`src/components/ui/icons/agentIcons.ts` 及覆盖测试；本机 `~/.omp/agent/`（config.yml、models.db、managed-skills）与 `~/.pi/agent/` 布局实证。

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
- 决策：新增 `ss-skills::validation` 单一 frontmatter 解析/校验实现（discovery 复用同一解析器）。阻塞级问题：description 缺失/非字符串/超 1024 字符/含尖括号、name 超 64 字符、frontmatter 缺失或 YAML 损坏 —— 在 repo-scan 安装（`scan_install`，覆盖 GUI/CLI/marketplace/频道/卡组）、pack 安装、bundle 导出/导入、本地目录采用（`adopt_folder`，改为完整目录复制）处 fail-closed，错误列出全部失败技能与可行动原因。咨询级问题（name 缺失回退目录名、非 kebab-case）不阻塞，经 `DiscoveredSkill.frontmatter_issues` 传入前端展示。本地创作/分歧副本（`local_skill::create`/`create_from_snapshot`）不做门禁，避免阻止用户对已损坏内容的保存流程。CLI 本地目录安装删除重复实现，改用 `adopt_folder` facade。
- 后果：新装/打包/采用技能保证可触发、有描述；批量安装中单个无效技能按项失败且不阻断其余；GUI 扫描预览对无效技能显示警告标记。既有已安装的无描述技能不受影响（门禁只作用于写入路径）。adoption 现在保留完整技能目录而非仅 SKILL.md。
- 证据：`crates/ss-skills/src/{validation,plugin_manifest,discovery,scan_install.rs,skill_pack.rs,skill_bundle.rs,local_skill.rs}.rs`、`crates/ss-app/src/cli/install.rs`、`src/features/my-skills/components/import-modal/SelectSkillsPhase.tsx` 及覆盖测试。

## D-020：更新检测采用 GitHub API 快速路径，凭据不出 Git session

- 日期：2026-08-10
- 状态：accepted
- 背景：patrol/批量刷新每小时对每个唯一 repo 执行 `git fetch`，只为判断是否有技能内容变化（fetch 后还需本地 rev-parse 对比 subtree hash）。`npx skills` 用 GitHub Trees API 的目录 tree SHA 直接做远程对比，避免包传输。SkillStar 的 Git 认证纪律（D-014）要求 token 只在 askpass 子进程环境内存在。
- 决策：新增 `ss-skills::update_api`：对 `github.com` 来源且能本地解析远端 ref（pinned ref 或 `origin/HEAD` symbolic ref）的 repo，每个 cycle 至多 40 个 repo、并发 8、超时 10s，以 `GET /repos/{o}/{r}/git/trees/{ref}`（非递归）获取顶层树；目录条目 sha 即本地对比所需的 subtree hash。成功 → 该 repo 跳过 prefetch fetch，`check_update_local_with_api` 对比本地 HEAD subtree hash 与 API hash（缺失目录 → None 保留徽标）。任何失败（私有仓库 404、限流 403、网络、truncated）→ 回退既有 git fetch 路径。HTTP Bearer 只在用户**已经**持有 SkillStar GitHub App session 时附带，把额度从匿名 60/h/IP 提到认证 5000/h；未登录保持匿名。凭据不出 Git session（D-014）。`X-RateLimit-Remaining: 0` / HTTP 429 写入进程内冷却至 `X-RateLimit-Reset`，本 cycle 未发出的 API 调用直接跳过；401/403/404/限流是设计内回退，记 debug 不记 warn。`prefetch_unique_repos_in_session_skipping` 与 `check_update_local_with_api_entry` 保持既有 None/Some 契约与 revision 裁决。
- 后果：github.com 公共来源的更新检测从网络 fetch 降为一个轻量 API 调用；已登录用户不再把同 IP 的匿名额度打爆；私有/非 github 来源继续走 git fetch。API 故障只影响速度不影响正确性。匿名限流（60/h/IP）由每 cycle 40 repo 上限、认证回退与冷却窗口共同兜底。
- 证据：`crates/ss-skills/src/{update_api.rs,update_checker.rs,installed_skill.rs}` 及 `api_remote_hashes_drive_update_detection_without_fetching` 等测试。

## D-021：仓库发现对齐生态容器目录深度与 Claude Code 插件清单

- 日期：2026-08-10
- 状态：accepted
- 背景：SkillStar 的 priority 容器目录只扫描一层直接子目录，`skills/<category>/<name>/SKILL.md` 这类 catalog 布局在已有扁平技能时被漏掉（`npx skills` 走 3 层且浅层技能遮蔽嵌套）；Claude Code 插件生态（anthropics/skills、daymade、Sylph 官方仓库）用 `.claude-plugin/marketplace.json`/`plugin.json` 声明技能路径，SkillStar 完全不读。
- 决策：`discovery::scan_priority_skill_dirs` 改为对每个容器目录最多走 3 层，含 SKILL.md 的目录遮蔽其下内容；仓库根保持 1 层。新增 `ss-skills::plugin_manifest`：读取 `.claude-plugin/marketplace.json`（`pluginRoot` + 本地 `./` 前缀 `source`/`skills[]`，跳过远程 source）与 `plugin.json`，在路径包含性与 `./` 前缀校验后，把声明的技能父目录以 depth-1 加入扫描。
- 后果：catalog 布局仓库在 root-first 模式即完整发现；Claude Code 插件市场仓库的声明技能可被扫描、预览与安装。manifest 只读取技能位置，不执行插件安装逻辑；越界/非 `./` 路径被拒绝。
- 证据：`crates/ss-skills/src/{discovery.rs,plugin_manifest.rs}`、`depth_and_plugin_tests` 与 `plugin_manifest` 测试。

## D-022：门禁补齐、死代码清理与单一名称解析（编排审查轮）

- 日期：2026-08-10
- 状态：accepted
- 背景：三名只读审查 agent（安装路径/死代码/解耦）确认 D-019 门禁在 scan_install/bundle/pack/adopt_folder/频道安装处生效，但发现三个可绕过入口：`install_skill` 的直接 clone 回退把门禁失败当 Ok(None) 后整库克隆且不校验；share_install embedded 走未门禁的 `local_skill::create`；projects/import 直接调 `adopt_existing_dir_locked` 不校验。同时：CLI 的 `find_target_skill_preview` 与域版存在已证实的语义漂移（显式 name 不匹配时预览误报 would-be-installed），`derive_name_hint` 双实现，`src-tauri/Cargo.toml` 把 4 个无条件 import 的内部 crate 声明成 macOS-only 目标依赖（Linux/Windows CI 提交后必红）。
- 决策：① 三个绕过入口全部接入 `validation::ensure_installable`：直接 clone 后校验、失败删克隆并返回可行动错误；embedded 创建后校验、失败回滚删除；项目导入逐技能校验、失败跳过并告警。② 删除经全仓 grep 证实的死代码：`adopt_existing_dir`/`validate_agent_ids`/`card_window_labels`/patrol 两个 sessionless wrapper/`normalize_repo_url` shim/S3 路径 helper/`TutorialLoadResult` 别名/`SkillCandidate.skill_md_path` 死字段/`shared.rs` 与 `src-tauri/src/core/path_env.rs` 两个一行垫片/根 re-export 裁剪（仅留 `Skill`/`SkillContent`/`discover_skills`）。③ 名称解析单一化：`source_resolver::derive_skill_name_hint`（Source-aware）成为唯一实现，域安装与 CLI 共用；`find_target_skill` 提升为 pub，CLI 删除内联副本。④ 4 个内部 crate 依赖移入通用 `[dependencies]`。
- 后果：门禁对任何安装入口都 fail-closed；预览与实装不再出现结论分歧；非 macOS 平台构建不再缺 crate；`ss-skills` 对外根路径只剩 3 个 re-export。死代码删除均为全仓零调用验证，不影响行为；`AgentProfile.installed`、`ss-git` sessionless wrappers、repo_history 写路径、ACP full-access 分支按审查结论保留待后续确认(2026-08-27 对抗审查轮已处理其中两项:sessionless wrappers 经全仓零调用复核后删除;repo_history 写路径在 `scan_github_repo` 成功分支接线,历史不再只读不写)。
- 证据：`crates/ss-skills/src/{skill_install,share_install,validation,source_resolver,discovery,local_skill}.rs`、`crates/ss-skills/src/projects/import.rs`、`crates/ss-app/src/cli/`、`crates/skillstar-channels/src/patrol/`、`src-tauri/Cargo.toml`、`direct_clone_gate_tests` 等测试；编排 run `run_3c969aa725f6`（REVIEW-A2/B/C2）。

## D-023：拉取多源化：Git mirror 候选链 + Marketplace host 链 + 内容寻址增量

- 日期：2026-08-10
- 状态：accepted（② 的自定义 `config/marketplace_mirror.json` 已移除。商店 host 链现在只是主站，外加启用 GitHub 加速时的包装地址；现行契约见 [marketplace](./features/marketplace/README.md#多源拉取与内容寻址)）
- 背景：对抗审查场景下任何单一远端都是单点故障：`skills.sh` 或 `github.com` 被 DNS 污染/SNI 阻断后，商店拉取与技能安装/更新整条链路不可用；已有 `github_mirror` 只支持一个 mirror，失败仅回退直连。同时快照同步每次全量 delete+reinsert，无法判断"远程内容是否变化"，也没有来源记录可供审计。
- 决策：① Git mirror 从单值扩展为候选链：`candidate_mirror_urls()` 返回"custom → 选中 preset → 其余 presets（去重、规范化）"，`ss-git` 的 transport/ops 对每个候选逐个尝试，全部失败才回退直连 GitHub；带凭据操作仍禁止走 mirror。② Marketplace 拉取增加 host 链：`remote::marketplace_hosts()` 以 `https://skills.sh` 为首，按 `config/marketplace_mirror.json` 追加镜像；`fetch_with_failover` 按序尝试、失败降级，并返回 `FetchMeta{payload_sha256, source_host, etag}`。③ sync_state schema v11 新增 `source_host`/`payload_sha256`/`etag` 列；快照同步与 MCP registry 同步接入内容寻址增量写：payload 未变化（304 或 sha256 相同）时只更新时间戳、保留旧数据与指纹，跳过全量重写。
- 后果：任何单个 mirror/host 失效都能自动降级到下一个候选，技能安装/更新与商店拉取在审查环境下可恢复；快照写放大显著下降且可审计数据来源。副作用：同步语义从"总是重写"变为"内容寻址增量"，依赖指纹正确性（sha256 冲突风险可忽略）；host 链按序尝试增加了失败时的延迟。
- 证据：`crates/ss-core/src/config/github_mirror.rs`、`crates/ss-git/src/{transport,ops}.rs`、`crates/ss-marketplace/src/remote/mod.rs`、`crates/ss-marketplace/src/snapshot/{sync,sync_state,migrations}.rs`、`v11_migration_adds_content_addressing_columns` 等测试。

## D-024：共享 skills 目录塌缩为部署目标，归属改由链接指向 hub 推导

- 日期：2026-08-12
- 状态：superseded（全局侧由 [D-081](#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线) 消解：canonical 即 `~/.agents/skills`；项目侧缺口仍由本条约束）
- 背景：`BUILTIN_AGENT_DEFS` 的 74 个内置 Agent 中，多组解析到**同一个物理目录**：Global 侧 `~/.agents/skills`（cline/dexto/kimi-code-cli/loaf/warp/zed）、`<config>/agents/skills`（amp/replit/universal）、`~/.zencoder/skills`（zencoder/zenflow）；Project 侧 `.agents/skills` 被 18 个 Agent 共用，另有 `.qoder/skills`、`.trae/skills`、`.zencoder/skills` 各 2 个。D-007 已为 Project 侧选择"manifest 单一 owner + 按路径去重"，但只读取证证明该模型两侧都未兑现，且失败形状同源：**磁盘上不存在"这条 entry 是谁装的"这一信息，代码在需要它时一律退化为"看起来像技能就删"**。Global 侧更彻底——`deployment/` 下没有任何归属记录，`unlink_skill_from_agent`(`deployment/mod.rs:305-336`) 直接删共享目录里的条目，其余 5 个仍启用的 Agent 静默失去该技能；Project 侧 `remove_skill_from_all_projects`(`projects/sync.rs:132-157`) 与 `clear_project_symlinks`(`projects/helpers.rs:36-39`) 会删掉从未登记进 `skills-list.json` 的目录，与 `sync.rs:103-106` 自己的注释直接矛盾。根本困难在于 per-agent 归属是产品虚构：`~/.agents/skills` 是生态共享约定，zed 事实上就能加载 cline 部署的技能，任何试图记录归属的方案在存量磁盘上都没有正确的起手（已有部署无归属记录，记为无主/归给全部/归给第一个三种起手都错）。
- 决策：① **目录即部署单元**：把 canonical 目录键提升为一等"部署目标"，N 个解析到同一目录的 Agent 在部署模型与 UI 上塌缩为 1 项并列出成员 Agent；不记录 per-agent 归属，因为它在物理上不存在。Agent 的**启用开关仍是 per-agent 的**（D-009 不变），只塌缩部署目标，不塌缩激活状态。② **归属零状态推导**：一条 entry 属于 SkillStar，当且仅当其链接目标落在 `hub_skills_dir()` 之下——已验证 5 个全局写入点（`deployment/mod.rs:165/406/532/536/631`）的 src 全是 hub 绝对路径，且 `read_link_resolved`(`fs_ops.rs:174-184`) 确定只解一跳，hub→repo cache 的两跳链返回中间的 hub 路径。③ **容器判定复用 `repo_link::is_inside`**(`repo_link.rs:65-77` 及其 `normalize`:79-93) 的形态（双侧 canonicalize with fallback + 分隔符归一 + Windows 小写折叠），提升为可复用实现，并把 `local_skill.rs:174`、`git/gh_manager.rs:432`、`storage_maintenance.rs:183` 三处裸 `starts_with` 一并收敛；`repo_link.rs:4-9` 已记录过"两份实现分叉导致 Windows junction 误判"的同类事故，不制造第四份。④ **目录身份键**用 `fs_ops::canonicalize_existing_prefix`（`ss-core`；`skill_update` 仍保留同名包装）处理"目录尚不存在"，与 ③ 的容器判定是两个不同问题，不合并。键只在每次 `list_profiles()` 快照内重算，**不持久化**——openclaw(`builtin.rs:487-494`) 与 5 个 env 驱动 Agent(`builtin.rs:102/115/152/237/294`)、`XDG_CONFIG_HOME`(`builtin.rs:473-478`) 的目录会随环境与磁盘状态漂移。⑤ **copy 形态用 sibling marker**（无链接可读），判定顺序是先试 link 谓词、`is_link` 为假才查 marker。⑥ **拒绝 project root 等于或包含任一 agent global 目录**：`ensure_project_root_exists`(`projects/types.rs:74-85`) 与 `cli/install.rs:132` 今天只检查 `is_dir()`，HOME 可被注册成 project 从而让 project 部署写进 global 共享目录，此时两个 surface 的 src 同为 hub、谓词无法区分。⑦ 归属判定**不复用** `acquire_skill_mutation_lease`：它是不可重入的进程级 `Mutex`(`skill_update/transaction.rs:4-7`)，三个 resync 入口已在其内，deployment 层再 acquire 会自死锁；改为按目录键的独立同步，并覆盖今天完全无锁的 5 个 Tauri 命令（`commands/skills.rs:91`、`commands/agents.rs:28/44/64/76`）。
- 后果：获得——存量**零迁移**（磁盘即真相，不需要任何归属回填或"收养"决策）；崩溃后重扫即一致（不引入第二份可与磁盘分叉的状态）；4 处计数串台（`installed_skill.rs:532-548`、`registry.rs:131-153`、`deployment/mod.rs:280-302`、`global_deploy.rs:23-38`）结构性消失而非逐个打补丁；CLI 已有的 `seen_dirs` 去重(`deployment/mod.rs:476,491`)从特例**泛化**为全局不变量。承担——Settings 里共享目录的多行塌缩为一行是**可见的 UX 退让**，需文案说明"这是这些 Agent 自己的生态约定，非 SkillStar 决定"；`AgentProfile` 是冻结的 8 字段 IPC（`registry.rs:16-18`、本文件 D-008），塞不进第 9 个字段，必须新开 `list_deploy_targets` IPC 而非扩展它；unlink 语义从"从某个 Agent 移除"变为"从某个目录移除（影响其全部成员 Agent）"，这是**如实陈述**而非降级——旧语义在磁盘上从未成立。本条扩展 D-007：D-007 的"按路径去重"方向正确但只在 `build_path_plans`(`projects/sync.rs:57-98`) 与 `add_skills_to_project_with_mode`(`sync.rs:419-465`) 两处兑现，scan/rebuild/cleanup 三处未兑现，本条把该不变量的适用范围扩展到 Global 侧并要求两侧同源实现。落地必须先于模型改动修掉三条既有 bug：`swap_in_fresh_deploy` 与 unlink 之间的 lost update（`mod.rs:635-640` + `mod.rs:102`，用户看到"已取消部署"但 rename 把技能复活）、`toggle_skill_for_agent` enable 分支先删后建不回滚（`mod.rs:154-173`，应复用 `mod.rs:619-654` 的先建后换）、以及 ⑦ 的无锁命令同步。
- 证据：只读取证编排 run `run_f21c372c2a96`（Global 取证 / Project 取证 / lease 与碰撞面 / 候选模型对抗评估 / provenance 验证，5 个 Task）。代码依据见 `crates/ss-skills/src/agents/builtin.rs`、`crates/ss-skills/src/deployment/mod.rs`、`crates/ss-skills/src/projects/{sync,helpers,rebuild,scan}.rs`、`crates/ss-skills/src/repo_link.rs`、`crates/ss-core/src/infra/fs_ops.rs`。可复发根因与自检见 [errors.md](./errors.md) 同日条目。

## D-025：OMP 模型角色存在 binding 级设置袋，未分配角色不写盘

- 日期：2026-08-13
- 状态：accepted
- 背景：D-018 只写了 `modelRoles.default`，而 OMP 的核心差异化能力恰恰是按任务意图路由的多角色系统（`default` 正常编码 / `smol` 廉价子代理 fan-out / `slow` 深度推理 / `plan` 规划模式，命令行对应 `--model` / `--smol` / `--slow` / `--plan`，另有 vision/designer/commit/tiny/task/advisor 六个）。用户要在 SkillStar 内完成这层配置，就必须能为**不同角色指定不同 provider**（典型配置是 default 用便宜的快模型、slow 用推理模型、smol 用最便宜的），因此角色不能挂在任何单个 provider 条目上。既有扩展点 `ToolActivation.settings` 是 per-entry（per-provider）的：`activate_tool` 在重新激活同一 provider 时按 provider 继承它（`crud.rs:331-342`），active 指针一变角色就会跟着漂。对 OMP v17.2.15 源码与二进制的实证确认：`modelRoles` 是**无 schema 校验**的开放 string map（`settings-schema.ts:569` 的 `type: "record"`，config.yml 整体不过 arktype），值语法为 `provider/model[:thinkingLevel]`，`@role` 是角色别名前缀，`smol`/`slow`/`designer` 未配置时由 OMP 自己回落到 `default`（`shouldInheritDefaultBeforePriority`）。
- 决策：在 `ToolBinding` 上新增**binding 级** `settings: Option<Value>`，作为 `ToolActivation.settings` 的工具级兄弟，首个消费者是 `OmpSettings { roles: BTreeMap<String, OmpRoleTarget> }`（`OmpRoleTarget { provider_id, model, thinking }`，存 SkillStar provider id 而非磁盘上的 `skillstar_*` 键，键在写入时由 `skillstar_managed_key` 现算，避免两处规则分叉）。写入命令 `update_tool_binding_settings` 与既有 `update_tool_settings` 对称。落盘策略与 models.yml 托管块的 retain 一致：**先删除全部指向 `skillstar_*` 的角色，再写当前集合**，因此 UI 取消分配等价于磁盘删除；指向用户自有 provider 的角色永不触碰。**未分配的角色不写**（OMP 自身的回落机制比我们写一个冗余指针更正确），`default` 缺失时由 active 条目兜底以保持 D-018 行为。悬空防护：角色指向未绑定 / 无 OpenAI base URL / 未选模型的 provider 一律跳过；角色名含 `/`、空白或以 `@` 开头（撞 OMP 别名语法）一律跳过。解绑与删除 provider 连带清除其角色分配（`prune_binding_roles_for_provider` 只操作 `roles` 键，保留设置袋内其他键）。
- 后果：获得——用户在 Models 矩阵内即可完成 OMP 的全部角色路由，无需手写 YAML；binding 级设置袋对未来其他"跨 entry 配置"（OpenCode 的 `small_model`、Claude 的层级模型）是现成接缝。承担——`ToolBinding` 新增字段需要前端手写类型 `src/types/models.ts` 同步（该文件不在 `types:gen` 覆盖范围内，是既有分歧，本条未修复）；不代写 OMP 的 `cycleOrder`，因此用户配置的 `plan` 角色不会进 Ctrl+P 循环（默认只循环 smol/default/slow），这是 OMP 侧行为，由 UI 文案说明而非替用户改设置。
- 证据：`crates/skillstar-models/src/tool_sync/{types.rs,omp_provider.rs}`、`crates/skillstar-models/src/providers/{types.rs,crud.rs}`、`src-tauri/src/commands/models_commands/tools.rs`、`src/features/models/lib/{ompRoles.ts,toolBinding.ts}`、`tool_sync/tests/part4.rs` 与 `providers/tests/part5.rs` 覆盖测试；schema 依据为 OMP v17.2.15 包内源码（`src/config/{model-roles,settings-schema,model-resolver,models-config-schema-bundle}.ts`）；落盘结果经真实 `omp` 二进制验证（`omp models --json` 零告警、`omp -p --model @slow/@smol/@plan` 均正常解析，未配置角色对照组报 `Model not found`）。

## D-026：官方 MCP Registry 为一等主源，GitHub registry 降级为增强镜像

- 日期：2026-08-13
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：MCP catalog 此前只有一个源 `api.mcp.github.com`，实测已从 `/v0` 返回 `Deprecation: true`，且其条款没有任何公开的再分发说明——我们却把它整表镜像进本地 SQLite 快照并长期保留。同期官方 registry `registry.modelcontextprotocol.io/v0.1` 已可用，实测 21338 条（`version=latest`），而 GitHub 侧只有 218 条；官方 registry 的 Terms of Service 明确把数据置于 **CC0 1.0**，是当前唯一在许可上允许长期本地镜像的源。两者字段互补：GitHub 侧额外带 stars / license / readme，官方侧没有。
- 决策：官方 registry 成为 `priority: 0` 的一等主源且 `mirrorable: true`；GitHub registry 降为 `priority: 10` 的**展示增强镜像**（`mirrorable: false`），只在合并时补 stars/license/readme 这类官方源不携带的字段，不再定义"有哪些 server"。`priority` 语义被明确为**权威性**而非偏好：用户自定义源起始 `priority: 50`，永远输给内置源，但可以补充无人收录的 server。分页熔断按源设置而非全局——主源被截断会让 catalog 永久性不完整（`max_pages: 400`），而受 `x-ratelimit-limit: 10` 限制的镜像宁可截断也不该被反复敲打（`max_pages: 50`）。
- 后果：获得——catalog 从 218 条扩到 21363 条（跨源合并 189、deprecated 243）；镜像行为落在明确许可的源上；单源故障不再等于 catalog 归零。承担——本地快照体量与同步耗时上一个数量级，因此**全量未分页读取不再是可用的浏览路径**（见 D-027 的分页要求）；聚合 fingerprint 必须改为按 `id=hash` 排序哈希，否则新增/移除一个源或某源答 304 都会产生假"已变更"。
- 证据：`crates/ss-marketplace/src/mcp_remote/{sources.rs,fetch.rs,merge.rs}`、`crates/ss-marketplace/src/mcp_snapshot/mod.rs`（原调研快照已随 D-074 删除）。

## D-027：跨源合并主键是 `server.json` 的 `name`，不是各源的 id

- 日期：2026-08-13
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：接入第二个源之后必须回答"两行是不是同一个 server"。每个 registry 都发自己的 id，同一个 server 在官方源和 GitHub 源下 id 不同；用 id 做主键会把同一个 server 重复上架两次，用户看到两张卡片、装出两份配置。`server.json` 的 `name` 是反向域名全名（如 `io.github.netdata/mcp-server`），schema 要求发布者证明命名空间所有权，是唯一跨源稳定的身份。
- 决策：合并以 `name` 为主键；同名多版本按 registry 的 `isLatest` 收敛。安装时记录到 `McpServerEntry::registry_name` 的也是这个 `name`，与写进工具配置的 sanitized key 明确区分——后者是配置键，会因为字符清洗而丢失身份信息。
- 后果：获得——跨源去重正确，"已安装"判定与更新检测可以摆脱 server 名字字符串的模糊匹配。承担——`name` 缺失或畸形的源行会被合并逻辑跳过；这是刻意的，一个连身份都没有的条目也无法被安全地安装或更新。
- 证据：`crates/ss-marketplace/src/mcp_remote/merge.rs`、`crates/ss-marketplace/src/mcp_models/mod.rs` 的 `namespace` 字段文档、`crates/ss-app/src/mcp/draft.rs` 的来源指纹映射。

## D-028：不接入 PulseMCP 与 mcp.so

- 日期：2026-08-13
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：调研阶段评估了六个第三方 MCP 目录作为潜在补充源。两个必须明确拒绝，否则日后会被反复重新提出。
- 决策：**不接入 PulseMCP**——其 API 公告 2026-09 全量日落，且在此之前已强制 API key；接一个已宣布死期的源，是在为一次确定的返工付费。**不接入 mcp.so**——其 `robots.txt` 明确禁止 `/api/`，抓取它等于无视站点的明示意愿，与我们对自己数据源的要求不一致。
- 后果：获得——避免重复调研，避免把工程投入押在确定会消失的端点上。承担——放弃这两家独有的收录条目。Smithery（`useCount` 热度、预抓取 `tools[]`）与 Glama（`spdxLicense`）未被拒绝，但只可能作为**运行时代理查询**的展示补充，不做长期镜像：两家都没有公开的再分发条款，与 D-026 的镜像许可要求冲突。
- 证据：原调研快照已随 D-074 删除，本节保留历史结论（端点与 robots.txt 抓取于 2026-08-13）。

## D-029：MCP 运行时形态优先 remote，且以本机实际可用运行时为准

- 日期：2026-08-13
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：一个 `server.json` 可以同时给出 `remotes[]` 与 `packages[]`。此前的选择逻辑是"有 packages 就取 `packages[0]`，否则取 `remotes[0]`"——由数组顺序决定在用户机器上执行什么，既不是安全判断也不是可用性判断。
- 决策：优先级为 `remotes[streamable-http]` > `remotes[sse]`（可用但传输已弃用，必须标注） > `packages[oci]`（容器隔离，最安全的本地形态） > `packages[mcpb]` > `packages[npm/pypi/nuget/cargo]`。remote 优先的理由是它零工具链依赖、零本地代码执行，且规范把 OAuth 授权明确限定为 HTTP 传输的能力。**rank 不是最终答案**：`runtimeHint` 只是提示，每个 stdio 候选都要对真实 `PATH` 探测一次（复用与真正启动进程相同的 resolver），不可用的候选排在所有可用候选之后——没装 Docker 的机器上 npm 包胜过 OCI 包。选择器返回**全部候选 + 推荐项**而非单一结果，用户可以覆盖。
- 后果：获得——默认选择同时是安全排序和可用性排序；"为什么推荐这个"可以在 UI 上解释（rank + 可用性 + 阻塞原因）。承担——`mcpb` 候选被列出但标为不可安装（SkillStar 没有"下载并校验 `fileSha256`"这一步，而 registry 明确不做校验），无 `runtimeHint` 的 `cargo` 候选同样不可安装（`cargo install` 是持久安装而非一次性运行器）；这两条是如实陈述能力边界，旧行为在这两种形态上本来也跑不起来，只是失败得更晚更难懂。
- 证据：`crates/ss-app/src/mcp/runtime.rs` 与 `crates/ss-app/src/mcp/tests.rs`（注入式 `PATH` 探测，排序规则逐条 pin）。

## D-030：Deck 自持 Agent 链接，不从成员 Skill 推导

- 日期：2026-08-13
- 状态：accepted
- 背景：Deck 卡片底部的 Agent rail 原本是纯派生量——卡组内已安装 Skill 全部链接到某 Agent 就点亮。自从 GUI 安装会把新装 Skill 部署到全部已启用 Agent（见 `crates/ss-app/src/global_deploy.rs`），任何新建卡组一出生就全亮，rail 不再表达任何用户意图，"取消链接"成了唯一可用操作。
- 决策：`SkillGroup` 增加 `agent_links: Option<Vec<String>>`，语义是**用户为这个卡组显式认领的 Agent**。新建卡组为空集，rail 全灰；点亮才批量链接成员 Skill。派生量降级为漂移指示：已认领但成员并非全部实际链接时显示 mixed，未认领一律不点亮。`None` 与空集合一样，卡组页按未点亮显示，不从成员链接反推，也不另做一次回填。
- 后果：获得——新建卡组的默认状态回到"什么都没做"，rail 重新可读为意图；卡组与 Skill 两级链接语义分离，安装期全局部署不再污染卡组视图。承担——两级状态可能漂移，必须靠 mixed 显式暴露，不能静默取整；缺少 `agent_links` 的历史卡组按未点亮显示。
- 证据：`crates/ss-skills/src/skill_group.rs` 与 `crates/ss-skills/src/workflows/skill_group_links.rs`；行为见 `docs/features/skills/README.md`。

## D-031：技能发布链路并入单一 GitHub App 身份，`gh` CLI 退出远程路径

- 日期：2026-08-14
- 状态：accepted
- 背景：`ss-skills::git::gh_manager` 是全仓最后一处绕过 SkillStar 身份与代理策略的远程链路。它用 `gh auth status`/`gh api user`/`gh repo list`/`gh api contents`/`gh repo create --push` 完成 REST，用裸 `git` 完成 clone/pull/push。后果有三：发布用的是机器上的全局 `gh` 登录而不是 D-013 的 App 身份；裸 Git 继承启动进程的 `HTTP_PROXY`，而 D-014 的 transport 路径是显式清空再按 SkillStar 配置重设，同一个应用出现两套代理行为；`gh repo list <login>` 只能列个人仓库，组织仓库根本无法作为发布目标。
- 决策：发布链路全部改用 App 凭据。REST 移入新的 `ss-skills::git::gh_rest`：`GET /user`、`GET /user/repos?affiliation=owner,collaborator,organization_member`（分页）、`GET /repos/{owner}/{name}/contents/skills`、`POST /user/repos`，凭据取自 `GitHubAuthFacade::api_credential()`，客户端一律由 `probe_http_client` 构造并对齐 `Accept: application/vnd.github+json` 与 `X-GitHub-Api-Version: 2022-11-28`。发布入口是同步的（Tauri `spawn_blocking`、CLI 主线程），因此在同步上下文内自建 current-thread runtime 驱动 async 客户端，而不是引入 `reqwest::blocking` 绕开统一 client。远程 Git（clone / pull --rebase / push）改走 `ss_git::transport::execute_remote_command` 与一次性 operation session；本地 Git（init/add/commit/remote add/rev-parse）保持裸子进程，因为它们不接触远端。`GhStatus` 形状不变但语义重映射为「发布所需的 `git` 未安装 / SkillStar 未登录或凭据过期 / 就绪且带 App login」，前端三个分支零改动继续有效。`gh` 只保留 Settings 的 `check_gh_installed` 环境探测。
- 后果：获得——发布与扫描/安装/更新使用同一身份、同一代理和同一脱敏边界，token 不进 argv、git config、remote URL 或日志；组织仓库首次成为可选发布目标；401/403/404/422/429 有可行动分类而不是 `gh` stderr 裸串。承担——发布现在要求用户在 SkillStar 内登录 GitHub，光有全局 `gh` 登录不再够用；新建仓库只能建在当前用户名下（组织建仓属于共享频道路径，见 D-015）；`~/.agents/.publish-repos/` 下的既有缓存 remote 仍是历史 `gh` 写入的 URL，靠 session push 时重新认证而不是重写 remote。
- 证据：`crates/ss-skills/src/git/{gh_rest.rs,gh_manager.rs}` 与 `gh_publish_tests.rs`（三态重映射、分页与 affiliation、凭据只出现在 Authorization header、stub git 的 argv/输出无 token）。

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
- 证据：`crates/ss-app/src/usage_switch/{custody.rs,cursor.rs,target.rs,keychain.rs,target/*}`、`crates/ss-usage/src/vscdb.rs` 与 `custody_tests.rs`（实体文件内容一致判 LinkedTo、CLI 轮换后快照自动新鲜、Cursor 两账号切换真实写入并回读 state.vscdb、IDE 切换失败时不移动 pin、opencode 无 API Key 也能切）；行为契约见 [features/usage/README.md](./features/usage/README.md)。

## D-034：DTO 投影拥有前端契约，有重构节奏的域类型不直接暴露给 ts-rs

- 日期：2026-08-14
- 状态：accepted
- 背景：`/usage` 的前端形状长期是 `src/features/usage/types.ts` 里手抄的 `usage/dto.rs` 镜像，已在六个方向漂移，其中两类靠 review 抓不住：带 `skip_serializing_if` 的字段 `None` 时**整个键从 JSON 消失**，手抄凭 `Option<T>` 直觉写成 `| null`，于是 `x === null` 分支从来没被触发过；永远序列化的 bool 是必填，手抄写成可选。同期把 usage DTO 接进 ts-rs 时发现，`usage_switch::SwitchOutcome` 若自己 derive `TS`，切换域的重构会直接震动前端契约。
- 决策：`ss-usage` 的 `subscription.rs` / `catalog.rs` 里**纯数据、无行为**的类型（枚举、usage 快照树）直接 derive `TS`——它们本来就是 wire 形状，再套一层投影只会制造两份要同步的定义。有自己重构节奏的域类型不上生成面：`ss_app::usage::dto` 定义 `SwitchOutcomeDto` 等投影，接缝落在 AGENTS.md 已经指定的跨域聚合 crate。`types.ts` 退化为纯 re-export barrel，只保留没有 Rust 对应物的前端自有物（表单能力声明、事件契约、筛选哨兵）。
- 后果：获得——手抄漂移这一整类缺陷被门禁消灭（`check_generated_types.sh`），且 ts-rs 读 serde 属性，上面两种错误它都不会犯。承担——(1) `impl From<SwitchOutcome> for SwitchOutcomeDto` 用**完全解构**而不是 `..` 展开，上游加字段会在这里产生编译错误，这正是希望的：新字段要么显式进入前端契约，要么显式被丢弃，不会静默消失。(2) ts-rs 默认把 `i64`/`u64` 映射成 `bigint`，而 Tauri IPC 经 `JSON.parse` 只产出 `number`，64 位字段必须按仓库既有惯例标 `#[ts(type = "number")]`。
- 证据：`crates/ss-app/src/usage/dto.rs`、`src/types/generated/`、`src/features/usage/types.ts`；契约描述见 [features/usage/README.md](./features/usage/README.md) 的「前端类型契约」。

## D-035：Provider store 四层分离（Catalog / Provider / Credential / AgentBinding）

- 日期：2026-08-15
- 状态：accepted
- 背景：v3 的 `ProviderEntryFlat` 把四件不同的事塞进一张扁平行。三个后果各自独立地伤人：①协议是端点的属性而不是 Provider 的属性——同一个中转常同时开 `/v1/chat/completions` 与 `/anthropic`（`deepseek` preset 即如此），两个硬编码 URL 字段撑不住第三种协议；②「没探测过」与「不支持」不可区分——Codex 默认靠「字段是否等于 serde 默认值」推断，于是用户显式选择与从未触碰长得一样；③凭据只能是一个 `String`，但 Codex 的 `env_key` 存的是变量名、OpenCode 支持 `{env:}`/`{file:}`、Claude 的 `apiKeyHelper` 是一条命令，四种语义不同的通道被压成一种。另外 `created_at` 是毫秒而 `last_sync_at` 是秒，同一个文件两种单位。
- 决策：v4 拆成四个 < 400 行的模块。`Provider` 持 `Endpoints`（每协议一个 `Option<String>`）+ `ProviderCaps`（三态 `Tri`）；`Credential` 是判别联合，`ExternalCli` 变体取代 v3 靠 id 白名单在六处分支的 Native Official 特例；`AgentBinding.roles` 把角色路由从设置袋提升为一等字段（v3 里 Claude 层级模型在 `provider.meta`、OMP 角色在 `binding.settings`，同一概念两套存储）；catalog 独立成型且移出 store 文件。所有时间字段带 `_ms` 后缀。
- 后果：获得——能力位可表达「未知」，因此迁移永远写 `Unknown` 而非 `No`，升级不会让用户已有绑定突然失效；Official 从「六处 id 比较」变成「一次 `matches!`」；角色路由可跨 Agent 推广。承担——(1) 需要 v3→v4 迁移与永久备份（见 D-036）；(2) `providers/types.rs` 降级为只供迁移读的历史类型，`crud`/`tool_sync` 的 v4 化是独立工作包，在那之前 v4 是已落地但未接入运行路径的一层。
- 证据：`crates/skillstar-models/src/providers/{provider,credential,binding,catalog}.rs`。

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
- 证据：`crates/skillstar-models/src/providers/secret_resolve.rs`、`src-tauri/src/commands/models_commands/diagnostics.rs`、`crates/ss-app/src/models/dto.rs`（`provider_dto_never_contains_plaintext_secret`）。

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
- 状态：accepted（description 超长条款由 D-048 取代）
- 背景：D-019 把 Anthropic `quick_validate.py` 的尖括号限制当成通用生态规则，导致 Vercel 官方技能因 description 中合法的 `` `<ViewTransition>` `` 文本被拒绝；公开 Agent Skills 规范只要求 description 非空且不超过 1024 字符。扫描 UI 又把任何 issue code 固定解释成“缺少 name/description”，掩盖了真实原因。
- 决策：尖括号不再产生 frontmatter issue，也不阻断安装；其余门禁保持不变。扫描预览直接把后端 issue code 映射为具体本地化原因，不再重建或概括后端规则。此决策仅取代 D-019 的“description 含尖括号”条款。
- 后果：符合公开规范且描述中含 JSX、HTML 或占位符的技能可以安装；具体元数据问题仍 fail-closed，并在 UI 中准确显示。
- 证据：`crates/ss-skills/src/validation.rs`、`src/features/my-skills/components/import-modal/SelectSkillsPhase.tsx` 及对应回归测试。

## D-041：上游移除/更名在检查期可见，处理复用移除流程，迁移是一等操作

- 日期：2026-08-21
- 状态：accepted
- 背景：作者会删除、改名或把 Skill 移到别的桶（mattpocock/skills 的 in-progress 明说"可能随时变动或消失"）。此前这只在用户恰好更新同仓库别的 Skill、pull 之后才以阻塞对话框出现；改名则表现为"一个被删 + 一个新技能"，用户得自己卸旧装新并重配 Agent。
- 决策：更新检查把 tracked ref 上的路径消失记为 `upstream_change: removed`，并用 `git diff -M` / frontmatter `name` 判定后继；`update_state` 仍是唯一所有者，`update_available` 语义不变。移除的处理入口直接复用既有「来源已不再提供」对话框与 `resolve_skill_update`；改名由 `ss-app::skill_migration` 作为跨域 use case 一步完成（安装后继、沿用 Agent/项目部署、卸载旧条目）。不新增第二套"待处理"对话框，也不把不可更新项混进「更新 N 项」。
- 后果：用户在卡片上就能看到并处理上游变动；后继判定是启发式（`-M` 相似度或同名），判错时用户仍可走"移除 + 从 ghost 安装"的手动路径。迁移不是单事务：install 与 uninstall 各自持更新锁，中间失败按步骤报告，下一次检查会把残留旧条目标为 `removed` 供用户收尾。
- 证据：`crates/ss-skills/src/update_checker.rs`（`UpstreamStatus`）、`crates/ss-skills/src/update_state.rs`、`crates/ss-app/src/skill_migration.rs`、`src/features/my-skills/`。

## D-042：移除 skill-pack 功能残骸（CLI、store、读侧），而不是补全安装链

- 日期：2026-08-28
- 状态：accepted
- 背景：2026-08-27 对抗审查证实 pack 安装链(`install_pack`/`detect_pack`)全仓不可达且 `git log -S "PackAction::Install"` 为空——没有任何已发布版本写过 `packs.json`。删除安装链后,README 记录的 `skillstar doctor` / `pack list` / `pack remove` 只能对一个永远为空的 store 打印 "No packs installed."。二选一:补全安装链让功能成立,或整体移除。
- 决策：整体移除。删 CLI 三命令(`Doctor`/`Pack` clap 变体、`cmd_doctor`/`cmd_pack_list`/`cmd_pack_remove`)、`skill_pack` 模块读侧(`list_packs`/`remove_pack`/`doctor_*` + store 类型)、`paths::packs_path` 与 legacy `packs.json` 迁移行、README 条目。理由:功能从未对用户成立(store 从未被写入),不存在破坏;真正的技能打包分发已由 bundle(.agd)与 share code 覆盖。
- 后果：`skillstar` CLI 少三个从未产生过效果的子命令;`.claude-plugin` 式 pack 若将来要做,从 git 历史(本决策前一提交)取回并重新设计,而不是在空 store 上续写。`marketplace_pack*` 表的 DDL 按迁移不可删原则保留为惰性 schema。
- 证据：`crates/ss-app/src/cli/{mod,manage}.rs`、已删除的 `crates/ss-skills/src/skill_pack*`、`README.md`、2026-08-27/28 对抗审查记录(errors.md 同日条目)。

## D-043：Agent 技能主开关持久化恢复意图，而不持久化 Agent 归属

- 日期：2026-08-30
- 状态：accepted
- 背景：Settings 的旧「所有已安装技能」主开关用当前 Hub inventory 推导动作：部分状态会补齐 Hub 缺口，全量关闭会清空当前链接。它既不能表达「只暂时停用本目录原有集合」，也在刷新或重启后不知道该恢复哪些名字。把集合按 Agent id 保存同样是错误模型：多个 profile 可以合法解析到同一个物理 Global skills 目录，磁盘没有 entry 的 per-Agent ownership 事实（D-024）。
- 决策：`profiles.toml` 增加 recovery-only journal，键是暂停当刻解析到的物理 Global skills 目录，值是**仍待恢复**的排序去重 Skill 名称。`ss-app::agent_managed_skills` 在首次停用前原子落盘精确活动集合，再逐项临时移除；完成后只保留实际已消失的名字。若 journal 存在，恢复只尝试其中目前仍缺失的名字；成功或用户手动放回的名字删除，Hub 源缺失、受保护冲突或失败则保留。当前活动集合永远从磁盘重读，journal 不作为链接真相，也不把目录 entry 归给任一 profile。路径后来解析到其他地方时不得按 id 迁移或重新部署旧 journal。
- 后果：获得——暂停/恢复跨刷新与重启保持精确集合，且恢复路径没有枚举 Hub inventory 的入口；共享目录天然共用状态和 pending 范围。承担——恢复源已从 Hub 删除时会保留可重试项而非“看似完成”；journal 是 D-024「磁盘即真相」的狭窄例外，只表达用户主动发起的恢复意图，因此必须在每个动作后以磁盘状态收敛，不能扩张为 ownership/provenance store。
- 证据：`crates/ss-skills/src/agents/profile_storage.rs`、`crates/ss-app/src/agent_managed_skills.rs`、`src-tauri/src/commands/agents.rs`、`src/features/settings/lib/agentSkillSync.ts`。

## D-044：pack 根目录 SKILL.md 垫片不是安装单元

- 日期：2026-08-30
- 状态：accepted
- 背景：impeccable 风格的技能包（如 `xxww0098/rust-skills`）把正文放在 `skills/<name>/`，同时在仓库根放一份同 identity 的 `SKILL.md`，好让一层扫描器把整仓当作技能目录。SkillStar 的 root-first 发现把根 `SKILL.md` 当成唯一技能，`folder_path` 为空就会把整个仓库（测试、脚本、各 harness 副本）链接进 Hub。全深度扫描也会因根路径优先级 4 赢过 `skills/`。
- 决策：发现阶段先剥掉「根 SKILL.md 与同 identity 嵌套 catalog **或** harness 副本」的垫片，再执行 root-first / 去重。`skills/` 与 `source/skills/` 同为规范目录，优先级高于 `.cursor/skills`、`.dsh/skills`、`.claude/skills` 等 harness 副本。真正的单技能仓库（根 SKILL.md 没有同名嵌套副本）行为不变。`.claude-plugin/plugin.json` 的 `skills` 同时接受字符串路径和数组（语义见 [D-075](#d-075异形分发仓库只有一张选副本表)：字符串是容器）。
- 后果：`skillstar add xxww0098/rust-skills` 安装 `skills/rust`（若 catalog 存在），不再把整仓当技能。只有 harness 副本、没有 catalog 时也安装该副本而不是整仓。一层扫描器仍可继续使用根垫片。
- 证据：`crates/ss-skills/src/{pack_layout.rs,discovery.rs,plugin_manifest.rs}` 及 `pack_root_shim_installs_canonical_skills_folder` / `root_shim_plus_harness_copies_does_not_install_the_repo_root` 测试。

## D-045：多 harness 技能包按 `.<harness>/` 安装

- 日期：2026-08-31
- 状态：accepted
- 背景：rust-skills / impeccable 这类包在每个 Agent 目录下各放一份独立技能。卡片 SVG 轮播原先只 `toggle` 已安装链接，Install 按钮走 `install_skill(url, name)` → 发现后 `global_deploy` 到全部已启用 Agent。发现层又漏了 `.cursor/skills` 与 `.dsh/skills`，根垫片会把 `source_folder` 写成空，Hub 链整仓。
- 决策：安装单元是含 `SKILL.md` 的 harness 技能目录（或 harness 根，若那才是单元）。未指定 harness 时 catalog `skills/<name>/` 优先；点轮播图标或 CLI 显式单个 `--agent` 时该 harness 文件夹赢。没有该文件夹时的回退见 [D-046](#d-046已安装轮播从-repo-cache-部署且缺-harness-时回退)。稀疏检出保留全部嵌套 `SKILL.md` 父目录，不再按 basename 丢掉 `.agent` / `.agents`（已被 [D-063](#d-063代表副本唯一物化与永不整仓下载) 与 [D-075](#d-075异形分发仓库只有一张选副本表) 取代：每个身份只物化一份，harness 请求只多物化它选中的那一份）。
- 后果：同一 Hub 名仍只有一条 lock，`source_folder` 跟随最近一次明确请求的 harness——**钉住例外**：tree URL 子路径把 `source_folder` 硬钉死，之后的 harness 请求一律 `Reuse`，不再跟随（见 [D-075](#d-075异形分发仓库只有一张选副本表)）。复用仅当现有 `source_folder` 已经是该次解析到的文件夹；否则从同一 clone 改指向（不二次 clone），并先把已链到其他 Agent 的链接钉到当前 payload。轮播未链接图标走 `install_skill(url, name, agentId)`，不得只 `toggle` 当前 Hub。缺 harness 文件夹的行为已由 D-046 修正，不再 fail-closed。
- 证据：`resolve_install_skills`、`existing_same_repo_action`、`pin_existing_global_links_to_current_source`、`install_skill(..., agentId)`、`AgentTargetCarousel` 接线测试、`stale_dsh_link_is_rewritten_to_requested_harness`。

## D-046：已安装轮播从 repo-cache 部署且缺 harness 时回退

- 日期：2026-08-31
- 状态：superseded（D-081）
- 背景：D-045 让未链接轮播图标走完整 `install_skill`。`clone_or_fetch` 在 cache 已有 `.git` 时仍 `git fetch --depth 1` + reset，已装 rust-skills / impeccable 点第二个图标像重装。同时 D-045 对缺 `.<harness>/` fail-closed，impeccable 没有 `.dsh` 时点 DeepSeek 报错，用户无法把技能落到 `~/.dsh/skills/<id>`。
- 决策：hub 已装且 repo-cache 已有 clone 时，轮播 / 显式单个 `--agent` 只扫描现有 checkout（`cached_repo_dir_if_present`），不 clone、不 fetch；`source_folder` 没变就不改 lock。cache 缺失才 fetch。请求的 harness 文件夹不存在时按顺序回退：规范 `skills/<name>/` 或 `source/skills/` → 已装则用现有 hub `source_folder` → 同 identity 的另一份嵌套 harness 副本。把该 payload 部署到被点 Agent。禁止 `source_folder: None` 整仓，禁止静默 no-op。只有完全没有嵌套 `SKILL.md` 才失败。
- 后果：已装卡的常见轮播点击是 cache-local 部署/改指向。Impeccable 点 DeepSeek 会把已有 skill 文件夹链到 `~/.dsh/skills/impeccable`，不再报「没有 `.dsh`」。首次安装和 cache 被删后的重装仍走网络。
- 证据：`scan_repo_preferring_local_cache_in_session`、`resolve_install_skills` 回退、`installed_rust_skills_deepseek_retargets_from_cache_without_clone`、`installed_impeccable_deepseek_falls_back_to_a_skill_folder`、`missing_git_cache_still_fetches_for_harness_install`。

## D-047：技能安装是 vercel-skills 五步管线，harness 文件夹是 identity 别名

- 日期：2026-08-31
- 状态：superseded（D-081）
- 背景：CLI、Tauri、轮播、batch 和整仓 clone 回退各自选文件夹，shim / catalog / harness 扫描器重复。用户要的是 `npx skills add` 那条管线，不是第六条路径。
- 决策：所有 git/local 安装走同一入口 `skill_install::install_from_source`：1. `Source::parse` 解析 `owner/repo`、URL、tree URL、本地路径；2. 发现含 `SKILL.md` 的目录；3. Hub 只链所选文件夹；4. Agent 目录 symlink（Windows 必要时 copy）；5. 调用方决定 project vs global。`.<harness>/skills/<name>` 与 `skills/<name>/` 是同一 identity：`--agent X` 优先该 harness，否则 catalog，否则现有 hub，否则另一份 harness 副本。没有 `SKILL.md` 才失败。禁止整仓 clone 回退。
- 后果：本地路径和 Git URL 产物一致。rust-skills / impeccable / ui-ux-pro-max-skill 都装得上。已删除 git ref 与 prefetch 失败仍按 D-046 / errors.md 处理。share-code 的 embedded 分支不是第六条 git 安装。
- 证据：`install_from_source`、`resolve_install_skills`、`install_pipeline_table_chooses_harness_or_fallback_folder`。

## D-048：规范兼容性上限只警告，不阻断技能安装

- 日期：2026-09-01
- 状态：accepted
- 背景：公开 Agent Skills 规范将 `description` 限制为 1024 字符，但现实仓库可能只有轻微超限且仍可使用；严格拒绝会让用户无法安装本可运行的技能。
- 决策：`DescriptionTooLong` 保留稳定 issue code，但降为咨询级；扫描预览显示黄色兼容性警告并允许选择、安装。缺失或非字符串 description、过长 name、损坏或不可读 frontmatter 仍显示红色并阻断。严重级别只由后端 `FrontmatterIssue::is_blocking` / `DiscoveredSkill.installable` 决定，前端不得按 issue code 复制规则。此决策仅取代 D-040 中 description 超长会阻断安装的条款。
- 后果：用户可以安装规范外的长描述技能，但目标 Agent 仍可能拒绝或忽略它；SkillStar 在安装前保留清晰警告。
- 证据：`crates/ss-skills/src/validation.rs`、`crates/ss-skills/src/repo_scanner/scan_install.rs`、`src/features/my-skills/components/import-modal/SelectSkillsPhase.tsx` 及对应回归测试。

## D-049：吸收通不过 deletion test 的浅 crate

- 日期：2026-09-01
- 状态：accepted
- 背景：D-002 规定 crate 只在独立变更节奏、依赖集合或 deletion test 证明有收益时才拆出。审查时 `skillstar-agents`（约 1.7k 行，只与 skills 同编译）、`ss-github-auth`（约 1.9k 行，skills 与 channels 都已依赖 skills）和 `skillstar-providers`（约 327 行静态表，models/usage 本就依赖 core）都通不过 deletion test：没有独立第三方依赖墙，也没有环。另有 `ss-sync → ss-skills` 幽灵 path dep（源码零引用）已由前置提交拆除。`skillstar-channels` 与 `ss-git` 仍独立——前者用 `SkillMutationPolicy` 打破环，后者把 `gix` 挡在 skills/sync 编译单元之外。
- 决策：`agents` 与 `github_auth` 收进 `ss-skills` 的公开模块；Provider identity/balance 收进 `ss-core::providers`。对外路径改为 `ss_skills::agents`、`ss_skills::github_auth`、`ss_core::providers`。`check_workspace_deps.sh` 拒绝这三个旧包名再现。不把 channels 或 git 并进 skills。
- 后果：workspace 从 13 个成员减到 10 个；channels 不再直连认证叶子；models/usage 不再多一跳 providers crate。D-004 的 SSOT 与「无产品域依赖」不变量保留在 `ss-core::providers` 模块（模块本身仍无产品域依赖，只是不再是独立 crate）。新增浅 crate 必须先过 deletion test。
- 证据：`crates/ss-skills/src/{agents,github_auth}/`、`crates/ss-core/src/providers/`、`scripts/internal/check_workspace_deps.sh`、本决策对应提交。

## D-050：对抗审查加固：熔断排序、SOCKS5H、GitHub 族匿名改写、ETag 绑 host

- 日期：2026-09-01
- 状态：accepted（候选排序语义被 [D-088](#d-088github-加速源改为用户排序回退) 修订：不再按延迟排序）
- 背景：D-023 给出了 Git mirror 候选链与 marketplace host 链，但仍按声明顺序串行尝试、SOCKS5 在本地解析 DNS、`insteadOf` 只改写 `github.com`、匿名 HTTP（raw/codeload/api/updater/`skills.sh`）直连 GitHub、ETag 跨 host 复用会假 304。这些在 GFW DNS 污染与单镜像故障下仍会把安装、商店和更新整条链路打掉。
- 决策：① `state/github_mirror_health.json` 记录每个加速源的成败与延迟；连续 2 次失败熔断 20 分钟；`candidate_mirror_urls()` 按健康度排序并跳过开路；全部开路则 fail-open 回声明顺序；保存新配置重置健康。连通性探测 GET `{mirror}https://raw.githubusercontent.com/octocat/Hello-World/master/README`，不再 HEAD 加速源根。② SOCKS5/SOCKS5H 出网一律 `socks5h`（远端 DNS）；新建 `proxy.json` 带国内 LLM 默认 bypass，**不**回写已有文件。③ 匿名 GitHub 族 URL（github / raw / codeload / objects / gist / 匿名 `api.github.com`）经健康加速源包装；带 `Authorization` 或 userinfo 的请求永不包装。Git `insteadOf` 覆盖同一组 origin，不含 `api.github.com`。④ 启用 GitHub 加速时，`marketplace_hosts()` 在 `skills.sh` 之后追加 `{mirror}https://skills.sh/`；`If-None-Match` 只发给当初签发 ETag 的 `source_host`。⑤ Settings 网络诊断探测代理监听、直连 GitHub、各加速源、skills.sh 与 MCP Registry，建议以 i18n key 返回。⑥ updater 插件直连失败时，经匿名链读取 `latest.json` 只用于发现新版本，安装仍走签名插件或手动 Releases，从不从第三方加速源安装二进制。
- 后果：公开拉取在单镜像/DNS 污染下可自动绕行；凭据与签名安装边界不变。承担：加速源是中间人，只应用于公开流量；熔断状态是可重建的 `state/` 文件。
- 证据：`crates/ss-core/src/config/{github_health,github_rewrite,github_mirror,proxy,network_doctor}.rs`、`crates/ss-core/src/infra/{github_http,http_client}.rs`、`crates/ss-git/src/transport.rs`、`crates/ss-marketplace/src/remote/mod.rs`。

## D-051：来源复合身份、精确内容修订与 skillstar-learning

- 日期：2026-09-01
- 状态：superseded（被 [D-053](#d-053移除学习功能与-skillstar-learning) 取代）
- 背景：私人教程按 `skill.name` 分桶，同名不同仓库会串学习记录；`ss-skills::tutorial` 把 HTML 安全、freshness 和 ACP 生成输入混在安装域里。P0 Learn 需要精确 revision 绑定，但不能一次拆完 `ss-core::Skill` 或新增一簇浅 crate。
- 决策：唯一新增 crate 是 `skillstar-learning`。身份是来源复合值（Git canonical repository + ref + content root / 本地 UUID sidecar / 频道 numeric repository ID + content root），教程与 Guide 绑定当前 v2 content hash 的 `SkillRevision`，`name` 只作安装表查找句柄。learning 只依赖 `ss-core`；`ss-app::learning` 按 channel > local > Git 投影 `ResolvedSkill`。私人教程双读单写：新写入 identity 路径，旧 name 路径只读且不能自动绑定。迁移按 [issue #49](https://github.com/xxww0098/SkillStar/issues/49) 冻结序列逐步接线，任一步可独立回退。不拆 `skillstar-tutorial` / `ss-gpuide` / `skillstar-identity`，三者共同构成 learning 的 deletion test。
- 后果：获得——同名 Skill 的学习记录可区分，本地编辑只使教程 stale 而不改 identity，失败保留最后一个可用 artifact。承担——旧 name-keyed artifact 在重新生成前保持 unbound；P0 不升级 lockfile schema，频道离线时可选 release 标签允许缺省。
- 证据：`crates/skillstar-learning/`、`crates/ss-app/src/learning/`、`crates/ss-skills/src/local_identity.rs`、`docs/features/learning/README.md`、issue #48 / #49。

## D-052：MCP 指挥中心是 SkillStar 原生形态，只吸收 Hermes 0.21 的平台能力

- 日期：2026-09-02
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：Hermes Agent v0.21（Pantheon）把 MCP 做成单一桌面指挥中心：已安装机群与目录同页、粘贴即导入、后台健康/重新授权、机群 schema token 加 30 天用量、`hermes://` 深链需确认。SkillStar 已有约两万行 FTS catalog、双纪元探测、CursorJack 级命令确认、多 Agent 投影，以及只按 host 跳到 `#mcp`、丢掉 query 的 `skillstar://` 深链。把 Hermes 的单页堆叠搬过来会卡死界面；编造 30 天用量则不诚实——SkillStar 不是 Agent 运行时，看不到调用次数。
- 决策：① 页面 IA 由 [D-059](#d-059mcp-配置页只留配置与商店) 取代（D-057 指定的「已安装 | 目录」随之作废）：MCP 是配置页（配置 | 商店），不是四 tab 指挥中心。禁止把两万行目录堆在已装列表下面一次滚完；商店是搜索 + 筛选 + 精选安装卡片，发布者是商店内的「精选 | 完整目录」范围而不是页面，完整目录走分页查询。② 「粘贴即解析」落在 `skillstar_models::mcp::parse_pasted_mcp`：社区 `mcpServers` JSON、URL、`npx`/`uvx`/`docker` 命令行、`skillstar://mcp?url|catalog|config|command`。解析只返回草稿，**永不自动写入 store**；UI 必须走现有新建表单或市场安装向导的确认路径。③ 已装列表的 schema 成本用 `tools/list` 里 `tools` 数组的紧凑 JSON 字节数，`schema_tokens = ceil(bytes / 4)`。不引入 tiktoken，不展示 30 天用量。④ 配置页首次挂载时对已装列表做一次顺序探测，上限 8，不在 window focus 上跑。`401 + WWW-Authenticate` 仍是 `authorization-required`，不是失败。⑤ `skillstar://mcp` 的 query 打开确认 UI；跳过确认即失败。不新增 crate，不抄 Hermes 页面。Marketplace 不再放 MCP 发现入口。
- 后果：获得——粘贴/深链与市场安装共用同一条确认边界，机群能看到真实的 schema 体积和需要重新授权的 server。承担——超过 8 个已装 server 不会在后台全部探测；token 数字是字节估算不是模型 tokenizer；catalog 深链仍依赖本地快照里真有那一行。
- 证据：`docs/features/mcp/README.md`、`crates/skillstar-models/src/mcp/import_paste.rs`、`crates/skillstar-models/src/mcp/probe/`、`src/pages/Mcp.tsx`、`src/lib/deepLink.ts`、issue [#79](https://github.com/xxww0098/SkillStar/issues/79)。

## D-053：移除学习功能与 skillstar-learning

- 日期：2026-09-02
- 状态：accepted
- 背景：Learn 页、私人教程、Guide/进度/Draft 与 ACP 教程生成构成独立产品域，但不再作为 SkillStar 交付面。继续保留 `skillstar-learning`、ACP 子进程和默认 `#learn` 会让安装/分发主路径背负无消费者的 crate、IPC 与 Settings。
- 决策：整枝删除学习功能。去掉 `skillstar-learning` crate、`ss-app::learning`、Learn 页/导航、私人教程面板、Guide/进度/Draft 命令、ACP 教程生成（含 Settings ACP 段与 `src-tauri` ACP client/prompts），以及仅为该域服务的 `ss-skills::tutorial` / `source_identity`。Skills 模式默认落地 `#skills`（`my-skills`）；旧 `#learn` hash 回落到同一页。`ss-skills::content` 快照与 `local_identity` sidecar 仍服务安装/更新，不随学习域删除。`~/.skillstar/learning/`、`~/.skillstar/tutorials/` 与 `config/acp.json` 成为孤儿，不再读写，也不做迁移删除。
- 后果：获得——workspace 少一个域 crate，GUI 不再暴露教程/Guide，ACP 依赖退出二进制。承担——已生成的本地教程/进度不会被应用清理；若将来恢复学习域，以 D-051 的 identity 模型为历史参考，而不是复活本次删除的代码路径。
- 证据：`docs/boundaries.md`、`docs/architecture.md`、`docs/features/frontend/README.md`、本决策对应提交。

## D-054：桌面多开是 SkillStar 原生实例，不绑 Usage catalog

- 日期：2026-09-03
- 状态：accepted
- 背景：Usage 卡片今天是订阅配额 + 默认 live 工具切号，不是启动器。用户需要在本机同时开多个 Cursor / Grok Bot / Antigravity，各自独立 Chromium/Electron profile。把多开塞进 `catalog.rs` / `Subscription`、或复用 `open_usage_card_window` / `open_external_url`，会把额度卡伪装成 IDE 启动器，也会把 Grok Bot 桌面应用误绑到 xAI CLI（`xai` / `~/.grok`），或把 Claude Desktop 误绑到 `anthropic`。
- 决策：① 独立实例清单与 profile，落点固定为 `~/.skillstar/instances/<app>/<id>/`，清单在 `config/app_instances.json`。禁止使用 `~/.grok-bot-profiles` 或第三方 Cockpit 目录。② 只交付已验证能隔离的三个 macOS 应用：Cursor（`--user-data-dir <dir> --new-window`）、Grok Bot desktop（`grok-bot`，`--user-data-dir <dir>`）、Antigravity（**必须** `--user-data-dir=<dir>` 等号形式，空格形式会被丢掉并附着默认 profile）。③ Claude Desktop 忽略 `--user-data-dir` 与 `CLAUDE_USER_DATA_DIR`，不交付；UI 若提及须标明原因。④ 不把 Grok Bot 启动绑到 catalog `xai`，不把 Claude 启动绑到 `anthropic`。⑤ 启动经域层 `/usr/bin/open -n`，停止只杀 cmdline 匹配该 `user-data-dir` 的 PID。Windows/Linux 返回明确不支持。⑥ 默认 live 切号仍只写各工具自己的默认存储；实例 Start 不得改写 `~/Library/Application Support/Cursor` 或 `~/.grok`。
- 后果：获得——同一台 Mac 可同时跑两份 Cursor / Grok Bot / Antigravity 而不污染默认 profile。承担——语言服务等仍可能写共享日志目录；Claude Desktop 无法多开；非 macOS 没有启动能力。
- 证据：`crates/ss-app/src/instances/`、`docs/features/usage/README.md`、本决策对应提交。

## D-055：外部技术规范可以成为产品无关叶子 crate

- 日期：2026-09-07
- 状态：superseded（2026-10-07，D-099 将 frontmatter 收回技能域）
- 背景：D-002 要求新能力先留在最内聚的现有 crate，只有独立变更节奏、依赖集合或 deletion test 证明收益时才拆出。Agent Skills `SKILL.md` frontmatter 是上游公开规范（agentskills.io），其解析与诊断不依赖 SkillStar 的 Hub、lockfile、discovery 或安装编排；继续放在 `ss-skills::validation` 会把规范演进绑在产品 crate 的编译单元上。D-049 吸收的是通不过 deletion test 的浅产品 crate，不禁止真正的规范叶子。
- 决策：满足 D-002 时，外部技术规范可以成为产品无关叶子 crate。第一例是 `skill-spec`：只拥有 SKILL.md frontmatter 解析与 issue 诊断，不得依赖任何 `ss-*` crate，也不得引入 Tauri、业务 HTTP/DB 或打包库。`ss-skills::validation` 保留公开路径与 `ensure_installable` 产品 adapter。同类候选（如 MCP registry schema）沿用同一规则，但不在本决策中预建空 crate。
- 后果：获得——规范解析可独立测试、独立跟随上游，产品 crate 不再编译这份纯 YAML 逻辑的反向依赖。承担——多一个 workspace member；产品策略（何为阻塞、安装错误文案）必须留在 adapter，不能渗进叶子。Deletion test：删掉 `skill-spec` 只会把 frontmatter 解析搬回 `ss-skills`，不会拆散安装/发现/打包；它能独立存在是因为上游规范节奏与极小依赖集，而不是因为产品编排需要它。
- 证据：`crates/skill-spec/`、`crates/ss-skills/src/validation.rs`、`scripts/internal/check_workspace_deps.sh`、[boundaries.md](./boundaries.md) 协议叶子规则。

## D-056：团队智能留在 ss-skills 私有 module

- 日期：2026-09-09
- 状态：accepted
- 背景：teamai-cli 的产品是 Execution × Context × Improvement 闭环。SkillStar 已覆盖 Execution（install / deploy / channels / patrol）。缺口是对本机已安装 Skill 的检索、摩擦沉淀和健康度，而不是 Marketplace FTS，也不是已按 [D-053](#d-053移除学习功能与-skillstar-learning) 删除的教程/Guide/ACP 学习域。新开 crate 通不过 D-002 的 deletion test：变更节奏与 `ss-skills` 的内容/安装语料绑定，依赖集也不独立。
- 决策：团队智能作为 `ss-skills::team` 的私有 module + 窄 facade。语料只读已安装 `SKILL.md` 与本地 notes；持久化仅为 `state/team.json`（路径由 core 解析）。CLI 为 `skillstar team …`。不新增 crate，不复活 `skillstar-learning`，不读写 `~/.skillstar/learning/`，本切片不加 Tauri command / GUI。`skillstar find` 继续只搜 Marketplace。
- 后果：获得——Context/Improvement 的第一刀可在现有 crate 内测试与发布，且不会把教程域带回来。承担——GUI 与频道推送/晋升为 Skill 仍是后续切片；本机 notes 不跨设备。
- 证据：`crates/ss-skills/src/team/`、`crates/ss-app/src/cli/team.rs`、[docs/features/team/README.md](./features/team/README.md)。

## D-057：MCP 是配置页，不是指挥中心

- 日期：2026-09-10
- 状态：superseded by [D-059](#d-059mcp-配置页只留配置与商店)；功能已被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 删除
- 背景：[D-052](#d-052mcp-指挥中心是-skillstar-原生形态只吸收-hermes-021-的平台能力) 吸收了 Hermes 0.21 的平台能力（粘贴解析、探测上限、深链确认、禁止 21k 同页堆叠），但把 MCP 画成 **Fleet | Official** 主分段外加 Tools / Sources 次级 tab。四个入口对一个配置域过重：已装列表才是每天的工作台，目录是添加路径，投影目标和目录源是检查器。工具状态再做成带指标条和筛选的迷你仪表盘，是同一错误的第二次。
- 决策：MCP 页面是配置页。默认表面是已安装服务器；**目录**是第二个视图（curated publisher grid + 精选卡片，GitHub 钻入仍分页）。Agent 配置与目录源从工具栏打开为检查器，不是对等 tab。推荐芯片只出现在添加表单，目录页不再放第二份推荐条。D-052 的粘贴解析、探测上限、深链确认、Marketplace 不再放 MCP 发现入口，全部保留。
- 后果：获得——侧栏 MCP 不再假装指挥中心，检查器按需出现。承担——目录源和投影状态要多点一次工具栏图标。
- 证据：`src/pages/Mcp.tsx`、`docs/features/mcp/README.md`（已随 D-074 删除）。

## D-058：curated 发布者的退役由代码注册表驱动清理

- 日期：2026-09-11
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：curated MCP 行是 *code as data*，`seeds::default_curated_mcp_servers()` 是唯一真相，`mcp_curated_server` 由 seeding 对齐。此前 `CURATED_ORDER` 的注释宣称「从顺序表移除一个条目只会把它从 grid 隐藏，行还在库里」，于是每次下线都必须自带一条手写 `DELETE` 迁移——两份真相，必然漂移。同时 [D-057](#d-057mcp-是配置页不是指挥中心) 之后 Official 页面直接读 curated 行，「去掉一个发布者」不再等于「从 grid 隐藏」。本次下线智谱（BigModel）是该场景第一次真实发生。
- 决策：curated 行的生命周期完全由代码注册表决定。seeding 每次运行都把 id 已不在注册表里的行连同其 FTS 行一起删除（`prune_retired_curated_rows`），不再要求手写迁移；`CURATED_ORDER` 只负责展示顺序与显示名，不再充当隐藏开关。下线一个发布者 = 从注册表与顺序表同时移除。
- 后果：获得——新增或退役发布者只有一处改动，Official 不会留下孤儿卡片。承担——已装过该发布者服务器的用户，store 条目保留（仍可正常使用、编辑与投影），但目录行消失，因此三态标记只会显示「已安装、无更新」，既不会被判成「有更新」也不会被判成「已弃用」；这是刻意的，没有目录行就没有可比较的来源指纹。要恢复该发布者时重新加回注册表即可，旧 store 条目会重新匹配上。
- 证据：`crates/ss-marketplace/src/mcp_snapshot/seeds/mod.rs`、`crates/ss-marketplace/src/mcp_snapshot/query/publishers.rs`、`crates/ss-marketplace/src/mcp_snapshot/seeding.rs`。

## D-059：MCP 配置页只留配置与商店

- 日期：2026-09-11
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：[D-057](#d-057mcp-是配置页不是指挥中心) 把四个 tab 收敛成「已安装 | 目录」，方向对，但只搬走了导航，没搬走仪表盘：已装页仍在列表之上压着健康汇总条（全部 / 健康 / 需登录 / 异常芯片 + schema token + 探测上限）、工具栏计数徽章、更新可用徽章、未检查更新文本、常驻的粘贴大文本框和整页拖拽遮罩，每张卡片还各有一个独立探测按钮；添加服务器有四个入口（工具栏按钮、常驻粘贴条、从工具导入按钮、表单里的推荐芯片）；商店页顶部是一整块发布者网格，而它唯一的用途是钻入某一个发布者。这些都是「把运行观测塞进配置页」，不是「配置」也不是「商店」。
- 决策：配置页只有两个视图，每个视图只答一个问题。**配置** = 已装列表 + 搜索 + 按 Agent 筛选 + 新建/安装/编辑悬浮窗；健康只作为卡片上的状态点，再探一次在编辑悬浮窗的探测面板里——页面不再有健康汇总条、计数/更新徽章、常驻粘贴条、整页拖拽遮罩或每卡探测按钮。**商店** = 搜索 + 筛选 + 精选安装卡片 + 「精选 | 完整目录」范围切换；发布者从页面降级为范围，发布者 grid 与发布者详情子页删除。添加服务器收敛为**一个**入口：工具栏按钮打开一个弹窗，弹窗内用模式切换承载「推荐 / 手动填写 / 粘贴解析 / 从工具导入」四条来源。D-052 的粘贴解析、探测上限（8）、深链确认、禁止 21k 同页堆叠全部保留——只是换了承载面。
- 后果：获得——一个配置域不再自带仪表盘，工具栏的 4 个动作收敛为 3 个（工具检查器 / 同步 / 添加），添加路径从四个入口变成一条，商店少一层导航。承担——列表上不再一眼看到「几个健康、几个需登录」，要读卡片状态点或打开某张卡片；商店不再有发布者 hero，进完整目录要多点一次范围切换。
- 证据：`src/pages/Mcp.tsx`、`src/features/mcp/components/McpManager.tsx`、`src/features/mcp/components/McpAddDialog.tsx`、`src/features/mcp/components/McpMarketPage.tsx`、`docs/features/mcp/README.md`（已随 D-074 删除）。

## D-060：MCP 商店精选收敛为编程向白名单

- 日期：2026-09-11
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：curated 种子层此前散在 `mod.rs` 内联的发布者小节、`publishers.rs` 的 9 个 `*_curated_servers()` 和一个单独的 `bigmodel.rs` 里，共 21 行，把编程工具和指纹浏览器（AdsPower）、桌面自动化（Cua Driver）、社交发帖（X）、笔记（Notion）、设计、支付、地图、云盘、流量分析混在同一屏；内置推荐芯片（19 项）又把同一片混杂复制了第二份。其中三个种子标识符（`@modelcontextprotocol/server-git`、`server-fetch`、`server-brave-search`）早已被上游归档，装上去就是死的。商店首屏本应是「我们为写代码背书的东西」，混杂之后用户读不出「官方推荐」和「恰好热门」的区别，这个承诺就失效了。
- 决策：精选是**编程向白名单**，不是「好用的 MCP 合集」。只收写代码与调试会用到的能力（版本控制、代码/文档检索、数据库、容器与云、可观测性），加上通用基础能力（文件系统、网页抓取、记忆、顺序思考、时间）；指纹浏览器、桌面自动化、社交、笔记、设计、支付、地图、云盘、流量分析一律不进精选，它们的发现路径是商店的「完整目录」。判断标准是「它是否让写代码更容易」。落地：种子层从散在 `mod.rs` / `publishers.rs` / `bigmodel.rs` 的发布者小节收敛为**一张 `catalog()` 数据表** + 一个 builder（加一条 server = 加一行 spec）；内置 preset 目录收敛为与 curated `recommended` 集合逐字节对齐的 12 项；被下线的条目（adspower / cua-driver / notion / x / supabase 等）不写删除迁移——[D-058](#d-058curated-发布者的退役由代码注册表驱动清理) 的 `prune_retired_curated_rows` 已让 curated 行的生命周期完全由代码注册表决定。所有收录条目的运行时标识符都对着官方 MCP Registry 与 PyPI 重新核实过（不再沿用归档包的旧名）。
- 后果：获得——商店首屏每一张卡都能回答「它为什么在编程场景里」；内置兜底目录与 curated `recommended` 不再各自漂移（曾经的 drift 是同一 server 出现两次）；加/删/改条目都是一处改动。承担——想装非编程 MCP 的用户要多点一次「完整目录」；`import_paste.rs` 仍保留 `cua-driver` 作为粘贴命令行识别的一个已知 launcher，因为粘贴导入是通用能力而不是商店条目（该分支确认无用可另行删除）；「纯 docker 包 + 容器内必填环境变量」的 server 暂时进不了精选，原因是安装计划器的 docker 环境变量转发缺陷，不是产品取舍（见 `docs/features/mcp/README.md`，已随 D-074 删除）。
- 证据：`crates/ss-marketplace/src/mcp_snapshot/seeds/catalog.rs`、`crates/ss-marketplace/src/mcp_snapshot/seeds/helpers.rs`、`crates/ss-marketplace/src/mcp_snapshot/query/publishers.rs`、`crates/skillstar-models/src/mcp/presets.rs`、`docs/features/mcp/README.md`（已随 D-074 删除）。

## D-061：精选收敛为推荐短名单

- 日期：2026-09-12
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：[D-060](#d-060mcp-商店精选收敛为编程向白名单) 把精选收敛成编程向白名单后还剩 29 行、八个发布者桶——数据库、云与集群、issue 跟踪、抓取、文档转换混在首屏，「推荐」标记只落在 8 条上。用户读这屏时的真实问题是「我装哪几个」，一个比芯片区大四倍的网格把答案稀释了；同时发布者分桶（谁做的）不是用户筛选心智（干什么用）。
- 决策：精选即推荐短名单——只保留常用工具型 MCP（编程 + 设计创作；条目数随 `catalog()` 注册表走，文档不钉数量），每条都 `recommended`，`source` 从发布者桶改为功能货架（`core` / `context` / `browser` / `creative`），商店精选页按货架分区渲染。内置 preset 兜底目录保持与精选逐字节对齐。被下线的条目不写迁移，`prune_retired_curated_rows`（[D-058](#d-058curated-发布者的退役由代码注册表驱动清理)）照旧清理；它们的发现路径是「完整目录」。筛选面板只对完整目录暴露（短名单上 kind/许可证/stars 筛选无意义），切换范围时清掉除搜索词外的 narrowing。D-060 的「一张 `catalog()` 数据表」机制不变。
- 后果：获得——商店首屏就是完整答案（核心 / 上下文 / 浏览器 / 创作四段），卡片按服务身份出图标；精选与推荐芯片是同一份清单，不再各自解释「为什么推荐」。承担——精选覆盖变窄，数据库、云等场景要切完整目录；`source` 值从发布者语义改为货架语义，旧安装条目指纹里的 `source_id` 保留旧桶名（仅作信息字段，不影响判定）。
- 证据：`crates/ss-marketplace/src/mcp_snapshot/seeds/catalog.rs`、`crates/ss-marketplace/src/mcp_snapshot/query/publishers.rs`、`crates/skillstar-models/src/mcp/presets.rs`、`src/features/mcp/lib/curatedShelves.ts`、`src/features/mcp/components/McpMarketBrowser.tsx`、`src/features/mcp/components/McpMarketCard.tsx`、`docs/features/mcp/README.md`（已随 D-074 删除）。

## D-062：MCP 工具配置写入以「没得删就不落盘 + 原子替换 + 按格式分档保真」为契约

- 日期：2026-09-12
- 状态：superseded（被 [D-074](#d-074删除-mcp-管理与-mcp-商店) 取代，功能已删除）
- 背景：MCP 的每个 target 都有自己的配置文件（`~/.claude.json`、`~/.codex/config.toml`、`~/.gemini/settings.json` …），里面绝大部分内容与 SkillStar 无关，而写入实现是「读整份 → 改自己那个键 → 整份写回」。三个具体问题：① `sync_server_public_tools` 对每个未启用的 target 也调 remove，remove 又无条件写回，于是「装一个 server」会把机器上所有 Agent 配置文件重新序列化一遍并各生成一份 backup；② 写入用裸 `std::fs::write` 覆盖，进程被杀会留下截断配置，而回滚只在 write 返回 `Err` 时执行；③ Codex/Grok 的 TOML 走 `toml::Table` 值模型，合并一次就删光用户写在该文件里的全部注释。
- 决策：写入契约收口为三条。**没得删就不落盘**——目标文件里没有这个 key 时 remove 立即返回，不重写、不备份；**原子替换**——所有 live config writer 复用 `ss_core::infra::fs_ops::atomic_write`（同目录 tmp + fsync + rename + 保留原权限），rollback 语义不变；**保真度按格式分档**——TOML 换 `toml_edit` 文档模型保留注释/空行/键序，YAML 与 JSON 保持值模型（YAML 键序保留、注释丢失；JSON 键序重排）。**不开** `serde_json/preserve_order`：它是全局序列化语义变更，会让 `tool_sync` 的逐字节基线失效，换来的只是 JSON 键序这一项排版收益。
- 后果：获得——常规路径（另一个 target 上的开关翻转、同步时的未启用 target）不再触碰无关文件；崩溃不会留下截断配置；Codex/Grok 用户手写的注释不再被一次安装删除。承担——JSON 目标的键序仍会在真正发生增删时被重排（不改变语义，也不影响任何客户端解析），YAML 注释仍会丢；`skillstar-models` 新增 `toml_edit` 依赖（单 crate 使用，按根 Cargo.toml 约定不进 workspace 表）；两个格式各有独立 writer，新增 target 时必须先决定它属于哪一档。
- 证据：`crates/skillstar-models/src/mcp/tools.rs`、`crates/skillstar-models/src/mcp/hermes.rs`、`crates/skillstar-models/src/mcp/dsh.rs`、`crates/skillstar-models/src/mcp/tests_lossless.rs`、`crates/ss-core/src/infra/fs_ops.rs`、`docs/features/mcp/README.md`（已随 D-074 删除）。

## D-063：代表副本唯一物化与永不整仓下载

- 日期：2026-09-21
- 状态：superseded（D-081）（代表副本的排名与「内容不同的副本一律物化」被 [D-075](#d-075异形分发仓库只有一张选副本表) 取代）
- 背景：分发型仓库（如 `pbakaus/impeccable`）把同一技能镜像进十几个 harness 目录，还携带 Rust/Node 工程等重型非技能内容。旧管线把**每个**含 `SKILL.md` 的目录都加入稀疏检出，逐副本懒取 blob；blob 物化一旦在镜像上失败（HTTP/2 framing 等），回退是**删掉部分克隆、整仓浅克隆**——等于把整个 monorepo 全量下载。加上全局事务锁串行一切安装、marketplace 安装无进度反馈，用户感知就是"卡死"。
- 决策：四条。**tree-SHA inventory**——treeless partial clone 的 `git ls-tree -t` 免费携带每目录 tree SHA，相同 SHA 即逐字节相同副本；每个 identity 只物化一个代表目录（manifest 声明 > `skills/<name>` > `.agents/skills/<name>` > 已安装 source_folder > 字典序），相同 SHA 的重复副本记入 `.git/skillstar-inventory.json` deferred 集合按需增量物化，**内容不同的副本一律物化**（frontmatter 可能是另一个 identity）。**永不整仓下载**——checkout blob 硬失败先去 mirror 直连重试一次，克隆整体失败改走 codeload `tar.gz` 单次 HTTPS（匿名 mirror 链）选择性解压 + 本地合成 commit 构建 cache（`skillstar.transport=tarball` 标记），完整浅克隆仅作最后手段。**锁粒度**——网络/发现阶段只持每仓库 cache 锁（`state/repo-locks/`），hub 提交才持全局短锁，锁序恒为 repo → global；baseline 刷新用 `state/lockfile.lock` 跨进程互斥。**基线 stat 短路**——fetch 前的 cleanliness 证明用上次可信快照的 mtime/size 指纹（`state/snapshot-stats/`）代替全字节重读，指纹失配即回退全量快照，fail-closed 语义不变。
- 后果：获得——impeccable 形态的安装从"物化 15+ 副本、失败即全量下载 monorepo"变为"物化 1 份代表副本、镜像协议坏了走单次 HTTPS"；不同仓库安装互不排队；安装阶段可见。承担——相同 SHA 副本的 harness 切换多一次增量物化；tarball cache 无真实 git 历史，更新走重下归档；mtime/size 指纹理论上可被刻意保时间的编辑绕过（与 make/git 同级信任，且有全量快照兜底）。
- 证据：`crates/ss-skills/src/repo_scanner/inventory.rs`、`crates/ss-skills/src/tarball_fetch.rs`、`crates/ss-skills/src/content_stats.rs`、`crates/ss-skills/src/skill_update/transaction.rs`、`crates/ss-git/src/ops.rs`、`crates/ss-git/src/tree.rs`、`crates/ss-skills/src/repo_scanner/cache.rs`、`crates/ss-skills/src/skill_install.rs`、[docs/features/skills/README.md](./features/skills/README.md)。

## D-065：SkillStar 作为 MCP 服务时不进入外部 MCP catalog

- 日期：2026-09-22
- 状态：accepted
- 背景：开发 Agent 需要本机 stdio 调用 SkillStar 来推荐并启用项目技能。外部 MCP 的 store、安装计划和 marketplace 模型已经由 `skillstar_models::mcp`、`ss_marketplace` 和 `ss_app::mcp` 分三层持有。把本机服务塞进其中任一层会让「SkillStar 调用别人」和「别人调用 SkillStar」共用一套类型。
- 决策：本机服务留在 `ss_app::project_skills_mcp`。进程入口是 `skillstar mcp serve --stdio`，stdout 只有 JSON-RPC。工具参数在 `protocol`，不接收批准字段。`rmcp` 只加入 `ss-app`，不新增 crate。项目部署仍由 `ss-skills::projects` 执行，并持有 `state/project-write.lock`。
- 后果：获得——外部 MCP 安装流程不被项目技能协议类型污染；CLI 与将来的桌面批准可以调用同一套领域函数。承担——stdio 传输和工具 schema 的演进跟 `rmcp` 走，不跟 marketplace 的 server.json 走。
- 证据：`crates/ss-app/src/project_skills_mcp/`、[docs/features/project-skills-mcp/README.md](./features/project-skills-mcp/README.md)、[boundaries.md](./boundaries.md) 的项目技能 MCP 接缝。

## D-067：Provider 私有状态用通用加密 blob

- 日期：2026-09-22
- 状态：accepted
- 背景：新 provider 的刷新上下文（Kiro IDC 的 client secret、Trae 设备密钥、Windsurf apiKey）形状各不相同。把它们塞进 `platform_token_encrypted` 会改变 DeepSeek 平台 token 的含义。
- 决策：`Subscription.provider_state_encrypted` 保存 AES-GCM 的版本化 JSON。DTO 只通过 `has_credential` 感知它。各 provider 自己定义明文形状。
- 后果：获得——加 provider 不必改订阅表。承担——blob 没有统一 schema，读错版本必须由该 provider 报错。
- 证据：`crates/ss-usage/src/subscription.rs`、`storage.rs` 的窄 patch、`specs/usage-cockpit-parity/choices.md`。

## D-068：OAuth 完成方式是显式四态

- 日期：2026-09-22
- 状态：accepted
- 背景：本地回调、服务端轮询、自定义 scheme 粘贴和本机采纳不能再靠 catalog id 在前端分支。
- 决策：`OAuthStartInfo.flow` 为 `LocalCallback` / `RemotePoll` / `SchemePaste` / `Immediate`。前端只按 flow 渲染。pending 登录不落盘。
- 后果：获得——新登录形态不用改面板分发。承担——进程重启会丢掉未完成的登录。
- 证据：`crates/ss-usage/src/fetchers/oauth/start_info.rs`、`src/features/usage/components/subscriptionEdit/oauth/OAuthLoginPanel.tsx`。

## D-069：IDE 切号走适配器注册表

- 日期：2026-09-22
- 状态：accepted
- 背景：Antigravity 和 Cursor 的写回是两段硬编码。后续 IDE 若再加 if，切号顺序会分叉。
- 决策：`usage_switch::ide::IdeCredentialAdapter` 注册表负责备份、写入、回读、最后才 pin。CLI 软链目标保持原样。OAuth 完成后重写本机存储的范围是「有 IDE 适配器，或 catalog 为 xai」。
- 后果：获得——新 IDE 只加一个适配器。承担——适配器文件容易变长，必须按 provider 拆开。
- 证据：`crates/ss-app/src/usage_switch/ide.rs`。

## D-070：多开入口只放实机验证过的应用

- 日期：2026-09-22
- 状态：accepted
- 背景：新 IDE 大多是 Chromium，看起来能用 `--user-data-dir`。没有逐个启动并核对登录态隔离之前，登记成可多开会让用户切到一份坏的 profile。
- 决策：Windsurf、Kiro、Qoder、CodeBuddy、CodeBuddy CN、ZCode 和四个 Trae 只登记为 Pending，不进 `INSTANCE_CATALOG_IDS`。Zed 和 GitHub Copilot 结构性 Blocked：Zed 没有独立数据目录且钥匙串是全局的；Copilot 没有独立应用。Claude Desktop 维持原有不支持。
- 后果：获得——界面不会提供未验证的多开。承担——这些应用的多开要等一次实机记录才能打开。
- 证据：`crates/ss-app/src/instances/apps.rs`、`src/features/usage/lib/desktopApps.ts`。

## D-071：不做 Antigravity 语言服务唤醒网关

- 日期：2026-09-22
- 状态：accepted
- 背景：参照实现用本地 TLS 网关冒充官方语言服务，发合成 Cascade 消息，把 5 小时或每周配额窗口提前重置。这是第一次主动消耗上游额度，而不只是读配额或写本机凭据。
- 决策：不实现唤醒网关，也不做直连探活的替代路径。用量监控不依赖它。若以后要做，另立 spec，且不得复用 `usage_switch` 的 pin 语义。
- 后果：获得——避开服务条款和风控风险，也少掉约两千行进程与证书代码。承担——配额窗口仍按服务端自己的节奏重置。
- 证据：`specs/usage-cockpit-parity/slices/27-wakeup-decision.md`。

## D-072：密钥纯本地加密 JSON 存储，彻底禁用系统 Keychain 写入

- 日期：2026-09-22
- 状态：accepted（定点修订：Claude Code 自己的钥匙串项豁免，见 D-083）
- 背景：原系统中 SSH 密码与部分 IDE/CLI（如 Zed 的 internet-password、Antigravity 与 Codex 的 generic-password）会向系统 Keychain / Keyring 写入凭据。用户明确要求所有凭据必须完全保存在本地加密 JSON 中，严禁写入 macOS Keychain 或系统钥匙串。
- 决策：① 移除根依赖 `keyring`。② SSH 凭据由 `EncryptedJsonSecretStore`（落盘在 `state/ssh_credentials.json`，权限 0600，AES-256-GCM sealed，派生自 machine-id）完全接管，保留 `KeyringSecretStore` 作为向后兼容单元结构体但仅转发到本地加密存储。③ 彻底移除所有向 macOS Keychain 的写操作：Antigravity 切号仅写入本地 SQLite `state.vscdb`；Codex `publish_external` / `write_merged` 静默忽略不写钥匙串；Zed 禁用钥匙串写回（切号适配器标记为不可用），`keychain_cli` 的 `add_internet_password` 与 `delete_internet_password` 明确拒绝写入。
- 后果：获得——所有敏感密钥与凭据纯净保留在用户应用本地加密文件内，无系统钥匙串提权弹窗、无外溢、可审计、与平台钥匙串彻底解耦。承担——Zed 无法通过写钥匙串实现外部 IDE 自动切号（Zed 保持仅本地导入与用量监控）。
- 证据：`crates/ss-sync/src/ssh/store.rs`、`crates/ss-app/src/usage_switch/{antigravity.rs,keychain.rs,zed.rs}`、`crates/ss-usage/src/tool_store/keychain_cli.rs`。

## D-073：模型网关是只依赖 core 的独立 crate

- 日期：2026-09-29
- 状态：accepted
- 背景：Models 要有本机网关。放进 `skillstar-models` 会让协议栈和密钥表绑在同一次编译里。让网关依赖 models 或 usage，则拿掉网关时会把密钥表或订阅存储的类型一起带走。
- 决策：网关是 `skillstar-gateway`，skillstar 依赖只有 `ss-core`。密钥表仍属 models，订阅仍属 usage。产品编排要到监听那一档才由 `ss-app` 依赖它。
- 后果：获得——删掉 gateway 之后 models 与 usage 仍能编译。承担——网关不能自己打开密钥表或配额库，调用方把快照交进来。
- 证据：`crates/skillstar-gateway`、`scripts/internal/check_workspace_deps.sh`。

## D-076：模型网关硬切换，不迁移 Agent 文件

- 日期：2026-09-29
- 状态：accepted
- 背景：旧的 tool sync 把厂商 URL 和 API key 写进 Agent 配置。网关接管之后，那些文件里的旧值不能在启动或保存密钥时被改写，否则用户还没在新工作台保存，Codex 或 Claude 就已经指向别处。
- 决策：不迁移。启动不改写 Agent 文件。保存 provider 不触发 Agent 写盘。v4 `model_providers.json` 不加路由列，`version` 仍是 4。厂商密钥留在 provider store。Codex 的环回配置只在显式保存时由 `skillstar-gateway` 写入。
- 后果：获得——已有 Agent 文件保持原样，直到用户保存这一档。承担——`[model_providers.skillstar]` 里留下的 `wire_api = "chat"` 要等 API 形态的这次保存才换掉，其它表不动；`tool_sync` 的六个 sync 不再写厂商 URL 或密钥。
- 证据：`crates/skillstar-gateway/src/codex.rs`、`crates/skillstar-models/tests/startup_agent_files.rs`。

## D-074：删除 MCP 管理与 MCP 商店

- 日期：2026-09-29
- 状态：accepted
- 背景：MCP 管理要把一份 server store 投影到约 18 个 Agent 目标的原生配置里，而这些格式互不一致：载体有 JSON、TOML、YAML 和 DSH patch，顶层键各不相同（`mcpServers`、`mcp`、`mcp_servers`、`mcp.servers` …），传输字段有五种写法，可选字段的支持也参差不齐。写入对象是用户与 Agent 共享的配置文件，每条写路径都要做到无损保真、解析失败即 fail-closed、原子替换，还要为改名或下线的目标维护 cleanup 墓碑。目标本身还在不断变格式、改路径、改名。这部分维护成本和出错面长期高于它给用户的价值；MCP 商店（多源 registry 快照、curated 精选、安装计划）又完全建立在这层投影之上。
- 决策：整体删除 MCP 管理与 MCP 商店：`skillstar-models::mcp`、`ss-marketplace` 的 MCP catalog/快照、`ss-app::mcp`、对应 Tauri 命令、前端 MCP 页面与 IPC、网络诊断里的 MCP Registry 探测。项目技能 MCP（`ss_app::project_skills_mcp`，SkillStar 自身作为 stdio MCP server，见 [D-065](#d-065skillstar-作为-mcp-服务时不进入外部-mcp-catalog)）与它无关，完整保留。
- 后果：获得——不再承担十几种第三方配置格式的写入正确性，也少了一整个快照同步域。承担——SkillStar 不再管理 MCP：用户已有的 `~/.skillstar/config/mcp_servers.json` 和各 Agent 配置里已写入的 MCP 条目原样保留、不做任何清理；市场快照 schema v14 迁移 `DROP` 掉 `mcp_registry_server`、`mcp_curated_server` 及其 FTS 表，并删除 `marketplace_sync_state` 中 `mcp_registry` / `mcp_registry:*` 行，旧 v8/v10/v13 迁移随之移除；`skillstar://mcp` 深链失效，按未知目标忽略。
- 证据：`crates/ss-marketplace/src/snapshot/migrations.rs`（v14）、`src/lib/deepLink.ts`、[marketplace](./features/marketplace/README.md)。

## D-075：异形分发仓库只有一张选副本表

- 日期：2026-09-29
- 状态：superseded（D-081）（按 [specs/irregular-skill-packs](../specs/irregular-skill-packs/README.md) 逐档补全）
- 背景：`pbakaus/impeccable` 这类仓库把一个技能改写成二十份 harness 专用副本：`name` 相同，字节不同，正文里写死各自路径。另外还有插件包装副本和 `tests/` 夹具。SkillStar 原来有三套互不一致的排名：inventory 优先 manifest，discovery 的 root(4)/catalog(3)/`.agent`·`.agents`(2)/其他(1)，harness 回退链自成一套。平局时取 `read_dir` 顺序。`plugin.json` 里字符串形式的 `skills`（容器路径）被当成单个技能路径取了父目录。全递归扫描还会把测试夹具当技能列出来。
- 决策：
  - **选副本只有一张表**：`pack_layout::choose_copy`。inventory 代表副本、发现去重、完整性收拢、harness 回退全部调用它，平局一律按路径字典序。
    - 默认：根 → `skills/`·`source/skills/` → `.agents/skills/` → manifest 容器 → 其他。
    - harness `h`：`h/skills/` → `h` → `h` 下其他 → catalog → 已安装 `source_folder` → `.agents/skills/` → manifest → 其他，永不选根。
    - pinned：只接受该目录。
  - `.agent` 不再与 `.agents` 同级。
  - `plugin.json` 的 `skills` 是字符串时，表示容器，推入其本身；是数组时，每一项是技能路径，推入父目录。
  - 全递归发现跳过 `pack_layout::IGNORED_DIR_NAMES`，其中包括 `tests`、`test`、`__tests__`、`fixtures`。被忽略的目录自身仍可以是技能，但不再往下扫。
  - priority 容器补上 `.gemini/skills`。
  - **请求只物化被选中的一份**：harness 请求（轮播点击、单个 `--agent`）用 `Inventory::copy_for` 按 harness 列选出一份延迟副本物化，与安装 chooser 走同一张表，结论必然一致。不指定 harness 的请求直接用已物化的代表副本，不再物化任何副本。
  - **身份按 `name` 判定**：inventory 先按 basename 分组；tree 不同的组，在一次 fetch 里预取各份 `SKILL.md`（连同 `.claude-plugin` manifest），按 frontmatter `name` 细分，每个身份只物化一份，其余延迟。读不出 `name` 时该组退回 tree-SHA 规则。`IGNORED_DIR_NAMES` 下的技能不进 inventory。重新应用稀疏集只增不减（已在磁盘上的目录保留）。sidecar 升到格式 2，旧文件直接丢弃重算，不做迁移。
  - **子路径 URL 是硬钉**：tree URL 的 `subpath` 只发现、只物化那一个目录，`LockEntry.pinned = true`，`source_folder` 写死那个路径。之后任何 harness 点击或普通 URL 重装都是 `existing_same_repo_action::Reuse`（`skill_install_choice.rs`），不再重新走排名表；只有卸载才清掉 `pinned`。子路径落在 `IGNORED_DIR_NAMES` 内部（如 `tests/...`）时例外放行：`InstallQuery.scope` + `SkillDiscovery::within` 按作用域根判断忽略目录，不看祖先路径段。`Source` 是来源规格唯一 owner（加 `Serialize`/`Deserialize`，字段映射 `ScanResult` 已有的 `source`/`source_url`），一次安装只在公共入口解析一次，`&Source` 贯穿到底；`ScanResult` 平铺它（`spec: Source`），扫描预览与安装因此看到同一份 ref/subpath。
  - **Claude 插件只提示，不装 hooks/agents**：仓库带 `.claude-plugin/marketplace.json` 或 `plugin.json`、且声明的插件目录下有 `hooks/` 或 `agents/` 时，`plugin_manifest::plugin_hint` 产出 `PluginHint{hooks, agents}`，GUI 在技能列表上方、CLI 在 `install`/`--list` 输出里各打印一行提示；SkillStar 不解析、不安装、不模拟 hooks/agents 的运行时行为。自己实现钩子注入等于把 `impeccable` 自带的 installer 重写一遍，用户要完整插件体验就用 Claude Code 自带的 `/plugin marketplace add`。
  - **带 `git_ref` 的缓存也走稀疏检出**：`repo_scanner::cache::clone_sparse` 泛化成同时接受 `git_ref: Option<&str>` 和 `extra: &[String]`，把原来「带 ref 就整仓浅克隆」和「不带 ref 就稀疏克隆」两条分支合并成一条——合并后的克隆步骤固定为 treeless clone → `sparse-checkout init --cone` →（有 ref 时）`fetch_and_reset_ref` → `inventory::apply`，顺序不能变：cone 模式必须在 `reset --hard` 之前生效，否则 reset 会把整棵树检出。稀疏克隆失败时的 tarball / 完整浅克隆回退链和不带 ref 的路径完全共用，末尾的完整克隆兜底在有 ref 时额外调一次 `fetch_and_reset_ref`。子路径本身的物化不在这条路径里：克隆只负责把默认代表副本落到本地，钉住的具体子路径由安装（`choose_install_skills`）和扫描预览（`scan_repo_with_mode_in_session`）各自再调 `inventory::materialize_dirs` 补上——04 只在安装路径做了这一步，05 把扫描预览路径的同一个缺口也补上了（否则预览一个子路径 tree URL 会因为文件还没落盘而报「未发现技能」）。
- 后果：
  - 获得：同一组副本在 inventory、发现、安装三处必然选出同一份，结果不依赖文件系统的遍历顺序。impeccable 不指定 harness 时选 `.agents` 版（仓库没有 `skills/`），不再选插件版。真实仓库首装从 15 秒、24 个 `SKILL.md`、1166 个文件降到 9 秒、1 个、80 个；`--agent cursor` 安装从 24 个 `SKILL.md`、1166 个文件降到 2 个、135 个。钉住一个子路径后，换 harness 或重装都不会再换内容，前端可以直接把 tree URL 当成「装这一份，别再变」的入口。
  - 承担：manifest 容器从第一位降到第四位；只在 `.agent/skills` 与其他非 `.agents` 副本之间做选择的仓库，默认副本会按字典序改变。已安装技能的 lock 不受影响。旧格式的 cache 不会收缩，只有新克隆受益。每次规划多一次（批量）blob fetch。`reconstruct_lock_entry`（lock 条目从磁盘重建的修复路径）不知道钉住语义，重建出的条目 `pinned` 恒为 `false`——这是决定 6「不做迁移」的直接后果，接受，走到这条路径本来就是罕见的手工修复场景。**在 06（带 ref 的稀疏缓存）完成之前，任何带 `git_ref` 的 URL（包括所有 tree URL）都走完整浅克隆**，钉住只保证选中的内容正确，不保证少下载。
- 证据：`crates/ss-skills/src/pack_layout.rs`（`copy_selection_table`）、`discovery.rs`、`plugin_manifest.rs`（`plugin_json_string_is_a_container_path`、`plugin_hint_detects_hooks_and_agents`、`plugin_hint_is_none_without_manifest`）、`repo_scanner/inventory.rs`（`impeccable_fixture_materializes_one_copy_per_identity`、`identity_resolution_costs_one_fetch`、`unreadable_manifest_degrades_to_tree_sha_rule`、`reapply_keeps_on_disk_copies_and_adds_installed`）、`skill_install_harness_tests.rs`（`harness_install_materializes_only_the_chosen_copy`、`harness_fallback_materializes_the_installed_source_folder`）、`skill_install_pin_tests.rs`（`tree_url_subpath_installs_exactly_that_copy`、`pinned_subpath_inside_tests_dir_installs`、`pinned_skill_is_not_retargeted_by_harness_click`、`plain_reinstall_keeps_pin`、`ref_pinned_cache_is_sparse`、`tree_url_install_never_materializes_outside_subpath`）、`update_checker/tests.rs`（`pinned_skill_update_follows_its_folder`）、`discovery/tests.rs`（`explicit_scope_inside_tests_is_discovered`）、`source_resolver.rs`（`source_serde_uses_scan_field_names`）、`lockfile.rs`（`pinned_flag_roundtrips_and_is_omitted_when_false`）、`git_skill.rs`（`scan_repo_honors_tree_url_ref_and_subpath`）、`repo_scanner/scan_install.rs`（`install_from_scan_keeps_ref_and_pin`）、`crates/ss-git/src/blobs.rs`、`crates/ss-skills/src/pack_fixture.rs`。部分取代 [D-063](#d-063代表副本唯一物化与永不整仓下载) 的排名那句，以及 [D-044](#d-044pack-根目录-skillmd-垫片不是安装单元) 中 `plugin.json` 字符串的语义。

## D-077：账号切换与桌面多开归位 ss-usage，滚动备份与 sandbox 名下沉 core

- 日期：2026-09-30
- 状态：accepted
- 背景：`ss-app` 按 D-003 只承载跨域 use case，但 `usage_switch`（约 1.6 万行 CLI/IDE 凭证切换引擎）与 `instances`（桌面多开）长期驻留其中，唯一原因是切换引擎借用了 `skillstar_models::tool_sync` 的五个符号（`create_rolling_backup`、三个凭证路径解析、`TOOL_SYNC_HOME_ENV`）——usage 不允许依赖 models，这段「跨域」是借来的假象；而凭证的读侧（fetchers）早已在 `ss-usage`。同一套 sandbox / 凭证路径语义在 models、usage、gateway 三处各写一份，并已开始分叉（usage 的 `codex_auth_path()` 无视 `CODEX_HOME` 与 sandbox）。
- 决策：(1) 无域属性的 `create_rolling_backup` / `cleanup_old_backups` 下沉 `ss_core::infra::fs_ops`，`TOOL_SYNC_HOME_ENV` 与 sandbox 读取收敛为 `ss_core::infra::paths::{TOOL_SYNC_HOME_ENV, tool_sync_home_override}`；models 经 `pub use` 原样再导出，调用方与行为不变。(2) `usage_switch` 整体迁入 `ss-usage`（模块名不变，路径 `ss_usage::usage_switch`），三个凭证路径解析改由 `tool_paths::switch_{codex,grok,opencode}_auth_path` 提供，语义照抄 models：sandbox 永远优先，其次 `CODEX_HOME` / `GROK_HOME` / `XDG_DATA_HOME`。(3) `instances` 一并迁入 usage，维持 D-054「多开不绑定 Usage catalog」的数据边界。(4) `ss_app::usage` facade 与 DTO 投影留在 app，D-034 不变。usage 的 skillstar 依赖归零到只剩 core。
- 后果：获得——Usage 块单 crate 自洽（凭证读写同屋檐），`ss-app` 缩回纯跨域层；滚动备份与 sandbox 变量名不再三处漂移。承担——ss-usage 编译单元变大；凭证路径语义在 models 与 usage 仍各持一份（core 只收敛无域属性部分），以注释互指；app 的 tokio 测试锁与 usage 的 std 测试锁合并为一把 std Mutex（`EnvGuard` 自持锁，跨 await 由 guard 结构体包裹）；fetcher 的 `codex_auth_path()` 刻意保持旧语义（默认安装路径读取），与 `switch_codex_auth_path()` 的差异留待后续统一。
- 证据：`crates/ss-usage/src/usage_switch/`、`crates/ss-usage/src/instances/`、`crates/ss-usage/src/tool_paths.rs`、`crates/ss-usage/src/test_support.rs`、`crates/ss-core/src/infra/fs_ops.rs`、`crates/ss-core/src/infra/paths.rs`、`crates/skillstar-models/src/tool_sync/backup_merge.rs`、`src-tauri/src/commands/instances.rs`、[boundaries.md](./boundaries.md)。

## D-078：LAN 门禁采用 loopback 放行 + 非 loopback 强制安装级 gateway key

- 日期：2026-10-02
- 状态：accepted
- 背景：网关把 `listen` 写成 `lan` 后听 `0.0.0.0`，而写进 Agent 文件的 bearer 全是可预测占位（`skillstar`、`skillstar-<id>`），局域网上任何人都能拿任意占位 bearer 使用这台机器的网关，等于无鉴权（详见 [errors.md](./errors.md)）。对照 magpie `internal/gateway/lan.go` 的 `callerKey`/`local` 模式，需要选定鉴权模型（choices C2）。
- 决策：loopback peer 对任意 bearer 放行，行为零变化（归因通道 `skillstar-<agent>`、选择通道 `skillstar/<model>`、本地 GET 面、Claude MCP callback 自带的环回 + token 双门全部不动）；非 loopback peer（LAN、WSL NAT）对除 callback 外的所有请求强制安装级 gateway key，不做路径白名单（`GET /`、`/api/hello`、`/v1/models` 一并拦），槽位顺序 `Authorization`（剥 `Bearer `）→ `x-api-key` → `x-goog-api-key` → `?key=`，任一匹配即过。key 是 `config_dir()/gateway.key`，≥32 字节随机数的 hex，Unix `0600` 惰性生成（先例 `redact.key`），无 UI、无轮换、不进 DTO/日志/账本。`save_listen("lan")` 与非 loopback `serve` 启动先确保 key 可用，否则拒绝且不产生副作用。WSL NAT 的 Codex 配置写实际 key，mirrored/本机写占位。
- 后果：获得——LAN 暴露面从「任意占位 bearer 即可用」收敛到「持有 key 文件内容才可用」，环回上的既有 Agent 配置一个字节都不用改。承担——对同用户本机攻击者不设防（key 文件用户可读，严格校验环回 bearer 对其增益约 0）；没有鉴权槽的 Agent 在非 loopback 上不可用，OMP（`auth:none`，无法携带 key）仅 loopback 可用，人工检查点确认可接受；key 无进程级缓存，每次非环回请求读一次 key 文件（小文件、LAN 流量低，换取测试沙箱可隔离）。
- 证据：`crates/skillstar-gateway/src/access.rs`、`crates/skillstar-gateway/src/serve.rs`（dispatch 门禁与 `ServeError::LanNeedsKey`）、`crates/skillstar-gateway/src/store/listen.rs`（`SaveListenError::Key`）、`crates/skillstar-gateway/src/codex.rs`（NAT bearer）、`crates/skillstar-gateway/tests/access.rs`、`crates/skillstar-gateway/tests/wsl_codex.rs`，参照 magpie `internal/gateway/lan.go`。

## D-079：model_gateway.json 的 typed schema 归位 gateway 的 store/doc.rs

- 日期：2026-10-02
- 状态：accepted
- 背景：`skillstar-gateway` 对 `model_gateway.json` 的 12 个接触点（5 写方 + 7 读方）各自打开文件、各自用 `serde_json::Value` 做行级手术，未知字段只靠「mutate 整个 Value」幸存；`family` 标签已被 stringly 读取，一等化在即，行形状再无主人就会二次迁移（原 spec 已随 D-082 删除）。
- 决策：`store/doc.rs` 的 `ModelGatewayDoc` 是该文件的唯一 typed schema owner：外层 typed + 行内保留 Value 混合（D5），已知顶层键与 providers/groups 行的共享字段（id/members/routing/affinity/family）typed 承载，未知键经 `#[serde(flatten)]` 整段回写；已知字段只在偏离默认值时序列化，open→save 对「行均为对象」的文件是恒等变换（不凭空造 `members: []` 或 `routing: null`），删键语义（Smart/Auto）留给 lens setter。**family 落在 gateway 的 store/ 而不是 models crate**：`family` 是 `model_gateway.json` 里 providers/groups 行上的字段，文件的主人是 gateway（D-073：models 不打开该文件，gateway 只依赖 core）；若 typed 行形状放进 models，要么让 models 反向依赖 gateway 的文件、要么复制第二份解析，两个都比现状糟。models 侧消费经 gateway 的公共读函数与 app 投影获得（树 spec 07），families 一等化的唯一 schema 改动点是 `store/doc.rs`。**仍非事务（D7 重申）**：open→改→save 是无锁的读-改-写循环，两进程交错可丢写；收口前如此，收口后仍如此，不引入进程内锁，声明由 `store/doc.rs` 模块文档承载。
- 后果：获得——行形状有单一权威定义，读写方改道（04/05）后任何「保字段」缺陷只在一处修；手编键（redact_*、vision、classifier、rules、note）的存活由 flatten 结构性保证而非测试运气。承担——严格 `open()` 对「已知字段形状坏」的文件整体拒绝（旧写方按字段各自宽容），这类文件在旧路径本就无人能安全写入；`model_efforts`/`visible` 无写方，容器不提供 setter；schema 形状是阶梯上唯一不可逆决策，改动即二次迁移。
- 证据：`crates/skillstar-gateway/src/store/doc.rs`、`crates/skillstar-gateway/tests/store_doc.rs`（round-trip/缺文件/坏文件/重复与无 id 行四钉；原 spec 已随 D-082 删除，D5/D7/D8 见 git 历史）。

## D-080：持久用量账本是网关度量面的唯一真相，环只补缺、价格只在读时

- 日期：2026-10-02
- 状态：accepted
- 背景：网关的「转发完成了什么」需要一张可对账的度量面：Usage 页要按 catalog/account/会话归因的今日消耗，Models 工作台要同一模型各候选路由的实测对照（原 spec 已随 D-082 删除）。三个候选真相源——进程内环、各 Agent 自己的会话文件、Usage 的订阅配额——各缺一角：环重启即忘，会话文件不知道网关归因，配额是厂商口径的份额不是实测调用。跨源直接相加会把一次经代理的回合记两遍（环一条 + 文件一行）。
- 决策：持久账本（数据根 ledger JSONL，逐行带 agent/session/模型/token/状态/延迟/catalog/account 归因，无密钥无上游 URL）是网关度量面的唯一真相；进程内 60 条环只为账本缺的行补位（append 失败的那几笔），读取面合并两边（`load_ledger_page`）。上层视图全是账本与会话文件的**派生投影**：`consumption_view` 以两阶段配对（request id 主键，回退 会话+token+时间±2s+成败一致、双侧唯一）吞掉账本已代表的文件行；`summarize`/`crossview` 在其上做汇总与会话 chip；`get_route_comparison` 以账本归因为口径聚合各候选的实测（calls/错误率/p50/p95/token/成本），余量与 rest 状态作为进程事实随行标注、绝不与实测混算。账本只存 token：价格在读时查当前价格表（`effective_price`），价格变更重述历史，UI 恒标「估算」；查不到的调用计入 `unpriced`（未知，不是免费）。芯片、chip、对照卡不新增任何真相字段。
- 后果：获得——「配额（厂商口径）× 实测消耗（网关+会话口径）× 成本」三角的全部数字可从两个源文件复算，对账有落点（docs/features/usage 的口径矩阵）；重启、多开、环丢失都不改变度量面。承担——成本永远是与「当时的」价格表相关的估算而非账单；rest 状态进程内即忘（重启后对照卡的休息标记消失，属诚实降级）；配对窗口 ±2s 与零 token 成功调用不配对是继承 magpie 的取舍，极高频同会话并发可能留下双记行（可见、可对账，优于静默吞行）。
- 证据：`crates/skillstar-gateway/src/ledger/`（append/load/query）、`crates/ss-app/src/usage/consumption/`（mod/summarize/crossview 与配对测试）、`crates/ss-app/src/usage/service/summary.rs`、`crates/ss-app/src/models/gateway/ledger.rs`，参照 magpie `internal/usage`（`gatewayMatches`/`bareModel`）。

## D-081：技能安装、锁与更新整体同步 vercel-labs/skills，删除自研管线

- 日期：2026-10-04
- 状态：accepted（规范副本与锁的位置被 [D-100](#d-100已安装技能的规范副本放在-skillstar-数据根) 取代；安装、更新与锁格式仍以本条为准）
- 背景：D-047 之后的自研层（持久仓库缓存 + 稀疏 inventory、tarball 回退链、lock.json v5 内容基线、update 事务/分歧分类/重命名迁移、ghost 检测、pack_layout 选副本表）累计约一万八千行，语义与 `npx skills` 持续漂移（同名跨源静默跳过、baseline fail-closed 等行为用户不可预期）。用户决策：不再做多余兼容，安装逻辑与 vercel-labs/skills 完全一致。
- 决策：全局安装采用 vercel 布局与语义——canonical 副本是 `~/.agents/skills/<name>` 的真实目录（`SKILLSTAR_DATA_DIR` 设置时落到数据根下以保持开发/测试隔离），各 Agent 目录以**相对**符号链接指向它（Windows junction，失败回退 copy，全局安装对以 `~/.agents/skills` 为自身全局目录的 Agent 不建链）；锁是 vercel 格式 `~/.agents/.skill-lock.json` v3（`$XDG_STATE_HOME` 优先），每技能记 `source/sourceType/sourceUrl/ref/skillPath/skillFolderHash(git tree SHA)/installedAt/updatedAt`，版本不符静默重置；获取是 `git clone --depth 1 [--branch ref]` 到 OS 临时目录、用完即删（SHA-pin 走 init+fetch，LFS 禁用，非交互，认证失败升级 `gh repo clone`→SSH）；发现去重为「优先目录顺序、先见者胜」；frontmatter 门禁对齐 vercel（`name` 与 `description` 必须是字符串，缺失即不可安装）；更新按 source+ref 分组对比上游 tree SHA（GitHub API 优先，克隆回退），变化即覆盖式重装，不检测本地修改。首次启动自动清理旧 `~/.skillstar/hub/{skills,repos}`、`lock.json` 与指向旧 hub 的 Agent 链接（幂等标记）。删除：repo cache/inventory/tarball/pack_layout/lockfile v5/skill_update 事务/update_checker/ghost 检测/仓库缓存管理。取代 D-046、D-047、D-063、D-075；D-024 的全局侧因 canonical 即 `~/.agents/skills` 而消解（项目侧不变）。共享频道保留自身 store 与发布验证，安装/更新写路径改走新核心。
- 后果：获得——与 `npx skills` 目录与锁互通、约一万行自研复杂度删除、跨源同名冲突语义变成 vercel 的「覆盖安装」、更新行为可预期。承担——本地修改会被更新覆盖（与 npx skills 一致）；无持久缓存后每次安装/更新都要网络获取；旧安装需要用户重装一次；ghost 提醒与「上游已移除/更名」迁移流程移除。
- 证据：`crates/ss-skills/src/{skill_lock,fetch,installer,update}.rs`、`docs/features/skills/README.md`。

## D-082：移除模型域，收敛到 Skills 安装管理 + Usage 展示 + 多账号切换

- 日期：2026-10-05
- 状态：accepted
- 背景：SkillStar 曾同时承载技能管理与模型接入（Provider store、本机模型网关、Agent 配置写入、应用内 AI），模型域累计约五万行且与技能域几乎无共享。用户决策：产品重新定域为「技能安装管理 + 用量展示 + 多登录账号切换」，不再配置模型、接入模型。
- 决策：整体删除 `skillstar-models`、`skillstar-gateway`、`skillstar-decision` 三个 crate 及其命令组（`models_commands`、`ai`、`decision`）、`skillstar gateway` / `skillstar decide` CLI、`claude-mcp-helper` 入口、桌面启动时的网关监听、消费汇总的网关账本合并与 401 自愈、路由对比与应用内 AI（skill 摘要、marketplace AI 关键词搜索、MCP 神经重排器）。计价下沉为 `ss-usage::pricing`（只读 `model_gateway.json` 的 `prices` 覆盖与 models.dev 缓存，按模型 id 反查），消费汇总改为会话文件单源；`ss-core::providers`（identity/balance）与 `SKILLSTAR_TOOL_SYNC_HOME` 沙箱保留。磁盘上的模型域用户数据不主动清理。
- 后果：获得——代码面收敛约五万行，产品语义单一，账号切换成为一等能力。承担——经网关流量的归因/计价维度消失（账本数据留存磁盘但不再展示）；会话行只能按模型 id 估值（无法区分中转）；价格表冻结在最后一次 models.dev 同步，不再刷新；应用内 AI 摘要与 AI 搜索不再提供。
- 证据：`crates/ss-usage/src/pricing.rs`、`crates/ss-app/src/usage/consumption/`、`docs/boundaries.md`、`docs/architecture.md`。

## D-083：定点豁免 D-072——允许写 Claude Code 自己的那条钥匙串项

- 日期：2026-10-05
- 状态：accepted
- 背景：D-072 禁止一切系统钥匙串写入，导致 Claude 账号切换在 macOS 不可用——Claude Code 在 macOS 的权威凭证存储就是 generic-password 钥匙串项（service `Claude Code-credentials`），只写 `.credentials.json` 不会被 CLI 读取，假装切换成功是谎言。用户明确授权：允许写 Claude Code 自己的那条钥匙串项。
- 决策：豁免范围仅此一项。`ss-usage::claude_credentials` 拥有唯一寻址实现：账号 = `$USER` / `$LOGNAME` 回退 `claude-code-user`；`CLAUDE_CONFIG_DIR` 非空时 service 追加该目录 SHA-256 前 8 位 hex 后缀（同 Claude Code 自身派生）。登录采用（`fetchers::oauth::anthropic`）保持只读；唯一写入方是切换适配器 `usage_switch::claude`：读现有 blob → 只替换 `claudeAiOauth` 键（`mcpOAuth` 等兄弟键原样保留，item 缺失时从明文文件迁移兄弟键）→ `security add-generic-password -U -X <hex>` 写回 → 回读一致才移动 pin → 删除过期的明文镜像文件（同 CLI 自身迁移行为）。钥匙串 IO 一律 shell out `/usr/bin/security`（不链 `security-framework`，ACL 授权绑定调用方签名），`SKILLSTAR_TOOL_SYNC_HOME` 沙箱内一律拒绝。D-072 对其余钥匙串项（Zed internet-password、Antigravity / Codex generic-password 等）的禁写不变。
- 后果：获得——macOS 上 Claude 多账号切换可用，且与 CLI 自身存储语义一致（同一 service 派生、同一 blob 合并规则）。承担——首次写入可能触发钥匙串确认弹窗；切换写整 blob，兄弟键靠合并保留、回读校验兜底；其余钥匙串禁写约束无变化。
- 证据：`crates/ss-usage/src/claude_credentials.rs`、`crates/ss-usage/src/usage_switch/claude.rs`、`crates/ss-usage/src/fetchers/oauth/anthropic.rs`、`docs/features/usage/README.md`。

## D-085：移除 Usage 顶层模式，导航收敛为 Skills + Accounts

- 日期：2026-10-05
- 状态：accepted
- 背景：D-082 之后产品只剩「技能安装管理 / 用量展示 / 多登录账号切换」三件事，但顶层模式仍是 Skills / Accounts / Usage 三个：Accounts 是账号管理工位，Usage 是只读消费视图，二者的侧栏（同一份 catalog + 账号计数）与页面（同一批订阅卡片、同一个支出摘要）大幅重叠。用户决策：tab 栏只保留 Skills 与 Accounts。
- 决策：删除 Usage 顶层模式及它独占的 UI（`src/pages/Usage.tsx`、`UsageNav`、`UsagePanel`、`TodaySessions` 与只服务它的 `StackedTokenBar`/`numberFormat`）、`AppMode` 的 `"usage"` 值、`#usage` 路由、命令面板的 Usage 条目；⌘5 改绑 Accounts（命令面板早已如此标注）。Usage 域保留为数据与卡片提供方，由 Accounts 经 `src/features/usage/index.ts` 消费；`get_today_consumption` 与 `TodayConsumption` DTO 保留在后端。
- 后果：获得——顶层模式与文档只剩两个，Accounts 成为唯一的消费与账号工位。承担——今日会话 chip 条（slice 13 的前端面）不再显示，`TodayConsumption` 变成无消费方的休眠契约；旧的 `#usage` 链接退化为默认 Skills 首页。
- 证据：`src/components/layout/ModeSwitcher.tsx`、`src/hooks/useNavigation.tsx`、`src/App.tsx`、[boundaries.md](./boundaries.md)、[features/accounts/README.md](./features/accounts/README.md)。

## D-086：按当前 Skills / Accounts 功能收敛 crate 域

- 日期：2026-10-05
- 状态：accepted
- 背景：模型域和 Usage 顶层页面移除后，Provider 元数据只剩账号消费者，账号 facade/消费汇总仍绕经 app；共享频道与技能安装、锁、部署共用事务却分属 crate，并要求每个入口手动注册保护策略。
- 决策：共享频道与 patrol 并入 `ss-skills::channels`；Agent 暂停/恢复、安装后部署、Deck 的 Agent rail 归 `skills::workflows`。账号 facade、DTO 与消费汇总下沉 `ss-usage::accounts`，Provider 元数据下沉其私有 `providers`；保留 usage package 名和所有磁盘/IPC 契约。app 保留 CLI、项目技能 MCP、市场与技能编排及全局维护，不保留单域转发兼容壳。
- 决策：保留 Marketplace 快照/FTS、Git 传输、SSH 传输及 skill-spec 规范叶子的独立边界（规范叶子部分由 D-099 取代）；不为了对应导航而把它们塞入产品大 crate。以完整 workspace 边白名单约束新增依赖。
- 后果：消除 channels → skills、app → usage 的编译依赖；频道保护默认生效，不再依赖组合根注册；合并后的环境变量测试共用所属 crate 的隔离锁。承担技能域编译单元变大，但第三方依赖集合几乎重合；后续拆分须重新证明独立生命周期或依赖收益。D-004/D-049 的 Provider SSOT 不变量保留，位置由本决策取代。
- 证据：`crates/ss-skills/src/channels/`、`crates/ss-skills/src/workflows/`、`crates/ss-usage/src/accounts/`、`scripts/internal/check_workspace_deps.sh`。

## D-087：`~/.skillstar` 存储目录按用途重新分类（v3）

- 日期：2026-10-05
- 状态：accepted
- 背景：v2 布局把订阅存储（含加密 token）、CLI 凭据快照、跨进程锁、可重建缓存与用户内容混放在 `config/`、`state/` 与顶层散目录中，"哪些要备份、哪些敏感、哪些可清理"无法从路径判断，新数据持续堆进 config/state。
- 决策：数据根顶层按用途分为 `config/`（声明式设置）、`data/`（持久业务数据与用户内容）、`secrets/`（凭据与含 token 存储，目录 0700）、`cache/`（可重建派生数据）、`state/`（跨重启运行状态）、`runtime/`（跨进程锁与协作文件）、`logs/`。落位判定顺序与完整旧→新映射以 [storage-layout.md](./storage-layout.md) 为布局 SSOT。Phase 1 落地：Usage 订阅存储 → `secrets/accounts/usage/`，CLI 凭据快照 → `secrets/accounts/cli/`，GitHub/SSH 凭据 → `secrets/{github,ssh}/`，市场 SQLite 与会话索引 → `cache/`，巡检状态 → `state/patrol/`，全部锁 → `runtime/locks/`，多开实例与本地创作 Skill → `data/`。迁移只在启动时幂等执行（目标已存在则不动源，失败 warn 不阻塞）；本地创作 Skill 迁移同步重建 Agent 链接；`legacy_cleanup` 收窄为只删指向旧 hub `skills/repos/content` 的链接。`SKILLSTAR_DATA_DIR`/`SKILLSTAR_HUB_DIR`/`SKILLSTAR_TOOL_SYNC_HOME` 语义不变，跨根迁移在任一覆盖激活时整体跳过。Phase 2（config 细分、OAuth client secret 入 secrets）与 Phase 3（state 业务记录归 data）记录在 storage-layout.md，未实施。
- 后果：获得——路径即语义（备份/清理/敏感度可从目录判定）、锁与缓存不再污染 config/state、凭据集中限权。承担——老用户首次启动发生一次性目录迁移；外部脚本硬编码旧路径需更新（errors.md 的 sqlite3 自检命令已同步）；`SKILLSTAR_HUB_DIR` 沙箱下本地创作 Skill 仍落 `<hub>/local`，与生产布局存在受控差异。
- 证据：`crates/ss-core/src/infra/{paths,migration}.rs`、`crates/ss-skills/src/storage_migration.rs`、[storage-layout.md](./storage-layout.md)。

## D-088：GitHub 加速源改为用户排序回退

- 日期：2026-10-05
- 状态：accepted
- 背景：D-050 的候选链按健康延迟自动排序，用户选择的首选项会被历史延迟覆盖，且无法表达"先走 A 再回退 B"的意图。
- 决策：`GitHubMirrorConfig` 新增 `order`（preset id + `custom` 条目的用户排序）。Settings 列表可直接上下移动；列表首位即选中源（`preset_id` 由 `order[0]` 归一化派生，旧配置无 `order` 时按原"选中优先"合成）。`candidate_mirror_urls()` 严格按 `order` 回退；熔断仍跳过开路源、全部开路仍 fail-open、保存配置仍重置健康，但不再按延迟重排。
- 后果：获得——列表顺序即回退顺序，符合直觉且可控；承担——健康记录不再影响排序（仅熔断），次快源需要用户手动排序。
- 证据：`crates/ss-core/src/config/{github_mirror,github_health}.rs`、`src/features/settings/{mirrorOrder.ts,sections/GitHubMirrorSection.tsx}`。

## D-089：GPUI 替代 Tauri 的可行性 spike 与阶段判断

- 日期：2026-10-05
- 状态：superseded by [D-091](#d-091退役-tauri-与-react全面转向-gpui)
- 背景：目标「gpui-kit 替代 tauri」。当时 Tauri 承担 WebView 壳、IPC、托盘、dock 菜单、deep-link、updater 和 CLI/GUI 分发。
- 决策：以 `crates/ss-gpui` 验证域 crates 可以被 GPUI 直接调用，不走 IPC。是否替换不在本记录范围。
- 后果：桥接方式（`spawn_domain`）被生产壳沿用。托盘、updater、deep-link 没有随 spike 一起落地。
- 证据：`crates/ss-gpui/`。

## D-090：技能扫描与添加按需获取内容

- 日期：2026-10-06
- 状态：accepted
- 背景：普通扫描与添加各自完整检出浅克隆，子目录来源也下载无关文件；大型混合仓库的传输与磁盘开销远大于技能自身。
- 决策：保留 D-081 安装/锁语义和临时目录生命周期，把通用扫描与安装的获取范围交给 `ss-skills::fetch`；复用 Git partial clone 和非 cone sparse-checkout，避免 cone 模式附带下载根目录大文件。用户行为契约以 [Skills 安装与更新](./features/skills/README.md#安装与更新) 为准。
- 后果：不新增持久缓存或另一套 HTTP 下载器；承担远端必须支持 blob 过滤才能节省网络传输的限制。完整内容消费者仍用既有完整 checkout 接缝，避免频道完整性校验把稀疏内容误当作完整快照。
- 证据：`crates/ss-skills/src/fetch.rs`、`crates/ss-git/src/ops.rs`、`crates/ss-skills/src/git_skill.rs`。

## D-091：退役 Tauri 与 React，全面转向 GPUI

- 日期：2026-10-06
- 状态：accepted
- 背景：D-089 只证明 GPUI 能直接调用域 crates。继续保留 Tauri 会让两个壳、两套构建和两套发布同时存在。用户要求退役 Tauri，而不是等 GPUI 功能对齐后再切换。
- 决策：删除 `src-tauri/`、React SPA 和前端工具链。产品二进制是仓库根 package `skillstar`，入口 `src/main.rs`。无参数、`gui` 和 `gui-gpui` 都启动 GPUI。已知 CLI 子命令仍走 `ss-app`。MCP serve 仍最先返回，且不初始化市场快照。进程启动（路径迁移、legacy cleanup、市场快照接线）和 GUI 存活期间的频道自动更新唤醒放在 `ss-app`。
- 后果：获得——单一壳、单一 lockfile、GUI 直接调用域 facade。承担——托盘、dock 菜单、签名应用内更新、深链注册、后台巡检循环、SSH 远端界面、共享频道管理界面、用量图表和命令面板没有移植。这些能力的域实现仍在；缺的是 GPUI 界面和进程级插件。发布只上传 `skillstar` 二进制，不再产出 `.dmg`、`.deb`、`.msi` 或 `latest.json`。Windows release 仍使用 `windows` subsystem，不调用 `AllocConsole`。
- 证据：`src/main.rs`、`crates/ss-app/src/bootstrap.rs`、`crates/ss-app/src/channel_wake.rs`、`crates/ss-gpui/src/lib.rs`、`.github/workflows/release.yml`。

## D-092：导入复用本地 Git 缓存并锁定预览提交

- 日期：2026-10-06
- 状态：accepted
- 背景：D-090 的临时目录在扫描后删除，安装和重扫仍重复 clone。
- 决策：普通导入改用可清理的 Git 对象缓存，修订 D-081/D-090 的导入临时目录生命周期；保留 canonical 复制与锁语义。复用 Git 按需对象获取，不建立另一份清单数据库。行为以 [Skills 安装与更新](./features/skills/README.md#安装与更新) 为准。
- 后果：已缓存范围可以离线重扫、已物化内容可以离线重装；用户通过显式刷新选择上游新版本。缓存清理后预览失效，需要重扫。完整频道快照与更新获取不受导入缓存影响。
- 证据：`crates/ss-skills/src/fetch/cache.rs`、`crates/ss-skills/src/git_skill.rs`。

## D-093：通用技能自动更新是显式开关，偏好归 core config、唤醒归 ss-app

- 日期：2026-10-06
- 状态：accepted
- 背景：D-081 之后通用技能更新只有用户显式触发；后台只有频道到期自动升级（`channel_wake`）。用户要求 Settings 提供「自动更新技能 / 手动更新技能」切换，开启后由系统在后台自动监测。
- 决策：新增 `config/skill_updates.json` 的 `auto_update` 作为唯一偏好，默认手动。同一文件的 `interval_minutes` 是自动检查间隔，只能取 15、30、60、360、1440，缺省或档位外按 60（1 小时）。Settings「更新模式」开关打开是手动、关闭是自动，自动时可选该间隔。GUI 存活期间由 `ss-app::skill_wake` 周期唤醒、按这份间隔调用 `ss-skills::update::auto_update_locked_skills`，它复用与手动完全相同的检查（`installed_skill::refresh_skill_updates_in_session`）与应用（`GitSkillFacade::update_skills`）路径。只记录上次运行时间到 `state/skill_auto_update.json`，不建立第二套更新状态或调度器。
- 后果：获得——默认行为不变（覆盖式更新不会在用户不知情时发生），把更新模式拨到自动后徽标、锁、部署与缓存一起自动前进，且偏好对 GUI/后台/CLI 都是同一份文件。承担——进程退出即停止监测，不承诺常驻后台；共享频道托管的技能仍只按频道自身的自动升级设置处理。「自动更新不检测本地修改」已由 [D-095](#d-095更新检测按-tree-逐层解析安装基线保护自动更新频道升级由类型化授权与本地备份承担) 修订：内容与安装基线不一致或基线缺失时跳过。
- 证据：`crates/ss-core/src/config/skill_updates.rs`、`crates/ss-skills/src/update.rs`、`crates/ss-app/src/skill_wake.rs`、`crates/ss-gpui/src/settings/skill_updates.rs`。

## D-094：技能物化统一为校验、暂存、交换原语，部署所有权由链接目标或部署标记证明

- 日期：2026-10-06
- 状态：accepted
- 背景：对抗审查发现安装、本地采用和复制部署各有一套复制函数，都会跟随逃出来源的符号链接；采用不校验 frontmatter 名字；部署侧用「是链接或含 SKILL.md 的目录」推断所有权，会删掉用户自己的同名文件夹，也会对 `~/.agents/skills` 本身的 Agent 执行取消链接，从而删除已安装技能；锁版本不符被静默重置为空锁。
- 决策：修订 D-081 的「覆盖式安装无暂存原子性」与 D-024 的所有权推导。`ss-skills::materialize` 是唯一物化原语：`canonical_skill_name` 换算并校验名字 → `copy_confined` 只解引用仍在边界内的链接 → 在目标旁的隐藏 `.skillstar-*` 目录暂存 → 交换（旧目录先移到备份）→ 最后写锁，全程持跨进程技能事务锁，失败自动还原。复制用全局已访问集合挡住链接放大（真实目录即使先被链接点到也照常复制），并有文件数、总字节和深度上限，超限 fail-closed；解析后路径的任一组件命中排除名就跳过。交换与回滚的 rename 有界重试。最外层取得事务锁时按间隔清扫自残留创建时刻起超过 `STALE_TRANSIENT_AGE` 的暂存（不看 rename 留下的目录 mtime）。目标已缺失的 backup、remove、retain 不删，留给医生还原；目标还在的 stage，以及目标还在且替换已提交的 backup / retain，仍可清掉。同线程重入不重复清扫。`deployment::ownership::owned_deployment` 是 Agent 路径的所有权判定：链接必须正好是 `<canonical|local|旧 hub/skills>/<技能名>` 且为目录（断链仍算待清理的链接；子目录或 hub 根下其他位置算外来）；复制须带 `.skillstar-deploy.json`，且 `contentHash` 与当前内容一致，空或不等算外来。无标记目录只有在不含 `.git` / `.skillstar` / 标记文件、且文件集合与 canonical 完全相同（长度前缀的内容 hash）时才算旧复制。项目路径用 `owned_project_deployment`：无标记只报告、不删除。全局目录等于或位于 canonical 根内的 Agent 由规范根直接提供；加载已保存的自定义 Agent 时重跑同一校验，不合规的跳过并报告，不从偏好里删。安装锁写侧对过新或损坏的文件先备份再拒绝写入；旧 schema 在写入时备份后从空锁重写，读侧对过新、损坏和过旧都展示为空锁且不改文件。`ref` 同时接受 `gitRef`，条目按目录名归并。删除已安装技能先列出将卸载的名字；锁过新或损坏时整次中止，缺失和旧 schema 不按锁里的名字卸载。发布只在本地预检时短时持锁，网络在锁外。`ss-core` 删除旧的链接/复制部署助手，不依赖 `ss-skills`。行为以 [Skills 生命周期](./features/skills/README.md#生命周期) 为准。
- 后果：获得——符号链接逃逸、名字穿越、误删用户目录和锁被清空都在原语层被挡住，新入口只要用原语就自动安全。带 `.git` 的同内容工作区、内容 hash 对不上的标记副本、以及项目里没有标记的目录都不会被整目录删除。承担——没有标记的旧复制部署只有文件集合与 canonical 完全相同且不含排除名才继续被识别，内容已过期的旧副本不再自动刷新或删除，需要用户重新部署；Agent 目录里内容恰好相同、且不含 `.git` / `.skillstar` 的手工副本仍会被当作旧部署删除；旧格式的标记（没有 `contentHash` 或 hash 已过期）变为外来并保留；指向子目录的链接不再算 SkillStar 的；上游改名不再自动跟随，需要用户显式安装新名字。
- 证据：`crates/ss-skills/src/materialize.rs`、`crates/ss-skills/src/deployment/ownership.rs`、`crates/ss-skills/src/skill_lock.rs`、`crates/ss-skills/src/deployment/production_layout_tests.rs`。

## D-095：更新检测按 tree 逐层解析，安装基线保护自动更新，频道升级由类型化授权与本地备份承担

- 日期：2026-10-07
- 状态：accepted
- 背景：审查发现通用更新检测与频道升级有一串正确性问题：Trees API 快速路径把 commit SHA 当根 tree SHA，根技能永远显示「可更新」；含 `/` 的嵌套 `skillPath` 一律被当作上游已移除；限流后每次检查都重新撞 API；批量检查的慢结果会盖掉刚完成的更新，「上游已移除」不落盘；自动更新会覆盖本地修改；频道安装靠绕过通用 gate 写入，回滚需要重新拉取上一个 Release，离线时无法补偿。
- 决策：修订 D-081 的检测实现与 D-093 的「自动更新不检测本地修改」。(1) `ss-skills::update_check` 负责检测：ref 先解析为 commit 再取 `tree.sha`，嵌套路径逐层读取子树，只有读到父 tree 且确实缺少目录才算移除；限流截止时间持久化为冷却文件，期间走同一 Git session 的克隆回退；分组检查有固定并发上限；空 ref 比较 `HEAD`。(2) `update_state` 用 stamp/commit_scan 防止旧检查覆盖新结果，Removed 与 IdentityChanged 落盘。(3) `install_baseline` 在每次安装/更新成功后记录 canonical 内容 hash；自动模式在内容不一致或基线缺失时跳过，手动更新仍按 D-081 覆盖。(4) 频道写入携带只能由频道模块构造的 `ChannelInstallAuthority`，代替绕过通用 gate；它不能覆盖另一个频道的 Skill。(5) 频道升级在拉取与校验成功后、替换前保留当前 canonical 副本，失败或补偿回滚时原样换回，升级结果持久化后才删除副本。
- 后果：获得——根技能与深路径不再误报，限流不会放大成每次失败，自动更新不会静默丢弃用户编辑，频道回滚不依赖网络，频道写权限在类型上可追踪。承担——基线机制之前安装的技能在用户手动更新一次之前不参与自动更新；进程在频道升级持久化前退出会留下隐藏的 `.skillstar-retain-*` 目录，订阅尚未改到新内容之前清扫和医生都不删除它；基线文件是 `data/` 下新增的一份业务事实。
- 证据：`crates/ss-skills/src/update_check.rs`、`crates/ss-skills/src/update_check_tests.rs`、`crates/ss-skills/src/install_baseline.rs`、`crates/ss-skills/src/update_tests.rs`、`crates/ss-skills/src/channels/shared_channels/install_authority.rs`、`crates/ss-skills/src/channels/shared_channels/channel_update_installer.rs`、`crates/ss-skills/src/channels/shared_channels/channel_install_authority_tests.rs`。

## D-096：存储健康检查只修复能证明所有权的条目

- 日期：2026-10-07
- 状态：accepted
- 背景：安装、部署和迁移会留下断链、暂存目录、过期副本和旧 hub。把「同名目录」或「外部 Agent 目录里的技能」一律迁入 `~/.skillstar` 再换成链接，会拿走用户自己的 `~/.claude/skills`。锁过新或损坏时也不该为了修复去改写它。
- 决策：`ss-skills::health` 是存储医生。`scan` 只读，报告缺锁、缺目录、缺 `SKILL.md`、断链、外来链接、自指链接、无标记同名目录、过期复制部署、镜像漂移、暂存残留、迁移冲突，以及锁过新、损坏或过旧。`plan` 不删除没有所有权证明的内容。doctor 不把旧 schema 的锁重写成空锁。`apply` 持技能事务锁，每步重查前置条件，因此可重复执行；`dry_run` 走同一组检查但不写盘。直接读规范根的 Agent（Pi、Cline）不作为部署目录扫描。`skillstar doctor [--json]` 只报告，`skillstar doctor --fix [--dry-run]` 执行或预览计划。设置存储页显示问题数和可修复数，并提供预览与执行。`local_skill::repair_installations` 保留为显式的重复副本收纳，存储页和 `doctor` 不调用它。导入缓存的「未使用」只统计锁里没有对应来源的缓存目录；锁无法安全读取时该计数为 0。
- 后果：获得——用户自己的同名文件夹、外来链接和已编辑的副本不会被修复删掉或搬走；修复可以先预览。承担——没有标记、内容又与 canonical 不同的旧复制部署只报告，需要用户重新部署。`repair_installations` 的搬家行为由 [D-097](#d-097本机-agent-已装技能经可预览的纳管计划进入-skillstar) 取代：它仍是显式入口，但只执行可预览的纳管计划。
- 证据：`crates/ss-skills/src/health/`、`crates/ss-app/src/cli/doctor.rs`、`crates/ss-gpui/src/settings/skill_repair.rs`、`crates/ss-skills/src/local_skill/repair.rs`。

## D-097：本机 Agent 已装技能经可预览的纳管计划进入 SkillStar

- 日期：2026-10-07
- 状态：accepted
- 背景：Claude、Codex、Gemini 等 Agent 会自己把技能装进各自的全局目录。D-096 禁止医生和设置修复静默把这些目录搬进 `~/.skillstar` 再换成绝对链接，因为所有权方向反了，也会搬走带 `.git` 的工作区。用户仍需要一条安全的办法，把已经装好的技能纳入 SkillStar。
- 决策：`ss-skills::local_skill::intake` 提供与存储医生同形的 `scan` / `plan` / `apply`。扫描只读，覆盖每个有全局技能目录的 Agent（含未启用），跳过直接读规范根的 Agent。分类为：已受管则跳过；与规范副本内容一致且没有排除项则换成相对链接、不搬文件；规范根没有这个名字则用 `materialize` 的受限复制放进 local，锁记为本地采用（`local/<agent>`），规范根到 local 的链接是绝对路径，再把 Agent 目录换成指向规范副本的相对链接；同名内容不同、带 `.git` 或其他排除项、外来链接、名字已被占用，只报告。`apply` 持技能事务锁，幂等，失败只还原刚改的那一步。提交前再比一次备份和已验证内容；多出来或字节不同的文件则还原并返回冲突，不删除这份备份，因此不删除用户目录里比已验证副本多出的文件。`skillstar doctor` 报告这些项；`doctor --fix` 不执行纳管。`doctor --adopt` 默认预览，`--adopt --apply` 才写盘。设置存储页在已有健康行列出可纳管项，预览和执行与修复按钮分开。`repair_installations` 改为调用同一计划，不再用绝对链接搬走用户目录。
- 后果：获得——本机已装技能可以先预览再纳管，原文件留在规范副本里，冲突和 Git 工作区保持不动，第二次执行是空操作。承担——内容比规范副本多出文件（含隐藏文件）的同名目录只报告、不改成链接；锁过新、损坏或过旧时跳过收养而不是改写锁；未启用的 Agent 目录也会出现在报告里。
- 证据：`crates/ss-skills/src/local_skill/intake.rs`、`crates/ss-skills/src/local_skill/repair.rs`、`crates/ss-app/src/cli/doctor.rs`、`crates/ss-app/src/agent_intake.rs`、`crates/ss-gpui/src/settings/skill_repair.rs`。

## D-098：技能生命周期不变量

- 日期：2026-10-07
- 状态：accepted
- 背景：D-094 到 D-097 各自挡住一类事故：符号链接逃逸与误删、更新误报与覆盖本地修改、没有所有权证明的修复、静默搬走 Agent 目录。分开读会把它们当成四次互不相关的补丁。
- 决策：它们共同构成一条不变量。一份技能只有一个规范副本。写入经 `materialize` 的校验、受限复制和暂存交换。部署、修复和删除只动能证明属于 SkillStar 的条目。自动更新不覆盖无法证明仍等于安装内容的副本。本机 Agent 已有技能只有经过可预览的纳管计划才进入规范副本。安装、更新、频道升级和发布的网络在跨进程事务锁之外；修复计划按锁重装时，拉取发生在该计划已经持有的同一把锁里。行为以 [Skills 生命周期](./features/skills/README.md#生命周期) 为准。D-094 至 D-097 仍是各自选择的记录，本条不重复它们。
- 后果：获得——新入口只要走这些原语和计划，就继承同一组不变量。承担——不另开一套安装、更新或修复路径。与这些选择冲突的便利（静默重置锁、把同名目录当成部署、在安装和升级的锁内拉网络）不再接受。
- 证据：[D-094](#d-094技能物化统一为校验暂存交换原语部署所有权由链接目标或部署标记证明)、[D-095](#d-095更新检测按-tree-逐层解析安装基线保护自动更新频道升级由类型化授权与本地备份承担)、[D-096](#d-096存储健康检查只修复能证明所有权的条目)、[D-097](#d-097本机-agent-已装技能经可预览的纳管计划进入-skillstar)。

## D-099：SKILL.md 解析收回 ss-skills 私有模块

- 日期：2026-10-07
- 状态：accepted
- 背景：`skill-spec` 只有 `ss-skills` 一个直接消费者，没有独立发布需求；现有解析与诊断可以在模块内测试，独立编译单元的收益不足以支撑额外边界。
- 决策：取代 D-055 的 frontmatter 拆分及 D-086 中保留该叶子的选择。解析实现与原有测试迁入 `ss-skills::validation` 下的私有 `frontmatter` 模块，公开类型、检查函数与安装门禁路径保持不变；删除独立 crate 和对应依赖边。未来只有实际复用或独立演进收益满足 D-002 时才重新拆分。
- 后果：减少一个 workspace member，解析与安装策略归同一技能域；解析测试随 `ss-skills` 编译，校验行为不变。
- 证据：`crates/ss-skills/src/validation.rs`、`crates/ss-skills/src/validation/frontmatter.rs`、[boundaries.md](./boundaries.md)。

## D-100：已安装技能的规范副本放在 SkillStar 数据根

- 日期：2026-10-07
- 状态：accepted
- 背景：D-081 把规范副本放在 `~/.agents/skills`，以便和 `npx skills` 共用目录。直接读这个目录的 Agent（Pi、Cline、Zed、Warp 等）因此会在安装完成时就看到技能，用户无法把「装进 SkillStar」和「装给某个 Agent」分开。
- 决策：规范副本改为 `~/.skillstar/data/skills/installed/<name>`，锁改为 `~/.skillstar/data/skills/.skill-lock.json`。安装、导入、本地创作和频道安装都只写这里。`~/.agents/skills` 恢复为普通 Agent 目录：只有用户把技能部署到读取它的 Agent 时才在那里建链接。启动时把旧目录里的条目和旧锁迁到新位置（新位置已有同名条目或锁则保留新的），并改写仍指向旧目录的 Agent 链接。`SKILLSTAR_HUB_DIR` 测试沙箱仍把规范根放在 `<hub>/skills`。项目内的 `.agents/skills` 不变。
- 后果：获得——新技能不再因为落在共享目录里而自动出现在所有读取该目录的 Agent 上。承担——不再与 `npx skills` 共用同一份正文和锁；那些 Agent 要再次看到已迁走的技能，需要用户显式链接。
- 证据：`ss_core::infra::paths::agents_skills_root`、`skill_lock_path`、`ss_skills::storage_migration::migrate_installed_skills`。

## D-101：gpui-component 以 vendored patch 引入，悬浮窗入场统一为原地弹出

- 日期：2026-10-07
- 状态：accepted
- 背景：所有悬浮窗（确认框、导入框、SKILL.md 悬浮窗、账号登录等）的入场动画写死在 `gpui-component` 的 `Dialog::render_once` 里：顶边从窗口顶部一路插值到 `margin_top`（约半个窗口高的滑入），且 crate 没有公开开关。产品要求悬浮窗在落点原地弹出（pop in），与 popover 的入场语言一致，而不是从上面飞入。GPUI（gpui-pre 0.3.8）没有子树 transform，无法做真正的缩放弹出，只能以「淡入 + 从落点上方 8px 落定 + 阴影渐显」近似。
- 决策：把 `gpui-component` 0.7.1 完整 vendor 到 `vendor/gpui-component/`，根 `Cargo.toml` 以 `[patch.crates-io]` 指向它；vendor 清单加空 `[workspace]` 隔离，不入 workspace members。vendor 树保持与发布版字节接近，唯一改动是 `dialog.rs` 的入场插值：`y * delta`（顶部 → 落点）改为 `y + POP_ENTER_OFFSET * (1 - delta)`（落点上方 8px → 落点，`POP_ENTER_OFFSET = -8px`，与 `popover::DROPDOWN_ENTER_OFFSET` 同值）。淡入与阴影动画保留。上游升级时重新 vendor 并重放这一处改动。
- 后果：获得——全部悬浮窗入场统一为原地弹出，滑入带来的半窗口位移掉帧面也随之消失；改动不经过 fork 发版即可生效。承担——仓库多约 3.9MB 的 vendor 树；上游发版频繁（一周内 0.6→0.7），升级要手动同步；vendor 的内联测试引用其 monorepo 布局里的 `themes/*.json`，发布包本身编不过，须以 `cargo test -p ss-gpui` 为验证面。
- 证据：`vendor/gpui-component/src/dialog/dialog.rs`（`POP_ENTER_OFFSET`、`pop-in`）、根 `Cargo.toml` 的 `[patch.crates-io]`、`ss-gpui` 的 `shell::dialog_motion` 测试。

## D-102：Claude Code 插件市场格式独立为协议叶子 crate

- 日期：2026-10-07
- 状态：accepted
- 背景：共享频道要把已发布技能导出为外部 harness 可消费的 Claude Code 插件市场目录（`marketplace.json` + 每插件 `plugin.json`）。该格式是 Claude Code 定义、Cursor/Devin/Factory Droid 等共同消费的事实标准，其演进由上游规范驱动，与 SkillStar 的发布列车不同步。若把 schema 与目录写出放进 `ss-skills`，外部格式变更会持续扰动产品 crate 的编译单元；而未来「发布时注入频道仓库」若复用同一 schema，也不能反向依赖 `ss-skills`。
- 决策：新建产品无关叶子 `crates/claude-marketplace`，只拥有外部格式：`marketplace.json` / `plugin.json` 的类型（解析宽松、生成只覆盖自包含相对路径形态）、命名与路径校验、以及自包含 marketplace 目录写出。不得依赖任何 `ss-*` crate，不得引入业务 HTTP/DB/打包依赖；频道注册表读取、baseline 校验与技能内容物化留在 `ss-skills::channels` 的私有导出模块。与 [D-099](#d-099skillmd-解析收回-ss-skills-私有模块) 的关系：D-099 收回的是无独立演进收益的 SKILL.md 解析；本叶子的变更节奏由上游 Claude Code 规范驱动。若长期只有单一消费方且格式稳定，按 D-099 同一先例收回 `ss-skills`。
- 后果：获得——外部格式演进隔离在独立编译单元，导出与未来发布注入共用一份 schema 不成环。承担——多一个 workspace member 及其依赖边维护；schema 覆盖面刻意保守（只生成相对路径源），远程源形态只解析不生成。
- 证据：`crates/claude-marketplace/`、`ss_skills::channels::shared_channels` 的 marketplace 导出模块、`scripts/internal/check_workspace_deps.sh` 白名单。

## D-103：应用版本检查为 check-only，共享 GitHub API 冷却

- 日期：2026-10-08
- 状态：accepted
- 背景：发版依赖 GitHub Releases（`v*` tag → release.yml → 维护者人工发布 draft），而 D-091 退役了签名更新器且二进制不签名。用户只能手动发现新版本，但自动下载/替换未签名二进制不可接受。
- 决策：只做检测，不做安装。域逻辑放 `ss-core::infra::release_check` 私有 module：经匿名 GitHub 链路（`get_anonymous`，加速源优先、直连兜底）请求 `/releases/latest`，与产品版本做严格 `MAJOR.MINOR.PATCH` 三元组比较（剥一个 `v` 前缀，解析失败一律视为不新，杜绝坏 tag 误报升级）。产品版本 SSOT 仍是根 `Cargo.toml` 的 `[package] version`，由 `skillstar` 二进制以 `env!("CARGO_PKG_VERSION")` 传入 `ss_gpui::run` 与 `ss-app` 唤醒（其余 crate 共享占位 workspace 版本，不可直接用）。结果持久化 `state/app/release_check.json`；GUI 唤醒（`ss-app::release_check_wake`）每小时评估、24 小时至多检查一次，与技能更新检查共享同一份 GitHub API 限流冷却（`state/skills/github_api_cooldown.json`，抽为 `ss-core::infra::github_api_cooldown`，同 IP 共享 60 次/小时额度）；设置 → 关于 可手动检查并跳转 Releases 页，永不下载或替换二进制。
- 后果：获得——客户端最迟 24 小时发现新发布（draft 人工发布后才可见，与发版流程兼容）；限流状态跨消费方一致，不会两家一起撞 403。承担——比较器不识别 prerelease tag（`v1.0.0-rc.1` 视为不新），仅提示不安装意味着升级仍需手动步骤；`get_anonymous` 吞掉响应头，403 只能记保守 1 小时冷却而非精确 reset。
- 证据：`crates/ss-core/src/infra/release_check.rs`、`crates/ss-core/src/infra/github_api_cooldown.rs`、`crates/ss-app/src/release_check_wake.rs`、`crates/ss-gpui/src/settings/about.rs`、`docs/features/platform/README.md` 的「Updater 与发布」。

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

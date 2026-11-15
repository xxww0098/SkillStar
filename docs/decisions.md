# 架构决策记录

状态：active

这里只保留长期有效、持续约束当前代码与目录边界的架构决策。当前目录与依赖见 [boundaries.md](./boundaries.md)，运行拓扑见 [architecture.md](./architecture.md)，具体行为以各功能文档和代码为准。

## D-001：GUI 与 CLI 共享一个二进制和域实现

- 日期：2026-07-10（持续生效）
- 状态：accepted
- 决策：唯一产品二进制由根包 `ss-bin` 产出，入口在 `src/main.rs`。启动按顺序分派 `ss_mcp::serve` → Git askpass 回调 → CLI 子命令（`ss-app`）→ 桌面 GUI（`ss_gui_host::run`）；GUI 与 CLI 调用同一套域 facade 与 `ss-app` use case，不新增第二个产品二进制。
- 后果：`ss-app` 保持 library-only，CLI 与 GUI 共享域不变量，只在展示与交互层各自适配。
- 证据：`Cargo.toml`、`src/main.rs`、`crates/ss-app/src/cli/`、`gui/host/src/lib.rs`。

## D-002：module-first，满足晋升条件后才拆 crate

- 日期：2026-07-10
- 状态：accepted
- 决策：新能力先放入最内聚的现有 crate，以私有 module + 窄 facade 暴露；只有在独立变更节奏、第三方依赖隔离或 deletion test 证明有显著收益时才新建或保留独立 crate（如 `ss-git` 隔离 `gix`、`ss-mcp` 隔离 `rmcp`/`schemars`、`ss-channels` 隔离频道远程发布）。
- 后果：UI 页面切片与 Rust crate 不要求一一对应，避免浅模块与编译扇出。
- 证据：[boundaries.md](./boundaries.md)、`scripts/internal/check_workspace_deps.sh`。

## D-003：表现层保持薄，跨域编排归 `ss-app`

- 日期：2026-07-10（随 D-110 / D-112 更新）
- 状态：accepted
- 决策：TSX 视图与 `gui/native`（`ss-gui-bridge`）只做展示与状态投影；单域逻辑归对应域 crate（`ss-skills` / `ss-accounts` / `ss-marketplace` / `ss-channels`），跨域事务与 CLI 编排下沉 `ss-app`。
- 后果：所有耗时与 IO 调用均走 `nsh` blocking 池，不在视图层直接写业务存储或网络请求。
- 证据：`gui/native/`、`crates/ss-app/`、[boundaries.md](./boundaries.md)。

## D-005：技能部署采用 link-first、copy fallback

- 日期：2026-06-10
- 状态：accepted
- 决策：Agent 部署按 symlink（Windows 视目标 Agent 限制允许 junction）→ 受管 copy 标记部署（`.skillstar-deploy.json`）的能力阶梯执行；对账、刷新与清理必须按实际部署形态分类识别。
- 后果：文档和界面不宣称「纯 symlink」；受管 copy 支持按内容指纹刷新与回流。
- 证据：`crates/ss-skills/src/deployment/`、[Skills 生命周期](./features/skills/README.md#生命周期)。

## D-006：文档按变化速率分层，并保持单一入口

- 日期：2026-07-14
- 状态：accepted
- 决策：`AGENTS.md` 是唯一 Agent 规则入口，`CLAUDE.md` 仅委托；项目树与依赖边界归 [boundaries.md](./boundaries.md)，运行拓扑归 [architecture.md](./architecture.md)，长期选择归本文件，可复发故障归 [errors.md](./errors.md)，功能行为归 `docs/features/`，术语归 `CONTEXT.md`。
- 后果：同一事实只在一个主文档维护，功能删减或结构重构必须在同一变更序列更新对应唯一文档与索引链接。

## D-007：Skill 安装采用 universal project surface 与 Agent path ownership

- 日期：2026-07-14
- 状态：accepted
- 决策：兼容 Agent 在项目内共用 `.agents/skills` surface，专属目录只保留给上游明确要求的 Agent；项目清单（`skills-list.json`）对共享路径选择单一 owner，部署、扫描与清理按规范路径去重。CLI `install` / `add` 对齐 `npx skills` 的来源与 scope 语义。
- 后果：多个 Agent profile 可解析到同一项目路径，所有 sync / scan / cleanup 实现均以路径去重为不变量。
- 证据：`crates/ss-skills/src/agents/`、`crates/ss-skills/src/projects/`、`crates/ss-app/src/cli/`。

## D-008：内置 Agent 以 vercel-labs 注册表为兼容基线，品牌图标统一投影

- 日期：2026-07-14
- 状态：accepted
- 决策：`ss-skills` 的内置 Agent 注册表以 `vercel-labs/skills` 注册表为兼容基线并辅以测试锁定；内置品牌 SVG 统一内嵌于 `gui/assets/agents/`（由 `ss-gui-bridge` 加载并按当前主题单色渲染），自定义 Agent 与未知 ID 回落通用图标；仅项目级 Agent 禁止进入全局安装与部署动作。
- 后果：新增或变更内置 Agent 时需同步更新注册表、兼容别名、SVG 图标与文档。
- 证据：`crates/ss-skills/src/agents/builtin.rs`、`gui/assets/agents/`、`gui/native/`、[Agents 文档](./features/agents/README.md)。

## D-009：本机 Agent 采用纯手动激活，不推断系统安装状态

- 日期：2026-07-14
- 状态：accepted
- 决策：不做本机 Agent 安装嗅探；所有内置 Agent 默认关闭，Settings 中的持久化配置（`profiles.toml`）是本机激活状态的唯一来源，CLI 隐式目标与各页面 Agent rail 仅消费该状态，`AgentProfile.installed` 仅作 `enabled` 镜像。
- 后果：共享 `~/.agents/skills` 目录或卸载残留不会触发误发现与误部署；真实部署或路径异常在动作边界显式报错。
- 证据：`crates/ss-skills/src/agents/`、[Agents 文档](./features/agents/README.md)。

## D-013：私有共享身份采用 GitHub App 设备流与应用私有文件存储

- 日期：2026-08-05
- 状态：accepted
- 决策：GitHub 登录支持 `github.com` Device Flow，获取的凭据保存至应用私有目录 `SKILLSTAR_DATA_DIR/secrets/github/` 下受限文件（Unix `0600`），不写入系统钥匙串，不接入全局 `gh` 登录态或用户粘贴 PAT，所有 HTTP 请求经 `ss_core::infra::http_client::probe_http_client`。
- 后果：应用启动不触发系统钥匙串弹窗；发布构建需配置启用 Device Flow 的 GitHub App client ID。
- 证据：`crates/ss-skills/src/github_auth/`、`crates/ss-core/src/infra/paths.rs`。

## D-014：私有 Git 认证采用操作级 askpass session

- 日期：2026-08-05
- 状态：accepted
- 决策：远程 Git 认证动作创建一次性 operation session，以当前 `skillstar` 可执行文件作为临时 `GIT_ASKPASS`，凭据仅通过子进程专用环境变量传递，不写入 remote URL、Git config、命令行参数、日志或前端状态；带凭据请求强制禁用第三方 GitHub 加速镜像，同时尊重 SkillStar 代理配置，并在 `src/main.rs` 最早阶段分发 askpass 回调。
- 后果：GUI、CLI 与后台任务共享同一套脱敏与认证边界；任何新增私有远端 Git 操作必须经过 session。
- 证据：`src/main.rs`、`crates/ss-git/`、`crates/ss-skills/src/github_auth/`。

## D-015：组织私有共享频道采用 repository ID 与可恢复两阶段绑定

- 日期：2026-08-05
- 状态：accepted
- 决策：组织私有共享频道以 GitHub 数字 `repository ID` 作为稳定身份，owner/name/URL 仅作可刷新路由元数据；创建/绑定分两阶段原子持久化可恢复描述；频道发布以 `channel-vNNNNNN` annotated tag manifest 与同名 Release 为不可变版本边界；订阅远端状态投影为五态（`active` / `revoked` / `offline` / `recoverable_failure` / `integrity_error`），每小时周期检查由 `ss-app::channel_wake` 触发，自动升级仅在显式开启时复用手动作业事务且保留回滚能力。
- 后果：仓库同组织改名不破坏频道身份；权限、成员与访问控制完全由 GitHub 持有，SkillStar 只保留本机消费选择、pin 与版本化基线。
- 证据：`crates/ss-channels/`、`crates/ss-skills/src/channel_registry/`、`crates/ss-app/src/channel_wake.rs`、[Team 与共享频道](./features/team/README.md)。

## D-019：Skill 安装/打包/采用采用 frontmatter 质量门禁

- 日期：2026-08-10（兼容规则由 D-040 / D-048 修订）
- 状态：accepted
- 决策：所有进入技能库的写入路径（仓库安装、目录导入/采用、频道安装等）共用 `ss-skills::validation` frontmatter 解析与阻塞门禁（缺失/损坏 frontmatter、缺少非空字符串 `description`、非法 `name` 均 fail-closed）；本地草稿保存不做阻塞，尖括号字符与超长 `description`（>1024）仅作咨询级兼容性警告而不阻断安装。
- 后果：确保新入库技能对兼容 Agent 可触发且具备完整描述，单个技能校验失败不阻断批量导入中其它有效技能。
- 证据：`crates/ss-skills/src/validation.rs`、[D-099](#d-099skillmd-解析收回-ss-skills-私有模块)。

## D-020：更新检测采用 GitHub API 快速路径，凭据不出 Git session

- 日期：2026-08-10（结合 D-113 新增 ETag 条件请求增强）
- 状态：accepted
- 决策：对 `github.com` 公开或已登录来源，批量更新检查优先通过限流与并发受控的 GitHub Trees API 读取远端 tree SHA 并配合本地 ETag 条件缓存比对，未变化或条件命中不拉包；遇到 403/429 限流写冷却状态并回落至 Git session 拉取检查。
- 后果：大幅降低常规更新巡检的 Git 网络开销；API 降级只影响检测速度而不损失正确性。
- 证据：`crates/ss-skills/src/update_check.rs`、`crates/ss-core/src/infra/github_api_cooldown.rs`。

## D-021：仓库发现对齐生态容器目录深度与 Claude Code 插件清单

- 日期：2026-08-10
- 状态：accepted
- 决策：仓库技能发现扫描标准容器目录（最高 3 层，浅层 `SKILL.md` 遮蔽子目录）并跳过测试/夹具噪音目录；同时读取 `.claude-plugin/marketplace.json` 与 `plugin.json`（只处理合规本地 `./` 相对路径声明，若含 `hooks/` 或 `agents/` 仅提示而不自动执行插件脚本）。
- 后果：兼容单技能仓、分级 catalog 仓与 Claude Code 插件市场仓的技能发现。
- 证据：`crates/ss-skills/src/discovery/`、`crates/ss-skills/src/plugin_manifest.rs`。

## D-023：拉取多源化与网络加固：镜像用户排序回退、SOCKS5H 与 ETag 绑 host

- 日期：2026-08-10（合并 D-050 与 D-088 修订）
- 状态：accepted
- 决策：
  1. GitHub 加速配置按用户显式排序列表（`order`）依次回退，健康跟踪记录连续失败并短暂熔断不可用镜像，全部开路则 fail-open 回声明顺序；仅作用于匿名公开流量，带凭据请求永不绕经加速镜像。
  2. SOCKS5 代理统一以 `socks5h` 在远端解析 DNS；匿名 GitHub 家族 URL（含启用加速时的 Marketplace 包装地址）均按候选链重试。
  3. Marketplace SQLite 快照写入采用内容寻址增量更新与质量防降级门禁，`If-None-Match`（ETag）严格绑定签发它的 `source_host`，避免跨镜像假 304。
- 后果：公开拉取链路在单镜像失效或 DNS 异常环境下具备自动回退与可恢复性；私密凭据与签发完整性边界不变。
- 证据：`crates/ss-core/src/config/{github_mirror,github_health,github_rewrite,proxy}.rs`、`crates/ss-marketplace/src/snapshot/`、[Marketplace 文档](./features/marketplace/README.md)。

## D-024：共享 skills 目录塌缩为部署目标，归属改由链接指向 hub 推导

- 日期：2026-08-12（全局副本位置后续按 [D-100](#d-100已安装技能的规范副本放在-skillstar-数据根) 与 [D-094](#d-094技能物化统一为校验暂存交换原语部署所有权由链接目标或部署标记证明) 演进）
- 状态：accepted（约束项目级共享路径及所有权零状态推导原则）
- 决策：物理磁盘上不存在 per-agent 的独占归属事实；多个 Agent 共用同一项目目录（如 `.agents/skills`）时按物理路径去重为同一部署单元；清理与卸载不得采用「看起来像技能就删」的启发式，只能依据链接目标归属、部署标记（`.skillstar-deploy.json`）或项目 manifest 执行。
- 后果：消除共用目录下的误删与多计数串台；目录内用户自有或外来条目得到结构性保护。
- 证据：`crates/ss-skills/src/deployment/ownership.rs`、`crates/ss-skills/src/projects/`。

## D-030：Deck 自持 Agent 链接，不从成员 Skill 推导

- 日期：2026-08-13
- 状态：accepted
- 决策：`SkillGroup`（卡组/Deck）显式保存用户认领的 `agent_links` 作为预期状态，而不是从当前成员 Skill 的已链接状态反推；当实际成员部署与卡组意图不一致时，以 mixed 状态显式暴露而非静默覆盖。
- 后果：卡组与单技能两级意图清晰分离，全局安装默认部署不会污染新建卡组状态。
- 证据：`crates/ss-skills/src/skill_group.rs`、`crates/ss-skills/src/workflows/`。

## D-031：技能发布链路并入单一 GitHub App 身份，`gh` CLI 退出远程路径

- 日期：2026-08-14
- 状态：accepted
- 决策：技能发布（用户信息、有写权限的个人/组织仓库列表、创建个人仓库与远程推送）全部使用 SkillStar GitHub App 凭据 + `probe_http_client` + askpass Git session，不再调用全局 `gh` CLI 进行远程操作；`gh` 仅保留本地环境探测用途。
- 后果：发布与扫描/更新/频道使用同一套账号身份、代理策略与脱敏边界。
- 证据：`crates/ss-skills/src/git/`、[Skills 文档](./features/skills/README.md)。

## D-032：dev 构建不加 `[profile.dev]` / `build-override` 优化清单，不搬迁 `target/`

- 日期：2026-08-14
- 状态：accepted
- 决策：根 `Cargo.toml` 仅保留 `[profile.release]`，不添加 `[profile.dev]`、`[profile.dev.build-override]` 或按包开启 `opt-level = 3` 的热路径清单，也不强制迁移外置 `target/`。
- 后果：避免 proc-macro 与 `libsqlite3-sys` / `aws-lc-sys` 在 `cargo check` / 冷构建中因优化编译导致构建耗时翻倍劣化。
- 证据：`Cargo.toml`。

## D-043：Agent 技能主开关持久化恢复意图，而不持久化 Agent 归属

- 日期：2026-08-30
- 状态：accepted
- 决策：Settings 中用于暂停/恢复全部已部署技能的主开关在 `profiles.toml` 按物理 Global 目录键记录待恢复的技能名集合（recovery-only journal），当前活动技能仍从磁盘真实重读，禁止将该恢复列表扩张为归属真相源。
- 后果：暂停与恢复跨重启保持精确范围，共享同一目录的 Agent 天然同步状态，源缺失或受保护条目保留待重试状态。
- 证据：`crates/ss-skills/src/agents/profile_storage.rs`、`crates/ss-skills/src/workflows/`。

## D-056：团队智能留在 ss-skills 私有 module

- 日期：2026-09-09
- 状态：accepted
- 决策：本机团队知识检索与摩擦记录作为 `ss-skills::team` 私有模块与窄 facade，语料读取 `ss_core::infra::paths::local_skills_dir()` 下的已安装 `SKILL.md` 及笔记文件，`state/` 下保留检索反馈，CLI 通过 `skillstar team …` 提供服务。
- 后果：语料与技能安装生命周期紧密同编译、共用沙箱契约，不为此单独拆分冗余 crate。
- 证据：`crates/ss-skills/src/team/`、`crates/ss-app/src/cli/team.rs`、[Team 文档](./features/team/README.md)。

## D-065：SkillStar 作为 MCP 服务时不进入外部 MCP catalog

- 日期：2026-09-22（由 D-112 拆入独立 crate `ss-mcp`）
- 状态：accepted
- 决策：SkillStar 向本机编码 Agent 暴露的「项目技能检索与启用」stdio MCP 服务独立位于 `crates/ss-mcp`，入口 `skillstar mcp serve --stdio` 在 askpass 与市场快照初始化之前优先返回，仅向 stdout 输出合法 JSON-RPC，不引入第三方外部 MCP 配置管理或商店快照。
- 后果：MCP 服务冷启动极快、`rmcp` / `schemars` 编译依赖与 GUI 及通用 CLI 解耦。
- 证据：`src/main.rs`、`crates/ss-mcp/`、[Project Skills MCP](./features/project-skills-mcp/README.md)。

## D-071：不做 Antigravity 语言服务唤醒网关

- 日期：2026-09-22
- 状态：accepted
- 决策：拒绝通过本地伪装 TLS 网关发送合成请求来主动消耗或重置账号配额窗口；账号管理与额度刷新严格只读官方或本机额度状态。
- 后果：规避账户风控与协议违规风险，保持 `ss-accounts` 轻量透明。

## D-072：密钥纯本地加密 JSON 存储为主，仅对官方硬要求目标定点豁免钥匙串

- 日期：2026-09-22（结合 D-083 与 [D-114](#d-114antigravity-的切号按-cockpit-复刻macos-需要写系统钥匙串) 定点豁免）
- 状态：accepted
- 决策：SkillStar 自身保存的账号库与凭据默认全部保存在 `secrets/` 目录下按权限限制的 AES-256-GCM 加密文件，不写入系统钥匙串。唯二的定点例外为官方客户端本身强制依赖系统钥匙串读凭据的场景：macOS 下 Claude Code 的 `Claude Code-credentials`（仅替换 `claudeAiOauth` 键、保留兄弟键）以及 Antigravity 的 `gemini/antigravity` 项（见 D-114），二者均经 `/usr/bin/security` 操作并在 `SKILLSTAR_TOOL_SYNC_HOME` 测试沙箱内严格禁止写入。
- 后果：最大限度减少系统钥匙串弹窗与凭据外溢，同时确保受支持的客户端真实切换生效。
- 证据：`crates/ss-accounts/src/switch/`、[Accounts 文档](./features/accounts/README.md)。

## D-081：技能安装、锁与更新整体同步 vercel-labs/skills，删除自研管线

- 日期：2026-10-04（规范根位置由 [D-100](#d-100已安装技能的规范副本放在-skillstar-数据根) 调整，部署与基线保护由 [D-094](#d-094技能物化统一为校验暂存交换原语部署所有权由链接目标或部署标记证明)/[D-095](#d-095更新检测按-tree-逐层解析安装基线保护自动更新频道升级由类型化授权与本地备份承担)/[D-113](#d-113skillstar-v4-目录树收敛统一技能管理树部署协调与版本保护升级) 增强）
- 状态：accepted
- 决策：技能获取、锁格式与覆盖更新对齐 `vercel-labs/skills`（`npx skills`）语义——按目录优先级去重、以 `.skill-lock.json`（v3）记录 `source/sourceType/sourceUrl/ref/skillPath/skillFolderHash/installedAt/updatedAt`、支持临时克隆或按需稀疏/缓存获取与覆盖重装；废弃早期自定义复杂重排/冲突跳过流水线。
- 后果：锁文件结构简洁且与 `npx skills` 格式互通，同名重装与来源更新语义清晰可预期。
- 证据：`crates/ss-skills/src/{skill_lock,fetch,installer,update}.rs`、[Skills 文档](./features/skills/README.md)。

## D-085：移除独立 Usage 顶层页，账号与额度统一归入 Accounts

- 日期：2026-10-05（随 D-105 / D-108 / D-109 收敛）
- 状态：accepted
- 决策：取消早期的独立消费度量与 Usage 顶层导航页面（以及冗余的本地历史会话费用重算链），将多账号切换与订阅/窗口额度展示统一收敛为 `Accounts` 页面与 `ss-accounts` 单一域。
- 后果：减少双页重叠与维护面，产品聚焦于「技能安装管理 + 多账号切号与配额状态」。
- 证据：`gui/src/pages/accounts/`、`crates/ss-accounts/`、[Accounts 文档](./features/accounts/README.md)。

## D-087：`~/.skillstar` 存储目录按用途分类（已随 D-113 升级至 v4）

- 日期：2026-10-05
- 状态：superseded by [D-113](#d-113skillstar-v4-目录树收敛统一技能管理树部署协调与版本保护升级)
- 决策：确立应用数据根按职责拆分为七个顶层目录的分类法则——`config/`、`data/`、`secrets/`（0700）、`cache/`、`state/`、`runtime/`、`logs/`，且当 `SKILLSTAR_DATA_DIR` / `SKILLSTAR_HUB_DIR` / `SKILLSTAR_TOOL_SYNC_HOME` 测试覆盖生效时保持沙箱隔离。
- 后果：通过路径即可区分敏感凭据、持久资产、可清理缓存与运行锁；完整结构以 [storage-layout.md](./storage-layout.md) 为唯一真相。

## D-090：技能扫描与添加按需获取内容

- 日期：2026-10-06
- 状态：accepted
- 决策：普通 Git 仓库扫描与添加保留 D-081 锁语义，同时使用 Git partial clone（blob filter）与非 cone `sparse-checkout` 按需检出选中的技能目录，避免在大型混合仓库（monorepo）下载无关资源；频道完整性校验仍使用完整 tree 视图以避免混淆。
- 后果：大幅缩短含大量资产仓库的首次扫描和安装等待时间。
- 证据：`crates/ss-skills/src/fetch/`、`crates/ss-git/src/ops.rs`。

## D-092：导入复用本地 Git 缓存并锁定预览提交

- 日期：2026-10-06
- 状态：accepted
- 决策：手动导入与扫描阶段将按需 Git 对象落入可清理缓存并锁定预览提交（commit），确认安装阶段复用已缓存对象直接物化与部署，无需重复网络克隆，用户也可显式刷新以获取上游新提交。
- 后果：预览与确认安装所见即所得，且缓存目录受存储清理控制。
- 证据：`crates/ss-skills/src/fetch/`、`crates/ss-skills/src/git_skill.rs`、[Skills 安装与更新](./features/skills/README.md#安装与更新)。

## D-093：通用技能自动更新是显式开关，偏好归 core config、唤醒归 ss-app

- 日期：2026-10-06（自动写入保护由 [D-095](#d-095更新检测按-tree-逐层解析安装基线保护自动更新频道升级由类型化授权与本地备份承担) 补全）
- 状态：accepted
- 决策：在 `config/skill_updates.json` 持久化通用技能的自动更新开关（默认关闭即手动更新）与检查间隔（15 / 30 / 60 / 360 / 1440 分钟，默认 60），GUI 存活期间由 `ss-app::skill_wake` 周期唤醒执行检查与受保护自动更新；频道技能仍遵循频道自身的自动升级策略。
- 后果：默认不会静默覆盖已装技能；GUI/CLI 共用统一配置文件，进程退出即停止轮询。
- 证据：`crates/ss-core/src/config/skill_updates.rs`、`crates/ss-skills/src/update.rs`、`crates/ss-app/src/skill_wake.rs`。

## D-094：技能物化统一为校验、暂存、交换原语，部署所有权由链接目标或部署标记证明

- 日期：2026-10-06
- 状态：accepted
- 决策：`ss-skills::materialize` 作为唯一写盘原语，固定执行「`canonical_skill_name` 校验 → `copy_confined` 防逃逸/防循环放大受限复制 → 同侧 `.skillstar-*` 暂存与有界备份交换 → 最后写锁」流程，全程持跨进程事务锁并按年龄清扫安全暂存。`deployment::ownership` 严格依据规范链指向或带有效内容指纹的 `.skillstar-deploy.json` 判定部署归属，无法证明所有权的项目/全局目录仅报告不删除。直接共享规范根或落入其内的目录由 `targets_canonical_root` 跳过，绝不对其取消链接。
- 后果：彻底封堵符号链接目录穿越、并发双写截断、误删用户手写同名技能目录以及锁损坏被静默覆写为空等风险。
- 证据：`crates/ss-skills/src/materialize.rs`、`crates/ss-skills/src/deployment/ownership.rs`、`crates/ss-skills/src/skill_lock.rs`。

## D-095：更新检测按 tree 逐层解析，安装基线保护自动更新，频道升级由类型化授权与本地备份承担

- 日期：2026-10-07
- 状态：accepted
- 决策：(1) `update_check` 先解析 ref 对应 commit 再读取 `tree.sha`，深层子路径逐层解析子树，仅确认父 tree 存在且目标缺失时判定移除；(2) `install_baseline`（及 D-113 `library.json`）在安装后记录基准内容指纹，自动更新遇到本地被修改或缺失基线时跳过而不覆盖；(3) 频道写入必须持有类型化的 `ChannelInstallAuthority`（无法伪造跨频道覆写），频道升级在替换前保留 `.skillstar-retain-*` 本地回滚备份，离线亦可即时还原。
- 后果：根技能与多层嵌套技能不再误报，自动更新不会丢失用户局部改动，频道回滚具备强类型与离线原子保障。
- 证据：`crates/ss-skills/src/update_check.rs`、`crates/ss-skills/src/install_baseline.rs`、`crates/ss-channels/`。

## D-096：存储健康检查只修复能证明所有权的条目

- 日期：2026-10-07
- 状态：accepted
- 决策：`ss-skills::health` 与 `skillstar doctor` 严格执行「只读扫描 → 预览计划 → 持锁幂等修复」，仅清理或修复确凿属于 SkillStar 的断链、暂存残留、镜像漂移与损坏指针；对用户自有同名目录、外来链接、过新或损坏的安装锁以及直接读规范根的 Agent 目录一律不动或只作诊断报告。
- 后果：存储修复可预测且支持 `--dry-run` 预览，不发生越权删除。
- 证据：`crates/ss-skills/src/health/`、`crates/ss-app/src/cli/doctor.rs`。

## D-097：本机 Agent 已装技能经可预览的纳管计划进入 SkillStar

- 日期：2026-10-07
- 状态：accepted
- 决策：`local_skill::intake` 与 `skillstar doctor --adopt [--apply]` 为本机已装但未受管的 Agent 技能提供独立的「扫描 → 预览 → 幂等纳管」流程：与规范根内容一致且干净的副本原地改链，全新无冲突目录通过受限复制并入规范根并建立相对链接，含 `.git` 工作区或同名内容冲突只报告跳过。
- 后果：不靠粗暴搬迁毁坏用户现有 Git 工作区，纳管意图清晰可控。
- 证据：`crates/ss-skills/src/local_skill/intake.rs`、`crates/ss-app/src/cli/doctor.rs`。

## D-098：技能生命周期不变量

- 日期：2026-10-07
- 状态：accepted
- 决策：整合 D-094 至 D-097 为系统级红线：每份技能只有一个规范副本；全部写盘必走 `materialize` 校验/暂存/交换；全部清理与删除只动可证明所有权的条目；自动更新不覆写与基线偏离的副本；未受管 Agent 技能只经显式可预览纳管计划入库；网络请求必须在跨进程事务锁之外进行。
- 后果：任何新增入口只要复用既有原语与计划接口即天然继承完整安全边界。
- 证据：[Skills 生命周期](./features/skills/README.md#生命周期)。

## D-099：SKILL.md 解析收回 ss-skills 私有模块

- 日期：2026-10-07
- 状态：accepted
- 决策：由于 `SKILL.md` frontmatter 解析在仓库内仅服务技能域安装与校验，将其收敛为 `ss-skills::validation` 下的私有 `frontmatter` 模块，避免无独立复用价值的零依赖小 crate 增加维护扇出。
- 后果：解析与门禁规则同编译同测，公开面保持紧凑。
- 证据：`crates/ss-skills/src/validation.rs`。

## D-100：已安装技能的规范副本放在 SkillStar 数据根

- 日期：2026-10-07（D-113 进一步消除了 `local/` 二级跳板）
- 状态：accepted
- 决策：SkillStar 管理的全部已安装技能规范副本统一存于 `~/.skillstar/data/skills/installed/<name>`，锁文件位于 `data/skills/.skill-lock.json`；`~/.agents/skills` 恢复为受显式部署开关控制的普通生态共享部署目录，而非装完即自动暴露给直接读共享目录的 Agent。
- 后果：明确区分「装入 SkillStar 技能库」与「激活到特定本机 Agent」两步动作；测试沙箱下可通过 `SKILLSTAR_HUB_DIR` 独立覆盖。
- 证据：`ss_core::infra::paths::agents_skills_root`、`crates/ss-skills/src/storage_migration.rs`。

## D-103：应用版本检查为 check-only，共享 GitHub API 冷却

- 日期：2026-10-08
- 状态：accepted
- 决策：应用发布版为未内置签名的桌面可执行程序，因此版本检查（`ss-core::infra::release_check` + `ss-app::release_check_wake`）严格为 check-only：匿名请求 `/releases/latest` 做 `MAJOR.MINOR.PATCH` 比较，每 24 小时评估一次并与技能更新检查共用 `state/skills/github_api_cooldown.json` 限流冷却；仅在「设置 → 关于」提示新版并引导打开 Releases 页面，永不自动下载或替换二进制。
- 后果：避免自动替换未签名二进制的安全风险，同时避免多处后台唤醒抢占匿名 GitHub 配额。
- 证据：`crates/ss-core/src/infra/{release_check,github_api_cooldown}.rs`、`crates/ss-app/src/release_check_wake.rs`。

## D-104：Claude Code 插件市场格式收回 ss-skills 私有模块

- 日期：2026-10-08（D-112 随频道远程逻辑归入 `ss-channels`）
- 状态：accepted
- 决策：Claude Code 插件市场导出格式仅由共享频道 `export-marketplace` 消费，不值得维护单独协议 crate；现保留为 `ss-channels` 下的私有 `plugin_market_format` 模块，不触网亦不反向读取频道注册表。
- 后果：格式演化与导出逻辑在同一编译单元内紧耦合验证。
- 证据：`crates/ss-channels/src/plugin_market_format/`。

## D-108：账号管理重建与以 cockpit-tools 为视觉参照的 nsh 页面设计

- 日期：2026-11-15（整合 D-108 / D-109 核心设计）
- 状态：accepted
- 决策：彻底淘汰历史多家族复杂模型（含独立消耗记账与遗留多开表），以极简索引 + 每账号独立 AES-256-GCM 加密文件重构账号存储，仅向界面暴露脱敏 `AccountView`；UI 在 `gui/src/pages/accounts/` 采用三段式高密度卡片与三态视图，严格遵循 `paletteToken()` 配色与零过渡状态切换，Agent 菜单由左侧边栏与注册表交集统一驱动。
- 后果：账号模型、OAuth、额度窗口刷新与切换适配器彻底脱离历史包袱。
- 证据：`crates/ss-accounts/src/store.rs`、`gui/src/pages/accounts/`、[Accounts 文档](./features/accounts/README.md)。

## D-110：nsh 成为唯一桌面应用框架并下线所有历史原型壳

- 日期：2026-11-15（结合 D-111 / D-116 最终形态）
- 状态：accepted
- 决策：全面统一为 `nsh`（当前锁定 `0.5.x`）桌面应用（TSX 信号层 + `ss-gui-bridge` + `ss-gui-host`），移除早期 WebView 方案、旧即时模式桌面壳以及与新运行时依赖冲突且缺少界面消费者的 SSH 远端同步模块；同时清理 `deny.toml` 冗余忽略项并将前端目录拍平至仓库顶层 `gui/`。
- 后果：全仓库统一为单锁文件、单产品二进制与单一套 GUI 页面实现，日常检查与编译链路收敛为 `nsh check/test gui` 与 `cargo check/test --workspace`。
- 证据：`Cargo.toml`、`gui/nsh.json`、`deny.toml`、`src/main.rs`。

## D-112：按能力域收敛 11 个 `ss-*` workspace 成员

- 日期：2026-11-15
- 状态：accepted
- 决策：将 workspace 收敛为职责正交、依赖单向无环的 11 个 `ss-*` 成员：将登录令牌、额度抓取与账号存储切号合入内聚的 `ss-accounts`；将组织私有频道远程编排拆入 `ss-channels` 并通过 `ss-skills::channel_registry` 保持变更守卫接缝；将本项目技能 stdio MCP 服务拆入 `ss-mcp` 以彻底隔离 `rmcp` 与 `schemars`；统一根二进制（`ss-bin`）、GUI 桥（`ss-gui-bridge`）与 GUI 宿主（`ss-gui-host`）命名并清理零调用死模块。
- 后果：每个边界均有明确所有权，并受 CI 依赖检查脚本强制校验。
- 证据：`Cargo.toml`、`scripts/internal/check_workspace_deps.sh`、`scripts/internal/check_dep_graph_doc.sh`、[boundaries.md](./boundaries.md)。

## D-113：~/.skillstar v4 目录树收敛、统一技能管理树、部署协调与版本保护升级

- 日期：2026-11-16
- 状态：accepted
- 决策：
  1. 完成 v4 目录树彻底归位：`groups.json`、`favorites.json`、`repo_history.json`、`projects/`、`channels/` 归入 `data/`；跨进程锁统一归 `runtime/locks/`；可重建缓存归 `cache/`；删除遗留旧仓库路径扫描并支持启动无损幂等迁移。
  2. 取消 `data/skills/local/` 软链双跳，所有技能实体统一收归 `data/skills/installed/<name>/`，辅以统一管理账本 `data/skills/library.json` 与滚动备份 `data/skills/backups/`（最多保留 25 批），同时保持 `.skill-lock.json` v3 与外部工具互通。
  3. 增加持久化全局与逐 Agent 部署模式偏好（`symlink | copy`）、`~/.agents/skills` 引用计数防冲突协调（避免双重加载兼容 Agent 重复写入）、以及带 `tree_max_mtime_ms` / `file_count` 的受管修改副本（`SkillStarModifiedCopy`）回流与备份覆写机制。
  4. 通过 `runtime_ignore` 隔离 `node_modules` / `.venv` / `__pycache__` 等运行期产物（更新时自动顺延迁入新版）；支持 CLI 命令串解析与同名异源自动重命名；为 GitHub 更新请求补齐 `cache/github-trees/` ETag 条件复用。
- 后果：存储语义严谨清晰、多 Agent 共用目录零互损、用户对已部署副本的现场微调支持无损回流，误覆盖可从备份回退。
- 证据：[storage-layout.md](./storage-layout.md)、[architecture.md](./architecture.md)、`crates/ss-core/src/infra/{paths,migration}.rs`、`crates/ss-skills/src/{library,backup,runtime_ignore,storage_migration}.rs`。

## D-114：Antigravity 的切号按 cockpit 复刻，macOS 需要写系统钥匙串

- 日期：2026-11-16
- 状态：accepted
- 决策：对 Antigravity 客户端凭据定点放宽 D-072 限制：macOS 非沙箱环境下写入 `gemini/antigravity` 钥匙串条目（值带 `go-keyring-base64:` 前缀，不加全局宽放 `-A`/`-T`），同步写入 `~/.gemini/jetski-standalone-oauth-token`、按需写入 `antigravity-oauth-token` 与本地 `state.vscdb`，任一环节失败按快照整体回滚；SkillStar 自有账号库仍仅存本地加密 JSON。
- 后果：macOS 切号后客户端状态真实同步且可回到 `LinkedTo`；同时须在设置与文档中明示该系统钥匙串副本在删除账号后不会由应用主动撤销。
- 证据：`crates/ss-accounts/src/switch/antigravity.rs`、[Accounts 文档](./features/accounts/README.md)。

## D-115：桌面宿主扩到 Windows，关闭窗口改为应用层隐藏（两平台一致的常驻）

- 日期：2026-11-16
- 状态：accepted
- 决策：`ss-gui-host` 同时支持 macOS（AppKit）与 Windows（Win32）；`gui/nsh.json` 声明 `closeOnPress: false`，由应用层 `gui/src/shell/residentWindow.ts` 在点关闭按钮时 `hide()` 主窗口实现两平台一致的后台常驻；macOS 通过 Dock 图标重新点击拉起，Windows 通过通知区托盘菜单与 `singleInstance: true` 拉起；顶部沉浸式隐藏标题栏按 `titleBar()` 动态安全区避让两侧红绿灯/Win32 caption 按钮（`gui/src/shell/caption.ts`）。
- 后果：macOS 与 Windows 共享统一壳交互、内嵌多尺寸图标及一致的常驻语义。
- 证据：`gui/host/src/lib.rs`、`gui/src/shell/{residentWindow,caption}.ts`、`gui/nsh.json`、`gui/App.test.tsx`、[Platform 文档](./features/platform/README.md)。

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

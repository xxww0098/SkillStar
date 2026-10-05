# Skills、Projects 与 Patrol

状态：active

本文件是技能安装、Agent 注册与手动启用、项目检测、部署、bundle、patrol 和相关 UI 行为的单一事实来源。新增内置 Agent 的操作步骤见 [../agents/README.md](../agents/README.md)。

## 所有权

- `skillstar-skills` 拥有技能 install/update/bundle/local/repo scan、安装锁/update detection、Agent registry、项目 manifest、deployment 和 patrol。无消费者的旧 terminal backend 不作为公共子系统保留。
- `skillstar-core` 只提供共享 `Skill` 契约与基础设施，不拥有技能安装锁/update detection。
- `src-tauri/src/commands/` 只转发 DTO、State 和事件；CLI 安装/管理复用相同 facade。
- 搜索结果安装等跨 marketplace/skills 流程由 `skillstar-app` 编排。
- 技能组部署的“补全 Marketplace 来源 → 安装缺失技能 → 同步 Project”由 `skillstar-app::skill_group_deploy` 编排，command 与单域 crate 都不复制该事务。
- `skillstar-skills::content` 是技能内容读取、文件枚举、本地创建/删除和嵌套内容目录解析的 facade；Tauri command 不直接组合 canonical 路径、安装锁与 cache invalidation。内容 facade 对外部输入先执行 `validate_skill_name`，再 canonicalize 有效目录并限制在 canonical skills 目录或本地创作目录内；外部符号链接不会被 read/list/open 跟随。只读快照（有效内容根、递归文件清单和确定性内容 hash）服务频道基线校验；通用安装/更新不再依赖内容基线（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线)）。
- `skillstar-skills::update` 拥有通用更新路径（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线)）：按锁的 source+ref 分组对比上游 tree SHA，变化即覆盖式重装；`update_skill(s)` 返回完整公开结果，command 不从底层 outcome 二次拼装 `Skill` DTO。
- `skillstar-skills::update_state` 是 `update_available` 的唯一所有者；只保存每技能 `update_available/checked_at` 投影，批量 refresh 和 update 完成都写穿它。
- 本地目录 adoption、share-code 安装和 deploy-status 检查同样由 Skills 域公开 use case 完成；command 只保留 blocking 调度与 `AppError` 适配。

## GitHub 身份与共享频道认证

- 第一版只连接 `github.com`，使用已注册 SkillStar GitHub App 的设备授权流，不要求用户粘贴 PAT，也不复用用户全局 `gh` 登录。
- GitHub App Client ID 解析顺序：进程环境变量 `SKILLSTAR_GITHUB_APP_CLIENT_ID` → 编译期嵌入 → 从当前工作目录或 crate 源码目录向上查找仓库根 `.env`。官方 Release 走编译期嵌入；本地 `tauri dev` 把该公开值写入 `.env` 即可，不必 `export`。缺失时登录动作明确不可用。
- `skillstar-skills::github_auth` 提供公开认证 facade；GitHub gateway、凭据仓库和时钟是可替换接缝。生产凭据写入 `SKILLSTAR_DATA_DIR/state/github_auth.json`：AES-256-GCM 加密 JSON（schema v2），首次创建和更新保持 Unix `0600`。密钥由本机 `machine_uid` 派生，**不访问系统钥匙串**，因此应用启动不会弹出钥匙串密码框。schema v1 明文文件在下次读取时就地改写成 v2。生产 gateway 的所有请求必须通过 `probe_http_client`。
- 设备授权的公开状态只包含用户码、GitHub 验证地址、轮询间隔和到期时间。device code、access token、refresh token 不得进入 IPC DTO、日志、错误、普通配置或 Git remote URL。
- 登录入口在侧边栏底部，打开同一设备授权面板。关闭面板**不取消**进行中的授权——用户要切到浏览器粘贴设备码；侧栏在等待期间显示「等待授权」。显式「取消」才清除进程内待处理设备授权。登出还会清除本地凭据文件和缓存身份。
- 界面必须说明凭据是本机加密 JSON，**不是**系统钥匙串；不要在登录文案里写「钥匙串 / credential store」。
- 登录说明以产品能力为主（私有技能、发布、共享频道），GitHub App 的 `Administration: write` / `Contents: write` 作为脚注。不请求 `Workflows: write`；有效操作权限仍受当前 GitHub 用户权限限制。
- 登录状态按 GitHub 返回的 `expires_in` / `refresh_token_expires_in` 元数据计算，不硬编码 token 寿命。显式刷新会轮换本地凭据文件并重新读取当前用户；过期且无法刷新的状态要求重新登录。
- GitHub App 由仓库所有者安装到明确选择的仓库。
- 已登录身份也是私有 `github.com` 仓库扫描、安装、更新检查、升级和**技能发布**的唯一 Git 认证来源；这些动作不依赖全局 `gh` 登录、Git credential helper 或预先改写过的 remote。发布不再调用 `gh` CLI：仓库列举、`skills/` 目录探查和建仓走 App 凭据的 GitHub REST（统一经 `probe_http_client`），clone/pull/push 走同一 operation session。`gh` 只剩 Settings 的环境检查一处用途。
- 每次远程 Git 操作创建独立 session。access token 只通过该子进程继承的临时 askpass 环境提供，操作结束即不可见；token 不得进入 remote URL、持久 Git config、命令参数、普通配置、IPC DTO、进度事件、错误或日志。所有 Git 子进程强制非交互，取消时终止当前子进程，进度只公开 session、阶段和无敏感信息的仓库标识。
- 私有认证只发送给规范化后的 `https://github.com/` 远端。带认证的操作不经过 GitHub 镜像，避免向第三方转发凭据；仍读取 SkillStar 当前代理设置并通过进程环境临时应用。公开仓库沿用无凭据路径，并同样不得弹出终端或系统凭据提示。
- 公开仓库的匿名拉取按 mirror 候选链执行：`candidate_mirror_urls()` 返回"custom → 选中 preset → 其余内置 preset（去重、规范化）"，transport/ops 对每个候选逐个尝试（每次独立 git 子进程），全部候选失败才回退直连 GitHub；非 GitHub/https 远端与带凭据操作不应用 mirror 重写。
- Git 失败按可行动状态区分：未登录、token 已过期、当前用户无仓库权限、GitHub App 未安装/无该仓库授权、网络/代理失败、用户取消。已安装 Skill 在认证或网络失败时保持不变，重试复用同一 Skills 域入口。

## 安装与更新

- Git/local 安装是 vercel-labs/skills 的同一条管线（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线)）：`Source::parse` 解析来源 → `fetch` 在 OS 临时目录做 `git clone --depth 1 [--branch ref]`（SHA-pin 走 init+fetch，LFS 禁用，`GIT_TERMINAL_PROMPT=0`，认证失败升级 `gh repo clone`→SSH）→ 发现 `SKILL.md` → 复制到 canonical `~/.agents/skills/<name>` → 各 Agent 目录用**相对** symlink 指向 canonical（Windows junction，失败回退 copy）→ 写入 vercel 锁。临时目录用完即删；没有持久仓库缓存，也没有 cache-local 安装——每次安装/更新都重新获取。GUI、CLI、轮播、batch、卡组补装都进同一个 facade。
- canonical 目录在 `SKILLSTAR_DATA_DIR` 设置时落到数据根下（开发/测试隔离），生产环境为真实 `~/.agents/skills`，与 `npx skills` 互通。
- 锁是 vercel 格式 `~/.agents/.skill-lock.json` v3（`$XDG_STATE_HOME` 优先）：每技能一条，记 `source/sourceType/sourceUrl/ref/skillPath/skillFolderHash(git tree SHA)/installedAt/updatedAt`；版本号不符静默重置为空锁。同名再装（无论来自哪个仓库）= **覆盖安装并改写 provenance**——不存在跨源拒绝或静默跳过；`pinned` 概念删除，tree URL 的 ref+subpath 天然由锁的 `ref`/`skillPath` 表达。
- 发现与去重对齐 vercel：仓库根有合法 `SKILL.md` 时普通模式只返回根技能；否则按优先目录顺序（`skills/` 及其 curated/experimental/system 子目录、各 Agent 容器目录、plugin manifest 声明目录）最多 3 层扫描，含 SKILL.md 的目录遮蔽其下内容；同名去重是**优先顺序先见者胜**（`skills/foo` 压过 `.claude/skills/foo`），不再有选副本排名表。全深度扫描为全递归（跳过构建产物/依赖/测试夹具目录），`--full-depth` 请求时根技能不再遮蔽嵌套内容。
- 安装与扫描必须走同一 frontmatter 质量判定（`skillstar-skills::validation`，对齐 vercel：`name` 与 `description` 必须是字符串）：缺失或非字符串、`name` 超 64 字符、frontmatter 缺失或 YAML 损坏 → 不可安装，扫描预览逐项显示原因、禁止选择阻塞项；`description` 超 1024 字符为咨询级警告仍可安装。`DiscoveredSkill.frontmatter_issues` 把稳定 issue code 传给前端，`installable` 投影同一判定；前端不得重建阻塞规则。
- 技能 id 使用 frontmatter `name`；目录名仅作展示回退（无 `name` 的技能不可安装）。
- 安装是覆盖式（rm -rf 后重建 canonical 目录），与 vercel 一致，无暂存原子性；复制排除 `.git`、`__pycache__`、`__pypackages__`、`metadata.json`，名称经 kebab-case 清洗并阻断路径穿越。
- 更新（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线)）：按锁的 `sourceUrl+ref` 分组；`github.com` 来源优先 GitHub Trees API 取整树对比 `skillPath` 的 tree SHA（`update_api`，带认证时用 Bearer），API 失败或非 GitHub 来源回退临时浅克隆后 `git rev-parse HEAD:<path>`；锁内 hash 与上游不一致即**覆盖式重装**（重跑安装管线），不检测、不保留本地修改。上游已不含该路径的技能列为「上游已移除」，出口是卸载或转为本地副本，没有自动删除。本地目录来源（local/bundle）不参与自动更新检查。
- 已安装列表先从本地快照返回（canonical 目录 + 锁 + 磁盘 Agent 链接归因），远程 update check 在有界后台任务中执行；`update_state` 只保存每技能 `update_available/checked_at` 投影。
- 安装全程向 `skillstar://git-progress` 发送阶段事件（`stage`：resolving/fetching/discovering/materializing/deploying，additive 字段，旧监听不受影响）；`install_skill` 接受可选 `sessionId`。
- GUI 的 `install_skill` / `install_from_scan` 新安装成功后默认部署到用户已启用的 Agent（`skillstar-app::global_deploy` 投影 Settings 启用集）；轮播点某个 harness 图标时把 `agentId` 传给 facade，仅部署到该 Agent。全局目录解析为 `~/.agents/skills` 本身的 Agent（共享目录组成员）不建链——canonical 已在其目录内；`batch_deploy_skills_to_agents` 继续按解析后的物理目录去重。`install_skill` 返回的 Skill 必须用 `installed_skill::agent_links_for` 重新读盘。没有 `git_url` 时回退 `onToggleAgent(..., true)`。一次安装只让该图标 pending，不得锁整行。
- repo scan 的 `ScanResult` 是来源规格本身；前端不重拼 URL。仓库声明 Claude 插件且带 `hooks`/`agents` 时 `ScanResult.plugin` 非空，ImportModal 与 CLI 打印同一条提示；SkillStar 只装 Skills。
- Codex 包内技能在 `.agents`（不是 `.codex/skills`）；Antigravity 在 `.agent`。发现按目录实际内容处理，不做 basename 特判。
- CLI `install` 与 `add` 是同一命令，来源解析兼容 `npx skills add` 的常用形式：`owner/repo`、`owner/repo/path`、`owner/repo@skill`、GitHub/GitLab tree URL、HTTPS/SSH Git URL、本地 `.ags`/`.agd` 和包含 `SKILL.md` 的本地目录。tree URL 的 ref 与 subpath 在克隆/扫描阶段生效。多技能来源在交互模式中选择；`-y` 未显式指定时装全部；`--skill '*'`/`--agent '*'` 展开全部；`--all` 等价两者加 `-y`；`--copy` 强制复制部署；`-g/--global` 全局（当前唯一 scope），`-s/--skill`、`-a/--agent` 过滤。
- 卡组/项目补装携带明确 Skill identity 时 fail-closed：仓库扫描成功但不再包含该 identity（包括只发现一个不同 identity）时不得整仓回退安装，错误列出缺失名称。卡组进度表示已处理数量而非成功数量。
- 未显式指定 Agent 时只使用 Settings 手动启用的 Agent；`-y` 下没有已启用 Agent 直接报错；显式 `--agent` 与 `--all` 优先。
- 默认部署为 link-first；`--copy` 必须真实强制目录复制。
- 卸载 = 删除 canonical 目录、所有指向它的 Agent/项目链接和锁条目；`skillstar-remove-*` staging 残留在下次卸载时自动清理。
- 与 `vercel-labs/skills` 兼容的 Agent 共用项目级 `.agents/skills`（项目域仍由 projects 模块拥有 manifest 语义）。

- 共享频道是绑定到 GitHub 组织专用私有仓库的版本化描述符；数字 `repository_id` 是稳定远程键，`owner`、`name`、HTTPS URL 仅是可变路由元数据。个人账户、公开仓库和非 `github.com` 主机不得绑定。
- 共享频道创建向导只展示当前 GitHub 身份所属的组织，并在提交前说明需要组织仓库 `Administration: write`、`Contents: write`，以及 GitHub App 对所选仓库的完整内容边界。创建者必须具有 Admin；远程权限投影规则为 Admin→owner、Maintain/Write→publisher、Read→subscriber。
- 创建前先校验 SkillStar GitHub App 已安装到目标组织、安装范围为 selected repositories，且授予 `Administration: write` 与 `Contents: write`。仓库由该 App 的用户身份创建；GitHub 会把 App 创建的新仓库自动纳入其 selected-repository 安装范围，SkillStar 不调用 GitHub App 用户令牌不支持的安装范围写接口。
- 共享仓库创建成功后先原子写入非敏感本地登记，状态为 `awaiting_app_installation`，再只读校验 App 可访问该数字 repository ID；若 GitHub 授权尚未生效，用户在安装设置中选择仓库后按 ID 续接，不能重建或凭 owner/name 猜测身份。校验完成后状态变为 `active`，空频道详情显示角色和授权范围。
- 两阶段恢复从 pending descriptor 成功落盘后成立。GitHub 返回创建成功到首次本地落盘之间无法与本地磁盘组成原子事务；若此时进程终止或落盘失败，SkillStar 不猜测同名仓库身份、也不自动删除远端仓库，而是保留它供组织所有者在 GitHub 手动处理。
- 频道描述符与本地 registry 各自显式携带 schema version。registry 不保存 token、邀请秘密或 GitHub credential；所有 GitHub REST 请求复用统一代理客户端和当前登录身份。前端位于独立 `src/features/shared-channels/`，由 My Skills 组合。
- 高级注册流程只列出当前组织 owner 通过 SkillStar GitHub App selected-repository 安装可访问的组织私有仓库。扫描与确认绑定到同一个随机 session 和数字 repository ID；预览未确认前不写频道 registry，确认时重新按 ID 校验 App 访问、Admin、私有性与重复绑定，并刷新改名后的路由元数据。
- 已有仓库扫描通过操作级 Git session 读取当前 revision 的完整 tracked tree，不把稀疏 checkout 或 cache untracked 文件当作远端库存；tree 中的全部 Skill 目录按需物化后再发现。确认页逐项展示发现的全部 Skill 与不属于任何 Skill 的 tracked 文件，并在提交前明确警告：频道成员将能读取整个仓库内容和完整 Git 历史，而不只读取列出的 Skill。扫描支持结构化进度和取消；取消使用 generation tombstone 丢弃晚到结果，确认先原子 claim 预览。session 只保存在当前 GitHub 登录生命周期内，取消、成功确认、登出或进程退出即销毁；确认失败保留原 session 供重试。
- 同一数字 repository ID 最多绑定一个本地共享频道；所有订阅读取先按该 ID 验证远端身份，同一组织内的 owner/name/URL 改名会原子写回频道 registry，不能产生第二个频道。仓库转移到不同组织、ID 被替换或路由指向另一仓库属于完整性错误，不能跟随新位置。扫描、预览和 registry 均不保存 GitHub token、askpass 环境或仓库凭据。
- 共享频道的普通默认分支提交都是草稿；只有 owner/publisher 在 SkillStar 完成发布确认后，订阅者才看见新版本。发布预览绑定当时的精确 commit，若确认前默认分支前进则停止并要求重新预览。
- 发布 revision 由远端不可变 `channel-vNNNNNN` tag 单调生成。annotated tag message 保存 canonical、版本化 release manifest；GitHub Release 保存用户填写的标题与说明。manifest 包含稳定 repository/channel 身份、精确 commit、发布者、时间，以及每个 Skill 的相对内容根、完整 snapshot hash、hash 算法版本和 added/updated/unchanged/removed 状态。removed 项保留上一版路径与 hash 作为审计证据。
- 发布预览用独立的无工作树 partial clone 精确跟随 GitHub API 返回的默认分支，不读取或重置任何本地安装内容；归档时显式禁用 `export-ignore`/`export-subst` 以及 `text`/eol 转换（含 Windows `core.autocrlf`），从 commit 的完整 tracked tree 发现全部 Skill，并使用频道 manifest 校验所用的有界完整目录 hash。不得为 Windows 另备一套 content-hash。预览 session 空闲 30 分钟后回收；响应体、Skill 数量、相对路径、重复 Skill 身份、未知 manifest 字段/schema、tag/commit 不一致都 fail-closed。发布只允许当前 GitHub 有 Admin/Maintain/Write 的用户；App Contents write 不得提升 Read 用户。
- 发布顺序先创建 annotated tag object，再创建 tag ref，最后创建 GitHub Release；只有可验证的非草稿、非预发布 Release 才进入订阅可见版本。Release 失败或结果不确定时不删除 ref，避免并发发布者已使用该 ref 时破坏有效 Release；留下的孤立 tag 只占用 revision、防止复用，不会成为已发布版本，也不在本地提前记录成功 revision。普通 GitHub 拒绝保留原错误分类；若因未授予 Workflows write 而拒绝，界面明确提示 SkillStar 不会请求或自动升级该权限。
- owner 可在频道详情中按 GitHub 用户名邀请 subscriber 或 publisher；subscriber 映射为仓库 `pull/read`，publisher 映射为 `push/write`。管理入口只对本地投影为 owner 的频道显示，所有读写动作仍在后端重新校验当前 GitHub Admin 权限，降权后的 owner 不能继续管理邀请。
- 成员和邀请状态每次从 GitHub 读取，SkillStar 不保存成员表或建立第二套 ACL。有效权限检查采用 GitHub 汇总直接、team、组织和 enterprise 授权后的最高角色；目标用户已有任一直接或继承访问时返回 accepted，不重复创建邀请。成员列表因此只表达 GitHub 当前有效访问，不能臆测授权来源。
- 邀请操作公开 `pending`、`accepted`、`failed`、`cancelled`：新邀请为 pending，已有访问为 accepted，GitHub 拒绝在界面显示 failed，取消成功为 cancelled。GitHub REST 不提供独立 resend 端点；“重新邀请”明确执行取消旧 pending 后重新创建，属于非原子操作，第二步失败时旧邀请保持已取消并提示用户再次邀请。SkillStar 只创建 subscriber/publisher，外部创建的 Admin pending invitation 只允许取消或在 GitHub 管理，不能用 re-invite 静默降为 publisher。
- 被邀请者 inbox 直接读取当前 GitHub 用户的 open repository invitations，只展示组织私有 `github.com` 仓库；GitHub invitation 本身没有 SkillStar 自定义元数据，因此界面明确显示仓库和邀请者，由用户决定是否作为频道导入。接受前先按数字 repository ID 原子写入 `awaiting_invitation_acceptance` 恢复标记，GitHub 接受成功后再转 active，并按 `read`→subscriber、`write/maintain`→publisher、`admin`→owner 投影；若最后一次本地保存失败，已消费的 GitHub invitation 不会丢失恢复入口，用户可从 pending 频道按 repository ID 重试完成导入。GitHub 明确拒绝接受时恢复原 descriptor 或移除新标记；网络中断、5xx 或协议异常使结果不确定时必须保留 marker，由恢复动作按当前远端读权限判定，不能假定远端未处理。若用户改为拒绝仍存在的 invitation，先删除同 repository ID 的不确定 marker，再调用 GitHub decline，避免留下不可恢复的假 pending。私有频道不生成、复制或消费分享码。
- GitHub 是 pending invitation、接受与拒绝的唯一状态真相；刷新后 cancelled/failed 只保留为当前操作反馈，不伪造远端历史。组织外部协作者策略、SAML SSO、2FA、seat、校验、主/次速率限制和每仓库邀请限额必须映射为独立可行动错误，不能归并成模糊的网络失败。
- 接受 GitHub 仓库邀请只建立读取频道的能力，不代表同意把其中的 Skill 安装到本机。active 频道必须先读取并验证最新不可变 Release，再展示稳定频道身份、完整私有仓库暴露、revision、发布者、发布时间、发布说明及全部未移除 Skill；首次评审默认全选，用户可以逐项取消后再确认订阅。
- 订阅确认重新读取最新 Release 并要求 repository ID、organization ID、revision、tag 与 commit 仍和评审目标一致；随后从精确 commit 的独立校验克隆核对每个选中 Skill 的 content root 与完整内容 hash，只有全部通过才调用频道批量安装（写 canonical 与安装锁）。只安装明确选中的 Skill，并记录固定 release target、选中集合、安装后的完整 baseline hash 与不含凭据的 Git URL/ref/skillPath provenance；Agent link/copy 与 Project copy 通过现有 reconciliation 刷新。
- 频道注册表与订阅选择独立持久化在版本化的非敏感本地 store 中，GitHub 仍只负责访问控制和远端发布事实。首次允许选择为空，并记录该发布已评审的 Skill identity 集合；后续即使跳过多个发布，也以该集合识别真正新增项，只作为未选择通知，不能静默扩大已持久化选择。用户明确应用或确认新 revision 后同步推进已评审集合。安装或本地持久化任一步失败时，新安装项必须回滚，旧订阅保持不变。重启后评审从该 store 恢复选择与目标；未来未知 registry、descriptor 或 subscription schema 只做宽容只读投影，并拒绝任何频道或订阅变更，不能猜测迁移；任一未来订阅若无法安全投影 repository 与受管 Skill identity，则整个 ownership 查询 fail-closed，不能把该条静默丢弃后放行通用写入。
- 订阅频道默认由应用后台任务每小时自动检查最新不可变 Release，但升级偏好按频道保存且默认保持手动；关闭自动升级只表示"不自动应用"，不表示"不检查"——`check_update` 是频道 Skill 更新状态的唯一写入者（频道 Skill 被 patrol 排除且 `update_available` 恒为 false），不检查就等于用户永远看不到新版本。因此关闭自动升级的频道照常参与后台探测，只是探测走不写 `remote_state` 的路径，探测失败不会冻结用户的手动操作；用户自己开启的自动升级失败则照常记录 `remote_state`。关闭应用进程时不承诺继续运行。订阅者显式开启受保护自动升级后，会立即检查并自动应用其中的安全项。检查结果展示 revision、发布者、发布时间、说明，以及 added/updated/removed/unchanged 差异。
- 自动升级只应用检查与写入前都仍等于 baseline 的已订阅 Skill，并复用手动升级的精确 Release、staged transaction、最终验证与逐项回滚。pinned、本地分歧、权限变化、removed、完整性错误和上次未解决失败均暂停自动处理；一个暂停项不阻止同频道其他干净项。新增 Skill 永远不自动选择、安装或确认消失，仍需用户显式评审。
- 每个频道持久化自动升级开关、最近尝试/完成时间、目标、已应用项、逐项暂停原因与可重试错误。网络、代理、未登录和临时协议错误保留最近已验证频道状态并等待下次到期重试，不得推断为撤权。手动检查或升级与后台任务共享频道 mutation lease 和统一 Skill 更新事务锁；重启、并发扫描及普通 Skill 更新不能让较旧结果覆盖较新状态。
- 订阅者可从同一频道已验证的历史 Release 中回滚单个已订阅 Skill。候选目标必须同时匹配 repository ID、manifest revision/tag/commit、Skill identity/content root 与完整内容 hash，且必须早于该 Skill 当前安装的发布；任何历史缺失、移动或篡改都在写入前 fail-closed。回滚复用逐 Skill staged update、最终验证与 Agent/Project 部署协调，失败时保留当前版本且不产生 pin。
- 历史回滚成功后只固定该 Skill 的精确 release target，频道整体已评审 target 不倒退。固定项继续展示最新版本与差异，但手动“应用安全更新”和自动升级都跳过它，直到用户显式“恢复跟随频道”。恢复动作原子清除 pin 并重新产生最新升级计划，不在同一步骤隐式覆盖本地内容；之后仍按手动或受保护自动策略应用。pin 和最近计划持久化，重启后不得丢失。
- 当最新已验证 Release 不再包含某个已安装 Skill 时，该项进入独立的 `removed_from_channel` 状态；SkillStar 不删文件、不清 Agent/Project 部署，也不让它阻止同频道其他干净 Skill 升级。用户可显式选择“卸载”，复用既有 canonical、锁、Agent 与 Project 清理语义；或“转为本地副本”，先按完整内容快照创建可编辑、冲突安全的本地 Skill，再解除原项的频道 provenance/跟踪。默认本地名为 `<name>.local`，冲突时使用 `.local.2` 等候选，且允许用户编辑。
- 卸载或转为本地副本成功后，该 Skill 从订阅的 tracked/known/pin 集合中移除，其他逐项升级事实不变。若发布者以后重新加入同名 Skill，它只作为未选中的新安装通知；即便重加发生在用户尚未处理移除时，既有 removal tombstone 也继续阻止普通更新，必须先完成卸载或转本地，再由用户显式“安装并跟踪”。重新安装会验证精确 manifest/commit/hash 并走 staged install，不得覆盖已转换的本地副本。
- 仍被订阅跟踪的频道 Skill 由频道事务独占所有权；通用 My Skills 的默认分支扫描/重装、普通更新、内容编辑/删除、本地创建/收养/旧版迁移、bundle 导入移除、项目扫描导入和普通卸载入口必须在任何 fetch/reset 或文件写入前拒绝这些名称及其受管仓库，不能绕过频道 remote state、不可变 Release 与 tracked/known/pin 元数据。通用更新徽标也不得投影频道 Skill。用户只能在频道面板按升级、removed 或 revoked 流程处理；解除跟踪或转为本地副本后才恢复通用操作。
- 逐 Skill 拒绝只覆盖按名字操作的入口，覆盖不到整目录级的全局存储维护，因此这四条路径按各自语义单独处理。三条 `force_delete_*` 是用户显式的破坏性重置：允许删除频道 Skill，但必须在任何删除动作之前把被删名字回报给 gate 拥有者，由订阅侧同步剪掉 tracked/pin 并清空已存的升级评审快照；订阅本身及 known 集合保留，用户随后仍能从频道面板重新安装。回报失败（例如订阅 store 处于未来 schema 无法安全改写）必须中止整次重置，不能留下"Skill 已删、订阅仍在跟踪"——那种名字既装不回也删不掉。
- 频道注册表与订阅 store 是"已安装内容的归属记录"，不是用户偏好，与安装锁同级；因此应用配置强制删除保留这两个文件，只清理真正的配置项。若把归属记录连同配置一起删掉，canonical 里的频道 Skill 会失去频道身份，普通更新路径随后会用匿名会话去 fetch 私有频道仓库并永久失败，而 gate 已不再拦截。
- 断链清理属于日常维护而非重置，必须整体跳过频道所有权 Skill：断链的频道 Skill 由频道面板修复，清理既不删它的 canonical 目录也不剪它的锁条目，更不改订阅。所有权查询失败一律按"属于频道"处理，读取错误不得升级为删除。存储概览仍会把这类断链计入 broken 数，清理后计数不归零是预期结果。
- 频道 owner 的成员撤销只调用 GitHub 的直接 collaborator 删除接口，不修改 Team、组织 membership 或 base permission；删除后必须重新查询该用户的 effective permission。无剩余权限时显示已撤销；继承权限仍存在时显示“未完全撤销”及 GitHub 管理指引；删除后的复查遇到网络、代理或暂时 API 错误时只报告未确认结果，不得声称权限已撤销。
- 订阅远程生命周期显式区分 `active`、`revoked`、`offline`、`recoverable_failure` 与 `integrity_error`。仓库删除、GitHub App 仓库授权撤销或当前用户明确失去读取权限进入 `revoked`；网络/代理不可达进入 `offline`；未登录、限流及暂时协议/API 失败进入 `recoverable_failure`；repository/organization 身份漂移、未知 manifest schema、tag/commit 解绑、非法或重复 Skill 身份、越界路径、内容根缺失以及完整内容 hash 不一致进入 `integrity_error`。除 `active` 外均冻结频道发起的安装、升级、回滚、历史读取和自动下载，不修改或删除 canonical 内容、锁、Agent/Project 部署及最近一次已验证升级快照；用户纯本地启停既有 Agent/Project 部署不读取或覆盖 Hub 内容，仍属于独立的本机配置操作。
- 显式检查和后台到期检查在冻结状态下只执行只读恢复探测；频道注册表/descriptor 查找失败也必须记录对应冻结状态并禁用旧快照上的升级动作，仓库身份、读取权限和最新 Release 完整性全部重新验证成功后才回到 `active`。`offline` 与 `recoverable_failure` 保留可重试性，不能升级为撤权；`integrity_error` 也必须由新的完整验证清除，不能靠用户忽略告警继续写入。`revoked` 状态仍允许用户逐项卸载，或用可编辑、冲突安全的 `<name>.local` 名称转为本地 Skill；其他冻结状态只保留本地内容和恢复入口，避免把暂时故障或可疑远端解释为删除授权。
- 每次消费 Release 都先验证 descriptor/store schema、stable repository/organization ID、manifest schema、revision/tag/commit 绑定、Skill identity 唯一性、规范化相对 content root 与完整 snapshot hash。精确 Release 验证使用与 Hub 安装内容隔离的 ref cache，绝不为验证而 reset 用户正在编辑的安装 checkout。未知 registry、descriptor 或 subscription schema 只读展示；任何 `..`、绝对路径、反斜杠逃逸、重复 identity、manifest 指向但精确 commit 中不存在的内容根或 hash 不符都在本地 mutation 之前 fail-closed。
- 频道升级以 Skill 为独立应用单元：当前完整内容仍等于订阅 baseline、锁记录的 provenance 仍逐项匹配的 updated Skill 才能进入精确 commit 的频道 staged 升级；即使目标内容 hash 未变化也必须重新检查本地 baseline，本地分歧项在任何锁或部署写入前停止，并复用统一的 `<name>.local` 保留或显式丢弃流程。同组织仓库改名只有在目标 Release 的身份、manifest 与完整内容验证通过后，才以 stable repository ID 授权一次受控路由迁移；成功的可恢复事务把频道 descriptor、安装锁与 subscription provenance URL 刷新为新 clone URL并使 My Skills 来源缓存失效，任一写入失败立即补偿旧值，进程在文件间被强杀留下的中间态由下一次完整验证按 stable repository ID 自愈，不能全局放宽不同仓库覆盖。目标 Release 不得低于订阅 target 或最近已验证 target；远端发布暂时回退时 fail-closed，不能降级已经前进的 Skill。一个 Skill 被阻塞或失败不妨碍其他干净项前进。
- 每项成功后同时更新 canonical 内容、baseline、release hash、无凭据 provenance、update state、Agent 与 Project 部署；任一步失败恢复该 Skill 的旧 canonical 内容、锁条目与部署。精确 Release 读取完成后、替换 canonical 前必须再次检查本地内容；读取期间出现的新编辑一律中止替换并原地保留，补偿回滚需要覆盖这些编辑时先保存为冲突安全的本地副本。订阅状态由逐项事实派生为 `up_to_date`、`update_available`、`partially_upgraded` 或 `blocked`，最近一次已验证检查与逐项结果持久化，离线或未登录时直接从本地 store 展示此前可用状态并允许重试。
- 通用更新不检测本地修改：上游 tree SHA 与锁不符即覆盖式重装（与 `npx skills update` 一致），本地修改会随重装丢失；频道托管的 Skill 例外，其保护由频道自身的订阅 baseline 负责（见上频道章节）。
- 上游已不再包含某技能的 `skillPath` 时，该技能标记为「上游已移除」：chip 入口提供「卸载」与「转为本地副本」两个出口，没有自动删除，也没有更名迁移/后继判定。是否移除最终以重查上游 tree 为准。
- `~/.agents/.skill-lock.json` 是唯一的安装 provenance；读写在 `skill_lock` 单一模块内完成，版本不符静默重置。
- 更新按锁条目逐项执行并汇总结果；单技能失败不阻断其他技能，失败名单带底层原因。

## Agent 注册、手动启用与项目检测

- `BUILTIN_AGENT_DEFS` 是内置 Agent 注册表；自定义 Agent 存储在 profiles 配置中。枚举和数量由代码测试锁定，文档不复制完整清单。
- 本机 Agent 注册表只描述 identity、图标以及 Global/Project 技能目标能力；列表读取不得探测 PATH、桌面应用、配置根或 skills 目录，也不得据此推断 Agent 是否存在。
- 所有内置与新建自定义 Agent 默认关闭。Settings 开关是本机 Agent 激活状态的唯一来源；只有用户显式启用的 profile 才是 Skill、Deck 与 Project 等本机 Agent rail 上的 target。GUI 的单项和批量 Global 部署在后端同样必须拒绝未启用 profile，不得因陈旧 UI/IPC 请求创建 `~/.agent/skills` 类目录；CLI 显式 `--agent` / `--all` 仍是用户的直接授权。关闭后不删除已部署内容；未启用的 profile 不出现在卡片轮播上，即使技能仍链接。
- Settings 的 Agent 列表始终把已启用项置于未启用项之前；两个分组内部保持注册表原有顺序。用户切换开关后列表立即按该规则重排，不另行持久化 UI 排序。
- Settings 按上述顺序默认只展示前 10 个 Agent；超过 10 个时在列表底部显示剩余数量，并由用户显式展开全部或收起回前 10 个。总数不超过 10 个时不渲染折叠控件。
- 冻结的 8 字段 `AgentProfile` 暂时保留 `installed` 以兼容 Tauri IPC；该字段只镜像手动 `enabled` 状态，不再表达系统安装探测。新代码不得以 `installed` 作为可见性或默认值来源。
- 工具栏 Agent 筛选只列当前启用的 profile。SSH 远端 discovery 属于用户显式连接后的远端目录扫描，不复用本机激活规则。
- Settings 的 Agent 行在“已链接技能明细尚未加载”时可用 `synced_count` 作为初始摘要；一旦 `get_agent_managed_skills_state` 返回，计数徽标和展开明细必须共同以其 `active_skill_names` 为准，空数组是有效的 `0`，不得回退到旧摘要。展开状态下即使计数为 `0` 也保留收起入口；收起后不展示零计数徽标。
- 已手动启用且支持 Global skills 的 Agent 行提供紧凑的「当前受管技能」开关。它**不枚举 Hub 安装列表，也不改变 profile 启用状态**：没有恢复 journal 时，后端先原子持久化该目录当时精确的 `active_skill_names`，再临时停用其中成功移除的项；有 journal 时，只尝试恢复其中仍缺失的名字。恢复绝不补齐后来安装、目录中从未存在或已经退出 Hub 的其他技能。失败和受保护的冲突继续留在 journal，供下次恢复重试；恢复成功、或该名字已被手动放回目录后，才从 journal 删除。Hub 中已不存在的恢复源同样保留为未完成项，绝不以其他 Hub 技能替代。
- 此暂停/恢复状态由 `profiles.toml` 按后端解析的物理 Global skills 目录持久化，而不是按 Agent id 持久化。共享目录的每个 Agent 行必须显示同一暂停状态和影响提示、共同禁用进行中操作，并在结束后一起刷新；它不把共享目录伪装成独立的 per-Agent ownership。
- `project_skills_rel` 允许多个 Agent 共享；兼容 open agent skills 规范的 Agent 使用 `.agents/skills`。空字符串仍表达 global-only；Windows 输入统一规范为 `/`。
- **Global 侧的 `global_skills_dir` 同样允许多个 Agent 解析到同一物理目录**，这不是异常配置而是生态约定。共享组由 `BUILTIN_AGENT_DEFS` 派生，不在文档手抄计数；判定必须按解析后的目录而非 agent id。自定义 Agent 的 `global_skills_dir` 在 `custom.rs` 的 `add()` 中只做一条窄校验：展开 `~` 后不得等于家目录本身或文件系统根——「取消全部链接」会遍历该目录并删掉其中的符号链接，这两个值会把它变成一次家目录清扫。指向内置共享目录、不在 `$HOME` 下、末段不叫 `skills` 都仍然允许，那是生态约定而非错误配置。
- 项目检测按路径聚合：唯一且存在的路径可作为项目导入候选；共享且存在的路径返回 `ambiguous_groups`，供 UI/调用方选择一个 manifest owner。它不反向激活 Settings profile。以下三点是**已知与实现不符**（见 D-024）：`detect_project_agents` 未校验项目是否已注册；`scan` 不去重（`.agents/skills` 的一个技能会按共用该路径的每个 profile 各产出一条，去重目前由前端补做）；`rebuild` 的 owner 是先到先得且不过滤启用状态，并抢在 disambiguation 之前落定，因此 `ambiguous_groups` 的选择弹窗在正常开项目流程里不会出现。真正按路径去重的只有 sync 的 `build_path_plans` 与增量安装的 `add_skills_to_project_with_mode`。
- Project registration 必须先于 scan/import/sync。检测、manifest 与部署逻辑均由 `skillstar-skills` facade 提供。

## 部署 reconciliation

- Project sync 同时增加选中技能、移除陈旧技能并清理空 Agent 目录；零技能 Agent 不保留 active 选择。**当前 `clear_project_symlinks` 清空的是目录下全部条目而非仅 manifest 中的陈旧项**，因此共享目录里未登记的技能会被一并清掉；同理 `remove_skill_from_all_projects` 不查 manifest。两者都待 D-024 的 provenance 谓词收敛。
- 部署能力阶梯是 symlink → junction → copy。`deploy_modes` **是当前生效的配置，不是遗留字段**：sync 读它决定 symlink/copy，增量安装会写它。stale copy 刷新必须按 copy 重建：`refresh_stale_copies` 进入重建分支时目标必然是 copy（symlink 与不存在的条目都已跳过），走 symlink-first 路径会把用户显式选择的 copy 静默降级，而 manifest 中的 `deploy_modes` 仍写着 copy。`projects::types` 因此只暴露带 mode 的 `deploy_skill_with_mode`，不提供无 mode 的 auto 变体。已知缺陷：不带 `--copy` 的 CLI 项目安装仍会把整条共享路径的 mode 覆写回 symlink。
- 全局 toggle/batch 与项目 deploy 使用同一能力阶梯。批处理执行全部项并返回累计失败，不在第一项失败时中止。
- 更新后的 `resync_existing_links` 同时刷新 link 和 copy；新部署先在 staging 路径建立，再原子替换。
- 打开项目时，copy 部署通过内容 hash 检测 stale；仅刷新仍被 manifest 选择的技能，不复活用户主动删除的条目。
- unlink 对 link、junction 和 copy 都使用统一删除入口；missing 视为幂等成功。

## 本地创作、Bundle 与 Share

- 本地创作位于 `~/.skillstar/hub/local/<name>`，通过 hub link 暴露。每个受管本地 Skill 在 `<local>/<name>/.skillstar/identity.json` 持有 UUID 身份；改名移动 sidecar，复制或外部 adopt 必须 mint 新 UUID，不得信任来源里的 sidecar。
- 从项目 Agent 目录导入的技能必须先采用到 local，再进入 hub；发布到 GitHub 后可以毕业为 repo-backed install，但只有 staged 安装与最终校验成功才提交新的 Git lock provenance，失败时同时恢复本地内容与发布前 lock 状态。
- 发布前置检查的三态含义是「发布所需的 `git` 是否可用 → SkillStar 是否有可用 GitHub App 身份 → 该身份的 login」，不再是 `gh` 的安装与登录状态。凭据有效但网络/限流导致读不到 login 时仍视为可发布，只是身份标签未知，不把连通性问题报成未登录。
- 发布目标仓库列举使用 `affiliation=owner,collaborator,organization_member` 分页拉取，因此组织仓库和被邀请为协作者的仓库都可以作为发布目标；此前的 `gh repo list <login>` 只能看到个人仓库。新建仓库以 `auto_init=false` 建在当前 GitHub 用户名下，首个 commit 由本地缓存推上去。
- 发布失败不留半成品：clone 失败删除半克隆缓存，新建仓库后的任何一步失败都删除该缓存目录（远端空仓库保留，由用户在 GitHub 处理）。发布 commit 始终携带 `SkillStar <skillstar@local>` committer 身份，不依赖机器上的全局 Git 配置。
- `.ags`/`.agd` 是带 manifest 和 checksum 的 tar.gz。
- Share code 安装由后端 `install_from_share_code` 统一执行“已安装 / git / embedded / skip”决策，前端 modal 不复制循环。
- 本地目录采用由 `adopt_local_folder` 和标准 discovery pipeline 处理，采用时复制完整技能目录（SKILL.md + scripts/references/assets），不只有 manifest；采用前同样经过 frontmatter 质量门禁，无效技能按项跳过并报告原因。CLI 本地目录安装复用同一 facade，不复制采用循环。

## ACP 图文教程

行为契约见 [Learning](../learning/README.md)。Skills 只拥有只读快照与本地身份 sidecar：

- 教程分析对象是当前 Skill 的**整个有效内容目录**。`skillstar-skills::content` 递归枚举目录内的文件，排除不属于 Skill 内容的 `.git`、`.skillstar`、操作系统垃圾和编辑器临时文件，不跟随逃出 Skill 根目录的内部符号链接；确定性 SHA-256 同时覆盖相对路径、文件类型、Unix executable 状态和内容。
- Skill 文件是待分析的不可信资料。教程 ACP 会话必须以当前 Skill 的隔离 staging 快照为工作目录。
- 现有详情页“AI 图文教程”入口、command 名和 wire DTO 在迁移期间保持兼容；Learn UI、Guide/Progress 与 Draft 转换不在本文件。

## Patrol 与页面职责

- Patrol 每个 cycle 先有界并发预取唯一 repo（每仓库一个 git 子进程），再有界并发做本地检查；`interval_secs` 是 cycle 间隔。
- Patrol 状态存入 `~/.skillstar/state/patrol.json`。
- Ghost（已缓存仓库上游新技能）检测已随 [D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线) 移除：无持久仓库缓存后不再有「已缓存仓库」可探测，`check_new_repo_skills`、dismissal 与 `patrol://new-skills-detected` 事件一并删除；patrol 只负责订阅频道的到期检查。
- 开启后台运行时关闭窗口转为隐藏；关闭后台运行时，窗口关闭应退出进程并移除 tray。
- My Skills 管理本地 hub，也组合 remote/cloud scope；scope 共享卡片数据形状和展示面，不伪造一个能力完全一致的数据接口。
- My Skills 本地 scope 的「来源」筛选除按 Hub/Local 类型与仓库过滤外，每个仓库来源行提供移除入口：确认后批量卸载该 `source` 下全部已安装技能（走既有 uninstall + 确认对话框），并在当前筛选指向该来源时清空筛选。
- 每个 GitHub 仓库来源行在移除按钮左侧提供「重新安装」入口：重新扫描当前仓库的全深度 Skill 清单，并只将这一仓库发现的全部 Skill 覆盖式重装（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线) 语义，同名即覆盖）；执行期间该行的重新安装按钮显示加载状态，不影响其他仓库。频道托管的 Skill 仍由 mutation gate 拒绝。
- 本地 scope 工具栏把「来源」筛选与当前列表数量合成同一 pill：左侧为来源标签与下拉/清除，右侧为 `countText`（层叠图标 + 数量）；无来源筛选时数量仍单独成 pill（远端 scope 等同）。
- 本地 scope 处理待更新的默认路径是独立主 CTA「更新 N 项」（与「待更新」筛选分离）：一点即更新 Hub 内全部已标记 `update_available` 的技能（不受当前筛选影响），名单以点击瞬间快照为准，无确认框；结束用既有汇总 toast。单卡「更新」保留为次要 ghost 入口。决策见 [Wayfinder: 更新全部成为默认更新路径](https://github.com/xxww0098/SkillStar/issues/16)。旁边「需处理 (N)」筛选块的 N 与 CTA 不同源：它算的是当前搜索/来源/仓库筛选之后仍需处理（更新或上游已移除）的数量，因此点开筛选一定能看到 N 张卡；其他筛选把需处理技能全部挡掉时 N 显示 0、空态改为说明「当前筛选范围外还有 K 个需要处理的技能」，而 CTA 仍按全局出现。筛选激活时列表上方显示一条状态横幅（「已筛选：仅显示需要处理的技能（N）」+「显示全部」），列表骤减不会被误读为技能丢失。
- 技能详情是覆盖式抽屉：打开时不让位、不挤压卡片网格，右侧卡片可以被盖住。抽屉背后有一层 `bg-black/20` 的点击关闭 scrim，但它**只在 `skill && !editing && !reading` 时渲染**：进入读/编辑 SKILL.md 后 scrim 不渲染、背景恢复可交互，关闭改走浮层自身的关闭入口。SKILL.md 只有一个「查看」入口：已安装技能读实时内容，未安装用市场快照；阅读器头部铅笔进入编辑器，查看/编辑浮层的 X 都直接关整个抽屉，编辑器底部「取消」退回详情页。已安装的 git 技能可以从抽屉重新安装，只重装当前 identity；来源已不再包含它时 fail-closed，不得把整仓当单技能回退。进入批量选择的一次性快捷键提示 toast 从 `bottom-center` 弹出，不遮抽屉底部主操作按钮。
- 详情抽屉只保留一个当前选择：已安装技能按 canonical 身份读取最新列表数据，不能仅因同名而替换来源。更新完成只刷新技能数据，不得重新打开用户已关闭的抽屉，也不得覆盖期间切换的选择。
- 工具栏搜索为常驻内联输入框（共享 `SearchInput`，匹配 name/description/localized_description/source）：输入即时过滤卡片；⌘F 聚焦并全选，`/` 聚焦。Spotlight 弹层（`SpotlightSearch`）只保留在市场页——AI 搜索入口在其输入行内；技能页传 `onSearchSelect` 才会切回该模式。⌘K 仍为全局 Command Palette，不混用。
- Projects 是 master-detail，必须对新增和删除做 reconciliation；Decks/SkillCards 负责组合、导入导出和进入 Projects 的预选流程。
- GitHub 导入扫描结果页的「快速打包」先安装所选技能，再打开新建卡组对话框。卡组名称默认取扫描来源 `owner/repo` 斜杠后面的仓库名（`owner/orca` → `orca`），用户可改。创建态下若该名称已存在，对话框按重名拦截，不得因为预填了同名就放行。
- Deck 的 Agent rail 由卡组自己的 `agent_links`（Agent id 集合）决定，不由成员 Skill 的链接状态推导：**新建、导入、分享码导入的卡组一律不点亮任何 Agent**，即使成员 Skill 已被安装期全局部署链接过去。复制卡组继承来源的 rail。点亮/熄灭仍逐个 Skill 执行链接与取消链接；全部 Skill 都失败时不改写 rail。卡组声明了某 Agent 但已安装成员并非全部实际链接（新增成员、单卡解绑）时显示 mixed 而非点亮，让漂移可见；再点一次补齐链接。缺少该字段的历史卡组由 `skillstar-app::skill_group_links` 一次性回填：成员中全部已安装 Skill 都链接到某 Agent 才算已点亮，回填结果落盘且不改 `updated_at`。

## 前端接缝

- `pages/MySkills.tsx` 保持 scope shell；每个 scope 的 `*Content` 自己持有 toolbar、selection 和 modal 状态。
- 本地与远端只共享 `SkillGrid`/`SkillCard` 的展示数据形状。`my-skills/remote` adapter 负责把 SSH 的 `RemoteSkill` 投影成 `Skill`。
- 库内 / 远端卡片不画「已安装」——整页都已在库中；更新是例外动作。市场卡片保留 rank 与 stars，不画 Hot/Popular 类营销徽标；通用 Git 图标与 `skills.sh` 来源标签也不再出现。
- `ScopeDetailDrawer` 用 discriminated union 表达 local/remote 能力，避免 capability flags 漏洞。
- 不允许 remote content → page → toolbar 的状态回流；生产状态的组件同时消费它。
- `useSkills` 是更新的唯一所有者：`runSkillUpdate(names)` 批量执行覆盖式重装并返回汇总 report（含上游已移除的名字）；页面不得各自发起更新循环。
- 浏览器 dev 的 `lib/ipc/devMock/skillsUpdateStore.ts` 是可变的更新状态源：更新真的会清除 `update_available`，并预置「上游已移除」停止项，使这些路径不进 Tauri 也能实测。
- `ScanResult` 就是来源规格本身（`#[serde(flatten)] spec: Source`），不是另一份 `(source, source_url)`。扫描后的 `ScanResult` 原样回传给 `install_from_scan` 的 `spec` 参数——前端不重新拼 URL，也不拆开 `source`/`source_url` 再传一遍；`git_ref`/`subpath` 通过它自动贯穿到安装。仓库声明 Claude 插件且带 `hooks`/`agents` 时，`ScanResult.plugin` 非空，`ImportModal` 在技能列表上方渲染一行提示（`githubImportModal.claudePluginHint`），CLI 的 `install`/`--list` 打印同一条提示；SkillStar 只装 Skills，不装插件的 hooks/agents（见非目标）。

## 验证

```bash
cargo test -p skillstar-skills
bun run test -- src/features/my-skills src/features/projects
cargo test -p skillstar --lib core::skill_tutorial
bash scripts/internal/check_file_size.sh
```

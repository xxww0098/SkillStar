# Skills、Projects 与 Patrol

状态：active

本文件是技能安装、Agent 注册与手动启用、项目检测、部署、bundle、patrol 和相关 UI 行为的单一事实来源。新增内置 Agent 的操作步骤见 [../agents/README.md](../agents/README.md)。

## 所有权

- `skillstar-skills` 拥有技能 install/update/bundle/local/repo scan、lockfile/update detection、Agent registry、项目 manifest、deployment 和 patrol。无消费者的旧 terminal backend 不作为公共子系统保留。
- `skillstar-core` 只提供共享 `Skill` 契约与基础设施，不拥有技能 lockfile/update detection。
- `src-tauri/src/commands/` 只转发 DTO、State 和事件；CLI 安装/管理复用相同 facade。
- 搜索结果安装等跨 marketplace/skills 流程由 `skillstar-app` 编排。
- 技能组部署的“补全 Marketplace 来源 → 安装缺失技能 → 同步 Project”由 `skillstar-app::skill_group_deploy` 编排，command 与单域 crate 都不复制该事务。
- `skillstar-skills::content` 是技能内容读取、文件枚举、本地创建/删除和嵌套内容目录解析的 facade；Tauri command 不直接组合 hub path、lockfile、Git checkout 与 cache invalidation。
- `skillstar-skills::content` 同时产出教程生成使用的只读 Skill 快照：有效内容根、递归文件清单和确定性内容 hash。`skillstar-skills::tutorial` 拥有 HTML 安全/覆盖校验、freshness 和 artifact 持久化；ACP 子进程与会话编排属于 `src-tauri/src/core/` 的桌面胶水，command 只转发 DTO 和事件。
- `skillstar-skills::skill_update` 拥有 update 事务，`update_skill` 返回完整公开结果，command 不再从底层 update outcome 二次拼装 `Skill` DTO。批量入口 `update_skills` 按物理 checkout 合并更新；相同 URL 的独立 clone 或不同 ref cache 不得误判为共享 checkout。
- `skillstar-skills::update_state` 是 `update_available` 的唯一所有者。批量 refresh、patrol 和 update 完成都写穿它；陈旧判定在该 module 内解决，UI 不再自行挡竞态。
- `skillstar-skills::repo_link` 拥有「hub 条目是否为 repo cache 链接、其 repo root 在哪」的判定；update 检测与 update 应用不得各自解析 symlink/junction。
- 本地目录 adoption、share-code 安装和 deploy-status 检查同样由 Skills 域公开 use case 完成；command 只保留 blocking 调度与 `AppError` 适配。

## GitHub 身份与共享频道认证

- 第一版只连接 `github.com`，使用已注册 SkillStar GitHub App 的设备授权流，不要求用户粘贴 PAT，也不复用用户全局 `gh` 登录。
- `skillstar-skills::github_auth` 提供公开认证 facade；GitHub gateway、凭据仓库和时钟是可替换接缝。生产 gateway 的所有请求必须通过 `probe_http_client`，生产凭据仓库只使用系统钥匙串。
- 设备授权的公开状态只包含用户码、GitHub 验证地址、轮询间隔和到期时间。device code、access token、refresh token 不得进入 IPC DTO、日志、错误、普通配置或 Git remote URL。
- 登录状态按 GitHub 返回的 `expires_in` / `refresh_token_expires_in` 元数据计算，不硬编码 token 寿命。显式刷新会轮换钥匙串凭据并重新读取当前用户；过期且无法刷新的状态要求重新登录。
- Settings 展示登录指导、等待授权、成功身份、过期/拒绝/代理失败和登出。取消或过期会清除进程内待处理设备授权；登出还会清除钥匙串凭据和缓存身份。
- GitHub App 由仓库所有者安装到明确选择的仓库。界面说明后续共享频道需要 `Administration: write`（直接成员邀请/移除）和 `Contents: write`（发布不可变频道版本），不请求 `Workflows: write`；有效操作权限仍受当前 GitHub 用户权限限制。
- 已登录身份也是私有 `github.com` 仓库扫描、安装、更新检查和升级的唯一 Git 认证来源；这些动作不依赖全局 `gh` 登录、Git credential helper 或预先改写过的 remote。
- 每次远程 Git 操作创建独立 session。access token 只通过该子进程继承的临时 askpass 环境提供，操作结束即不可见；token 不得进入 remote URL、持久 Git config、命令参数、普通配置、IPC DTO、进度事件、错误或日志。所有 Git 子进程强制非交互，取消时终止当前子进程，进度只公开 session、阶段和无敏感信息的仓库标识。
- 私有认证只发送给规范化后的 `https://github.com/` 远端。带认证的操作不经过 GitHub 镜像，避免向第三方转发凭据；仍读取 SkillStar 当前代理设置并通过进程环境临时应用。公开仓库沿用无凭据路径，并同样不得弹出终端或系统凭据提示。
- Git 失败按可行动状态区分：未登录、token 已过期、当前用户无仓库权限、GitHub App 未安装/无该仓库授权、网络/代理失败、用户取消。已有缓存和已安装 Skill 在认证或网络失败时保持不变，重试复用同一 Skills 域入口。

## 安装与更新

- 已安装列表先从本地快照返回，远程 update check 在有界后台任务中执行。
- repo-cache 安装只创建指向共享 checkout 的 link，**绝不改写 checkout 内被 git 跟踪的文件**：provenance（git_url/source_folder）只存 lockfile，CLI 与 GUI 安装路径产物一致。共享 checkout 对安装只读，保证 update 的 `git reset --hard` 不会抹掉本地注入的元数据而自造内容分歧。
- repo scan 默认 root-first；仓库根有合法 `SKILL.md` 时首先视作一个技能，也可显式启用全深度 discovery。
- 技能 id 优先使用 frontmatter `name`，再回退目录名；根技能可回退仓库名。
- CLI `install` 与 `add` 是同一命令，来源解析兼容 `npx skills add` 的常用形式：`owner/repo`、`owner/repo/path`、`owner/repo@skill`、GitHub/GitLab tree URL、HTTPS/SSH Git URL、本地 `.ags`/`.agd` 和包含 `SKILL.md` 的本地目录。tree URL 的 ref 与 subpath 必须在 clone/scan 阶段生效，不能把网页 URL 直接交给 Git。
- 多技能来源在交互模式中选择 Skill；`-y` 未显式指定 Skill 时安装发现到的全部 Skill。`--skill '*'` 只展开全部 Skill，`--agent '*'` 只展开全部目标 Agent；`--all` 等价于两者加 `-y`。
- 未显式指定 Agent 时，只使用用户已在 Settings 手动启用的 Agent；SkillStar 不探测本机是否安装了对应 binary、桌面应用或配置目录。`-y` 下没有已启用 Agent 时直接报错，不得回退到全部 Agent。显式 `--agent` 与显式 `--all` 始终优先。
- 交互安装在未显式给出 `--global`/`--project` 时选择 Project 或 Global scope；`-y` 默认 Project。Project 部署进入项目技能目录并登记 `skills-list.json`，Global 部署进入所选 Agent 的用户级技能目录；两者都以 SkillStar hub 为内部 canonical source。
- 默认部署为 link-first；多个目标时交互选择 symlink/copy，`--copy` 必须真实强制目录复制，不能只改变日志。相同目标目录只物化一次。
- 与 `vercel-labs/skills` 兼容的 Agent 共用项目级 `.agents/skills`。共享目录是 universal install surface；Agent 归属只用于 UI/manifest，不得重复部署或让一个 Agent 的移除误删另一个仍在使用的共享目录。
- update 使用 staged swap；刷新失败不得先删除用户现有可用 link/copy。失败按 Agent 聚合并显式返回。
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
- 发布预览用独立的无工作树 partial clone 精确跟随 GitHub API 返回的默认分支，不读取或重置共享 repo cache；归档时显式禁用 `export-ignore`/`export-subst`，从 commit 的完整 tracked tree 发现全部 Skill，并使用与安装/本地分歧相同的有界完整目录 hash。预览 session 空闲 30 分钟后回收；响应体、Skill 数量、相对路径、重复 Skill 身份、未知 manifest 字段/schema、tag/commit 不一致都 fail-closed。发布只允许当前 GitHub 有 Admin/Maintain/Write 的用户；App Contents write 不得提升 Read 用户。
- 发布顺序先创建 annotated tag object，再创建 tag ref，最后创建 GitHub Release；只有可验证的非草稿、非预发布 Release 才进入订阅可见版本。Release 失败或结果不确定时不删除 ref，避免并发发布者已使用该 ref 时破坏有效 Release；留下的孤立 tag 只占用 revision、防止复用，不会成为已发布版本，也不在本地提前记录成功 revision。普通 GitHub 拒绝保留原错误分类；若因未授予 Workflows write 而拒绝，界面明确提示 SkillStar 不会请求或自动升级该权限。
- owner 可在频道详情中按 GitHub 用户名邀请 subscriber 或 publisher；subscriber 映射为仓库 `pull/read`，publisher 映射为 `push/write`。管理入口只对本地投影为 owner 的频道显示，所有读写动作仍在后端重新校验当前 GitHub Admin 权限，降权后的 owner 不能继续管理邀请。
- 成员和邀请状态每次从 GitHub 读取，SkillStar 不保存成员表或建立第二套 ACL。有效权限检查采用 GitHub 汇总直接、team、组织和 enterprise 授权后的最高角色；目标用户已有任一直接或继承访问时返回 accepted，不重复创建邀请。成员列表因此只表达 GitHub 当前有效访问，不能臆测授权来源。
- 邀请操作公开 `pending`、`accepted`、`failed`、`cancelled`：新邀请为 pending，已有访问为 accepted，GitHub 拒绝在界面显示 failed，取消成功为 cancelled。GitHub REST 不提供独立 resend 端点；“重新邀请”明确执行取消旧 pending 后重新创建，属于非原子操作，第二步失败时旧邀请保持已取消并提示用户再次邀请。SkillStar 只创建 subscriber/publisher，外部创建的 Admin pending invitation 只允许取消或在 GitHub 管理，不能用 re-invite 静默降为 publisher。
- 被邀请者 inbox 直接读取当前 GitHub 用户的 open repository invitations，只展示组织私有 `github.com` 仓库；GitHub invitation 本身没有 SkillStar 自定义元数据，因此界面明确显示仓库和邀请者，由用户决定是否作为频道导入。接受前先按数字 repository ID 原子写入 `awaiting_invitation_acceptance` 恢复标记，GitHub 接受成功后再转 active，并按 `read`→subscriber、`write/maintain`→publisher、`admin`→owner 投影；若最后一次本地保存失败，已消费的 GitHub invitation 不会丢失恢复入口，用户可从 pending 频道按 repository ID 重试完成导入。GitHub 明确拒绝接受时恢复原 descriptor 或移除新标记；网络中断、5xx 或协议异常使结果不确定时必须保留 marker，由恢复动作按当前远端读权限判定，不能假定远端未处理。若用户改为拒绝仍存在的 invitation，先删除同 repository ID 的不确定 marker，再调用 GitHub decline，避免留下不可恢复的假 pending。私有频道不生成、复制或消费分享码。
- GitHub 是 pending invitation、接受与拒绝的唯一状态真相；刷新后 cancelled/failed 只保留为当前操作反馈，不伪造远端历史。组织外部协作者策略、SAML SSO、2FA、seat、校验、主/次速率限制和每仓库邀请限额必须映射为独立可行动错误，不能归并成模糊的网络失败。
- 接受 GitHub 仓库邀请只建立读取频道的能力，不代表同意把其中的 Skill 安装到本机。active 频道必须先读取并验证最新不可变 Release，再展示稳定频道身份、完整私有仓库暴露、revision、发布者、发布时间、发布说明及全部未移除 Skill；首次评审默认全选，用户可以逐项取消后再确认订阅。
- 订阅确认重新读取最新 Release 并要求 repository ID、organization ID、revision、tag 与 commit 仍和评审目标一致；随后从精确 commit 的独立 ref cache 校验每个选中 Skill 的 content root 与完整内容 hash，只有全部通过才调用既有 staged batch install。只安装明确选中的 Skill，并记录固定 release target、选中集合、安装后的完整 baseline hash 与不含凭据的 Git URL/ref/source-folder provenance；Agent link/copy 与 Project copy 通过现有 reconciliation 刷新。
- 频道注册表与订阅选择独立持久化在版本化的非敏感本地 store 中，GitHub 仍只负责访问控制和远端发布事实。首次允许选择为空，并记录该发布已评审的 Skill identity 集合；后续即使跳过多个发布，也以该集合识别真正新增项，只作为未选择通知，不能静默扩大已持久化选择。用户明确应用或确认新 revision 后同步推进已评审集合。安装或本地持久化任一步失败时，新安装项必须回滚，旧订阅保持不变。重启后评审从该 store 恢复选择与目标；未来未知 registry、descriptor 或 subscription schema 只做宽容只读投影，并拒绝任何频道或订阅变更，不能猜测迁移；任一未来订阅若无法安全投影 repository 与受管 Skill identity，则整个 ownership 查询 fail-closed，不能把该条静默丢弃后放行通用写入。
- 订阅频道默认由应用后台任务每小时自动检查最新不可变 Release，但升级偏好按频道保存且默认保持手动；关闭应用进程时不承诺继续运行。订阅者显式开启受保护自动升级后，会立即检查并自动应用其中的安全项。检查结果展示 revision、发布者、发布时间、说明，以及 added/updated/removed/unchanged 差异。
- 自动升级只应用检查与写入前都仍等于 baseline 的已订阅 Skill，并复用手动升级的精确 Release、staged transaction、最终验证与逐项回滚。pinned、本地分歧、权限变化、removed、完整性错误和上次未解决失败均暂停自动处理；一个暂停项不阻止同频道其他干净项。新增 Skill 永远不自动选择、安装或确认消失，仍需用户显式评审。
- 每个频道持久化自动升级开关、最近尝试/完成时间、目标、已应用项、逐项暂停原因与可重试错误。网络、代理、未登录和临时协议错误保留最近已验证频道状态并等待下次到期重试，不得推断为撤权。手动检查或升级与后台任务共享频道 mutation lease 和统一 Skill 更新事务锁；重启、并发扫描及普通 Skill 更新不能让较旧结果覆盖较新状态。
- 订阅者可从同一频道已验证的历史 Release 中回滚单个已订阅 Skill。候选目标必须同时匹配 repository ID、manifest revision/tag/commit、Skill identity/content root 与完整内容 hash，且必须早于该 Skill 当前安装的发布；任何历史缺失、移动或篡改都在写入前 fail-closed。回滚复用逐 Skill staged update、最终验证与 Agent/Project 部署协调，失败时保留当前版本且不产生 pin。
- 历史回滚成功后只固定该 Skill 的精确 release target，频道整体已评审 target 不倒退。固定项继续展示最新版本与差异，但手动“应用安全更新”和自动升级都跳过它，直到用户显式“恢复跟随频道”。恢复动作原子清除 pin 并重新产生最新升级计划，不在同一步骤隐式覆盖本地内容；之后仍按手动或受保护自动策略应用。pin 和最近计划持久化，重启后不得丢失。
- 当最新已验证 Release 不再包含某个已安装 Skill 时，该项进入独立的 `removed_from_channel` 状态；SkillStar 不删文件、不清 Agent/Project 部署，也不让它阻止同频道其他干净 Skill 升级。用户可显式选择“卸载”，复用既有 Hub、lockfile、Agent 与 Project 清理语义；或“转为本地副本”，先按完整内容快照创建可编辑、冲突安全的本地 Skill，再解除原项的频道 provenance/跟踪。默认本地名为 `<name>.local`，冲突时使用 `.local.2` 等候选，且允许用户编辑。
- 卸载或转为本地副本成功后，该 Skill 从订阅的 tracked/known/pin 集合中移除，其他逐项升级事实不变。若发布者以后重新加入同名 Skill，它只作为未选中的新安装通知；即便重加发生在用户尚未处理移除时，既有 removal tombstone 也继续阻止普通更新，必须先完成卸载或转本地，再由用户显式“安装并跟踪”。重新安装会验证精确 manifest/commit/hash 并走 staged install，不得覆盖已转换的本地副本。
- 仍被订阅跟踪的频道 Skill 由频道事务独占所有权；通用 My Skills 的默认分支扫描/重装、普通更新、本地分歧继续更新、内容编辑/删除、本地创建/收养/旧版迁移、bundle/pack 导入移除、项目扫描导入和普通卸载入口必须在任何 fetch/reset 或文件写入前拒绝这些名称及其受管仓库，不能绕过频道 remote state、不可变 Release 与 tracked/known/pin 元数据。通用更新徽标也不得投影频道 Skill。用户只能在频道面板按升级、removed 或 revoked 流程处理；解除跟踪或转为本地副本后才恢复通用操作。
- 频道 owner 的成员撤销只调用 GitHub 的直接 collaborator 删除接口，不修改 Team、组织 membership 或 base permission；删除后必须重新查询该用户的 effective permission。无剩余权限时显示已撤销；继承权限仍存在时显示“未完全撤销”及 GitHub 管理指引；删除后的复查遇到网络、代理或暂时 API 错误时只报告未确认结果，不得声称权限已撤销。
- 订阅远程生命周期显式区分 `active`、`revoked`、`offline`、`recoverable_failure` 与 `integrity_error`。仓库删除、GitHub App 仓库授权撤销或当前用户明确失去读取权限进入 `revoked`；网络/代理不可达进入 `offline`；未登录、限流及暂时协议/API 失败进入 `recoverable_failure`；repository/organization 身份漂移、未知 manifest schema、tag/commit 解绑、非法或重复 Skill 身份、越界路径、内容根缺失以及完整内容 hash 不一致进入 `integrity_error`。除 `active` 外均冻结频道发起的安装、升级、回滚、历史读取和自动下载，不修改或删除 Hub 内容、lockfile、Agent/Project 部署及最近一次已验证升级快照；用户纯本地启停既有 Agent/Project 部署不读取或覆盖 Hub 内容，仍属于独立的本机配置操作。
- 显式检查和后台到期检查在冻结状态下只执行只读恢复探测；频道注册表/descriptor 查找失败也必须记录对应冻结状态并禁用旧快照上的升级动作，仓库身份、读取权限和最新 Release 完整性全部重新验证成功后才回到 `active`。`offline` 与 `recoverable_failure` 保留可重试性，不能升级为撤权；`integrity_error` 也必须由新的完整验证清除，不能靠用户忽略告警继续写入。`revoked` 状态仍允许用户逐项卸载，或用可编辑、冲突安全的 `<name>.local` 名称转为本地 Skill；其他冻结状态只保留本地内容和恢复入口，避免把暂时故障或可疑远端解释为删除授权。
- 每次消费 Release 都先验证 descriptor/store schema、stable repository/organization ID、manifest schema、revision/tag/commit 绑定、Skill identity 唯一性、规范化相对 content root 与完整 snapshot hash。精确 Release 验证使用与 Hub 安装内容隔离的 ref cache，绝不为验证而 reset 用户正在编辑的安装 checkout。未知 registry、descriptor 或 subscription schema 只读展示；任何 `..`、绝对路径、反斜杠逃逸、重复 identity、manifest 指向但精确 commit 中不存在的内容根或 hash 不符都在本地 mutation 之前 fail-closed。
- 频道升级以 Skill 为独立应用单元：当前完整内容仍等于订阅 baseline、旧 checkout HEAD 仍等于逐项 provenance 的 updated Skill 才能进入精确 commit 的 staged update transaction；即使目标内容 hash 未变化也必须重新检查本地 baseline，本地分歧项在任何 checkout、lockfile 或部署写入前停止，并复用统一的 `<name>.local` 保留或显式丢弃流程。同组织仓库改名只有在目标 Release 的身份、manifest 与完整内容验证通过后，才以 stable repository ID 授权一次受控路由迁移；成功的可恢复事务把频道 descriptor、lock 与 subscription provenance URL 刷新为新 clone URL并使 My Skills 来源缓存失效，任一写入失败立即补偿旧值，进程在文件间被强杀留下的中间态由下一次完整验证按 stable repository ID 自愈，不能全局放宽不同仓库覆盖。目标 Release 不得低于订阅 target 或最近已验证 target；远端发布暂时回退时 fail-closed，不能降级已经前进的 Skill。一个 Skill 被阻塞或失败不妨碍其他干净项前进。
- 每项成功后同时更新文件、baseline、release hash、无凭据 provenance、update state、Agent 与 Project 部署；任一步失败恢复该 Skill 的旧 checkout、lockfile 与部署。精确 Release 读取完成后、替换 Hub 前必须再次检查本地内容；读取期间出现的新编辑一律中止替换并原地保留，补偿回滚需要覆盖这些编辑时先保存为冲突安全的本地副本。订阅状态由逐项事实派生为 `up_to_date`、`update_available`、`partially_upgraded` 或 `blocked`，最近一次已验证检查与逐项结果持久化，离线或未登录时直接从本地 store 展示此前可用状态并允许重试。
- Git-backed Skill 安装或成功更新后记录完整受管内容的 baseline hash。更新开始前必须重新计算当前完整目录 hash；若与 baseline 不同，则将该 Skill 标记为“本地分歧”并在任何 fetch/reset、lockfile 写入或部署变更之前停止。
- `lock.json` 当前 schema 为 v5，并显式记录完整内容 hash 算法版本；旧版、缺失、损坏或未来版本的 baseline 一律 fail-closed，不把未知状态推断为“未修改”。
- 本地分歧只能由用户显式解决：一是把当前完整内容保留为独立本地副本后继续更新，二是清理 tracked、untracked 与 ignored 的受管修改后继续更新。默认副本名为 `<原名>.local`，允许编辑；名称冲突时提出 `.local.2` 等非破坏性候选。是否为本地 Skill 只由存储位置与 provenance/type 决定，不解析名称后缀。所有页面的单 Skill 更新入口共享同一个选择对话框；CLI 在交互终端提供同样的保留/丢弃/跳过选择，非交互输入保持 fail-closed。
- 完整内容 hash 与保留副本覆盖 `SKILL.md`、scripts、templates、references、assets、Unix executable 状态及其他受管文件，同时排除 `.git`、SkillStar 自有状态和操作系统临时文件。分歧检测严格只读，不得为 lazy worktree 触发 checkout。共享同一 repo checkout 的任一已安装 Skill 存在本地分歧时，该次 repo pull 必须整体停在写入之前；每次只清理用户已确认的 Skill 子树，整组全部解决后才允许移动 checkout。
- repo check/update 复用 `~/.skillstar/hub/repos/` cache，远程 HTTP/Git 遵循统一 proxy/mirror 规则；任何既有 cache 在 fetch/reset 前都必须从 Hub 实际链接枚举其全部已安装 Skill 并逐项核对 lock 与完整 baseline，缺少 lock、发现修改或 baseline 未知时停止并要求用户先保留或显式丢弃。
- 一次 update 是一个事务：pull、lockfile hash 写入、同 checkout 兄弟技能的 hash 扇出、Agent relink、项目 cascade 和 update state 清除必须走同一入口。pull 后若完整 baseline 或 lockfile 原子保存失败，先恢复旧 Git revision、旧 sparse-checkout 配置与更新前受管内容，再返回失败；`.skillstar`、编辑器临时文件等不属于 baseline 的运行时文件不得被回滚清理。GUI 与 CLI 走同一入口，任何调用方都不得只做 pull。
- 批量 update 每个 repo 只拉一次；未被拉取但内容随之移动的技能报告为 `skipped`，失败的 repo 把它本会覆盖的全部名字报告为 `failed`，不得计入成功。
- `update_available` 的判定可能过期：一次扫描开始后若该技能被更新，扫描结果作废。该规则由 `update_state` 按技能名的 revision 裁决，扫描以起始 revision 提交。patrol 事件是通知而非记录，其载荷是已裁决后的状态。

## Agent 注册、手动启用与项目检测

- `BUILTIN_AGENT_DEFS` 是内置 Agent 注册表；自定义 Agent 存储在 profiles 配置中。枚举和数量由代码测试锁定，文档不复制完整清单。
- 本机 Agent 注册表只描述 identity、图标以及 Global/Project 技能目标能力；列表读取不得探测 PATH、桌面应用、配置根或 skills 目录，也不得据此推断 Agent 是否存在。
- 所有内置与新建自定义 Agent 默认关闭。Settings 开关是本机 Agent 激活状态的唯一来源；只有用户显式启用的 profile 才能进入 Skill、Deck、Project 与 MCP 等本机 Agent rail。关闭后立即从这些投影中移除，但不删除已部署内容。
- Settings 的 Agent 列表始终把已启用项置于未启用项之前；两个分组内部保持注册表原有顺序。用户切换开关后列表立即按该规则重排，不另行持久化 UI 排序。
- Settings 按上述顺序默认只展示前 10 个 Agent；超过 10 个时在列表底部显示剩余数量，并由用户显式展开全部或收起回前 10 个。总数不超过 10 个时不渲染折叠控件。
- 冻结的 8 字段 `AgentProfile` 暂时保留 `installed` 以兼容 Tauri IPC；该字段只镜像手动 `enabled` 状态，不再表达系统安装探测。新代码不得以 `installed` 作为可见性或默认值来源。
- MCP rail 只在手动启用 profile 的基础上叠加静态能力映射，不再用 MCP tool 安装探测决定是否渲染；真实写入失败由执行动作返回。SSH 远端 discovery 属于用户显式连接后的远端目录扫描，不复用本机激活规则。
- Settings 的 Agent 行在“已链接技能明细尚未加载”时可用 `synced_count` 作为初始摘要；一旦 `list_linked_skills` 返回，计数徽标和展开明细必须共同以该列表为准，空数组是有效的 `0`，不得回退到旧摘要。展开状态下即使计数为 `0` 也保留收起入口；收起后不展示零计数徽标。
- `project_skills_rel` 允许多个 Agent 共享；兼容 open agent skills 规范的 Agent 使用 `.agents/skills`。空字符串仍表达 global-only；Windows 输入统一规范为 `/`。
- 项目检测只识别已注册项目中的技能目录，不反向激活 Settings profile。它按路径聚合：唯一且存在的路径可作为项目导入候选；共享且存在的路径返回 `ambiguous_groups`，由 UI/调用方选择一个 manifest owner。scan、rebuild、sync 与 cleanup 均按路径去重。
- Project registration 必须先于 scan/import/sync。检测、manifest 与部署逻辑均由 `skillstar-skills` facade 提供。

## 部署 reconciliation

- Project sync 同时增加选中技能、移除陈旧技能并清理空 Agent 目录；零技能 Agent 不保留 active 选择。
- 部署能力阶梯是 symlink → junction → copy；`deploy_modes` 只为兼容旧 manifest，当前实现按真实文件类型判断。
- 全局 toggle/batch 与项目 deploy 使用同一能力阶梯。批处理执行全部项并返回累计失败，不在第一项失败时中止。
- 更新后的 `resync_existing_links` 同时刷新 link 和 copy；新部署先在 staging 路径建立，再原子替换。
- 打开项目时，copy 部署通过内容 hash 检测 stale；仅刷新仍被 manifest 选择的技能，不复活用户主动删除的条目。
- unlink 对 link、junction 和 copy 都使用统一删除入口；missing 视为幂等成功。

## 本地创作、Bundle 与 Share

- 本地创作位于 `~/.skillstar/hub/local/<name>`，通过 hub link 暴露。
- 从项目 Agent 目录导入的技能必须先采用到 local，再进入 hub；发布到 GitHub 后可以毕业为 repo-backed install，但只有 staged 安装与最终校验成功才提交新的 Git lock provenance，失败时同时恢复本地内容与发布前 lock 状态。
- `.ags`/`.agd` 是带 manifest 和 checksum 的 tar.gz。
- Share code 安装由后端 `install_from_share_code` 统一执行“已安装 / git / embedded / skip”决策，前端 modal 不复制循环。
- 本地目录采用由 `adopt_local_folder` 和标准 discovery pipeline 处理。

## ACP 图文教程

- SKILL.md 翻译入口已移除。已安装 Skill 的详情页提供唯一的“AI 图文教程”入口，阅读器和编辑器只保留原文/编辑与摘要能力；教程只使用 Settings 中显式启用的 ACP Agent。
- 教程分析对象是当前 Skill 的**整个有效内容目录**，不是只把 `SKILL.md` 文本发给模型。`skillstar-skills::content` 递归枚举目录内的文件，排除不属于 Skill 内容的 `.git`、`.skillstar`、操作系统垃圾和编辑器临时文件，不跟随逃出 Skill 根目录的内部符号链接；确定性 SHA-256 同时覆盖相对路径、文件类型、Unix executable 状态和内容。该文件清单随 prompt 提供，生成结果必须逐项给出覆盖说明。
- Skill 文件是待分析的不可信资料，不能覆盖系统任务。教程 ACP 会话必须以当前 Skill 的隔离 staging 快照为工作目录，ACP 协议侧只开放根内读文件能力并拒绝 terminal/写入权限，prompt 同时禁止网络和修改；模型必须先核对完整清单，再输出一个自包含 HTML5 文档。用户配置的 ACP 可执行程序仍是本机受信任边界，SkillStar 不把任意外部程序伪装成 OS sandbox。
- HTML 必须使用当前界面语言，包含基于真实内容的步骤、示例、文件导航、故障排查和至少一个有信息量的内联 SVG 图示；不得虚构未在 Skill 中出现的能力。证据引用使用相对文件路径；推断必须显式标记。
- Settings → ACP 持久化教程风格，初始提供 `guided`（循序导览，默认）、`reference`（技术手册）与 `workshop`（实战工坊）。三种风格分别使用独立 prompt 片段改变信息组织、示例密度和图示重点，而不是给同一 HTML 换 CSS；风格 id、所选风格在内的完整 prompt bundle hash 与规范化界面语言共同进入 artifact 版本键和 freshness 判定，修改提示词无需依赖人工记得提升版本号。
- 输出只允许内联 CSS、内联 SVG 和 `data:` 图片，不允许 JavaScript、事件属性、表单、iframe、外链资源或网络 URL。后端在持久化前校验完整 HTML、全部文件覆盖和危险结构；前端只在无权限的 sandbox iframe 中展示。
- ACP 只返回完整 HTML 文件内容，不负责发布或托管；在线预览/分享 URL 不能视为成功结果。后端校验后通过跨进程文件锁、同步落盘、staging/backup 目录替换与中断恢复，把结果持久化到本机 `~/.skillstar/tutorials/<skill-key>/tutorial.html` 与同目录 metadata，断网仍可打开。metadata 至少记录 Skill 名称、内容 hash、完整源文件清单、教程风格、prompt/schema 版本、ACP Agent 和生成时间；目录 key 由 Skill 名称派生，不能直接信任名称作为路径。
- 打开详情时先计算当前 Skill hash。hash、所选风格、规范化界面语言与 prompt/schema 版本均匹配时直接复用持久化 HTML；内容、语言或风格/prompt 变化时保留旧教程可读，但必须显示过期原因和“更新教程”动作。刷新失败不得覆盖最后一个可用教程。
- 生成开始和结束各取一次快照；若 Skill 在 ACP 分析期间发生变化，本次结果不得标记或写入为最新，用户需要基于新版本重试。编辑器存在未保存修改时不得开始生成，避免磁盘快照与屏幕内容不一致。

## Patrol 与页面职责

- Patrol 每个 cycle 先预取唯一 repo，再逐技能本地检查；`interval_secs` 是 cycle 间隔。
- Patrol 状态存入 `~/.skillstar/state/patrol.json`。
- 开启后台运行时关闭窗口转为隐藏；关闭后台运行时，窗口关闭应退出进程并移除 tray。
- My Skills 管理本地 hub，也组合 remote/cloud scope；scope 共享卡片数据形状和展示面，不伪造一个能力完全一致的数据接口。
- My Skills 本地 scope 的「来源」筛选除按 Hub/Local 类型与仓库过滤外，每个仓库来源行提供移除入口：确认后批量卸载该 `source` 下全部已安装技能（走既有 uninstall + 确认对话框），并在当前筛选指向该来源时清空筛选。
- 每个 GitHub 仓库来源行在移除按钮左侧提供「重新安装」入口：重新扫描当前仓库的全深度 Skill 清单，并只将这一仓库发现的全部 Skill 重新安装；执行期间该行的重新安装按钮显示加载状态，不影响其他仓库。
- 本地 scope 工具栏把「来源」筛选与当前列表数量合成同一 pill：左侧为来源标签与下拉/清除，右侧为 `countText`（层叠图标 + 数量）；无来源筛选时数量仍单独成 pill（远端 scope 等同）。
- 本地 scope 处理待更新的默认路径是独立主 CTA「更新 N 项」（与「待更新」筛选分离）：一点即更新 Hub 内全部已标记 `update_available` 的技能（不受当前筛选影响），名单以点击瞬间快照为准，无确认框；结束用既有汇总 toast。单卡「更新」保留为次要 ghost 入口。决策见 [Wayfinder: 更新全部成为默认更新路径](https://github.com/xxww0098/SkillStar/issues/16)。
- 工具栏搜索为 Spotlight 弹层（`SpotlightSearch`）：常驻仅为紧凑搜索按钮；⌘F / `/` 打开，Esc 关闭但保留 query；结果列表 ↑↓ 选择、Enter 打开详情；query 与背后列表过滤同步。⌘K 仍为全局 Command Palette，不混用。
- Projects 是 master-detail，必须对新增和删除做 reconciliation；Decks/SkillCards 负责组合、导入导出和进入 Projects 的预选流程。

## 前端接缝

- `pages/MySkills.tsx` 保持 scope shell；每个 scope 的 `*Content` 自己持有 toolbar、selection 和 modal 状态。
- 本地与远端只共享 `SkillGrid`/`SkillCard` 的展示数据形状。`my-skills/remote` adapter 负责把 SSH 的 `RemoteSkill` 投影成 `Skill`。
- `ScopeDetailDrawer` 用 discriminated union 表达 local/remote 能力，避免 capability flags 漏洞。
- 不允许 remote content → page → toolbar 的状态回流；生产状态的组件同时消费它。

## 验证

```bash
cargo test -p skillstar-skills
bun run test -- src/features/my-skills src/features/projects
cargo test -p skillstar --lib core::skill_tutorial
bash scripts/internal/check_file_size.sh
```

# Skills、Projects 与 Patrol

状态：active

本文件是技能安装、Agent 注册与手动启用、项目检测、部署、bundle、patrol 和相关 UI 行为的单一事实来源。新增内置 Agent 的操作步骤见 [../agents/README.md](../agents/README.md)。

## 所有权

- `ss-skills` 拥有技能 install/update/bundle/local/repo scan、安装锁/update detection、Agent registry、项目 manifest、deployment 和 patrol。无消费者的旧 terminal backend 不作为公共子系统保留。
- `ss-core` 只提供共享 `Skill` 契约与基础设施，不拥有技能安装锁/update detection。
- GUI 和 CLI 都调用同一 facade。壳不复制安装事务。
- 搜索结果安装等跨 marketplace/skills 流程由 `ss-app` 编排。
- 技能组部署的“补全 Marketplace 来源 → 安装缺失技能 → 同步 Project”由 `ss-app::skill_group_deploy` 编排，command 与单域 crate 都不复制该事务。
- `ss-skills::content` 是技能内容读取、文件枚举、本地创建/删除和嵌套内容目录解析的 facade；壳不直接组合 canonical 路径、安装锁与 cache invalidation。内容 facade 对外部输入先执行 `validate_skill_name`，再 canonicalize 有效目录并限制在 canonical skills 目录或本地创作目录内；外部符号链接不会被 read/list/open 跟随。只读快照（有效内容根、递归文件清单和确定性内容 hash）服务频道基线校验。通用技能会不会被自动覆盖，由安装基线判断，见[生命周期](#生命周期)。
- `ss-skills::update` 拥有通用更新路径（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线)）：按锁的 source+ref 分组对比上游 tree SHA。手动更新在 tree 变化时覆盖式重装；自动更新还要看安装基线。规则见[生命周期](#生命周期)。`update_skill(s)` 返回完整公开结果，command 不从底层 outcome 二次拼装 `Skill` DTO。
- `ss-skills::update_state` 是 `update_available` 的唯一所有者；保存每技能 `update_available/upstream_change/checked_at` 投影，批量 refresh 和 update 完成都写穿它。批量检查开始前取时间戳（`stamp`），结束后只写入此后没有被更权威结果（`record`，如更新完成）覆盖的条目（`commit_scan`），慢检查不能把刚完成的更新改回「可更新」。
- 本地目录 adoption、share-code 安装和 deploy-status 检查同样由 Skills 域公开 use case 完成；command 只保留 blocking 调度与 `AppError` 适配。

## 生命周期

获取、安装、维护、监测、修复、升级，以及把本机 Agent 已装技能纳管进来，共用下面这一组不变量。[D-094](../../decisions.md#d-094技能物化统一为校验暂存交换原语部署所有权由链接目标或部署标记证明) 到 [D-097](../../decisions.md#d-097本机-agent-已装技能经可预览的纳管计划进入-skillstar) 分别记录物化与所有权、更新与频道升级、存储健康、本机纳管；合在一起的选择是 [D-098](../../decisions.md#d-098技能生命周期不变量)。复制上限、排除名和 Agent 清单以代码常量为准，这里不手抄。

### 来源变成规范副本

- 来源经 `Source::parse` 解析，内容经 `ss-skills::fetch` 进入 checkout。普通导入可以复用按仓库与 ref 隔离的缓存（[D-090](../../decisions.md#d-090技能扫描与添加按需获取内容)、[D-092](../../decisions.md#d-092导入复用本地-git-缓存并锁定预览提交)）。发现 `SKILL.md` 之后，落盘名只经 `materialize::canonical_skill_name` 换算一次（`installer` 再导出这个函数）：按 vercel 规则折叠字符，再过 `validate_skill_name`。换算后为空，或含非 ASCII 的名字，不可安装。同一批次里两个技能落到同一个目录名时，整批拒绝。
- 规范副本是 `~/.skillstar/data/skills/installed/<name>` 上的真实目录（[D-100](../../decisions.md#d-100已安装技能的规范副本放在-skillstar-数据根)）。`~/.agents/skills` 只是部分 Agent 自己的目录，安装不会往那里写正文。数据根隔离见 [架构](../../architecture.md)。用户把技能部署到某个 Agent 时，该 Agent 的全局目录用链接指向规范副本；Windows junction 的目标形式见架构文档。全局目录等于或位于规范根之内的 Agent 不建链，也不取消链接，技能由规范根直接提供（`deployment::targets_canonical_root`）。按名字特判会漏掉以后把目录指进规范根的自定义 Agent。
- 本地创作、本机纳管和频道安装最后也进入同一份规范副本。本地创作和纳管的文件在本地技能目录，规范根上是指向它的链接；Git 与频道来源把 checkout 里的那一份复制进规范根。

### 落盘

- 本地创作 Skill 的旧位置迁移遇到同名冲突时保留原内容与链接，重连规则见 [存储迁移语义](../../storage-layout.md#迁移语义)。
- 唯一写入原语是 `ss-skills::materialize`（[D-094](../../decisions.md#d-094技能物化统一为校验暂存交换原语部署所有权由链接目标或部署标记证明)）。顺序是：校验名字，用 `copy_confined` 复制到目标旁边的隐藏暂存目录（前缀 `TRANSIENT_PREFIX`，列表忽略），再交换（旧目录先挪到备份位），最后写安装锁。任一步失败就还原已交换的目录，锁保持交换前的内容。交换和回滚的 rename 有界重试。
- 受限复制只解引用解析后仍在来源边界内的符号链接。逃出来源的链接和断链跳过。已经经由链接复制过的目录不再跟第二次；真实目录即使先被链接指到，仍按原位复制。文件数、总字节或目录深度超过 `COPY_LIMITS` 时整次失败。解析后路径的任一组件命中调用方传入的排除名就跳过，指向排除名的链接因此不会把那棵树复制进来。
- 最外层取得技能事务锁时，按间隔清扫规范根和各 Agent 全局目录里、自残留创建时刻起超过 `STALE_TRANSIENT_AGE` 的暂存（同盘 rename 保留的目录 mtime 不算这个时刻；`materialize::sweep_stale_transients_now_and_then`）。目标还在的 `.skillstar-stage-*`，以及目标还在且替换已经提交的 `-backup-*` / `-retain-*`，才会被清掉。目标已经不在的 `-backup-*`、`-remove-*`、`-retain-*` 留给医生还原，清扫不删。同线程重入不重复清扫。`skill_update::sweep_stale_transients` 是持锁后立即清扫的显式入口。
- 卸载先确认锁可写，再持有同一把事务锁：把规范目录改名到隐藏的移除暂存，删锁条目，最后删目录。锁写失败就把目录改回去。频道移除若提交失败，目录和锁条目都要还原；`rollback_complete` 只有两者都还原才为真。

### 部署所有权

- Agent 路径上是不是 SkillStar 的，只有 `deployment::ownership::owned_deployment`。链接必须正好指向 `<规范根|本地技能目录|旧 hub 的 skills>/<技能名>`，并且该路径是目录。断链仍算待清理的自有链接。指向子目录，或旧 hub 根下 `skills/` 以外的位置，算外来。
- 复制部署必须有 `.skillstar-deploy.json`，且其中 `contentHash` 与当前目录的 `dir_content_hash` 一致。hash 为空或不一致算外来，不删除，也不刷新。没有标记的目录，只有在不含比较排除名、并且文件集合与规范副本相同时，才算旧复制。这个 hash 对路径和文件长度做前缀，避免把不同的树拼成同一个摘要。项目路径用 `owned_project_deployment`：没有标记只报告，不删除。
- 带比较排除名的目录，以及内容和规范副本不同的手工目录，不会被卸载或修复删掉。文件集合恰好相同、且不含那些名字的手工副本，仍会被当成旧部署。这是留下的边界，见 [错误记录](../../errors.md)。

### 锁

- 安装来源只有 `~/.skillstar/data/skills/.skill-lock.json` v3，读写都在 `skill_lock`。启动时把原先的 `~/.agents/.skill-lock.json`（以及 `$XDG_STATE_HOME/skills/.skill-lock.json`）迁到这里，新锁已存在则保留新锁。字段与 vercel 对齐：`source`、`sourceType`、`sourceUrl`、`ref`、`skillPath`、`skillFolderHash`（git tree SHA）、`installedAt`、`updatedAt`。磁盘上的 ref 键是 `ref`，读取同时接受以前的 `gitRef`。`lastSelectedAgents` 按 camelCase 写出，也接受旧的 snake_case。未知顶层字段、未知条目字段和无法解析的条目原样保留。未知 `sourceType` 读成 `unknown`，不参与更新。
- 条目键经 `folder_for_key`（与 `canonical_skill_name` 相同）对应目录；`entry_for_folder` 按目录找回条目。`upsert` 合并同一目录旧键上的未知字段，并保留最早的 `installedAt`。同名再装是覆盖安装并改写来源，没有跨源拒绝。tree URL 的 ref 与 subpath 记在锁的 `ref` 和 `skillPath` 里，没有单独的 pinned 标志。
- 读侧（列表、徽标）对缺失、过旧、过新和损坏都展示为空锁，不改文件。写侧经 `mutate`：持 `runtime/locks/skills/skill-lock.lock`，持锁后重读，再原子替换。缺失视为空锁。旧 schema 备份后从空锁重写，与 vercel 一致。版本过新或解析失败先备份，再拒绝写入，不得用空锁覆盖。安装和卸载在改规范目录之前调用 `ensure_writable`；写失败则本次回滚。显式恢复才调用 `reset_after_backup`。
- 设置里的「删除已安装技能」在第二次确认前，用和执行相同的名单（`storage_maintenance::preview_force_delete_installed_skills`）。名单包括当前 schema 可直接使用的锁里、来源不是 `unknown` 的条目，以及规范根上指向本地技能的链接。本地技能的原文件保留。没有锁条目的规范目录，以及 Agent 目录里用户自己的文件夹，不动。锁过新或损坏时，预览和删除都中止，确认框列不出名字。缺失或旧 schema 不按锁里的名字卸载，名单里至多出现那些本地链接。结果返回实际卸载、失败和保留的名字。

### 事务锁

- 跨进程技能事务锁是 `runtime/locks/skills/update.lock`（`skill_update::acquire_update_transaction_lock`）。同一线程可以重入，频道安装再调用通用安装器不会把自己卡死。其他进程排队。
- 安装、更新、频道升级和发布把网络放在这把锁外面：先拉取和校验，持锁后重新核对锁条目（来源、ref、路径）或本地内容没有变，才做暂存交换和写锁。发布的本地预检用 `try_acquire_update_transaction_lock` 限时等待；clone、pull、push 在释放之后。频道精确 Release 的拉取和 hash 校验同样在锁外。失败直接返回，不改规范副本。
- 存储修复和纳管的 `apply` 为整份计划持有这把锁。修复里按锁重装，因此会在持锁期间拉取。这不是第二把锁。`--dry-run` 和纳管预览走同一组检查，不写技能内容。

### 更新、本地修改与自动更新

- 检测在 `ss-skills::update_check`（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线)、[D-095](../../decisions.md#d-095更新检测按-tree-逐层解析安装基线保护自动更新频道升级由类型化授权与本地备份承担)）。按锁的 `sourceUrl+ref` 分组；`ref` 为空时比较远端 `HEAD`。`github.com` 先把 ref 解析成 commit，再取该 commit 的根 `tree.sha`。commit SHA 不是 tree SHA，见 [错误记录](../../errors.md)。嵌套 `skillPath` 逐层读父 tree，同一仓库的多条路径共享已经读过的 tree。只有父 tree 已经读到、并且确实没有该目录，才是「上游已移除」。API 失败或非 GitHub 来源走同一次 Git session 里的浅克隆（`git rev-parse HEAD:<path>`），私有仓库沿用这份认证。GitHub 返回限流（剩余额度为 0，或 429；没有 reset 头时按一小时）时，把截止时间写入 `state/skills/github_api_cooldown.json`，截止前直接走克隆。分组检查有固定并发上限。`local`、`bundle` 和 `unknown` 不参与检查，更新时报告为无上游可更新。
- 应用时，同一来源只拉取一次 checkout，再逐个在事务锁内重核，然后覆盖式重装。写回的仍是锁里的目录名。上游把 frontmatter `name` 换成另一个落盘名时不安装，结果为 `IdentityChanged`，不会出现第二个目录。界面提示「上游已改名为 X，原技能保持不变；如需跟随请安装 X 并卸载原技能」。上游已移除不自动删除。
- `install_baseline` 在每次安装或更新成功后记录规范目录的内容 hash，卸载时删除。它只回答相对上次成功安装有没有改过，不进入安装锁。手动更新仍覆盖；本地已修改时，技能卡把按钮标成「更新（覆盖本地修改）」。自动模式在内容与基线不一致，或基线缺失时跳过，并标成「本地已修改」。上游改名也不自动跟随。频道托管技能不走这条自动更新，改由订阅 baseline 和频道升级处理。
- 自动更新的偏好是 `config/skill_updates.json`（[D-093](../../decisions.md#d-093通用技能自动更新是显式开关偏好归-core-config唤醒归-ss-app)）。`auto_update` 默认关闭（手动）。`interval_minutes` 是自动检查间隔，只能是 15、30、60、360、1440，缺省或档位外的值按 60（1 小时）。Settings「更新模式」开关打开是手动，关闭是自动；自动时可选上述间隔。GUI 存活期间由 `ss-app::skill_wake` 按该间隔唤醒，调用 `update::auto_update_locked_skills`。检查与手动相同；应用前先按安装基线滤掉已改和已改名的技能，剩下的才走手动那条覆盖安装。关闭进程即停止。

### 频道升级

- 频道安装、升级和回滚携带只能由频道模块按 repository ID 构造的 `ChannelInstallAuthority`（[D-095](../../decisions.md#d-095更新检测按-tree-逐层解析安装基线保护自动更新频道升级由类型化授权与本地备份承担)）。它代替通用 mutation gate 对该仓库的拒绝，但不能写另一个频道的技能。没有这个授权的入口仍被 gate 挡住。
- 升级在锁外完成精确 Release 的拉取和内容校验。替换前把当前规范目录留成隐藏的 `.skillstar-retain-*`，并记入升级回执。失败或补偿回滚只把该副本换回，不再拉取上一个 Release。升级结果持久化之后才删除副本。进程停在这两步之间会留下该隐藏目录，列表忽略它。

### 健康扫描与修复

- `ss-skills::health` 是存储医生（[D-096](../../decisions.md#d-096存储健康检查只修复能证明所有权的条目)）。`scan` 只读，覆盖规范根、安装锁、已启用且不直接读规范根的 Agent 目录及其镜像，以及旧 hub。它报告缺锁、缺目录、缺 `SKILL.md`、断链、外来链接、自指链接、无标记同名目录、过期复制、镜像漂移、暂存残留、迁移残留，以及锁过新、损坏或过旧。
- `plan` 不删除没有所有权证明的内容。无标记同名目录、外来链接、缺 `SKILL.md` 的目录、部署后被改过的副本，以及过新或损坏的锁，都只报告。未改过的自有复制可以刷新。自有断链在规范副本还在时重连；规范副本已经不在时删掉链接。按锁能重新取回的缺失技能会重装，不能重装的锁条目可以剪掉。目标已经不在时，`backup` / `remove` / `retain` 残留还原回原路径，不先删除；目标还在时 stage 可以删，backup / retain 只在能证明替换已提交时才删（订阅或锁仍指向旧发布时 retain 保留）。频道拥有的规范根问题留给频道，不在这里改。旧 schema 的锁留给下一次成功写入去重写；doctor 不会为此把锁清空。
- `apply` 持事务锁，每步重查前置条件，重复执行是安全的。`skillstar doctor` 只报告，并列出可纳管项。`doctor --fix` 执行计划，`--fix --dry-run` 不写盘。设置存储页的修复按钮走同一计划。这两处都不收养 Agent 目录，也不收编规范根里没有锁条目的文件夹。

### 本机 Agent 纳管

- 纳管是另一条计划：`ss-skills::local_skill::intake`（[D-097](../../decisions.md#d-097本机-agent-已装技能经可预览的纳管计划进入-skillstar)）。它只读扫描每个有全局技能目录的 Agent，包括未在设置里启用的，并跳过直接读规范根的 Agent。`doctor --fix` 不调用它。
- 含 `SKILL.md` 的目录分成几类。已经是受管部署则跳过。与规范副本内容一致、且没有排除项，则换成指向规范副本的相对链接，不搬文件。规范根没有这个名字，则用受限复制放进 local，规范根上的条目以绝对链接指向 local，锁记为 `local/<agent>`，再把 Agent 目录换成指向该规范根条目的相对链接。同名内容不同（含比规范副本多出的文件）、带排除项、外来链接、名字已被锁或目录占用、频道已经占用该名字，都只报告。锁过新、损坏或过旧时跳过收养，不改写锁。
- `apply` 持事务锁，每步重查。提交前再比一次备份和已验证内容；多出来或不同的文件则还原并返回冲突，不删备份。失败只还原刚改的那一步。第二次执行没有可写步骤。`skillstar doctor` 的报告列出这些项。`doctor --adopt` 预览，不写技能内容；`doctor --adopt --apply` 才执行。设置存储页在健康行下列出可纳管项，预览和纳管是独立按钮。`repair_installations` 只是这条计划的显式入口。存储维护的 `repair_skills` 只调用它；断链清理留在 `clean_broken_skills`。它不再把用户目录改名搬进 local，也不再给 Agent 目录写绝对链接。

## GitHub 身份与共享频道认证

- 第一版只连接 `github.com`，使用已注册 SkillStar GitHub App 的设备授权流，不要求用户粘贴 PAT，也不复用用户全局 `gh` 登录。
- GitHub App Client ID 解析顺序：进程环境变量 `SKILLSTAR_GITHUB_APP_CLIENT_ID` → 编译期嵌入 → 从当前工作目录或 crate 源码目录向上查找仓库根 `.env`。官方 Release 在编译 `skillstar` 时嵌入；本地 `cargo run -p skillstar` 把该公开值写入仓库根 `.env` 即可，不必 `export`。缺失时登录动作明确不可用。
- `ss-skills::github_auth` 提供公开认证 facade；GitHub gateway、凭据仓库和时钟是可替换接缝。生产凭据写入 `SKILLSTAR_DATA_DIR/secrets/github/auth.json`：AES-256-GCM 加密 JSON（schema v2），首次创建和更新保持 Unix `0600`。密钥由本机 `machine_uid` 派生，**不访问系统钥匙串**，因此应用启动不会弹出钥匙串密码框。schema v1 明文文件在下次读取时就地改写成 v2。生产 gateway 的所有请求必须通过 `probe_http_client`。
- 设备授权的公开状态只包含用户码、GitHub 验证地址、轮询间隔和到期时间。device code、access token、refresh token 不得进入 IPC DTO、日志、错误、普通配置或 Git remote URL。
- 登录入口在侧边栏底部，打开同一设备授权面板。关闭面板**不取消**进行中的授权——用户要切到浏览器粘贴设备码；侧栏在等待期间显示「等待授权」。显式「取消」才清除进程内待处理设备授权。登出还会清除本地凭据文件和缓存身份。
- 界面必须说明凭据是本机加密 JSON，**不是**系统钥匙串；不要在登录文案里写「钥匙串 / credential store」。
- 登录说明以产品能力为主（私有技能、发布、共享频道），GitHub App 的 `Administration: write` / `Contents: write` 作为脚注。不请求 `Workflows: write`；有效操作权限仍受当前 GitHub 用户权限限制。
- 登录状态按 GitHub 返回的 `expires_in` / `refresh_token_expires_in` 元数据计算，不硬编码 token 寿命。显式刷新会轮换本地凭据文件并重新读取当前用户；过期且无法刷新的状态要求重新登录。
- GitHub App 由仓库所有者安装到明确选择的仓库。
- 已登录身份也是私有 `github.com` 仓库扫描、安装、更新检查、升级和**技能发布**的唯一 Git 认证来源；这些动作不依赖全局 `gh` 登录、Git credential helper 或预先改写过的 remote。发布不再调用 `gh` CLI：仓库列举、`skills/` 目录探查和建仓走 App 凭据的 GitHub REST（统一经 `probe_http_client`），clone/pull/push 走同一 operation session。`gh` 只剩 Settings 的环境检查一处用途。
- 每次远程 Git 操作创建独立 session。access token 只通过该子进程继承的临时 askpass 环境提供，操作结束即不可见；token 不得进入 remote URL、持久 Git config、命令参数、普通配置、IPC DTO、进度事件、错误或日志。所有 Git 子进程强制非交互，取消时终止当前子进程，进度只公开 session、阶段和无敏感信息的仓库标识。
- 私有认证只发送给规范化后的 `https://github.com/` 远端。带认证的操作不经过 GitHub 镜像，避免向第三方转发凭据；仍读取 SkillStar 当前代理设置并通过进程环境临时应用。公开仓库沿用无凭据路径，并同样不得弹出终端或系统凭据提示。
- 公开仓库的匿名拉取按 mirror 候选链执行：`candidate_mirror_urls()` 返回用户在 Settings 中排的加速源顺序（`order`，首位是选中源，包含 custom 行；去重、规范化），熔断中的源被跳过但其余顺序不变；transport/ops 对每个候选逐个尝试（每次独立 git 子进程），全部候选失败才回退直连 GitHub；非 GitHub/https 远端与带凭据操作不应用 mirror 重写。
- Git 失败按可行动状态区分：未登录、token 已过期、当前用户无仓库权限、GitHub App 未安装/无该仓库授权、网络/代理失败、用户取消。已安装 Skill 在认证或网络失败时保持不变，重试复用同一 Skills 域入口。

## 安装与更新

- Git/local 安装保留 vercel-labs/skills 的安装与锁语义（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线)）：解析来源、获取 checkout、发现 `SKILL.md`，再按[生命周期](#生命周期)落成规范副本并写锁。GUI、CLI、轮播、batch、卡组补装都进同一个 facade。
- 普通仓库扫描与添加按需获取（[D-090](../../decisions.md#d-090技能扫描与添加按需获取内容)）：远端先做浅层 partial clone，只检出 `SKILL.md` 和发现所需的插件清单；带 subpath 的来源只读取该子树的技能清单，同时保留根插件声明。选中安装只检出目标目录的完整内容。显式全深度只扩大清单搜索范围，不下载整仓正文。Git ref/SHA、质量门禁、去重和代理/认证/取消规则不变。本地目录原地读取；共享频道完整快照仍使用完整内容入口。远端不支持 blob 过滤时 Git 会回退传输全部对象，但工作树仍按范围检出；根目录本身作为技能安装时需要其完整内容；所选目录含符号链接时回退完整 checkout，以保留跨目录链接的复制语义。
- 导入缓存（[D-092](../../decisions.md#d-092导入复用本地-git-缓存并锁定预览提交)）：扫描、安装及跨重启重试共用按仓库 URL + ref 隔离的持久 Git 缓存，subpath 不另建仓库。已下载对象复用，安装只补取缺少的内容。普通扫描和全深度重扫读取缓存 revision，不自动探测上游；扩大扫描范围可能补取该 revision 的清单。导入框标明本地缓存与获取时间，显式「刷新上游」才增量 fetch。预览携带 commit，安装匹配该 commit；缓存被清理或提交不可用时要求重扫，不能悄悄改装最新版。并发导入按仓库/ref 加可取消的进程锁；设置中的清理缓存跳过正在使用的条目，canonical 副本不依赖缓存。缓存不会自动过期，磁盘占用由设置中的缓存清理管理。
- canonical 目录在生产环境是 `~/.skillstar/data/skills/installed`。`SKILLSTAR_DATA_DIR` 会连同数据根一起把它搬走。锁的格式、兼容和损坏时拒绝写入见[生命周期](#生命周期)。
- 发现与去重对齐 vercel：仓库根有合法 `SKILL.md` 时普通模式只返回根技能；否则按优先目录顺序（`skills/` 及其 curated/experimental/system 子目录、各 Agent 容器目录、plugin manifest 声明目录）最多 3 层扫描，含 SKILL.md 的目录遮蔽其下内容；同名去重是**优先顺序先见者胜**（`skills/foo` 压过 `.claude/skills/foo`），不再有选副本排名表。全深度扫描为全递归（跳过构建产物/依赖/测试夹具目录），`--full-depth` 请求时根技能不再遮蔽嵌套内容。
- 导入框提示支持技能子目录。来源步在地址框下方放「扫描」（普通深度）和「全深度扫描」；「选择 .ags / .agd 文件」与「从本地文件夹收编」同一行。最近的仓库超过四条时，在列表里用滚轮翻阅。全深度重扫保留上次输入的 ref、subpath 和技能筛选，不退回仓库默认分支或扩大到子目录外。
- CLI `install/add --refresh` 在扫描前增量更新来源缓存，兼容 `--list` 和 `--preview`；未指定时复用缓存。预览和列举可以写入导入缓存，但不改安装内容或锁。
- 安装与扫描必须走同一 frontmatter 质量判定（`ss-skills::validation`，对齐 vercel：`name` 与 `description` 必须是字符串）：缺失或非字符串、`name` 超 64 字符、frontmatter 缺失或 YAML 损坏 → 不可安装，扫描预览逐项显示原因、禁止选择阻塞项；`description` 超 1024 字符为咨询级警告仍可安装。`DiscoveredSkill.frontmatter_issues` 把稳定 issue code 传给界面，`installable` 投影同一判定；界面不得重建阻塞规则。
- 技能 id 使用 frontmatter `name`；目录名仅作展示回退（无 `name` 的技能不可安装）。落盘名、同批重名拒绝，以及门禁、去重、「已安装」判定、部署和锁共用这个名字，见[生命周期](#生命周期)。
- 安装仍是同名覆盖，但写入只走[生命周期](#生命周期)里的暂存交换。更新检测、身份变化、本地修改和自动跳过也以那一节为准。上游已移除的出口是卸载或转为本地副本，没有自动删除。
- 已安装列表先从本地快照返回（canonical 目录 + 锁 + 磁盘 Agent 链接归因），远程 update check 在有界后台任务中执行。列表读取路径不写磁盘：`list_installed_skills` 不调用 `local_skill::reconcile_hub_symlinks`。该函数是 `pub fn`，进程启动时由 `ss-app::bootstrap::prepare_process` 调用一次，项目导入也会调用。存储健康、修复和本机纳管见[生命周期](#生命周期)；`repair_installations` 不是列表读取这条路径。要修旧 hub 链接时直接调用 `reconcile_hub_symlinks`。
- 安装全程向 `skillstar://git-progress` 发送阶段事件（`stage`：resolving/fetching/discovering/materializing/deploying，additive 字段，旧监听不受影响）；`install_skill` 接受可选 `sessionId`。GPUI 导入对话框没有这条事件总线，扫描和安装改用同一个 session 的 progress sink，在进程内把 preparing / running / discovering / materializing / cancelled 画到加载文案上。
- GUI 新安装（`install_skill`、`install_from_scan`、市场安装、分享码、bundle 导入、卡片上不指定 Agent 的安装）只写入规范副本，不链接到任何 Agent。轮播点某个 harness 图标时把 `agentId` 传给 facade，仅部署到该 Agent。批量「链接到智能体」和卡组的「部署到全部已启用 Agent」是用户显式动作。全局目录等于或位于规范根之内的 Agent 由规范根直接提供，部署与取消链接都跳过，见[生命周期](#生命周期)。卡片轮播把它们显示为已链接，点击时提示「由规范根直接提供」。自定义 Agent 不允许把全局目录设为规范根或其子目录；已经保存的不合规定义在加载时跳过并写日志，仍留在偏好里。`batch_deploy_skills_to_agents` 按解析后的物理目录去重。`install_skill` 返回的 Skill 必须用 `installed_skill::agent_links_for` 重新读盘。没有 `git_url` 时回退 `onToggleAgent(..., true)`。一次安装只让该图标 pending，不得锁整行。
- repo scan 的 `ScanResult` 包含来源规格、预览 commit 和缓存状态；界面不重拼 URL，安装将整个预览交还 facade。仓库声明 Claude 插件且带 `hooks`/`agents` 时 `ScanResult.plugin` 非空，ImportModal 与 CLI 打印同一条提示；SkillStar 只装 Skills。
- Codex 包内技能在 `.agents`（不是 `.codex/skills`）；Antigravity 在 `.agent`。发现按目录实际内容处理，不做 basename 特判。
- CLI `install` 与 `add` 是同一命令，来源解析接受这些形式：`owner/repo`、`owner/repo/path`、`owner/repo@skill`、GitHub/GitLab tree URL、HTTPS/SSH Git URL、本地 `.ags`/`.agd` 和包含 `SKILL.md` 的本地目录。tree URL 的 ref 与 subpath 在克隆/扫描阶段生效。多技能来源在交互模式中选择；`-y` 未显式指定时装全部；`--skill '*'`/`--agent '*'` 展开全部；`--all` 等价两者加 `-y`；`--copy` 强制复制部署；`-g/--global` 全局（当前唯一 scope），`-s/--skill`、`-a/--agent` 过滤。
- 卡组/项目补装携带明确 Skill identity 时 fail-closed：仓库扫描成功但不再包含该 identity（包括只发现一个不同 identity）时不得整仓回退安装，错误列出缺失名称。卡组进度表示已处理数量而非成功数量。
- CLI 未显式指定 Agent 时只使用 Settings 手动启用的 Agent；`-y` 下没有已启用 Agent 直接报错；显式 `--agent` 与 `--all` 优先。
- 默认部署为 link-first；`--copy` 必须真实强制目录复制。
- 卸载、频道移除回滚，以及设置里「删除已安装技能」会列出的名字，见[生命周期](#生命周期)。Agent 与项目侧只删除能证明属于 SkillStar 的部署。「删除应用配置」只删白名单里的偏好与可重建状态（AI/代理/镜像/更新偏好、更新调度与检测投影、镜像健康、巡检状态、仓库历史）；Agent profile、卡组、项目注册、团队数据、SSH 主机与 host key、OAuth 客户端和频道归属记录都保留，新增文件默认不在白名单内。清理缓存只处理 SkillStar 自己的缓存目录。
- 与 `vercel-labs/skills` 兼容的 Agent 共用项目级 `.agents/skills`（项目域仍由 projects 模块拥有 manifest 语义）。

- 共享频道是绑定到 GitHub 组织专用私有仓库的版本化描述符；数字 `repository_id` 是稳定远程键，`owner`、`name`、HTTPS URL 仅是可变路由元数据。个人账户、公开仓库和非 `github.com` 主机不得绑定。
- 共享频道创建向导只展示当前 GitHub 身份所属的组织，并在提交前说明需要组织仓库 `Administration: write`、`Contents: write`，以及 GitHub App 对所选仓库的完整内容边界。创建者必须具有 Admin；远程权限投影规则为 Admin→owner、Maintain/Write→publisher、Read→subscriber。
- 创建前先校验 SkillStar GitHub App 已安装到目标组织、安装范围为 selected repositories，且授予 `Administration: write` 与 `Contents: write`。仓库由该 App 的用户身份创建；GitHub 会把 App 创建的新仓库自动纳入其 selected-repository 安装范围，SkillStar 不调用 GitHub App 用户令牌不支持的安装范围写接口。
- 共享仓库创建成功后先原子写入非敏感本地登记，状态为 `awaiting_app_installation`，再只读校验 App 可访问该数字 repository ID；若 GitHub 授权尚未生效，用户在安装设置中选择仓库后按 ID 续接，不能重建或凭 owner/name 猜测身份。校验完成后状态变为 `active`，空频道详情显示角色和授权范围。
- 两阶段恢复从 pending descriptor 成功落盘后成立。GitHub 返回创建成功到首次本地落盘之间无法与本地磁盘组成原子事务；若此时进程终止或落盘失败，SkillStar 不猜测同名仓库身份、也不自动删除远端仓库，而是保留它供组织所有者在 GitHub 手动处理。
- 频道描述符与本地 registry 各自显式携带 schema version。registry 不保存 token、邀请秘密或 GitHub credential；所有 GitHub REST 请求复用统一代理客户端和当前登录身份。管理界面没有移植到 GPUI，不要恢复 `src/features/shared-channels/`。
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
- 订阅确认重新读取最新 Release 并要求 repository ID、organization ID、revision、tag 与 commit 仍和评审目标一致；随后从精确 commit 的独立校验克隆核对每个选中 Skill 的 content root 与完整内容 hash。这次拉取和核对不持技能事务锁，锁只包住随后的本地写入；安装回滚只还原本地目录，不再拉取。只有全部通过才调用频道批量安装（写 canonical 与安装锁）。只安装明确选中的 Skill，并记录固定 release target、选中集合、安装后的完整 baseline hash 与不含凭据的 Git URL/ref/skillPath provenance；Agent link/copy 与 Project copy 通过现有 reconciliation 刷新。
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
- 频道写入授权，以及升级失败时换回本地副本，见[生命周期](#生命周期)。用户选择的历史版本回滚仍要读取那个 Release，见上面的回滚条目。
- CLI：`skillstar channel list` 列出订阅；`channel check [<repository_id>] [--json]` 检查最新 Release（省略 ID 时检查全部）；`channel apply <repository_id>` 应用安全更新，本地有分歧的项需用 `--keep-local <name>`（保留为 `<name>.local`）或 `--discard-local <name>` 显式决定，未点名的分歧项保持阻塞；`channel rollback <repository_id> <skill> [--revision N] [--keep-local|--discard-local]` 回滚单个 Skill 并固定在该版本，省略 `--revision` 时列出可回滚的 Release。编排在 `ss-app::cli::channel`，复用与 GUI 相同的频道 facade。
- 安装基线、手动覆盖和自动跳过见[生命周期](#生命周期)。频道托管的 Skill 由频道自己的订阅 baseline 负责。
- 上游已不再包含某技能的 `skillPath` 时，该技能标记为「上游已移除」：chip 入口提供「卸载」与「转为本地副本」两个出口，没有自动删除，也没有更名迁移或后继判定。检查和更新都把它写入 `update_state`，重启后仍可见；是否移除最终以重查上游 tree 为准。上游改名（`IdentityChanged`）同样持久化并显示「上游已改名」，直到用户处理。
- 安装来源只有 `skill_lock` 里的那一把锁。过新或损坏时拒绝写入，见[生命周期](#生命周期)。
- 更新按锁条目逐项执行并汇总结果；单技能失败不阻断其他技能，失败名单带底层原因。

### 更新策略：自动与手动

- 谁可以自动更新、哪些技能会被跳过，见[生命周期](#生命周期)。Settings 的「更新模式」和检查频率只读写那一份偏好。
- 手动（默认，「更新模式」开关打开）：只有用户点技能卡或详情抽屉的「更新」、工具栏「更新」，或 CLI `skillstar update` 时才更新。检查不等于修改：My Skills 在列表加载后静默检查上游（同一进程内的最小间隔是页面自己的 `BACKGROUND_CHECK_INTERVAL`，不读自动更新的 `interval_minutes`），工具栏只有一个刷新按钮（悬停「刷新并检查更新」）：点击即手动检查并 toast 结果，结束后重读本地列表。静默检查和这次手动检查都只刷新徽标，不改任何技能。导入是下载图标，悬停「导入」。搜索框优先把宽度让到大约 160px。筛选分段内边距更紧，品牌图标大约五个同屏，再多才在胶囊里滚动，筛选画在搜索和操作之间，不再盖住它们。
- CLI `skillstar update` 先检查再应用：不带名字时只更新检查结果为可更新的技能，并列出上游已移除、上游已改名、频道托管与无上游来源的技能；`--check` 只检查不更新，`--dry-run` 打印将要更新的名单。指定名字时直接对该技能执行更新。只有更新失败影响退出码。
- 自动（「更新模式」开关关闭）由 `ss-app::skill_wake` 在 GUI 进程存活期间唤醒，间隔是偏好里的 `interval_minutes`。从手动拨到自动，或带着已开启的偏好启动应用时，下一次唤醒立即检查一次；拨回手动只阻止后续检查，不中止已开始的运行。只改频率不额外触发一次检查，下一次是否到期按新间隔和上次运行时间比较。调度时间戳落在未来（系统时钟回拨）时视为到期，立即运行一次。
- 调度状态只记 `state/skill_auto_update.json` 的上次运行时间；次数与结果只进日志，不落盘。关闭应用进程即停止，不承诺后台常驻。
- My Skills 每次进入都重新读取本地快照与更新投影，后台自动更新的结果无需额外刷新动作即可显示。

## Agent 注册、手动启用与项目检测

- `BUILTIN_AGENT_DEFS` 是内置 Agent 注册表；自定义 Agent 存储在 profiles 配置中。枚举和数量由代码测试锁定，文档不复制完整清单。
- 本机 Agent 注册表只描述 identity、图标以及 Global/Project 技能目标能力；列表读取不得探测 PATH、桌面应用、配置根或 skills 目录，也不得据此推断 Agent 是否存在。
- 所有内置与新建自定义 Agent 默认关闭。Settings 开关是本机 Agent 激活状态的唯一来源；只有用户显式启用的 profile 才是 Skill、Deck 与 Project 等本机 Agent rail 上的 target。GUI 的单项和批量 Global 部署在后端同样必须拒绝未启用 profile，不得因陈旧 UI/IPC 请求创建 `~/.agent/skills` 类目录；CLI 显式 `--agent` / `--all` 仍是用户的直接授权。关闭后不删除已部署内容；未启用的 profile 不出现在卡片轮播、详情抽屉的部署开关，以及批量选择条的「链接到智能体」菜单里，即使技能仍链接。这三处条目是同一集合：有全局技能目录，且 Settings 开关为开。
- Settings 开关写入成功后，同一轮把新的启用集投影到仍存活的 Skill、Deck 与 Project rail。这些页面不会自己作废构造时的 profile 快照；不得等用户刷新技能列表或重新进入页面，轮播和工具栏里的品牌 SVG 才出现或消失。
- Settings 的 Agent 列表始终把已启用项置于未启用项之前；两个分组内部保持注册表原有顺序。用户切换开关后列表立即按该规则重排，不另行持久化 UI 排序。
- Settings 按上述顺序默认只展示前 10 个 Agent；超过 10 个时在列表底部显示剩余数量，并由用户显式展开全部或收起回前 10 个。总数不超过 10 个时不渲染折叠控件。
- 冻结的 8 字段 `AgentProfile` 仍保留 `installed`，但只镜像手动 `enabled`，不再表达系统安装探测。新代码不得以 `installed` 作为可见性或默认值来源。它不再为 IPC 存在。
- 工具栏 Agent 筛选只列当前启用的 profile。品牌图标在胶囊里大约十个同屏，再多时鼠标滚轮按卡片底栏轮播的同一规则横向滚动。SSH 远端 discovery 属于用户显式连接后的远端目录扫描，不复用本机激活规则。
- Settings 的 Agent 行在“已链接技能明细尚未加载”时可用 `synced_count` 作为初始摘要；一旦 `get_agent_managed_skills_state` 返回，计数徽标和展开明细必须共同以其 `active_skill_names` 为准，空数组是有效的 `0`，不得回退到旧摘要。展开状态下即使计数为 `0` 也保留收起入口；收起后不展示零计数徽标。
- 已手动启用且支持 Global skills 的 Agent 行提供紧凑的「当前受管技能」开关。它**不枚举 Hub 安装列表，也不改变 profile 启用状态**：没有恢复 journal 时，后端先原子持久化该目录当时精确的 `active_skill_names`，再临时停用其中成功移除的项；有 journal 时，只尝试恢复其中仍缺失的名字。恢复绝不补齐后来安装、目录中从未存在或已经退出 Hub 的其他技能。失败和受保护的冲突继续留在 journal，供下次恢复重试；恢复成功、或该名字已被手动放回目录后，才从 journal 删除。Hub 中已不存在的恢复源同样保留为未完成项，绝不以其他 Hub 技能替代。
- 此暂停/恢复状态由 `profiles.toml` 按后端解析的物理 Global skills 目录持久化，而不是按 Agent id 持久化。共享目录的每个 Agent 行必须显示同一暂停状态和影响提示、共同禁用进行中操作，并在结束后一起刷新；它不把共享目录伪装成独立的 per-Agent ownership。
- `project_skills_rel` 允许多个 Agent 共享；兼容 open agent skills 规范的 Agent 使用 `.agents/skills`。空字符串仍表达 global-only；Windows 输入统一规范为 `/`。
- **Global 侧的 `global_skills_dir` 同样允许多个 Agent 解析到同一物理目录**，这不是异常配置而是生态约定。共享组由 `BUILTIN_AGENT_DEFS` 派生，不在文档手抄计数；判定必须按解析后的目录而非 agent id。自定义 Agent 的 `global_skills_dir` 在 `custom.rs` 的 `add()` 中只做一条窄校验：展开 `~` 后不得等于家目录本身或文件系统根，也不得等于或位于 canonical 根之内——「取消全部链接」会遍历该目录并删掉其中的符号链接，这两个值会把它变成一次家目录清扫。指向内置共享目录、不在 `$HOME` 下、末段不叫 `skills` 都仍然允许，那是生态约定而非错误配置。加载注册表时对已存自定义 Agent 再跑同一校验：不合规的不进入 profile 列表，定义仍留在配置里。
- 项目检测按路径聚合：唯一且存在的路径可作为项目导入候选；共享且存在的路径返回 `ambiguous_groups`，供 UI/调用方选择一个 manifest owner。它不反向激活 Settings profile。以下三点是**已知与实现不符**（见 D-024）：`detect_project_agents` 未校验项目是否已注册；`scan` 不去重（`.agents/skills` 的一个技能会按共用该路径的每个 profile 各产出一条；当时由已删除的前端补做，新界面不要自己重写这套去重）；`rebuild` 的 owner 是先到先得且不过滤启用状态，并抢在 disambiguation 之前落定，因此 `ambiguous_groups` 的选择弹窗在正常开项目流程里不会出现。真正按路径去重的只有 sync 的 `build_path_plans` 与增量安装的 `add_skills_to_project_with_mode`。
- Project registration 必须先于 scan/import/sync。检测、manifest 与部署逻辑均由 `ss-skills` facade 提供。

## 部署 reconciliation

- Agent 与项目路径的部署所有权见[生命周期](#生命周期)。下面只写 reconciliation 怎么使用这个判定。
- Project sync 同时增加选中技能、移除陈旧技能并清理空 Agent 目录；零技能 Agent 不保留 active 选择。`clear_project_symlinks` 与 `remove_skill_from_all_projects` 走 `remove_project_owned`：只删除有标记且 hash 一致的复制，或精确指向受管目录的链接；没有标记的项目目录只报告。仍不查 manifest（待 D-024 的 provenance 谓词收敛）。项目导入遇到 Hub 已有同名技能时，只有项目内文件夹与 Hub 内容相同才替换为链接，否则保留并告警。
- 部署能力阶梯是 symlink → junction → copy。`deploy_modes` **是当前生效的配置，不是遗留字段**：sync 读它决定 symlink/copy，增量安装会写它。stale copy 刷新必须按 copy 重建：`refresh_stale_copies` 进入重建分支时目标必然是 copy（symlink 与不存在的条目都已跳过），走 symlink-first 路径会把用户显式选择的 copy 静默降级，而 manifest 中的 `deploy_modes` 仍写着 copy。`projects::types` 因此只暴露带 mode 的 `deploy_skill_with_mode`，不提供无 mode 的 auto 变体。已知缺陷：不带 `--copy` 的 CLI 项目安装仍会把整条共享路径的 mode 覆写回 symlink。
- 全局 toggle/batch 与项目 deploy 使用同一能力阶梯。批处理执行全部项并返回累计失败，不在第一项失败时中止。
- 更新后的 `resync_existing_links` 只刷新 SkillStar 拥有的 link 和 copy；新部署先在 staging 路径建立，再原子替换。Agent 镜像同样先在旁边建好链接再 rename，不先删掉现有条目。镜像排除每个 Agent 的全局目录和 canonical 根，只替换或删除自己拥有的条目。
- 打开项目时，带标记的 copy 部署通过内容 hash 检测 stale（无标记的目录不刷新）；仅刷新仍被 manifest 选择的技能，不复活用户主动删除的条目。
- Agent 侧的 unlink 对 link、junction 和 copy 使用 `ownership::remove_owned`；项目侧使用 `ownership::remove_project_owned`。missing 视为幂等成功，不属于 SkillStar 的条目原样保留。

## 本地创作、Bundle 与 Share

- 本地创作位于 `~/.skillstar/data/skills/local/<name>`，通过 hub link 暴露。每个受管本地 Skill 在 `<local>/<name>/.skillstar/identity.json` 持有 UUID 身份；改名移动 sidecar，复制或外部 adopt 必须 mint 新 UUID，不得信任来源里的 sidecar。
- 从项目 Agent 目录导入的技能必须先采用到 local，再进入 hub；发布到 GitHub 后可以毕业为 repo-backed install，但只有 staged 安装与最终校验成功才提交新的 Git lock provenance，失败时同时恢复本地内容与发布前 lock 状态。
- 发布前置检查的三态含义是「发布所需的 `git` 是否可用 → SkillStar 是否有可用 GitHub App 身份 → 该身份的 login」，不再是 `gh` 的安装与登录状态。凭据有效但网络/限流导致读不到 login 时仍视为可发布，只是身份标签未知，不把连通性问题报成未登录。
- 发布目标仓库列举使用 `affiliation=owner,collaborator,organization_member` 分页拉取，因此组织仓库和被邀请为协作者的仓库都可以作为发布目标；此前的 `gh repo list <login>` 只能看到个人仓库。新建仓库以 `auto_init=false` 建在当前 GitHub 用户名下，首个 commit 由本地缓存推上去。
- 发布的本地预检（名字、变更策略、锁文件可写、来源解析和内容 hash）用 `try_acquire_update_transaction_lock` 最多等 5 秒；clone、pull、push 和建仓在锁释放之后进行，慢网络不占着技能事务。发布失败不留半成品：clone 失败删除半克隆缓存，新建仓库后的任何一步失败都删除该缓存目录（远端空仓库保留，由用户在 GitHub 处理）。发布 commit 始终携带 `SkillStar <skillstar@local>` committer 身份，不依赖机器上的全局 Git 配置。
- `.ags`/`.agd` 是带 manifest 和 checksum 的 tar.gz：单技能 `.ags` 的 manifest 是 `manifest.json`，deck `.agd` 是 `multi_manifest.json`，把一种文件喂给另一种 importer 会因 manifest 缺失而整体失败。接受任一格式的文件选择入口必须走 `skill_bundle::import_any_bundle` 按扩展名分流（`.agd` 的 multi importer 对只含 `manifest.json` 的包仍兜底单技能导入）；两个 importer 成功后都失效 installed-skill 缓存。
- `.agd` 导入成功后自动创建同名卡组：组名取文件 stem 并剥去导出附加的 `-bundle-<时间戳>` 后缀（`deck_name_from_bundle_path`），描述为「N 个技能」；建组失败（如重名）不改写导入成功的结果。GPUI 导入对话框的「选择 .ags / .agd 文件」与卡组工具栏的文件导入入口都遵循该分流与建组行为。
- Share code 安装由后端 `install_from_share_code` 统一执行“已安装 / git / embedded / skip”决策，导入对话框不复制循环。
- 本地目录采用由 `adopt_local_folder` 和标准 discovery pipeline 处理，采用时复制完整技能目录，不只有 manifest。落盘名、受限复制和暂存交换见[生命周期](#生命周期)。采用前经过 frontmatter 质量门禁，无效技能按项跳过并报告原因。CLI 本地目录安装复用同一 facade，不复制采用循环。
- 存储健康、修复计划，以及 `doctor --fix` 与设置里「修复」的范围，见[生命周期](#生命周期)。设置存储页显示问题数和可修复数；预览列出将执行的步骤和留下不动的原因。
- 本机 Agent 已装技能的纳管，以及 `doctor --adopt` 与 `--fix` 的差别，见[生命周期](#生命周期)。

## ACP 图文教程

行为契约见 Learning 域（已按 [D-053](../../decisions.md#d-053移除学习功能与-skillstar-learning) 删除）。Skills 只拥有只读快照与本地身份 sidecar：

- 教程分析对象是当前 Skill 的**整个有效内容目录**。`ss-skills::content` 递归枚举目录内的文件，排除不属于 Skill 内容的 `.git`、`.skillstar`、操作系统垃圾和编辑器临时文件，不跟随逃出 Skill 根目录的内部符号链接；确定性 SHA-256 同时覆盖相对路径、文件类型、Unix executable 状态和内容。
- Skill 文件是待分析的不可信资料。教程 ACP 会话必须以当前 Skill 的隔离 staging 快照为工作目录。
- 现有详情页“AI 图文教程”入口、command 名和 wire DTO 在迁移期间保持兼容；Learn UI、Guide/Progress 与 Draft 转换不在本文件。

## Patrol 与页面职责

- Patrol 每个 cycle 先有界并发预取唯一 repo（每仓库一个 git 子进程），再有界并发做本地检查；`interval_secs` 是 cycle 间隔。
- Patrol 状态存入 `~/.skillstar/state/patrol/status.json`。
- Ghost（已缓存仓库上游新技能）检测已随 [D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线) 移除：无持久仓库缓存后不再有「已缓存仓库」可探测，`check_new_repo_skills`、dismissal 与 `patrol://new-skills-detected` 事件一并删除；patrol 只负责订阅频道的到期检查。
- 托盘和“关闭窗口转为后台”没有移植。不要为了后台运行把它们加回来。
- My Skills 管理本地 hub，也组合 remote/cloud scope；scope 共享卡片数据形状和展示面，不伪造一个能力完全一致的数据接口。
- My Skills 本地 scope 的「来源」筛选除按 Hub/Local 类型与仓库过滤外，每个仓库来源行提供移除入口：确认后批量卸载该 `source` 下全部已安装技能（走既有 uninstall），并在当前筛选指向该来源时清空筛选。选择栏批量卸载、来源行移除和详情抽屉单卡卸载共用同一个确认框：标题写清数量或技能名，正文说明会从本机库删除并移除已同步链接，按钮是「取消」和「卸载」。位置与按钮规则见 [平台窗口](../platform/README.md)。
- 每个 GitHub 仓库来源行在移除按钮左侧提供「重新安装」入口：重新扫描当前仓库的全深度 Skill 清单，并只将这一仓库发现的全部 Skill 覆盖式重装（[D-081](../../decisions.md#d-081技能安装锁与更新整体同步-vercel-labsskills删除自研管线) 语义，同名即覆盖）；执行期间该行的重新安装按钮显示加载状态，不影响其他仓库。频道托管的 Skill 仍由 mutation gate 拒绝。
- 本地 scope 工具栏的来源筛选收成一个紧凑按钮，高度与左侧 Agent 筛选胶囊相同，只显示层叠图标、当前数量和下拉箭头；生效中的来源或仓库筛选使按钮高亮，悬停显示来源类型与完整仓库名。点击打开同一个浮层：顶部是一条来源分段（全部、Hub、本地），下方是仓库名单。仓库名保持一行，`owner` 用次要色，过长从末尾截断，悬停显示完整路径；每行保留重新安装和移除。「全部仓库」单独清除仓库筛选，切换本地时清除仓库筛选并隐藏仓库列表。没有本地技能、没有仓库、也没有生效中的来源或仓库筛选时，数量仍单独成 pill。
- 工具栏右侧的网格/列表切换改变卡片排列，不改变筛选。两个图标悬停分别是「网格」和「列表」。网格走 [界面约定](../frontend/README.md#技能卡) 的共用轨道；列表把每张卡拉成内容区的一整行，高度不变。只有一张卡时，网格保持轨道宽度，列表铺满这一行。本地列表按行虚拟化，只排视口内的行。向下滚动超过约 300px 后，卡片区右下角显示「回到顶部」，和市场列表同一个控件（`crates/ss-gpui/src/chrome/scroll_top.rs`）。切换网格/列表会回到列表顶部。
- 本地 scope 处理待更新的默认路径是独立主 CTA「更新」（与「待更新」筛选分离）：按钮只写这两个字，悬停说明数量且不受当前筛选影响。一点即更新 Hub 内全部已标记 `update_available` 的技能（不受当前筛选影响），名单以点击瞬间快照为准，无确认框；结束用既有汇总 toast。进行中工具栏只旋转图标、文案不改宽；这份名单里的技能卡右上角同时进入和单卡更新相同的「更新中」省略号。选择栏批量更新也让其中可更新的卡进入这个动画。单卡「更新」保留为次要入口。决策见 [Wayfinder: 更新全部成为默认更新路径](https://github.com/xxww0098/SkillStar/issues/16)。旁边「需处理 (N)」筛选块的 N 与 CTA 不同源：它算的是当前搜索/来源/仓库筛选之后仍需处理（更新或上游已移除）的数量，因此点开筛选一定能看到 N 张卡；其他筛选把需处理技能全部挡掉时 N 显示 0、空态改为说明「当前筛选范围外还有 K 个需要处理的技能」，而 CTA 仍按全局出现。筛选激活时列表上方显示一条状态横幅（「已筛选：仅显示需要处理的技能（N）」+「显示全部」），列表骤减不会被误读为技能丢失。
- 技能详情是工具栏下方的右侧列，宽度与市场详情列相同（`crates/ss-gpui/src/my_skills/detail_drawer.rs`）。打开时从卡片区宽度里减去列宽再算列数，最右一张卡的边框留在滚动区域里；再次点击同一张卡或点关闭会收起。打开的卡使用 [界面约定](../frontend/README.md#技能卡) 的选中面。列里每个事实只出现一次，并且用完整值：名称、完整描述、一条来源（有 HTTP 地址就作为可打开的完整链接；这条地址已经包含的 `owner/repo` 或作者不再另起一行）、人读的更新时间（本地时区、到分钟），以及上游移除、改名或本地修改的说明。不展示安装路径，也不在列里截一段 SKILL.md。智能体开关和「更新」「在文件夹中打开」「卸载」固定在列的底部。「卸载」悬停时底色、边框、文字和垃圾桶图标用大约 80ms 的弹簧走到悬停面，指针离开时从当前进度返回，见 [界面约定](../frontend/README.md#动画)。部署列表第一行是总开关，滑动开关与每一行相同：尚未全部链接时拨开，会把这一列里每个未链接的智能体都链接上；已经全部链接时拨关，会把已链接的全部取消。每一行左侧是该智能体的 16px 品牌图标。链接与否只由滑动开关表示，行内不写「已链接」或「未链接」。直接读取规范根的智能体没有可写部署，拨动后保持原样。某一项正在切换时，总开关不再另起一次。「查看 SKILL.md…」打开居中悬浮窗，按 Markdown 渲染该技能目录里的整份 `SKILL.md`（frontmatter 收成代码块）。读的是磁盘上的实时文件。点关闭、按 Escape，或点悬浮窗外，都会关掉它并回到详情列。设置里的「翻译」有两个开关，默认都关：描述、SKILL.md。打开「描述」后，进入我的技能或市场时，卡片和详情列上还不是译文语言的描述会自动翻译；打开「SKILL.md」后，查看 SKILL.md 时每个还不是译文语言的段落下方附上译文。译文语言默认简体中文，设置里可改成繁體中文、English、日本語或한국어。悬浮窗在正文还有可译段落时，在「SKILL.md」右侧放「翻译」：这一下只翻译当前这次打开，不写入设置开关。请求进行中按钮旁的语言图标转圈，直到这段译出来。译文显示后按钮改为「原文」，再点只藏起这一次。开关开着时进来就是译文，按钮直接是「原文」。正文已经是译文语言时不放按钮。两处都套用设置里的译文样式（示例卡片，点一张即选用，可以收起）。代码块不翻译。关掉开关后，下一次打开只显示原文；已经译过的仍留在本地缓存里，再点「翻译」直接用缓存。正文在悬浮窗内用滚轮翻动，不把卡片撑出窗口。入场滑动期间正文留空，滑动停后再排版这份 Markdown。设置里的「翻译」可选机翻或大模型：机翻走 Google，不需要密钥。大模型只接受账户里的 OpenCode Go、Ollama、Command Code 密钥。OpenCode Go 的补全请求带固定的 `x-opencode-session`，否则网关直接拒绝。先在账户页填写，设置里只列出已有密钥的账户并记住其中一个为默认，沿用该密钥。没改过模型时用该服务的推荐模型，OpenCode Go 是 `deepseek-v4.1-flash`。点「拉取」向该账户要模型列表，进行中按钮旁的刷新图标转圈；再点一个即换，并记住这次选择；换账户则回到新服务的推荐模型。设置页不手填地址或密钥，也不把账户密钥复制到翻译配置。译文落在可重建的本地缓存里，换引擎、地址或模型后按新范围再译。位置见 [存储布局](../../storage-layout.md)。进入批量选择的一次性快捷键提示 toast 从 `bottom-center` 弹出，不遮列底部的主操作按钮。
- 详情抽屉只保留一个当前选择：已安装技能按 canonical 身份读取最新列表数据，不能仅因同名而替换来源。更新完成只刷新技能数据，不得重新打开用户已关闭的抽屉，也不得覆盖期间切换的选择。
- 批量选择条是卡片面板底部居中的悬浮胶囊（不是顶栏下的横带）：勾选第一张卡时不再把网格往下挤。胶囊内依次是选中计数徽标、三态全选（未选/半选/全选）、分隔线、「链接到智能体」（菜单向上展开、点击外部关闭；指针在菜单或胶囊上时，被挡住的技能卡不进入悬停；条目与卡片轮播相同，每项左侧是该 Agent 的 16px 品牌图标，由 Settings 开关决定）、「更新」（标注所选里可更新的数量，0 项时禁用）、「卸载」与图标式「清除」。计数文案走 i18n 的 `selectionBar.selected`（中文「已选 N 个」）。
- 工具栏搜索是常驻内联输入框（`chrome` 的 `toolbar_search`）。点到框外，或框内没有菜单和补全时按 Escape，会失焦。技能页按 `my_skills/filters.rs` 即时过滤 name、description、localized_description、source 和 author。市场页用聚光灯弹出层（`marketplace/spotlight.rs`）在当前列表里点选技能名。当前壳没有全局命令面板。
- Projects 是 master-detail，必须对新增和删除做 reconciliation；Decks/SkillCards 负责组合、导入导出和进入 Projects 的预选流程。
- GitHub 导入扫描结果页的「快速打包」先安装所选技能，再打开新建卡组对话框。卡组名称默认取扫描来源 `owner/repo` 斜杠后面的仓库名（`owner/orca` → `orca`），用户可改。创建态下若该名称已存在，对话框按重名拦截，不得因为预填了同名就放行。
- Deck 的 Agent rail 由卡组自己的 `agent_links`（Agent id 集合）决定，不由成员 Skill 的链接状态推导：**新建、导入、分享码导入的卡组一律不点亮任何 Agent**，即使成员 Skill 已被安装期全局部署链接过去。复制卡组继承来源的 rail。点亮/熄灭仍逐个 Skill 执行链接与取消链接；全部 Skill 都失败时不改写 rail。卡组声明了某 Agent 但已安装成员并非全部实际链接（新增成员、单卡解绑）时显示 mixed 而非点亮，让漂移可见；再点一次补齐链接。缺少该字段的历史卡组由 `ss-app::skill_group_links` 一次性回填：成员中全部已安装 Skill 都链接到某 Agent 才算已点亮，回填结果落盘且不改 `updated_at`。

## 界面接缝

- 我的技能页在 `crates/ss-gpui/src/my_skills/`。库内卡片不画「已安装」——整页都已在库中；更新是例外动作。市场卡片保留 rank 与 stars，不画 Hot/Popular 类营销徽标。
- 批量更新只有一个所有者。页面不得各自发起更新循环：工具栏「更新」与选择栏批量更新都只调用一次 `GitSkillFacade::update_skills`，用一条 toast 汇总成功、失败原因、跳过（移除/改名/频道托管/无上游）、未刷新的 Agent 链接与 Project 副本；进行中的批量更新期间再次点击被忽略。更新完成后同一入口把新内容同步到引用这些技能的 Project。
- SSH 远端技能页没有移植。不要恢复 `my-skills/remote` 或浏览器 dev mock。
- `ScanResult` 就是来源规格本身（`#[serde(flatten)] spec: Source`），不是另一份 `(source, source_url)`。扫描后的 `ScanResult` 原样回传给 `install_from_scan` 的 `spec` 参数——界面不重新拼 URL，也不拆开 `source`/`source_url` 再传一遍；`git_ref`/`subpath` 通过它自动贯穿到安装。仓库声明 Claude 插件且带 `hooks`/`agents` 时，`ScanResult.plugin` 非空，导入对话框在技能列表上方渲染一行提示，CLI 的 `install`/`--list` 打印同一条提示；SkillStar 只装 Skills，不装插件的 hooks/agents（见非目标）。

## 验证

```bash
cargo test -p ss-skills
bash scripts/internal/check_file_size.sh
```

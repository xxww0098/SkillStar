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
- `skillstar-skills::skill_update::update_skill` 返回完整公开结果，command 不再从底层 update outcome 二次拼装 `Skill` DTO。
- 本地目录 adoption、share-code 安装和 deploy-status 检查同样由 Skills 域公开 use case 完成；command 只保留 blocking 调度与 `AppError` 适配。

## 安装与更新

- 已安装列表先从本地快照返回，远程 update check 在有界后台任务中执行。
- repo scan 默认 root-first；仓库根有合法 `SKILL.md` 时首先视作一个技能，也可显式启用全深度 discovery。
- 技能 id 优先使用 frontmatter `name`，再回退目录名；根技能可回退仓库名。
- CLI `install` 与 `add` 是同一命令，来源解析兼容 `npx skills add` 的常用形式：`owner/repo`、`owner/repo/path`、`owner/repo@skill`、GitHub/GitLab tree URL、HTTPS/SSH Git URL、本地 `.ags`/`.agd` 和包含 `SKILL.md` 的本地目录。tree URL 的 ref 与 subpath 必须在 clone/scan 阶段生效，不能把网页 URL 直接交给 Git。
- 多技能来源在交互模式中选择 Skill；`-y` 未显式指定 Skill 时安装发现到的全部 Skill。`--skill '*'` 只展开全部 Skill，`--agent '*'` 只展开全部目标 Agent；`--all` 等价于两者加 `-y`。
- 未显式指定 Agent 时，只使用用户已在 Settings 手动启用的 Agent；SkillStar 不探测本机是否安装了对应 binary、桌面应用或配置目录。`-y` 下没有已启用 Agent 时直接报错，不得回退到全部 Agent。显式 `--agent` 与显式 `--all` 始终优先。
- 交互安装在未显式给出 `--global`/`--project` 时选择 Project 或 Global scope；`-y` 默认 Project。Project 部署进入项目技能目录并登记 `skills-list.json`，Global 部署进入所选 Agent 的用户级技能目录；两者都以 SkillStar hub 为内部 canonical source。
- 默认部署为 link-first；多个目标时交互选择 symlink/copy，`--copy` 必须真实强制目录复制，不能只改变日志。相同目标目录只物化一次。
- 与 `vercel-labs/skills` 兼容的 Agent 共用项目级 `.agents/skills`。共享目录是 universal install surface；Agent 归属只用于 UI/manifest，不得重复部署或让一个 Agent 的移除误删另一个仍在使用的共享目录。
- update 使用 staged swap；刷新失败不得先删除用户现有可用 link/copy。失败按 Agent 聚合并显式返回。
- repo check/update 复用 `~/.skillstar/hub/repos/` cache，远程 HTTP/Git 遵循统一 proxy/mirror 规则。

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
- 从项目 Agent 目录导入的技能必须先采用到 local，再进入 hub；发布到 GitHub 后可以毕业为 repo-backed install。
- `.ags`/`.agd` 是带 manifest 和 checksum 的 tar.gz。
- Share code 安装由后端 `install_from_share_code` 统一执行“已安装 / git / embedded / skip”决策，前端 modal 不复制循环。
- 本地目录采用由 `adopt_local_folder` 和标准 discovery pipeline 处理。

## ACP 图文教程

- SKILL.md 翻译入口已移除。已安装 Skill 的详情页提供唯一的“AI 图文教程”入口，阅读器和编辑器只保留原文/编辑与摘要能力；教程只使用 Settings 中显式启用的 ACP Agent。
- 教程分析对象是当前 Skill 的**整个有效内容目录**，不是只把 `SKILL.md` 文本发给模型。`skillstar-skills::content` 递归枚举目录内的文件，排除不属于 Skill 内容的 `.git` 元数据，不跟随逃出 Skill 根目录的内部符号链接；确定性 SHA-256 同时覆盖相对路径、文件类型和内容。该文件清单随 prompt 提供，生成结果必须逐项给出覆盖说明。
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
- 本地 scope 工具栏把「来源」筛选与当前列表数量合成同一 pill：左侧为来源标签与下拉/清除，右侧为 `countText`（层叠图标 + 数量）；无来源筛选时数量仍单独成 pill（远端 scope 等同）。
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

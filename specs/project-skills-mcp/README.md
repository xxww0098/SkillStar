# 项目技能 MCP

状态：08 已落地。下一档是 09。最后更新：2026-09-22。

## Next Agent Prompt

你正在实现 SkillStar 的项目技能 MCP。不要从聊天记录恢复上下文，以本目录为准。

08 已完成。从 [slices/09-strict-enable.md](slices/09-strict-enable.md) 开始，做完一档再做下一档。不要并行改两档的行为。01 到 17 是默认阶梯。18 处于休眠：只有环境变量 `SKILLSTAR_LAYA_ONNX` 指向本机导出目录时才做，缺模型时不要开工，也不要把它算进 17 的完成条件。

已落地：01 stdio；02 绑定；03 写锁；04 owner；05 事实；06 已安装检索；07 部署计划；08 批准记录（只有 elicitation 和 skillstar 两种来源）。`rmcp` initialize 对 `2026-07-28` 回落 `2025-11-25`。Windows release 管道探针还没跑。工作树是 `feat/project-skills-mcp`。

全局决定已经写在下面，不要重开。切片里标成「可改」的才是实现自由。做完一档之前，运行该档写明的测试。文档和代码同一档一起改，不留「待补」。

结束本轮之前，更新本节的状态、下一档入口，并勾掉已完成的 TODO。

### TODO

- [x] 01 stdio 进程入口 — [slices/01-stdio-process.md](slices/01-stdio-process.md)
- [x] 02 项目绑定 — [slices/02-project-binding.md](slices/02-project-binding.md)
- [x] 03 项目写锁 — [slices/03-project-write-lock.md](slices/03-project-write-lock.md)
- [x] 04 共享路径 owner — [slices/04-shared-owner.md](slices/04-shared-owner.md)
- [x] 05 技能事实 — [slices/05-skill-facts.md](slices/05-skill-facts.md)
- [x] 06 已安装技能检索 — [slices/06-installed-search.md](slices/06-installed-search.md)
- [x] 07 部署计划 — [slices/07-deployment-plan.md](slices/07-deployment-plan.md)
- [x] 08 批准记录 — [slices/08-approval-record.md](slices/08-approval-record.md)
- [ ] 09 严格启用 — [slices/09-strict-enable.md](slices/09-strict-enable.md)
- [ ] 10 推荐编排 — [slices/10-recommend.md](slices/10-recommend.md)
- [ ] 11 查询编排 — [slices/11-get-project-skills.md](slices/11-get-project-skills.md)
- [ ] 12 应用编排 — [slices/12-apply.md](slices/12-apply.md)
- [ ] 13 三个 MCP 工具 — [slices/13-protocol-tools.md](slices/13-protocol-tools.md)
- [ ] 14 elicitation — [slices/14-elicitation.md](slices/14-elicitation.md)
- [ ] 15 CLI 批准 — [slices/15-cli-approve.md](slices/15-cli-approve.md)
- [ ] 16 桌面批准 — [slices/16-gui-approve.md](slices/16-gui-approve.md)
- [ ] 17 ort CPU — [slices/17-ort-cpu.md](slices/17-ort-cpu.md)
- [ ] 18 Laya 图（休眠）— [slices/18-laya-graph.md](slices/18-laya-graph.md)

## 目标

开发 Agent 通过本机 stdio MCP 调用 SkillStar：

```text
项目上下文 → 技能候选与部署计划 → 用户确认具体变更
→ 增量启用项目级技能 → 读回部署事实，并说明当前会话尚未验证加载
```

推荐、安装、项目启用、当前会话已加载是四件事。本方案做推荐、确认、启用和部署复核。安装留在现有 CLI。当前会话是否已加载技能，结果里固定为未验证。

可选的 Laya 重排只改变候选顺序。推理引擎是 `ort` 的 CPU Execution Provider，Windows、macOS、Linux 共用同一份 ONNX 和同一条 `Session` 构建路径。模型文件不在时，推荐仍按 BM25 返回。

## 人可以怎么看

打开 [visualizations/ladder.html](visualizations/ladder.html)。每一档的探针写在对应切片里，第一档是把 `initialize` 送进 `skillstar mcp serve --stdio`。

## 切片图

```text
01 stdio 进程
02 绑定 ──► 05 事实 ──► 11 查询
03 写锁 ───────────────► 09 严格启用
04 owner ──────────────► 09
06 检索 ──► 10 推荐 ──► 13 三个工具 ──► 14 elicitation
05 + 06 ─► 07 计划 ──► 12 应用 ──► 13
08 批准 ──► 12
08 + 07 ─► 15 CLI 批准 ──► 16 桌面批准
10 的重排接口 ──► 17 ort CPU ──► 18 Laya 图（休眠）
```

01、02、03、04、06、08 没有相互依赖，仍按编号做，避免两档同时改公共入口。

## 所有者

一个概念只有一个所有者。禁止平行的第二份。

| 概念 | 所有者 |
| --- | --- |
| stdio 进程规则（不进 GUI、stdout 只有协议、跳过 askpass 与 marketplace init） | `src-tauri/src/main.rs` 的 `mcp` 分支 + `skillstar_app::project_skills_mcp::serve` |
| 项目绑定 | `skillstar_skills::projects::binding` |
| 项目写锁 | `skillstar_skills::projects::write_lock` |
| 共享路径 owner | `skillstar_skills::projects::owner` |
| 项目技能事实 | `skillstar_skills::projects::facts` |
| 严格启用 | `skillstar_skills::projects::strict` |
| 已安装技能检索 | `skillstar_skills::team::recall::search_installed_skills` |
| 技能内容哈希 | 现有 `skillstar_skills::content::snapshot` |
| 目录链接 | 现有 `skillstar_core::infra::fs_ops::create_symlink` |
| 计划、回执 | `skillstar_app::project_skills_mcp::plan` |
| 批准记录 | `skillstar_app::project_skills_mcp::approval` |
| 推荐 / 查询 / 应用编排 | 同模块的 `recommend` / `inspect` / `apply` |
| 重排 | 同模块的 `ranker`。`PassthroughReranker` 是永久回退，不是以后要删的垫片 |
| 协议 DTO | 同模块的 `protocol`。禁止 `From<工具参数> for ApprovalRecord` |
| 外部 MCP 安装计划 | 保持 `skillstar_app::mcp`。本功能不往那里加类型 |

`rmcp` 与 `ort` 只加入 `skillstar-app`，用 `cargo add`，版本写进根 `Cargo.toml`。不新增 crate。

数据目录一律经 `skillstar_core::infra::paths`，`SKILLSTAR_DATA_DIR` 继续生效：

| 文件 | 路径 |
| --- | --- |
| 项目写锁 | `state/project-write.lock` |
| 计划 | `state/project-skill-plans/<plan_id>.json` |
| 批准 | `state/project-skill-approvals/<plan_id>.json` |
| 回执 | `state/project-skill-receipts/<idempotency_key>.json` |

这些文件不进用户项目树，也不进 `skills-list.json`。

## 已定行为

### 进程

`skillstar mcp serve --stdio` 必须被 `is_cli_subcommand` 认成 CLI。漏掉时 `src-tauri/src/main.rs` 会打开桌面窗口。

`argv[1] == "mcp"` 时，在 `handle_internal_askpass` 之前进入 serve。该函数在 `SKILLSTAR_GIT_ASKPASS_MODE=1` 时会 `println!` 并吞掉进程（`crates/skillstar-git/src/transport.rs`）。serve 自己调用 `install_global_policy` 和 `migrate_legacy_paths`。不调用 marketplace snapshot `initialize`。tracing 只写 stderr。stdout 只有换行分隔的 JSON-RPC。

Release Windows 使用 `windows_subsystem = "windows"`。不要改子系统，不要 `AllocConsole`。父进程接上的管道就是传输。

### 项目

Agent 传绝对路径。身份是 `std::fs::canonicalize` 的结果。没有 `prj_*`。

`observe` 不写 `projects.json`。已有条目按「仍存在的 path 再 canonicalize」匹配，不改旧字符串。两条活条目落到同一规范路径则拒绝。`register_project` 的字符串相等语义保持不变，宽松部署继续用它。

规范注册只发生在项目写锁内，且只由已批准、并写明「注册并部署到这个真实目录」的 apply 调用。新行的 `path` 用规范路径。

每个将写入的目标，已存在的祖先 canonicalize 之后必须仍在项目根内。`.agents` 指向项目外时，在 `create_dir_all` 之前拒绝。技能名继续走 `validate_skill_name`。`project_skills_rel` 只来自 Agent profile，不来自工具参数。

### 共享目录

不要把受影响的 Agent 写死成 cursor、codex、antigravity。以 `list_profiles()` 里相同的 `project_skills_rel` 为准。DeepSeek 的注册路径是 `.dsh/skills`，同时也会读 `.agents/skills`。确认文案要披露这种额外读者，部署仍只写本次选中 Agent 的那一个相对目录，不再部署一份到 `.dsh/skills`。

一条物理路径在清单里只有一个 owner。已有 owner 保持不变。没有 owner 时用本次选中的 Agent。禁止空 Agent 列表回退到 `.agents/skills`。

### 严格启用

不调用 `add_skills_to_project_with_mode`、`save_and_sync`、`full_sync`、`deploy_skill_with_mode`、`create_symlink_or_copy`。

预检失败则项目目录、清单、项目索引都不改。通过后只为本次新建链接。链接全部成功才合并清单。中途失败时删掉本次新建的链接，不改清单，不删用户原有目录。

只调用 `create_symlink`。Windows 在错误 1314 时，该函数会建同盘 junction。junction 算目录链接，`already_present` 认它。复制永远失败关闭，不作为回退。跨盘且没有符号链接权限时，整份计划失败。

目标已是 copy 模式则整份拒绝，不改模式。真实目录、指向别处的链接是冲突，不覆盖。指向同一 Hub 技能的链接是 `already_present`，不拆建。Hub 缺失则整份失败，不从市场安装。

### 计划与批准

只有调用方传入非空的显式 selection 才写计划。服务器不按分数填写 selection。一份计划只有一个 `agent_id`、一条物理路径，技能最多 8 个。

`plan_hash` 是 SHA-256，域分隔 `skillstar.project-skill-plan.v1\0`。输入只含：规范项目根、是否会新建项目索引行、排序后的技能名、每项 `content_hash` 与 `SNAPSHOT_HASH_VERSION`、物理相对路径、owner、受影响 Agent 的排序名单、每项操作（create 或 already）。不含分数、重排器名、过期时间、`plan_id`、批准来源。

内容哈希只用 `content::snapshot` 的 `content_hash`。它会顺带写 `state/snapshot-stats/<name>.json`，这是该所有者的现有缓存，不是第二套哈希。禁止 `snapshot_materialized`。只给显式选中的技能做 snapshot，不为整个 Hub 做。

TTL 15 分钟。测试注入时钟。

批准只有两个写入函数：`record_from_elicitation` 与 `record_from_skillstar`。没有 `user_confirmed`。工具参数不能反序列化成批准记录。同一计划已有另一来源的记录则拒绝。

客户端 `initialize` 声明了 form elicitation 时，协议层在调用领域 `apply` 之前发起 elicitation。这次调用里接受的结果才算数。事先放好的 SkillStar 批准文件不能代替它。客户端没有该能力时，协议层不发 elicitation，领域 apply 在没有 SkillStar 批准时返回 `approval_required` 且零写入。领域 `apply` 自己不发起 elicitation。

幂等键由调用方传入，1 到 64 个 `[A-Za-z0-9_-]`。同键同 `plan_hash` 返回原回执，不再碰链接。同键不同哈希则冲突，零写入。

### 结果

`get` 与 `apply` 共用 `inspect_project_skills`。`runtime_visibility` 只有 `unverified`。`next_action` 为 `load_skill`。路径是项目内相对路径，正斜杠。不返回技能正文，不发 `notifications/tools/list_changed` 充当技能刷新。不读项目源码。`constraints` 与 `focus_paths` 只回显，并可以拼进检索词，不是部署约束。`catalog_scope` 只接受 `installed`。

第一版只接受本机可 canonicalize 的绝对路径。不接受 SSH 主机。

### 重排

`SkillReranker` 只重排已有候选的顺序，不增删 id，不产生 selection，不写计划或批准。默认实现是 `PassthroughReranker`。

`ort` 关闭 default features，只保留编出 CPU Execution Provider 的最小 feature。不要同时启用 `cuda`、`coreml`、`directml`。PyTorch 不进 workspace。权重不进 git，不进安装包，运行时不下载。

## 被否决的做法

| 做法 | 为什么不采用 |
| --- | --- |
| 把新服务放进 `skillstar_models::mcp` 或 `skillstar_app::mcp` | 那里是外部 MCP 的 store、探测和市场安装计划 |
| 包装 `add_skills_to_project_with_mode` | 先写清单、缺失技能返回 `Ok(0)`、真实目录跳过但清单已记、拆掉已有链接、空 Agent 回退、覆盖 deploy mode |
| 复用 `state/skill-update.lock` | import 与技能更新会在持有它时再写项目；同线程再取会自锁。锁序只能是更新锁然后项目锁 |
| 每项目一把锁 | `projects.json` 和 `remove_skill_from_all_projects` 是全局的；三层锁增加死锁面 |
| 把已有 `ProjectEntry.path` 改写成规范路径 | 那是迁移。匹配在内存里 canonicalize，旧字符串保持不动 |
| 用 MCP Roots 或 `user_confirmed` 当权限 | Roots 不是授权；布尔值由模型填写 |
| 推荐阶段 `register_project` | 推荐不是写入 |
| 调用 `team::recall()` | 它把命中写进 `state/team.json` 的 `recall_events`，语料还含 learning |
| 一份计划里自动勾选高分技能 | 分数不是授权 |
| apply 里从市场补装、执行技能脚本、安装技能建议的 MCP | 那些是另一份批准 |
| 为每次推荐建 SkillGroup，或走 `save_and_sync` | 卡组是用户以后主动保存的；`save_and_sync` 会清空再重建 |
| GPU Execution Provider、应用内 PyTorch | 三平台没有同一个 GPU 后端；CPU 足够做可选重排 |
| 把 1.7GB 的英文 Laya 权重复制进仓库 | CI 不下载。中文 multilingual ONNX 目前没有现成文件 |

## 防火墙

- 不改 `add_skills_to_project_with_mode` 的返回语义，不改 `register_project` 的字符串匹配，不改 `crates/skillstar-usage/src/fetchers/oauth/cursor.rs`。
- 不把工具写进 `docs/features/mcp/README.md` 的类型说明。新行为写 `docs/features/project-skills-mcp/README.md`，并在 `Agents.md` 功能入口加一行链接。`docs/boundaries.md`、`docs/architecture.md`、`docs/decisions.md` 只加本功能的所有权、stdio 与锁。`README.md` 只加这两条 CLI。
- 测试把 `SKILLSTAR_DATA_DIR`、`HOME`，Windows 再加 `USERPROFILE`，指到临时目录。不写真实家目录。
- 无协议兼容垫片，无 `projects.json` 迁移，无第二套工具名。
- 01 到 17 在当前操作系统上可以继续。Windows release 管道探针没跑过之前，不要把传输标成三平台已验证。该探针失败时停在 01，改规格，不要改子系统。

## 已知未知

- rmcp 3.4 里读取 form elicitation 能力的具体方法名，以该版本文档为准。策略不能改：能力来自 `initialize`，不来自工具参数。
- `ort` 关闭 default features 之后，CPU 包对应的 feature 名以引入时的 ort 文档为准。禁止为了编过而打开硬件 EP。
- Windows release 的继承管道要在真机 release 二进制上看一次。macOS 上的 debug 探针不能代替它。
- Laya 英文图的打包格式以 [receptron/laya](https://github.com/receptron/laya) 的导出和加载代码为准。数值对照是 18，不是 17 的门。

## 来源

- MCP Rust SDK：<https://github.com/modelcontextprotocol/rust-sdk>（`rmcp` 3.4.0，规范 2026-07-28）
- stdio：<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports>
- elicitation：<https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation>
- ort features：<https://ort.pyke.io/setup/cargo-features>
- 英文 ONNX：<https://huggingface.co/receptron/laya-onnx>

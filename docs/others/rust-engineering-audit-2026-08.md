# 《Rust 大型项目开发宝典》对照审计 · 未落地发现与实测边界（2026-08）

状态：historical / 一次性审计快照
审计日期：2026-08-14
基准文档：`ozon-pod/docs/rust-engineering.md`（第一部结构 / 第二部测试 / 第三部构建 / 第四部依赖 / 第五部门禁 + 附录 B）
仓库快照：`main`，HEAD 在审计期间由 `d6ad988` 推进到 `8a82471`（见 §8.2）
测量环境：Apple Silicon 12 核 / 24 GiB，macOS darwin 25.6，rustc 1.97.1，仓库位于外置卷 `/Volumes/Acasis`

---

## 0. 这份文档是什么

这是一次对照审计留下的**证据档案**，不是待办清单。

四份只读审计报告 + 五份实现报告共 3838 行。其中**已落地的部分**已经进入 git 历史与 [decisions.md](../decisions.md)，按 SSOT 规则本文不复述（指针见 §1）。本文只保留两类内容：

1. **量过但没有落地的发现**——它们的价值在于"已经量过了"。重新量一次要几小时（模块 SCC、可见性普查、依赖图 BFS、`--timings` 逐单元差分），而重量出来的数字和这里是一样的。
2. **实测推翻宝典、或实测判定无收益因而明确不做的结论**（§6 / §7）——这是本次审计最贵的部分。它防止后人再花几小时把同一件事重新测一遍，或者照着宝典把已经验证过是负收益的东西加回来。

每条发现给出三样东西：**量化现状 / 机制 / 尚未测量的部分**。值不值得动由读者自己判断——本文不替读者做这个判断，也不暗示这些事都该做。

> **引用纪律**：本文的数字全部逐字取自当时的审计报告，未重新推导。源材料标了「未测量」的地方，本文保留这个标注——审计报告最有价值的品质就是诚实区分"测了"和"没测"。§8 给出可信度边界，§9 给出源材料之间的矛盾与口径差异，§10 记录 2026-08-14 对部分数字的复核结果。
>
> 结构事实以 [boundaries.md](../boundaries.md) 为准，运行拓扑以 [architecture.md](../architecture.md) 为准，行为契约以 `docs/features/` 为准。本文是快照，不是任何一类事实的 SSOT。

---

## 1. 已落地的部分去了哪里（指针，不复述）

读本文之前先看这张表，避免把已经做完的事当成缺口。

| 已落地 | 落点 |
| --- | --- |
| `[workspace.package]` 继承、38 处冗余 path dep `version`、`skillstar-core` 补 `rust-version`、marketplace 版本归一 | commit `d9d82e3` |
| `[workspace.lints.clippy]` 三条 deny（`todo` / `unimplemented` / `dbg_macro`）+ 13 个成员 `[lints] workspace = true` | commit `d9d82e3` |
| 13 个成员 `[lib] doctest = false`；`tempfile` 移入 skills 的 `[dev-dependencies]`；tauri dev-dep 统一 | commit `d9d82e3` |
| `[profile.release]` `lto = "fat"` → `"thin"`；新增 `[profile.release-fast]` | commit `d9d82e3` |
| 删 `src-tauri` 24 个零引用直接依赖；cargo-deny allowlist 与 license 元数据 | commit `d9d82e3` |
| GPL-3.0+ `html2md` → Apache-2.0 `htmd` | commit `55290b0` |
| CI 删掉冗余的 `cargo check --workspace` 步骤 | commit `3918695` |
| **dev profile / `build-override` / package 热路径清单 / `target/` 搬迁的"不做"决定与实测数字** | **[decisions.md D-032](../decisions.md)** |
| git hooks（pre-commit 5 s / pre-push 64 s）+ installer；cargo-deny 转 blocking | commit `178e577` |
| 新增 `check_no_orphan_modules.sh`、`check_dep_graph_doc.sh`；`check_file_size.sh` 测试文件第二档阈值（1500）；`rust-toolchain.toml` pin 1.97.1 | commit `178e577` |

**D-032 的边界**：dev profile 覆盖、`build-override`、package 热路径清单、`target/` 搬内置盘这四件事的**结论和主要数字都在 D-032 里**。§6.1 / §6.2 / §7.1 / §7.2 只留指针 + D-032 没有收录的机制证据，不重复它的结论。

---

## 2. 结构类未落地发现（第一部）

> 全部只测量了依赖图形态，**没有测量编译墙钟时间**。第一部审计全程禁跑 `cargo build/check/test`（并行 worker 占用 `target/` 锁）。任何"拆 crate 提速""缩短链提速"的说法在本仓库都**未经验证**——而第三部的实测（§7.6）反而说明热增量根本没有痛点。

### 2.1 `skillstar-skills` 的 13 模块 SCC，其中 9 条边是 re-export 假象

**量化现状。** 用 Tarjan SCC 跑顶层模块邻接表，同时给出「含测试」和「仅生产」两份（测试文件的跨模块引用会制造假环）：

| crate | 顶层模块 | 生产模块行数 | 最大 SCC | SCC 行数 | 占比 | 含测试口径 |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `skillstar-skills` | 27 | 18,723 | **13 模块** | **12,730** | **68.0%** | 14 模块 / 71.9% |
| `skillstar-marketplace` | 7 | 13,312 | 2 模块 | 8,595 | 64.6% | 同 |
| `skillstar-models` | 7 | 13,481 | 2 模块 | 5,098 | 37.8% | 45.7% |
| `skillstar-channels` | 3 | 13,174 | **无环** | — | **0%** | 2 模块 / 97.6%（**假环**） |

对照宝典反例「15 个顶层模块中 10 个处于同一个 SCC，覆盖 81.7%」——`skillstar-skills` 的 68.0% 与之同量级。

13 个模块：`content · deployment · git · installed_skill · local_skill · lockfile · projects · repo_link · repo_scanner · skill_install · skill_pack · skill_update · update_checker`。

**机制：`git` 模块的 9 条入边不是真耦合。** `crates/skillstar-skills/src/git/mod.rs` 的内容是 `pub use skillstar_git::*;` + `pub mod gh_manager;`。实测生产代码里的 `crate::git::*` 引用分布：

```
43  transport
18  ops
 0  gh_manager
```

**61 处引用 100% 指向 `skillstar-git` 的 re-export，0 处指向本地的 `gh_manager`。** 15 个生产文件把 `use skillstar_git::ops` 写成了 `use crate::git::ops`，凭空制造了 9 条模块内依赖边。

在邻接表上模拟切除这些边（不改仓库）：

| 步骤 | 最大 SCC | 行数 | 占比 | 脱环模块 |
| --- | ---: | ---: | ---: | --- |
| 现状 | 13 | 12,730 | 68.0% | — |
| ① `crate::git::{transport,ops}` → `skillstar_git::{transport,ops}` | **11** | 11,789 | **63.0%** | `git`、`repo_link` |
| ② 再消掉 `lockfile → content` 的 3 处引用 | **9** | 10,888 | **58.2%** | `lockfile`、`update_checker` |

② 的全部代价是 3 行：

```
crates/skillstar-skills/src/lockfile.rs:73   crate::content::validate_skill_name(&entry.name)
crates/skillstar-skills/src/lockfile.rs:103  crate::content::SNAPSHOT_HASH_VERSION
crates/skillstar-skills/src/lockfile.rs:175  crate::content::SNAPSHOT_HASH_VERSION
```

`validate_skill_name` 属于 `validation`（已经是出度 0 的叶子），`SNAPSHOT_HASH_VERSION` 是一个常量——正是宝典 1.8「把共享类型抽到第三个模块」的场景。

**尚未测量。** ①② 都只在邻接表上模拟过，**没有实际改代码，也没有测过编译墙钟**。①是纯机械替换（`git/mod.rs` 就是 `pub use skillstar_git::*;`，语义等价），②要动 3 行引用的归属。审计当时给出的判断是「先做零风险的 ①②，把 SCC 从 68% 压到 58%，然后再判断核心 SCC 值不值得动」——核心的 9 模块 / 10,888 行是真正的领域纠缠，需要设计工作而不是搬文件。

### 2.2 出度为 0、今天就能抽 crate 的模块清单

**量化现状**（`skillstar-skills`，8 个 / 2,595 行）：

| 模块 | 行数 | 入度 | 备注 |
| --- | ---: | ---: | --- |
| `source_resolver` | 524 | 3 | 已在 `lib.rs` `pub mod` 导出 |
| `validation` | 493 | 8 | SKILL.md frontmatter 校验，纯函数 |
| `update_state` | 392 | 1 | |
| `skill_group` | 360 | **0** | 入度也是 0——完全孤立，抽走零影响 |
| `update_api` | 262 | 2 | GitHub API 更新检测快速路径 |
| `plugin_manifest` | 207 | 1 | `.claude-plugin` 清单发现 |
| `hub_entry` | 182 | 4 | |
| `skill_mutation` | 175 | **11** | mutation-gate 策略接缝；入度最高的叶子 |

做完 §2.1 的 ①② 后变成 10 个 / 3,076 行，新增 `lockfile`（287，入度 10）和 `repo_link`（194，入度 6）。

其它 crate 的出度 0 模块：`marketplace` 的 `remote`(1,838, 入度 3) / `mcp_models`(1,186, 入度 2) / `models`(325) / `db`(25)；`models` 的 `latency`(498) / `provider_ref`(16)。

**机制与警告。** 宝典 1.11 / 附录 B #20 的警告在这里成立：这 8–10 个叶子如果一股脑抽成一个 `skillstar-skill-types` crate，它的入度会是 skills 内几乎所有模块，等于「环没了，全仓库重编还在」；而且它**不会缩小那个 9 模块 / 10,888 行的核心 SCC**。`skill_mutation`（入度 11）、`validation`（入度 8）合并进同一个新 crate 就是这个反模式的形状。

**尚未测量。** 抽 crate 的编译收益。第三部的实测（§7.6）给了一个相反方向的强证据：touch 入度最高的 `skillstar-core` 后全 workspace 重 check 只要 3.42 s，**本仓库没有可回收的热增量收益**。

### 2.3 两个真环（marketplace / models）

**`skillstar-marketplace`：`snapshot`(5,697) ↔ `mcp_snapshot`(2,898)，8,595 行 / 64.6%。**

反向边**全部集中在 `snapshot/migrations.rs` 一个文件**，4 处，且全是"建表 / 表名 / 列定义"：

```
migrations.rs:433,475  create_mcp_registry_tables(conn)
migrations.rs:571      MCP_SERVER_TABLES
migrations.rs:577      MCP_SERVER_COLUMNS_V13
```

机制：两个 snapshot 域共用同一个 SQLite 迁移序列，而迁移编排这个共享关注点被放在了 `snapshot` 里。破环形态明确（抽一个 `migrations` 顶层模块，两侧都依赖它）。**尚未测量**：改动的行为风险（迁移序列的顺序敏感）与编译收益都没测。

**`skillstar-models`：`providers`(1,936) ↔ `tool_sync`(3,162)，5,098 行 / 37.8%。**

双向都是真引用：`providers/crud.rs` 10 处 → `tool_sync`；`tool_sync` 下 7 个文件 → `providers`。这是"CRUD 写入后触发工具配置同步、同步时又要读 provider 定义"的经典双向耦合。占比 37.8%，是四个大 crate 里最健康的，**没有廉价破法**。

**对照组（做对了的）**：`skillstar-channels` 生产代码零模块环。含测试口径报出的 `policy ↔ shared_channels` 2 模块 SCC（97.6%）是假环——反向的 2 处引用都在 `channel_access_tests.rs`（第 199、276 行）。生产方向是单向的 `policy → shared_channels`（7 处），`policy` 只有 65 行，是宝典推荐的"注入式接缝"形态。**这条同时是方法学证据：不分开测生产与测试，SCC 分析会误报。**

### 2.4 可见性：305 / 920 的 `pub` 从未被 crate 外引用

**量化现状。** 全仓 crate 外可达的 `pub` 条目 **920 个**：

| 指标 | 数量 | 占比 |
| --- | ---: | ---: |
| (a) 模块外从未被引用 | **126** | 13.7% |
| (b) **crate 外从未被引用** | **305** | **33.2%** |

整体 `pub : pub(crate)` = **1,849 : 459 = 4.0 : 1**（宝典健康线 ≈ 1:1，宝典实测反例 8.3:1）。分布极不均匀：

| 已达标 | 比值 | | 另一个极端 | 比值 |
| --- | ---: | --- | --- | ---: |
| `skillstar-agents` | 0.6 : 1 | | `skillstar-core` | **44.0 : 1**（88 pub / 2 pub(crate)） |
| `skillstar-marketplace` | 0.9 : 1 | | `skillstar-git` | 39.5 : 1 |
| `skillstar-models` | 1.7 : 1 | | `skillstar-github-auth` | ∞（0 个 `pub(crate)`） |

逐 crate 的 (b) 比例：`skillstar-app` 48.3%（29/60）、`marketplace` 46.8%（44/94）、`models` 45.5%（20/44）、`sync` 42.2%（35/83）、`usage` 36.4%（51/140）、`github-auth` 34.4%（11/32）、`skills` 33.5%（84/251）。

**机制：最省力的修法是先改 `mod` 声明，而不是逐条目。** 正面证据在同一个仓库里：`skillstar-channels` 只在 `lib.rs` 暴露 3 个 `pub mod`，30 个模块里只有 7 个 crate 外可达，一次封住了 192 个 `pub` 里的 155 个；`marketplace` 44 个模块只有 14 个 crate 外可达。反面是 `skillstar-app/src/lib.rs` 把 10 个模块全部 `pub mod` 出去，而 `src-tauri` 实际只用到 `usage_switch::SwitchOutcome` 一个类型。

交叉点：`skillstar-core` 的 44:1 尤其值得注意——它是入度 11、下游覆盖 **97.6% workspace 行数**的枢纽。API 面越大，越容易因为无关改动触发那 97.6% 的重编。

**尚未测量（重要）。** 这个指标是**正则 + 整词匹配**得到的：会把注释、字符串、同名的其他条目都算成"被引用"，因此**系统性高估使用、低估过度暴露**——**305 / 920 是保守下界，真实值只会更高**。反过来它也不识别 trait impl 里的方法和宏生成的引用，极少数条目可能被误报为未使用；抽样人工复核了 5 个，5/5 吻合，但样本小。

**`unreachable_pub` 的真实基数只读不可测**，必须先 `warn` 记基数再考虑 `deny`（第五部给出的棘轮路线也把它排在最后一步）。截至 2026-08-14，`[workspace.lints.rust]` 段不存在，`unreachable_pub` 未启用——已落地的只有 clippy 侧那三条实测 0 违规的 deny。

### 2.5 `GitAuthMaterial` 下沉 core：可切断 `github-auth → git`

**量化现状。** `skillstar-github-auth` 生产代码对 `skillstar-git` 的**全部依赖只有 1 行**：

```
crates/skillstar-github-auth/src/lib.rs:13   use skillstar_git::transport::GitAuthMaterial;
```

5 处调用（`git_auth_material()` 的返回值构造）。`GitAuthMaterial` 是 `skillstar-git/src/transport.rs:22–105` 的一个约 80 行纯值类型（内部一个 4 变体 enum + `Arc<str>`），**没有任何 git 相关依赖**。

把它下沉到 `skillstar-core`（github-auth 和 git 都已经依赖 core）后：

| | 现在 | 之后 |
| --- | --- | --- |
| crate DAG 最长链 | **7 crate（深度 6）** | **6 crate（深度 5）** |
| `github-auth` 层级 | L2 | L1 |
| `skills` 层级 | L3 | L2 |
| L1 层宽 | 5 | 6 |

当前层宽分布：`L0:2, L1:5, L2:1, L3:1, L4:2, L5:1, L6:1`。L0→L1 已经是宝典推荐的"vocab → 并行功能"形态（2 个词汇 crate 扇出 5 个功能 crate）；从 L2 开始变成 `github-auth → skills → channels → app → skillstar` 一条宽度只有 1–2 的 5 层链。

**尚未测量（诚实边界）。** `skillstar-git` 只有 2,517 行，把它从关键路径上摘掉一环，**墙钟收益很可能很小，且完全没测过**。真正吃掉墙钟的是 L3–L6 那 60,600 行串联（skills 21,059 + channels 19,734 + app 9,948 + skillstar 9,859，占 workspace 的 49%），而那是领域固有的顺序（channels 确实建立在 skills 之上）。改动面：会动 `skillstar-git` 的公共 API，3 个下游要改 `use`。

补充一条第三部的实测佐证：冷 `cargo check` 的最后 12.5 秒平均并行度只有 **2.1**（`skillstar-*` 串行尾），这条链确实在关键路径上——但它的总量只有 12.5 s，而且其中我们自己的 13 个编译单元合计只有 **15.7 unit-seconds，占全部 588.2 unit-seconds 的 2.7%**（§6.1）。

### 2.6 其余结构发现（较小，均未落地）

| 发现 | 量化 | 机制 / 边界 |
| --- | --- | --- |
| `src-tauri` 94.1% 的行对集成测试不可达 | 9,278 / 9,862 行；crate 外可达 `pub` 只有 2 个，367 个被私有 `mod` 封住；无 `tests/` 目录 | **审计明确反对**为了可测性把 `mod commands`/`mod core` 改成 `pub`——那会把 Tauri 框架 DTO、State、事件适配变成公共 API。正确方向是把非胶水部分下沉到域 crate。体量与"胶水"不符的三个文件：`core/acp_client/{client,runner}.rs` 1,037 行、`commands/ssh_hosts/remote_skills.rs` 636 行、`commands/models_commands/tools.rs` 492 行 |
| `ProxyFingerprint` 双份声明 | `core/src/infra/http_client.rs:27`（7 字段，含 `normalize_bypass` 的 no-proxy 列表）、`models/src/ai_provider/http_client.rs:23`（6 字段） | 两个都是私有 struct，用途相同（按代理配置缓存 HTTP client）。与 AGENTS.md「所有远程 HTTP 必须通过 `probe_http_client`」红线交叉，见 §4.3 |
| `DeployKind` 双份且语义漂移 | `app/src/cli/commands.rs:246`（`Link/Dir/Broken/Missing`）、`skills/src/deployment/status.rs:12`（`Missing/Link/Copy/Unknown`） | 变体名不同，CLI 那份是私有的手工重新分类，**没有 `From<>`**——是隐式映射，比显式转换更难发现漂移 |
| `src-tauri` 目录名 ≠ crate 名的例外没有写进任何文档 | 目录 `src-tauri/` ↔ package `skillstar` ↔ lib target `skillstar_lib` | 这是 Tauri 工具链硬性要求，属于宝典 1.3 明文承认的"多语言单仓库例外"，**判定为合理例外**。缺的只是宝典原话要求的"把这个例外写进文档，而不是留给后人重新发现"：`cargo tree`/`--timings` 报告里出现的是 `skillstar`/`skillstar_lib`，磁盘上是 `src-tauri` |
| `skillstar-marketplace` 7 个顶层模块全是技术层名 | `db` / `models` / `remote` / `snapshot` / `mcp_models` / `mcp_remote` / `mcp_snapshot`；`models`(325 行)、`db`(25 行)尤其典型 | **尚未致病**：`remote`/`models`/`db` 出度全为 0，没有发展成宝典描述的"持久化层反向 import 业务域"。审计判断是"不建议为了改名而改名" |

---

## 3. 测试类未落地发现（第二部）

### 3.1 内嵌测试占 75%，Top 10「改测试就重编库」清单

**量化现状**：内嵌 `#[cfg(test)] mod X { … }` **118 个（75%）** vs 文件式 `#[cfg(test)] mod X;` **39 个（25%）**。39 个文件式模块（含 14 处 `#[path]`）说明团队已经知道并在用这个手法，只是没覆盖到最大的那批文件。

按「文件行数 × 内嵌测试行数」排序：

| # | 文件 | 文件行数 | 内嵌测试行数 | 测试占比 |
| --- | --- | ---: | ---: | ---: |
| 1 | `crates/skillstar-skills/src/projects/mod.rs` | 685 | **587** | **86%** |
| 2 | `crates/skillstar-skills/src/discovery.rs` | 960 | 306 | 32% |
| 3 | `crates/skillstar-skills/src/deployment/mod.rs` | 970 | 242 | 25% |
| 4 | `crates/skillstar-git/src/ops.rs` | 928 | 235 | 25% |
| 5 | `crates/skillstar-sync/src/ssh/sftp/list.rs` | 740 | 288 | 39% |
| 6 | `crates/skillstar-skills/src/update_checker.rs` | 615 | 332 | 54% |
| 7 | `crates/skillstar-usage/src/fetchers/oauth/cursor.rs` | 925 | 172 | 19% |
| 8 | `crates/skillstar-channels/src/shared_channels/github.rs` | 995 | 135 | 14% |
| 9 | `src-tauri/src/commands/projects.rs` | 408 | 305 | 75% |
| 10 | `crates/skillstar-models/src/ai_provider/skill_pick.rs` | 692 | 148 | 21% |

**机制：下游放大。** #1/#2/#3/#6 都在 `skillstar-skills`，该 crate 被 4 个成员依赖（app / channels / sync / src-tauri）；#4 在 `skillstar-git`，被 5 个成员依赖。改这些文件里的一行测试文本，会重编 crate 本身 + 全部下游。

⚠️ **#7 `cursor.rs` 受 AGENTS.md 保护**（"除非用户明确要求，不修改"），列出仅供参考。

**尚未测量（关键）。** 幅度完全没测——审计在只读约束下无法做 A/B。而第三部的独立实测给出了一个**下调预期的强证据**：`cargo check` 的热增量最坏情况只有 3.42 s（§7.6），所以"改一行测试要等很久"这个痛点在本仓库的 `check` 循环里并不存在。真正会放大的是 `cargo build`/`cargo test` 路径（touch `skillstar-core` 的 `cargo build --workspace` 增量是 15.78 s，是 check 的 4.6 倍）。

### 3.2 两个测试辅助函数泄漏进生产公共 API，且全仓无处门控

**量化现状**（复核 2026-08-14 仍然成立）：

| 位置 | 符号 | 跨 crate 调用者 |
| --- | --- | --- |
| `crates/skillstar-skills/src/skill_mutation.rs:119` | `pub fn replace_skill_mutation_policy_for_test(...) -> SkillMutationPolicyGuard` | `channels/src/shared_channels/channel_access_tests.rs:198, 275` |
| `crates/skillstar-skills/src/update_state.rs:239` | `pub fn reset_for_test()` | `channels/src/shared_channels/channel_update_installer_tests.rs:64, 284` |

连带 `pub struct SkillMutationPolicyGuard`（`skill_mutation.rs:128`）也是纯测试构件。`skillstar-skills` 是 app / channels / sync / src-tauri 的**普通依赖**，这三个符号进了发布二进制的依赖图。

**机制：为什么不能简单加 `#[cfg(test)]`。** 调用方是 `skillstar-channels` 的**单元测试**，它把 skills 当外部 crate 链接，`#[cfg(test)]` 对它不可见。唯一正解是 `test-util` feature——而 **13 个成员的 `Cargo.toml` 里没有一个有 `[features]` 段**（2026-08-14 复核仍为 0），也没有任何 optional 依赖。**今天连门控的地方都不存在**，这正是宝典 2.7 实测反例的形状。

对照组（做对了的）：4 处 `test_support` 全部正确门控——`skillstar-app/src/lib.rs:8-9`、`skillstar-sync/src/lib.rs:10-11`、`src-tauri/src/commands/ssh_hosts/mod.rs:163-164` 都是 `#[cfg(test)] pub(crate) mod`。

**误报记录**：`skillstar-git/src/ops.rs:436,440` 的 `reset_to_revision` / `reset_to_revision_in_session` 会被 `for_test` 正则扫到，但它们是真实的 git reset 生产 API，不计入。

### 3.3 33 个函数、51 份逐字重复的测试辅助

**量化现状**（函数体去空白归一化后 MD5 相同，且体积 ≥60 字符）：

| 辅助函数 | 逐字重复份数 |
| --- | ---: |
| `EnvGuard::drop` | **6** |
| `test_env_lock` | **6** |
| `lock_test_env` | 4 |
| `EnvGuard::new` | 3 |
| `env_lock` | 3（第三种拼写的同一概念） |
| 其余 26 个（`make_temp_root` / `create_pool` / `seed_named_skills` / `subscription_index` / …） | 各 2 |
| **合计** | **33 个函数、51 份冗余副本** |

`EnvGuard`（new+drop）的 6 份分布在 2 个 crate、6 个文件：`skills/src/local_skill.rs:670,698`、`skills/src/skill_install_tests.rs:70,98`、`skills/src/skill_install_removal_tests.rs:40`、`skills/src/repo_scanner/scan_install.rs:282,310`、`channels/src/shared_channels/channel_conversion_tests.rs:45`、`channels/src/shared_channels/subscription_installer_tests.rs:42`。

**机制上的重要限定。** `test_env_lock` 的 6 份**不能**用 `tests/common/mod.rs` 消除——它们是 `src/` 内的单元测试基建，每个测试二进制是独立进程，跨 crate 共享一把锁在运行时也没有意义。**能消除的是"代码重复"而非"锁实例"**，落点是一个 dev-only 的 `skillstar-test-support` crate（这与 AGENTS.md「先做私有 module」有张力，但 M-TEST-UTIL 是被认可的例外；真要做需同时更新 boundaries.md）。全仓有 11 处手写 env 互斥锁、155 个调用点，是一套自制的 `serial_test`，**它们该保留**。

顺带（超出第二部范围）：`urlencoding` × 4（`usage/src/fetchers/oauth/{opencode,codex,antigravity}.rs`、`fetchers/cookie/opencode.rs`）和 `save_config` × 4（`channels/src/patrol/config.rs`、`core/src/config/{proxy,marketplace_mirror,github_mirror}.rs`）是**生产代码**的逐字重复。

**尚未测量**：抽 crate 的改动面（1–2 天量级是估算，不是实测）。

### 3.4 其余测试类发现

| 发现 | 量化 | 边界 |
| --- | --- | --- |
| 无基准测试，却在为性能开关付费 | `benches/` **0 个**、criterion/divan 依赖 **0**、性能基线文档**未找到**；`[profile.release]` 上方**无任何解释性注释** | `lto` 已由 `fat` 降到 `thin`（`d9d82e3`），但**运行时代价至今未测**（§8.4-a）。审计的立场是：这不是"补一个 `benches/` 就好了"的问题，正确顺序是先回答"桌面应用的哪条路径对延迟敏感"，而不是先补基准来追认一个已经在付的成本 |
| 48 个"测试"其实是代码生成器 | marketplace 29 / models 10 / app 8 / src-tauri 1 = **48**，占 listed 1263 的 **3.8%** → 真实单元测试 **1215**，加集成 65 = **1280** | 这是 ts-rs `#[ts(export)]` 展开的 `#[test]`，不断言任何行为。设计是刻意的且 CI 已用 `check_generated_types.sh` 接成 gate，审计**不建议改**。但"`cargo test` 会写入 `src/types/generated/`"是一个会反复咬人的隐式契约，值得写明——见 §9 第 2 条对这条结论的修正 |
| 第二部门禁 5 条，落地 1 条 | `check_no_orphan_modules`（2.6）**已落地**（`178e577`）。仍缺：`check_no_test_code_in_lib`（2.7，今天 2 处违规）、`check_tests_common_shape`（2.3，今天 0 违规）、`check_no_silent_test_skip`（2.9，今天 0 违规）、`check_internal_doctest_off`（2.10，改动已落地但无门禁锁住） | 三条今天 0 违规的可以直接进 blocking（宝典 5.3 的"0 违规才进 deny"） |
| E2E 层为 0 | 无 `assert_cmd`、无 `Command::cargo_bin`、无 playwright/`*.spec.ts` | 同一个 `skillstar` 二进制同时是 GUI 与 CLI，**CLI 路径今天没有进程级端到端测试**。宝典对内部项目不强制 E2E，判定为**缺口而非违规** |
| 同义反复测试 3 处，均为轻度 | `marketplace/src/remote/tests.rs:388`（`assert_eq!(SEARCH_API_HARD_LIMIT, 200)`）、`src-tauri/src/lib.rs:85,86`（`DEEP_LINK_SCHEME` / `DEEP_LINK_EVENT`） | 三处都是真实行为测试里夹带的一行常量钉子，**不构成假安全感**。AI 生成痕迹抽样（channels 的 `*_tests.rs`、`grok_tests.rs` 894 行）**未发现**宝典警告的"抄实现"模式 |
| `#[ignore]` 形态 | 4 个测试，全部是宝典认可的形态且写了原因 | 唯一 nit：`marketplace/src/remote/ai_search_tests.rs:7,85` 是裸 `#[ignore]`，原因写在文件头而不是跟着测试走 |
| 集成二进制配比**已经做对** | 20 个测试二进制（14 单元 + 6 集成），集成/成员 = **0.46**；集成测试只占全部测试的 **5%**（65/1280） | 宝典 2.8 的反例是"69 个测试二进制"，本仓库是反过来的。`providers_prop_tests.rs` 用 `#[path]` 把 `part2/part3` 挂成模块而不是独立二进制，正是宝典推荐形态。**这条列在这里是为了防止后人"优化"掉它** |

---

## 4. 依赖类未落地发现（第四部）

### 4.1 规模与放大倍数

| 指标 | 数值 |
| --- | ---: |
| `Cargo.lock` package 条目 | 915 |
| 唯一包名 | 784 |
| 存在多版本的包名 | **91**（排除 `windows-*` / `wasi` / `wit-bindgen` / `r-efi` 后仍有 **68**） |
| 从 workspace 成员实际可达的包（host = macOS） | 758 |
| 自己直接声明的第三方依赖 | 37（workspace 表）+ 26（成员各自）= **63** |
| 被编译成 >1 个 `(version, featureset)` 库单元的包 | **73 / 578（12.6%）**，占 **30.7%** 的 unit-seconds（180.4 / 588.2） |

**63 个直接依赖拖出 758 个包，放大约 12 倍。**

### 4.2 4.6 挑出的最可疑依赖：五个里两个已处理，三个仍在

| # | 候选 | 状态 | 量化 |
| --- | --- | --- | --- |
| 1 | `src-tauri` 的 24 个零引用直接依赖（`petgraph` / `pulldown-cmark` / `urlencoding` / `pbkdf2` 等） | **已落地** `d9d82e3` | — |
| 2 | 三套 HTML 解析栈 | **部分落地** | 见下 |
| 3 | `reqwest 0.12` 与 `0.13` 并存 | **仍在** | 见 §4.3 |
| 4 | RustCrypto 双世代 | **仍在** | 见 §4.4 |
| 5 | `tiny_http 0.12`（`skillstar-usage`，OAuth 回调本地服务器，4 处调用 / 2 个文件） | **保留** | 列出是为了说明筛选标准不是"体量大就删"——它确实在被用，与 #1 的化石形成对比 |

**#2 HTML 解析栈的量化与残余。** 审计当时实测：`html5ever`×4（0.27/0.29.1/0.38/0.39）+ `markup5ever`×4 + `selectors`×3 + `cssparser`×3 + `string_cache`×2 + `tendril`×2 + `servo_arc`×2 + `web_atoms` + `phf`×4 = **52.0 unit-seconds**，发生在 0–44 s 那段并行度 11.9/12 的满载窗口里，折算 **≈4–5 s 墙钟**。三套的来源：`skillstar-skills` 用 `scraper 0.27`，`skillstar-marketplace` + `src-tauri` 用 `html2md 0.2`，`tauri-utils` 自带第三份。

`html2md → htmd` 已落地（`55290b0`），从 lockfile 移走了 `html2md 0.2.15` / `html5ever 0.27` / `markup5ever 0.12,0.14` / `jni`。**残余部分没有重新测量**：`crates/skillstar-skills/Cargo.toml:35` 仍是 `scraper = "0.27.0"`（全仓 4 处调用，1 个文件），`tauri-utils` 那份也还在。**52.0 unit-seconds 是换 htmd 之前的数字，不能拿来描述今天。**

### 4.3 `reqwest` 0.12 / 0.13 双版本的来源，以及它的非体量后果

**来源（精确）。** workspace 表钉 `0.13.3`；`crates/skillstar-models/Cargo.toml:43` 另外以别名显式引入第二份：

```toml
reqwest_012 = { package = "reqwest", version = "0.12", default-features = false, features = ["rustls-tls", "json", "stream"] }
```

原因是迁就 `async-openai 0.27` 的传递约束（需要与它交换 reqwest 类型）。**这是唯一一处 workspace 版本表被绕过的地方**，也是 91 个多版本包里**唯一由本仓库清单造成的**一个。

**代价（unit-seconds）**：reqwest 本体 0.78 + `async-openai` 3.19 + `derive_builder` 系 2.81 + `reqwest-eventsource`/`eventsource-stream`/`backoff`/`secrecy` ≈ **7.1 s**。好消息是代价被控制住了：`hyper`、`rustls`、`http` 在 lockfile 里都只有一个版本，其余重复只有 `wasm-streams 0.4/0.5` 和 `base64 0.21/0.22`。

**★ 非体量后果（更重要）。** AGENTS.md 红线要求"所有远程 HTTP 必须通过 `skillstar_core::infra::http_client::probe_http_client`"，而 `probe_http_client` 返回的是 reqwest **0.13** 的 `Client`。实测域 crate 内有 3 处直接构造 client：

| 位置 | 形态 | 判定 |
| --- | --- | --- |
| `skillstar-usage/src/oauth/manual_callback.rs:23` | 裸 `reqwest::Client::builder()` | **真违规**——完全不读用户代理配置 |
| `skillstar-models/src/ai_provider/http_client.rs:69` | 第二套 client 构造器，自己重新实现了一遍代理处理 | 精神符合、SSOT 分裂（与 §2.6 的 `ProxyFingerprint` 双份是同一件事） |
| `skillstar-models/src/ai_provider/openai_client.rs:46` | `reqwest_012::Client::builder()` | **类型上不可能**用 `probe_http_client` |

即：**这条红线对 AI 推理路径是物理上无法遵守的**，直到 `async-openai` 升到 reqwest 0.13 为止。审计逐个查过 `tracing` / `rand` / RustCrypto，**没有发现"两份全局 registry 各自持有状态"级别的运行时幽灵**；最接近的就是这条——它不是不可见的指标，而是不可见的**代理配置失效**。

### 4.4 RustCrypto 双世代与 `ssh-key` 的 `=` 精确版本

**量化现状。** 一条 rc 版 SSH 依赖把整个 RustCrypto trait 世代分裂了：

```
crates/skillstar-sync/Cargo.toml:23   ssh-key = { version = "=0.7.0-rc.10", features = ["alloc"] }
    → ssh-cipher 0.3.0-rc.9 → aes-gcm 0.11.0-rc.4
```

受影响的包约 16 个各编两遍（`digest`/`block-buffer`/`crypto-common`/`generic-array`/`cipher`/`aes`/`aead`/`ctr`/`ghash`/`polyval`/`universal-hash`/`inout`/`sha1`/`sha2`/`hmac`/`pbkdf2`/`cpufeatures`），合计 **23.4 unit-seconds**。

**机制。** 本仓库钉的是 RustCrypto 新一代（`aes-gcm 0.11.0-rc.4`），而 `russh`/`ssh-key`/`scrypt`/`argon2` 走旧一代。**不构成运行时幽灵**——`sha2 0.10` 与 `0.11` 的 `Digest` 是两个不同的 trait，混用在编译期就会失败，是编译摩擦而非静默错误。消除它不是单方面能决定的，要等整个 RustCrypto 生态出稳定版。

**顺带**：`ssh-key` 这行是**全仓唯一的 `=` 精确版本，且一个字的注释都没有**。它用得对（rc 版本 Cargo 兼容性推断脆弱；`skillstar-sync` 实际通过 `russh::keys` 使用它，`grep -rw ssh_key crates/skillstar-sync` = 0 命中，说明这条依赖存在的唯一目的就是把版本钉到 russh 期望的那一个）——但后人看到一个指向 rc 版本的 `=` 只会不敢动，也不知道为什么不能动。同一文件里 `keyring` 上方有 2 行注释解释理由，`ssh-key` 那行没有。

**关于给 `tauri` 加 `=2.11.1`**：审计结论是**不该**。宝典的判据是"曾经出过事才钉"，而 `docs/errors.md` 的 50 条记录 + 三个 workflow 的 Failure lessons 里没有一条是 tauri patch 版本引起的；已有 lockfile + `--locked` 提供确定性；真加了 `=`，`@tauri-apps/cli ^2.11.1` 和 6 处 `tauri-plugin-* = "2"` 不会跟着钉，反而制造"Rust 侧钉死、JS/plugin 侧浮动"的更隐蔽分裂。

### 4.5 `tokio` 的 5 处 `features = ["full"]`

**量化现状**（2026-08-14 复核仍为 5）：`skillstar-core`、`skillstar-marketplace`、`skillstar-models`、`skillstar-app`、`src-tauri` 声明 `["full"]`；另外 5 个成员（`channels`、`github-auth`、`skills`、`sync`、`usage`）已经很认真地写了窄清单（`sync`/`time`/`macros`/`rt-multi-thread`/`fs`/`io-util`）。

实测只编了**一份** tokio 单元，feature 集是：`bytes,default,fs,full,io-std,io-util,libc,macros,mio,net,parking_lot,process,rt,rt-multi-thread,signal,signal-hook-registry,socket2,sync,time,tokio-macros`。

**机制（这条最容易踩空）。** feature 是可叠加的、在 workspace 构建里取并集——**那 5 份窄清单在 `cargo build --workspace` 里收益为零**，只要还有任何一个成员写着 `"full"`。要拿到裁剪收益，必须**同一个 commit 里把 5 处全改**，改 4 处等于没改。

**尚未测量**：改完之后到底省多少。tokio 只编 1 份，省下的是它的编译体量的一部分，没有实测数字。

其它重型依赖的 `default-features` 核对全部合格：`gix`（`default-features = false` + `max-control` 点名，只编 1 份）、`reqwest`、`keyring`、`russh`、`reqwest_012`、`image`、`tar`、`flate2` 均已做对；`rusqlite` 的 `bundled` 是刻意的（跨平台发布必需），`libsqlite3-sys` build script 在基线里只花 3.67 s，不值得动——但**千万别给它加 `opt-level`**（原因在 D-032）。

### 4.6 `aws-lc-sys`：冷构建关键路径上最大的单元，且上游强制

**量化现状。** `rustls 0.23.38` 只编了一份，但那一份同时打开了两个 crypto provider：`reqwest 0.12` 的 `rustls-tls` → `ring`；`reqwest 0.13` 的 `rustls` → `aws-lc-rs` → `aws-lc-sys`。于是两套 C 密码学库都要编：

```
start= 23.1 dur= 26.90 end= 50.0   aws-lc-sys v0.40.0 [build-script (run)]   <- 单个最大单元
start= 24.3 dur=  7.84 end= 32.2   ring       v0.17.14 [build-script (run)]
```

**`aws-lc-sys` 的 build script 单独跑了 26.90 s，占 62.55 s 冷 check 墙钟的 43%，而且它在关键路径上**：50.0 结束 → `aws-lc-rs` 50.5 → `rustls` 52.0 → `reqwest` 52.5 → `skillstar-core` 52.9 → …… → `skillstar` 62.4。全机在 44–50 s 这 6 秒里基本只在等它一个。

**机制：feature 裁剪消不掉它。**

```
gix 0.80 feature "blocking-http-transport-reqwest-rust-tls"
  → gix-transport "http-client-reqwest-rust-tls" → "reqwest/rustls"
reqwest 0.13 [features] rustls = ["__rustls-aws-lc-rs", "dep:rustls-platform-verifier", "__rustls"]
```

`reqwest 0.13` 把 `rustls` 这个 feature **硬编码**成 aws-lc-rs provider，`gix` 只提供 `-rust-tls` 这一个 rustls 变体。**即使把自己 6 个成员的 `reqwest` 全部换成 `rustls-no-provider`，`gix` 仍会打开 `reqwest/rustls`，`aws-lc-sys` 照编不误。** `ring` 那份也去不掉——`russh` 直接依赖它。

可行方向只有三条，都是架构决策不是构建调参：① 把 gix 的 HTTP 传输换成 `blocking-http-transport-curl-rust-tls`（引入 curl，未必更便宜，**未测**）；② 等 `gix` 上游提供 ring 变体；③ 接受它。审计的建议是③，并把这条记下来——**免得后人反复来查"为什么冷构建卡在 aws-lc-sys"**。

**上游强制、不可消除的其它重复**（一并记下，避免重复排查）：`getrandom` 四代 5 单元 12.7 s、`thiserror` 1/2 12.7 s、`syn` 1/2 7.5 s、`hashbrown` ×5 1.4 s、`serde`/`serde_core`/`serde_json` 同版本 2 份 27.9 s（`resolver = "3"` 把 build-dependency / proc-macro 与普通依赖分开解析 feature，是**正确行为不是配置错误**）。

---

## 5. 门禁与文档 SSOT 类未落地发现（第五部）

### 5.1 规则可机器检查率：26 条里 15 条仍是纯散文

**量化现状**（把 `AGENTS.md` 逐句拆成可判定条目，共 26 条）：

| 状态 | 条数 | 占比 |
| --- | ---: | ---: |
| 完全有自动检查 | **2** | 8% |
| 部分检查 | **7** | 27% |
| **仅存在于散文里** | **15** | **58%** |
| 不适用（流程建议） | 2 | 8% |

对照宝典实测样本（8 完全 / 6 部分 / 16 散文，共 30 条）：**绝对数字更差，但结构完全一致——大约六成规则没有任何东西在它被违反的那一刻叫停。**

本次已落地的改动把 R1（依赖图 ⇄ `boundaries.md`）从"部分"抬到了完全（`check_dep_graph_doc.sh`，实测 38 ⇄ 38 零差异），并补上了 R16 的测试文件盲区。**其余 15 条纯散文规则原样保留。**

**机制：宝典的规律在本仓库逐条印证。** 有门禁的 R14（feature 边界）、R19（生成类型新鲜度）→ 基线为空 / 严格相等，**零漂移**。没门禁的 R3（decisions 格式）、R5（README 链接）、R6（不留"待补"）、R7（SSOT）、R11（统一 HTTP client）→ **全部已经漂了**，而且是在只读扫描里被动撞见的，不是特意去找的。唯一的例外是 R17（测试不碰真 `$HOME`）和 R26（提交格式），今天 100% 成立——合理解释是它们被烧过（`ci.yml` Failure lesson #2 记的就是 HOME 事故），而**痛感会消退，门禁不会**。

**最容易补的几条及其今天的违规数**（**0 违规意味着可以直接进 blocking，落地当天不会红**）：

| 规则 | 检查方式 | 违规数 | 成本 |
| --- | --- | ---: | --- |
| R18（不改 `oauth/cursor.rs`） | pre-commit: `git diff --cached --name-only \| grep -q 'oauth/cursor\.rs'` | **0** | **1 行** |
| R8（前端不绕过 `invoke()`） | `grep` `fetch(` / `XMLHttpRequest` / `new WebSocket`（排除测试） | **0** | ~10 行 shell |
| R26（Conventional Commits） | `commit-msg` hook + 正则 | **0**（最近 12 个提交全合规） | ~10 行 |

### 5.2 门禁盲区（两处仍在）

| 盲区 | 量化 | 边界 |
| --- | --- | --- |
| `check_command_boundaries.sh` 只扫 `src-tauri/src/commands/` | `src-tauri/src/core/` 有 **15 处** `std::fs::`/`tokio::fs::`；`src-tauri/src/cli/` 完全不在范围 | 按 `boundaries.md` 的分工，`core/` 拥有 Tauri 胶水、文件操作可能正当——但这是**未写下来的豁免**，不是被检查的边界 |
| `check_i18n_hardcoded.sh` 只扫 `src/`（前端） | Rust 侧有 **362 行非注释 CJK**（49 个文件） | 其中相当一部分是会到达 UI 的错误消息（如 `UsageError::Other(format!("OAuth 回调客户端创建失败: {}", e))`），另一部分是正当的种子数据（`mcp_snapshot/seeds/publishers.rs` 40 行中文发布者名）和 CLI 输出。**不是说该把 362 行全塞进 i18n**，而是这条规则今天只覆盖了一半的表面，且没有任何地方写明另一半是豁免的 |

另有三条脚本自身的形态问题：`check_workspace_deps.sh` 的 **7 条硬编码禁止边里有 3 条恒真**（指向 `skillstar-projects`/`skillstar-ai` 等已删 crate），它**不是通用的方向检查、也不查环**——新增一条反向边（比如 `core → git`）不会被它发现（这条已被新的 `check_dep_graph_doc.sh` 部分覆盖）；`check_error_strings.sh` 实测 **29 < 基线 34**（基线曾在 2026-07-26 从 22 **上调**到 34，是棘轮机制最脆弱的时刻）；`check_feature_imports.sh` 只查 `from "..."` 形式，动态 `import()`/`require()` 不匹配（今天仓库里没有，属理论漏洞）。

### 5.3 `release.yml` 与 `ci.yml` 触发条件不同

**量化现状**：

| workflow | 触发 | 跑了什么门禁 |
| --- | --- | --- |
| `ci.yml` | push→main / PR | lint、build、vitest、`cargo test --workspace`、全部棘轮、cargo-deny |
| `windows-ci.yml` | push→main / PR | lint、build、npm test、`cargo test --workspace --exclude skillstar`、**只有 `check_file_size.sh`** |
| `release.yml` | **push tag `v*`** / dispatch | `bun install --frozen-lockfile`、`bun run build`、GitHub App client ID 检查、`tauri-action` 构建 |

**`release.yml` 不跑任何 Rust 测试，也不跑任何棘轮，也不 `needs:` 一次通过的 CI。** 即 `git tag v0.0.5 && git push --tags` 可以在 `ci.yml` 从未对该 commit 跑过的情况下直接出包。已落地的 pre-push hook 能挡住一部分（推 tag 时也触发），**但不等于 release 门禁**。

### 5.4 文档自相矛盾 / 引用不存在的对象（10 处，2026-08-14 复核仍在）

只统计 active 文档（`docs/others/` 已标 historical，其中的陈旧路径引用是预期行为，不计入）：

| # | 位置 | 问题 |
| --- | --- | --- |
| 1 | `AGENTS.md`「Agent skills → Domain docs」 | 声明 "根目录 `CONTEXT.md` + **`docs/adr/`**" —— **`docs/adr/` 目录不存在**（ADR 实际住在 `docs/decisions.md`） |
| 2 | `docs/agents/domain.md` | 同上 |
| 3 | `scripts/internal/check_file_size.sh`（第 3、18、118 行） | 引用 **`docs/ROADMAP.md`** —— 不存在（实际是 `docs/others/roadmap.md`）。**第 118 行是门禁失败时打印给用户看的那句话**，会把人指向一个不存在的文件 |
| 4 | `scripts/internal/file_size_baseline.txt:3` | 同样引用 `docs/ROADMAP.md` |
| 5 | `README.md:122` | 链接 `./crates/skillstar-skills/src/agents/builtin.rs` —— 文件已随 `d6ad988` 移到 `crates/skillstar-agents/src/builtin.rs` |
| 6 | `docs/decisions.md` D-008 / D-009 / D-022 的证据行 | 引用 `crates/skillstar-skills/src/agents/`、`crates/skillstar-skills/src/github_auth/`（均已移走）、`src-tauri/src/core/path_env.rs`（D-022 自己记录了删除它，同一条的证据行仍指向它） |
| 7 | `docs/decisions.md` D-016（复核在 `:155`） | 证据行写 `commit 待定` —— 直接违反 AGENTS.md 的"不留待补" |
| 8 | `docs/decisions.md` D-013 | 缺 `证据` 段，不符合该文件底部自己定义的六段格式 |
| 9 | `docs/decisions.md` D-014 / D-015 | 用 `状态：已接受（2026-08-05）` 合并了日期与状态，不符合同一文件定义的格式 |
| 10 | `AGENTS.md`「常用验证」vs `.github/workflows/ci.yml` | AGENTS.md 列 **4** 个脚本，CI 实际跑 **10** 个（审计时是 8 个，本次又新增 2 个）。照文档办事的人会跳过 `check_clippy_ratchet` / `check_error_strings` / `check_generated_types` / `check_i18n_hardcoded` / `check_no_orphan_modules` / `check_dep_graph_doc` |

**机制**：第 6 条尤其值得注意——它们全部是 `d6ad988`（crate 拆分）造成的，说明**大重构会一次性打断一批文档引用，而没有任何东西在那一刻叫停**。

这 10 处**全部可机器检查**：一个遍历 active 文档、把 markdown 链接与反引号里的仓库相对路径拿去做存在性判定的脚本（审计已写过一次性版本，跑一遍 <1 s）会报出第 1/2/5/6 条；`docs/decisions.md` 的格式校验会报出第 8/9 条。

### 5.5 被复述在多处的事实

| 事实 | 声称的 SSOT | 实际出现在 | 漂了吗 |
| --- | --- | --- | --- |
| **门禁脚本清单** | 无声明的 SSOT | `ci.yml`（权威）、`AGENTS.md`、`features/frontend/README.md`、`features/skills/README.md`、`boundaries.md`（3 处分散提及）、`others/roadmap.md` | **已漂**（见 §5.4 第 10 条）。最小修复不需要任何代码：在 AGENTS.md「常用验证」下写明"完整门禁清单的 SSOT 是 `.github/workflows/ci.yml`，本节只列必须手动跑的子集" |
| **完整验证命令** | 无声明的 SSOT | `AGENTS.md` 5 条、`README.md:187-191` **逐字重复**同样 5 条、`architecture.md:130` 指向 AGENTS.md（正确做法）、各 `features/*/README.md` 列子集 | 目前一致，无门禁 |
| **产品版本 `0.0.4`** | 无声明 | `package.json:4`、`src-tauri/Cargo.toml:3`、`src-tauri/tauri.conf.json` | 目前一致，无门禁。**两处各存一份、互不引用**（见 §9 第 1 条） |
| **crate 职责描述** | `boundaries.md` 所有权表 | `boundaries.md` 的表格 + 同一文件项目树注释、部分 `crates/*/Cargo.toml` 顶部注释 | 目前一致 |

**做对了的部分**（列出以免被"优化"掉）：`CLAUDE.md` 只有一行 `@AGENTS.md`（委托而不复述）；`docs/architecture.md` 全文**没有出现任何一个具体版本号**，只逐类指向 manifest；`docs/others/README.md` 的决策表给每份历史文档标了状态 + 决策 + 当前 SSOT 指向；`docs/decisions.md` 开头主动声明"当前结构以 boundaries.md 为准"。

### 5.6 `cargo xtask`：判据、切法与优先级

**判据**（宝典 5.1）："检查"进 xtask，"编排"留给 shell。逐个过之后的切法是 4 进 4 留：

| 进 xtask（检查） | 留 shell（编排） |
| --- | --- |
| `check_file_size` / `check_i18n_hardcoded` / `check_feature_imports` | `check_generated_types`（起子进程 + `diff -r`） |
| `check_workspace_deps` + `check_command_boundaries`（**已经在 bash 里内嵌 Python heredoc** 解析 `cargo metadata` JSON，是"用 bash 包着的 Python"） | `check_clippy_ratchet`（起子进程 + 计数） |

**支持的证据**：`windows-ci.yml` 的 Failure lesson #3 明写"ratchet 脚本假设 POSIX shell"，因此 Windows CI 只跑了全部门禁里的 1 个——xtask 是跨平台的；`check_i18n_hardcoded.sh` 用了 **28 行**注释解释两个 bash 陷阱（`LC_CTYPE=C` 下字符类退化成字节比较、`grep -q` 触发 SIGPIPE 让 `set -o pipefail` 误报），`check_file_size.sh` 里还记了 `find -o … -print0` 的优先级坑——**这些注释是 bash 的税，不是问题域的复杂度**；依赖已存在（`toml`/`serde_json`/`regex` 都在 workspace 表里）。

**优先级说明（重要）。** 审计当时明确写了"xtask 是重构，它不解决门禁没在变更路径上跑这个真问题——先做 hooks，再考虑 xtask。把 8 个不跑的 shell 检查重写成 8 个不跑的 Rust 检查，收益为零"。**hooks 已经落地（`178e577`）**，所以今天 xtask 剩下的理由只有两条：Windows CI 的覆盖缺口，和那些 bash 陷阱注释。

---

## 6. ★ 实测推翻宝典的三条

这一节和 §7 是本次审计最贵的部分。它们的存在是为了**防止后人照着宝典把已经验证过是负收益的东西加回来**。

### 6.1 3.3 `[profile.dev.build-override]` + 3.2 package 热路径清单

**结论与主要数字已归 [decisions.md D-032](../decisions.md)，本文不复述**（那里有 opt-level 0/1/2/3 的单调曲线、derive 密度 756 vs 4090 = 18% 的根因、`--timings` 逐单元差分 +980/−137、以及 `OPT_LEVEL → cc` 传给 `build.rs` 的陷阱）。

**本文只补 D-032 没有收录的那部分——为什么 profile 这个方向在本仓库结构上就没有空间。**

冷 `cargo check --workspace`（62.55 s 基线）的 `--timings` 并行度曲线（12 核）分成三段：

| 阶段 | 墙钟 | unit-seconds | 平均并行度 |
| --- | ---: | ---: | ---: |
| 0–44 s 并行主体（650 个第三方包） | 44.0 s | 521.7 | **11.9 / 12** |
| 44–50 s 等 `aws-lc-sys` 的 build script | 6.0 s | 39.8 | 6.6 |
| 50–62.5 s `skillstar-*` 串行尾 | 12.5 s | 26.6 | **2.1** |

**这张图直接判了 profile 的死刑**：前 44 s 已经是 11.9/12 的满并行，`opt-level` 再怎么调也压不动；剩下 18.5 s 是两个**结构性**瓶颈（一个单线程 C 构建脚本 + 一条深 DAG），同样与 profile 无关。

配套的第二个数字，是本次审计最该记住的一条：

> **我们自己的 13 个 workspace 编译单元合计 15.7 unit-seconds，占全部 588.2 unit-seconds 的 2.7%。**

任何针对 `skillstar-*` 源码的优化（拆 crate、减泛型、破 SCC）对**冷构建**的天花板都只是这 15.7 秒里的一部分。真正的钱在那 665 个第三方包和它们的重复上。

### 6.2 3.13「`target/` 放内置 SSD」

**结论与 B 组（冷 `build` 写 8.3 G）的数字已归 [decisions.md D-032](../decisions.md)，本文不复述。**

**本文只补 D-032 没有的 A 组读数与机制解释**，因为它演示了一个重要的读数纪律。

A 组，冷 `cargo check --workspace`（写 1.5 G），内外交替：

| 顺序 | target dir 所在卷 | real | user | sys |
| --- | --- | ---: | ---: | ---: |
| 1 | 内置 `/private/tmp` | 57.07 s | 178.76 s | 34.25 s |
| 2 | **外置 `/Volumes/Acasis`** | **50.35 s** | 197.15 s | 35.30 s |
| 3 | 内置 | 44.63 s | 180.06 s | 33.61 s |
| 4 | **外置** | **41.99 s** | 193.36 s | 35.59 s |

外置**反而更快**——但这四次是单调下降的（57.07 → 50.35 → 44.63 → 41.99，机器在逐渐变安静），**趋势幅度大于内外差**。正确读法是"差异被漂移淹没"，而不是"外置更快"。这就是为什么最终结论必须靠 B 组那种"用两次内置夹住一次外置"的形态来下（见 D-032）。

**机制**：`--timings` 显示前 44 s 是 11.9/12 的满 CPU 并行，**cargo 的写入是与重 CPU 工作交错的，I/O 从来不是瓶颈**。`sys` 时间内外几乎一致（A 组 33.6–35.6 s，B 组 46.5–49.3 s）直接印证了这点。宝典那条测的是卷的裸写入带宽，本工作负载根本跑不到那个上限。

### 6.3 附录 B #24「workspace 表里开 feature 会让所有成员被迫编」——规则成立，但**代价在本项目实测为零**

**量化现状**（逐条核对 workspace 表里开着的 5 个 feature）：

| workspace 表里开的 feature | 依赖它的成员数 | 其中"用不到却被迫编"的成员 | 实际代价 |
| --- | ---: | ---: | --- |
| `serde = { features = ["derive"] }` | 12 | **0** | 唯一不需要 derive 的成员是 `skillstar-providers`，而它是零依赖叶子（`[dependencies]` 段是空的），**根本不依赖 serde**。12 个消费者的 derive 计数分别是 3/21/95/13/7/10/60/55/72/19/60/12——全部在用 |
| `chrono = { features = ["serde"] }` | 10 | 无法只读判定 | chrono 的 serde feature 只加 impl，不生成额外代码单元。可忽略 |
| `clap = { features = ["derive"] }` | 2 | **1**（`src-tauri` 声明了却零引用） | 代价是"多编一个 clap"，而 `skillstar-app` 无论如何都要编它。正确修法是删声明——**已随 `d9d82e3` 落地** |
| `tauri = { features = ["tray-icon","image-png"] }` | **1** | 0 | 零代价 |
| `rusqlite = { features = ["bundled"] }` | 4 | 0 | 4 个消费者都要 bundled（桌面应用不能依赖系统 sqlite）；C amalgamation 在 workspace 内只编**一次**并共享，不是每成员一次 |

**判定：不建议为它做重构。** 把 5 条 feature 从表里下放到 12 个成员，会新增约 20 行清单、制造新的分裂面，换来的是 **0 个成员少编代码**。

**但治理面的隐患是真的（不要把上面的结论读成"这条规则不用管"）**：workspace feature 是**并集**，成员只能加不能减。将来出现第一个需要 `serde` 不带 derive、或 `rusqlite` 不带 bundled 的成员时，它没有退出路径，只能绕过表自己声明——那时就产生第三个分裂点（今天的两个分裂点是 `tauri` 的 dev-dep 写法和 `reqwest` 双版本，前者已随 `d9d82e3` 落地修复）。审计给出的最小动作是：**在根 `Cargo.toml` 现有注释里补一句"表里的 feature 是并集、成员无法关闭"**，把这个约束从隐性变显性。

### 6.4 附带：宝典自己已标［推翻］、本仓库独立复核成立的一条

**4.1「统一版本能大幅提速」——本仓库的价值是治理，不是速度。**

证据：`Cargo.lock` 91 个包名存在多版本，逐个反查后**由本仓库清单造成、且可通过统一清单消除的只有 `reqwest` 一个**——而它也不是靠改表能修的（`async-openai 0.27` 的传递约束，见 §4.3）。其余全部是上游传递依赖强制的（`syn 1/2`、`hashbrown ×5`、`getrandom ×4`、`rand ×4`、`phf ×4`、`windows-* ×20+`），改自己的表对它们零影响。

所以 `[workspace.dependencies]` 在这里的真实价值是"37 个共享依赖只有一处版本，且下一次 `cargo update` 不会引入新分裂"。**不应对外承诺它省了构建时间——本审计没有测过，也没有理由相信它省得多。**

---

## 7. ★ 实测判定无收益、明确不做的六件事

这六件事按审计当时的原始清单排列。每一条都**跑过**，不是推断。

### 7.1 `[profile.dev.build-override]` —— 负收益

→ 见 [decisions.md D-032](../decisions.md) 与本文 §6.1。

### 7.2 `[profile.dev.package.<hot>]` 热路径清单 —— 负收益

→ 见 [decisions.md D-032](../decisions.md)。

### 7.3 换链接器（lld / mold）—— 天花板 1.19 秒，且在 macOS 上是倒退

**量化现状。** 方法：给 rustc 挂一个 linker wrapper，用 `/usr/bin/time -p` 包住真正的 `/usr/bin/cc`，每次链接记一行。

冷 `cargo build --workspace`（95.83 s 墙钟 / 375.47 s CPU）共 **114 次链接、合计 9.37 s**：

| 口径 | 占比 |
| --- | ---: |
| 链接总耗时 / 冷构建 **CPU** | 9.37 / 375.47 = **2.5%** |
| 链接总耗时 / 冷构建**墙钟** | 9.37 / 95.83 = 9.8%（但 114 次链接是并行发生的，不构成 9.8% 的串行阻塞） |
| 热重编里的链接占比（最坏，touch `skillstar-core`） | 1.37 / 15.78 = **8.7%** |
| 热重编里的链接占比（只改 `src-tauri`，比例反而最高） | **1.19 / 5.90 = 20.2%** |
| release 构建（R_fat）的链接占比 | 9.79 / 425.20 = **2.3%** |
| 最终二进制单次链接 | **0.48 s**（dev）/ **0.29 s**（R_fat） |

**结论：换链接器的天花板是 1.19 s**（只改 `src-tauri` 时的那 2 次链接），而且那还得假设新链接器**免费**。

**机制。** 宝典 3.5 已在同类机器上实测：macOS 上 `ld64.lld` 比 Apple 默认 ld-prime **慢 1.7 倍**，`mold` 根本不支持 Mach-O。把这两条与本仓库的 2.3%–20% 占比放在一起：**换 lld 会让本仓库的热重编变慢，不是变快。双重不值得。**

顺带：`split-debuginfo = "unpacked"` 在 macOS 上早已是 `dev` 的默认值，写进清单**零收益**（宝典 3.5 / 附录 B #25）。

另一个角度的佐证：`fat` LTO 比 `thin` 多花的 126 秒**全部发生在 rustc 的 codegen 阶段**，链接本身只有 0.29 秒（R_fat 0.29 / R_thin ~0.2 / R_fast 0.21）。无论 dev 还是 release，本仓库的链接器都不是瓶颈。

**方法学说明**：wrapper 只在被测的 `/usr/bin/time -p /usr/bin/cc` 区间内计时，wrapper 自身开销（mktemp/awk）在计时区间之外，因此每条链接的秒数是干净的；但挂 wrapper 会给那次构建的总墙钟额外加约 3–4 s（114 次 × ~30 ms）。**A/B/C 主实验没有挂 wrapper。**

### 7.4 本地设 `CARGO_INCREMENTAL=0` —— 无显著差异

**量化现状**（背靠背，用两个基线锚点夹住）：

| 组 | real | user | target dir |
| --- | ---: | ---: | ---: |
| A3 现状 | 54.02 s | 224.75 s | 1.5 G |
| **F** `CARGO_INCREMENTAL=0` | **76.31 s** | 239.36 s | **1.3 G** |
| A4 现状 | 77.04 s | 304.68 s | 1.5 G |

**F 的 real 和 user 都落在 A3 与 A4 之间。冷 `cargo check` 上关掉 incremental 实测无显著差异，本地不必设。**

唯一可见收益是 target dir 小 13%（1.5 G → 1.3 G）。这在 CI 上才有意义（直接等于缓存体积和上传/下载时间），值得为"缓存体积"和"配置可读性"显式写进 workflow 的 `env:`，**但不要当成提速手段**。

### 7.5 引入 `cargo-hakari`（workspace-hack）—— 没有可收拾的 feature 分裂

**量化现状**（从 `--timings` 的 `UNIT_DATA` 直接读每个编译单元真正用的 feature 集，比 `cargo tree -e features` 更接近事实）：

| 依赖 | 编译单元数 | 备注 |
| --- | ---: | --- |
| `tokio` | **1** | feature 集是 5 处 `"full"` 与 5 处窄清单的并集 |
| `reqwest 0.13` | **1** | 6 个成员的 feature 声明不同，但**并集只编 1 份**——重复来自版本（0.12 vs 0.13），不是 feature |
| `gix` | **1** | `default-features = false` + `max-control` 用法正确 |
| `rustls` | **1** | 但那一份同时打开两个 crypto provider（§4.6） |

**宝典自己就说"先用 `cargo tree -e features` 确认真有分裂，再引入，不要预防性加一层"。确认结果：不需要。**

### 7.6 为热增量拆 crate（3.9）—— 本仓库根本没有热增量痛点

**量化现状。** `cargo check --workspace` 的热增量（`touch` 一个文件后重跑）：

| 被 touch 的文件 | A = 现状 | B = 宝典 3.11 profile |
| --- | ---: | ---: |
| `crates/skillstar-core/src/lib.rs` | **3.42 s** | 3.49 s |
| `crates/skillstar-skills/src/lib.rs` | 3.07 s | 3.23 s |
| `crates/skillstar-marketplace/src/lib.rs` | 2.69 s | 2.92 s |
| `crates/skillstar-models/src/lib.rs` | 2.60 s | 2.79 s |
| `src-tauri/src/lib.rs` | 1.11 s | 1.16 s |
| 空跑（cargo 自身开销地板） | 0.52 s | 0.49 s |

**结论 1（验证宝典 3.4）**：profile 对热增量实测无显著差异，最大差 0.69 s，与 0.5 s 的 cargo 开销地板同量级。

**结论 2（本仓库特有）**：宝典 3.4 所说的痛点（"保存一次要等 25 秒"）在本仓库**不存在**。最坏情况——touch 那个所有人都依赖的 `skillstar-core`——全 workspace 重新 check 只要 **3.42 s**。宝典给出的唯一杠杆（3.9 拆 crate）在这里**没有可回收的收益**。

**机制：行数不是爆炸半径，入度才是**（用 `cargo check -v` 实测，不是推测）：

| touch 的 crate | 实际重编的 workspace 单元 | 个数 |
| --- | --- | ---: |
| `skillstar-core`（**2,616 行**，全仓第 9 大） | 全部 | **13** |
| `skillstar-skills` | skills, channels, sync, app, skillstar_lib, skillstar | 6 |
| `skillstar-models`（最大的 crate） | models, app, skillstar_lib, skillstar | 4 |
| `src-tauri` | skillstar_lib, skillstar | 2 |

真要动，方向是把 `skillstar-core` 里"每天都改的部分"和"几乎不动的部分"分开，**而不是拆大 crate**——但因为绝对值只有 3.42 s，现在没有必要动手。

**边界**：以上全是 `cargo check` 的数字。`cargo build` 的热增量是它的 4.6–5.3 倍（touch core：15.78 s vs 3.42 s；touch models：12.57 s vs 2.60 s）——这也是"用 `cargo check` 而非 `cargo build` 迭代"这条建议在本仓库的实测倍率。

**顺带（同一类）**：`sccache`（3.10）也不建议——CI 已有 `rust-cache`，`sccache` 与 `CARGO_INCREMENTAL` 不兼容，而单机上它打不过 cargo 增量（热增量已经是 0.5–3.4 s）。

---

## 8. 方法学与可信度边界

**这一节决定上面的数字能不能被后人直接引用。**

### 8.1 会话内漂移：最快 41.99 s vs 最慢 117.16 s（2.8 倍）

同一条基线命令（冷 `cargo check --workspace`，全新 `CARGO_TARGET_DIR`）在两小时里跑了 **9 次**：

| # | 时刻 | real (s) | 备注 |
| ---: | --- | ---: | --- |
| A1 | 00:39 | 62.55 | |
| A3 | 00:51 | 54.02 | |
| A4 | 00:55 | 77.04 | 受扰窗口 |
| A2 | 00:47 | **117.16** | 受扰窗口（最慢） |
| A5 | 01:13 | 58.29 | |
| A6 | 01:18 | 60.62 | |
| INT1 | 01:40 | 57.07 | 机器开始变安静 |
| INT2 | 01:42 | 44.63 | |
| EXT2 | 01:43 | **41.99** | 最快 |

**最快与最慢差 2.8 倍——比要检验的任何一个 profile 效应都大。** 同一现象也出现在 `cargo build` 上（95.83 s vs 69.66 s，同一条命令）。

**因此所有结论都建立在背靠背相邻配对的比值上，绝不跨时段比绝对值。** 每组 A/B 都在同一个 2–9 分钟窗口内、用基线锚点夹住：`A3…D…A4`、`A5…BO1…BO2…A6`、`INT…EXT…INT`。关键结论额外给出 `user`（CPU 秒）作为抗漂移的第二证据。

**读数提醒（重要）**：基线里"冷 check 62.55 s / 冷 build 95.83 s"这类**绝对值偏高**（机器安静时分别是 ~42 s 和 ~70 s），只用于交代量级，不参与任何结论。§7 里的三组 release 数字（425 / 299 / 149 s）同样测于较忙的窗口——**三者之间的比较有效，绝对值应当视为上界**。

同一份报告还记录了一个形态吻合的观察：审计时 `target/` 是 **77 GB / 389,311 个文件**（`incremental/` 39 GB + `deps/` 38 GB），`cargo-sweep` 未安装。宝典 3.8 记录过"一次后台文件系统回收让 137 s 的构建膨胀到 280 s"，本次观察到的 54 s → 117 s（2.2×）与那条记录形态一致——**但本次无法证明是同一原因**（也可能是并发 agent 的负载）。

### 8.2 审计期间有另一个 agent 在同一仓库提交

- 会话开始时 HEAD 是 `d6ad988`，01:15 时已是 `8a82471`（`e6e2651`、`8a82471` 两个 commit 在 00:55 左右落地，改的是 `skillstar-channels` / `skillstar-app` / `skillstar-skills` 的少量 `.rs`）。这两个 commit **没有动依赖图**，改动量相对 126k 行 / 665 个包在冷构建里 < 1 s。但第 2 轮的 A4/B2 与 A3/D/F **严格来说不是同一份源码**。
- 该 agent 在 01:28:03 还改了 `crates/skillstar-skills/Cargo.toml` 与 `Cargo.lock`（加一条 `reqwest.workspace = true`），**落在 release 三组之间**（`R_fat` 用改动前的 manifest，`R_thin`/`R_fast` 用改动后的）。逐条评估的结论是它无法解释 126 秒差值：`reqwest 0.13.3` 本就在依赖图里且只编 1 份，`reqwest.workspace = true` 不带任何 feature，不新增待编译的包也不改变 feature 并集——只多一条 DAG 边，而 `reqwest` 在 `skillstar-skills` 开始编译之前早已完成（时间线：reqwest 52.5 s 结束，skills 54.9 s 才开始）。
- 第二部审计开始时工作区有 11 个 modified 文件，结束时变成 18 modified + 4 untracked，其中**新增了一个集成测试二进制**（`crates/skillstar-app/tests/storage_maintenance_channel_ownership.rs`，00:44 创建）。因此第二部的编译秒数（24.35 s @00:36、67 s @00:47）**不能相减、也不代表冷构建**。
- 根 `Cargo.toml` 与 `.cargo/config.toml` **全程未被任何人改动**（收尾时 `git diff` 为空），所有 profile 变体都通过 `cargo --config` 命令行注入。

### 8.3 哪些数字是精确的 / 抽样的 / 明确未测

| 类别 | 具体 |
| --- | --- |
| **精确**（可复跑、结论稳） | 依赖图 BFS 与包计数；`Cargo.lock` 解析（915 / 784 / 91）；模块 SCC 邻接表与 Tarjan 结果（每条边都用 `grep -n` 回查过行号）；`cargo metadata` 派生的成员/版本/edition/依赖边；`--timings` 的 `UNIT_DATA`（单元数、feature 集、并行度曲线、逐单元差分）；孤儿模块对账（三种独立方法一致）；mermaid 图 ⇄ cargo 边（38 ⇄ 38）；链接逐次耗时 |
| **抽样 / 有系统性偏差** | 可见性过度暴露（正则 + 整词匹配，**系统性低估**，305/920 是保守下界；人工复核 5/5）；SCC 邻接表基于 `crate::X` 文本匹配（不识别 `use super::`/`self::`，**低估**边数；而 re-export 会**高估**耦合——`crate::git` 那条发现就是这么找到的）；1.6 的功能横跨面依赖挑选的标识符正则；未使用依赖用逐成员 `grep -rw`（`cargo-machete`/`udeps` 未安装）；lint 违规基数用正则按 `#[cfg(test)]` 切分生产/测试段；CI 各步骤耗时只有一次采样（run 30860357912） |
| **明确未测** | 见 §8.4 |

### 8.4 明确未测、至今悬着的问题

| # | 问题 | 为什么重要 |
| --- | --- | --- |
| **a** | **`lto` `fat` → `thin` 的运行时代价** | §7 只测了构建耗时（−126.11 s / −29.7%）和二进制体积（+2,470,096 B / +7.6%）。`thin` **已经落地**（`d9d82e3`），但运行时对照**至今没做**。要补，正确做法是跑一次启动耗时 / 技能扫描耗时的 A/B——这两条是 SkillStar 里最接近计算热路径的操作。**这是本次审计留下的最大一个悬空问题。** |
| **b** | `codegen-units = 1` 的单独代价 | `R_thin` 仍然是 `codegen-units = 1`，所以 −126 s **全部归因于** `fat`→`thin`；而 `R_fast` 同时动了两个变量，其 −275.78 s 是两者合力，**不能拆分归因**给 LTO 或 CGU 中的任一个 |
| **c** | `[profile.ci]`（`incremental = false` + `debug = 0`）的实际收益 | 只给了方向和理由，没有数字——它必须改 workflow 才能真跑。预期收益主要在**缓存体积**（冷 `test --no-run` 产出 7.9 G target dir）而非耗时 |
| **d** | 第一部**所有**结构结论的编译墙钟 | 破 SCC、抽叶子 crate、缩短 crate 链（§2.1 / §2.2 / §2.5）全部只测了依赖图形态。第三部的实测（§7.6、§6.1）反而给了下调预期的证据 |
| **e** | churn 数据 | `skillstar-core`（下游 97.6%）到底多久改一次、每次改动触发多大重编——第一部审计没有跑 git，**无法回答**。这是判断入度枢纽严重度的关键输入 |
| **f** | `unreachable_pub` 的真实基数 | 只读不可测（§2.4）。必须先 `warn` 记基数 |
| **g** | 单元测试的实际运行耗时 / 「编译:运行」倍率 | 第二部审计不能跑测试（当时判断会写入被跟踪的生成文件，见 §9 第 2 条对这个判断的修正）。后续实现批实测了 `cargo test --workspace` 通过、20 个 suite / 1336 passed，**但那是编译 + 运行的合计，倍率仍未拆开** |
| **h** | 换 `htmd` 之后 HTML 解析栈的残余重复 | 52.0 unit-seconds 是换之前的数字（§4.2） |
| **i** | 前端 1,808 行的 `SharedChannelsContent.test.tsx` | 已随新阈值进 baseline（可见、不拦），**未拆** |

---

## 9. 源材料之间的矛盾与口径差异

审计报告与后续实现报告之间有若干不一致。**以实测方为准**，逐条记下来，免得后人拿错数字。

| # | 分歧 | 以哪一方为准 |
| --- | --- | --- |
| 1 | 第一部 §1.4 写"`src-tauri` 的 `version = "0.0.4"` 是 `tauri.conf.json` 的版本来源" | **实现批实测推翻**：`src-tauri/tauri.conf.json` 自己硬编码了 `"version": "0.0.4"`，并不从 Cargo.toml 读（Tauri v2 的规则是 conf 里写了就用 conf 的，省略时才回落）。真实关系是**两处各存一份、互不引用**。"保留 `0.0.4` 不参与 workspace 继承"这个结论不变，但理由不同——而且这里藏着一个 SSOT 缺口：改版本要改两个文件，没有任何东西会提醒 |
| 2 | 第二部 §2.1 补充断言"`cargo test --workspace` 会写入 48 个**被 git 跟踪**的 `src/types/generated/*.ts`，运行即违反只读约束" | **实现批实测部分推翻**：跑完 `cargo test --workspace` 后 `git status --short -- src/types/generated` 是**完全空的**——重新生成的内容与已提交的字节一致。准确说法是"测试套件会**重写**这些文件，但只要生成器与仓库同步就不产生 diff"。审计当时因此**没有**运行测试套件，这也是"单元测试运行耗时本次无法测量"（§8.4-g）的直接原因 |
| 3 | 第四五部引用 `ci.yml` 注释里的"7 个真实漏洞、22 个 unmaintained" | **实现批实测为准**：cargo-deny 0.20.2 实测是 **3 vulnerability / 1 unsound / 19 unmaintained / 1 yanked**。`ci.yml` 的注释当时已经过时（已随实现批更正） |
| 4 | crate 行数在两份报告里不同 | 口径与时点差异，**不是矛盾**：第一部（`d6ad988`）models 21,244 / skills 21,059 / channels 19,734；第三部（`8a82471`）models 23,057 / skills 21,006 / channels 20,918。引用时必须带上口径与快照 |
| 5 | 个别文件行数差 1 行 | `discovery.rs` 959（第一部）vs 960（第二部）；`deployment/mod.rs` 969 vs 970。属统计口径（是否计末行）差异，不影响任何结论 |
| 6 | `.rs` 文件总数 403 / 404 / 405 / 407 | 时间序列而非矛盾：第四五部起点 403 → 第一部 404 → 第四五部收尾 405 → 门禁批实测 407。**引用文件数时必须带日期** |
| 7 | 第一部 §1.12 的命令注释写"30 条 intra-workspace path dep"，同一节的表格写 39 条 | **39 条为准**（实现批实测：38 条带冗余 `version` + 1 条不带 = 39）。注释里的 30 是笔误 |
| 8 | 删 24 个未使用依赖的收益，两边口径不同 | 都对，口径不同：第四五部用自写 BFS 得 758 → 750（−8）；实现批用 `cargo metadata --filter-platform` 得 678 → 668（−10），lockfile 全量 915 → 909（−6）。**三个数字描述的是三件不同的事**，引用时必须说明口径。结论一致：其余 20 个依赖会继续通过别的 crate 存在，**一个包都不会少编**——这项改动的价值是"清单不再谎报所有权"，不是构建速度 |

---

## 10. 2026-08-14 复核记录

本文成稿时对若干"仍未落地"的断言做了低成本复核，避免归档一个已经过期的快照。

| 断言 | 复核结果 |
| --- | --- |
| 13 个成员零 `[features]` 段 | **仍成立**（0） |
| `unreachable_pub` 未启用 / 无 `[workspace.lints.rust]` 段 | **仍成立**（根 `Cargo.toml` 只有 `[workspace.lints.clippy]` 三条 deny） |
| 2 个 `pub fn *_for_test` 泄漏 | **仍成立**（`skill_mutation.rs:119`、`update_state.rs:239`） |
| `crate::git::*` 的 re-export 假象 | **仍成立**，且 `gh_manager` 仍是 **0 处**。引用总数从审计快照 `8a82471` 的 61（transport 43 + ops 18，已回查该 commit 确认）变成今天的 **62**（transport 44 + ops 18）——审计之后的提交多引入了 1 处 `crate::git::transport`，方向与审计发现一致。**本文正文保留审计原值 61，此处记录复核值** |
| `tokio` 5 处 `"full"` | **仍成立**（5） |
| `skillstar-models` 的 `reqwest_012` 别名 | **仍成立**（`Cargo.toml:43`） |
| `skillstar-skills` 的 `scraper 0.27.0` | **仍成立**（`Cargo.toml:35`） |
| `GitAuthMaterial` 跨 crate 引用 | **仍成立**（`github-auth/src/lib.rs:13`） |
| `ProxyFingerprint` 双份 | **仍成立**（`core/.../http_client.rs:27`、`models/.../http_client.rs:23`） |
| `docs/adr/` 不存在 | **仍成立** |
| `README.md:122` 死链 | **仍成立** |
| `docs/decisions.md` 的 `commit 待定` | **仍成立**（现在在 `:155`） |
| 4 处引用不存在的 `docs/ROADMAP.md` | **仍成立**（`check_file_size.sh` 第 3、18、118 行 + `file_size_baseline.txt:3`） |
| AGENTS.md 列 4 个脚本 vs CI 实际跑的数量 | **仍成立且差距扩大**：AGENTS.md 4 个，`ci.yml` 现在是 **10** 个（审计时 8 个） |
| 4 个孤儿脚本零调用点 | **仍成立**（`ui_page_pass.mjs`、`gen_seed_registry.cjs`、`build_merged_latest_json.cjs`、`build_windows_cross.sh`） |

# 异形分发仓库安装

状态：**已完成并收尾**。00–06 全部落地，代码、测试、文档、真实网络验证、最终选择清单（`choices.md`）都已完成。不需要继续实现工作。最后更新：2026-09-30。

## Next Agent Prompt

本规格已经做完。`close-spec` 技能在当前环境不可用（尝试调用报 `Unknown skill`），所以归档只做到「状态清楚标记为已完成、TODO 全部勾掉、选择清单收敛成最终版」这一步，没有把 `specs/irregular-skill-packs/` 目录搬走或删除——如果这个仓库有专门的「已完成规格」归档位置或约定，接手的人可以按那个约定处理，本文件不代为决定。

如果你是被叫来验证或扩展这个功能，只需要知道：`pbakaus/impeccable` 这类「一份技能、多个 harness 改写副本」的仓库，SkillStar 现在能：
- 一个身份只物化一份代表副本（`pack_layout::choose_copy` 是唯一的选副本表）；
- harness 点击只多物化它选中的那一份；
- tree URL 子路径是硬钉（`LockEntry.pinned`），钉住后不再随 harness 或重装换副本；
- 带 ref 的 tree URL 现在也走稀疏检出，不再整仓浅克隆；
- GUI 扫描预览和 CLI 都能看到 ref/subpath，仓库是 Claude 插件时会提示 hooks/agents 不会安装。

全部记录在 [D-075](../../docs/decisions.md#d-075异形分发仓库只有一张选副本表)。真实仓库上 `--agent cursor` 的结果：cache 里的 `SKILL.md` 从 24 个降到 2 个，工作区文件从 1166 个降到 135 个；tree URL 子路径安装（`.claude/skills/impeccable`）跑通，装出 pinned 条目并部署到指定 Agent。

主工作区如果有别的 session 的未提交改动导致 `--locked` 失败，按 `choices.md`「验证环境」一节的做法，在隔离的 worktree 里验证，不要替别人更新 Cargo.lock。这台机器经常有其他并发 session 占满构建锁，`cargo check/test --workspace --locked` 排队十几分钟属于正常情况。验证门必须包含一次 `cargo check --workspace --locked`——只查所在 crate 不够，会漏掉跨 crate 的破坏（04 的教训）。不要 push。

### TODO

- [x] 00 探针与夹具 — [slices/00-probe-and-fixture.md](slices/00-probe-and-fixture.md)
- [x] 01 选副本规则只有一个 owner — [slices/01-copy-rules.md](slices/01-copy-rules.md)
- [x] 02 inventory 按名字确认身份 — [slices/02-inventory-identity.md](slices/02-inventory-identity.md)
- [x] 03 请求只物化被选中的一份 — [slices/03-request-materialize.md](slices/03-request-materialize.md)
- [x] 04 Source 贯通与子路径硬钉 — [slices/04-source-pin.md](slices/04-source-pin.md)
- [x] 05 GUI 接上 Source 与插件提示 — [slices/05-gui-plugin-hint.md](slices/05-gui-plugin-hint.md)
- [x] 06 带 ref 的缓存也走稀疏检出 — [slices/06-ref-sparse-cache.md](slices/06-ref-sparse-cache.md)
- [x] 收尾：H5 真实网络数字写进 `choices.md`，整档 review，`choices.md` 重写成最终选择清单（`close-spec` 技能不可用，未做目录归档）

## 目标

安装 `pbakaus/impeccable` 这类仓库时：

```text
一个名字只物化一份副本
→ 点某个 harness 只多物化那个 harness 的一份
→ 用户贴子路径 URL 就钉死那一份
→ 测试夹具不进 cache，也不进扫描列表
→ 仓库是 Claude 插件时提示 hooks/agents 不会安装
```

今天的实际情况：
- impeccable 的 20 份发布副本内容各不相同，D-063 的「内容不同的副本一律物化」让首装物化约 20 份。
- `materialize_deferred_matching` 的过滤条件是 `name_hit || prefix_hit`，点任意一个 harness 就会把全部同名的延迟副本物化出来。
- 子路径 URL 在安装时被忽略，GUI 扫描和安装还会丢掉 ref。

## 仓库事实（2026-09-29 用 gh api 核实）

- 24 个 `SKILL.md`：
  - 18 个点目录副本：`.agent .agents .claude .cursor .dsh .gemini .github .grok .hermes .kiro .opencode .pi .qoder .rovodev .trae .trae-cn .veto .vibe`，路径都是 `<dir>/skills/impeccable`。
  - `plugin/skills/impeccable` 和 `cursor-plugin/skills/impeccable`。
  - 4 个测试夹具：`tests/oracle/workspaces/ctx-pin/{.agents,.claude,.cursor}/skills/impeccable`（`description: fixture`）和 `.../.claude/skills/audit`。
- **仓库根没有 `skills/`**，所以不指定 harness 时，新规则选中的代表副本是 `.agents/skills/impeccable`。
- 20 份副本的 `name` 都是 `impeccable`，但 tree SHA 两两不同：
  - frontmatter 各有差异：`.agents` 把 version 放进 `metadata`，cursor 去掉了 `user-invocable`。
  - 正文写死各自的路径：`.claude/skills/impeccable/scripts/...`、`.cursor/...`；plugin 版用 `${CLAUDE_SKILL_DIR}`。
- `.claude-plugin/marketplace.json` 的 `source` 是 `./plugin`。`.claude-plugin/plugin.json` 的 `"skills": "./.claude/skills/"` 是一个字符串形式的**容器**路径。插件本身带 `plugin/hooks/hooks.json` 和 `plugin/agents/*.md`。
- `scripts/impeccable` 在 git 里是 100755。
- 真正的源码是 `skill/SKILL.src.md`，它不是技能，现在已被正确忽略。

## 同类工具怎么处理（2026-09-29 读源码）

| 工具 | 策略 | 装 impeccable 的结果 |
|---|---|---|
| vercel-labs/skills | 先查优先目录，按 frontmatter name 去重，先到先得 | 1 个 `.agents` 版；项目级 `update` 扫到 20 个同名，判为歧义，永远跳过 |
| runkids/skillshare | walk 时跳过所有 harness 点目录；另有 `plugin add` 读 marketplace.json，交给原生 `claude plugin install` | 1 个（plugin 或 cursor-plugin）；只有它能装上 hooks/agents |
| farion1231/cc-switch | 下载 zip，按完整路径去重 | 20 多张同名卡片，可执行位丢失 |
| numman-ali/openskills | 无限递归，不去重 | 24 个候选，同名副本叠加覆盖，更新后只剩夹具空壳 |
| xingkongliang/skills-manager | 回退到 `skill/` 目录 | 0 个 |
| qufei1993/skills-hub | 固定几个路径 | 1 个 `.claude` 版，所有工具共用 |
| jiweiyeah/Skills-Manager | 根目录有 README 就把整仓当一个技能 | 装进 3000 多个文件，不能用，可执行位丢失 |

结论：没有一家做到「按目标 harness 选副本」，这一点 SkillStar 已经领先。本方案补的是「只物化一份」「钉住」「规则只有一个 owner」。hooks/agents 只做提示，理由见下面的非目标。

## 全局决定（和用户定的，不重开）

1. **范围 B**：只保证安装与发现的正确性。每个身份只物化一份；harness 请求只物化需要的那一份；补上 `.gemini/skills`；修正 plugin.json 字符串的容器语义；安装时认子路径；跳过测试夹具目录；插件 hooks/agents 只提示。
2. **身份**：先按 basename 分组；组内 tree SHA 有两种以上时，只读各候选的 `SKILL.md` blob，取 frontmatter `name`。name 相同就是同一身份的变体，只物化代表副本，其余延迟；name 不同就是不同技能。
3. **排名表只有一张**，由 `pack_layout` 持有。不指定 harness 时的顺序：根 > `skills/` 或 `source/skills/` > `.agents/skills` > manifest 声明的容器 > 其他，同级按字典序。已安装技能的 `source_folder` 仍走现有链路，保持被选中。
4. **子路径 URL 是硬钉**：只发现、只物化这个子路径，写进 lock；这个技能之后不再随 harness 换副本，更新也跟着这个路径。
5. **跳过测试夹具目录**：全递归发现和 inventory 都跳过 `tests/`、`test/`、`__tests__/`、`fixtures/`。判断只看**祖先路径段**；子路径明确指向这些目录内部时例外。
6. **不做兼容，不做迁移**：直接硬切。inventory sidecar 的格式升级后，旧文件丢弃并重新规划；已有 lock 的 `source_folder` 不改。

## 实施中替用户默认的决定（可在审查点推翻）

| 决定 | 默认 | 理由 | 被否掉的方案 |
|---|---|---|---|
| `.agent` 是否与 `.agents` 同级 | 不同级，`.agent` 归入「其他」 | 决定 3 的表里只有 `.agents`；也顺带消除 errors.md 2026-08-14 那类字典序陷阱 | 保持同级：需要在表里多加一条特例 |
| 读 name 失败 | 这一组退回 D-063 的旧规则（只延迟 tree SHA 相同的副本） | 永远不比今天差，绝不丢技能 | 当成同一身份：可能误删一个不同的技能 |
| 批量预取 blob | 由 00 的 P0a 结果决定：一次往返能行就在 02 做批量；不行就逐个懒取，并加总时限 | 草案之间分歧最大的一点，拿数字定 | 01 就直接写批量（草案 A）：没有证据 |
| 已在磁盘上的副本 | **粘滞**：重新规划时只增不减，已物化的技能目录保留 | 被钉住的 Agent 链接直接指向 cache 里的副本，而 `installed_source_folders` 只读 lock（`deployment/mod.rs:99-134`，`repo_scanner/ops.rs:68-86`）。一旦收缩 sparse，链接就会悬空 | 在 sidecar 里另记 `on_demand` 字段（草案 C）：多一份状态，而且覆盖不到钉住的链接 |
| 来源规格怎么传 | `Source` 加 serde，由 `ScanResult` 平铺带回前端，安装时整份回传（`&Source` 贯通到底） | 一次安装里 `Source::parse` 调了约 4 次，GUI 还把 ref 丢了；收成一个 owner | 前端传原始 URL，后端再解析一次（草案 A/B）：diff 小，但解析逻辑仍然散在各处 |
| 用普通 URL 重装一个已钉住的技能 | 不解钉；只有卸载才解钉 | 钉住是用户的明确意图 | 普通 URL 重装即解钉：意图难以判断 |
| `reconstruct_lock_entry` 丢失钉住标记 | 接受，写进决策条目的「后果」 | 这是罕见的修复路径，决定 6 不做迁移 | — |
| 决策条目 | [D-075](../../docs/decisions.md#d-075异形分发仓库只有一张选副本表)，随各档逐步补全；D-063、D-045、D-044 标注「部分被取代」 | SSOT | 每档各写一条 |

## 非目标（记录，不做）

- 安装 hooks/agents。只提示「这是插件」；要完整插件就用 `/plugin marketplace add`。自己实现 hook 注入，等于把 impeccable 自带的 installer 重写一遍。
- launcher 运行时下载的二进制会弄脏 cache 快照：用户没选范围 D。impeccable 的 README 写的是下到 `~/.impeccable/bin/`，大概率不受影响。
- tarball cache 刷新时会整仓下载（草案 B 的 F5，`cache.rs:142-163`）：违反 D-063，另开一个 PR。
- 频道侧的 `collapse_pack_identity_copies` 会把 `plugin/` 和 `cursor-plugin/` 判成重复身份：这是既有问题，本次不改它的语义，只把排名换成新表。
- 用 frontmatter 的 `version` 判断更新：更新继续比较子树的 tree hash。impeccable 各副本都写了 version，但 lock 里跟踪的是目录内容。
- `PRIORITY_SKILL_DIRS` 补 `.grok .hermes .rovodev .trae-cn .veto`：harness 请求走的是全深度发现，不受影响；只补决定 1 点名的 `.gemini`。

## 单一 owner 不变量

每个概念只允许一个 owner。任何一档引入平行实现，都算没做完。

| 概念 | owner | 要删掉的平行实现 |
|---|---|---|
| 选哪份副本 | `pack_layout::choose_copy` | `pack_layout::{source_priority, discovered_folder_priority, harness_folder_rank}`、`discovery::{better_nested_copy, select_canonical_skill, select_nested_skill_copy, select_preferred_nested_skill}`、`inventory::choose_representative` |
| 身份键 | `pack_layout::identity_key` | inventory 里按小写 basename 的分组 |
| 忽略目录 | `pack_layout::IGNORED_DIR_NAMES` + `is_under_ignored_dir` | `discovery::find_all_skill_md_files` 里的局部 `SKIP_DIRS` |
| 稀疏集合的应用 | `inventory::apply` | `cache.rs:174-192`、`cache.rs:442-451`、`repo_scanner/ops.rs:44-60` 这三份复制 |
| 延迟副本的匹配 | `Inventory::copy_for` / `materialize_for` | `SparsePlan::deferred_matches`、`materialize_deferred_matching` |
| 子路径作用域 | `SkillDiscovery::within` | `scan.rs:22-52` 手工拼前缀 |
| 来源规格 | `source_resolver::Source` | `(short, repo_url)` 字符串对；`ScanResult{source, source_url}` 各自单独传 |

终态检查：读代码的人应该感觉不到「这是后来补上的」。异形仓库只是 `choose_copy` 表里的一行，不是一条特例分支。

唯一允许的过渡状态：00 把 git/write/commit 测试小工具放进 `pack_fixture.rs`，但 `inventory/tests.rs:5-39` 和 `skill_install_harness_tests.rs:81-122` 里还各有一份。02 改 inventory 测试时迁走前者，03 改 harness 测试时迁走后者。03 结束时，这两个文件里不再有私有的同名小工具。

## 为什么切成 7 档

三份草案的切法：A 只切 2 档，B 切 8 档以上，C 切 9 档。

- **没有采用 A 的 2 档**：它把「排名表」和「inventory 行为」放进同一档，结果 H1（看表）和 H2（真网络实测）两个审查点只能挤在一起；而且批量预取在没有证据的情况下就写进去了。
- **没有采用 C 的 9 档**：排名表、忽略目录、`.gemini`、plugin.json 这四件都是纯函数，审查面是同一张表，所以合并成 01。
- **采用了 B 的做法**：先跑探针（00），再决定预取方式和 06 做不做。

三份草案都独立指出、因此可以放心采用的点：
- 排名表由 `pack_layout` 独占；
- inventory 格式升级到 2；
- `.claude-plugin` 固定物化；
- 命中 sidecar 时也要合并 installed 集；
- 合并父目录时不能吞掉延迟的兄弟目录；
- 删除 `materialize_deferred_matching`；
- 拆出 `install_choice.rs`；
- `pinned: bool`；
- 钉住后返回 `Reuse`；
- `.agent` 与 `.agents` 不同级。

## 切片图

```text
00 探针 + 夹具
 └─► 01 选副本规则（排名表 · 忽略目录 · .gemini · plugin.json）   [H1]
      └─► 02 inventory 身份（按名字 · 粘滞 · .claude-plugin · 格式 2） [H2 真网络]
           └─► 03 请求级物化（harness 只物化一份 · 拆 install_choice）
                └─► 04 Source 贯通 + 子路径硬钉                    [H3]
                     ├─► 05 GUI + 插件提示                          [H4]
                     └─► 06 带 ref 的缓存走稀疏检出（看 P0b）       [H5]
```

## 审查点（都不阻塞）

打开产物，给用户约 5 分钟回应；没有回应就按证据决定，把理由写进 `choices.md`，然后继续。

| 点 | 时机 | 看什么 |
|---|---|---|
| H0 | 00 后 | P0a/b/c 的数字 |
| H1 | 01 后 | `copy_selection_table` 测试里的那张表 |
| H2 | 02 后 | 真实 impeccable 的 fetch 次数、耗时、cache 里 `SKILL.md` 的份数 |
| H3 | 04 后 | lock 里 `pinned` 的形态；钉住后点其他 harness 的行为 |
| H4 | 05 后 | GUI 用 tree URL 扫描再安装的截图，以及插件提示 |
| H5 | 06 后 | 带 ref 的子路径安装的下载量 |

## 验证门

- 每一档：先跑该档列出的 `cargo test -p skillstar-skills --locked <filter>`。
- 每一档合入前：
  - `cargo check --workspace --locked`
  - `cargo test --workspace --locked`
  - `bash scripts/internal/check_file_size.sh`（重点看 `skill_install.rs`，现在 953 行）
- 动到 `skillstar-git` 时：`bash scripts/internal/check_workspace_deps.sh`。
- 动到前端或命令层时（05）：`bun run lint && bun run build && bun run test`、`check_command_boundaries.sh`、`check_feature_imports.sh`。
- `bun run types:gen` 不需要跑：`ScanResult` 和 `SkillInstallTarget` 的 TS 类型是手写的（`src/types/marketplace.ts:87`、`src/types/skill.ts:181`）。PR 描述里说明这一点。
- 下面这些回归测试任何一档都必须保持绿：
  - `installed_impeccable_deepseek_falls_back_to_a_skill_folder`
  - `installed_rust_skills_deepseek_retargets_from_cache_without_clone`
  - `missing_git_cache_still_fetches_for_harness_install`
  - `install_pipeline_table_chooses_harness_or_fallback_folder`
  - `pack_root_shim_installs_canonical_skills_folder`
  - `stale_dsh_link_is_rewritten_to_requested_harness`
  - `second_harness_install_deploys_that_harness_folder`
  - `adding_a_duplicate_provider_path_does_not_look_like_source_removal`（`skill_update/tests/source_dropped.rs`）
  - `pipeline_fetches_stale_cache_when_requested_skill_is_missing`
  - `cargo test -p skillstar-channels --locked`

## 文档落点

| 文档 | 改什么 | 哪档 |
|---|---|---|
| `docs/decisions.md` | 新条目「异形分发仓库：单一选副本 owner、名字身份、子路径硬钉」；D-063 的「内容不同一律物化」和排名那句、D-045 的「保留全部嵌套父目录」、D-044 的 plugin.json 字符串语义，分别标注被部分取代 | 01 起逐档补 |
| `docs/features/skills/README.md` | :41 inventory、:46 source_folder 跟随 harness（钉住例外）、:47 去重、:51 发现规则、:52 tree URL；排名只链接到决策条目，不抄表 | 01–05 |
| `docs/errors.md` | harness 请求按名字物化全部同名副本；plugin.json 字符串被当成技能路径、测试碰巧通过 | 03、01 |
| `README.md` | tree URL 示例旁加一句「子路径会钉住该副本」 | 04 |

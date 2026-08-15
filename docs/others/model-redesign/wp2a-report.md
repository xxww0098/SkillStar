状态：historical

# WP-2A 报告：v4 接入运行路径

本文件记录 WP-2A 的实施结果与取舍，供后续工作包引用。长期有效的结论已分别落进
[decisions.md](../../decisions.md)（D-038、D-039）、[boundaries.md](../../boundaries.md)、
[architecture.md](../../architecture.md) 和 [features/models/README.md](../../features/models/README.md)；
这里只保留「这一包做了什么、为什么这样做、留下了什么」。

## 1. 磁盘格式切换的实际路径

启动入口是 `providers::store_v4::load_store_and_repair(path)`，由 `get_providers_flat` 唯一调用：

```
read_store_v4 → 已是 v4？直接返回
             ↓ 否
            文件存在？否 → 空 store（首次运行）
             ↓ 是
            先确认能解析成 JSON（否则 Corrupted，不动文件）
             ↓
            双备份（rolling + 永久 model_providers.v3.json），任一失败即中止
             ↓
            migrate_store_if_needed（v1→v2→v3 链，原样复用）
             ↓
            migrate_v3_to_v4（纯函数）
             ↓
            写盘 → 读回校验 → 不一致则从备份还原
             ↓
            catalog_cache::persist_extracted（先于 re-sync，OpenCode writer 要读它）
             ↓
            tool_sync::repair_agent_configs（§3.3）
             ↓
            再次写盘（丢弃的 Codex 条目是 store 变更，只留在内存会在重启时复活）
```

其余命令走不做修复的 `providers::load_store()`——修复会重写 Agent 配置文件，那是一次性动作，
不该挂在每条命令上。

**同时修掉的一个隐患**：`migrate_store_if_needed` 的 v1 分支是**排除法**到达的（不是 v3、不是 v2
就当 v1），而 v1 结构每个字段都有 serde 默认值。于是一个 v4 文件会被**成功**解析成四个空桶，
`backup_and_write` 随即把这个空 store 覆盖回用户的真实配置——一次降级启动就足够。现在遇到高于
`FLAT_STORE_VERSION` 的版本直接 `bail!` 并保留原文件。测试：
`store_v4::the_v3_reader_refuses_a_v4_file_instead_of_emptying_it`。

## 2. tool_sync 端口

生产文件里 v3→v4 的改动点约 120 处（`agents.rs` / `sync.rs` / `multi_provider.rs` /
`omp_provider.rs` / `types.rs` / `backup_merge.rs` / `mod.rs`）。writer 签名统一从
`(&ToolBinding, &[ProviderEntryFlat])` 变为 `(&AgentBinding, &[Provider])`。

字段映射集中在新增的 `tool_sync/view.rs` 一处，**不散在各 writer 里**——这也是字节对照能成立的前提：
每个函数都是一个 v3 字段的 v4 拼法，对应关系本身就是契约。

| v3 | v4 |
| --- | --- |
| `provider.base_url_openai` | `endpoints.openai_chat` |
| `provider.base_url_anthropic` | `endpoints.anthropic_messages` |
| `provider.api_key` | `credential.literal_secret()` |
| `provider.default_model` | `default_model` |
| `provider.meta.model_catalog` | `catalog_cache::read_catalog(id)` |
| `provider.meta.claude_*_model` | `binding.roles["fast"/"sonnet"/"opus"]` |
| `binding.settings.roles` | `binding.roles`（`ModelRef`） |
| `provider.codex_wire_api` | 删除（恒为 `responses`） |
| `provider.codex_auth_mode` | `entry.settings.auth_mode` |
| `is_native_official_provider(p)` | `p.is_external_cli()` |
| `AgentSpec.required_url` | `AgentSpec.required_wire`（`RequiredWire`） |

## 3. 「字节不变」是怎么证明的

**基线是跑出来的，不是写出来的。** 从 `0fab682` 拉了一个 worktree，用同一份 fixture 跑 **v3 的
writer**，把产物原样存进 `crates/skillstar-models/src/tool_sync/tests/golden_v3/`（7 个文件）。
新测试拿同一份 fixture 走真实 `migrate_v3_to_v4`、再走新 writer，逐字节比对。

fixture 覆盖：两个 provider（一个有 anthropic 端点一个没有）、Claude 三档模型、
`model_catalog`（含 `display_name`/`limit`/`cost`/`source_name`）、未知 meta 键、
三个 OMP 角色（含 thinking 后缀与无后缀）、multi binding 且 `active_index = 1`。

`golden_v3/` 已从 biome 的管辖范围排除——格式化它就等于销毁它的用途。

**这个对照当场抓到三个真实回归**，没有一个能被手写断言发现：

1. **OMP 角色名**：迁移把 `smol` 规范化成 `fast`，writer 直接把 `fast` 写进 `config.yml`——OMP
   不认识这个键，用户的 `smol` 路由被删、多出一个 OMP 忽略的角色。修复是加 `migrate::omp_role_key`
   反向映射，双向由 round-trip 测试锁定。
2. **OMP 角色顺序**：`modelRoles` 是有序映射，store 内部改名让 YAML 无故重排。修复是按 OMP 的角色名排序。
3. **OpenCode 模型元数据**：catalog 移出 store 后 `limit` / `cost` / `name` 全部消失。修复是
   `catalog_cache` 的读侧，并把 catalog 落盘排在 re-sync **之前**。

Codex 是唯一豁免，且豁免范围收窄到「本来就写 `responses` 的 `api.openai.com` 行仍然逐字节相同」；
它该变的部分在 `part6` 里按行为逐条断言。

## 4. Codex 修复的影响面

- 注册表 13 条 preset 中，**11 条**（deepseek / kimi / kimi-coding / minimax / glm / glm-coding /
  longcat / xiaomi-mimo / openrouter / siliconflow / grok）都只有 chat 端点，**不再投影给 Codex**。
  这正是 00-coordinator-notes §1.1 里被测试锁死的那八个 provider 的超集。
- 2 条原生登录种子（`claude-official` / `codex-official`）**不受影响**：`Credential::ExternalCli`
  在门禁里豁免，空端点是它们的语义而不是缺陷。
- 未经探测仍可投影给 Codex 的只有 `api.openai.com` 的行。其余要等 WP-2B 的探测把
  `Tri::Unknown` 变成 `Yes`。
- 判定依据是**端点存在性**，`Tri::Unknown` 从不拒绝（R-2）——迁移给每一行写的都是 `Unknown`，
  把「没探测过」当「不支持」会在升级时静默解绑所有人。只有探测得到的 `No` 才拒绝。

## 5. 有意的取舍（后续包需要知道）

- **IPC 线上形状仍是 v3**。store 与所有 writer 都是 v4，翻译集中在
  `src-tauri/src/commands/models_commands/compat.rs` 一处。理由：把前端 IA-2 一起做会让 store bug
  和渲染 bug 在同一次改动里长得一样；而直接换成 `ProviderDto` 会让 provider 编辑器读不到 key，
  那是行为回退不是修复。**明文 key 因此仍然过 IPC，与改动前一致**——它随 WP-4 的编辑器一起消失。
- 写入走**打补丁**而非重建：v3 表达不了的 `caps` / `headers` / key 故障转移链 / `ext` 不会在每次
  保存时被清空。测试：`compat::tests::a_patch_preserves_everything_v3_cannot_express`。
- **`providers/crud.rs` 已删除**，不是并存。v3 的 CRUD 只剩迁移输入这一个用途，留着两套会让
  「哪个是真的」变成读者的问题。
- **catalog 缓存的读侧在本包实现**（WP-2B 拥有三级来源策略）。不实现读侧就会静默剥掉所有人
  `opencode.json` 里的模型元数据——那是回退，不是「留给下一个包」。

## 6. 未做（留给后续包，避免冲突）

- 角色泛化到所有 Agent 的写盘（WP-3）。`AgentBinding.roles` 类型已就位，Claude 的三档与 OMP 的角色
  已从新位置读取，但把角色接到其余 Agent 属 WP-3。
- 模型目录三级回退（WP-2B）。
- 前端 IA-2 重写（WP-4）。前端只改到「能编译、能跑通、行为不退化」。
- `docs/errors.md` 的 `wire_api` 条目（附录 B 归 WP-3）。该文件在本次工作区里有**与本包无关的
  脏改动**，按派工约束未触碰；根因与自检方法已先写进
  [features/models/README.md](../../features/models/README.md)。

## 7. 验收结果

全部 10 项绿：

| 命令 | 结果 |
| --- | --- |
| `cargo check --workspace --locked` | Finished，0 warning |
| `cargo test --workspace --locked` | 20 个测试二进制全绿，0 failed（skillstar-models 415 + 11 + 4） |
| `bun run types:gen` | 新增 `src/types/generated/PresetCategory.ts`，其余无 diff |
| `bun run lint` | Checked 494 files，No fixes applied |
| `bun run build` | ✓ built |
| `bun run test` | 93 files / 703 tests 全绿 |
| `check_workspace_deps.sh` | OK |
| `check_command_boundaries.sh` | 0 regression |
| `check_file_size.sh` | 0 new over-limit |
| `check_generated_types.sh` | up to date |
| `check_ts_orphan_modules.sh` | 0 new orphan |

新增的关键用例：

- 往返：`store_v4::a_migrated_store_survives_a_restart_with_every_field_intact`、
  `the_v3_reader_refuses_a_v4_file_instead_of_emptying_it`、
  `a_store_from_an_unknown_future_version_is_refused_rather_than_migrated`
- 字节对照：`tool_sync::tests::golden::*`（5 个）
- Codex：`tool_sync::tests::part6::*`（11 个）

## 8. 过程中修掉的一个测试隔离缺陷

`catalog_cache` 最初用 `SKILLSTAR_DATA_DIR` 环境变量做测试沙箱。环境变量是**进程全局**的，而
libtest 一个测试一个线程：一个测试设它会把同时在跑的其它测试一起改根，而它的 temp dir 被 drop 后，
幸存者就在写一个已经不存在的目录——这直接导致两个 golden 测试写进了开发者真实的
`~/.skillstar/cache/`（已清理）。

改成 **thread-local override**（与 `tool_sync` 的 sandbox home 同构），并加两道防线：
`#[cfg(test)]` 下即使没装 override 也落到进程级 temp dir 而非真实 data root；`golden.rs` 的
`migrated()` 把 `&DataDirSandbox` 收成参数，让「忘了拿 guard」变成编译错误而不是约定。
连跑 3 次全绿。

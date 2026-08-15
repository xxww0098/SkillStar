# WP-1 完成报告｜v4 类型、迁移与生成面

状态：**部分完成**。已交付的部分全部跑绿（验收命令输出见 §4），但 **v4 尚未接入运行路径**——`crud` / `tool_sync` / 命令读写仍在 v3 上。这是本包最重要的一条分歧，理由写在 §3.1。改动全部留在工作区，**未 `git add`、未 `git commit`**。

---

## 1. 交付了什么

| # | 内容 | 位置 | 测试 |
| --- | --- | --- | --- |
| 1 | v4 域类型（§2.3 全部） | `providers/{provider,credential,binding,catalog}.rs` | 随下方各项覆盖 |
| 2 | v3→v4 纯函数迁移 + 迁移报告 | `providers/migrate/{v3_to_v4,report}.rs` | **29 passed**（含 2 个 proptest） |
| 3 | store 安全外壳（双备份 / 写后读回 / G6） | `providers/store_v4.rs` | **8 passed** |
| 4 | 前端 DTO 投影（剥离明文凭据） | `skillstar-app/src/models/dto.rs` | **10 passed** |
| 5 | R-8 后端取 key | `providers/secret_resolve.rs` + `models_commands/diagnostics.rs` | **6 passed** |
| 6 | ts-rs 生成面 | `src/types/generated/` 新增 28 个文件 | `check_generated_types.sh` 绿且幂等 |
| 7 | `check_generated_types.sh` 过期注释 | 已修（§3.5） | — |
| 8 | 文档：D-035 / D-036 / D-037 + 两处 README + boundaries | `docs/decisions.md` 等 | `check_dep_graph_doc.sh` 绿 |

新增文件行数（1000 行上限，四个类型模块另有 < 400 行的要求）：

```
262  providers/provider.rs          212  providers/credential.rs
264  providers/binding.rs           224  providers/catalog.rs
263  providers/store_v4.rs          248  providers/secret_resolve.rs
592  providers/migrate/v3_to_v4.rs  111  providers/migrate/report.rs
728  providers/migrate/tests.rs     327  skillstar-app/src/models/dto.rs
```

四个类型模块都在 400 行以内。`v3_to_v4.rs` 592 行——它是迁移而非类型模块，且低于 800 行的拆分触发线。

### 1.1 v4 类型的最终形状

与 §2.3 的差异只有四处，都是补充而非删减，逐条列在 §3.2。核心形状照方案落地：

- `Provider` + `Endpoints`（每协议一个 `Option<String>`，取代两个硬编码 URL 字段）+ `ProviderCaps`（三态 `Tri`）+ `ext`（唯一的无 schema 口袋）。所有时间字段带 `_ms`。
- `Credential` 判别联合六变体。`ExternalCli` 取代 v3 靠 id 白名单在六处分支的 Native Official 特例——`Provider::is_external_cli()` 是一次 `matches!`。
- `AgentBinding.roles: BTreeMap<String, ModelRef>` 一等字段。`ModelRef` 是三元组 `(provider_id, model, effort)`。
- catalog 两层拆分（`Serving` 是「这家怎么卖」，`ModelFacts` 是「模型本身」），且**不进** `model_providers.json`。

### 1.2 迁移覆盖的 v3 形状

`migrate_v3_to_v4(FlatProvidersStore, &[ProviderPresetFlat]) -> MigrationOutcome`，纯函数：无 IO、无时钟、无网络。catalog 只被**抽出**交给调用方落盘（写失败不中止——catalog 可重建，绑定不可）。

`migrate_provider` 对 `ProviderEntryFlat` **完全解构**，v3 类型加字段会在这里编译失败，而不是静默不迁移。

覆盖的形状：

| v3 输入 | v4 输出 | 测试 |
| --- | --- | --- |
| `base_url_openai` / `_anthropic` / `models_url` 空串 | `Endpoints` 的 `None` | proptest |
| `api_key` 非空 | `Credential::ApiKey`（单键，uuid 化 id） | `migrate_maps_api_key_to_a_single_key_credential` |
| `api_key` 空 + 有端点 / 无端点 | `None{LocalService}` / `None{NativeLogin}` | `migrate_distinguishes_a_local_service_from_a_native_login` |
| `claude-official` / `codex-official` | `ExternalCli{surface}` | `migrate_maps_official_seeds_to_external_cli` |
| `codex_wire_api` | **丢弃**（断言序列化结果不含 `wire_api` 与 `"chat"`） | `migrate_discards_the_dead_codex_wire_api_field` |
| `codex_auth_mode` | 下沉到 `BindingEntry.settings`，entry 已有值时不覆盖 | 2 个测试 |
| `meta.claude_{haiku,sonnet,opus,main}_model` | `roles["fast"/"sonnet"/"opus"/"default"]` | 2 个测试 |
| `settings.roles`（OMP 9 个 thinking level） | `roles` + `ModelRef.effort` | 逐值断言，见 §2.2 |
| `entries[].last_sync_at`（秒） | `last_sync_at_ms`（×1000） | `migrate_converts_last_sync_seconds_to_milliseconds` |
| `meta.model_catalog` | 抽出到 `MigrationOutcome.catalogs` | `migrate_lifts_the_model_catalog_out_of_the_store` |
| `meta.baseURL`（v1 遗留） | 丢弃 | `migrate_parks_unrecognised_meta_keys_in_ext_and_names_them` |
| `meta` 其余键 | `ext` + 记进 `report.preserved_ext_keys` | 同上 |
| `tool_activations["claude-desktop"]` | 丢弃 + 进 `report.dropped_bindings` | `migrate_drops_the_claude_desktop_binding_and_names_it_in_the_report` |

v1→v2→v3 链**未改动**，`store_v4` 复用它再接 v3→v4：没升级过的老用户仍能一路到 v4。

---

## 2. 三条硬性约束的实测结果

### 2.1 三条件回填（§3.2.4）

6 个用例，正反都覆盖：

| 场景 | 三条件 | 结果 |
| --- | --- | --- |
| preset 有值 + openai URL 与 preset 逐字相同 + anthropic 空 | ①②③ 全真 | **回填**，并进 `report.backfilled_anthropic` |
| **用户改过 openai URL**（指向自己的中转） | ③ 假 | **不回填**（`migrate_leaves_user_edited_urls_alone`） |
| 行已有 anthropic 值 | ① 假 | 不回填，保留用户的值 |
| preset 本身没有 anthropic | ② 假 | 不回填 |
| 无 `preset_id` | 无 preset 可依 | 不回填 |
| `models_url` | 同一条规则 | 回填并单独记账 |

条件 ③ 是全部论证所在：它区分「前端 bug 造成的空」（行其余部分未被动过 → 补上才是用户本来要的）与「用户主动清空」（这行已经是他自己的配置 → 补上等于覆盖他的决定）。

### 2.2 `caps` 一律写 `Unknown`（R-2）

`migrate_writes_unknown_caps_never_no` 对每个迁移出的 provider 断言三个能力位**都不是** `Tri::No`，且 `probed_at_ms` 为 `None`。proptest 里对任意输入行同样断言这三条。唯一写 `Tri::Yes` 的是 `api.openai.com`——它是离线可知的唯一事实。

`Tri::is_denied()` 只对 `No` 返回真，是 UI 该不该禁用绑定入口的唯一判据；`Unknown` 永远意味着「需要检测」。

OMP 九个 thinking level 逐值断言（`migrate_maps_omp_thinking_levels_onto_canonical_effort`）：`off→None`、`minimal/low/medium/high/xhigh/max` 一一对应，**`inherit` 与 `auto` → 不设 effort**——它们是「延后决定」的指令而不是档位，写成档位等于把用户留浮动的值钉死。非法值同样不猜。

### 2.3 迁移安全（R-1 / G6）

- `corrupted_store_returns_error_and_keeps_file`：损坏文件返回 `StoreError::Corrupted`，并断言**文件字节与原来完全一致**。v3 在这里返回空 store，紧接着的写盘就会用空 store 覆盖用户的全部配置。
- `migration_aborts_when_the_backup_cannot_be_written`：把目录设成只读让备份失败（源文件仍可读——这正是 v3 只 warn 就继续的场景），断言返回 `BackupFailed` 且 v3 文件完好可运行。
- `migration_takes_both_backups_before_writing`：rolling `.bak.<ms>` 与永久 `model_providers.v3.json` 各一份，且断言永久备份的**字节等于迁移前原文**。
- `a_second_migration_does_not_clobber_the_permanent_v3_copy`：第二次迁移不覆盖已有的 v3 快照——否则唯一的退路会被一份已降级的副本毁掉。
- `migration_round_trips_through_disk`：迁移→读回→再 load 不二次迁移；`write_then_read_preserves_every_v4_only_field` 断言 roles / caps / credential 变体全部往返无损。
- 写后读回不一致时从永久备份还原（`verify_or_restore`）。

### 2.4 DTO 不含明文（R-8 的另一半）

`provider_dto_never_contains_plaintext_secret` 对 `serde_json::to_string(&dto)` 断言不含密钥原文，单键与多键（含 disabled 的 fallback 键）两种形状都测。另外：

- 掩码规则 `sk-a••••mnop`；**≤8 字符的短 key 整串替换**——露出六位里的四位不叫掩码。
- `EnvVar` / `File` / `Command` 的 summary 是指针本身（`$OPENAI_API_KEY`）而非掩码：变量名不是秘密，藏起来只会让用户看不出 key 从哪来。
- `From<Provider>` 完全解构（D-034），`ext` 显式丢弃并注明理由。

---

## 3. 分歧与未完成项

### 3.1 【最重要】v4 未接入运行路径

**做了什么**：v4 类型、迁移、store 外壳、DTO 全部实现并测试，`load_or_migrate_store_v4` 可用。
**没做什么**：没有把它接进启动流程。`crud.rs` / `tool_sync/*` / `models_commands/*` 仍读写 `FlatProvidersStore`（v3）。

**为什么停在这里**：切换磁盘格式和改写盘方是两个独立的高风险改动，叠在一起会产生一个无法分别验证的状态。具体地——一旦 `load_or_migrate_store_v4` 跑过，磁盘上是 `version: 4`；而现存的 `migrate_store_if_needed` 看到非 3 的版本会当作 v1 `ProvidersStore` 解析并失败。所以「接入」不是加一行调用，而是必须**同时**把下列全部改完才能编译并正确运行：

- `crud.rs`（569 行，24 处引用）——且 §2.4 要求同时拆命令语义（`activate_tool` → `bind_provider` / `set_active_binding`；`deactivate_tool` → `unbind_provider` / `unbind_agent`）。
- `tool_sync/{sync,multi_provider,omp_provider,types,agents,backup_merge}.rs`——约 60 处字段读取，且 §3.3「已写盘配置的 re-sync」（清掉磁盘上 `wire_api = "chat"` 的 Codex 表）**只能**在 writer 里做，那是 WP-3 的文件集。
- `providers/tests/part1-5` + `tool_sync/tests/part1-4` + 两个 proptest 集成测试——约 4000 行里遍布 `ProviderEntryFlat { .. }` 字面量。
- 前端：`src/types/models.ts` 退化为 barrel、`useProvidersFlat` / `activations.ts` / `providerPatch.ts` / 整个 matrix UI 的字段读取——这些正是 WP-4 要重写的文件。

**留给后续包的桩**：`store_v4.rs::read_legacy_flat_store` 是唯一的显式桥接点，doc 注明「WP-3 移除；一旦 `tool_sync` 与 `crud` 说 v4 就没有调用方了」。`ResolvedConnection::from_v4` 已写好，与 `from_v3` 并排放置，切换时删掉后者即可。

**建议**：把「v4 接入 + crud 命令语义拆分 + tool_sync 端口 + §3.3 re-sync」作为一个独立工作包派发，它天然与 WP-3 是同一批文件。

### 3.2 v4 类型对 §2.3 的四处补充

| 补充 | 理由 |
| --- | --- |
| `ProviderCaps.source: CapSource{Preset,Probe,UserOverride}` | §2.3 的表格要求「UI 必须显示当前值来自哪一级」，但类型里没有承载它的字段。加了一个枚举，否则那条 UI 要求无处实现。 |
| `RequiredWire` 枚举 + `Provider::endpoint_for()` | `Endpoints` 有四个字段，「这个 Agent 该读哪一个」需要一个具名的问法，否则每个调用点各写一次 match。 |
| `Credential::ExternalCli.surface` 用 `String` 而非 `&'static str` | 方案原文是 `&'static str`，但该类型要 `Deserialize`（它在磁盘上），`&'static str` 无法从任意文档反序列化。 |
| `Effort` 未派生 `Default` | 「不设 effort」由外层 `Option<Effort>` 表达，`Effort::None` 是「显式请求 none」——两者是不同的请求（关掉思考 vs 思考档位=none）。给 `Effort` 一个默认值会让这个区分立刻塌掉。 |

另有一处**格式选择**：v4 类型不加 `#[serde(rename_all = "camelCase")]`，字段保持 snake_case。理由是 04 §6.1 第 13 条记录过的事故——`FlatProvidersResponse` 独自加了 `rename_all` 导致 `tool_activations` 变成 `toolActivations`，前端所有消费者读 snake_case，于是该字段永远是 `undefined`。v4 是新格式，选 snake_case 与现有前端消费习惯一致，且生成的 TS 类型字段名与磁盘一致。

### 3.3 R-8 实际改了 6 条命令而非 4 条

任务列了 `test_provider_connection` / `fetch_provider_models` / `query_provider_balance` / `test_endpoints_latency`。另外两条同属一类且同样收明文 key，一并改了：`fetch_provider_model_catalog`、`test_provider_latency`。留一半在旧签名上没有意义。

三处顺带修掉的东西：

- `models_url` 的兜底规则（空则用 `base_url_openai + "/models"`）此前前端一份、后端一份；现在只在 `ResolvedConnection::models_endpoint()` 里有一份。
- `test_provider_connection` 之前对 anthropic 格式的探测也用 openai base URL；现在按 format 选端点。
- `query_provider_balance` 的 `preset_id` 之前由前端传回；现在从行上读。

**行为变化（需要 WP-4 知道）**：探测的是**已保存**的连接。`AppAiModelsPicker` 的草稿态因此改成「先落盘再探测」——这与 §4.5「凭据显式提交」的保存策略一致，但当前 provider drawer 仍是 600ms autosave，两者语义要在 WP-4 对齐。

### 3.4 未做的 §5 WP-1 条目（逐条）

| 条目 | 状态 | 归属 |
| --- | --- | --- |
| `providers/types.rs` 标 `pub(crate)` | **未做**——现有 v3 消费者仍需要它 `pub` | 随 3.1 的接入包 |
| `crud.rs` 按 §2.4 拆命令语义 | 未做 | 同上 |
| `models_commands` 换成生成 DTO / `FlatProvidersResponse` 替换 | 未做 | 同上 |
| `src/types/models.ts` → re-export barrel | 未做（需前端先消费 `ProviderDto`） | 同上 / WP-4 |
| `ai_provider` 的 `app_id` → `agent_id` + `ai.json` 迁移 | **未做** | 建议单独派，它有自己的磁盘文件与迁移 |
| `ProviderPreset.category` 拆 `PresetCategory` 枚举 | 未做 | 同 3.1 |
| 删 `test_get_all_presets_flat_count` | 未做（该测试仍锁着 v3 preset 表，现在删掉会降低覆盖） | 同 3.1 |
| §3.3 已写盘 Agent 配置 re-sync | **未做**（只能在 writer 里做） | WP-3 |

### 3.5 `check_generated_types.sh` 注释的错误陈述

原文写「`crates/skillstar-models/src/providers/types.rs` (ProviderPreset)」携带 `#[derive(TS)]`。实测 `grep -n 'derive(.*TS' providers/types.rs` **零命中**——该文件从来没上过生成面。已改为如实描述：`types.rs` 只是迁移读的历史形状，真正上生成面的是 `providers/{provider,binding,catalog,credential}.rs` 与 `migrate/report.rs`，并补记了新增的 `skillstar-app/src/models/dto.rs`。

---

## 4. 验收命令实际输出

```
########## cargo check --workspace --locked ##########
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.24s

########## cargo test --workspace --locked ##########
20 个 "test result: ok" 段，0 failed、0 error、无 failures 段
（本包新增：migrate 29 passed / store_v4 8 passed / secret_resolve 6 passed / app models::dto 10 passed）

########## bun run types:gen ##########
no diff residue on tracked files      # 重跑一次后 git diff --exit-code src/types/generated/ 通过
新增 28 个生成文件（ProviderDto / Endpoints / Tri / ModelRef / Effort / Reasoning / MigrationReport …）

########## bun run lint ##########
Checked 494 files in 151ms. No fixes applied.        # exit 0

########## bun run build ##########
✓ built in 8.79s                                      # tsc 无 error

########## bun run test ##########
 Test Files  93 passed (93)
      Tests  703 passed (703)

########## 结构门禁 ##########
check_workspace_deps           OK      check_command_boundaries       OK
check_file_size                OK      check_generated_types          OK
check_ts_orphan_modules        OK      check_no_orphan_modules        OK
check_i18n_hardcoded           OK      check_feature_imports          OK
check_dep_graph_doc            OK      check_error_strings            OK
```

新增依赖：`thiserror`（经 `cargo add thiserror -p skillstar-models`，走 workspace 版本）——`StoreError` 的四个变体需要它，`check_workspace_deps.sh` 绿。

五个无关脏文件（`repo_scanner/ops.rs`、`skill_update/tests/source_dropped.rs`、`docs/errors.md`、`DeckCard.tsx`、`Marketplace.tsx`）未被触碰，`git diff --stat` 与开工前一致（171 insertions / 46 deletions）。

---

## 5. 给协调者的建议

把剩余部分作为**一个**工作包派发，而不是两个：「v4 接入运行路径 + crud 命令语义拆分（§2.4）+ tool_sync 端口 + §3.3 已写盘配置 re-sync」。它们共享同一批文件（`providers/crud.rs`、`tool_sync/*`、`providers/tests/*`、`tool_sync/tests/*`），拆开必然互相冲突——这正是原方案给 WP-1 写的那条冲突提示所预言的。`ai_provider` 的 `app_id → agent_id` 与 `ai.json` 迁移则可以独立并行，它不碰 provider store。

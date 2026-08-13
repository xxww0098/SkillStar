# MCP 现状与缺口盘点（T2）

状态：historical / 一次性盘点快照
盘点日期：2026-08-13
盘点范围：`crates/skillstar-marketplace`（商店层）、`crates/skillstar-models/src/mcp`（安装/投影层）、`src-tauri/src/commands`（编排层）、`src/features/mcp` + `src/pages`（前端层）、以及全部 MCP 相关测试。

> 本文只做**只读盘点**，不改行为。所有结论均带 `file_path:line_number` 证据。
> SSOT 规则依 [AGENTS.md](../../AGENTS.md)：工具清单以 `McpToolSpec` 注册表及其测试为准，本文只描述、不复制枚举计数（引用的计数均标注了其代码/测试出处）。
> 长期有效的边界与运行拓扑见 [docs/boundaries.md](../boundaries.md) 与 [docs/architecture.md](../architecture.md)；行为契约见 [docs/features/mcp/README.md](../features/mcp/README.md)。

---

## 0. 一句话总览

SkillStar 的 MCP 由**两条互不相通的数据链**组成：

1. **发现链**（marketplace）：GitHub MCP Registry → `mcp_registry_server` / `mcp_curated_server`（SQLite）→ 卡片/详情 → `registry_to_entry` 生成草稿。
2. **安装链**（models）：`mcp_servers.json` 统一 store → 每个 Agent 工具的原生配置文件（7 个公开 target）。

两条链之间**只有一次性的草稿转换**（`mcp_market_entry_to_draft`），安装后不再保留任何来源指纹：`McpServerEntry` 里没有 registry id、没有版本、没有 source（`crates/skillstar-models/src/mcp/types.rs:47-126`）。这是当前所有"升级/更新/弃用提示"能力缺失的**根因**。

---

## A. 商店 / 远程层：`crates/skillstar-marketplace/`

### A.1 远程抓取 `src/mcp_remote.rs`

| 维度 | 现状 | 证据 |
| --- | --- | --- |
| 数据源 | `https://api.mcp.github.com/v0/servers`，公开、免鉴权 | `crates/skillstar-marketplace/src/mcp_remote.rs:16` |
| HTTP client | 强制走 `probe_http_client`（遵守用户代理配置），符合架构红线 | `crates/skillstar-marketplace/src/mcp_remote.rs:22-25`；实现见 `crates/skillstar-core/src/infra/http_client.rs:108` |
| 超时 | 单一 30s（连接+读取合并），无分级、无重试 | `crates/skillstar-marketplace/src/mcp_remote.rs:17` |
| 分页 | `limit=100`，游标 `metadata.next_cursor`，硬上限 25 页 | `crates/skillstar-marketplace/src/mcp_remote.rs:18-20,33-37,80-93` |
| 游标编码 | 自写最小 percent-encoding（不引依赖） | `crates/skillstar-marketplace/src/mcp_remote.rs:115-126` |
| hash | SHA-256 覆盖**所有分页 body 的字符串拼接**，用于内容寻址跳过全量重写 | `crates/skillstar-marketplace/src/mcp_remote.rs:66,78,86` |
| ETag | 恒为 `None`，从未利用服务端 ETag | `crates/skillstar-marketplace/src/mcp_remote.rs:88,106` |
| 错误处理 | `error_for_status()` 后整链 `?`，**任一页失败则整次同步失败**，已抓到的页全部丢弃 | `crates/skillstar-marketplace/src/mcp_remote.rs:46-47,69-71` |

**缺口 A.1-a（静默截断）**：命中 25 页上限时只 `warn!` 一行，然后仍返回 `Ok` 且 `degraded: false`（`crates/skillstar-marketplace/src/mcp_remote.rs:96-110`）。下游 `replace_servers` 会用这份被截断的目录**全量替换**本地目录（`crates/skillstar-marketplace/src/mcp_snapshot/query.rs:23-26`），且 `marketplace_sync_state.degraded_reason` 从来不写（见 A.3-c）。即：registry 超过 2500 条时，用户会静默丢失尾部条目，且事后无法从 DB 判断这次同步是否完整。

**缺口 A.1-b（无重试/无部分成功）**：第 N 页 429/超时 → 整次 fetch 失败 → `mark_error`，本地目录保持上一版（`crates/skillstar-marketplace/src/mcp_snapshot/mod.rs:267-275`）。行为上安全，但对弱网用户等于"永远同步不成功"，且没有指数退避。

### A.2 模型与解析 `src/mcp_models.rs`

**类型**（全部 `camelCase` serde + ts-rs 导出）：

- `McpServerKind`：`stdio | remote | both | unknown`，`as_db_str`/`from_db_str` 为 DB 存储契约（`mcp_models.rs:20-49`）。
- `McpRegistryPackageSummary`：只有 `runtime` / `identifier` / `version` / `required_env`（`mcp_models.rs:52-64`）。
- `McpRegistryRemoteSummary`：只有 `transport` / `url` / `required_headers`（`mcp_models.rs:67-77`）。
- `McpRegistryServer`（快照行）：`mcp_models.rs:81-111`，含 `raw_server_json` + 本地 `recommended` / `source`。
- `McpMarketEntry`（卡片）/ `McpMarketServerDetail`（详情，flatten entry + readme + packages + remotes）：`mcp_models.rs:114-146`。
- `McpPublisherSummary`：`id`（= curated `source` 或 `"github"`）/ `name` / `server_count` / `url`：`mcp_models.rs:153-165`。

**`parse_servers_response` / `parse_registry_element` 实际解析的字段**（`mcp_models.rs:348-434`）：

| server.json 字段 | 是否落到结构化列 | 证据 |
| --- | --- | --- |
| `name` | ✅ → `namespace`，末段 → `name` | `mcp_models.rs:350-351,225-232` |
| `id` | ✅（缺失时回落 `namespace`） | `mcp_models.rs:356` |
| `description` | ✅ | `mcp_models.rs:357` |
| `repository.url` / `repository.readme` | ✅ | `mcp_models.rs:358-362` |
| `version_detail.version` \| `version` | ✅ | `mcp_models.rs:363-366` |
| `updated_at` \| `created_at` | ✅（二选一挤进同一列） | `mcp_models.rs:367` |
| `packages[]` | ⚠️ 仅 4 个派生字段 | `mcp_models.rs:261-284` |
| `remotes[]` | ⚠️ 仅 3 个派生字段 | `mcp_models.rs:286-305` |
| `x-github.stars` / `.license` | ✅（三处兜底路径） | `mcp_models.rs:307-334` |

**缺口 A.2-a（packages 信息大量丢弃）**：`parse_packages` 只保留 `runtime`（由 `registry_type`/`runtime_hint` 派生）、`identifier`、`version`、以及 `environment_variables` 中 `is_secret || is_required` 的**名字**（`mcp_models.rs:266-283`，过滤逻辑在 `mcp_models.rs:244-259`）。被丢弃的有：

- `environment_variables[].description / default / choices / format / is_required` 与 `is_secret` 的**布尔值本身**（只剩名字，UI 无法区分"必填"与"密钥"）；
- `runtime_arguments` / `package_arguments`（结构化列完全没有；仅安装时从 `raw_server_json` 现读，见 `src-tauri/src/commands/mcp_marketplace.rs:142-162,218-219`）；
- `registry_base_url`、`file_sha256`、`transport`（package 级 transport）；
- 多 package 的差异：卡片只用 `runtimes` 去重列表（`mcp_models.rs:377-382`），安装只取 `packages[0]`（`src-tauri/src/commands/mcp_marketplace.rs:308-321`）。

**缺口 A.2-b（remotes 信息丢弃）**：`parse_remotes` 只保留 `transport`（归一化为 `http|sse`）、`url`、必填 header 的**名字**（`mcp_models.rs:286-305`）。丢弃 header 的 `value` 模板、`description`、`is_secret`。注意 header 的 value 模板在**安装路径**又被从 raw JSON 读回来（`src-tauri/src/commands/mcp_marketplace.rs:266-278`），即同一份信息两套解析、结构化层缺失。

**缺口 A.2-c（version / deprecation 完全没有语义）**：

- `version` 只作为展示字符串（`mcp_models.rs:363-366`），没有比较、没有"有新版本"通道。
- registry 的 `status`（`active` / `deprecated`）、`_meta.io.modelcontextprotocol.registry/official.isLatest` **完全没有被解析**：全仓 `crates/skillstar-marketplace/src/mcp_*` 与 `crates/skillstar-models/src/mcp/` 下 grep `deprecat|isLatest|"status"` 零命中。因此弃用服务器会与正常服务器一起展示、一起可安装。

**缺口 A.2-d（`raw_server_json` 只存 `server` 对象）**：`mcp_models.rs:385` 序列化的是 `server_object(element)` 的结果，**不含信封上的 `x-github`**。因此 stars/license 一旦从结构化列丢失就无法从 raw 恢复；反过来 `server` 内部的 `status`/`websiteUrl`/`_meta` 虽然留在 raw 里，但既不可查询也不可展示。

### A.3 快照层 `src/mcp_snapshot/`

#### Schema（`mcp_snapshot/mod.rs:47-112`）

两张**结构完全对称**的表 + 两个 FTS 虚表：

- `mcp_registry_server`：`id`(PK), `name`, `namespace`, `description`, `repo_url`, `stars`, `license`, `version`, `kind`, `runtimes_json`, `readme`, `packages_json`, `remotes_json`, `raw_server_json`, `updated_at`, `fetched_at`（`mod.rs:49-66`）。
  - 唯一索引：`idx_mcp_registry_stars ON (stars DESC)`（`mod.rs:67`）。
- `mcp_registry_server_fts`：fts5(`id`,`name`,`namespace`,`description`)，`tokenize='unicode61'`（`mod.rs:69-75`）。
- `mcp_curated_server`：上表所有列 **+** `source`(默认 `skillstar-curated`)、`is_recommended`、`priority`（`mod.rs:77-97`）。
  - 索引：`idx_mcp_curated_recommended_priority ON (is_recommended DESC, priority ASC, name ASC)`（`mod.rs:98-99`）。
- `mcp_curated_server_fts`：同构 fts5（`mod.rs:101-107`）。

**注意**：两张表的 `id` 各自为 PK，但**跨表没有唯一约束**；去重靠查询里的 `WHERE id NOT IN (SELECT id FROM mcp_curated_server)`（`query.rs:208,285,351`）。

#### 迁移

- v8 建 registry 表：`crates/skillstar-marketplace/src/snapshot/migrations.rs:428-431`。
- v10 建 curated 表（复用同一个 `create_mcp_registry_tables`，因为它是 `IF NOT EXISTS` 幂等的）：`migrations.rs:471-474`。
- v11/v12 给共享的 `marketplace_sync_state` 加 `source_host`/`payload_sha256`/`etag`/`degraded_reason`：`migrations.rs:491-510,528-542`。
- 当前 `SNAPSHOT_SCHEMA_VERSION = 12`，`user_version` 只在整条链跑完后才 bump：`crates/skillstar-marketplace/src/snapshot/mod.rs:29`、`migrations.rs:97-135`。

**缺口 A.3-a（curated 表列结构变更不可迁移）**：`create_mcp_registry_tables` 全部是 `CREATE TABLE IF NOT EXISTS`（`mod.rs:47-107`）。存量用户的 DB 已有 v10 建的 `mcp_curated_server`，**再往这个函数里加列不会生效**，必须新写一个 `ALTER TABLE ADD COLUMN` 迁移（参照 v11/v12 的 `column_exists` 幂等模式：`migrations.rs:491-527`）。这是本次改造最容易踩的坑。

#### 写入路径

- `replace_servers`（`query.rs:19-84`）：`DELETE FROM mcp_registry_server` + `DELETE FROM ..._fts` + 逐行 INSERT，单事务全量交换。
- `seed_default_curated_mcp_servers`（`mod.rs:129-224`）：`INSERT ... ON CONFLICT(id) DO UPDATE SET`（全列覆盖）+ FTS 先删后插；单事务。
- **每次读之前都会重新 seed**：`list_curated_mcp_servers`(`mod.rs:283`)、`list_mcp_servers_local`(`mod.rs:291`)、`search_mcp_servers_local`(`mod.rs:363`)、`get_mcp_server_detail_local`(`mod.rs:441`)、`get_registry_server_local`(`mod.rs:483`)、`list_mcp_publishers`(`mod.rs:493`)、`list_mcp_servers_by_publisher`(`mod.rs:507,521,542`)。

**缺口 A.3-b（seed 的写放大）**：每次列表/详情/发布者读取都执行一次全量 curated upsert + FTS 重建事务（`mod.rs:129-224`）。curated 行数由代码注册表决定（见 `mod.rs:658-668` 测试注释与 `mod.rs:751` 的 publisher 计数断言），量不大但这是**每次读都拿写锁**，与已知的 "MCP registry sync 失败 database is locked" 故障同源（背景见 `migrations.rs:437-446` 的注释）。

**缺口 A.3-c（degraded 从不写）**：`mark_success_with_meta` 写 `source_host`/`payload_sha256`/`etag`，**没有 `degraded_reason` 列**（`query.rs:534-567`）。v12 加的这一列对 MCP scope 永远是 NULL，因此 A.1-a 的截断不可观测。

#### 查询能力（`query.rs`）

| 能力 | 现状 | 证据 |
| --- | --- | --- |
| 全量列表 | curated UNION ALL registry，排序 `recommended DESC, sort_priority ASC, stars DESC, name ASC`，**无 LIMIT / 无 OFFSET** | `query.rs:264-298` |
| 搜索 | 双 FTS UNION ALL + `bm25(...,0.0,8.0,4.0,2.0)`，同一排序键 + `LIMIT ?` | `query.rs:315-365` |
| 搜索表达式 | 只保留 alnum token，每个 `"term"*` 前缀通配并 AND；空查询回落到全量截断 | `query.rs:302-313,320-324` |
| 按发布者 | curated 按 `source =`，github 读 registry 表并排除 curated id | `query.rs:197-227` |
| 详情/全量行 | 先查 curated 再查 registry | `query.rs:397-437` |
| 发布者聚合 | curated 按 `source` GROUP BY，顺序由**硬编码的 `CURATED_ORDER` 常量**决定，GitHub 永远最后 | `query.rs:96-193`（常量在 `query.rs:99-140`） |

**缺口 A.3-d（筛选维度几乎为零）**：SQL 层**没有**按 `kind`、`runtimes`、`license`、`recommended`、`updated_at`、`stars` 区间的任何过滤入口；排序键写死在 SQL 字符串里，不可参数化（`query.rs:287,353`）。前端只能拿到全量后在内存里过滤（见 D 节）。

**缺口 A.3-e（无分页）**：`load_cards` 没有 LIMIT（`query.rs:264-298`），`list_mcp_servers_by_publisher("github")` 也没有（`query.rs:201-212`）。GitHub registry 全量（含 `readme` 之外的所有卡片列）一次性跨 IPC 序列化到前端。

#### `seeds/` 目录用途

`mcp_snapshot/seeds/` 是 curated 卡片的**代码内种子数据**，不是文件资源：

- `seeds/mod.rs:32-` `default_curated_mcp_servers()` 聚合各发布者工厂，返回带 `priority` 的有序种子列表。
- `seeds/helpers.rs:18-51` `make_stdio_curated` / `:59-` `make_remote_curated`：统一构造 `McpRegistryServer`，其中 `raw_server_json` **手写成 GitHub registry `server.json` 形状**，从而复用同一条 `registry_to_entry` 安装路径（设计意图见 `seeds/mod.rs:9-11`）。
- `seeds/bigmodel.rs`、`seeds/publishers.rs`：各发布者的具体条目。

**缺口 A.3-f（`recommended` 实际只有 1 条，导致 18 条预置 preset 变成死代码）**：全仓 `recommended: true` 只出现在 `seeds/mod.rs:91`（AdsPower）；`seeds/helpers.rs:49,89` 与 `seeds/bigmodel.rs:54` 都写死 `false`。而 `get_mcp_presets` 命令的逻辑是：curated 列表非空 → 走 curated 分支 → `.filter(|s| s.recommended)`（`src-tauri/src/commands/mcp_commands.rs:233-238`）。由于 curated 列表永远非空，`skillstar_models::mcp::get_mcp_presets()` 里手写的 18 条 preset（`crates/skillstar-models/src/mcp/types.rs:241-404`）**在生产中永远不会被返回**，前端 preset 芯片区实际只有 1 个。这是一个当前就存在的行为 bug 级缺口。

---

## B. 安装 / 投影层：`crates/skillstar-models/src/mcp/`

### B.1 `types.rs` — `McpServerEntry` 全字段语义

`crates/skillstar-models/src/mcp/types.rs:47-126`：

| 字段 | 语义 | 行 |
| --- | --- | --- |
| `id` | UUID v4，创建时生成 | `types.rs:48-49`，生成于 `store.rs:115` |
| `name` | **逐字写入各工具配置的 server key**（不做任何清洗） | `types.rs:50-51` |
| `transport` | `stdio`(默认) / `http` / `sse` | `types.rs:52-54,128-130` |
| `command` / `args` / `env` / `cwd` | stdio 启动规格 | `types.rs:57-64` |
| `url` / `headers` | http/sse 规格 | `types.rs:67-70` |
| `description` / `homepage` / `tags` | 纯本地元数据，**不投影到任何工具** | `types.rs:73-78` |
| `enabled: BTreeMap<String,bool>` | per-tool 开关；另可携带 legacy tombstone | `types.rs:80-84` |
| `auto_approve_all` | 仅 Kiro 有等价物（`autoApprove:["*"]`），其余工具无效 | `types.rs:86-92` |
| `auto_approve_tools` | 仅 Kiro | `types.rs:93-97` |
| `disabled_tools` | Kiro `disabledTools` + Codex `disabled_tools` | `types.rs:98-101` |
| `timeout_ms` | OpenCode `timeout`(ms)；Codex 折算成 `tool_timeout_sec` | `types.rs:102-114` |
| `sort_index` | 前端排序用 | `types.rs:116-117` |
| `created_at` / `updated_at` | epoch ms | `types.rs:118-125` |

**缺口 B.1-a（无来源指纹）**：没有 `source_id` / `registry_id` / `installed_version` / `source_kind` 任一字段。安装后与市场条目**彻底失联**，无法做：更新检测、弃用提示、"已安装"精确匹配（当前前端只能按 `name` 字符串比对，见 `src/pages/McpPublisherDetail.tsx:66`）。

**缺口 B.1-b（`McpStore.version` 写而不读）**：`McpStore { version: u32 }`（`types.rs:453-459`，Default = 1）。全仓 `crates/skillstar-models/src/mcp/*.rs` 中对 `.version` 的读取为**零命中**——没有任何版本闸门、没有向前/向后兼容判断。

### B.2 `registry.rs` — `McpToolSpec` 与 7 个 tool_id 事实表

`McpToolSpec` 结构（`crates/skillstar-models/src/mcp/registry.rs:21-38`）：`id` / `label` / `resolve_config_path` / `installed` / `count_live` / `read_servers` / `upsert` / `remove`。

注册表 `MCP_TOOL_SPECS`（`registry.rs:42-123`，顺序 = 展示顺序，由 `registry.rs:145-148` 的测试钉死等于 `MCP_TOOL_IDS`）：

| tool_id | label | 配置路径 | installed 探测 | wire format | 行 |
| --- | --- | --- | --- | --- | --- |
| `claude-code` | Claude Code | `~/.claude.json`（`tools.rs:21-24`） | 二进制/桌面 App/`~/.claude`/`~/.claude.json` 四选一（`tools.rs:91-98,109-116`） | JSON `mcpServers.<name>` | `registry.rs:43-52` |
| `codex` | Codex | `resolve_codex_config_path`（tool_sync 复用） | `~/.codex` 存在 | TOML `[mcp_servers.<name>]` | `registry.rs:53-62` |
| `grok` | Grok | `~/.grok/config.toml`（`tools.rs:42-45`） | `~/.grok` 存在 | TOML（无 `type`，http 用 `headers`） | `registry.rs:63-72` |
| `opencode` | OpenCode | `resolve_opencode_config_path` | `~/.config/opencode` 或已有 opencode.json（`tools.rs:102-107`） | JSON `mcp.<name>`，`local`/`remote` 形 | `registry.rs:73-82` |
| `zcode` | ZCode | `~/.zcode/cli/config.json`（`tools.rs:13-16`） | `~/.zcode` 存在 | JSON `mcp.servers.<name>`（canonical 形）+ 顺带清理 v2 旧条目 | `registry.rs:83-102` |
| `kiro` | Kiro | `~/.kiro/settings/mcp.json`（`tools.rs:48-51`） | `~/.kiro` 存在 | JSON `mcpServers.<name>` + `autoApprove`/`disabledTools` | `registry.rs:103-112` |
| `cursor` | Cursor | `~/.cursor/mcp.json`（`tools.rs:54-57`） | `~/.cursor` 存在 | JSON `mcpServers.<name>` | `registry.rs:113-122` |

隐藏的 legacy cleanup id（**刻意不在注册表**，由 `registry.rs:160-164` 测试钉死）：`claude-desktop`（`types.rs:25`，路径 `tools.rs:28-32`）、`gemini`（`types.rs:30`，路径 `tools.rs:36-39`）。二者只在 `resolve_mcp_config_path`（`tools.rs:64-69`）与 `remove_server_from_tool_inner`（`sync.rs:87-94`）里特判。

### B.3 `specs.rs` — wire format 转换与 canonical 覆盖度

| 工具 | 转换函数 | 相对 canonical 的增量 | 行 |
| --- | --- | --- | --- |
| canonical | `canonical_spec` | stdio: `type/command/args/env/cwd`；http/sse: `type/url/headers` | `specs.rs:13-42` |
| claude-code | `claude_code_spec` | **无**（原样 canonical） | `specs.rs:48-50` |
| cursor | `cursor_spec` | **无** | `specs.rs:56-58` |
| zcode | `zcode_cli_spec` | **无** | `specs.rs:121-123` |
| kiro | `kiro_spec` | `+autoApprove`、`+disabledTools` | `specs.rs:64-78` |
| opencode | `opencode_spec` | 自有形状：`local`/`remote`、`command` 数组、`environment`、`enabled:true`、`+timeout` | `specs.rs:85-116` |
| grok | `grok_toml_table` | TOML，**不写 `type`**，http 用 `headers` | `specs.rs:126-164` |
| codex | `codex_toml_table` | TOML，写 `type`，http 用 `http_headers`；`+disabled_tools`、`+tool_timeout_sec`(ms/1000, 下限 1) | `specs.rs:171-223` |

**缺口 B.3-a（三个 approval 字段的覆盖率极低）**：`auto_approve_all` / `auto_approve_tools` 只有 Kiro 生效（`specs.rs:69-75`）；`disabled_tools` 只有 Kiro + Codex（`specs.rs:74-76,210-217`）；`timeout_ms` 只有 OpenCode + Codex（`specs.rs:112-114,218-221`）。表单对全部 7 个 target 无差别展示这些字段（见 D 节），实际写入时对 4 个 target 静默丢弃。代码注释已诚实记录（`specs.rs:44-47,52-55,80-84`），但**UI 层没有 per-target 的能力提示**。

**缺口 B.3-b（`description`/`homepage`/`tags` 永不投影）**：`canonical_spec` 与两个 TOML builder 都不写这三个字段（`specs.rs:13-42,126-164,171-223`）。反向 `import` 也读不回来，因此从工具导入 → 再导出会丢描述。

### B.4 `tools.rs` — 路径解析 / installed 探测 / 备份与 merge 写入

- 路径解析全部走 `sync_home_dir()`（受 `SKILLSTAR_TOOL_SYNC_HOME` 控制），符合 AGENTS.md 测试约束：`tools.rs:13-57`。
- `resolve_mcp_config_path` 对未知 id `bail!`（`tools.rs:63-74`）；`tool_installed` 对未知 id 返回 false（`tools.rs:79-87`）。
- `tool_statuses()` 逐 spec 汇总 `config_path` / `installed` / `server_count`（`tools.rs:185-201`），`count_live_servers` 对任何读失败一律返回 0（`tools.rs:169-182`）。
- 备份：`backup_if_exists` → `create_rolling_backup`，文件名 `<path>.bak.<epoch_ms>`，保留最近 5 份（`tools.rs:207-213`；实现 `crates/skillstar-models/src/tool_sync/backup_merge.rs:11-27`）。
- merge 语义：只 upsert/remove **一个** server key，其余 JSON/TOML 字段保留（`tools.rs:236-261,293-315,318-354,390-409`）。

**缺口 B.4-a（畸形配置被静默清空 —— 最高危）**：`read_json_object` 在**文件读失败或 JSON 解析失败时返回空 Map**（`crates/skillstar-models/src/mcp/tools.rs:224-233`）。随后 `json_mcpservers_upsert`（`tools.rs:236-249`）在这份空 Map 上加一个 `mcpServers`，再 `write_json_pretty` **整文件覆盖**（`tools.rs:285-290`）。后果：若用户的 `~/.claude.json`（同时承载 Claude Code 的大量非 MCP 配置）存在语法错误或临时不可读，一次 MCP 开关就会把整个文件替换成 `{"mcpServers":{...}}`。同样问题在 `codex_upsert`（`tools.rs:319-326`，`.ok().unwrap_or_default()`）、`opencode_upsert`（`tools.rs:293-304`）、`zcode_cli_upsert`（`tools.rs:390-396`）。
唯一做了 fail-closed 的是 legacy Desktop Chat 的 `json_mcpservers_remove_strict`（`tools.rs:265-283`，解析失败即 `?` 返回错误、原文件不动），说明团队已识别过这个模式但**只应用在了 legacy 清理路径**。缓解：写前有备份（`sync.rs:51`），但用户无任何提示。
`ensure_mcp_servers_map` 是另一个正面例子——`mcp` 字段非对象时拒绝写入（`tools.rs:355-378`），但它只覆盖 zcode 的 `mcp` 键，不覆盖顶层根对象。

**缺口 B.4-b（installed 探测过宽/过窄）**：Claude Code 只要四个信号任一命中即视为已安装（`tools.rs:91-98`）；其余工具只判断一个目录是否存在（`registry.rs:57,68,106,116`）。既可能对未真正安装的工具写文件，也可能因为目录名变化而误判未安装从而静默 skip（见 B.5-a）。

### B.5 `sync.rs` — project / remove 的原子性

#### 完整安装链路时序（文字版）

以「用户在市场点击安装」为例：

```
[前端] McpMarketCard.onInstall(id)
   └─> McpPublisherDetail.handleMcpInstall  (src/pages/McpPublisherDetail.tsx:129-140)
        └─> invoke("mcp_market_entry_to_draft", {id})
             └─[Rust] mcp_marketplace::mcp_market_entry_to_draft  (src-tauri/.../mcp_marketplace.rs:97-104)
                  ├─ mcp_snapshot::get_registry_server_local(id)   (mcp_snapshot/mod.rs:481-486)
                  │    └─ with_conn → seed_default_curated + load_full_server (curated 优先)
                  └─ registry_to_entry(&server)                    (mcp_marketplace.rs:282-325)
                       ├─ serde_json::from_str(raw_server_json)   ← 解析失败静默变 Value::Null (mcp_marketplace.rs:283)
                       ├─ name = sanitize_key(server.name)         (mcp_marketplace.rs:112-129)
                       └─ packages[0] ? fill_stdio : remotes[0] ? fill_remote : 空草稿
                                                                   (mcp_marketplace.rs:308-322)
        └─> 打开 DrawerShell + McpServerForm(defaults=draft)       (McpPublisherDetail.tsx:305-327)

[用户填密钥 / 勾选 target] → McpServerForm.handleSubmit          (McpServerForm.tsx:125-163)
   └─> 前端校验：name 非空 / remote 需 url / stdio 需 command      (McpServerForm.tsx:127-138)
   └─> invoke("create_mcp_server", {entry})
        └─[Rust] mcp_commands::create_mcp_server                   (mcp_commands.rs:83-100)
             ├─ 取 McpWriteLock                                    (mcp_commands.rs:88)
             ├─ read_mcp_store(path)   ← 畸形文件静默返回 default!  (store.rs:19-44)
             ├─ create_server(&mut store, entry)                   (store.rs:110-129)
             │    ├─ validate_entry                                (store.rs:71-99)
             │    ├─ 重名检查（按 name）                            (store.rs:112-114)
             │    ├─ uuid / created_at / updated_at / sort_index   (store.rs:115-125)
             │    └─ enabled.retain(is_supported_tool)             (store.rs:126)
             ├─ write_mcp_store(&store, path)  ← tmp + rename 原子  (store.rs:47-64)
             └─ sync_server_public_tools(&created, force=false)     (sync.rs:118-130)
                  对 MCP_TOOL_IDS 中每个 tool：
                    enabled ? sync_server_to_tool : remove_server_from_tool
                      sync_server_to_tool                           (sync.rs:12-44)
                        ├─ !is_supported_tool → error 结果
                        ├─ !force && !tool_installed → success=true, skipped=true  ← 静默跳过
                        └─ inner: validate → resolve path → backup → upsert        (sync.rs:46-54)
             └─ 返回 McpServerWithSync { server, sync_results }
[前端] failedMcpSyncCount>0 → toast.warning("部分同步失败", count)  (McpManager.tsx:154-159)
```

#### 原子性与失败模式清单

| # | 失败点 | 现状后果 | 有无回滚 | 证据 |
| --- | --- | --- | --- | --- |
| F1 | store 文件畸形 | 静默当成空 store，随后一次写入**永久覆盖用户全部 MCP 配置** | ❌ 无备份、无提示 | `store.rs:19-44`（`Ok(McpStore::default())`）+ `store.rs:47-64` |
| F2 | 目标工具配置畸形 | 整文件被覆盖为只含 `mcpServers` 的新 JSON | ⚠️ 有 rolling backup，但不提示、不自动恢复 | `tools.rs:224-233,236-249,285-290` |
| F3 | create 时部分 target 写失败 | store **已提交**，前端只弹 warning 计数 | ❌ | `mcp_commands.rs:92-95`（先 write_mcp_store 再 sync）+ `McpManager.tsx:154-157` |
| F4 | update 时部分 target 写失败 | `*store = next` 无条件执行；失败只在 results 里 | ❌ | `sync.rs:278-288`（`sync_server_all_tools` 返回 `Vec` 而非 `Result`） |
| F5 | rename 时旧名清理失败 | ✅ `ensure_cleanup_succeeded` bail，store 不提交，可重试 | ✅ 唯一有闸门的路径 | `sync.rs:213-225,273-277` |
| F6 | delete 时部分 target 清理失败 | bail 且 store 不提交，但**已成功删除的工具配置不会恢复** → store 与实际配置不一致 | ⚠️ 半回滚 | `sync.rs:305-310` |
| F7 | 工具未安装 | `success=true, skipped=true` 静默跳过，store 里 `enabled=true` 但配置未写 | ❌ 状态漂移 | `sync.rs:30-34` |
| F8 | legacy tombstone 清理失败 | update/delete/toggle 均 bail | ✅ | `sync.rs:183-211,245-252,303-304,329-330` |
| F9 | registry `raw_server_json` 损坏 | `Value::Null` → 草稿无 command/url → 提交时才被 `validate_entry` 拒绝 | ❌ 无早期反馈 | `mcp_marketplace.rs:283` + `store.rs:71-99` |
| F10 | 并发写 | 仅 Tauri 命令层用 `McpWriteLock` 串行化；域函数本身不加锁 | ⚠️ | `mcp_commands.rs:32-38,88,111,130,148,164,179,194,212` |

#### 明确的"无回滚 / 无校验 / 无健康检查"位置

**无回滚**：

- `crates/skillstar-models/src/mcp/sync.rs:46-54` — `backup_if_exists` 拿到 backup 路径后，`upsert` 失败**不使用**它恢复；backup 只作为字符串回传给 UI（`sync.rs:37-40`）。
- `crates/skillstar-models/src/mcp/sync.rs:118-130` — 7 个 target 逐个写，前 N 个成功、第 N+1 个失败时，前 N 个不撤销。
- `crates/skillstar-models/src/mcp/sync.rs:278-288` — update 的 store 提交不受 sync 结果影响。
- `crates/skillstar-models/src/mcp/sync.rs:305-310` — delete 已写出的删除不可撤销。

**无校验**：

- `crates/skillstar-models/src/mcp/store.rs:71-99` — `validate_entry` 只检查：name 非空、stdio 需 command 非空、http/sse 需 url 非空、transport 三选一。**没有**：name 字符集校验（而 name 会逐字写进各工具配置，`types.rs:50-51`）、URL scheme/格式校验、command 是否存在于 PATH、env key 合法性、headers 名合法性。
- `crates/skillstar-models/src/mcp/store.rs:19-44` — store 读取无 schema 版本校验（配合 B.1-b）。
- `crates/skillstar-models/src/mcp/store.rs:132-202` — `update_server` 允许把 transport 改成 http 但不清空 `command`/`args`/`env`（也不清空 url→stdio 的反向），会留下矛盾字段；只有 `validate_entry`(`store.rs:200`) 的最小检查。
- `src-tauri/src/commands/mcp_marketplace.rs:112-129` — `sanitize_key` 只作用于**市场草稿**；手动创建/编辑路径完全不过这道清洗。

**无健康检查**：

- 全仓 MCP 代码路径中**不存在**任何"启动一次 server 验证可连通"的动作：`crates/skillstar-models/src/mcp/` 与 `src-tauri/src/commands/mcp_*.rs` 下 grep `health|ping|probe.*server|spawn` 零命中（唯一的 `probe_*` 是 HTTP client 工厂 `probe_http_client`，`mcp_remote.rs:23`）。
- `McpToolStatus.server_count`（`types.rs:499`）只统计**配置文件里的条目数**（`tools.rs:169-182`），不代表任何 server 真的能跑。

### B.6 `store.rs` — `mcp_servers.json` 读写与校验

- 路径：`~/.skillstar/config/mcp_servers.json`（`store.rs:14-16`，走 `skillstar_core::infra::paths::config_dir()`，因此 `SKILLSTAR_DATA_DIR` 覆盖继续生效）。
- 读：不存在→default；读失败→warn+default；BOM 剥离；解析失败→warn+default（`store.rs:19-44`）。
- 写：`create_dir_all` + 写 `.json.tmp` + `rename` 原子替换（`store.rs:47-64`）。**没有先备份原文件**（对比工具配置写入是有 rolling backup 的）。
- CRUD：`create_server`(`store.rs:110-129`)、`update_server`(`store.rs:132-202`)、`delete_server`(`store.rs:205-212`)、`set_tool_enabled`(`store.rs:215-232`) 均为纯函数，只改内存 `McpStore`。

### B.7 `import.rs` — 从工具现有配置反向导入

- `read_servers_from_tool`（`import.rs:124-135`）：registry 驱动，未知 id 回落到"顶层 `mcpServers`"解析器。
- 四个格式读取器：TOML（`import.rs:142-155`）、OpenCode（`:158-169`）、ZCode（`:172-187`）、canonical JSON（`:190-201`）。
- `entry_from_json_spec`（`import.rs:11-54`）：http/sse 必须有 url、stdio 必须有 command，否则整条跳过（`import.rs:27,49`）。
- `apply_common_approval_fields`（`import.rs:59-87`）：宽松读回 `autoApprove` / `trust` / `disabledTools` / `excludeTools` / `timeout`。
- `entry_from_codex_table`（`import.rs:203-263`）：读回 `disabled_tools` 与 `tool_timeout_sec|startup_timeout_sec`（秒→ms）。
- `import_from_tool`（`import.rs:312-342`）：**按 `name` 匹配**；已存在同名 → 只把该工具的 `enabled` 置 true；不存在 → 新建并置 true。

**缺口 B.7-a（同名不同定义静默丢弃）**：`import.rs:319-324` 命中同名时**完全不比较 command/args/env/url**，工具里的真实定义被丢弃，用户会以为已导入。

**缺口 B.7-b（OpenCode remote 被导成 `sse`）**：`entry_from_opencode_spec` 把 `type:"remote"` 一律建成 `blank_entry(name,"sse")`（`import.rs:270`），而写出去时 `opencode_spec` 对 http 和 sse 都写 `type:"remote"`（`specs.rs:88-97`）。因此 http 远程 server 经过一次「导入 → 再同步」会被改写成 sse，且 canonical 工具（claude-code/cursor/kiro）拿到的 `type` 也会跟着变成 `sse`。这是一条真实的往返失真链。

**缺口 B.7-c（批量导入的失败被吞）**：前端逐工具 `try/catch` 且 catch 体为空（`src/features/mcp/hooks/useMcpServers.ts:98-100`），单个工具解析失败对用户完全不可见。

---

## C. 编排与命令层

### C.1 `skillstar-app` 中是否有 MCP use case

**没有。** 在 `crates/skillstar-app/src/` 下 `grep -i mcp` **零命中**。MCP 是当前唯一一个"命令层直接调用两个域 crate、没有 app 层编排"的功能域：`mcp_commands.rs` 直接用 `skillstar_models::mcp`，`mcp_marketplace.rs` 直接用 `skillstar_marketplace::mcp_snapshot`，而 `mcp_commands::get_mcp_presets` 更是**跨域**同时依赖两者（`src-tauri/src/commands/mcp_commands.rs:233,248`）。

> 这与 AGENTS.md「跨域 use case 进入 `skillstar-app`」的红线存在张力：`curated_server_to_preset`（`mcp_commands.rs:247-279`）本质是 marketplace→models 的跨域映射，却住在命令层；`registry_to_entry`（`mcp_marketplace.rs:282-325`，约 200 行转换逻辑）同理。任何 MCP 改造都应先把这两段挪到 `skillstar-app`。

### C.2 前后端契约清单

**MCP store（`src-tauri/src/commands/mcp_commands.rs`，注册于 `src-tauri/src/lib.rs:430-440`）**

| command | 入参 | 出参 | 行 |
| --- | --- | --- | --- |
| `list_mcp_servers` | — | `McpStore` | `mcp_commands.rs:64-68` |
| `mcp_tool_statuses` | — | `Vec<McpToolStatus>` | `mcp_commands.rs:71-74` |
| `create_mcp_server` | `entry: McpServerEntry` | `McpServerWithSync` | `mcp_commands.rs:83-100` |
| `update_mcp_server` | `id: String, patch: McpServerPatch` | `McpServerWithSync` | `mcp_commands.rs:104-120` |
| `delete_mcp_server` | `id: String` | `Vec<McpSyncResult>` | `mcp_commands.rs:124-136` |
| `set_mcp_tool_enabled` | `id, tool_id, enabled` | `McpSyncResult` | `mcp_commands.rs:140-154` |
| `sync_mcp_server` | `id: String, force: bool` | `Vec<McpSyncResult>` | `mcp_commands.rs:157-170` |
| `sync_all_mcp` | `force: bool` | `Vec<McpSyncResult>` | `mcp_commands.rs:173-184` |
| `import_mcp_from_tool` | `tool_id: String` | `usize` | `mcp_commands.rs:188-202` |
| `reorder_mcp_servers` | `ordered_ids: Vec<String>` | `()` | `mcp_commands.rs:206-223` |
| `get_mcp_presets` | — | `Vec<McpPreset>` | `mcp_commands.rs:226-245` |

**MCP marketplace（`src-tauri/src/commands/mcp_marketplace.rs`，注册于 `src-tauri/src/lib.rs:251-258`）**

| command | 入参 | 出参 | 行 |
| --- | --- | --- | --- |
| `list_mcp_publishers_local` | — | `Vec<McpPublisherSummary>` | `mcp_marketplace.rs:25-29` |
| `list_mcp_servers_by_publisher_local` | `publisher_id: String` | `LocalFirstResult<Vec<McpMarketEntry>>` | `mcp_marketplace.rs:31-39` |
| `list_mcp_market_servers_local` | — | `LocalFirstResult<Vec<McpMarketEntry>>` | `mcp_marketplace.rs:41-49` |
| `search_mcp_market_local` | `query: String, limit: Option<u32>` | `LocalFirstResult<Vec<McpMarketEntry>>` | `mcp_marketplace.rs:51-60` |
| `get_mcp_market_server_detail_local` | `id: String` | `LocalFirstResult<Option<McpMarketServerDetail>>` | `mcp_marketplace.rs:62-70` |
| `sync_mcp_market_scope` | `scope: String`（只接受空串或 `mcp_registry`） | `()` | `mcp_marketplace.rs:72-87` |
| `get_mcp_market_sync_states` | — | `Vec<SyncStateEntry>` | `mcp_marketplace.rs:89-92` |
| `mcp_market_entry_to_draft` | `id: String` | `McpServerEntry` | `mcp_marketplace.rs:97-104` |

**DTO**：命令层只自定义了一个 DTO —— `McpServerWithSync { server, sync_results }`（`mcp_commands.rs:51-57`，ts-rs 导出到 `src/types/generated/McpServerWithSync.ts`）。

**事件**：**MCP 域没有任何 Tauri 事件**。`src-tauri/src/lib.rs` 中 MCP 相关只有命令注册与 `McpWriteLock` state（`lib.rs:149`）。所有进度/结果都是命令返回值同步回传 —— 意味着长耗时的 `sync_all_mcp`（7 target × N server 的文件 IO）与 `sync_mcp_market_scope`（最多 25 次网络往返）**全程无进度反馈**。

**缺口 C.2-a（3 个命令是前端死代码）**：`list_mcp_market_servers_local`、`search_mcp_market_local`、`sync_mcp_server`、`get_mcp_market_sync_states` 在 `src/` 中（排除 `src/lib/ipc/` 的类型声明与 devMock）**零调用点**。前端实际路径是"发布者宫格 → 发布者详情 → `list_mcp_servers_by_publisher_local` + 内存过滤"（`src/pages/Marketplace.tsx:461`、`src/pages/McpPublisherDetail.tsx:75-81,118-127`）。因此后端已实现的 FTS 搜索、`LIMIT`、`bm25` 排序（`query.rs:315-365`）在生产中**从未被使用**。

---

## D. 前端：`src/features/mcp/` + `src/pages/`

### D.1 数据流

```
Marketplace.tsx (MCP tab)
  └─ useMcpPublishers(isMcpTab)  →  list_mcp_publishers_local      (hooks/useMcpPublishers.ts:13-25, Marketplace.tsx:93,461)
       └─ McpPublishers 宫格 → onPublisherClick
            └─ McpPublisherDetail                                   (src/pages/McpPublisherDetail.tsx)
                 ├─ useQuery → list_mcp_servers_by_publisher_local  (:75-81)
                 ├─ 内存过滤 visibleEntries                          (:118-127)
                 ├─ McpMarketBrowser（卡片网格 + 详情抽屉）           (:273-283)
                 │    └─ useQuery → get_mcp_market_server_detail_local (McpMarketBrowser.tsx:85-89)
                 └─ handleMcpInstall → mcp_market_entry_to_draft → McpServerForm 抽屉 (:129-140,305-327)

Mcp.tsx  →  McpManager                                              (src/pages/Mcp.tsx:13-15)
  ├─ useMcpServers  → list_mcp_servers / create / update / delete /
  │                    set_mcp_tool_enabled / sync_all_mcp /
  │                    mcp_tool_statuses + import_mcp_from_tool /
  │                    reorder_mcp_servers                          (hooks/useMcpServers.ts:29-128)
  ├─ useMcpPresets  → get_mcp_presets                               (hooks/useMcpPresets.ts:15-23)
  ├─ useAgentProfiles + selectMcpAgentTargets                       (McpManager.tsx:60,84; lib/agentTargets.ts:30-35)
  ├─ McpServerCard（AgentTargetCarousel 切换 per-tool）              (components/McpServerCard.tsx:66-83)
  └─ McpServerForm（创建/编辑抽屉）                                  (McpManager.tsx:328-388)
```

`src/types/mcp.ts` 只做 re-export，全部形状来自 `src/types/generated/Mcp*.ts`（ts-rs 产物，不可手改）：`src/types/mcp.ts:15-29`。唯一的手写内容是 `MCP_TOOL_IDS` 常量数组与 `isMcpToolId` 守卫（`src/types/mcp.ts:33-46`）——它与 Rust 侧 `types.rs:13-21` 是**两份需要人工保持一致的清单**，且 `McpServerForm.tsx:8-16` 的 `TOOL_LABELS` 是第三份。

### D.2 UI 现有能力矩阵

**市场浏览（`McpMarketBrowser` / `McpMarketCard` / `McpPublisherDetail`）**

| 能力 | 状态 | 证据 |
| --- | --- | --- |
| 发布者宫格入口 | ✅ | `src/pages/Marketplace.tsx:461` |
| 卡片网格 / 列表两种视图 | ⚠️ 组件支持 `viewMode`，但详情页写死 `"grid"` 且无切换 UI | `McpMarketBrowser.tsx:31,80-83`；`McpPublisherDetail.tsx:58` |
| 响应式列数计算 | ✅ ResizeObserver + 迟滞 | `McpMarketBrowser.tsx:51-78` |
| 搜索 | ⚠️ 仅**发布者内**、仅内存、仅 name/namespace/description 三字段子串 | `McpPublisherDetail.tsx:118-127` |
| 全局市场搜索（FTS） | ❌ 后端有、前端不调 | C.2-a |
| 分页 / 无限滚动 | ❌ 一次性渲染全量 | `McpMarketBrowser.tsx:124-134` |
| 按 kind / runtime / license / stars 筛选 | ❌ | 全文件无筛选 UI |
| 排序控制 | ❌ 完全由 SQL 固定序决定 | `query.rs:287` |
| "已安装"标记 | ⚠️ 仅按 `name` 字符串比对 | `McpPublisherDetail.tsx:66`；`McpMarketBrowser.tsx:128` |
| 详情抽屉（stars/license/version/repo/packages/remotes/README） | ✅ | `McpMarketBrowser.tsx:138-242` |
| 必填 env / header 高亮 | ✅ 琥珀色提示 | `McpMarketBrowser.tsx:199-203,220-224` |
| 手动刷新 | ⚠️ 按钮存在但**在发布者详情里被传成空函数** | `McpMarketBrowser.tsx:110-114` ← `McpPublisherDetail.tsx:281` (`onRefresh={() => {}}`) |
| 后台 stale 刷新 | ⚠️ 仅 GitHub 发布者、每发布者只触发一次 | `McpPublisherDetail.tsx:102-112` |
| 快照状态可见性（fresh/stale/miss/error） | ⚠️ 仅 `seeding`→loading、`remote_error`→空态文案 | `McpMarketBrowser.tsx:94,102-114` |

**安装表单（`McpServerForm`）**

| 字段 | 有 | 校验 | 证据 |
| --- | --- | --- | --- |
| name | ✅ | 仅非空 | `McpServerForm.tsx:168-175,127-130` |
| transport（stdio/http/sse 三按钮） | ✅ | — | `McpServerForm.tsx:177-196` |
| url | ✅ | 仅非空（remote 时） | `:200-208,131-134` |
| headers（`K=V` 每行） | ✅ | ❌ 无 | `:209-218,152` |
| command | ✅ | 仅非空（stdio 时） | `:222-230,135-138` |
| args（每行一个） | ✅ | ❌ 无 | `:231-240,155-158` |
| env（`K=V` 每行） | ✅ | ❌ 无 | `:241-250,159` |
| cwd | ✅ | ❌ 无（不校验目录存在） | `:251-259` |
| description / homepage | ✅ | ❌ 无 URL 校验 | `:263-277` |
| autoApproveAll（含 YOLO 警告） | ✅ | — | `:279-307` |
| autoApproveTools / disabledTools | ✅ | — | `:308-330` |
| timeoutMs | ✅ | 仅数字过滤 | `:332-341` |
| 目标工具多选（7 个） | ✅ | ❌ 无"该字段对此工具无效"提示 | `:346-369` |
| 错误反馈 | ⚠️ 单条字符串，展示在表单底部；后端错误走 toast | `:371`；`McpManager.tsx:161-164` |

**已安装管理（`McpManager` / `McpServerCard`）**

| 能力 | 状态 | 证据 |
| --- | --- | --- |
| 搜索（name/description/homepage/transport/tags/command） | ✅ 内存 | `McpManager.tsx:87-101,44-56` |
| 按 Agent 过滤 | ✅ `AgentFilterPill` | `McpManager.tsx:211` |
| 计数徽标 | ✅ | `McpManager.tsx:213-220` |
| per-tool 开关（乐观更新 + 回滚） | ✅ | `McpServerCard.tsx:68-81`；`useMcpServers.ts:60-80` |
| 全量同步 | ✅（`force=false`） | `McpManager.tsx:193-205` |
| 从工具导入 | ✅（先 `mcp_tool_statuses` 过滤再逐个 import） | `useMcpServers.ts:87-105` |
| 拖拽排序 | ❌ `reorder` 已在 hook 暴露但**无任何调用点** | `useMcpServers.ts:107-110,127,139`；`McpManager.tsx` 未引用 |
| 单条重新同步（`sync_mcp_server`） | ❌ 未接线 | C.2-a |
| 同步结果明细 | ❌ 只有"N 个失败"计数 toast，看不到是哪个 target / 什么错误 / backup 在哪 | `lib/syncResults.ts:4-6`；`McpManager.tsx:154-157,172-176,196-201` |
| 工具安装状态展示 | ❌ `McpToolStatus`（含 `config_path`/`installed`/`server_count`）只在 import 内部用一次，从不展示 | `useMcpServers.ts:89-93` |

### D.3 UI 缺失项汇总

1. 无全局 MCP 市场搜索页（只能逐发布者进）——后端 FTS 白建。
2. 无任何筛选/排序控件（kind、runtime、license、stars、recommended、更新时间）。
3. 无分页/虚拟化，GitHub 发布者会一次性渲染整个 registry。
4. 无"已安装/有更新/已弃用"三态；"已安装"仅靠 name 猜测。
5. 同步失败无明细面板（target、错误文本、backup 路径都已在 `McpSyncResult` 里但未展示，`types.rs:474-487`）。
6. 无 per-target 能力提示：表单让用户填 autoApprove/timeout，但只有 1~2 个工具真的会写（B.3-a）。
7. 无工具健康/配置路径视图（`McpToolStatus` 被浪费）。
8. 快照新鲜度、上次同步时间、同步错误对用户不可见（`get_mcp_market_sync_states` 未接线）。

---

## E. 测试覆盖

### E.1 Rust

| 测试文件 / 模块 | 保证了什么 | 没保证什么 |
| --- | --- | --- |
| `crates/skillstar-marketplace/src/mcp_remote.rs:128-165` | 游标 percent-encoding 正确（`:132-136`）；**一个 `#[ignore]` 的真实网络冒烟测试**（`:141-164`） | 分页循环、25 页上限行为、错误分支、hash 稳定性 —— 默认 `cargo test` 下 `mcp_remote` 只跑 1 个纯函数测试 |
| `crates/skillstar-marketplace/src/mcp_models.rs:436-531` | stdio 包解析、remote+secret header 解析、`runtime_command_for` 回落、无信封的裸 server 元素 | `x-github` 三种兜底路径、`_meta` 路径、多 package/多 remote、缺字段组合、`kind=Unknown` 之外的边界 |
| `crates/skillstar-marketplace/src/mcp_snapshot/mod.rs:587-894` | `replace_servers` 往返 + 全量交换语义（`:645-715`）；sync_state 新鲜度迁移（`:717-731`）；FTS 表达式注入安全（`:733-742`）；发布者聚合顺序与计数（`:744-796`）；curated/registry 分桶（`:798-893`） | `sync_mcp_registry_scope` 的 unchanged 短路分支、`mark_success_with_meta` 的 `unchanged=true` 保留语义、并发/BUSY、无 LIMIT 的大结果集 |
| `crates/skillstar-models/src/mcp/registry.rs:139-184` | 注册表 = `MCP_TOOL_IDS` 且同序（`:145-148`）；id 唯一 + label 非空（`:151-157`）；legacy id 不在注册表（`:160-164`）；所有 resolver 在沙箱 home 下可解析为绝对路径（`:167-175`）；label helper 一致（`:178-183`） | 每个 spec 的 `count_live`/`read_servers`/`upsert`/`remove` 四列是否配对正确（例如 grok 用 `codex_remove` 但写 `grok_toml_table`，无测试交叉验证） |
| `crates/skillstar-models/src/mcp/tests.rs`（566 行，32 个 `#[test]`） | canonical/opencode/codex/grok/kiro/cursor/zcode 的 wire 形状（`:35-174,468-495`）；approval 字段的 per-tool 投影（`:124-165`）；store CRUD/校验/往返（`:175-237`）；**legacy Desktop Chat 清理的全套语义**（`:238-416`，含畸形 JSON fail-closed `:309-322`）；preset 目录良构（`:417-445`）；未知工具报错（`:496-506`）；`sync_all` 每工具一条结果（`:507-527`） | **①** 目标配置畸形时 upsert 的覆盖行为（B.4-a）完全无测试；**②** 部分失败的回滚/一致性（F3/F4/F6）无测试；**③** `import_from_tool` 同名冲突（B.7-a）与 OpenCode http↔sse 往返（B.7-b）无测试；**④** `mcp_servers.json` 畸形导致的数据丢失（F1）无测试；**⑤** 无并发测试 |
| `src-tauri/src/commands/mcp_marketplace.rs:327-478` | `registry_to_entry` 的 npm/pypi/oci/remote 四种映射（`:354-421`）；`sanitize_key`（`:422-428`）；两条真实 curated raw JSON 的转换（`:430-477`） | `raw_server_json` 非法时的空草稿路径（F9）；多 package 选择策略；`package_arguments` 与 `runtime_arguments` 的组合 |
| `crates/skillstar-marketplace/src/snapshot/migrations.rs` | v11/v12 的幂等 ALTER 有 `column_exists` 保护（`:496-506,533-538`） | v8/v10 的 `CREATE TABLE IF NOT EXISTS` 无列级迁移能力（A.3-a）——**没有任何测试覆盖"老 DB + 新列"场景** |

`cargo test -p skillstar-models -p skillstar-marketplace -p skillstar export_bindings` 是文档给出的验证命令（`docs/features/mcp/README.md:45`）。

### E.2 前端

| 文件 | 保证了什么 |
| --- | --- |
| `src/features/mcp/lib/agentTargets.test.ts` | Settings profile → MCP target 的交集映射与失效 filter 回落 |
| `src/features/mcp/lib/syncResults.test.ts` | `failedMcpSyncCount` 正确忽略 `skipped` |
| `src/lib/ipc/devMockCoverage.test.ts` | 每个声明的 command 都有 devMock（棘轮式允许列表，`:20-29`） |

**前端测试缺口**：`McpServerForm`、`McpManager`、`McpMarketBrowser`、`McpMarketCard`、`useMcpServers`、`useMcpPresets`、`useMcpPublishers`、`McpPublisherDetail` **全部无测试**。表单的 `parseKv`/`parseList`（`McpServerForm.tsx:49-80`）这类纯函数也没有单测——注意 `parseKv` 会 `trim()` value（`:56`），带前后空格的密钥会被静默改写。

---

## F. 改造风险点（会破坏存量用户数据的地方）

按"破坏面 × 不可逆程度"排序。

### R1 — `mcp_servers.json` 的 schema 变更（最高风险）

**为什么危险**：`read_mcp_store` 对**任何**解析失败一律返回空 store（`crates/skillstar-models/src/mcp/store.rs:34-43`），而下一次写入是 `tmp + rename` 的**整文件替换**（`store.rs:47-64`），且**写前不备份**。因此任何让老文件解析失败的改动（新增非 `#[serde(default)]` 字段、改字段类型、改 `enabled` 的值类型、把 `Vec` 改成 `Option<Vec>` 等）都会导致：用户一打开 MCP 页面点任何按钮 → 全部 MCP 服务器**静默永久丢失**。`McpStore.version`（`types.rs:453-459`）写了但从没人读，救不了场。

**必须怎么迁移**：

1. **先加安全网，再改 schema**：把 `read_mcp_store` 的解析失败分支从"返回 default"改为"返回错误 + 把原文件另存为 `mcp_servers.json.corrupt.<ts>`"（对应改动点 `store.rs:34-43`）；并在 `write_mcp_store` 里对已存在的文件做一次 rolling backup（可直接复用 `tool_sync::backup_merge::create_rolling_backup`，`crates/skillstar-models/src/tool_sync/backup_merge.rs:11-27`）。
2. **只做加法，且必须 `#[serde(default)]` + `skip_serializing_if`**，与现有全部可选字段保持一致（`types.rs:57-125`）。新增 `source_id` / `installed_version` 之类字段属于这一类，安全。
3. **需要改字段语义时才 bump `McpStore.version`**，并在 `read_mcp_store` 里实装版本分支：`version > CURRENT` → 拒绝写入（fail closed，避免新版写的文件被旧版清空）；`version < CURRENT` → 就地升级后立刻写回。这是当前完全缺失的机制。
4. **`enabled` map 的 key 是跨版本契约**：`create_server` 会 `retain(is_supported_tool)` 丢弃未知 key（`store.rs:126`），但 `read_mcp_store` 保留（测试钉死于 `crates/skillstar-models/src/mcp/tests.rs:238-259`）。若新增/重命名 tool_id，必须同时更新 `MCP_TOOL_IDS`(`types.rs:13-21`)、`MCP_TOOL_SPECS`(`registry.rs:42-123`)、前端 `MCP_TOOL_IDS`(`src/types/mcp.ts:37`)、`TOOL_LABELS`(`src/features/mcp/components/McpServerForm.tsx:8-16`)、`MCP_TOOL_BY_AGENT_ID`(`src/features/mcp/lib/agentTargets.ts:5-13`) 五处；**移除**某个 tool_id 时必须像 gemini 一样加 legacy tombstone 清理路径（`types.rs:30`、`sync.rs:165-181`），否则用户机器上会残留孤儿配置。

### R2 — marketplace SQLite snapshot 的列变更

**为什么危险**：`create_mcp_registry_tables` 全是 `CREATE TABLE IF NOT EXISTS`（`crates/skillstar-marketplace/src/mcp_snapshot/mod.rs:47-107`）。存量用户 `user_version` 已是 12（`crates/skillstar-marketplace/src/snapshot/mod.rs:29`），这个函数对他们**一行也不会执行**。往里加列 = 新装用户有、老用户没有 = 所有 `SELECT` 立刻 `no such column` 而整个 MCP 市场瘫痪。

**必须怎么迁移**：

1. 新写 `migrate_v12_to_v13`，**严格照抄 v11/v12 的幂等模式**：单事务 + `column_exists` 判断 + `ALTER TABLE ADD COLUMN`（模板 `crates/skillstar-marketplace/src/snapshot/migrations.rs:491-510` 与 `:528-542`，守卫在 `:496-506` 与 `:533-538`）。切勿用裸 `ALTER TABLE`——`user_version` 只在整条链成功后才 bump（`migrations.rs:134`），中途崩溃会导致下次启动 `duplicate column name` 而永久 brick（这段教训已写在 `migrations.rs:485-490`）。
2. **同时**把新列加进 `create_mcp_registry_tables` 的建表语句，新装用户才拿得到。两处必须同改。
3. 加列后必须同步更新：`replace_servers` 的 INSERT 列表（`query.rs:30-34`）、`seed_default_curated_mcp_servers` 的 upsert 列表与 `ON CONFLICT DO UPDATE SET`（`mod.rs:137-160`）、`row_to_card`(`query.rs:240-262`) / `row_to_full_server`(`query.rs:367-395`)、以及 `load_cards`/`search_cards` 里两段手写的 UNION ALL SELECT（`query.rs:264-298,315-365`）——**UNION 两侧列数必须一一对应**，漏一处就是运行时 SQL 错误。
4. **FTS 虚表加列必须重建**：`mcp_registry_server_fts` / `mcp_curated_server_fts` 无法 ALTER，只能 `DROP` + `CREATE` + 全量重灌（参照 v9 的 FTS 重建 `migrations.rs:448-466`）。重灌 registry 侧需要从 `mcp_registry_server` 反查，curated 侧靠下次 seed 自动补（`mod.rs:163-218`）。
5. **curated 表是"代码即数据"，不是用户数据**：`seed_default_curated_mcp_servers` 每次读前都全列覆盖（`mod.rs:129-224`），所以 curated 行可以自由改；但**`id` 是主键且是发布者分桶键**，改 id = 老行变孤儿（不会被删，因为 seed 只 upsert 不 delete）。若要重命名 curated id，必须在迁移里显式 `DELETE FROM mcp_curated_server WHERE id IN (...)` + `DELETE FROM mcp_curated_server_fts WHERE id IN (...)`。

### R3 — `sanitize_key` / name 规则变更

`name` 是**逐字写进 7 个工具配置文件的 key**（`types.rs:50-51`）。若改动 `sanitize_key`（`src-tauri/src/commands/mcp_marketplace.rs:112-129`）或给手动创建路径加上清洗，存量条目的 name 与工具配置里的 key 会错位 → 后续 remove/rename 按新 name 去删，**旧 key 永远留在用户配置里成为孤儿**。迁移必须：改名前先用旧 name 从所有 target remove（模式已存在于 `sync.rs:273-277`），或提供一次性的孤儿扫描清理。

### R4 — `McpSyncResult` / `McpServerWithSync` 形状变更

这两个类型经 ts-rs 导出到 `src/types/generated/`（`types.rs:471-487`、`mcp_commands.rs:51-57`）。它们不落盘，所以无存量数据风险，但**改后必须运行 `bun run types:gen` 并提交产物**（AGENTS.md 硬约束），否则前端类型与真实 wire 不一致。

### R5 — 移除/重排 curated 发布者

`CURATED_ORDER` 是硬编码常量（`crates/skillstar-marketplace/src/mcp_snapshot/query.rs:99-140`），且 `load_publishers` 只输出"在常量里 **且** DB 里有行"的发布者（`query.rs:158-168`）。从常量里删掉一个 source 会让它的 curated 行变成**查不到的僵尸数据**（seed 仍在写、grid 不再显示、`load_cards` 仍会把它们混进全量列表 `query.rs:270-277`）。必须配套 `DELETE FROM mcp_curated_server WHERE source = ?` 迁移。

### R6 — 改动"未安装即 skip"语义

当前 `sync_server_to_tool` 在工具未安装时返回 `success=true, skipped=true` 且**不写文件**（`sync.rs:30-34`）。用户 store 里可能已积累大量 `enabled=true` 但从未落盘的条目（F7）。若改成"总是写"，会在用户机器上凭空创建一批 `~/.kiro/settings/mcp.json`、`~/.cursor/mcp.json` 等配置文件；若改成"skip 时把 enabled 置回 false"，会静默改写用户意图。任一方向都需要一次性的对账迁移 + 明确的 UI 告知。

---

## G. 建议的改造优先级（仅供参考，不构成决策）

| 级别 | 事项 | 依据 |
| --- | --- | --- |
| P0 | 修 `read_json_object` 的静默清空（B.4-a / F2）与 `read_mcp_store` 的静默 default（F1 / R1） | 会造成用户数据不可逆丢失 |
| P0 | 修 `get_mcp_presets` 只返回 1 条（A.3-f） | 当前就是可见的功能退化 |
| P1 | `McpServerEntry` 加来源指纹（B.1-a），解锁更新/弃用能力 | 一切版本化能力的前置 |
| P1 | 同步结果明细 UI（D.3-5）+ 失败可重试 | 现有数据已够，纯前端 |
| P1 | 把 `registry_to_entry` / `curated_server_to_preset` 迁到 `skillstar-app`（C.1） | AGENTS.md 红线 |
| P2 | 接线已有的 FTS 搜索与筛选/排序（A.3-d、C.2-a、D.3-1/2） | 后端大半已就绪 |
| P2 | 解析并展示 `status`/`deprecated`、`environment_variables` 的完整语义（A.2-a/c） | 需 R2 迁移 |
| P2 | 分页（A.3-e / D.3-3） | 大目录下的性能与内存 |
| P3 | 部分失败的回滚或补偿（F3/F4/F6）、健康检查（B.5 无健康检查段） | 设计成本较高 |

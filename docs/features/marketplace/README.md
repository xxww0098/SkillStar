# Marketplace

状态：active

本文件维护技能市场快照、搜索和 Publisher 浏览契约。MCP 安装与工具同步见 [../mcp/README.md](../mcp/README.md)。

## 所有权

- `skillstar-marketplace` 拥有 SQLite snapshot、FTS、远程 seed/refresh、Marketplace 专用 DTO 和 MCP registry/curated catalog。
- 市场列表、搜索结果与已安装技能共用 `skillstar-core::types::Skill`；Marketplace 不定义同名副本，也不增加仅用于掩盖重复所有权的转换链。
- `db` 与远程 MCP loader 是 crate 内实现，不作为外部深路径 API；调用方消费 crate root DTO 或明确的 `snapshot`/`mcp_snapshot` use-case 入口。
- `src-tauri/src/core/marketplace_snapshot/` 只包装 Tauri State；业务查询和 schema 不应回流到该目录。
- 技能安装仍归 `skillstar-skills`；“搜索结果 → 安装”的跨域流程通过 command/`skillstar-app` 组合窄 facade。

## 本地优先

- 页面和 CLI search/find 先查询本地 snapshot/FTS，返回 freshness/seeding 状态。
- 远程同步是明确后续动作，不能让页面直接以浏览器 HTTP 替代本地数据源。
- publisher/detail 页面与主列表复用同一 local-first flow；缺 description 时不在浏览器临时 hydrate 另一份数据。
- DB 操作优先短生命周期 WAL connection，避免进程级单 connection lock 阻塞并发读。
- 所有远程 HTTP 使用 `probe_http_client`，GitHub repo 操作遵循 mirror/fallback。
- 同一个仓库有两个写入方：`publisher_repos:<publisher>` 来自 `/official` 聚合（仓库清单完整，但会保留仓库已经不再提供的技能），`repo_skills:<source>` 来自仓库页（当前状态）。仓库页一旦同步成功就是该仓库技能行的唯一权威，聚合刷新只给从未抓过仓库页的仓库做种子；仓库卡片上的技能数由本地技能行推导，没有行时才回退到聚合计数，因此卡片与点进去看到的列表不会不一致。
- taxonomy/pack command surface 未挂前端，crate API 与 SQLite 表保留。

## 多源拉取与内容寻址

对抗审查设计：商店不能依赖单一远端。

- 拉取按 host 候选链执行：`remote::marketplace_hosts()` 以 `https://skills.sh` 为首，按 `config/marketplace_mirror.json` 的 `enabled`/`hosts` 追加镜像（仅接受 `https://`，去重，主站始终第一）。`fetch_with_failover` 按序尝试，成功即停，全部失败返回聚合错误。
- 每次成功拉取产生 `FetchMeta{payload_sha256, source_host, etag, degraded}`：sha256 是响应体内容指纹；`source_host` 记录实际服务端；服务端 ETag 存在时记录；`degraded` 标记这份 payload 是解析降级/兜底得到的，不是完整可信结果。
- snapshot schema v11 起，`marketplace_sync_state` 含 `source_host`/`payload_sha256`/`etag` 三列。快照同步与 MCP registry 同步都是内容寻址增量写：本次 payload 与上次记录相同（304 或 sha256 一致）时，只刷新 scope 时间戳并保留旧指纹与旧 `source_host`，不重写数据表；内容变化才走既有 delete+reinsert 事务。唯一例外是 `etag`：服务端可能在同字节响应上轮换 validator，所以本次带回 ETag 就采纳（`COALESCE(new, old)`），否则一直发一个服务端已经不认的 token，再也拿不到 304。
- 镜像只在主站不可达时启用；镜像内容是中间代理，应只添加可信来源。

## 降级数据不冒充新鲜数据

远端结构改版或响应无法完整解析时，拉取侧可以退回兜底解析，让用户至少看到一部分内容，但这份内容不允许以「已是最新」的姿态呈现。

- 判定发生在**合并兜底数据之前**：榜单是否降级由「SSR HTML 有没有解析出榜单」决定，而不是由合并后的行数决定。`/api/search` 只是补充；HTML 解析不出东西时它就成了全部答案（≤200 条对字面词 "skill" 的模糊匹配、没有 skills.sh 的排名），这个事实本身就置位 `degraded`。合并后再问「结果是不是空的」永远得到「不空」，等于整套机制在它唯一要防的场景里不生效。
- 两半都没有产出（HTML 解析为空且 API 也失败）不是降级而是**没有载荷**：直接失败，不允许用空榜单覆盖已有快照。
- `FetchMeta.degraded` 为真时，scope 同步落库后不写正常 TTL，而是把该 scope 标记为**需要再次刷新**：下一次本地读到的状态是 `stale`，不是 `fresh`。
- degraded 状态必须有出口。完整载荷是唯一出口，且**优先于内容寻址**：stored 为 degraded 时，即使字节与上次完全相同也强制重写（当时存下的指纹属于一份「当时解析不了」的载荷，解析器修好后重新解析同一份字节正是主要恢复路径）；同理 stored 为 degraded 时不发 `If-None-Match`，否则只拿到 304 就永远没有 body 可重新解析。
- 读路径的新鲜度契约按**该读路径有没有自己的 scope**分两级，两级都不得因为「表里有行」就断言 `fresh`：
  - 有 scope 的读路径（`all` 列表、hot / trending 榜单、publishers、repo skills、skill detail）完整遵守 scope 新鲜度：TTL 过期或 degraded 一律 `stale`。`all` 列表的数据虽然读全表，新鲜度仍由 `leaderboard_all` scope 决定。
  - search / AI search 读的是 `marketplace_skill` 全表，没有自己的 TTL（命中可能来自榜单同步，也可能来自刚刚的单次 query seed，后者不因榜单到期而变旧），因此只适用契约中的降级部分：`leaderboard_all` 处于 degraded 期间，search 一律报 `stale` 而非 `fresh`——兜底写入的行同样会被搜出来，不允许以「已是最新」呈现。
- AI search 是否要为某个关键词回远端补种，**按关键词逐个判定**，判据是该关键词自己的 `search_seed:<keyword>` 同步记录（大小写归一），不是快照表的行数。行数是「榜单同步了多少」的事实，而榜单同步对任何具体关键词一无所知，回答不了「我们问过这个词没有」——旧判据 `snapshot_rows < 500` 因此恒为假（榜单 SSR 约 600 行 + API 补充 ≤200，首次同步后恒在 600–800），整个补种分支是死代码。补种记录带 TTL 且降级载荷不给 TTL，所以既不会重复问同一个词，也不会让一次陈旧或残缺的回答把这个词永久钉死。
- 前端因此会在降级数据上显示 stale 标签并触发一次后台自动刷新；用户看到的是「这份数据不完整、正在重取」，而不是一个静默的残缺榜单。
- 降级写入本身仍是成功路径（数据可用），不把 scope 记成失败态（`degraded_reason` 非空、`last_error` 保持 NULL）。但 `last_success_at` 与 `last_error` 同时非空是**正常可达状态**（任何一次成功之后的刷新失败都会产生它，包括拒绝降级载荷时）：`last_success_at` 回答「数据何时落地」，`last_error` 回答「最近一次刷新为何失败」，「数据可不可信」只由 `degraded_reason` 回答。诊断消费方不得用 `last_error.is_some()` 推断「没有数据」。

## 快照状态与错误呈现

`LocalFirstResult<T>`、`SnapshotStatus`、`SyncStateEntry` 的形状只有一个 SSOT：`crates/skillstar-marketplace/src/snapshot/mod.rs`；`MarketplaceSkillDetails` 与 `SecurityAudit` 的 SSOT 是 `crates/skillstar-marketplace/src/remote/skill_details.rs`。这些类型都带 ts-rs `#[ts(export)]`，由 `bun run types:gen` 生成到 `src/types/generated/`，`src/types/marketplace.ts` 只做 re-export。前端不得再手写它们的副本——手写副本已经三次与 Rust 侧脱节（TS 声明了 Rust 没有的 `error` 字段；`SyncStateEntry` 新增 `source_host`/`payload_sha256`/`etag`/`degraded_reason` 后 TS 侧没跟上；`MarketplaceSkillDetails` 新增 `security_audits` 后 TS 侧完全没有这个字段，安全审计结果因此在 UI 上不可见），`scripts/internal/check_generated_types.sh` 现在把这类漂移变成 CI 失败。

`LocalFirstResult.snapshot_status` 的六个取值在前端都有明确呈现，没有"未知即当作正常"的分支。

- 状态按 scope（`leaderboard` / `publishers` / `search`）分别持有。skill tab 只读 leaderboard/search 的状态，publisher tab 只读 publishers 的状态；一个 scope 的失败不会串到另一个 tab。
- 首次响应落地前状态是"未知"，页面不渲染任何新鲜度断言，也不会先闪一下"fresh"。
- `fresh` 不显示标签；`stale` / `seeding` / `miss` / `error_fallback` / `remote_error` 各有独立标签。
- `seeding` 且当前无内容时并入 loading 态，不渲染成"市场为空"。
- `miss` 与 `remote_error` 且当前无内容时渲染专用空态并带动作按钮（立即同步 / 重试），不再退化成普通"无结果"。搜索场景仍优先走"在线搜索并保存到本地"。
- `error_fallback` 表示本地快照读取失败、已用在线数据兜底，渲染为警告级提示条而非错误级，且不遮挡内容。

错误呈现遵循一条边界：hook 层不产出面向用户的文案。hook 只吐结构化错误 `{ kind, scope, detail }`（`kind` ∈ `remote_error` / `error_fallback` / `query_failed` / `sync_failed` / `search_failed`），渲染层把 `kind` 映射到 i18n key。后端 `LocalFirstResult.error` 与 IPC 抛出的原始错误链只作为 `detail`，收在可折叠的"详细信息"里并写入 console，不作为主文案。

自愈与重试：

- 快照为 `stale` 时后台自动刷新。刷新失败**不再**永久禁用该 scope，只消耗一次重试额度（同一 scope 上限 3 次），额度耗尽后仍可由显式重试重置。
- 每个错误提示条都带重试按钮：leaderboard / publishers 走与后台刷新相同的 sync + refetch 路径，search 走一次在线搜索。用户不需要靠切走再切回 tab 解锁。

## 技能搜索与导入

- GitHub repo import 分为 scan 和 install 两阶段，扫描本身不改变安装状态。
- Marketplace 只返回可安装描述；repo cache、root-first discovery 和实际 install 属于 Skills 域。
- Publisher 与 curated source 的完整清单以 seed/registry 代码和测试为准，文档不复制数量或排序。

## 前端信息架构

- Marketplace 是统一发现入口，但 Skills 与 MCP 在左侧 category rail 中保持清晰分组。
- skill tab 进入技能列表；MCP 官方入口先显示 Publisher grid，再进入 Publisher detail。
- Publisher drill-down 复用主市场的 grid/list 和 toolbar 交互，不创建第二套 fetch 逻辑。
- installed MCP 管理不放 Marketplace，而在 MCP 页面处理。

## 验证

```bash
cargo test -p skillstar-marketplace
bun run test -- src/features/marketplace src/pages/Marketplace.tsx src/pages/PublisherDetail.tsx
```

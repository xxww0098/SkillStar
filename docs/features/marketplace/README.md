# Marketplace

状态：active

本文件维护技能市场快照、搜索和 Publisher 浏览契约。

## 所有权

- `ss-marketplace` 拥有 SQLite snapshot、FTS、远程 seed/refresh、Marketplace 专用 DTO。
- 市场列表、搜索结果与已安装技能共用 `ss-core::types::Skill`；Marketplace 不定义同名副本，也不增加仅用于掩盖重复所有权的转换链。
- `db` 与远程 loader 是 crate 内实现，不作为外部深路径 API；调用方消费 crate root DTO 或明确的 `snapshot` use-case 入口。
- 市场快照的进程接线在 `ss-app::bootstrap`。业务查询和 schema 留在 `ss-marketplace`。MCP serve 不调用 `initialize`。
- 技能安装仍归 `ss-skills`；“搜索结果 → 安装”的跨域流程通过 command/`ss-app` 组合窄 facade。

## 本地优先

- 页面和 CLI search/find 先查询本地 snapshot/FTS，返回 freshness/seeding 状态。
- 远程同步是明确后续动作，不能让页面直接以浏览器 HTTP 替代本地数据源。
- publisher/detail 页面与主列表复用同一 local-first flow；缺 description 时不在浏览器临时 hydrate 另一份数据。
- DB 操作优先短生命周期 WAL connection，避免进程级单 connection lock 阻塞并发读。
- 所有远程 HTTP 使用 `probe_http_client`，GitHub repo 操作遵循 mirror/fallback。
- 同一个仓库有两个写入方：`publisher_repos:<publisher>` 来自 `/official` 聚合（仓库清单完整，但会保留仓库已经不再提供的技能），`repo_skills:<source>` 来自仓库页（当前状态）。仓库页一旦同步成功就是该仓库技能行的唯一权威，聚合刷新只给从未抓过仓库页的仓库做种子；仓库卡片上的技能数由本地技能行推导，没有行时才回退到聚合计数，因此卡片与点进去看到的列表不会不一致。
- taxonomy/pack 没有界面，crate API 与 SQLite 表保留。

## 描述回填

榜单 SSR 载荷与 `/api/search` 行都不携带 description（2026-10-06 实测：SSR 技能对象只有 `source/skillId/name/installs/weeklyInstalls/isOfficial`），技能的一行描述只存在于各自详情页的 JSON-LD `SoftwareApplication.description`（`remote::fetch_skill_description`）。

- 每次榜单同步（含内容未变的 skip-rewrite）都会在后台 fire-and-forget 触发一轮 `snapshot::backfill_missing_descriptions`：取榜单排名最高、仍缺描述的 `ENRICH_BATCH`（24）个技能，`ENRICH_CONCURRENCY`（6）并发抓详情页，把描述写回 `marketplace_skill.description` 并刷新 FTS。测试构建不触发，单元测试不会因此碰网络。
- 只回填 `ENRICH_RANK_CAP`（300）名以内的技能；可见的头部几屏在数轮同步内填满，长尾不值得逐页流量。
- snapshot schema v15 给 `marketplace_skill` 加 `description_sync_at` 水位：详情页没有描述或抓取失败的技能不会被每一轮反复重试，`ENRICH_RETRY_DAYS`（7 天）后才再次尝试。抓到空白（纯空白文本）按未命中处理，只写水位。
- 写入用 `WHERE description = ''` 守卫，不会覆盖更完整来源（如 detail sync、搜索 seed）写入的描述。
- GPUI 市场页在手动同步成功后延迟 8 秒重读一次列表，让当轮回填的描述无需用户操作即可浮现；其余读取路径按各自正常节奏看到新描述。

## 多源拉取与内容寻址

对抗审查设计：商店不能依赖单一远端。

- 拉取按 host 候选链执行：`remote::marketplace_hosts()` 以 `https://skills.sh` 为首；若启用了 GitHub 加速，则追加 `{mirror}https://skills.sh/`（按用户排序、跳过熔断源，去重）。`fetch_with_failover` 按序尝试，成功即停，全部失败返回聚合错误。商店没有单独的加速源列表。
- 每次成功拉取产生 `FetchMeta{payload_sha256, source_host, etag, degraded}`：sha256 是响应体内容指纹；`source_host` 记录实际服务端；服务端 ETag 存在时记录；`degraded` 标记这份 payload 是解析降级/兜底得到的，不是完整可信结果。
- `If-None-Match` **只**发给当初签发该 ETag 的 `source_host`。把 skills.sh 的 validator 带到加速源包装地址上会产生假 304，把快照钉死在旧 host 上。
- snapshot schema v11 起，`marketplace_sync_state` 含 `source_host`/`payload_sha256`/`etag` 三列。快照同步是内容寻址增量写：本次 payload 与上次记录相同（304 或 sha256 一致）时，只刷新 scope 时间戳并保留旧指纹与旧 `source_host`，不重写数据表；内容变化才走既有 delete+reinsert 事务。唯一例外是 `etag`：服务端可能在同字节响应上轮换 validator，所以本次带回 ETag 就采纳（`COALESCE(new, old)`），否则一直发一个服务端已经不认的 token，再也拿不到 304。
- snapshot schema v14 删除已下线 MCP 商店的 registry/curated 表及其 FTS，并清掉 `marketplace_sync_state` 中的 `mcp_registry` 行（见 [D-074](../../decisions.md#d-074删除-mcp-管理与-mcp-商店)）；旧 v8/v10/v13 迁移已移除。
- 加速包装只在主站请求失败后尝试。加速源是中间代理，只在用户打开 GitHub 加速时进入候选链。

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
- 界面因此会在降级数据上显示 stale 标签并触发一次后台自动刷新；用户看到的是「这份数据不完整、正在重取」，而不是一个静默的残缺榜单。
- 降级写入本身仍是成功路径（数据可用），不把 scope 记成失败态（`degraded_reason` 非空、`last_error` 保持 NULL）。但 `last_success_at` 与 `last_error` 同时非空是**正常可达状态**（任何一次成功之后的刷新失败都会产生它，包括拒绝降级载荷时）：`last_success_at` 回答「数据何时落地」，`last_error` 回答「最近一次刷新为何失败」，「数据可不可信」只由 `degraded_reason` 回答。诊断消费方不得用 `last_error.is_some()` 推断「没有数据」。

## 快照状态与错误呈现

`LocalFirstResult<T>`、`SnapshotStatus`、`SyncStateEntry` 的形状只有一个 SSOT：`crates/ss-marketplace/src/snapshot/mod.rs`；`MarketplaceSkillDetails` 与 `SecurityAudit` 的 SSOT 是 `crates/ss-marketplace/src/remote/skill_details.rs`。没有 TypeScript 生成物。改这些结构时改 Rust，并让 GPUI 调用方跟着改。

`LocalFirstResult.snapshot_status` 的六个取值在界面都有明确呈现，没有"未知即当作正常"的分支。

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
- Marketplace 卡片安装直接调用 Skills 域的 `install_skill`，进行中只标记忙碌，不订阅 `skillstar://git-progress`。分阶段文案只在导入对话框的 progress sink 里。
- Publisher 与 curated source 的完整清单以 seed/registry 代码和测试为准，文档不复制数量或排序。

## 界面

- Marketplace 只做技能发现：总排行 / 趋势 / 热门 / 官方技能发布者。
- 技能 Publisher drill-down 复用主市场的 grid/list 和 toolbar 交互，不创建第二套 fetch 逻辑。
- 网格与来源 chip 见 [界面约定](../frontend/README.md#技能卡)。市场详情列打开时先减去列宽再算列数，规则同一处。
- 主列表（技能与官方发布者）向下滚动超过约 300px 后，在内容区右下角显示“回到顶部”悬浮按钮，点击回到列表顶部（`crates/ss-gpui/src/chrome/scroll_top.rs`）；切换 tab、清除搜索或网格/列表切换都会重置滚动位置，从新数据集的顶部开始。

## 技能详情列

市场技能卡（榜单和发布者详情里的同一张卡）点击后打开右侧详情列，几何与我的技能详情列相同：同一条 352px 轨道（`layout.rs` 的 `DETAIL_COLUMN_W`，一列卡加一档列距，打开正好少一列）、共用 `chrome` 的悬浮 sheet 表面（12px 环、16px 圆角、整圈 hairline、与 kit 弹窗同款的投影、`panel` 底色）、标题和关闭按钮，作为工具栏下方的兄弟列，不覆盖卡片，也不裁掉最右一张卡的边框。再次点击同一张卡或点关闭会收起。打开的卡使用 [界面约定](../frontend/README.md#技能卡) 的选中面。安装按钮和来源 chip 各自 `stop_propagation`，点击不会打开或关闭详情列。

打开时网格按减去详情列宽度后的内容宽度重算列数；列表模式仍是单列。切到官方发布者、离开当前技能列表，或当前列表不再包含该技能时关闭详情列。

详情列先展示列表行已有的名称、描述、来源、下载量和安装状态，同时按 `source` + `name` 读取 `get_skill_detail_local`。没有 `source` 时不发请求。请求绑定当前选择（epoch + source + name），切换选择后旧响应不得覆盖新选择。详情补全摘要、周安装量、GitHub 星标、首次出现和安全审计。SKILL.md 不嵌在列里；正文和描述不同时，「查看 SKILL.md…」打开与技能页相同的悬浮窗，渲染这次详情里的全文，不再读盘。设置里打开「描述」且界面语言为中文时，卡片和详情列上的英文描述换成已缓存的译文，规则与 [技能详情](../skills/README.md) 相同。关闭时只显示原文。本地结果为 `stale` 或 `miss` 时后台再同步一次，不循环。安装和卸载与卡片按钮走同一个 `set_installed`。实现在 `crates/ss-gpui/src/marketplace/detail_drawer/`。

## 验证

```bash
cargo test -p ss-marketplace
cargo test -p ss-gpui --lib marketplace::detail_drawer
```

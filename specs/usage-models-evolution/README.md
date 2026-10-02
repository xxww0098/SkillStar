# usage/models 演进 Phase 0-5（度量面与安全）— Spec

状态：open（12/13）
更新：2026-10-02
关联：`specs/usage-models-tree/`（open，gateway crate 树重构，本 spec 多个切片有 ⛩ 前置）

## Next Agent Prompt

你是下一个接手本 spec 的实现者。已完成：切片 01（LAN 门禁，D-078）、切片 05（sessions 地基 + claude 系 parser，`usage/src/sessions/`，trait 带 replay 全量重建 + `data_root()/sessions/index.json` 增量索引）。进行中：03（ledger）、02（custody 改道）、06（四 parser）。

- **先读本 README 的「头号事实」「已拍板决策」「全局防火墙」**，再读你将做的切片文件；分歧裁决见 `choices.md`，不重开讨论。
- 下一切片按依赖：06（进行中）→ 07（需 03+06 数据形状）；04/08/09/10/11/12 等 ⛩ tree 前置（tree spec 全部未动工——完成本 spec 需先做 tree 的 02-09）。
- 每片完成：跑该片验证 + `cargo check --workspace --locked` + `bash scripts/internal/check_clippy_ratchet.sh`（baseline 零改动，现为 1），更新本节与 TODO。
- 环境注意：`serve_binds_default_port` 本机端口占用是环境性失败，gateway 测试统一 `-- --skip serve_binds_default_port`。
- 涉及 DTO 的片：`bun run types:gen && git diff --exit-code src/types/generated/` 必须零意外 diff。
- 代码注释一律英文（仓库约定；中文只在 Display/UI 字符串与 docs）。
- 结束你的 pass 前，把本节改写成下一个 agent 需要的样子。

### 全局 TODO

- [x] 01 gateway key 与 LAN 门禁（P0）— D-078、errors.md 已记；omp 仅 loopback 可用已接受
- [x] 02 account_book 改道 custody（P0）— `usage_switch::signing_material` 三态（Live/Row/Diverged），AccountSnapshot 契约不变；probe 纯读实测无副作用
- [x] 03 账本 append 热路径（P1）— `ledger/`（record+append，5MB 轮转，O_APPEND+进程锁），TurnFacts 管道 + SSE 尾帧/JSON 双提取，上游原始字节入账（回译失败 token 不丢）
- [x] 04 账本读取面与前端换源（P1，⛩tree-07）— LedgerQuery + get_ledger_page + ModelsHub 三新列；账本为准、环补缺（(agent,model_asked,status,output) 匹配）
- [x] 05 sessions 地基 + claude 系 parser（P2）— claude-desktop 实测走 entrypoint 前缀归因（`claude-desktop-3p`），Cowork glob 本机未命中但保留
- [x] 06 codex/opencode/pi/omp parser（P2）— 每 parser 一 commit；ruzstd 实测成功（golden .zst）；口径矩阵：codex input 剥 cache / pi·omp 原样 / opencode output 加 reasoning
- [x] 07 ConsumptionView 合并去重（P2/P3）— app/usage/consumption.rs 纯函数；Record.request_id 退化接口（record_request_id 恒 None，切片 10 补字段后换函数体激活主键级）
- [x] 08 价格源（P3，⛩tree-03+06）— cost.rs 三级（prices 键 > catalog cost > None）；PriceRow 带.currency 预留；形状不符=整文件拒绝（对齐 doc.rs 语义）
- [x] 09 Summarize + 今日消耗（P3，⛩tree-09）— consumption/summarize.rs 纯函数（UTC 日界）+ service/summary.rs 三源组装 + get_consumption_summary + Usage 页今日行；by_catalog=经网关口径
- [x] 10 upstream 接线与 turn 状态机（P4，⛩tree-04+05）— forward.rs 状态机（env>静态测试源>502）；UpstreamEnv{resolve,book,attribute}；app resolve 最小实现（group 展开+openai_chat 端点）；401 透传+Auth 归因（11 前置）
- [x] 11 401 自愈（P4，⛩tree-09）— D9 三限钉死；HealingBook 常驻 heal 线程桥（跨 runtime 探针通过，未触发队列降级）；AUTH_REST 30 分钟与 Verify 同档
- [x] 12 AllowanceSnapshot 口径收敛（P5，⛩tree-04）— `{percent, renews_at}` 统一：语义漂移字段 `used` 改名 `percent`（钉死跨 provider 不可比）；rest.rs 的 renews 旁路注入删除，坐窗（≥98 且未来）统一读 snapshot；account_book 取最紧张窗口（max percent + 同窗 reset_at，延续 max 语义）
- [ ] 13 交叉视图与文档收口（P5）

## 头号事实：路由编排未接线（三份草稿独立证实）

`ServeOptions::from_env()` 的 `upstream` 恒为 `None`（`crates/skillstar-gateway/src/serve.rs:105-123`），生产仅有的两个启动方（CLI `skillstar gateway serve` 与桌面后台，`app/src/cli/gateway.rs:26,53-63`）都不注入；`forward_body` 在无 upstream 时直接 502（serve.rs:374-376）。`route_smart` / `sign_upstream` / `UsageAccountBook` / `rest_after` 全部**已导出、已测试、未接线**（全仓 grep：sign_upstream 只有测试调用方）。

因此：P4（切片 10-11）是**首次建造** turn 转发状态机（路由→签名→401 重试→记账接进 dispatch），不是修复；P1 账本在 10 落地前只能记 502 与本地路由流量（测试用 tests/serve.rs 的 fake upstream 模式验证）；P0 的 LAN 暴露是「接线前的止血」——**门必须先于线**（防火墙 8）。

## 目标

补齐对抗审查（2026-10-02，对比 magpie + cockpit-tools）认定缺失的**度量面**与**安全面**：

- P0 安全：网关入站鉴权（LAN 门禁）+ 账户簿真相源改道 custody（止住"解密存储行签名"这条错路）；
- P1 账本：网关持久 JSONL 账本（替换进程内 60 条环）；
- P2 会话解析：受管 6 agent 本地会话文件只读解析（增量 checkpoint），与账本合并去重；
- P3 成本：models.dev 价格 + 用户覆盖价 + Summarize（today/7d/30d × agent/model/account）；
- P4 凭据通道：upstream 首次接线 + 401 adopt 单次重试自愈；
- P5 交叉视图：今日消耗行、候选 chip、路由可比口径，文档收口。

参照实现：`/Users/xxww/Code/REPO/magpie`（internal/usage 账本与 gatewayMatches 去重、internal/sessions 增量解析、internal/gateway/lan.go LAN 鉴权模式）。

## 已拍板决策（实现者继承，不再讨论）

| # | 决策 | 结论 |
| --- | --- | --- |
| D1 | 范围 | 全量 Phase 0-5 一个 spec（用户拍板） |
| D2 | 鉴权模型 | **magpie LAN 模式**：loopback 放行任意 bearer（归因通道 `skillstar-<agent>`、选择通道 `skillstar/<model>`、omp `auth:none`、本地 GET 面全部不动）；**非 loopback peer（LAN/WSL NAT）强制 key**，槽位 = Authorization / x-api-key / x-goog-api-key / ?key=。理由见 choices C2：key 文件用户可读，严格校验 loopback 对同用户攻击者增益≈0 |
| D3 | gateway key | 安装级随机（≥32 字节），`config_dir()/gateway.key`，0600 原子写（先例 redact.key）；纯内部，无 UI、无轮换、不进 DTO/日志/账本 |
| D4 | 会话归因 | 优先读原生会话头（affinity.rs:28-35 的 `session_id()` 已支持 X-Skillstar-Session → 原生头清单 → body 派生，dispatch 目前没提取——切片 03 补管道）；apply_gateway 注入 X-Skillstar-Session 仅用于**实测无原生头**的 agent，注入不了就留空、由切片 07 的会话文件归因兜底（choices C4） |
| D5 | 解析范围 | 受管 6 agent = AGENT_SPECS 的 6 个：claude-code、claude-desktop（同 projects JSONL 家族，magpie calls.go 先例）、codex、opencode、pi、omp。zcode 出范围（input 含 cache 口径需单独校准） |
| D6 | 兼容 | 硬切：无兼容层、无迁移脚手架；账本/DTO schema 一次定形不预留迁移 |
| D7 | crate 边界 | gateway 不新增对 usage/models 的依赖（boundaries.md:129 红线）；凭据/上游知识经 `AccountBook` trait 与注入供给 |
| D8 | 成本语义 | 读时计价：账本只存 token，价格表会变（magpie 同款）；`effective_price` = model_gateway.json 顶层 `prices` 键（经树 spec store lens）> models.dev 缓存 |
| D9 | 401 自愈三限 | 仅 401（403 永不触发，`request.rs is_auth_error()` 与 usage README 错误分级表是裁决点）、仅一次、仅未 committed（rest.rs:100-112 语义）；`retried` 是 turn 栈变量，循环在结构上不可能 |
| D10 | 账本位置与轮转 | `data_root()/gateway/usage.jsonl`（网关自有派生数据）；轮转策略委托实现但下限强制（按大小或按月分文件，老行可丢——度量面不是审计面） |

## 与树 spec 的前置表

| 本 spec 切片 | 碰 tree 移动的文件 | 前置 |
| --- | --- | --- |
| 01/02/03/05/06 | 无（新文件 + serve/listen/account_book 原地） | 无（account_book 在 tree-07 才移动，02 先做则原地改，tree-07 顺带搬） |
| 04 | app/models/recent.rs → tree-07 移入 gateway/ | ⛩ tree-07 |
| 07 | 无（app 新文件） | 无（但需要 03+05/06 的数据形状） |
| 08 | catalog 解析（tree-06）、store/doc.rs（tree-03） | ⛩ tree-03 + tree-06 |
| 09 | app/usage/service.rs（tree-09 拆分） | ⛩ tree-09 |
| 10 | route.rs/store lens（tree-04/05） | ⛩ tree-04 + tree-05 |
| 11 | app/usage/service.rs | ⛩ tree-09 |
| 12 | route.rs（tree-02/04） | ⛩ tree-04 |
| 13 | 前端 + docs | 无 |

两 spec 并行纪律：任何切片不与 tree 的切片改同一文件同一 commit；tree 的 lib.rs pub use 冻结（D3）对本 spec 同样有效——本 spec 新模块只**新增**导出（如 `ledger`、`access`），不动旧清单。D2 决策下 `PLACEHOLDER_BEARER` 保留，与 tree D3 无冲突。

## 全局防火墙（违反即拆片重做）

1. **账本 append 失败绝不打断 turn**：IO 错误降级内存环 + tracing warn；故障注入测试钉死（magpie 血泪规则）。
2. **key 与凭据红线**：gateway.key 0600 原子写；key/token 明文不进 DTO、事件、日志、账本、trace——账本 account 字段用 `key:<sha256 前 8 hex>` 指纹（先例 custody.rs:668-677 orphan_id）；沿用 trace.rs:155-175 的 Debug 断言测试模式。
3. **gateway 不 import usage/models**；每片跑 `check_workspace_deps.sh` + `check_dep_graph_doc.sh`。
4. **会话解析只读** agent 自己的文件；写 agent 目录的唯一路径仍是 apply_gateway 既有接管机制。测试全部走 `SKILLSTAR_TOOL_SYNC_HOME` + `SKILLSTAR_DATA_DIR` sandbox，绝不碰真实 $HOME。
5. **gateway 线（01-04、08 半、10-12）与 usage 线（05-07、09、11 半）不同提交**。
6. **门先于线**：切片 10 禁止先于切片 01 合并（接线即真实暴露面成立）。
7. **401 自愈三限**（D9）；锁序必须复用 usage 既有的 catalog serialization domain + CLI lease 序列（service.rs:363-400 是模板），禁止新造锁。
8. **serve.rs 不超 800 行**：key/ledger/forward 各进新文件；每片 `check_file_size.sh` + `check_clippy_ratchet.sh`，**两个 spec 的 baseline 都零改动**。
9. 硬切（D6）：每片涉及 DTO 时 `bun run types:gen && git diff --exit-code src/types/generated/`。
10. 每片完成即更新被打破的文档声明（AGENTS 同序列规则）：models/README.md:46/99/105/113、usage/README.md:15、architecture.md:128/139/142-146——具体落点在各切片。

## 验证门槛（每片至少）

```bash
cargo check --workspace --locked
bash scripts/internal/check_clippy_ratchet.sh
bash scripts/internal/check_file_size.sh
```

gateway 片：`cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port`；usage 片：`cargo test -p skillstar-usage --locked`；app/前端片：`cargo test -p skillstar-app --locked` + `bun run types:gen`（零 diff）+ `bun run lint && bun run build`。

## 已知未知

- omp 是否能在 models.yml 携带任何鉴权字段（切片 01 检查点；D2 模型下 loopback omp 不受影响，仅 LAN 下不可用——写进文档即可）。
- 网关线程跨 runtime 进 usage 异步锁（切片 11 探针测试；死锁则降级异步自愈，见该切片）。
- ruzstd 解 codex .zst 的完整性（切片 06；不行换 zstd，再不行 .zst 首期不支持并记 docs/errors.md）。
- claude-desktop 的 Code 标签会话实际路径（切片 05 验证；与 claude-code 不同则独立 discovery、同 parser）。

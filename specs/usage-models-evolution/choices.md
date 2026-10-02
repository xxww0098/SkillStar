# choices.md — 终局裁决账本

本 spec（含 ⛩ 前置 tree spec 的 10 片）已全部实现并收口。本文件是从零重写的最终合并账本：按裁决分组、组内最不自信在前，每条独立可读（ELI5：先一句话讲给完全没跟过的人），只记裁决与理由，不记过程与门槛结果。实现者与审查者从这里读分歧的终态，不重开讨论。

## 一、前提后来被修正的裁决（结论保留，理由改写）

最不自信的一组：立项时的依据有一处被证明不成立，最终裁决与原表述不同。

### 1. 消费合并的主键级匹配恒空供给（修正 S7 的激活承诺）

ELI5：把「网关记的调用」和「agent 自己会话文件记的调用」合并去重时，原本设想第一步按请求 id 精确配对；实际这个第一步永远空转，配对全部靠第二步（会话 + token 数 + 时间）完成——而且是**故意**让它空转的。

裁决：切片 07 曾记「切片 10 给网关 Record 补 request_id 字段后激活主键级」。终局修正：magpie 的主键匹配成立，是因为它的网关是 Anthropic 协议直通——SSE envelope 里就带着同一个 `req_…` id。SkillStar 的网关做协议翻译，会话文件记录的 `req_…` id 只有协议直通网关才看得到；网关能提取的上游 body id（`chatcmpl-…`/`msg_…`）与它永远不相等。给 Record 加字段只会造一个永远 join 不上的死重。终局：stage-1 匹配逻辑保留且经投影缝可测，但 `record_request_id` 恒返回 `None`；若未来某上游回显 agent 自己的请求 id，换函数体即激活。

### 2. 「序列化不 skip」按「缺席不造」实现（tree-03 契约措辞修正）

ELI5：store 的 typed schema 契约原写「序列化时任何字段都不跳过」；真做会发现 provider 行凭空多出 `"members": []`，round-trip 测试根本过不了。实际语义是「载入的不丢、缺席的不造」。

裁决：已知字段带 `skip_serializing_if`（None/空集合/空串 = 键缺席），`rest`/`extra` 无任何 skip。`None`/空 = 键缺席正是 Smart/Auto「删键」lens 需要的把手。措辞已在 tree 账本 T3 调和，此处仅存目。

### 3. 「生产网关无 upstream」是立项快照，不是现状（头号事实的时效框定）

ELI5：spec 开头那段「路由编排未接线」的三份草稿共同发现，写的是立项时刻的代码；切片 10 之后两个启动点都注入 UpstreamEnv、sign_upstream 有生产调用方。段落保留（它解释 spec 为何这样切），但已加「立项时事实」框定行，避免读者当作现状。

## 二、接受的偏差与已知边界（有意为之，后续接手者不要顺手"修复"）

### 4. 账本保留上限：6 个归档 + 活文件（≈35MB）

ELI5：账本会一直长大，所以要定一个「最多留多少」：最多 6 个 5MB 归档加一个活文件。轮转时最老的归档删掉、其余顺移重编号（编号无洞，读取按编号序走的语义不变）。依据 D10「度量面不是审计面，老行可丢」。

### 5. 路由对照恒读全量保留集

ELI5：ModelsHub 的「同模型各候选对照」（p50/p95、错误率、成本）聚合的是账本保留的全部记录——这是刻意的口径契约（用户对照的数字恒同 scope），不做时间窗。保留上限（裁决 4）使这个全量读有界。`LedgerQuery.since` 下推只服务时间窗读（今日/汇总），对照面板 `since: None`。

### 6. heal 队列容量 1：突发 401 时 1 个在跑 + 1 个在等 + 其余透传

ELI5：凭据失效自愈一次只允许一个刷新在跑、最多一个排队；更多排队中的请求直接放弃（401 透传给 agent，候选进 30 分钟 AUTH_REST 座位）。容量 0 的「纯会合」通道有启动竞态（桥线程还没就位时发送必失败），所以是 1 不是 0。

### 7. reauthorize 是同步钩子，最多占一个 runtime worker 5 秒

ELI5：401 自愈的钩子在异步转发路径里同步等最多 5 秒。网关 runtime 是多线程的，这只占住一个 worker；401 本身稀少（单次 + AUTH_REST 30 分钟封顶）。把钩子改异步需要把注入 seam 从 Box 改 Arc（动一片测试 fake），收益不值——接受现状。同理，heal 桥线程与进程共享 reqwest 客户端（池对象挂在停靠 runtime 上）是观察项，探针通过，未演示失败。

### 8. 跨 provider 余量不可比；tightest window = 最大 percent + 同窗 reset

ELI5：各厂商配额窗口口径不同，70% 不能和另一个厂商的 70% 比大小。路由与展示只用「同一 provider 内部」的相对序。取「最紧张窗口」时 renews_at 必须来自同一个窗口，不把宽松窗口的 reset 时间外漏。`AllowanceSnapshot { percent, renews_at }` 两个字段钉死这个语义。

### 9. Anthropic 回译 SSE 的 502 是既有 bug，本 spec 只保证 usage 不丢

ELI5：Anthropic 入站请求翻译到 OpenAI 上游、回复要翻译回 Anthropic 流式格式；回复流聚合的翻译失败会 502（agent 没拿到回复）。这个 bug 在 translate.rs，修复需要 SSE 聚合，不属本 spec。本 spec 保证的是：上游原始字节在翻译前已入账本，token 数不丢。

### 10. 消耗浮窗不显示今日行

ELI5：Usage 页主网格的卡片有「今日消耗行」和会话 chips；悬浮预览窗（UsageCardWindow）组合的是另一套组件，没有今日行。接入需要额外一次 today 读取，切片 09 时记录为边界，未做。

### 11. 路由模式只有 group ref 读存储值；裸 ref 多候选恒 Smart

ELI5：RoutingControl 里用户能选四种路由模式（smart/order/rotate/usage）。接线后的规则：模型 ref 是 `group/<id>` 时读该 group 行存储的模式（rotate 用进程级相位计数，重启相位归零、策略不变）；单个 provider 无所谓顺序；裸模型 ref 命中多个 provider 时没有唯一 owner 行可读，恒 Smart。

### 12. 双网关进程共享一个 data_root 不支持

ELI5：账本的追加互斥锁是进程内的；两个 serve 进程同时轮转理论上可能互相覆盖归档。这是不支持的部署形态（一个安装跑一个网关），接受。

### 13. 零散小边界（都验证过、都接受）

- store 透镜行匹配的 trim 分歧（groups 按 trim 匹配、routing 精确匹配）：tree-01 行为锁分别钉住的既有语义，保留不改。
- models 与 usage 前端互用对方的 i18n 键（todaySessions/viaGateway 共享词汇）：检查门不覆盖，接受词汇共用。
- 无入站 body 大小上限：serve.rs 既有形态（本 spec 之前就是），信任边界是环回或持 key 对端，不扩权修复。
- 会话 checkpoint 索引把每文件全量调用列表存进 `index.json`，随历史线性增长：度量面可丢弃重建，接受。
- codex 不可解析时间戳行的 token 差分被吞（该行 total 已推进但无 emit）；pi/omp 同尺寸原地重写且前 256 字节不变的检测盲区：解析器已知极限，测试钉住可接受面。
- heal 的 30 秒硬停可能落在「厂商已消耗刷新令牌、新令牌尚未落盘」之间：下一次 heal 遇死授权锁 requires_reauth，属 errors.md 已载的降级链。
- claude-desktop 主 Code 标签正文文件本机未找到；Desktop 内嵌会话经 `claude-desktop-3p` entrypoint 前缀归因可用。

## 三、已被代码与测试证实的裁决（按主题收拢）

### 安全与凭据（P0）

- **LAN 门禁 = magpie 模型**（D2）：loopback 放行任意 bearer（归因通道 `skillstar-<agent>`、选择通道 `skillstar/<model>`、omp `auth:none`、本地 GET 面全部不动）；非 loopback 对**所有**非 callback 请求强制安装级 key，四槽位 Authorization(Bearer 剥前缀)→x-api-key→x-goog-api-key→?key=，任一命中即过。理由：key 文件 0600 用户可读，严格校验 loopback 对同用户攻击者增益≈0，而会炸掉四条无辜通道。无路径白名单（magpie lanGuard 同款全拦）。
- **key 文件**（D3）：`config_dir()/gateway.key`，≥32 随机字节的 hex，照抄 redact::write_key 先例 0600 写（先例本身非原子：崩溃最长留下截断文件，读回 <32 字符视为无效自动再生成，语义自愈）。无进程缓存（每次非环回读一次，换测试沙箱可隔离）。比较为常数时间，长度不等折为不等式而非 XOR（XOR 折进 u8 会被抵消）。IPv4-mapped IPv6 环回地址（`::ffff:127.0.0.1`）在两处门（LAN 门与 claude callback）都按环回处理，LAN 侧 mapped 地址保持非环回——无旁路。
- **凭据改道 custody**（切片 02）：签名材料 live-first 三态（Live/Row/Diverged），`Custody::probe` 纯读无副作用；probe 出错降级行回退 + warn；Row 态 subscription_id=Some(行 id) 供账本归因；无 CLI target 的 catalog 直接行回退。
- **密钥防火墙**：key/token 明文不进 DTO、事件、日志、账本、trace。账本 account 字段 = 订阅 id 或 `key:<sha256 前 8 hex>`。手写 Debug 脱敏覆盖所有携带明文的类型（SigningMaterial、TurnFacts、Record 混淆、ProviderSnapshot、SignedUpstream（只打 header 名）、AccountSnapshot）。
- **账本 append 永不失败 turn**（防火墙 1）：IO 错误 warn + 留内存环；崩溃留下的撕裂尾在下一次 append 前补换行修复（旧尾丢弃、新行完整）。

### 账本与读取面（P1/P4 读取侧）

- **位置与形状**（D10/C9/S3）：`data_root()/gateway/usage.jsonl`（网关派生数据，不是用户配置）；一行一 turn，O_APPEND + 进程 Mutex 双保险；5MB 轮转；schema 硬切一次定形（D6），8 种 ErrorKind 全部定义（quota/verify 由切片 10 词表产出）。
- **上游原始字节入账**：回译失败 502 时 agent 无回复但 token 已耗——从翻译前的原始字节记账，字段 None 同时是 `local` 判据。
- **读取语义**：`load(since)` 按归档序读完整行；`LedgerQuery` 维度过滤 + 最新优先分页 + since 下推；环补缺合并——环只补账本缺的行，覆盖判定用 (agent, model_asked, status, output tokens) 匹配而非时间（环记完成时刻、账本记派发时刻差一个 latency）。

### 会话解析（P2）

- **范围**（D5/C5）：受管 6 agent = AGENT_SPECS 钉死的 6 个（claude-code、claude-desktop 同家族共享 parser、codex、opencode、pi、omp）；zcode 出范围（input 含 cache 口径需单独校准）。
- **增量 checkpoint**（C7）：magpie sessions state 形状——`v1:<长度>:<hex>` 头指纹（长度参与：短文件增长后头窗口变长）+ `sample-v1:` 采样前缀（≤64KiB 全量否则 16×4KiB）+ size/offset + 双层版本失效（store 版本 + parser 版本，不符全量重读，无迁移）落 `data_root()/sessions/index.json` 原子写。未增长且头一致不打开正文；写一半的行不消费、offset 不推进。
- **read_calls 全量视图**：每次经 replay 从 checkpoint 重建 + 跨文件 msg-id 去重（最早文件优先），消费方幂等整体替换；trait 形状 = PARSER_VERSION 关联常量 + replay + 对象安全分发面。
- **口径矩阵**（S6）：codex `input -= cached`（饱和减）；pi/omp input 原样；opencode input 原样 + output 加 reasoning；codex token_count 差分三态（单调取差/回退或首帧取 last/重复 total 零调用）；omp 子会话归并 root 且 task 工具 `details.usage` 不读（双计防线）；codex/pi/omp 的 msg id 恒空不参与跨文件去重。
- **ruZstd**（C6）：ruzstd 纯 Rust 解码成功，无降级；损坏帧 warn + 空视图重试，不记 errors.md。
- **归因**（D4/C4）：管道优先——dispatch 提取 session 头（X-Skillstar-Session → 原生头清单 → body 派生三级回退），apply_gateway 注入仅是实测无原生头 agent 的补充手段。

### 消耗合并与成本（P2/P3）

- **两阶段匹配**（S7）：stage-1 request id（实现可测、恒空供给，见裁决 1）；stage-2 = 同 (agent,session) + 四 token 数相等 + 文件时间戳在记录结束时刻 ±2s + 成败一致 + 零 token 只配 failed-failed + 双向唯一（歧义放弃、两行都可见）。不比较模型 id（magpie 同款，`matching_ignores_model_ids` 钉死）；`same_model` 归一函数（context 尾巴/13 厂商前缀/version atom 链，1-2 位数字不是 atom）公开供对照用。
- **价格**（D8/C8）：读时计价（账本只存 token）；`effective_price` 三级 = model_gateway.json 顶层 `prices` 键（`<catalog>/<model>` 精确 > `<catalog>` 通配，经 store typed lens 读）> models.dev 目录 cost > None；查不到 = unpriced ≠ 免费；查价键 = (catalog, 应答模型优先)——该规则本轮收敛为 `Record::served_model()` 单点（crossview/ledger 投影/summarize 共用）；每命令 memoize（同一 (catalog,model) 只读两次价格文件）。
- **Summarize**（S9）：UTC 日界为 wire 契约，与 Window 的本地日界刻意并存（读池 24h 配对缓冲）；by_catalog = 经网关口径（bypass 行只进 totals）；Period（UTC）与 Window（本地）是两个概念不混用。
- **交叉视图**（S13）：对照五列只聚合该候选 catalog 的账本记录（scope 封闭）；resting 与 allowance 是进程事实经 CandidateFact 注入，不造第二真相；候选排序 = route_smart 原序；三角导航走 nav bridge focus 事件不造 URL 形状；StackedTokenBar 纯 CSS 不引图表库。

### 上游接线与自愈（P4）

- **UpstreamEnv 形状**（S10）：`resolve: Fn(&str) -> Vec<Upstream>`（owned 候选带 id/catalog_id/endpoint/provider；借用无法从 Box<dyn Fn> 返回）+ book + attribute 惰性求值；优先级 env > ServeOptions::upstream（静态源降级为纯测试路径，14 个既有 serve 测试零改动）> 502；两路径共用 send/finish 机体。
- **轮转谓词** rotates = 402|408|429|5xx；其余 4xx 不轮转（请求自身的错）；传输失败 → backoff rest → 下一候选；bridge 候选不发 HTTP。
- **401 自愈三限**（D9）：仅 401（403/429/5xx 永不触发）∧ 每 turn 一次 ∧ 未 committed；gate 单独成函数可测；`retried` 在重发前置位，循环结构不可能。AUTH_REST 30 分钟；锁被占超时同样退避（防死登录逐 turn 重放；代价一次瞬态竞争停 30 分钟，重启即清，errors.md 已载）。
- **HealingBook 形状**（S11）：trait 默认方法 reauthorize（None）+ 装配处包装（account/allowance 纯委托）——对 book 零依赖；常驻 heal 线程持自己的 current-thread runtime，turn 侧 sync_channel recv_timeout（5s 软等/30s 硬停），不嵌套 runtime；锁序复用 catalog serialization + CLI lease 模板，无新锁；off-turn 完成落 latch。
- **app resolve 边界**：endpoint 仅取 openai_chat 且剥尾 /v1；glm /v4 根等异形端点暂不成候选；订阅账户的 catalog↔账户映射是后续工作。

### 树重构（⛩ 前置，裁决细节见 tree 账本）

行为锁先行（tree-01）→ store/route/catalog 目录化 + doc.rs typed schema owner（「载入的不丢、缺席的不造」）→ 读写经 lens → catalog 单点解析 → app 投影分组 → service 按用例拆分 → families 只写契约不实现（实现时另立 spec 引用，四个文件零决策）。families 一等化契约：owner family（行上标签）与 catalog family（目录事实）互不相干；members 不内联（双向真相冗余）。

### 本轮 review 补充的实现裁决

- **served-model 计费键单点化**：「应答模型优先、空则回落 asked」原复制四处（summarize/crossview×2/ledger 投影），收敛为 `Record::served_model()` + `summarize::served_model` 两处（各管自己的类型）。
- **pinned-row 单点化**：「pinned 行优先、否则 catalog 首行」原复制三处（signing/account_book/healable_row），上提为 `usage_switch::pinned_row` 一处导出。
- **会话延迟保护共享**：`MAX_LATENCY_MS`（2h）从 codex/pi 两处同名重复上提到 sessions/mod.rs 单点。
- **库内环境单测共享一把锁**：设 `SKILLSTAR_DATA_DIR` 的单测必须共用 `TEST_PATH_ENV_LOCK`（各自 static 锁在并行测试下互踩 data_root）。
- **fs_ops 毫秒碰撞**（基线 bug，非本 spec）：备份名毫秒时间戳紧循环碰撞 → `+1` 顺延直到名字空闲；后缀保持纯数字，cleanup 的解析与排序不受影响。

## 四、流程裁决（对后续工作持续有效）

- **代码注释一律英文**；中文只在 Display/UI 字符串与 docs/。
- **网关测试统一 `-- --skip serve_binds_default_port`**（本机端口占用是环境性失败）；满载并行下的 socket 预算统一 20s（5s 在全量套件下确定性翻车）。
- **`cargo check` 不带 `--all-targets` 不编译测试文件**——验证中间态必须带。
- **人工中间态提交可 `--no-verify`**（pre-commit 快检对「工作树含未来切片文件」的中间态误报）；每个 lane 的终态提交全钩通过。
- **子代理只许 `mv`、不碰 git index**（一次 `git mv` 污染了泳道提交，soft-reset 重建）。
- **DTO 变化后 `bun run types:gen` 零意外 diff 是每片门槛**；gateway 线与 usage 线不同提交（防火墙 5）。

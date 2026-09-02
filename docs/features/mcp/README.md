# MCP 管理与市场投影

状态：active

本文件维护 MCP 本地 store、Agent tool sync、多源 catalog、安装流程契约和前端页面职责。模块布局见 [boundaries.md 的 MCP 模块布局](../../boundaries.md#mcp-模块布局)。

## 三类模型不可混用

- `skillstar_models::mcp`：用户本地 MCP store、server patch、preset、tool status、sync result 和健康探测报告。
- `skillstar_marketplace::{mcp_models, mcp_remote, mcp_snapshot}`：Marketplace Publisher、源描述符、registry package/remote、Input 语义、卡片查询与 detail。
- `skillstar_app::mcp`：把前两者接起来的跨域 use case（形态候选、草稿、安装计划、preset 芯片编排与映射）。
- `src-tauri::commands::mcp_commands::McpServerWithSync`：命令层返回的 server + per-tool sync DTO。

这四组 Rust 类型通过 ts-rs 导出到 `src/types/generated/`，`src/types/mcp.ts` 只做 re-export。修改字段后运行 `bun run types:gen`；不得在 TypeScript 手写第二份大型 wire type。

## 多源 catalog

- catalog 不是「某一个 registry 返回了什么」，而是所有启用源的合并结果。每个源由一个 `McpSourceDescriptor` 描述；新增源是数据，不是控制流。
- 合并权威由 `priority` 编码（越小越权威），语义是**权威性**而非偏好：官方 registry 是发布者自己的记录且是唯一明确 CC0 1.0 的源，GitHub registry 是同一 OSS 快照的镜像但额外带 stars/license/readme，用户源永远排在内置源之后。具体源清单与 priority 值以 `mcp_remote::sources` 代码为准，本文件不抄枚举。
- **合并主键是 `server.json` 的 `name`（反向域名全名），不是各源自己的 id。** 同一个 server 在不同源下 id 不同，用 id 做主键会把同一个 server 重复上架。
- 用户自定义源持久化在 `<config_dir>/mcp_sources.json`（经 `skillstar_core::infra::paths` 解析，`SKILLSTAR_DATA_DIR` 覆盖继续生效）。支持两种形态：`registry`（`v0.1` 形状的 REST 端点）与 `localDirectory`（本地 JSON 文件，`{"servers":[…]}` 或裸数组）。该文件读失败一律降级为「没有自定义源」，绝不能让一个坏配置把内置源一起带下线；写入走临时文件 + rename。
- 用户源 id 强制带 `custom:` 前缀，永远不可能遮蔽内置源 id。

## server.json `2025-12-11` 字段覆盖

- packages 侧：`registryType`（含 `npm`/`pypi`/`nuget`/`cargo`/`oci`/`mcpb`）、`registryBaseUrl`、`fileSha256`、`runtimeHint`、`transport`、`runtimeArguments`、`packageArguments`、`environmentVariables`。
- remotes 侧：`type`（`streamable-http` / `sse`）、`url`、`headers`、`variables`。
- server 侧：`title`、`websiteUrl`、`icons`、`_meta` 的 `status` 与 `isLatest`、`publishedAt`。
- Input 语义完整保留：`isRequired`、`isSecret`、`format`（`string`/`number`/`boolean`/`filepath`）、`choices`、`default`、`placeholder`、`value` 模板与其 `variables`。旧解析只留下「必填/密钥变量的名字」，表单因此无法区分必填与密钥、也无法渲染下拉框。
- `variables` 只解析一层。schema 允许无限嵌套，但实践中从未出现，递归类型会导出成自引用的 TypeScript 别名。

### status / isLatest 语义

- `status`：`active` / `deprecated` / `deleted`。`deprecated` 仍然上架但**必须标注**；`deleted` 不进默认列表。两者以前都未解析，弃用的 server 与健康的 server 被一视同仁地推荐安装。
- `isLatest`：registry 已知有更新版本时为 `false`。默认反序列化为 `true`——`#[derive(Default)]` 会让它变成 `false`，那样每一行用 `..Default::default()` 构造的数据（curated 种子、测试）都会被 UI 标成过期。
- 查询默认**不**按 status 过滤：registry 自己的规则就是「列出但警告」。

## 查询 API

- `query_mcp_servers_local(&McpServerQuery) -> LocalFirstResult<McpServerPage>` 是浏览路径的唯一入口：支持 search（FTS）、publisher、kind、runtime、license、status、recommended、latest、stars 区间、排序方向与 limit/offset。
- `McpServerPage` 同时返回 `total`，所以「显示 60 / 共 21363」不需要第二次往返。
- 全量未分页的 `list_mcp_servers_local` 只适用于小的 publisher 分桶。合并后的 catalog 是两万量级，把它整体推过 IPC 再在渲染进程里内存过滤会直接卡死界面。
- 排序键是枚举映射到固定的 `ORDER BY` 片段，所有值绑定为参数；调用方永远碰不到 SQL 文本。

## degraded 可观测性

- sync state 有两种 scope：`mcp_registry`（UI 新鲜度横幅读的聚合）与 `mcp_registry:<source_id>`（每源一行，带自己的 ETag、payload hash 和 `degraded_reason`）。
- 「某个源失败/被截断」和「catalog 完整」必须可区分：一次四源里挂了一个的同步仍然算成功，只有 `degraded_reason` 能说清楚为什么结果不完整。前端要能显示「本次同步不完整，原因 X」。
- `degraded_reason` 在成功路径上**总是**写入：完整同步时清空它，是一个曾被截断的 catalog 停止被报成截断的唯一途径。

## 运行时形态选择

一个 server 可能同时给出 `remotes[]` 与 `packages[]`。旧行为是「有 packages 就取 `packages[0]`，否则取 `remotes[0]`」——由数组顺序决定运行什么，既不是安全判断也不是可用性判断。现在的优先级（`skillstar_app::mcp::runtime`）：

| rank | 形态 | 理由 |
| --- | --- | --- |
| 0 | `remotes[]` `streamable-http` | 零工具链依赖、零本地代码执行、走标准 OAuth |
| 1 | `remotes[]` `sse` | 可用，但传输已弃用——**永远标注** |
| 2 | `packages[]` `oci` | 本地但容器隔离，安全性最好的本地形态 |
| 3 | `packages[]` `mcpb` | 预编译产物；`fileSha256` 由客户端而非 registry 校验 |
| 4 | `packages[]` `npm` / `pypi` / `nuget` / `cargo` | 需要对应工具链 |

- **rank 不是最终答案。** `runtimeHint` 只是提示：没装 Docker 的机器跑不了 OCI 包，rank 再高也没用。每个 stdio 候选都用 `skillstar_models::mcp::resolve_runtime` 对真实 `PATH` 做一次探测（与后续真正启动进程用的是同一个 resolver，两者不可能不一致），不可用的候选排在所有可用候选之后。
- **选择结果可被用户覆盖。** 选择器返回**全部**候选（各带 rank、可用性、阻塞原因和警告）加上推荐项 id；安装命令接受任意候选 id 覆盖推荐，未知 id 退化为推荐而不是报错。
- `mcpb` 目前列出但不可安装：SkillStar 还没有「下载并校验 `fileSha256`」这一步，而 registry 明确不做校验。发布者没声明 `fileSha256` 时额外警告。
- `cargo` 没有 `npx` 式的一次性运行器，条目通常也没有 `runtimeHint`；没有 hint 的 cargo 候选直接标为不可安装并给出 `cargo install` 指引，而不是拼一条跑不起来的 `cargo <crate>`。
- **「本机缺工具链」与「这个形态根本表达不出来」是两种失败。** 前者（`npx` 没装）仍然预填草稿——条目本身是对的，装上运行时就能起来，给一张空表单帮不了任何人；阻塞原因由安装计划陈述。后者（mcpb、无 hint 的 cargo、没有 runner 的包）绝不作为预填来源，否则会拼出一条本身就是错的命令行。
- 选中的形态写进 `McpServerEntry::runtime_kind`，与 store 的来源指纹是同一套词汇。

## 手动添加表单的传输标签

内部 store 仍是三个值：`stdio` / `http` / `sse`。`http` **就是** Streamable HTTP，对应 `2026-07-28` 无状态协议（无 `initialize` 握手、无 `Mcp-Session-Id`，每次请求自带 `MCP-Protocol-Version`）。

表单不得把这三个 token 原样画给用户：

| store 值 | 展示名 | 说明 |
| --- | --- | --- |
| `http` | Streamable HTTP | 推荐远端。URL 占位符是 `/mcp`，不是 `/sse`。 |
| `stdio` | 本地进程 | 本机启动命令。 |
| `sse` | SSE（已弃用） | 只在发布者没有 streamable HTTP 端点时使用。 |

健康检查把 `epoch: modern` 展示成「无状态 · 2026-07-28」，把 `legacy` 展示成「会话握手」，不要把内部词 modern/legacy 暴露给用户。`tools/list` 带来的 `ttlMs` 一并显示。

## 来源指纹

安装时记录，用于更新检测、弃用提示和精确的「已安装」判定——不再依赖 server 名字字符串：

- `registry_name`：`server.json` 自己的反向域名全名，与写进工具配置的 sanitized key 不是一回事。
- `installed_version`：选中包的版本，回落到 registry 给的 server 级版本。
- `source_id`：这一行主要来自哪个源；SkillStar 自己的 curated 行没有远端源，落到 publisher 分桶。
- `runtime_kind`：实际选中的形态，不是猜的。

四个字段都是 `Option` + `#[serde(default)]`：旧版本写的 store 一个都没有，必须继续逐字节解析。拿不到可信值时留 `None`，不编造。

## 安装前的命令确认

- 一键安装本地 server 前，必须把**完整未截断的、已解析的**命令交给用户确认。这是规范的 MUST，也是 CursorJack 类 deeplink 攻击的唯一有效缓解。
- 安装计划（`skillstar_app::mcp::install`）同时给出：将要执行的命令与参数、`PATH` 解析出的绝对路径（真正会被执行的那个二进制）、`usesShell: false`（launcher 直接 exec，registry 作者的参数字符串永远不进 `sh -c`）、全部运行时候选、以及每个表单字段的完整 Input 语义。
- 表单字段按 `(scope, 序号)` 寻址，不按名字。位置参数没有名字，展示标签退化到 `valueHint` 或字面量 `argument`，两个都没有 `valueHint` 的位置参数因此共用同一个标签；只有所属 scope 内的序号能把它们分开，否则填第一个会连带改掉第二个。
- `commandPreview` 只用于展示，永远不被重新解析或执行；含空格/引号的参数加单引号，好让用户看清每个参数的边界。渲染只有 Rust 一份实现。
- 用户填写的答案在**参数被扁平化之前**代入草稿的三个预填点（包参数、环境变量与请求头、URL 变量），因此 `args` 是重新生成一遍，而不是在字符串数组上找位置插入。带 `default` 的命名/位置参数因此只出现一次，带 `{花括号}` 模板的参数带着替换结果落盘。
- 取值为空的命名参数**连同它的 flag 一起丢弃**：用户清空一个可选的 `--port` 后，命令行不应该留下一个后面什么都没有的 `--port`，多数 server 解析不了。判断依据是发布者自己给的信号——有 `default`、有 `valueHint` 或有 `choices` 的就是需要取值的 flag；三者皆无的是真正的布尔开关（`--verbose`），继续单独出现。
- 没人填过的环境变量与请求头**整行不写**，而不是写成空字符串。丢弃发生在 `registry_to_entry_answered` 里，所以安装计划的 draft 和第一份预览显示的是同一组行——只在预览侧过滤会让确认界面在首个预览到达前的约 300ms 里闪出永远不会安装的空行。
- 答案按 `(scope, 序号, 变量名)` 寻址，由 `mcp_market_install_preview` 送回后端：它是纯函数（不解析 `PATH`、不读文件系统），返回最终 entry、渲染好的命令预览与尚未满足的必填项。答案含密钥，因此这条命令的结果**不进任何缓存**（不得进入 query key），日志只记 id 与运行时形态。
- 提交走 `mcp_market_install`：它把答案、启用目标和**用户确认过的那份 `approvalTarget`** 一起交给 `prepare_install`（同样是纯函数，包在 `preview_install` 之上，不重新推导），后者重新推导后逐字比对，一致才返回校验通过的 entry；加锁、读 store、`create_server_and_sync` 与投影留在命令适配器。
- 比对目标是 `McpInstallPreview.approvalTarget`，覆盖确认界面上**全部**内容：命令行（远端形态则是解析后的 URL）、环境变量表、请求头表，以及写进各工具配置的那个 key（`entry.name`）。只比对命令行是不够的——目录同步可以在不碰 `packageArguments[]` 的前提下新增一条 `environmentVariables[]`，命令行逐字节不变，而用户从未见过的 `HTTP_PROXY` 就这样写进了每个启用工具的配置。该串由后端一处推导、前端原样回传，**前端不得自己拼**；它是不透明的（JSON 编码，避免值里的换行伪造出一行），给用户看的仍然是 `commandPreview`。
- 这条比对不是可选的：提交时目录行是**从本地快照重新读取**的，而目录同步可能在用户阅读预览期间改写这一行。比对的是未掩码的原串——掩码只发生在展示边缘（`maskSecrets`），先掩码再比对会让任何带密钥参数的安装永远对不上。
- 拒绝有两种，且必须可区分：`missingInputs` 指出缺的是哪一项，`commandChanged` 说明这已不是你确认过的那条命令。拒绝时什么都没写入，前端保留用户已填的内容，并同时**让安装计划失效**再重新拉一次预览：只重拉预览的话，屏幕上仍是旧命令；而安装计划有 60s 缓存，不失效就等于表单继续按旧那一行的字段顺序绑定——答案是按序号寻址的，新目录行如果在 `--port` 前插了一个位置参数，用户填的端口会悄悄变成一个路径参数。表单以**声明出来的 inputs**为重新播种的依据，行没变就不动用户填的内容。
- 后端只强制**必填性**这一条（以及上面那条重新确认）。`choices` 与 `format` 由渲染层校验——能给出下拉框或文件选择器的也只有它。没有渲染层的调用方（规范预期的 CLI）因此仍拿得到必填与重新确认这两项保证，但要自己套用 `choices` / `format`：`McpInstallInput.input` 原样带着它们就是为了这个。
- 手动新建路径继续走 `create_mcp_server`，**不受必填项校验约束**——它提交的是用户自撰的条目，强制发布者声明的必填项没有意义。

## 密钥分流

- 表单按 Input 语义渲染：`isSecret` → 密码框、`choices` → 下拉必选、`format: filepath` → 文件选择器、`format: number/boolean` → 数字框/开关、`default` → 预填、`placeholder` → 灰字提示（**不是**值）、`value` 已设定 → 只读。
- `value` 是模板时，它的 `{花括号}` 由安装计划直接下发成一份已播种的变量清单（每个变量带 `isRequired`/`isSecret`/`format`/`choices`/`default` 与初始值），前端不再自己扫描模板。变量只解析一层——schema 允许无限嵌套，实践中从未出现。
- **落点取舍**：secret 值只写入用户级配置——SkillStar 自己的 `~/.skillstar/config/mcp_servers.json` 和每个启用工具位于 home 目录下的配置文件。SkillStar 不写任何项目级 MCP 配置，因此「密钥进版本控制」这一暴露面不存在，且这个结论由 `McpSecretPolicy::writes_project_scoped_config` 从真实解析出的路径实时计算，而不是硬编码断言。
- **没有走系统凭证库**，尽管 `keyring` 已是 workspace 依赖。需要这个密钥的进程是 agent 工具本身（Claude Code / Codex / Cursor）在读它自己的配置文件；只存在 SkillStar 钥匙串里的密钥等于 MCP server 永远收不到的密钥，条目会装上然后静默起不来。这是能力限制，不是疏忽。

## 健康检查（双纪元）

`2026-07-28` 修订删除了 `initialize` 握手，因此「这个 server 健康吗」必须先在**没有握手可问**的情况下判断对方实现的是哪一版协议：

- modern：`server/discover` 成功即 modern；收到 MCP 保留错误码段内的错误仍是 modern（换版本重试一次）；其他错误或超时才回落 legacy。
- legacy：`initialize` → `notifications/initialized`。
- 两条路都收敛到 `tools/list`，它是真正的存活证明，也是 `ttlMs` / `cacheScope` / schema 体积的来源。`schemaBytes` 是 `tools` 数组紧凑 JSON 的 UTF-8 字节数，`schemaTokens` 是 `ceil(schemaBytes / 4)`；两者只在探测真正拿到 listing 时出现。这是上下文成本的粗指标，不是 tiktoken，也不是 30 天调用量。
- 纪元结论按**进程（stdio）/ origin（HTTP）** 缓存；探测失败即驱逐，被升级或临时故障的 server 会重新判定而不是钉死在旧结论上。
- 前端把 `modern` 读成「无状态 · 2026-07-28」，把 `legacy` 读成「会话握手 · ≤2025-11-25」；内部枚举值不进界面文案。`ttlMs` 有值就显示。
- **`401 + WWW-Authenticate` 不是失败**，它是 server 正确地要求授权，有独立状态 `authorization-required`，前端应据此发起 OAuth 而不是画红叉。
- stdio launcher 不在 `PATH` 上也有独立状态 `runtime-missing`：「装 Node」和「这个 server 坏了」是两条完全不同的指令。
- `McpProbeStatus` 是 kebab-case 上线的（`#[serde(rename_all = "kebab-case")]`），Rust 变体名是驼峰。前端一律以 `src/types/generated/McpProbeStatus.ts` 的字面量为准，dev mock 也必须——写成驼峰不会报错，只会让这个状态在浏览器 dev 模式下静默渲染不出来。

## Store 与工具同步

- Rust 侧 MCP 工具事实（label、配置路径、安装探测、wire-format 的计数/读取/写入/移除 dispatch）的 SSOT 是 `skillstar_models::mcp` 的 `McpToolSpec` 注册表；新增工具只加一行 spec（新 wire format 才需要新的 spec builder）。隐藏的 legacy cleanup id 刻意不进注册表。
- `MCP_TOOL_IDS` 在前端有三份人工同步的镜像：`src/types/mcp.ts`、`src/features/mcp/lib/toolRegistry.ts` 的 `MCP_TOOL_LABELS`（原先在 `McpServerForm`）、`src/features/mcp/lib/agentTargets.ts` 的 `MCP_TOOL_BY_AGENT_ID`。Rust 侧的常量是 SSOT，四份必须同一次变更内落地；前两份由 `toolRegistry.test.ts` 钉住，第三份由 `agentTargets.test.ts` 钉住。
- `MCP_TOOL_BY_AGENT_ID` 有几条需要解释的行：`github-copilot -> vscode`（`vscode` 目标写的就是 `~/.copilot/mcp-config.json`，与该 profile 同一配置根）；`gemini-cli -> gemini-cli` 与 `antigravity -> antigravity` 两侧各自同名，但配对不是自动的——两者都落在 `~/.gemini` 下，产品不同、写入的文件也不同（`settings.json` vs `config/mcp_config.json`），互不顶替。`hermes -> hermes` 写入 YAML（`$HERMES_HOME/config.yaml`），不是 JSON。
- **不是每个 MCP target 都该有 Agent profile。** `claude-desktop-chat` 没有映射行是决定而非遗漏：Claude Desktop 是聊天 App，没有可验证的 skills 目录，为了换一个 MCP 开关而在 Skills 注册表里编一个 skills 根目录是本末倒置。没有 profile 只意味着拿不到 Agent rail 上的 per-server 开关；目标本身照样可写——新建/编辑表单和工具视图直接枚举 `MCP_TOOL_IDS`，不走这张映射表，`mcpToolIdsWithoutAgentProfile` 会把它如实列为「无 profile 可达」。
- MCP store 与 Marketplace snapshot 是不同数据源：市场只负责发现，安装后进入 Models MCP store。
- create/update/delete/rename 通过统一 store facade 编排各 Agent projector；部分失败要返回每个目标结果，不静默吞掉。
- live config 路径使用与 Models tool-sync 相同的 `SKILLSTAR_TOOL_SYNC_HOME` resolver，测试不写真实 home。
- 所有 live config 与 store 的读取都 fail closed：文件不存在（或为空）才视为空配置并继续写；存在但读不出、解析失败或目标键类型不对，一律返回错误且原文件一字不动。这些文件承载 SkillStar 之外的用户配置，而写入是整文件替换，宽松解析等于静默清空。
- `mcp_servers.json` 解析失败时把原文件另存为 `mcp_servers.json.corrupt.<epoch_ms>`（同内容只留一份）后报错；写入前对已存在的 store 做一次 rolling backup，与工具配置写入使用同一个 `create_rolling_backup`。
- MCP 可操作 target 遵循 [Skills 的本机 Agent 可见性规则](../skills/README.md#agent-注册手动启用与项目检测)，只与 MCP 支持映射取交集；不再用实际 tool probe 隐藏用户已手动启用的 Agent。同步时若目标配置不可写，按目标返回明确失败。

## 墓碑与它的公开后继：distinct id + subsumption

一个曾被下架、后来又被重新纳入的 target，**墓碑 id 与公开 id 必须不同**。两者语义不同：墓碑授权的是「恰好一次移除」，公开 target 是「常驻 enable 开关」。共用一个 id 会让旧 store 里遗留的一个 `true` 反复删掉活目标刚写进去的键。目前有两对，都写同一个文件：

| 墓碑 id | 公开 id | 共同文件 |
| --- | --- | --- |
| `gemini` | `gemini-cli` | `~/.gemini/settings.json` |
| `claude-desktop` | `claude-desktop-chat` | OS config dir 下的 `Claude/claude_desktop_config.json` |

- **subsumption 规则**：一个 entry 同时带墓碑和公开 target 时，墓碑被**消费但不碰文件**（`success` + `skipped`）。因为 `sync_server_all_tools` 先跑公开 pass，直接删会抹掉几毫秒前刚写入的键；而墓碑要防的事（不留下无人管理的旧条目）已经由接管该键的活目标做到了。实现见 `sync::cleanup_legacy_desktop_chat` / `cleanup_legacy_gemini` 与共用的 `subsumed_tombstone`。
- 墓碑单独存在时行为不变：删除 SkillStar 管理的 named server，保留其他 JSON 字段；malformed JSON fail closed，原文件不动。
- rename/delete 在 cleanup 失败时不提交新 store 状态，以便下次重试。
- 墓碑 id 永不进 `McpToolSpec` 注册表，也永不进 `MCP_TOOL_IDS`；这条不变量由 `registry::tests` 钉住，同时钉住「公开后继必须在注册表里、且与墓碑解析到同一个文件」。

## Claude 兼容边界

Claude 有**两个表面，两个 target**，因为它们读不同的文件、用不同的 wire format：

- `claude-code` → `~/.claude.json`，同时服务 CLI 与 Desktop Code；社区 JSON，**有 `url` 必须写 `type`**，否则 Claude Code 直接报配置错误并跳过该 server。
- `claude-desktop-chat` → macOS `~/Library/Application Support/Claude/claude_desktop_config.json`、Windows `%APPDATA%\Claude\claude_desktop_config.json`（Linux 落在 `~/.config/Claude/`）；顶层键同为 `mcpServers`，但**没有 `type` 字段**，以 `command` 为主、`url` 表示远端。

两者规则相反，因此**不共用 spec builder**：`claude_desktop_chat_spec` 走 `JsonDialect::PlainNoType`（与 Zed 同 dialect），`claude_code_spec` 走 `Typed`。把 Code 的 `type` 写进 Chat 的文件，就是 P0-3 那个 bug 的镜像版本。

- 配置路径经 `sync_config_dir()` 解析（`~/Library/Application Support` / `%APPDATA%` / `~/.config`），它与 `sync_home_dir()` 共用同一个 `SKILLSTAR_TOOL_SYNC_HOME` sandbox 判断，测试不落到真实 home。
- `claude-desktop-chat` 关闭时会像其他任何 target 一样，把同名 server 从 Chat 配置里移除——这是「公开 target」的完整语义，与只读不写的墓碑时代不同。

## Marketplace 接缝

- curated rows 与远端 catalog snapshot 合并，远端刷新不得覆盖或删除 curated rows。
- Publisher 顺序、source id 和 server 清单以 `skillstar-marketplace` seed/query 代码为准，不在文档复制枚举。
- curated 行与 registry 行列对称，从而复用同一条查询和同一条安装路径。

## 指挥中心（Fleet | Catalog）

MCP 页面是该域的唯一入口，但不再把四个表面画成同等权重的 tab。主分段是 **机群（Fleet）** 与 **目录（Catalog）**；**工具** 与 **目录源** 是次级，因为它们回答的是「投影写到了哪」和「目录从哪来」，不是每天的安装/运行工作台。这是吸收 Hermes Agent v0.21 指挥中心的产品形状，同时守住 SkillStar 自己的约束（见 [D-052](../../decisions.md#d-052mcp-指挥中心是-skillstar-原生形态只吸收-hermes-021-的平台能力)）：

- catalog 是两万量级，**禁止**把已装列表和全量目录堆在同一条滚动里。浏览目录仍然走分页查询，不得拉全量进渲染进程。
- 机群页顶部是「粘贴即解析」条：用户可以丢进社区 `mcpServers` JSON、Streamable HTTP URL、`npx`/`uvx`/`docker` 命令行，或 `skillstar://mcp` 深链。解析在 `skillstar_models::mcp::parse_pasted_mcp`，命令适配器只是把它露出来。**解析结果不是安装。** 目录命中打开现有 `McpInstallWizard`；其余命中预填现有新建表单。两条路都要用户确认后才写 store。
- 机群在指挥中心**首次挂载**时对已装列表做一次顺序健康探测，上限 8，不在 window focus / 缓存过期时重跑。超过上限的 server 仍可在编辑抽屉里按需探测。`401 + WWW-Authenticate` 继续是 `authorization-required`，机群条把它显示成「需要登录」而不是红叉。
- `skillstar://mcp?url=` / `?catalog=` / `?config=` / `?command=` 会唤醒应用并打开对应的确认 UI。深链不得绕过安装确认。后端本来就把 `query` 放进 `skillstar://deep-link` 事件；前端必须读它，而不能只按 host 跳到 MCP 页。
- 「从工具导入」仍然是读各 Agent 活配置的那条路（`import_from_tool`），与粘贴解析互补：前者有磁盘上的权威文件，后者接受用户随手丢来的片段。

## 前端职责

- preset 芯片区的数据是「curated `recommended` 行」与「内置 preset 目录」的合并去重（按 id 和大小写不敏感的 name，curated 在前），不是二选一；snapshot DB 缺失或损坏时仍要保底返回内置目录。这整段编排（初始化快照 → 列 curated → 过滤 recommended → 映射 → 合并去重）在 `skillstar_app::mcp::presets`，命令层只是适配器；快照 runtime 的装配（db 路径、data root、已装技能 loader）仍属宿主胶水（GUI 在 `src-tauri/src/core/marketplace_snapshot`，CLI 在 `skillstar_app::cli`）。
- preset 芯片有两条安装路径，按 `McpPreset.catalogId` 这个显式标记分流，不靠「先试着解析目录行、解析不到再回退」：带 `catalogId` 的 curated 芯片打开安装向导（`McpInstallWizard`，与市场 tab 同一个入口，因而同样有运行时形态选择、密钥掩码密码框、必填标注和完整命令确认）；不带的内置 preset 没有目录行，继续预填新建表单。curated preset 的 id 本来就是目录行 id，所以芯片可以直接把它交给向导。
- MCP 页面是 MCP 域的唯一入口。主分段是 **机群（Fleet）** 与 **目录（Catalog）**，次级是 **工具** 与 **目录源**；四者仍共用同一批 hook。机群页（`McpManager`）承载已装卡片、粘贴解析条、机群健康条和新建/安装抽屉；目录页仍是 `McpMarketPage` 的全目录分页浏览。市场、工具、目录源三项此前完全没有 UI。
- Marketplace MCP tab 保留 Publisher grid 入口；`McpPublisherDetail` 现在只是一层 hero，主体复用同一个 `McpMarketPage`，只是带上 `publisherId`。发布者页不再自己拉全量再内存过滤。
- Agent rail 复用 `AgentTargetCarousel`，显示名和图标来自 Settings profile，而不是 MCP 自己维护 SVG registry。Settings 关掉但这条 server 仍写入的 target 留在轮播里，SVG 进停用态（灰度、不可点），让启停可被卡片感知；工具栏筛选仍只列当前启用的 profile。新建/安装表单的默认勾选跟随当前启用 profile，完整 `MCP_TOOL_IDS` 仍可手选。
- 商店浏览必须走分页查询命令并展示 `total`；不得再拉全量后在内存里过滤。筛选、排序、分页全部编译进一次 `query_mcp_market_servers_local`，渲染进程不做二次过滤。
- 弃用条目默认不出现在浏览结果里（前端默认 `statuses: ["active"]`），需要显式打开开关才列出，且始终带弃用标记。后端默认「列出但警告」不变——这是 UI 的取舍，不是契约变化。
- 市场卡片展示名称、描述、安装/更新动作、弃用/被取代例外和 stars；kind 用图标表达，推荐用标题旁的标记。runtime、版本、仓库和详情留在抽屉与安装向导，不在卡片页脚重复。
- 已安装卡片用运输图标区分 stdio / http，不重复运输文字徽标；描述与命令行/URL 相同时不画第二行。YOLO 与待更新仍作为例外保留。
- 三态标记（已安装 / 有更新 / 已弃用）以来源指纹判定：`McpServerEntry.registryName` 对 `McpMarketEntry.namespace`。按 `name` 的字符串比对只作为老条目（无指纹）的兜底，且**永远不判定"有更新"**——那份版本号不一定来自这一行。
- 安装向导必须展示完整命令预览与运行时候选；secret 字段必须掩码，且不得回显进日志。用户填写的参数会改变命令行，因此确认步骤显示的是 `mcp_market_install_preview` 按最终值渲染出的那一条——前端不持有任何参数拼装、模板替换或命令行渲染逻辑，只做掩码与即时的必填/格式/可选值校验（纯函数，即时反馈不该走一趟 IPC；后端的校验才是权威）。预览按 300ms 去抖调用，在途期间提交按钮禁用，避免批准一条过期的命令。向导提交的是答案 + 已确认的那条字符串，不是自己拼出来的 entry；提交时传的运行时形态是安装计划**选定**的那个（`selectedRuntimeId`），不是选择器的临时状态——`null` 会让后端回落到排序推荐，可能是另一种形态。本地（stdio）安装必须勾选确认框才能提交；远端形态零本地执行，不要求这次勾选。字段级校验**不禁用提交按钮**：禁用只会说「不行」而不说哪一项不行。带着错误提交会就地标出出错的字段并且什么都不发出去。
- 同步结果必须逐 target 展示成功/跳过/失败/已回滚/回滚失败，并给出错误原文、配置路径与备份路径；失败项可单条重试（`set_mcp_tool_enabled`）或整条重投（`sync_mcp_server`，`force`）。批次一致性（applied / rolledBack / drifted）由前端按同一语义从结果数组重算。
- `autoApprove` / `disabledTools` / `timeout` 只有部分 target 会写入，表单必须按**当前选中的 target**说明谁会写、谁会忽略。这张表在 `lib/toolRegistry.ts`，SSOT 仍是 `specs.rs` 的各个 writer，两边必须同一次变更内落地。
- 工具未检测提示（target 选择器里那句 `notDetectedSuffix`）由 `useMcpToolStatuses` 返回的 `noteForTool` 派生；已安装页与市场页都用它，不各写一份。
- `KEY=VALUE` 解析在 `lib/kv.ts`：只 trim key，值默认 trim，但**加引号即逐字保留**。旧实现无条件 trim 值，会静默改写带首尾空格的密钥。

## 验证

```bash
cargo test -p skillstar-app -p skillstar-models -p skillstar-marketplace mcp
cargo test -p skillstar-models -p skillstar-marketplace -p skillstar-app -p skillstar export_bindings
bun run test -- src/features/mcp
bun run types:gen
```

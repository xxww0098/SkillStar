# MCP 现代设计理念调研（2026-08）

状态：active（调研快照，不是 SSOT）
类型：一次性外部技术调研。本文只记录**外部生态事实**与由此推导的建议，不定义 SkillStar 的架构、边界或功能行为。
落点约束：本文出现的任何结论若要变成 SkillStar 的实现约定，必须先写进 `docs/boundaries.md` / `docs/architecture.md` / `docs/features/mcp/README.md` / `docs/decisions.md`，再由本文链接过去。

抓取日期：**2026-08-13**（对应 UTC 2026-08-12，所有 `curl` 响应 `Date` 头均为 `Wed, 12 Aug 2026`）。
调研方式：直接抓取 modelcontextprotocol.io 规范页、GitHub 官方仓库、以及各 registry 的**真实 HTTP 响应**；未使用记忆或二手总结作为结论依据。

---

## 0. 执行摘要（先看这 8 条）

1. **MCP 当前规范版本是 `2026-07-28`**，这是一次**破坏性重构**：协议变成**无状态**——删除 `initialize`/`notifications/initialized` 握手、删除 `Mcp-Session-Id`、删除 SSE 断线续传。每个请求自带协议版本与能力，放在 `_meta` 里。
2. **传输只剩两种**：`stdio` 与 `Streamable HTTP`。旧 `HTTP+SSE`（2024-11-05）已正式进入 Deprecated 生命周期状态。Streamable HTTP 本身也变了：**没有 GET 流、没有 `Last-Event-ID`、没有 session**。
3. **服务端不再能主动发起 JSON-RPC 请求**。sampling / elicitation / roots 全部改成 **MRTR（Multi Round-Trip Requests）**：服务端返回 `resultType: "input_required"`，客户端补齐信息后**用新的 request id 重发原请求**。
4. **Roots / Sampling / Logging 三个特性被废弃**（SEP-2577），最早移除时间是 2027-07-28 之后的首个版本。**Dynamic Client Registration（RFC7591）也被废弃**，取而代之的是 **CIMD（Client ID Metadata Documents）**——把 HTTPS URL 当 `client_id`。
5. **官方 Registry 的现行 API 路径是 `/v0.1`**，不是 `/v0`。其数据以 **CC0 1.0** 公有领域奉献，明确允许聚合器抓取（建议每小时一次），但**不提供可用性与持久性保证**。
6. **SkillStar 目前打的 `https://api.mcp.github.com/v0/servers` 已经返回 `Deprecation: true` 响应头**（实测），GitHub 侧现行路径是 `/v0.1`。这是本次调研发现的唯一"已经在流血"的问题。
7. **第三方目录里只有 Smithery / Glama / Docker Catalog 值得接**；PulseMCP 已开始**按比例随机拒绝** v0beta 请求且 v0.1 强制要 API key；mcp.so 的 `robots.txt` 明确 `Disallow: /api/`，无公开 API。
8. **客户端配置格式没有收敛**，且分歧点恰好在最容易踩坑的地方：远端 URL 的键名在 `url` / `serverUrl` / `httpUrl` 三者间摇摆，传输类型在 `http` / `streamable-http` / `streamableHttp` / 无字段 之间摇摆。

---

## 1. MCP 规范当前版本与近一年演进

### 1.1 版本时间线与"纪元"划分

| 版本 | 定位 | 说明 |
| --- | --- | --- |
| `2024-11-05` | legacy | 初版，HTTP+SSE 传输 |
| `2025-03-26` | legacy | 引入 Streamable HTTP，HTTP+SSE 开始弃用 |
| `2025-06-18` | legacy | 引入 `MCP-Protocol-Version` 头、结构化输出、resource links |
| `2025-11-25` | legacy | 引入 URL mode elicitation、实验性 tasks |
| **`2026-07-28`** | **modern（当前）** | 无状态重构 |

规范正式引入了纪元术语：**Modern** = `2026-07-28` 及以后（按请求携带元数据）；**Legacy** = `2025-11-25` 及以前（`initialize` 握手建立会话）；**Dual-era** = 同时支持两者。
来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning>（抓取 2026-08-13）

Schema 权威来源为 TypeScript 定义：`https://github.com/modelcontextprotocol/specification/blob/main/schema/2026-07-28/schema.ts`
来源：<https://modelcontextprotocol.io/specification/latest>（抓取 2026-08-13）

### 1.2 无状态化（SEP-2567 / SEP-2575）——本次最大变化

删除 `initialize` / `notifications/initialized`；删除 `Mcp-Session-Id`；`tools/list` 等列表结果**不得再随连接变化**（但**可以随请求携带的授权变化**，因为凭证是每请求输入而非连接状态）。

每个请求 `params._meta` 必须携带：

```json
{
  "_meta": {
    "io.modelcontextprotocol/protocolVersion": "2026-07-28",
    "io.modelcontextprotocol/clientInfo": { "name": "ExampleClient", "version": "1.0.0" },
    "io.modelcontextprotocol/clientCapabilities": {}
  }
}
```

- `io.modelcontextprotocol/protocolVersion`：**必需**。HTTP 上必须与 `MCP-Protocol-Version` 头一致，否则服务端必须返回 `400` + `HeaderMismatch`。
- `io.modelcontextprotocol/clientCapabilities`：**必需**。服务端**不得**从之前的请求推断能力。
- `io.modelcontextprotocol/clientInfo`：SHOULD，自报且不被协议验证，**不得用于安全决策**。
- `io.modelcontextprotocol/logLevel`：可选，**已废弃**；不设置则服务端不得发 `notifications/message`。

服务端需要跨调用状态时，必须**显式铸造 handle**（如购物车 ID）并作为**普通工具参数**回传；协议层没有 handle 概念。
来源：<https://modelcontextprotocol.io/specification/2026-07-28/changelog>、<https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/main/schema/2026-07-28/schema.ts>、<https://modelcontextprotocol.io/specification/2026-07-28/server/tools>（抓取 2026-08-13）

### 1.3 `server/discover`（新增，服务端 MUST 实现）

```ts
export interface DiscoverResult extends CacheableResult {
  supportedVersions: string[];
  capabilities: ServerCapabilities;
  instructions?: string;
}
```

客户端 MAY 先调用它做前置版本选择；在 stdio 上它同时是**判断服务端是 modern 还是 legacy 的探针**。
来源：schema.ts（同上）、<https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning>（抓取 2026-08-13）

### 1.4 传输：stdio

现状：**依然是一等公民，且几乎没变**。换行分隔 JSON-RPC；`stdout` 只能写 MCP 消息；`stderr` 任意日志且**不得视作错误信号**；关闭 stdin 是唯一可移植的优雅关停信号；进程异常退出后客户端 SHOULD 重启——因为协议无状态，在途请求直接丢弃重试即可。取消仍用 `notifications/cancelled`（stdio 独有，HTTP 上不用）。

规范额外说明：该 framing 可原样复用到 Unix domain socket / TCP，自定义传输 SHOULD 复用它而不是另发明一套。
来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio>（抓取 2026-08-13）

### 1.5 传输：Streamable HTTP（2026-07-28 形态）

单一 MCP endpoint，仅支持 POST：

- 客户端 `Accept` **必须**同时列出 `application/json` 与 `text/event-stream`。
- 请求体是**单个** JSON-RPC request 或 notification；客户端**不得**发送 response。
- notification → `202 Accepted` 空体。
- request → 服务端选择返回单个 JSON 对象，或**该请求作用域内**的 SSE 流（progress/message 通知 + 最终响应）。
- **取消 = 关闭该请求的 SSE 响应流**（不发 `notifications/cancelled`）。
- SSE 流 SHOULD 带 `X-Accel-Buffering: no`；长连接 SHOULD 周期性发 `:` 注释行保活。
- **`Last-Event-ID` 续传已删除**：流断了就丢，客户端必须**用新 request id 重发**。
- 服务端 MUST 校验 `Origin`（非法则 `403`），本地运行 SHOULD 只绑 `127.0.0.1`。

必需请求头（合规强制）：

| 头 | 来源字段 | 适用 |
| --- | --- | --- |
| `MCP-Protocol-Version` | `_meta.io.modelcontextprotocol/protocolVersion` | 全部请求 |
| `Mcp-Method` | `method` | 全部请求 |
| `Mcp-Name` | `params.name` 或 `params.uri` | `tools/call`、`resources/read`、`prompts/get` |

可选：`Mcp-Param-{Name}`，由工具 `inputSchema` 里属性上的 `x-mcp-header` 扩展声明（仅 string/integer/boolean，禁止 `number`，必须静态可达）。**服务端使用是可选的，客户端支持是 MUST。** 非 ASCII 值用 `=?base64?<b64>?=` 哨兵编码。

设计意图很明确：**让网关/WAF/负载均衡不解析 body 就能路由和限流**。同时规范要求服务端**必须校验头与 body 一致**（不一致 → `400` + `-32020`），防止"LB 按头路由、server 按 body 执行"的分裂。

示例：

```http
POST /mcp HTTP/1.1
Content-Type: application/json
MCP-Protocol-Version: 2026-07-28
Mcp-Method: tools/call
Mcp-Name: execute_sql
Mcp-Param-Region: us-west1

{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
  "name":"execute_sql",
  "arguments":{"region":"us-west1","query":"SELECT * FROM users"},
  "_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28",
           "io.modelcontextprotocol/clientCapabilities":{}}}}
```

来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http>（抓取 2026-08-13）

### 1.6 会话管理：已删除，向后兼容如何做

对旧客户端流量，只支持新版的服务端 SHOULD：GET/DELETE → `405`；`Mcp-Session-Id` → 忽略且不回发；`Last-Event-ID` → 忽略。

**双纪元探测算法**（客户端）：

- **stdio**：先发 `server/discover`。返回 `DiscoverResult` → modern；返回**可识别的** modern 错误（如 `UnsupportedProtocolVersionError`）→ modern 但版本不匹配，**不要**回落；其他错误或超时 → legacy，回落 `initialize`。
- **HTTP**：先发 modern 请求。遇 `400` **先看 body**：是可识别 modern JSON-RPC 错误 → modern，按 `supported` 重试；body 为空或不可识别 → 回落 `initialize`，再不行才回落 HTTP+SSE（`GET` 期待 `endpoint` 事件）。

纪元判断是**服务端属性**，客户端 SHOULD 按进程（stdio）或 origin（HTTP）缓存，MAY 跨重启持久化。
来源：同上 + <https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning>（抓取 2026-08-13）

### 1.7 取消 / 进度 / 日志

| 能力 | 2026-07-28 现状 |
| --- | --- |
| 取消 | stdio：`notifications/cancelled`；HTTP：关闭响应流。服务端 SHOULD 尽快停工，MUST NOT 再发该请求的任何消息 |
| 进度 | `notifications/progress`，靠 `_meta.progressToken` 触发；**只在其所属请求的响应流上**，不走 listen 流 |
| 日志 | `notifications/message`，级别由 `_meta["io.modelcontextprotocol/logLevel"]` **按请求**指定；`logging/setLevel` **已删除**；整个 Logging 特性**已废弃**，建议改用 stderr（stdio）或 OpenTelemetry |
| ping | **已删除** |

来源：<https://modelcontextprotocol.io/specification/2026-07-28/changelog>（抓取 2026-08-13）

### 1.8 `subscriptions/listen`（替代 GET 流与 resources/subscribe）

客户端 POST 一个 `subscriptions/listen` 请求，其**响应流长期保持打开**，只投递客户端显式勾选的通知类型：`toolsListChanged`、`promptsListChanged`、`resourcesListChanged`、`resourceSubscriptions`。服务端先回 `notifications/subscriptions/acknowledged`，随后每条通知用 `_meta["io.modelcontextprotocol/subscriptionId"]` 标记。stdio 上同一条 stdout 通道，靠该 id 关联。
来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http>、`.../transports/stdio`（抓取 2026-08-13）

### 1.9 MRTR（SEP-2322）——elicitation / sampling / roots 的新载体

所有结果新增**必填** `resultType` 字段：`"complete"` 或 `"input_required"`（Tasks 扩展再加 `"task"`）。旧版服务端省略该字段时，客户端 **MUST** 当作 `"complete"`。

```json
{"jsonrpc":"2.0","id":1,"result":{
  "resultType":"input_required",
  "inputRequests":{
    "github_login":{"method":"elicitation/create","params":{
      "mode":"form","message":"Please provide your GitHub username",
      "requestedSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}},
    "capital_of_france":{"method":"sampling/createMessage","params":{
      "messages":[{"role":"user","content":{"type":"text","text":"What is the capital of France?"}}],
      "maxTokens":100}}},
  "requestState":"AEAD-protected blob"}}
```

客户端补齐后**用新的 JSON-RPC id** 重发原请求：

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
  "name":"get_weather","arguments":{"location":"New York"},
  "inputResponses":{"github_login":{"action":"accept","content":{"name":"octocat"}}},
  "requestState":"AEAD-protected blob"}}
```

关键规则：
- 仅 `prompts/get`、`resources/read`、`tools/call` 三个请求允许返回 `InputRequiredResult`。
- `requestState` 对客户端**完全不透明**，客户端 MUST NOT 解析/修改，MUST 原样回传。
- 服务端 MUST 把 `requestState` 视为**攻击者可控输入**；若它影响授权/资源访问/业务逻辑，MUST 做完整性保护（HMAC 或 AEAD）并拒绝校验失败的状态。
- 防重放 SHOULD 在受保护载荷内包含：认证主体、短 TTL、原请求标识（方法名 + 关键参数摘要）。
- 服务端 MUST NOT 发送客户端未声明能力的 `inputRequests`。

来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr>（抓取 2026-08-13）

### 1.10 Elicitation

能力声明在**每个请求**的 `_meta.io.modelcontextprotocol/clientCapabilities.elicitation`，形如 `{"form":{},"url":{}}`；空对象等价于只支持 `form`。

- **form 模式**：`requestedSchema` 只允许**扁平对象 + 原始类型**（string/number/integer/boolean/enum，含 `oneOf`/`anyOf` 带 title 的枚举与多选数组）。string 支持 `format`：`email`、`uri`、`date`、`date-time`。
- **url 模式**：服务端给一个 URL，敏感信息**不经过 MCP 客户端**。
- 硬性安全线：服务端 **MUST NOT** 用 form 模式索取密码/API key/令牌/支付凭证，**MUST** 用 url 模式。
- 响应三态：`accept`（带 `content`）/ `decline` / `cancel`。url 模式的 `accept` 只表示**用户同意打开**，不表示交互完成。
- 2026-07-28 删除了 `notifications/elicitation/complete` 与 `elicitationId`——因为 MRTR 下客户端靠重试得知结果。
- 客户端 URL 处理硬约束：MUST NOT 预取；MUST NOT 未经同意打开；MUST 完整展示 URL；MUST 用**客户端与 LLM 都无法窥视内容**的方式打开（iOS 举例：`SFSafariViewController` 可以，`WKWebView` 不行）。

来源：<https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation>（抓取 2026-08-13）

### 1.11 Sampling / Roots：已废弃

`SEP-2577` 废弃 Roots、Sampling、Logging。**至少 12 个月**的废弃窗口，最早移除时间为**"2027-07-28 之后发布的首个规范版本"**。建议迁移路径：
- Roots → 用工具参数、resource URI 或服务端配置传目录/文件。
- Sampling → 直接对接 LLM 提供方 API。
- Logging → stderr / OpenTelemetry。

`includeContext` 的 `"thisServer"` / `"allServers"` 也已 Deprecated，跟随 Sampling 一起移除。
来源：<https://modelcontextprotocol.io/specification/2026-07-28/deprecated>（抓取 2026-08-13）

### 1.12 结构化工具输出（structured tool output）

- `structuredContent` 现在可以是**任意 JSON 值**（对象/数组/字符串/数字/布尔/null），不再限定对象。
- `inputSchema` / `outputSchema` 放宽为**任意 JSON Schema 2020-12 关键字**（SEP-2106），并新增 `$ref` 解析要求与组合关键字资源上界。
- 有 `outputSchema` 时：服务端 **MUST** 保证结构化结果符合它，客户端 **SHOULD** 校验。
- 向后兼容：返回结构化内容的工具 SHOULD 同时把序列化 JSON 放进一个 `TextContent`。
- 规范特别澄清：`structuredContent` 是**服务端产出的结果数据**，与 LLM 的 "structured outputs"（受 schema 约束的模型生成）无关。

```json
{"jsonrpc":"2.0","id":5,"result":{
  "resultType":"complete",
  "content":[{"type":"text","text":"{\"temperature\": 22.5, \"conditions\": \"Partly cloudy\"}"}],
  "structuredContent":{"temperature":22.5,"conditions":"Partly cloudy","humidity":65}}}
```

来源：<https://modelcontextprotocol.io/specification/2026-07-28/server/tools>（抓取 2026-08-13）

### 1.13 Resource links

工具结果里的 `resource_link` 内容块（自 2025-06-18 起，2026-07-28 保留）：

```json
{"type":"resource_link","uri":"file:///project/src/main.rs","name":"main.rs",
 "description":"Primary application entry point","mimeType":"text/x-rust"}
```

注意：**工具返回的 resource link 不保证出现在 `resources/list` 中**。
来源：同上（抓取 2026-08-13）

### 1.14 Tool annotations（readOnlyHint 等）

schema.ts 原文：

```ts
export interface ToolAnnotations {
  title?: string;
  /** If true, the tool does not modify its environment. Default: false */
  readOnlyHint?: boolean;
  /** If true, the tool may perform destructive updates... Default: true */
  destructiveHint?: boolean;
  /** If true, calling repeatedly with same args has no additional effect. Default: false */
  idempotentHint?: boolean;
  /** If true, this tool may interact with an "open world"... Default: true */
  openWorldHint?: boolean;
}
```

**默认值很重要**：不写 = `destructiveHint: true` + `openWorldHint: true`（最悲观），`readOnlyHint: false`。
规范双重警告：所有字段都只是 **hint**；客户端 **MUST** 把来自不可信服务端的 annotations 视为不可信，**绝不能**据此做工具使用决策。
来源：schema.ts（抓取 2026-08-13）、<https://modelcontextprotocol.io/specification/2026-07-28/server/tools>

### 1.15 缓存：`CacheableResult`（SEP-2549，新增且必填）

`tools/list`、`prompts/list`、`resources/list`、`resources/read`、`resources/templates/list`、`server/discover` 的结果**必须**带：

```ts
export interface CacheableResult extends Result {
  ttlMs: number;                       // 0 = 立即过期；正数 = 新鲜期毫秒
  cacheScope: "public" | "private";    // private = 不得跨授权上下文共享缓存
}
```

配套：服务端 SHOULD 让 `tools/list` **返回确定性顺序**，以提升客户端缓存与 LLM prompt cache 命中率。
来源：schema.ts、<https://modelcontextprotocol.io/specification/2026-07-28/changelog>（抓取 2026-08-13）

### 1.16 扩展机制与 Tasks

扩展通过 capabilities 的 `extensions` map 协商（键必须是带前缀的反向 DNS 标识）：

```json
{"capabilities":{"tools":{},"extensions":{"io.modelcontextprotocol/tasks":{}}}}
```

官方扩展：`io.modelcontextprotocol/tasks`（异步任务）、`io.modelcontextprotocol/ui`（MCP Apps，内联 HTML 界面）、`io.modelcontextprotocol/oauth-client-credentials`、`io.modelcontextprotocol/enterprise-managed-authorization`。

**Tasks 扩展**（从核心移出）：服务端返回 `CreateTaskResult`（`resultType: "task"`，含 `taskId` / 初始状态 / `ttlMs` / `pollIntervalMs`），客户端用 `tasks/get` 轮询；状态机 `working` → `input_required` / `completed` / `failed` / `cancelled`；中途需要输入时用 `tasks/update` 提交 `inputResponses`；`tasks/cancel` 是协作式的。原 `tasks/result`（阻塞）与 `tasks/list` 已删除。
来源：<https://modelcontextprotocol.io/extensions/tasks/overview>、<https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning>（抓取 2026-08-13）

### 1.17 错误码分配策略（新）

`-32000`~`-32019` 保留给实现自定义（现有 SDK 用法既往不咎）；`-32020`~`-32099` 保留给 MCP 规范。已分配：

| 码 | 名称 |
| --- | --- |
| `-32020` | `HeaderMismatch` |
| `-32021` | `MissingRequiredClientCapability` |
| `-32022` | `UnsupportedProtocolVersion` |
| `-32601` | 方法不存在（HTTP 上配 `404`） |
| `-32602` | Invalid Params；**资源不存在从 `-32002` 改为 `-32602`** |

来源：<https://modelcontextprotocol.io/specification/2026-07-28/changelog>（抓取 2026-08-13）

---

## 2. 授权与安全

### 2.1 规范依据清单

授权是 **OPTIONAL**；HTTP 传输 SHOULD 遵循本规范；**stdio 传输 SHOULD NOT 遵循，而是从环境变量取凭证**。

引用标准（原文列举）：OAuth 2.1（`draft-ietf-oauth-v2-1-13`）、RFC6750（Bearer）、RFC8414（AS Metadata）、RFC7591（DCR，已弃）、RFC8707（Resource Indicators）、RFC9728（Protected Resource Metadata）、RFC9207（AS Issuer Identification）、`draft-ietf-oauth-client-id-metadata-document-00`（CIMD）、OpenID Connect Discovery 1.0 与 Dynamic Client Registration 1.0。
来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization>（抓取 2026-08-13）

### 2.2 硬性要求（MUST 级）

1. MCP 服务端 **MUST** 实现 RFC9728 Protected Resource Metadata；客户端 **MUST** 用它做授权服务器发现。
2. 授权服务器 **MUST** 至少提供 RFC8414 或 OIDC Discovery 之一；客户端 **MUST** 两者都支持。
3. 客户端 **MUST** 实现 RFC8707 `resource` 参数，**授权请求与令牌请求都要带**，值为 MCP 服务端的规范 URI（无 fragment，SHOULD 无尾斜杠，SHOULD 尽量具体）。即便 AS 不支持也 MUST 发送。
4. 客户端 **MUST** 在把授权码送去任何 token endpoint 之前，按 RFC9207 §2.4 校验 `iss`。
5. 服务端 **MUST** 校验令牌的 audience 是自己（RFC8707 §2）；**MUST NOT** 接受或转发任何非发给自己的令牌。
6. 令牌 **MUST** 走 `Authorization: Bearer`，**MUST NOT** 放 URI query。

`iss` 校验判定表（原文）：

| `authorization_response_iss_parameter_supported` | 响应中有 `iss` | 客户端动作 |
| --- | --- | --- |
| `true` | 有 | 按 RFC3986 §6.2.1 简单字符串比较 |
| `true` | 无 | **拒绝响应** |
| `false` / 缺失 | 有 | 仍然比较 |
| `false` / 缺失 | 无 | 放行 |

且解码 `iss` 后 **MUST NOT** 做 scheme/host 大小写折叠、默认端口省略、尾斜杠或百分号编码归一化。错误响应同样适用——不匹配时 MUST NOT 展示 `error_description`。

### 2.3 客户端注册：CIMD 取代 DCR

选择优先级（原文）：
1. 已有预注册凭证 → 用它；
2. AS metadata 中 `client_id_metadata_document_supported: true` → **用 CIMD**；
3. AS metadata 中有 `registration_endpoint` → 回落 DCR；
4. 都没有 → 提示用户手工填。

CIMD 要求：`client_id` 是 **HTTPS URL 且必须含 path**（如 `https://example.com/client.json`）；文档中的 `client_id` 必须与 URL **完全一致**；至少含 `client_id`、`client_name`、`redirect_uris`。

```json
{
  "client_id": "https://app.example.com/oauth/client-metadata.json",
  "client_name": "Example MCP Client",
  "client_uri": "https://app.example.com",
  "logo_uri": "https://app.example.com/logo.png",
  "redirect_uris": ["http://127.0.0.1:3000/callback", "http://localhost:3000/callback"],
  "grant_types": ["authorization_code"],
  "response_types": ["code"],
  "token_endpoint_auth_method": "none"
}
```

**CIMD 的关键优势**：client_id 是自托管 HTTPS URL，**跨授权服务器可移植**，换 AS 不需要重新注册。

对比之下，DCR 与预注册凭证 **MUST** 按 AS 的 `issuer` 标识键存储，**MUST NOT** 跨 AS 复用，AS 变更时 **MUST** 重新注册。DCR 时客户端 **MUST** 指定 `application_type`：桌面/移动/CLI/localhost → `"native"`；远程浏览器应用 → `"web"`（省略时 OIDC 默认 `"web"`，会与 native 风格 redirect URI 冲突）。
来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration>（抓取 2026-08-13）

### 2.4 Scope 与 token 存储

- 服务端 SHOULD 在 `WWW-Authenticate` 里带 `scope`，客户端 **MUST** 把挑战里的 scope 当作当前操作的权威值。
- 客户端 scope 选择优先级：401 挑战里的 `scope` > PRM 的 `scopes_supported` > 省略。
- 权限不足运行时错误：`403 Forbidden` + `error="insufficient_scope"` + `scope="..."` + `resource_metadata`。
- **Scope 累积是客户端责任**：重新授权时 SHOULD 取"历史已请求 scope ∪ 本次挑战 scope"的并集，否则会丢失已授权限。
- 刷新令牌：客户端 **MUST** 保证传输与存储机密性；SHOULD 在 `grant_types` 里带 `refresh_token`；MAY 加 `offline_access`（仅当 AS 的 `scopes_supported` 里有）。**服务端 SHOULD NOT** 把 `offline_access` 放进 `WWW-Authenticate` 或 PRM 的 `scopes_supported`。
- 反模式清单：`scopes_supported` 里堆全部 scope、使用 `*` / `all` / `full-access` 这类通配 scope、每次挑战返回整个目录、把令牌里声称的 scope 当作充分授权而不做服务端授权逻辑。

来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization>、<https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices>（抓取 2026-08-13）

### 2.5 官方安全最佳实践清单（逐项）

| 攻击 | 核心结论 |
| --- | --- |
| **Confused Deputy** | MCP 代理服务端用静态 client_id + 允许下游动态注册 + 三方 AS 有同意 cookie ⇒ 可跳过用户同意窃取授权码。缓解：**MUST** 实现 per-client 同意登记表并在转发到三方前检查；同意页 MUST 显示客户端名/scope/redirect_uri，MUST 有 CSRF 防护与 `frame-ancestors` 防点击劫持；同意 cookie MUST 用 `__Host-` 前缀 + `Secure`/`HttpOnly`/`SameSite=Lax` 且绑定到具体 `client_id`；`redirect_uri` MUST 精确字符串匹配；`state` MUST 单次使用、短过期，且**同意通过之后**才写入 |
| **Token Passthrough** | 服务端 **MUST NOT** 接受任何非发给自己的令牌。危害：绕过限流/审计、审计链断裂、信任边界外溢 |
| **SSRF** | OAuth 发现链（`resource_metadata` URL、`authorization_servers`、`token_endpoint`）全部来自可能恶意的服务端。缓解：生产强制 HTTPS（loopback 例外）；屏蔽 `10/8`、`172.16/12`、`192.168/16`、`127/8`、`169.254/16`（云 metadata）、`fc00::/7`、`fe80::/10`；对重定向目标同样校验；用 egress proxy；注意 DNS TOCTOU。规范明确提示**不要手写 IP 校验**（八进制/十六进制/IPv4-mapped IPv6 绕过）。同样适用于**授权服务器抓取 CIMD 文档**时 |
| **State Handle Hijacking**（新，替代旧 Session Hijacking） | 无状态化后服务端自铸 handle。服务端 **MUST** 校验每个入站请求，**MUST NOT** 把持有 handle 当作认证；SHOULD 用 CSPRNG 生成并按 `<user_id>:<handle>` 绑定到已验证用户 |
| **Local MCP Server Compromise** | 支持一键本地安装的客户端 **MUST** 在执行前展示**未截断的完整命令**、明确标注为危险操作、要求显式批准、允许取消。SHOULD 高亮 `sudo` / `rm -rf` / 访问 `~/.ssh` 等模式，并沙箱化运行 |
| **OAuth Authorization URL Validation** | 客户端 **MUST** 只允许 `http://`(仅 loopback)/`https://`，**MUST** 拒绝 `javascript:` `data:` `file:` `vbscript:`；**MUST NOT** 用 shell（`cmd.exe`/`sh`/PowerShell）打开 URL |
| **stdio 代理架构提权** | XSS → 窃取本地代理鉴权 token → 通过代理 spawn 任意进程 → RCE。缓解：沙箱化被 spawn 的进程、限制文件系统访问、记录全部 stdio 用法 |
| **Mix-Up** | 仅靠 PKCE **不能**防御（code_verifier 会被送到攻击者 token endpoint）；靠 RFC9207 `iss` 绑定 |
| **Localhost Redirect URI 冒充** | CIMD 只能证明域名控制权，**不能证明哪个本地进程在监听 localhost 端口**。攻击者可拿正牌 client 的 metadata URL 当 `client_id` + 自己的 localhost 端口收码。缓解在 AS 侧：对 localhost-only redirect URI 额外告警、显著展示 redirect 主机名 |
| **CIMD Trust Policies** | AS 可用域名白名单 / 开放接受任意 HTTPS / 信誉检查 / 域龄与证书校验；应显著展示 CIMD 主机名防钓鱼 |
| **Scope Minimization** | 见 §2.4 |

### 2.6 Prompt injection 的规范立场

MCP 规范层面**没有**给 prompt injection 一个独立章节，而是把它拆成三条可执行约束：

1. 工具描述与 annotations **是不可信数据**——"descriptions of tool behavior such as annotations should be considered untrusted, unless obtained from a trusted server"。
2. **人在回路是硬要求**：宿主 SHOULD 提供 UI 明示暴露了哪些工具、调用时插入明显视觉指示、对操作给出确认提示；"there SHOULD always be a human in the loop with the ability to deny tool invocations"。
3. 客户端 SHOULD 在调用前**向用户展示工具输入**（避免恶意或误操作导致的数据外泄），并在把结果交给 LLM 前**校验结果**。

另外 elicitation 侧有针对性的反钓鱼要求：URL mode 的链接可被攻击者转发给受害者，服务端 **MUST** 验证打开 URL 的用户与发起 elicitation 的用户是同一人（典型做法：比对 MCP AS 的 `sub` 与浏览器会话 cookie 的 subject）。

来源：<https://modelcontextprotocol.io/specification/latest>、`.../server/tools`、`.../client/elicitation`、`.../tutorials/security/security_best_practices`（抓取 2026-08-13）

### 2.7 生态中的真实事故（对客户端实现的警示）

Proofpoint 披露的 **CursorJack**：Cursor 的 `cursor://` 协议处理器接受内嵌 base64 MCP 配置的 deeplink，可向 `~/.cursor/mcp.json` 写入条目；**用户一次点击 + 一次安装对话框确认**即可导致以用户权限执行任意命令（受控测试环境，2026-01-19）。
来源：<https://www.proofpoint.com/us/blog/threat-insight/cursorjack-weaponizing-deeplinks-exploit-cursor-ide>（抓取 2026-08-13）

**对 SkillStar 的直接含义**：任何"一键安装 MCP"的入口都必须落到规范里 Local MCP Server Compromise 那条 MUST——展示完整未截断命令 + 显式批准。

---

## 3. 官方 MCP Registry

### 3.1 状态与 API 形态

Registry 仍标注 **preview**：「Breaking changes or data resets may occur before general availability」。

Base URL：`https://registry.modelcontextprotocol.io`（staging：`https://staging.registry.modelcontextprotocol.io`）

| 端点 | 说明 |
| --- | --- |
| `GET /v0.1/servers` | 列出全部 server（聚合器主用） |
| `GET /v0.1/servers/{serverName}/versions` | 某 server 的全部版本 |
| `GET /v0.1/servers/{serverName}/versions/{version}` | 指定版本，`version` 可用特殊值 `latest` |
| `PATCH /v0.1/servers/{serverName}/versions/{version}/status` | 改单版本状态 |
| `PATCH /v0.1/servers/{serverName}/status` | 改全部版本状态 |
| `POST /v0.1/validate` | 不发布地校验 `server.json` |

**路径参数必须 URL 编码**：`io.modelcontextprotocol/everything` → `io.modelcontextprotocol%2Feverything`。

查询参数：`limit`、`cursor`、`search`（server 名称的大小写不敏感子串匹配）、`updated_since`（RFC3339）、`version`（当前支持 `latest`）、`include_deleted`（默认 false）。

**分页游标形如 `<name>:<version>`**（不是 opaque token）。实测：

```bash
$ curl "https://registry.modelcontextprotocol.io/v0.1/servers?limit=2"
{"servers":[...],"metadata":{"nextCursor":"ac.inference.sh/mcp:1.0.1","count":2}}
```

来源：<https://modelcontextprotocol.io/registry/registry-aggregators>、<https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/api/official-registry-api.md>、实测 `curl`（均 2026-08-13）

> 兼容性提示：`/v0/servers` 实测仍返回 `200` 且响应结构一致，但**官方文档已全面切到 `/v0.1`**，应视为随时可能下线。

### 3.2 响应包封与 `_meta`

实测原始响应（截断）：

```json
{
  "servers": [
    {
      "server": {
        "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
        "name": "ac.inference.sh/mcp",
        "description": "Run 150+ AI apps — image, video, audio, LLMs, 3D and more.",
        "title": "inference.sh",
        "version": "1.0.0",
        "remotes": [
          { "type": "streamable-http", "url": "https://sh.inference.ac" },
          { "type": "streamable-http", "url": "https://api.inference.sh/mcp" }
        ]
      },
      "_meta": {
        "io.modelcontextprotocol.registry/official": {
          "status": "active",
          "statusChangedAt": "2026-04-13T17:32:20.852269Z",
          "publishedAt": "2026-04-13T17:32:20.852269Z",
          "updatedAt": "2026-04-13T17:32:20.852269Z",
          "isLatest": false
        }
      }
    }
  ],
  "metadata": { "nextCursor": "ac.inference.sh/mcp:1.0.1", "count": 2 }
}
```

**注意有两个 `_meta`**：
- `server._meta`：发布者提供的，官方 registry **只保留 `io.modelcontextprotocol.registry/publisher-provided` 一个键**，其余键**静默丢弃**，且该键 marshaled JSON **上限 4096 字节**。
- 响应级 `_meta`：registry 托管的元数据（`io.modelcontextprotocol.registry/official`），发布者不可设置或覆盖。

来源：实测 `curl`、<https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/official-registry-requirements.md>（2026-08-13）

### 3.3 状态与 deprecation 语义

| status | 含义 |
| --- | --- |
| `active` | 正常，默认列表可见 |
| `deprecated` | 仍可见，但应带警告展示 |
| `deleted` | 默认列表隐藏 |

Server 元数据**除 `status` 外基本不可变**。`deleted` 通常意味着违反了 moderation policy（垃圾/恶意/违法），**聚合器可以选择直接从索引中移除**。
来源：<https://modelcontextprotocol.io/registry/registry-aggregators>（抓取 2026-08-13）

### 3.4 `server.json` schema（`2025-12-11`）

`$id`: `https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json`（JSON Schema draft-07，由 `openapi.yaml` 自动生成）

`ServerDetail` 必填：`name`、`description`、`version`。

| 字段 | 约束 |
| --- | --- |
| `name` | `pattern: ^[a-zA-Z0-9.-]+/[a-zA-Z0-9._-]+$`，maxLength 200。**反向 DNS 格式，必须恰好一个 `/` 分隔命名空间与服务名** |
| `description` | **maxLength 100** |
| `title` | maxLength 100（展示名） |
| `version` | maxLength 255，SHOULD semver |
| `repository` | `{url, source, id?, subfolder?}`，`url`+`source` 必填 |
| `websiteUrl` | URI |
| `icons[]` | `{src(必填, HTTPS, maxLen 255), mimeType(png/jpeg/jpg/svg+xml/webp), sizes[], theme(light\|dark)}` |
| `packages[]` | 见下 |
| `remotes[]` | 见下 |
| `_meta` | 反向 DNS 命名的扩展元数据 |

**`Package`**（必填 `registryType`、`identifier`、`transport`）：

```json
{
  "registryType": "npm",
  "registryBaseUrl": "https://registry.npmjs.org",
  "identifier": "remote-filesystem-mcp-server",
  "version": "0.1.5",
  "runtimeHint": "npx",
  "transport": { "type": "stdio" },
  "runtimeArguments": [{ "type": "positional", "value": "-y" }],
  "packageArguments": [],
  "environmentVariables": [
    { "name": "GCS_BUCKET", "description": "Google Cloud Storage bucket name.", "isRequired": true },
    { "name": "GCS_PRIVATE_KEY", "description": "Service account private key.", "isSecret": true },
    { "name": "GCS_MAKE_PUBLIC", "description": "Make uploaded files public.", "default": "false" }
  ]
}
```

- `registryType`：`npm` / `pypi` / `nuget` / **`cargo`** / `oci` / `mcpb`。
- `transport`（`LocalTransport`）：`stdio` | `streamable-http` | `sse`——**本地包也可以是 HTTP 传输**。
- `fileSha256`：`^[a-f0-9]{64}$`，**MCPB 必填**，其他可选。规范说明：registry 不校验该 hash，但**客户端应在安装前校验**。
- `version`：**必须是确定版本**，`^1.2.3` / `~1.2.3` / `>=1.2.3` / `1.x` / `1.*` 一律拒绝。

**`remotes[]`**（`RemoteTransport` = `StreamableHttpTransport | SseTransport` + `variables`）：

```json
{
  "type": "streamable-http",
  "url": "https://api.example.com/{region}/mcp",
  "headers": [
    { "name": "Authorization", "description": "API token", "isSecret": true, "isRequired": true }
  ],
  "variables": {
    "region": { "description": "Deployment region", "choices": ["us", "eu"], "default": "us" }
  }
}
```

- `type` 只能是 `streamable-http` 或 `sse`；`url` 必须匹配 `^https?://[^\s]+$`，支持 `{curly_braces}` 变量。

**`Input` 语义（安装向导的直接依据）**：

| 字段 | 语义 |
| --- | --- |
| `isRequired` | 默认 `false`。true → 必填 |
| `isSecret` | 默认 `false`。true → **客户端应安全处理**（不明文落盘、不回显） |
| `format` | `string`(默认) / `number` / `boolean` / **`filepath`**（应解释为用户文件系统路径 → 应给文件选择器） |
| `choices[]` | 有值时用户**必须**从中选择 → 渲染下拉 |
| `default` | 默认值，必须是合法值 |
| `placeholder` | 只做提示/示例，**不是默认值** |
| `value` | 已设定值，**不应让终端用户改** |
| `variables` | `value` 里 `{curly_braces}` 的替换表，每项还是一个 `Input` |

派生类型：`KeyValueInput`（+ `name`，用于 `environmentVariables` 与 `headers`）、`NamedArgument`（`type:"named"` + `name` 如 `--port` + `isRepeated`）、`PositionalArgument`（`type:"positional"` + `valueHint` 或 `value`，二选一必填）。

Schema 自带的**命令注入警告**（原文）：参数会拼进命令行，可能含用户输入；客户端应优先使用**非 shell 执行**（如 `posix_spawn`），做不到时必须在执行前取得用户或 agent 对**已解析命令**的同意。

来源：`https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json`（实测下载并解析）、实测 registry 响应（2026-08-13）

### 3.5 命名空间与所有权验证

发布者必须证明命名空间所有权：发布到 `com.example/server` 需证明拥有 `example.com` 域名（DNS 验证）；`io.github.<user>/<name>` 走 GitHub 认证。

包所有权验证（按 registryType）：

| 类型 | 验证方式 |
| --- | --- |
| npm | `package.json` 里 `"mcpName": "<server name>"` |
| PyPI | README（即 PyPI 描述）中含 `mcp-name: <server name>`，可藏在 `<!-- -->` 注释里 |
| NuGet | 同 PyPI（README 中 `mcp-name:`，可注释） |
| cargo | README 中 `mcp-name:`，**但 crates.io 会剥离 HTML 注释，必须写成可见 markdown 文本** |
| OCI | 镜像标注 `LABEL io.modelcontextprotocol.server.name="<server name>"` |
| MCPB | `identifier` URL 必须含字符串 `mcp`；必须提供 `fileSha256` |

允许的 registry base URL（白名单，不接受私有仓库/镜像）：npm `registry.npmjs.org`；PyPI `pypi.org`；NuGet `api.nuget.org/v3/index.json`；Cargo `crates.io`；OCI `docker.io` / `ghcr.io` / `quay.io` / `*.pkg.dev` / `*.azurecr.io` / `mcr.microsoft.com`；MCPB 仅 GitHub / GitLab releases。
来源：<https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx>、`.../official-registry-requirements.md`（抓取 2026-08-13）

### 3.6 版本与聚合器规则

版本字符串**每次发布必须唯一**，发布后不可修改。推荐 semver；能解析成 semver 就参与排序并可能标 `latest`，**解析失败则总是标 `latest`**（这是个坑）。版本范围字符串一律禁止。

官方给聚合器的版本比较规则（SHOULD）：
1. 若一方标了 `latest`，视其为更新；
2. 双方都是合法 semver → semver 比较；
3. 都不是合法 semver → 比 `publishedAt`；
4. 一方是合法 semver 一方不是 → **semver 那方视为更新**。

来源：<https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/versioning.mdx>（抓取 2026-08-13）

### 3.7 抓取许可与 ToS（关键）

- Registry 提供**免鉴权只读 REST API**，「Aggregators are expected to scrape data on a regular but infrequent basis (e.g., **once per hour**)」，并**自行持久化**。
- 明确声明：「The MCP Registry **does not provide uptime or data durability guarantees**」。
- **Registry Data 以 CC0 1.0 Universal 奉献公有领域**（ToS 第 10 条），发布者同意其为公开数据，并放弃在部分司法辖区的访问/更正/删除/限制/反对处理权（第 11 条）。**这意味着抓取、缓存、再分发官方 registry 元数据在法律上是明确允许的。**
- 品牌限制（第 6 条）：只能说"数据来自 Official MCP Registry"，**不得**暗示合作/背书/官方身份。
- 生效日期 2025-09-02，适用加州法律。

来源：<https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/terms-of-service.mdx>、<https://modelcontextprotocol.io/registry/registry-aggregators>（抓取 2026-08-13）

### 3.8 Subregistry 模式

聚合器若同时实现官方 OpenAPI 规范，即成为 **subregistry**，客户端可用统一接口消费。自定义元数据注入 `_meta`，建议用反映自身身份的反向 DNS 键：

```json
"_meta": {
  "com.example.subregistry/custom": {
    "user_rating": 4.5,
    "download_count": 12345,
    "security_scan": { "last_scanned": "2025-10-23T12:00:00Z", "vulnerabilities_found": 0 }
  }
}
```

来源：<https://modelcontextprotocol.io/registry/registry-aggregators>（抓取 2026-08-13）

---

## 4. 生态中其他 MCP 目录 / Registry

所有结论均基于 **2026-08-13 实测 HTTP 请求**。

### 4.1 GitHub MCP Registry — `api.mcp.github.com`

| 项 | 实测结果 |
| --- | --- |
| 公开 REST API | 是，免 key，CORS `*` |
| 现行路径 | **`/v0.1/servers`** |
| 旧路径 | `/v0/servers` 仍 `200`，但响应头带 **`deprecation: true`** |
| 限流 | `x-ratelimit-limit: 10`，`x-ratelimit-reset` **每秒递增** ⇒ **约 10 req/s**，不是 10 req/hour |
| 健康端点 | `GET /`（返回 `status`、`sync.strategy`、`data.status: "ready_for_testing"`、`last_oss_snapshot_at`） |
| 数据形态 | 与官方 registry 同构（`{servers:[{server,_meta}],metadata:{nextCursor,count}}`），`$schema` 混用 `2025-09-29` 与 `2025-12-11` |
| 特有字段 | `server._meta["io.modelcontextprotocol.registry/publisher-provided"].github`：`nameWithOwner`、`primaryLanguage`、`license`、`ownerAvatarUrl`、`opengraphImageUrl`、`pushedAt`、`stars` 相关、**以及完整 `readme` 全文**（响应体因此极大，1 条记录即 30KB+） |
| 许可 / ToS | 未见独立 ToS；数据源自官方 registry 的 OSS 快照（`last_oss_snapshot_at`） |
| 稳定性 | 中。`data.status` 自称 `ready_for_testing`；`/v0` 已标记弃用 |

**结论：继续接入，但必须迁到 `/v0.1`。** 它对"展示 GitHub 星标/语言/头像"这类 UI 增强很有价值，代价是 `readme` 全文导致响应体膨胀——建议解析后丢弃 `readme`，或改用官方 registry 拿骨架、GitHub registry 只拿展示增强字段。

### 4.2 Smithery — `registry.smithery.ai`

| 项 | 实测结果 |
| --- | --- |
| 公开 REST API | **是**，`GET /servers?page=&pageSize=` 与 `GET /servers/{qualifiedName}` **无需 key 即返回 200** |
| 规模 | `totalCount: 7912`（2026-08-13） |
| 列表字段 | `id`、`qualifiedName`、`namespace`、`slug`、`displayName`、`description`、`iconUrl`、`verified`、**`useCount`**、`remote`、`isDeployed`、`unlisted`、`inactive`、`createdAt`、`homepage`、`bySmithery`、`owner`、`score` |
| 详情字段 | 额外含 `deploymentUrl`、`connections[]`（`type`、`deploymentUrl`、`configSchema`）、**`tools[]`（含完整 `inputSchema`）**、`security` |
| 鉴权 | 写操作/会话/金库需 API key（`x-api-key`）或 OAuth 2.0+PKCE；**浏览目录可匿名** |
| robots.txt | `smithery.ai` 上 `Disallow: /api/`；但 API 主机是独立的 `registry.smithery.ai`，且官方文档 `smithery.ai/docs/use/registry` 公开记载 |
| ToS | `smithery.ai/terms` 实测 **404**，未找到公开 ToS 文档 |
| 稳定性 | 中高。有官方 TypeScript SDK（`@smithery/registry`） |

**结论：可作为补充数据源接入（只读列表 + 详情），但优先级低于官方 registry。** 它的独有价值是 **`useCount` 使用量**与**预抓取的 `tools[]`**——这两项官方 registry 没有，对"热门排序"和"安装前预览工具"很有用。**风险：没有公开 ToS，再分发条款不明确**，建议只做**运行时代理查询**或短期缓存，不做长期镜像。

### 4.3 PulseMCP — `api.pulsemcp.com`

| 项 | 实测结果 |
| --- | --- |
| `v0beta` | 仍可访问，但**已进入按比例随机失败的日落流程**。原文错误体：「Starting January 2026: 1% of requests fail. Starting April 2026: 10%. Starting June 2026: 50%. **September 2026: Fully sunset (100%)**」 |
| `v0.1` | `401 Unauthorized`，`{"error":"Invalid or missing API key","details":{"header":"X-API-Key"}}` |
| 数据字段 | `name`、`url`、`external_url`、`short_description`、`source_code_url`、`github_stars`、`package_registry`、`package_name`、`package_download_count`、`EXPERIMENTAL_ai_generated_description`、`remotes[]` |

**结论：不推荐接入。** 一个月后（2026-09）旧 API 100% 失效，新 API 强制 API key。让桌面客户端携带共享 API key 分发给终端用户，既有泄露风险又违反多数服务条款；让每个用户自备 key 又是不可接受的安装摩擦。除非 PulseMCP 提供无 key 的公开只读层，否则不接。

### 4.4 Glama — `glama.ai/api/mcp/v1`

| 项 | 实测结果 |
| --- | --- |
| 公开 REST API | **是**，`GET /servers?first=N&after=<cursor>`，免 key |
| 分页 | Relay 风格：`pageInfo{endCursor,hasNextPage,hasPreviousPage,startCursor}`，游标是 base64 JSON（`{"createdAt":…,"id":…}`）——**实测 `after` 可用** |
| 字段 | `id`、`name`、`namespace`、`slug`、`description`、`url`、`attributes[]`（如 `hosting:remote-capable` / `hosting:local-only`）、`repository`、**`spdxLicense`**、`tools[]`、**`environmentVariablesJsonSchema`** |
| 规模 | 站点自称 71,417 个（2026-08-13） |
| robots.txt | 未对通用 `User-agent: *` 设 `Disallow`（仅为若干社交爬虫显式 Allow）；未见 API 层禁令 |
| ToS | 未找到明确的 API 数据再分发条款 |
| 稳定性 | 中。API 无官方文档页（实测 `glama.ai/mcp/servers/api` 是普通目录页），仅靠站点提及 |

**结论：可选补充，谨慎。** 独有价值是 **`spdxLicense`（许可证识别）**与 **`environmentVariablesJsonSchema`（结构化的环境变量 schema）**——对"安装前告诉用户需要哪些密钥"很有用。但**无正式 API 文档、无 SLA、无明确 ToS**，不应作为主数据源，也不应长期镜像。

### 4.5 mcp.so

| 项 | 实测结果 |
| --- | --- |
| 公开 REST API | **无**。`https://mcp.so/api/servers` → `404`；`https://api.mcp.so/servers` → `502` |
| robots.txt | **`Disallow: /api/`**（同时禁 `/search`、`/playground` 等） |

**结论：不推荐接入。** 既无公开 API，`robots.txt` 又明确禁止 `/api/`，抓取页面即违反站点意图。

### 4.6 Docker MCP Catalog

| 项 | 实测结果 |
| --- | --- |
| 公开数据源 | **是**，`https://desktop.docker.com/mcp/catalog/v2/catalog.yaml`（539,294 B）与 **`/v3/catalog.yaml`（580,980 B）**，`Content-Type: application/yaml`，免 key |
| 结构 | `version`、`name: docker-mcp`、`displayName`、`registry: { <slug>: { description, title, type, dateAdded, image(带 sha256 digest), ref, readme(URL), toolsUrl(URL), … } }` |
| 补充端点 | `https://hub.docker.com/v2/repositories/mcp/?page_size=N`（`count: 245`，含 `pull_count`、`star_count`、`last_updated`），限流 `x-ratelimit-limit: 180` |
| 源仓库 | `github.com/docker/mcp-registry`，**MIT 许可**，542 stars，2026-08-12 仍在更新 |
| 条目格式 | `servers/<name>/server.yaml`：`name`、`image`、`type`、`meta.category/tags`、`about.title/description/icon`、`source.project/commit`、`run.allowHosts`、`config.secrets[]`（`name`/`env`/`example`/`description`）、`oauth[]` |
| 稳定性 | 高（Docker 官方产品数据面），但**格式与 `server.json` 不兼容**，需要自写映射 |

**结论：推荐作为"容器化运行时"分支的补充目录。** 独有价值是 `run.allowHosts`（网络出站白名单）与 `image` 带 digest 固定 —— 这是**安全性最好的一类本地 MCP 分发形态**。代价是需要一个独立的 YAML→内部模型映射层。**优先级低于官方 registry**，属于"如果要做容器化安装才值得"。

### 4.7 Awesome-MCP-Servers（`punkpeye/awesome-mcp-servers`）

MIT 许可，92,144 stars，2026-08-03 更新。**纯 README 列表，无结构化数据、无 API。**

**结论：不作为运行时数据源。** 可以作为一次性的人工 curation 输入（比如挑选 curated rows 时参考），但不应该写抓取器。

### 4.8 汇总裁决

| 目录 | 公开 API | 需 key | 许可明确 | 稳定性 | 裁决 |
| --- | --- | --- | --- | --- | --- |
| **官方 Registry** | ✅ `/v0.1` | ❌ | ✅ **CC0 1.0** | 中（preview，无 SLA） | **推荐，设为主数据源** |
| **GitHub MCP Registry** | ✅ `/v0.1` | ❌ | ⚠️ 未独立声明 | 中（`/v0` 已弃用） | **推荐（已接入，需迁 `/v0.1`）** |
| **Smithery** | ✅ | ❌（只读） | ❌ 无公开 ToS | 中高 | **可选补充**（`useCount`/`tools[]`），不镜像 |
| **Glama** | ✅ | ❌ | ❌ 无公开 ToS/文档 | 中 | **可选补充**（`spdxLicense`），不镜像 |
| **Docker MCP Catalog** | ✅ YAML | ❌ | ✅ MIT（仓库） | 高 | **条件推荐**（做容器化安装时） |
| **PulseMCP** | ⚠️ 日落中 | ✅ v0.1 强制 | — | 低 | **不推荐** |
| **mcp.so** | ❌ | — | ❌ robots 禁止 | — | **不推荐** |
| **Awesome-MCP-Servers** | ❌ | — | ✅ MIT | — | **不作运行时源** |

---

## 5. 主流客户端的 MCP 配置文件格式

### 5.1 速查表

| 客户端 | 配置路径 | 顶层键 | 传输类型键与取值 | 远端 URL 键 |
| --- | --- | --- | --- | --- |
| **Claude Code** | 项目 `.mcp.json`；本地/用户 `~/.claude.json` | `mcpServers` | `type`: `stdio` \| `http` \| `sse` \| `ws`（`streamable-http` 是 `http` 的别名） | `url` |
| **Claude Desktop** | macOS `~/Library/Application Support/Claude/claude_desktop_config.json`；Windows `%APPDATA%\Claude\claude_desktop_config.json` | `mcpServers` | 无 `type`（以 `command` 为主） | — |
| **VS Code** | 工作区 `.vscode/mcp.json`；用户配置（`MCP: Open User Configuration`）；`~/.copilot/mcp-config.json`；devcontainer `customizations.vscode.mcp` | **`servers`**（+ `inputs`、`sandbox`） | `type`: `stdio` \| `http` \| `sse` | `url` |
| **Cursor** | 全局 `~/.cursor/mcp.json`；项目 `.cursor/mcp.json` | `mcpServers` | `type`: `stdio`（远端靠有无 `url` 判定） | `url` |
| **Windsurf** | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` | 文档未列 `type` | **`serverUrl`**（亦接受 `url`） |
| **Cline** | CLI `~/.cline/mcp.json`；IDE 扩展 `cline_mcp_settings.json`（VS Code globalStorage） | `mcpServers` | `type`: **`streamableHttp`** \| `sse`（驼峰！） | `url` |
| **Continue** | `config.yaml` 的 `mcpServers`；或 `.continue/mcpServers/*.yaml` | `mcpServers`（**YAML 数组**，元素带 `name`） | `type`: `stdio` \| `sse` \| `streamable-http` | `url` |
| **Zed** | `~/.config/zed/settings.json` | **`context_servers`** | 无 `type`（有 `command` 即本地，有 `url` 即远端） | `url` |
| **JetBrains AI Assistant** | 无独立文件；`Settings \| Tools \| AI Assistant \| Model Context Protocol (MCP)`，可粘贴 JSON、可「Import from Claude」 | `mcpServers` | 无 `type` | `url` |
| **Gemini CLI** | `~/.gemini/settings.json`；项目 `.gemini/settings.json` | `mcpServers` | **无 `type`——靠键区分** | **SSE 用 `url`，Streamable HTTP 用 `httpUrl`** |

### 5.2 逐项字段差异

| 字段 | Claude Code | VS Code | Cursor | Windsurf | Cline | Continue | Zed | Gemini CLI |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `command` / `args` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `env` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅（支持 `$VAR` / `${VAR}`） |
| `envFile` | ❌ | ✅ | ✅（仅 stdio） | ❌ | ❌ | ❌ | ❌ | ❌ |
| `cwd` | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| `headers` | ✅（另有 `headersHelper` 动态生成） | ✅ | ✅ | ✅ | ✅ | ❌（用 `requestOptions`） | ✅ | ✅ |
| `disabled` | 靠 `disabledMcpServers` / `enabledMcpServers`（记录在 `~/.claude.json`，按项目） | 靠 UI/设置 | 靠 UI | 靠 UI | **✅ `disabled: bool`** | ❌ | ❌ | 靠 `mcp.allowed` / `mcp.excluded` |
| `autoApprove` | ❌（用权限系统） | ❌ | ❌ | ❌ | **✅ `autoApprove: []`** | ❌ | ❌ | **`trust: bool`**（跳过确认） |
| `timeout` | ✅ 毫秒（另有 `MCP_TIMEOUT` / `MCP_TOOL_TIMEOUT` 环境变量） | ❌ | ❌ | ❌ | ✅ `timeout` / `networkTimeout` | `connectionTimeout` | ❌ | ✅ 毫秒，**默认 600000** |
| 工具过滤 | ❌ | ❌ | ❌ | `disabledTools`（管理侧） | ❌ | ❌ | ❌ | ✅ `includeTools` / `excludeTools`（exclude 优先） |
| 变量插值 | `${VAR}`、`${VAR:-default}`（作用于 `command`/`args`/`env`/`url`/`headers`） | `${input:id}`、`${env:VAR}`、`${workspaceFolder}` | `${env:NAME}`、`${userHome}`、`${workspaceFolder}`、`${workspaceFolderBasename}`、`${pathSeparator}`/`${/}` | `${env:AUTH_TOKEN}` | ❌ | `${{ secrets.X }}` | ❌ | `$VAR` / `${VAR}` |
| 沙箱 | ❌ | ✅ 顶层 `sandbox`（macOS/Linux）+ 每服务器 `sandboxEnabled` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| OAuth 配置 | 自动（`/mcp` 内交互） | `oauth` 对象 | `auth: {CLIENT_ID, CLIENT_SECRET, scopes}` | ❌ | ❌ | ❌ | ❌ | `oauth` + `authProviderType`（`dynamic_discovery` / `google_credentials` / `service_account_impersonation`） |

### 5.3 几个必须知道的坑

1. **Claude Code：`url` 没有 `type` 是配置错误。** 官方文档原文：有 `url` 无 `type` 的条目会被读成 stdio server，Claude Code 跳过并报 `MCP server "<name>" has a "url" but no "type"; add "type": "http" (or "sse" / "ws") to this entry`。**任何写 Claude Code 配置的工具都必须补 `type`。**
2. **Claude Code 的 `type` 接受 `streamable-http` 作为 `http` 的别名**，正是为了让从服务端文档复制的配置能直接用。
3. **Claude Code 作用域优先级**：Local (`~/.claude.json` 按项目) > Project (`.mcp.json`) > User (`~/.claude.json` 顶层)。**同名冲突时整条 entry 取最高优先级来源，字段不跨作用域合并。**
4. **Claude Code 保留服务器名**：`workspace`、`claude-in-chrome`、`computer-use`、`Claude Preview`、`Claude Browser` —— 定义同名会被跳过并警告。
5. **Claude Code 会对配置值里的首尾空白告警**（检查 `command`、`url`、每个 `args`、`env`/`headers` 的键和值），但**不会自动裁剪**。粘贴带换行的 token 是常见故障源。
6. **Windsurf 用 `serverUrl` 而不是 `url`**，是全场最容易写错的一处。
7. **Gemini CLI 用 `url` 表示 SSE、`httpUrl` 表示 Streamable HTTP**，没有 `type` 字段——从别处复制配置一定会错。
8. **Cline 的 `streamableHttp` 是驼峰**，与规范的 `streamable-http` 和 VS Code 的 `http` 都不同。
9. **Zed 的键是 `context_servers` 而非 `mcpServers`**。
10. **Continue 的 `mcpServers` 是 YAML 数组**（元素带 `name`），不是对象 map。
11. **VS Code 的顶层键是 `servers` 而非 `mcpServers`**（但 `~/.copilot/mcp-config.json` 与工作区 `.mcp.json` 用于跨 Copilot 工具的可移植配置）。

### 5.4 VS Code 的 `inputs` 机制（最成熟的密钥引导范式）

```json
{
  "inputs": [
    { "type": "promptString", "id": "api-key", "description": "API Key", "password": true },
    { "type": "pickString", "id": "region", "description": "Region", "options": ["us", "eu"] },
    { "type": "command", "id": "token", "command": "myext.getToken" }
  ],
  "servers": {
    "example": {
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "example-mcp"],
      "env": { "API_KEY": "${input:api-key}" },
      "cwd": "${workspaceFolder}",
      "sandboxEnabled": true
    }
  },
  "sandbox": {
    "filesystem": { "allowWrite": ["${workspaceFolder}"] },
    "network": { "allowedDomains": ["api.example.com"] }
  }
}
```

相关设置键：`chat.mcp.access`、`chat.mcp.discovery.enabled`、`chat.mcp.autostart`、`chat.mcp.serverSampling`。
来源：<https://code.visualstudio.com/docs/agents/reference/mcp-configuration>、<https://code.visualstudio.com/docs/copilot/customization/mcp-servers>（抓取 2026-08-13）

### 5.5 来源

- Claude Code：<https://code.claude.com/docs/en/mcp>（抓取 2026-08-13）
- Claude Desktop：<https://modelcontextprotocol.io/docs/develop/connect-local-servers>（抓取 2026-08-13，路径从页面 HTML 原文提取）
- VS Code：<https://code.visualstudio.com/docs/copilot/customization/mcp-servers>、<https://code.visualstudio.com/docs/agents/reference/mcp-configuration>、<https://code.visualstudio.com/api/extension-guides/ai/mcp>
- Cursor：<https://cursor.com/docs/context/mcp>
- Windsurf：<https://docs.windsurf.com/windsurf/cascade/mcp>（307 重定向到 <https://docs.devin.ai/desktop/cascade/mcp>）
- Cline：<https://docs.cline.bot/mcp/configuring-mcp-servers>
- Continue：<https://docs.continue.dev/customize/deep-dives/mcp>
- Zed：<https://zed.dev/docs/ai/mcp>
- JetBrains：<https://www.jetbrains.com/help/ai-assistant/configure-an-mcp-server.html>
- Gemini CLI：<https://google-gemini.github.io/gemini-cli/docs/tools/mcp-server.html>

---

## 6. 安装体验的业界做法

### 6.1 一键安装 deeplink

| 客户端 | Deeplink 格式 |
| --- | --- |
| **VS Code** | `vscode:mcp/install?<urlencode(JSON.stringify({name, command, ...}))>`；Insiders 用 `vscode-insiders:mcp/install?...` |
| **Cursor** | `cursor://anysphere.cursor-deeplink/mcp/install?name=<NAME>&config=<BASE64_JSON>` |
| **Windsurf / Devin Desktop** | `windsurf://windsurf-mcp-registry?serverName=<name>` |

生成代码（VS Code 官方示例）：

```js
const link = `vscode:mcp/install?${encodeURIComponent(JSON.stringify(obj))}`;
```

**安全提醒**：deeplink 安装是已被实证利用的攻击面（CursorJack，§2.7）。规范对应的 MUST 是"执行前展示完整未截断命令并取得显式批准"。
来源：<https://code.visualstudio.com/api/extension-guides/ai/mcp>、Cursor 社区与 Proofpoint 报告、<https://docs.devin.ai/desktop/cascade/mcp>（抓取 2026-08-13）

### 6.2 参数 / 密钥引导：以 `server.json` 的 Input 语义驱动表单

业界事实上的做法是把 `server.json` 的 `environment_variables` / `package_arguments` / `runtime_arguments` / `headers` 直接编译成安装表单：

| Input 属性 | 表单渲染 |
| --- | --- |
| `isRequired: true` | 必填校验 |
| `isSecret: true` | 密码输入框；值存 keychain/OS 凭证库，**不写入共享配置文件** |
| `format: "filepath"` | 文件/目录选择器 |
| `format: "number"` / `"boolean"` | 数字框 / 开关 |
| `choices: [...]` | 下拉单选（用户**必须**选其一） |
| `default` | 预填值 |
| `placeholder` | 灰字提示（**不是**值） |
| `value` 已设定 | 只读，不暴露给终端用户编辑 |
| `variables` | 对 `value` 中 `{curly_braces}` 的二级递归表单 |

VS Code 的 `inputs`（`promptString` + `password: true` / `pickString` + `options` / `command`）是这一模式在客户端侧最成熟的实现，可以直接作为 UI 参照。

参数拼装侧还必须处理 schema 自带的**命令注入警告**：优先 `posix_spawn` 式非 shell 执行；无法避免 shell 时，必须在执行前把**已解析的完整命令**展示给用户确认。
来源：`server.schema.json`（`2025-12-11`）、<https://code.visualstudio.com/docs/agents/reference/mcp-configuration>（抓取 2026-08-13）

### 6.3 安装后健康检查

**这里有一个必须更新的认知：`2026-07-28` 之后不再有 `initialize` 握手。** 正确的健康检查是**双路径**的：

```
Modern（2026-07-28+）：
  stdio → server/discover  → 成功即 modern，取 supportedVersions
        → 收到可识别 modern 错误（-32022 等）→ 仍是 modern，换版本重试
        → 其他错误 / 超时 → 判定 legacy
  HTTP  → POST 一个 modern 请求（带三个必需头 + _meta）
        → 400 时先看 body：可识别 modern JSON-RPC 错误 → modern
        → body 空或不可识别 → 回落 legacy
  然后 → tools/list （顺带拿到 ttlMs / cacheScope 用于缓存）

Legacy（≤2025-11-25）：
  initialize → notifications/initialized → tools/list
```

纪元判定结果 SHOULD 按**进程（stdio）/ origin（HTTP）** 缓存，MAY 跨重启持久化，失败时重新探测。

补充信号：
- `server/discover` 的 `instructions` 字段是**给 LLM 的自然语言指引**，安装后可以直接展示给用户看"这个 server 是干什么的"。
- `tools/list` 的 `ttlMs` / `cacheScope` 应该被安装后缓存层直接采纳；`cacheScope: "private"` 的结果**不得跨授权上下文共享**。
- 远端 server 的第一次探测很可能返回 `401 + WWW-Authenticate`，这**不是失败**，而是应当触发 OAuth 流程的信号。

来源：<https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning>、`.../transports/stdio`、`.../transports/streamable-http`（抓取 2026-08-13）

### 6.4 运行时形态选择：remote 优先还是本地包优先

`server.json` 同时可能给出 `remotes[]` 与 `packages[]`。业界与规范信号综合下来的推荐次序：

1. **`remotes[]` 且 `type: "streamable-http"`** —— 首选。零工具链依赖、零本地代码执行、走标准 OAuth、跨设备一致。规范也把 OAuth 授权明确限定为"HTTP 传输"的能力。
2. **`remotes[]` 且 `type: "sse"`** —— 可用但**传输已 Deprecated**，应标注并优先找 streamable-http 替代。
3. **`packages[]` + `registryType: "oci"`** —— 本地但**容器隔离**，`image` 带 digest 可固定，Docker Catalog 还提供 `run.allowHosts` 出站白名单。安全性最好的本地形态。
4. **`packages[]` + `registryType: "mcpb"`** —— 预编译产物，用户无需工具链，**必须校验 `fileSha256`**（registry 不校验，客户端校验）。
5. **`packages[]` + `npm` / `pypi` / `nuget` / `cargo`** —— 需要对应工具链（`npx` / `uvx` / `dnx` / `cargo install`）。注意 **cargo 没有 `npx` 式的一次性运行器**：`cargo install` 是一次性安装，之后按二进制名直接调用，所以 cargo 条目通常**没有 `runtimeHint`**。

选择时还应把用户机器上**实际可用的运行时**纳入判断（`runtimeHint` 只是提示），并对本地形态执行 §2.5 的一键安装同意流程。
来源：`server.schema.json`、<https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx>、<https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization>（抓取 2026-08-13）

---

## 7. 对 SkillStar 的可执行建议

> 说明：以下每条给出**唯一改动落点**。本文不修改任何代码或其他文档；采纳前需按 AGENTS.md 把对应结论写进 `docs/features/mcp/README.md` 或 `docs/decisions.md`。

### P0（正在流血 / 正确性问题）

| # | 建议 | 落点 |
| --- | --- | --- |
| P0-1 | 把 GitHub MCP Registry 的 base URL 从 `https://api.mcp.github.com/v0/servers` 迁到 `/v0.1/servers`——`/v0` 实测已返回 `Deprecation: true` 响应头。 | `crates/skillstar-marketplace/src/mcp_remote.rs`（常量 `MCP_REGISTRY_BASE` 与文件头 doc comment） |
| P0-2 | 新增官方 registry `https://registry.modelcontextprotocol.io/v0.1/servers` 作为一等远端源（**唯一许可条款明确为 CC0 1.0 的源**），GitHub registry 降级为展示增强镜像。 | `crates/skillstar-marketplace/src/mcp_remote.rs` + `crates/skillstar-marketplace/src/mcp_snapshot/mod.rs` |
| P0-3 | 写 Claude Code 配置（`.mcp.json` / `~/.claude.json`）时，凡有 `url` 的条目必须写 `type`，否则 Claude Code 判为配置错误并跳过该 server。 | `crates/skillstar-models/src/mcp/specs.rs`（wire-format writer）+ `crates/skillstar-models/src/mcp/sync.rs` |
| P0-4 | 安装表单按 `Input.isSecret` 分流：secret 值进系统凭证库，绝不明文写入项目级共享配置（`.mcp.json` 会进版本控制）。 | `crates/skillstar-models/src/mcp/store.rs` + `src/features/mcp/`（安装 drawer） |

### P1（能力缺口 / 一年内必须做）

| # | 建议 | 落点 |
| --- | --- | --- |
| P1-1 | 把 `server.json` 解析升级到 `2025-12-11` schema：补 `cargo` / `mcpb` / `oci` 的 `registryType`，补 `fileSha256`（MCPB 安装前必须校验）、`icons`、`websiteUrl`、`title`。 | `crates/skillstar-marketplace/src/mcp_models.rs` |
| P1-2 | 实现"remote 优先"的运行时选择器（streamable-http → sse(标注弃用) → oci → mcpb → npm/pypi/nuget/cargo），并结合本机可用运行时探测。 | `crates/skillstar-app/`（跨域 use case，不放进域 crate） |
| P1-3 | 健康检查改双纪元：modern 走 `server/discover` → `tools/list`，legacy 走 `initialize` → `tools/list`；纪元结论按 origin/进程缓存。 | `crates/skillstar-models/src/mcp/tools.rs` 或新增 `mcp/probe.rs` 私有 module |
| P1-4 | 增量同步改用 `updated_since`(RFC3339) + `cursor`，按官方建议约每小时一次；沿用现有 `MAX_PAGES` 熔断。 | `crates/skillstar-marketplace/src/mcp_snapshot/mod.rs` + `.../snapshot/sync_state.rs` |
| P1-5 | `McpToolSpec` 注册表按 §5.1/§5.2 差异表补齐目标客户端的 wire format 差异（`serverUrl` vs `url` vs `httpUrl`、`streamableHttp` 驼峰、`context_servers` 键名）。**每个新目标只加一行 spec**，符合现有 SSOT 约定。 | `crates/skillstar-models/src/mcp/registry.rs` + `specs.rs` |
| P1-6 | 一键安装本地 server 前，展示**完整未截断的解析后命令**并要求显式确认；优先非 shell 执行。这是规范的 MUST，也是 CursorJack 类攻击的唯一有效缓解。 | `crates/skillstar-app/` + `src/features/mcp/` |
| P1-7 | 远端 server 探测收到 `401 + WWW-Authenticate` 时，走 RFC9728 PRM 发现 → CIMD 优先的 OAuth 流程（DCR 仅作回落），并按 `issuer` 键存储凭证。 | 新增 `crates/skillstar-marketplace` 或 `skillstar-core` 私有 module（先按窄 facade 落在既有 crate，不新建 crate） |

### P2（体验增强 / 机会型）

| # | 建议 | 落点 |
| --- | --- | --- |
| P2-1 | 在 server/tool 详情展示 `ToolAnnotations` 风险标记，并**明确按最悲观默认值渲染**（缺省即 destructive + openWorld）；同时标注"这些是不可信提示"。 | `src/features/mcp/` + `crates/skillstar-marketplace/src/mcp_models.rs` |
| P2-2 | 生成导出 deeplink（`vscode:mcp/install?...`、`cursor://anysphere.cursor-deeplink/mcp/install?name=&config=`），让用户把 SkillStar 里配好的 server 一键推到其他客户端。 | `crates/skillstar-app/` + `src/features/mcp/` |
| P2-3 | 接入 Smithery（`useCount` 热度、预抓取 `tools[]`）与 Glama（`spdxLicense`）作为**运行时代理查询**的补充展示，不做长期镜像（两家均无公开再分发 ToS）。 | `crates/skillstar-marketplace/src/mcp_remote.rs` 的私有 module |
| P2-4 | 若未来做容器化安装，接入 Docker MCP Catalog（`desktop.docker.com/mcp/catalog/v3/catalog.yaml`，MIT 仓库），利用其 `run.allowHosts` 与 digest 固定的 `image`。 | `crates/skillstar-marketplace/src/mcp_snapshot/`（新增独立 loader） |
| P2-5 | 采纳 `CacheableResult` 的 `ttlMs` / `cacheScope` 作为本地缓存策略；`private` 结果不得跨授权上下文复用。 | `crates/skillstar-marketplace/src/mcp_snapshot/query.rs` |
| P2-6 | 用 `server/discover` 的 `instructions` 字段丰富安装后的 server 详情页。 | `src/features/mcp/` |
| P2-7 | 明确**不接入** PulseMCP（2026-09 全量日落 + 强制 API key）与 mcp.so（`robots.txt` 禁 `/api/`），把这条判断写进 `docs/decisions.md` 以免日后重复调研。 | `docs/decisions.md` |

---

## 附录：本次调研使用的一手来源

**规范**
- <https://modelcontextprotocol.io/specification/latest>
- <https://modelcontextprotocol.io/specification/2026-07-28/changelog>
- <https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning>
- <https://modelcontextprotocol.io/specification/2026-07-28/basic/transports>
- <https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio>
- <https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http>
- <https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr>
- <https://modelcontextprotocol.io/specification/2026-07-28/server/tools>
- <https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation>
- <https://modelcontextprotocol.io/specification/2026-07-28/deprecated>
- <https://raw.githubusercontent.com/modelcontextprotocol/modelcontextprotocol/main/schema/2026-07-28/schema.ts>
- <https://blog.modelcontextprotocol.io/posts/2026-07-28/>

**授权与安全**
- <https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization>
- <https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization/client-registration>
- <https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices>
- <https://www.proofpoint.com/us/blog/threat-insight/cursorjack-weaponizing-deeplinks-exploit-cursor-ide>

**Registry**
- <https://modelcontextprotocol.io/registry/registry-aggregators>
- <https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/api/official-registry-api.md>
- <https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/official-registry-requirements.md>
- <https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/versioning.mdx>
- <https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx>
- <https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/terms-of-service.mdx>
- <https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json>

**扩展与客户端矩阵**
- <https://modelcontextprotocol.io/extensions/tasks/overview>
- <https://modelcontextprotocol.io/extensions/client-matrix>

**实测 API 端点（2026-08-13）**
- `https://registry.modelcontextprotocol.io/v0.1/servers`
- `https://api.mcp.github.com/v0.1/servers`、`/v0/servers`、`/`
- `https://registry.smithery.ai/servers`、`/servers/{name}`
- `https://api.pulsemcp.com/v0beta/servers`、`/v0.1/servers`
- `https://glama.ai/api/mcp/v1/servers`
- `https://mcp.so/robots.txt`、`https://smithery.ai/robots.txt`、`https://glama.ai/robots.txt`
- `https://desktop.docker.com/mcp/catalog/v2/catalog.yaml`、`/v3/catalog.yaml`
- `https://hub.docker.com/v2/repositories/mcp/`

**客户端配置文档**：见 §5.5。

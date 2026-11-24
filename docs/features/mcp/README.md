# 外部 MCP 管理与内置商店

状态：active（[D-118](../../decisions.md#d-118外部-mcp-管理域拆分到-ss-mcp-hub实施三层无损入侵并与-magpie-对齐-35-个-mcp-agent)、[D-122](../../decisions.md#d-122移除-gemini-cli-mcp-支持)）

本文档定义 `crates/ss-mcp-hub`、CLI `skillstar mcp` 管理子命令及 GUI **MCP** 页面的功能边界、35 个 Agent 支持矩阵、三层无损入侵约束与内置商店规范。本机项目技能推荐 stdio 服务仍在 [Project skills MCP](../project-skills-mcp/README.md)（`crates/ss-mcp`）。

---

## 1. 支持的 MCP Agent（35 个）

基于 `/Users/xxww/Code/tmp/magpie` `internal/library/targets.go`（`t.MCP != nil`）的内置 MCP Agent 列表，并已移除 `gemini`（Gemini CLI，[D-122](../../decisions.md#d-122移除-gemini-cli-mcp-支持)）；`agy` 自动归一化为规范 ID `antigravity`（共用 `~/.gemini/config/mcp_config.json`）；排除 magpie 内部路由 `claude-magpie-3p`；不纳入仅支持 Skills 的 Agent。

| 规范 ID | 协议格式（`McpFormat`） | 能力限制 | 工具禁用机制 |
| --- | --- | --- | --- |
| `claude` | `Claude` | — | `~/.claude/settings.json` `permissions.deny`（`mcp__<server>__<tool>`） |
| `claude-desktop` | `Desktop` | `no-remote`（仅 stdio） | — |
| `codex` | `Codex` | `no-sse`（环境变量拆分 `env_vars` / `bearer_token_env_var` / `env_http_headers`） | `[mcp_servers.<name>].disabled_tools` |
| `antigravity`（别名 `agy`） | `Antigravity` | `remote=serverUrl`，`no-env-ref` | — |
| `cursor` | `Cursor` | env syntax `${env:NAME}` | — |
| `opencode` | `OpenCode` | `local` cmd array / `remote`，env `{env:NAME}` | `tools.<server>_<tool> = false` |
| `deveco` | `OpenCode` | 同 `OpenCode` | 同 `OpenCode`（支持写入与无损回读 `tools.<server>_<tool> = false`） |
| `mimocode` | `OpenCode` | 同 `OpenCode` | 同 `OpenCode`（支持写入与无损回读 `tools.<server>_<tool> = false`） |
| `pi` | `PiNative` / `Pi`（adapter 3+） | `PiNative`: `no-sse`、`strict`；`Pi`: Literal | — |
| `omp` | `Omp` | `type: stdio \| http \| sse` | — |
| `omo` | `PiNative` | `no-sse` | — |
| `mu` | `PiNative` | `no-sse` | — |
| `goose` | `Goose` | YAML `extensions.<name>`、`no-sse`、env 名限定大写字母数字下划线 | — |
| `copilot` | `Copilot` | `type: local`、`tools: ["*"]` | — |
| `crush` | `Crush` | `mcp.<name>` | `disabled_tools` |
| `zcode` | `ZCode` | `mcp.servers.<name>` | — |
| `kimi` | `Kimi` | `transport: http \| sse`，`no-env-ref` | — |
| `alma` | `Alma` | `url` + `headers`（SSE 加 `transport: sse`），`no-env-ref` | — |
| `hermes` | `Hermes` | YAML `mcp_servers.<name>` | `tools.exclude` |
| `dsh` | `Dsh` | YAML patch `- insert: # skillstar`，`no-sse`，`^[A-Za-z0-9_-]{1,32}$` | — |
| `devin` | `Devin` | `transport: stdio \| http \| sse`，`no-env-ref` | — |
| `grok` | `Grok` | TOML `[mcp_servers.<name>]`，`no-sse` | — |
| `grok-app` | `Grok` | 同 `Grok` | — |
| `droid` | `Claude` | `no-cmd-refs`（`command` / `args` / `env` / `headers` 均按 `Plain` 原样渲染，不使用 `${NAME}` 变量引用） | — |
| `qoder` | `Omp` | 同 `Omp` | — |
| `qoder-cn` | `Omp` | 同 `Omp` | — |
| `cline` | `Cline` | `mcpServers` + nested `transport`，`no-env-ref` | — |
| `commandcode` | `CommandCode` | `no-sse`，`enabled: true` | — |
| `workbuddy` | `Claude` | — | — |
| `workbuddy-ai` | `Claude` | — | — |
| `codebuddy` | `Claude` | — | — |
| `atomcode` | `Omp` | — | — |
| `zed` | `Zed` | `context_servers.<name>`，`no-sse`，`no-env-ref` | — |
| `kiro-cli` | `Kiro` | `no-sse` | — |
| `liveagent` | `LiveAgent` | `{ servers: [...], selected }`，`no-sse` | — |

全部 35 个内置目标的具体落盘路径、平台 env 覆写与 `SKILLSTAR_TOOL_SYNC_HOME` 沙箱映射以 `crates/ss-mcp-hub/src/targets/` 为唯一代码事实源。

---

## 2. 三层无损入侵契约

1. **字节 / 跨度（Byte/Span）保护**：
   - 编辑前完整检测 UTF-8 BOM 与 CRLF 行尾，写入后保持原编码与换行符。
   - 穿透 symlink 真实落盘路径写入，保持符号链接不被替换为普通文件；多硬链接目标采用同文件截断覆盖而非 rename 拆链；序列化字节与原文件完全一致时跳过磁盘写与备份。
   - **JSON / JSONC**：保留全部 `//`、`/* ... */` 注释、尾逗号、原有键序与缩进风格（`\t` 或空格数），仅替换目标 server 键值的字节区间或做最小补丁插入/移除。
   - **TOML**：精确解析 `[table]` 与 `[a.b]` 区间 span；修改 `[mcp_servers.<name>]` 及其 `[mcp_servers.<name>.env]` / `.headers` 子表或 dotted 键时，原样保留文件中其它全部表（如 Codex `[projects.*]`）、注释与排版。
   - **YAML / DSH Patch**：`Goose` / `Hermes` 仅替换对应顶级容器或目标条目的连续行区块，保留文档头 `---`、无关配置与注释；`DSH` 仅维护含 `# skillstar` 注释标记的 `- insert:` / `- set:` 项，不触碰用户手写区块。
   - **滚动备份**：写入真实变动前，将原文件（或空文件占位标记）归档至 `~/.skillstar/data/mcp/backups/` 并按滚动策略清理旧备份。

2. **条目级字段所有权合并**：
   - 每个 `McpFormat` 显式声明自己的 `owned_fields`；写入既有条目时先清理过时旧格式变体（如 `http` 切 `stdio` 时的互斥键），仅写入本次受管字段，保留用户手动加入的非托管字段（例如 `timeout`、`cwd`、`oauth`、`autoApprove`、`startup_timeout_ms` 与未显式覆写的 `startup_timeout_sec` 等）。
   - **Zed 扩展条目守卫**：遇到 `context_servers.<name>` 既无 `command` 又无 `url` 时判定为 `foreign`（Zed Extension 安装的内置 server），跳过并告警，不覆盖或删除。

3. **账本级所有权追踪**：
   - `~/.skillstar/data/mcp/library.json`（互斥锁 `runtime/locks/mcp/library.lock`）记录 `servers[]` 与每个 Agent 的 `applied[agent].mcp` / `applied[agent].tools_off`。
   - 仅当 `agent` 在当前期望集合中或已存在 `applied[agent]` 记录时才读取/写入该 Agent 配置文件；删除操作仅移除账本中明确归属于 SkillStar 的同名条目。
   - 对 Agent 配置中未经纳管的现存 MCP（排除已知客户端内置服务，如 Codex `/cua_node/`、`/openai/codex/runtimes/`），只以 `foundServers` 暴露给用户选择「纳管（import）」或「带备份移除（remove-found）」。

---

## 3. 内置商店、健康探测与 CLI / GUI 接口

- **内置 MCP 商店**：
  - `featured`：内嵌精选推荐列表（已移除 magpie 专有网关服务 `magpie-image` / `magpie-browser` / `magpie-web-search` / `magpie-vision` / `magpie-memory`）。
  - `snapshot`：嵌入自 magpie 对齐的离线快照 `crates/ss-mcp-hub/assets/market_servers.json`（包含 launch args / headers / env schema 提示），支持零网络即时检索与配置展开。
  - `online`：经 `ss_core::infra::http_client::probe_http_client` 查询官方 MCP Registry（`https://registry.modelcontextprotocol.io/v0/servers`），并在直输仓库/包名时在线探测 GitHub `pyproject.toml`（`github.com/owner/repo` 与裸 `owner/repo`）及 PyPI 包元数据；失败或离线时优雅回退至离线结果而不阻塞操作。
  - 直接地址识别：离线即可直接解析 `https://...`（Streamable HTTP / SSE）、`npm:<pkg>`、`npx ...`、`uvx ...` 命令串并支持手动 URL fallback；开启在线查询时补全 `github.com/owner/repo`、裸 `owner/repo` 与裸 PyPI 包名解析。
- **连通性探测（`check_server` / `skillstar mcp check`）**：
  - Stdio 经 `ss_core::infra::path_env::enriched_path()` 补全常用工具链与 Agent 二进制目录后执行 `initialize` → `notifications/initialized` → 分页 `tools/list`；HTTP / SSE 经 `probe_http_client_for_url` 执行 Streamable HTTP 与 Legacy SSE 握手（HTTP `405` 时自动互为回退）并分页获取工具列表；探测到的工具列表可在编辑/详情中逐项禁用并在支持的 Agent（`claude` / `codex` / `crush` / `hermes` / `opencode` / `deveco` / `mimocode`）中应用禁用规则，选中不支持工具禁用的 Agent 时详情页显示不兼容提示条（`mcp-detail-tools-unsupported`）。
- **项目级 MCP 片段**：
  - 支持 `claude` / `codex` / `cursor` / `opencode` / `zcode` / `pi` / `qoder` / `qoder-cn` / `codebuddy` / `droid` 共 10 种项目配置格式生成预览（不擅自修改工程文件）。
- **CLI 命令**：保留 `skillstar mcp serve --stdio` 与 `skillstar mcp approve <plan_id>`；提供 `list` / `agents [--installed]` / `store [query] [--online] [--limit N]` / `store-install` / `add` / `import` / `remove` / `remove-found` / `pause` / `resume` / `paste` / `snippets` / `tools` / `rename` / `sync` / `check` / `backups (list | restore)`（hub 子命令均支持 `--json`）。
- **GUI 页面**：Skills 侧栏导航组新增「MCP」入口（仅在 Skills 模式侧栏，保持 D-085 顶层两模式），`gui/native` 提供 `McpHub` 原生桥模块。页面布局见下节。

---

## 3.1 GUI 页面布局（`gui/src/pages/mcp/`）

MCP 页是操作台，不是长文档：**页面主体不滚动**，只有栏内滚动。一屏内同时看到服务清单与当前服务的详情。

| 区域 | 代码 | 约定 |
| --- | --- | --- |
| 顶部工具栏 | `index.tsx` + `chrome/toolbar.tsx` | 浮动 `PageBar`：标题 + `BarSegmented`（`tone="mode"` 三个页签）；仅「已纳管 / 内置商店」显示 `BAR_SEARCH_W` 搜索框与计数胶囊；右侧动作区为 `BarIconButton` 备份历史（`mcp-btn-backups`）、同步全部（`loading` 旋转态）与粘贴导入 + `BarPrimary` 新建服务。 |
| 左栏（主列表） | `libraryPane.tsx` | 固定帧：延迟骨架屏（`Pulse`）→ 紧凑同步提醒条 → 未纳管发现条 → 统一 `28px` 资源库横条（计数 / `IconSpin` 检测全部 / 排序 / 选择）→ 批量条（选择模式）→ 间隔 `8px` 的行清单。行清单是这一栏里**唯一**的滚动区。 |
| 未纳管发现条 | `discoveryStrip.tsx` | 默认折叠成一行（标题 + 计数 + 来源 Agent + 内联无损提示）；`found` 不超过 3 条时默认展开。展开后视口定高封顶 `116px` 并在内部滚动，不把资源库挤下去。无损纳管 / 移出原配置使用统一 `24px`（`7px` 圆角）行内动作按钮。 |
| 服务行 | `serverRow.tsx` | `58px` 独立圆角卡片行（`12px` 圆角、`8px` 间距、选中态 `accent` 边框 + `card_active` 底色）：`32×32` 传输方式图标块 + 名称（等宽）/ 状态胶囊 / 端点命令；右侧依次是 `22×22` Agent 覆盖图标（最多 4 枚 + `+N` 或「未同步」）与 `ok` 色调启停开关。**每行的 Agent 开关与删除都在详情栏**。 |
| 详情栏 | `detailDrawer.tsx` + `chrome/sheet.tsx` | 嵌于右侧 `DETAIL_COLUMN_W`（352）浮动 `<Sheet>`（`12px` 外间距、`16px` 圆角、`SHEET_SHADOW`）：吸顶头部 → 单一 `<Scroll>` 正文承载三张结构卡片（连接命令/URL 与环境变量/请求头键名胶囊 → 工具探测、逐项禁用与 `mcp-detail-tools-unsupported` 不兼容提示 → Agent 同步双列网格，`AGENT_TILE_W=135`，支持「仅已装 / 全部 35」与不兼容能力提示）→ 吸底编辑 / 删除脚。首次载入自动选中第一个服务，关闭后刷新不自动重开。 |
| 内置商店 | `storeView.tsx` | 顶部 `40px` 筛选栏（`All / Featured / Stdio / Remote` 快捷筛选胶囊 + 错误提醒 + 带标签的「在线 Registry」开关），首次加载显示 3 列 `Pulse` 骨架网格，下方滚动区为 `16px` 圆角卡片（带 `hoverStyle` 与 `38px` `well` 页脚承载传输/运行器/登录态及安装按钮）。 |
| 项目配置与弹窗 | `projectSnippets.tsx`、`editorModal.tsx`、`backupsModal.tsx` | 项目配置拆分为左 `224px` Agent 侧轨 + 右带标题栏与复制按钮的代码预览卡片；编辑器 / 粘贴 / 商店安装 / 备份列表恢复 / 导入冲突弹窗采用 `<Overlay onCancel>` + `promptVeil()`（支持 `Escape` 关闭）：编辑器支持 `multiline` 多行输入（`args` / `env` / `headers`）、必填及格式校验与可选 `startupTimeoutSec`；商店安装弹窗内容区可滚动并支持密钥输入掩码（`masked`）；遇到重名不同配置纳管时弹出 `mcp-import-conflict-modal` 选择保留/替换/重命名。 |

文案走 `mcpStrings.ts` 的 `ms()`（`zh-CN` / `en` 同步维护），颜色一律经 `paletteToken()`（`current` / `paper` 双主题）。新增字段或交互时同步更新上表与本节。

---

## 4. 如何向内置商店新增 MCP 服务

内置商店在 `crates/ss-mcp-hub/src/store/mod.rs`（`query_store`）中按 **`featured`（置顶精选） > `snapshot`（离线快照） > `online`（官方 MCP Registry / GitHub / PyPI）** 顺序聚合去重：

- 同 `id` 仅保留先命中条目；若 `snapshot` 的短 `name` 已在 `featured` 中出现，也会被去重让位于 `featured`。
- 两个离线数据源都在编译期嵌入二进制（`featured.rs` Rust 源码 + `assets/market_servers.json` 的 `include_str!`），**修改后需重新构建/运行程序方可生效**。

| 需求场景 | 修改位置 | 特点 |
| --- | --- | --- |
| 需置顶推荐、`featured` 徽标、精调描述或交互输入模板 | [`crates/ss-mcp-hub/src/store/featured.rs`](../../../crates/ss-mcp-hub/src/store/featured.rs) | 排在最前；可精确控制 `args`、默认 `env`、`StoreInputSpec` 与 `format` |
| 批量收录社区服务，或从 MCP Registry 格式 JSON 导入 | [`crates/ss-mcp-hub/assets/market_servers.json`](../../../crates/ss-mcp-hub/assets/market_servers.json) | 遵循官方 Registry JSON 结构；自动解析包管理器与环境变量输入提示 |

---

### 方式一：新增置顶精选服务（`featured.rs`）

在 `featured_servers()` 返回的 `Vec<StoreItem>` 中追加一条 `local(...)`（Stdio）或 `remote(...)`（Streamable HTTP）。严禁重新引入任何 `magpie-*` 专有条目。

#### 1) 本地 Stdio 服务示例

```rust
local(
    "postgres",                                  // 唯一 id 与默认安装名（建议 <=32 字符）
    "PostgreSQL",                                // 商店卡片标题
    "Model Context Protocol",                    // 发布者
    gh("modelcontextprotocol"),                  // 图标 URL（gh("owner") 生成 GitHub 头像，或传 PNG/SVG URL）
    "https://github.com/modelcontextprotocol/servers/tree/main/src/postgres",
    "Read-only access to PostgreSQL databases and schema inspection.",
    "npx",                                       // runs / 启动命令：npx | uvx | docker | 可执行名
    &["-y", "@modelcontextprotocol/server-postgres"],
    &[],                                         // 默认固定环境变量：&[("KEY", "VALUE")]
    vec![StoreInputSpec {
        key: "dsn".into(),
        where_field: "arg".into(),               // "env" | "header" | "arg"
        label: "Connection URL".into(),
        description: "PostgreSQL DSN passed as positional CLI argument".into(),
        placeholder: "postgresql://localhost/mydb".into(),
        secret: false,
        required: true,
        format: String::new(),                   // 含 "{}" 时按模板替换，如 "--dsn {}"
    }],
),
```

若只需一个必填的密钥型环境变量，可直接复用内置辅助函数 `secret_env(key, label, description)`：

```rust
local(
    "my-tool",
    "My Tool",
    "Acme",
    gh("acme"),
    "https://github.com/acme/my-tool-mcp",
    "Short description shown on card.",
    "uvx",
    &["acme-mcp-tool"],
    &[("DEFAULT_REGION", "us-east-1")],
    vec![secret_env("ACME_API_KEY", "API key", "Get token at app.acme.example/keys")],
),
```

#### 2) 远程 HTTP / SSE 服务示例

```rust
remote(
    "my-cloud",                                  // id 与默认本地服务名
    "My Cloud MCP",
    "Acme Cloud",
    gh("acme"),
    "https://docs.acme.example/mcp",
    "https://mcp.acme.example/mcp",              // Streamable HTTP 端点 URL
    "Manage cloud resources over MCP.",
    false,                                       // sign_in: true 表示首次使用需浏览器 OAuth
    vec![StoreInputSpec {
        key: "Authorization".into(),
        where_field: "header".into(),
        label: "API Token".into(),
        description: "Bearer token for Acme MCP".into(),
        placeholder: "tok_...".into(),
        secret: true,
        required: true,
        format: "Bearer {}".into(),              // 安装时自动格式化为 "Bearer <用户输入>"
    }],
),
```

> 注：`remote(...)` 默认构造 `Transport::Http`（各客户端兼容度最高）。若服务端点只支持旧版 SSE，可在构造后把返回项的 `transport` 与 `template.transport` 改为 `Transport::Sse`。

#### `StoreInputSpec` 字段说明

- `where_field`:
  - `"env"`：写入 `server.env[key]`
  - `"header"`：写入 `server.headers[key]`
  - `"arg"`：将格式化后的值按空白分词并追加到 `server.args` 末尾
- `format`：留空直接写入原值；包含 `{}`（例如 `"Bearer {}"` 或 `"--root {}"`）时，安装器会把 `{}` 替换为用户填的值。

---

### 方式二：新增或更新离线快照（`assets/market_servers.json`）

文件顶层为 JSON 数组（`RawMarketServer[]`），与官方 `registry.modelcontextprotocol.io/v0/servers` 的 `server` 结构对齐；检索权重优先按 `match_by_addr`、`name`/`title` 精确/包含匹配与 `stars` 排序。

#### 1) 包发布型（Stdio：npm / pypi / oci docker）

```json
{
  "name": "io.github.example/my-mcp-server",
  "title": "My MCP Server",
  "description": "One-line description displayed in built-in store.",
  "websiteUrl": "https://github.com/example/my-mcp-server",
  "repository": { "url": "https://github.com/example/my-mcp-server" },
  "icons": [{ "src": "https://github.com/example.png?size=96" }],
  "packages": [
    {
      "registryType": "npm",
      "identifier": "@example/my-mcp-server",
      "runtimeHint": "npx",
      "transport": { "type": "stdio" },
      "packageArguments": [
        {
          "type": "named",
          "name": "--workspace",
          "valueHint": "/path/to/workspace",
          "description": "Workspace root directory",
          "isRequired": true
        }
      ],
      "environmentVariables": [
        {
          "name": "EXAMPLE_API_KEY",
          "description": "API key for Example service",
          "isRequired": true,
          "isSecret": true
        }
      ]
    }
  ],
  "stars": 1200
}
```

自动映射规则（[`snapshot.rs`](../../../crates/ss-mcp-hub/src/store/snapshot.rs)）：
- 优先选用 `registryType` 为 `"npm"`（默认 `npx -y`）或 `"pypi"`（默认 `uvx`），其次为 `"oci"` / `"docker"`（默认 `docker run --rm -i`），也可用 `runtimeHint` 显式指定执行器。
- `packageArguments` 有固定 `value` 或 `default` 时直接加入默认 `args`；含 `valueHint` 或 `isRequired: true` 且无默认值时自动生成安装输入项（`where_field: "arg"`，`named` 类型自动带 `"--flag {}"` 格式）。
- `environmentVariables` 有固定 `value` 时写入默认 `env`；无固定值且 `isRequired` / `isSecret` 为 `true` 时自动生成 `where_field: "env"` 输入项。

#### 2) 远程端点型（Streamable HTTP / SSE）

```json
{
  "name": "com.example/remote-mcp",
  "title": "Example Remote MCP",
  "description": "Cloud-hosted MCP endpoint.",
  "websiteUrl": "https://example.com/mcp",
  "repository": { "url": "https://github.com/example/remote-mcp" },
  "remotes": [
    {
      "type": "streamable-http",
      "url": "https://mcp.example.com/v1",
      "headers": [
        {
          "name": "Authorization",
          "value": "Bearer {EXAMPLE_TOKEN}",
          "description": "API token from dashboard",
          "isRequired": true,
          "isSecret": true
        }
      ]
    }
  ],
  "stars": 850
}
```

- `remotes[].type` 填 `"streamable-http"` 映射为 `Transport::Http`，填 `"sse"` 映射为 `Transport::Sse`。
- `headers[].value` 若含 `{...}` 占位符，会自动转化为 GUI/CLI 安装输入弹窗（`Bearer {...}` 转为 `"Bearer {}"` 格式模板）。

---

### 收录前兼容性检查清单

1. **名称规范**：`id` 与生成的默认短 `name` 建议仅用 `[A-Za-z0-9_-]` 且长度 `<= 32`（满足 DeepSeek Harness `^[A-Za-z0-9_-]{1,32}$`）。
2. **环境变量键名**：优先使用全大写字母、数字与下划线（`^[A-Z][A-Z0-9_]*$`），保证 Goose `env_keys`、Codex `env_vars` 与 OpenCode `{env:NAME}` 无障碍透传。
3. **传输协议兼容度**：`claude-desktop` 仅支持 `stdio`；`codex`、`goose`、`dsh`、`zed`、`kiro-cli` 等 10 家不支持 `sse`。远程服务建议优先提供 `streamable-http`（`Http`）并尽量附上对应的本地包（`packages`）。
4. **模板合法性**：Stdio 必须带非空 `command`；远程必须带有效 `http://` 或 `https://` URL，以便通过 `env_refs::validate_server`。

---

### 验证命令

完成编辑后，执行以下本地验证即可：

```bash
# 1. 自动校验 featured 唯一性/无 magpie 前缀，以及全部 featured + snapshot 模板合法性
cargo test -p ss-mcp-hub --locked

# 2. 用 CLI 检索并确认新条目展示与安装参数正常
cargo run -p ss-bin --bin skillstar --locked -- mcp store <关键词>

# 3. 验证 GUI 桥接与 MCP 商店视图
nsh check gui && nsh test gui
```

# Models 网关

状态：36 已落地。最后更新：2026-09-30。

## Next Agent Prompt

你正在实现 SkillStar Models 的本机网关。不要从聊天记录恢复产品决定，以本目录为准。`/tmp/skillstar-models-gateway-brief.md` 和 `docs/others/model-redesign/05-redesign-proposal.md` 都已被本目录取代。

36 已落地。请求里的 effort 在出站前收成目录缓存里的等级。没有这个模型时原样通过。分组成员可以写成 `provider/model:<等级>`，固定值优先。Claude 进程桥的 `xhigh` → `max` 仍只在启动参数上。参照作物不在仓库里，按五问接受。选择记在 [choices.md](choices.md)。

下一档是 [slices/37-visible-families.md](slices/37-visible-families.md)：每个 Agent 可以有一份可见名单。名单里的项是家族标签、provider id 或 group id。家族标签写在 provider 或分组的 `family`。没有名单时看到全部模型。名单收窄该 Agent 的 `/v1/models` 和写进它文件的模型目录。被收窄的 id 若仍来请求，网关照常回答。路由顺序不读这份名单。已经保存的 model ref 不被清空。这是视觉档，裁该 Agent 打开选择器时的列表，看 1440×900 与 1280×800。不要重启已经在跑的 Vite。不要改 `cursor.rs`。做完一档，把该档会改的文档一起提交，然后回到本节：改状态、把下一档指到新的入口、勾掉对应 TODO。

网关是独立 crate `skillstar-gateway`（`crates/skillstar-gateway`）。01 档建它。它的 skillstar 依赖只有 `skillstar-core`。`skillstar-models` 和 `skillstar-usage` 不依赖它，它也不依赖这两个 crate，也不依赖 `skillstar-decision` 或 `skillstar-app`。只有 `skillstar-app` 依赖它，从 04 档起。

不要打开 `crates/skillstar-usage/src/fetchers/oauth/cursor.rs`。不要改 Tauri updater（`src-tauri/tauri.conf.json`、`src-tauri/src/commands/updater.rs`、`src/features` 里的 updater hook、`.github/workflows/release.yml`）。不要请求 `usemagpie.ai`，不要移植 `internal/update`、`site/worker.js`、`/api/latest`、`/download` 或 `install.sh`。不要在启动时改写 Agent 配置文件。不要把网关代码放进 `skillstar-models`。

全局决定在下面。切片里标成「可改」的才是实现自由。未列出的决定先改规格，再写代码。

### TODO

- [x] 01 协议夹具 — [slices/01-protocol-fixtures.md](slices/01-protocol-fixtures.md)
- [x] 02 首内容字节 — [slices/02-content-byte.md](slices/02-content-byte.md)
- [x] 03 一条路由决定 — [slices/03-routing-probe.md](slices/03-routing-probe.md)
- [x] 04 环回监听 — [slices/04-loopback-serve.md](slices/04-loopback-serve.md)
- [x] 05 Claude 进程桥 — [slices/05-claude-bridge.md](slices/05-claude-bridge.md)
- [x] 06 Codex 写入 — [slices/06-codex-writer.md](slices/06-codex-writer.md)
- [x] 07 信息架构 — [slices/07-information-architecture.md](slices/07-information-architecture.md)
- [x] 08 HTTP 表面 — [slices/08-http-surface.md](slices/08-http-surface.md)
- [x] 09 四种路由 — [slices/09-routing-modes.md](slices/09-routing-modes.md)
- [x] 10 亲和 — [slices/10-affinity.md](slices/10-affinity.md)
- [x] 11 休息与换上游 — [slices/11-rest.md](slices/11-rest.md)
- [x] 12 分组 — [slices/12-groups.md](slices/12-groups.md)
- [x] 13 规则 — [slices/13-rules.md](slices/13-rules.md)
- [x] 14 分类器 — [slices/14-classifier.md](slices/14-classifier.md)
- [x] 15 脱敏 — [slices/15-redact.md](slices/15-redact.md)
- [x] 16 视觉转述 — [slices/16-vision.md](slices/16-vision.md)
- [x] 17 订阅签名 — [slices/17-signing.md](slices/17-signing.md)
- [x] 18 共享写入 — [slices/18-shared-writers.md](slices/18-shared-writers.md)
- [x] 19 Claude Code 文件 — [slices/19-claude-code.md](slices/19-claude-code.md)
- [x] 20 Claude Desktop — [slices/20-claude-desktop.md](slices/20-claude-desktop.md)
- [x] 21 OpenHanako — [slices/21-hanako.md](slices/21-hanako.md)
- [x] 22 Alma — [slices/22-alma.md](slices/22-alma.md)
- [x] 23 Cindy — [slices/23-cindy.md](slices/23-cindy.md)
- [x] 24 WSL Codex — [slices/24-wsl-codex.md](slices/24-wsl-codex.md)
- [x] 25 models.dev — [slices/25-models-dev.md](slices/25-models-dev.md)
- [x] 26 提供商行 — [slices/26-provider-row.md](slices/26-provider-row.md)
- [x] 27 模型选择器 — [slices/27-model-picker.md](slices/27-model-picker.md)
- [x] 28 次要字段 — [slices/28-secondary-fields.md](slices/28-secondary-fields.md)
- [x] 29 最近请求 — [slices/29-recent-calls.md](slices/29-recent-calls.md)
- [x] 30 路由控件 — [slices/30-routing-controls.md](slices/30-routing-controls.md)
- [x] 31 空态 — [slices/31-empty-states.md](slices/31-empty-states.md)
- [x] 32 分组控件 — [slices/32-group-controls.md](slices/32-group-controls.md)
- [x] 33 配置档 — [slices/33-profiles.md](slices/33-profiles.md)
- [x] 34 局域网 — [slices/34-lan.md](slices/34-lan.md)
- [x] 35 模型改名 — [slices/35-model-rename.md](slices/35-model-rename.md)
- [x] 36 effort — [slices/36-effort.md](slices/36-effort.md)
- [ ] 37 可见家族 — [slices/37-visible-families.md](slices/37-visible-families.md)
- [ ] 38 effort 子集 — [slices/38-effort-subset.md](slices/38-effort-subset.md)
- [ ] 39 不写环回 URL — [slices/39-name-only.md](slices/39-name-only.md)

## 目标

Models 页变成 magpie 那种本机模型切换：每个探测到的 Agent 一行，模型 id 是 `provider/model` 或 `group/<id>`。托管写入只把该 Agent 指到 SkillStar 的环回网关。厂商密钥留在 v4 provider 存储。订阅登录、配额、账号切换留在 Usage。网关转发时读取这两处已经写下的凭证。

用户定案：

- 接线是 1:1 复刻 magpie 的网关与 Agent 配置，范围只在 Models。
- 可以拆掉现在的 Models 产品行为。
- 不迁移。不改写已经写进 Agent 文件的厂商 URL 和密钥。那些文件只在用户于新工作台保存时改变。
- `usemagpie.ai` 的更新源不学。SkillStar 继续用自己的 Tauri updater。

参照实现（只读，CI 不编译它）：`/Users/xxww/Code/tmp/magpie`，模块 `github.com/yetone/magpie`。行为以该树的源码和测试为准。本目录的常量表是从该树抄下的数字；实现时若源码与本表不一致，先改本表再改代码。

打开 [visualizations/ladder.html](visualizations/ladder.html) 看档位。01 的探针不监听端口。04 起人可以运行 `skillstar gateway serve`。

## 切片图

```text
01 协议夹具 ──► 02 首内容字节 ──► 04 环回监听 ──► 08 HTTP 表面
01 ──► 03 一条路由决定 ──► 09 四种路由 ──► 10 亲和 ──► 11 休息
04 ──► 05 Claude 进程桥
04 ──► 06 Codex 写入 ──► 07 信息架构 ──► 26…37 视觉
08 ──► 12 分组 ──► 13 规则 ──► 14 分类器 ──► 32 分组控件
08 ──► 15 脱敏
08 ──► 16 视觉转述
04 ──► 17 订阅签名
06 ──► 18 共享写入 ──► 19 Claude Code
18 ──► 39 不写环回 URL
18 ──► 20 Desktop ──► 21 Hanako
18 ──► 22 Alma ──► 23 Cindy ──► 24 WSL Codex
08 ──► 25 models.dev ──► 27 模型选择器
09 ──► 30 路由控件
11 ──► 29 最近请求
06 ──► 31 空态
18 ──► 33 配置档
04 ──► 34 局域网
25 ──► 35 改名 ──► 36 effort ──► 38 effort 子集
25 ──► 37 可见家族
```

按编号做。01、02、03 可以先后独立验证，仍不要并行改 `skillstar-gateway` 的公共类型。

## 所有者

一个概念只有一个所有者。网关是今天就独立的 crate，不是 `skillstar-models` 里后加的模块。

`skillstar-gateway` 放在 `crates/skillstar-gateway`。workspace 的 `members = ["crates/*"]` 会收进它。`edition`、`license`、`version` 用 workspace。skillstar 依赖只有 `skillstar-core`，用 `cargo add -p skillstar-gateway skillstar-core` 加。第三方依赖同样用 `cargo add`，版本归一在根 `Cargo.toml`。

删除测试：从 workspace 拿掉这个 crate 之后，`cargo check -p skillstar-models --locked` 和 `cargo check -p skillstar-usage --locked` 仍然通过。所以 models 与 usage 的 `Cargo.toml` 不出现 `skillstar-gateway`，网关的 `Cargo.toml` 也不出现 `skillstar-models`、`skillstar-usage`、`skillstar-decision`、`skillstar-app`。01 档把这些禁止边写进 `scripts/internal/check_workspace_deps.sh`，并写一条正向断言：网关的 skillstar 依赖集合等于 `{skillstar-core}`。

| 概念 | 所有者 |
| --- | --- |
| 协议翻译、路由、分组、规则、分类器、脱敏、视觉转述、Codex 后端路径、图像路由、环回占位 bearer、Agent 配置写入 | `skillstar-gateway` |
| 监听循环、`claude-mcp-helper` | `skillstar_gateway::serve`。GUI 与 CLI 经 `skillstar-app` 调用它。`src-tauri` 只认 argv |
| 上游流式 HTTP | `skillstar_core::infra::http_client` 里与 `probe_http_client` 共用代理指纹的流式客户端。连接 10 秒，没有覆盖整个响应体的总超时，响应头最多等 10 分钟 |
| 短探测、models.dev 下载 | 现有 `probe_http_client`。流式生成不走它 |
| provider 密钥表 | 现有 v4 `model_providers.json`，所有者仍是 `skillstar-models`。`version` 保持 4。本梯不往这张表加路由、亲和或回退字段 |
| 路由、亲和、回退、分组、脱敏、视觉模型、监听模式 | `skillstar-gateway` 的 `config_dir()/model_gateway.json` |
| 交给网关的密钥与账户 | 网关 crate 里的 `ProviderSnapshot` / `AccountSnapshot`。`skillstar-app` 从 Models 与 Usage 填它们。网关不打开 `model_providers.json`，不调用 Usage 的存储函数 |
| 接管前的 Agent 字段 | `config_dir()/agent_stash.json`，权限 `0600`，原子替换。所有者是 `skillstar-gateway` |
| 脱敏密钥 | `config_dir()/redact.key`，权限 `0600` |
| 新页面用的模型目录 | `skillstar-gateway` 写 `<data_root>/cache/gateway-catalog/models.dev.json`。不写 `ProviderEntryFlat.meta.model_catalog`，不并进 `cache/model_catalog/` |
| 最近 60 条调用 | 网关进程内的环。`traceKeep = 60`。不进 Usage，不落配额库 |
| Usage 凭证与配额快照 | `skillstar-usage`。缺失快照是 unknown，不是耗尽。网关不发起配额请求 |
| 跨域编排（填 snapshot、GUI 启动 serve、命令） | `skillstar-app`。从 04 档起它是唯一依赖 `skillstar-gateway` 的产品 crate |
| 命令 DTO | `skillstar-app` 的 models DTO。响应不带明文密钥 |
| Settings 里的 App AI 与 Decision 模型 | 保持现状。`get_providers_flat` 与 `compat.rs` 只为这条路径继续存在 |
| 新 Models 页 | `src/features/models/`。不调用 v3 flat bind，不读 `compat.rs` |
| 决策模型 AgentJev | `skillstar-decision`。分类器不调用它，网关 crate 不依赖它 |
| 旧的直写厂商密钥 | `skillstar-models::tool_sync` 在 06 档失去这些写入。此后不再有第二套 writer |
| 更新 | 现有 Tauri updater。平台文档 `docs/features/platform/README.md` |

`skillstar-models` 不依赖 `skillstar-usage`，也不依赖 `skillstar-decision`。`skillstar-usage` 不依赖 `skillstar-models`。

数据目录继续走 `skillstar_core::infra::paths`。`SKILLSTAR_DATA_DIR` 与 `SKILLSTAR_TOOL_SYNC_HOME` 继续生效。测试把这两个变量和 `HOME`（Windows 再加 `USERPROFILE`）指到临时目录。

## 常量表

监听：

| 名字 | 值 |
| --- | --- |
| 默认地址 | `127.0.0.1:21847` |
| 环境变量 | `SKILLSTAR_GATEWAY_ADDR` |
| 拒绝的端口 | `3425`（magpie 的 `DefaultAddr`）。环境变量写成这个端口时进程非 0 退出，不监听 |
| 写入 Agent 文件的 URL | `http://127.0.0.1:<实际端口>`。局域网档打开后仍然写这个环回 URL |
| 入站读头超时 | 30 秒 |
| 入站空闲超时 | 5 分钟 |
| 占位 bearer | `skillstar` |
| 按 Agent 的占位 bearer | `skillstar-<agent-id>`，对应 magpie 的 `TokenFor` |
| 会话头 | `X-Skillstar-Session`。同时仍读 Agent 自己的 `x-opencode-session`、`x-session-affinity`、`x-session-id`、`session_id`、`session-id`、`x-claude-code-session-id`。不读 `X-Magpie-Session` |
| 分类器 UA | `skillstar-router/1` |
| 视觉转述 UA | `skillstar-vision/1` |
| hello 名字 | `skillstar` |
| Claude 回调 | `POST /_skillstar/claude-mcp/{token}`，仅环回 |
| 临时目录前缀 | `skillstar-claude-` |
| Desktop 配置档 id | `00000000-0000-4000-8000-736b696c6c73`（末 12 位十六进制是 ASCII `skills`）。不是 magpie 的 `6d6167706965` |
| Desktop 别名前缀 | `anthropic/skillstar-` 与 `mythos-skillstar-`。序号是该模型 id 的 FNV-1a 64 位，对 `1e10` 取模，十进制补零到 10 位 |
| Codex 目录文件 | `skillstar-models.json` |
| Codex 表名 | `[model_providers.skillstar]` |
| 压缩标记 | `skillstar1:`。Codex 自带的 Apache-2.0 压缩提示词原文保留 |

路由与分类（抄自 magpie，实现时对单测）：

| 名字 | 值 | 出处 |
| --- | --- | --- |
| 路由空字符串 | smart | `groups_cli.go` |
| 亲和空字符串 | auto：回合内保持；跨回合在缓存读够且未冷时保持 | `affinity.go` |
| `lowShare` / `usedShare` | 90 / 98 | `routing.go` |
| `cacheWorth` | 1024 | `affinity.go` |
| `cacheCold` | 5 分钟 | `affinity.go` |
| `stickKeep` | 24 小时 | `affinity.go` |
| `creditRest` | 30 分钟 | `routing.go` |
| `quotaRest` | 15 分钟 | `routing.go` |
| `longestWait` | 1 小时 | `routing.go` |
| `longestQuota` | 8 天 | `routing.go` |
| `longestRetry` | 10 分钟 | `routing.go` |
| `verifyRest` / `verifyHold` | 30 分钟 / 1 分钟 | `routing.go` |
| `fallbackCooldown` | 1 分钟 | `fallback.go`。频率限制没有 Retry-After 时用它 |
| `resetsHeader` | `X-Skillstar-Resets-At`。不读 `X-Magpie-Resets-At` | `routing.go` 的 `X-Magpie-Resets-At` |
| `holdLongest` / `holdMost` | 15 秒 / 1 MiB | `fallback.go` |
| `maxNest` | 8 | `provider/group.go` |
| `traceKeep` | 60 | `trace.go` |
| `jevSure` | 0.4 | `decide.go` |
| `classifyTimeout` | 8 秒 | `classify.go` |
| `classifyKeep` | 10 分钟 | `classify.go` |
| `classifyRest` | 30 秒 | `classify.go` |
| Claude 回合中止 | 30 分钟 | `claude_subscription.go` |
| `idleLongest` / `idleMost` | 20 分钟 / 6 | 同文件 |
| `parkLongest` | 5 分钟 | 同文件 |
| `visionTimeout` / `visionParallel` / `sightsKept` | 2 分钟 / 4 / 256 | `vision.go` |
| `drawTimeout` | 5 分钟 | `draw.go` |

首内容字节之前可以换上游。`message_start`、`response.created`、`response.in_progress`、`response.queued`、`ping`、只带 `role` 的 Chat 块、以及 `codex.` 前缀事件都不是内容。内容字节、或等待超过 `holdLongest`、或缓冲超过 `holdMost` 之后，这一回复不再换上游。

## 已定行为

### 网关听什么

复制 magpie `Handler` 的这些路由，品牌换成上表：

- `GET /`、`GET /api/hello`
- `GET /v1/models`、`GET /models`、`GET /v1/models/{id...}`
- `POST /v1/chat/completions`、`POST /chat/completions`
- `POST /v1/responses`、`POST /responses`
- `POST /v1/messages`、`POST /messages`、`POST /v1/messages/count_tokens`
- `POST /v1/images/generations`、`POST /images/generations`
- `POST /v1/images/edits`、`POST /images/edits`
- `GET /v1beta/models`、`POST /v1beta/models/{call...}`
- `POST /_skillstar/claude-mcp/{token}`
- `/backend-api/codex/`（WebSocket Upgrade 回 426；模型 id 含 `/` 时留在本机，不落到 OpenAI）

不提供 `/v1/magpie/quotas`，也不提供 `/v1/skillstar/quotas`。最近请求是进程内的 60 条环，给 Models 页看，不给 Agent 当配额接口。

模型 id 含 `/` 的请求由网关解析。解析失败是本机错误，不把路径转给厂商。

### Claude 订阅

订阅生成走本机 `claude` 二进制加 MCP 工具桥。查找顺序与 magpie `claudeBinary` 相同：`PATH`，然后 `~/.local/bin/claude`、`/usr/local/bin/claude`、`/opt/homebrew/bin/claude`。

子进程环境先删掉 `ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`、`ANTHROPIC_AUTH_TOKEN`、`CLAUDE_CODE_OAUTH_TOKEN`、`CLAUDECODE`、`CLAUDE_CODE_ENTRYPOINT`、`CLAUDE_CODE_SSE_PORT`，再补上 `ENABLE_CLAUDEAI_MCP_SERVERS=0`、`DISABLE_AUTO_COMPACT=1`、`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`。

参数：`-p --output-format stream-json --input-format stream-json --include-partial-messages --verbose --model <model> --tools <WebSearch 或空> --strict-mcp-config --mcp-config <json> --setting-sources "" --dangerously-skip-permissions --no-session-persistence`。effort `xhigh` 写成 `max`，并带 `--effort` 与 `--thinking-display summarized`。MCP server 的命令名是当前 `skillstar` 可执行文件，参数是 `claude-mcp-helper`、回调 URL、工具文件。

SkillStar 不把 Usage 的 access token 放进 `CLAUDE_CODE_OAUTH_TOKEN`，不调用 Anthropic 的令牌 URL，不代刷新，不把令牌写回钥匙串或 `~/.claude/.credentials.json`。二进制读它自己的登录态。这是相对 magpie `claude_subscription.go` 里 `oauth != ""` 时注入令牌的允许差异。

### 谁可以签名

Usage 已有的目录 id 才能被网关拿来签上游。映射：

| Usage `catalog_id` | 网关账户 |
| --- | --- |
| `anthropic` | 只走进程桥，不签 Anthropic HTTP |
| `codex` | Codex 后端路径上 magpie 会替已保存账户签的那些上游 |
| `github-copilot` | copilot |
| `cursor` | cursor。签名代码放在 `skillstar-gateway`。实现时可以读 `cursor.rs`，不能改它 |
| `xai` | grok |
| `kiro` | kiro |
| `zcode` | zcode |
| `antigravity` | antigravity |

gemini CLI、devin、workbuddy、commandcode 没有 Usage 凭证行。它们的 Agent 配置仍在范围内，上游只使用 provider 表里的 API 密钥。不为它们做第二套登录，不读厂商自己的认证文件。

### Agent 花名册

与 magpie `agent.All()` 相同，外加 Windows 上的 `codex@wsl:<distro>`：Claude Code、Claude Desktop、Codex、Gemini CLI、OpenCode、MiMo Code、Pi、Goose、Cursor CLI、Copilot CLI、Crush、DeepSeek Harness、Command Code、fx、omp、Devin、Hermes、Cline、Qoder、Qoder CN、Grok Build、ZCode、WorkBuddy、OpenHanako、Alma、Cindy。

06 档先证明 Codex 的两种写盘，并让其余旧 writer 停止写出厂商密钥。18 档用同一个 `apply_gateway` 覆盖「环回 base URL + 占位 bearer」的文件型 Agent。19–24 是该契约盖不住的特例。未实现的 Agent 保存时返回 `agent_not_managed`，写入字节数为 0。

Claude Code 的 base URL 是网关根，不带 `/v1`，令牌字段是 `ANTHROPIC_AUTH_TOKEN`，值是占位 bearer。Claude Desktop 写原生 `claude_desktop_config.json` 与 Claude-3p 配置档。现在的 `skillstar-binding.json` marker 不再写入。

Cindy 只生成 `cindy://provider/import?...`，数据库只读。Hanako 与 Alma 在对方进程活着时走它们的本地 API；进程不在时 Hanako 写文件，Alma 不是错误。

### 启动时不改 Agent 文件

`load_store_and_repair` 不再为了投影绑定去调用 `repair_agent_configs`。provider 保存不再调用 `resync_active_tools`。已有的 v1→v4 **store 文件**读取保持原样，那是密钥表自己的磁盘格式，不是本功能新增的迁移。本功能不新增 store 版本号，不给 v4 加路由字段，不双写厂商 URL。路由在 `model_gateway.json`。

`Credential::ApiKey` 的「故障转移链只取第一把钥匙」语义随旧 writer 一起停用。v4 里多钥匙的存储可以留在密钥表。

### 允许的字节差异

夹具比较前只允许这些差异，其余都是缺陷：

- 端口、进程名、User-Agent、`Date`
- 标识里的品牌：`magpie` → `skillstar`（路径、bearer、会话头、hello、别名前缀、配置档 id、压缩标记、临时目录、目录文件名、provider 表名）
- Chat 的 `id` 与 `created`
- 不刷新、不注入 Claude OAuth
- 不请求配额 URL
- 不比较连接池大小

`visionSystem` 与 Codex 的 `codexCompactPrompt`、`codexSummaryPrefix` 按源码逐字复制。

## 被否决的做法

| 做法 | 为什么不采用 |
| --- | --- |
| 默认或允许监听 `3425` | 那是 magpie 的端口。两边一起开时会抢端口 |
| 默认端口用 `18721` | 与 `21847` 没有实测冲突差别。四份草稿对半。选定 `21847`，只留一个默认值 |
| 把网关放进 `skillstar-models` 的私有模块 | 用户要求做成 crate。删除测试是拿掉 `skillstar-gateway` 后 models 与 usage 仍能编译。网关不依赖它们，密钥表和订阅存储也不会被网关的协议栈拖着编译 |
| 让 `skillstar-gateway` 依赖 `skillstar-models` 或 `skillstar-usage` | 那样删除测试只剩一半：crate 还在时，改密钥表会重编整座网关；crate 拿掉时，网关的类型会把 models 的结构带走 |
| 把路由、亲和、回退加进 v4 `model_providers.json` | 那些字段的所有者是网关配置。密钥表保持密钥表，`version` 保持 4 |
| 把 AgentJev 当成路由分类器 | magpie 的 Jev 是一次 HTTP 模型调用，阈值 0.4。AgentJev 是 Settings 里的本地模型，接进去会改变路由 |
| 把未过期的 Usage access token 放进 `CLAUDE_CODE_OAUTH_TOKEN` | Usage 拥有刷新。刷新令牌是一次性的。网关注入令牌之后，下一步就会变成代刷新和写回 |
| 启动时把旧绑定改写成环回 URL | 用户选了不迁移。文件只在保存时变 |
| 继续用 v3 IPC 渲染新 Models 页 | `compat.rs` 只留给 Settings。新页的 DTO 不带明文密钥 |
| 本梯顺手改掉 Settings 的 App AI | 那是另一条产品。`get_providers_flat` 留到 Settings 不再调用它 |
| 局域网地址写进 Agent 文件 | magpie 的 `URL()` 在监听 `0.0.0.0` 时仍对 Agent 公布环回。34 档保持这一点 |
| 再做一套配额页、登录或 OAuth | Usage 已经拥有 |
| 移植库、会话扫描、WebDAV、TUI、托盘独立应用、`magpie web`、PostHog、图像库 MCP、keepalive | 用户要的是网关和 Agent 配置 |
| 移植 `usemagpie.ai` | 用户原话是更新源不要学 |
| 把 `docs/others/model-redesign/05-redesign-proposal.md` 当实施说明 | 那份提案写于 2026-08-15，并明确推迟了 proxy takeover |

## 防火墙

- 不改 `crates/skillstar-usage/src/fetchers/oauth/cursor.rs`。
- 不改 updater 三处与 `release.yml` 的签名 `latest.json` 流程。
- 不手改 `src/types/generated/`。DTO 变化后运行 `bun run types:gen`。
- 新 Rust 依赖用 `cargo add`，版本归一在根 `Cargo.toml`。
- 单个源文件接近 800 行就拆，不超过约 1000 行。
- 前端只通过 `invoke()` 和事件。远程 HTTP 只通过上面的两个客户端，遵守 `proxy.json`。
- 命令文件只做注册、DTO、State、错误和事件。网关域逻辑留在 `skillstar-gateway`。跨域编排留在 `skillstar-app`。
- 01 档起，`check_workspace_deps.sh` 禁止 `models → gateway`、`usage → gateway`、`gateway → models`、`gateway → usage`、`gateway → decision`、`gateway → app`、`core → gateway`、`models → decision`。正向断言网关的 skillstar 依赖只有 `skillstar-core`。04 档起正向断言 `skillstar-app` 依赖 `skillstar-gateway`，`src-tauri` 不直接依赖它。
- 测试不写真实 `$HOME`。
- 文档与代码同一档完成。行为写 `docs/features/models/README.md`。所有权写 `docs/boundaries.md` 与 `docs/architecture.md`。硬切换写 `docs/decisions.md` 的下一个编号。用户可见的 CLI 在 `README.md`。不手抄会变的数量；花名册以注册表和它的测试为准。

`compat.rs` 是留给 Settings 的短命缝。删除条件是 Settings 不再调用 `get_providers_flat`。那一次不在本梯。Models 页从 07 档起就不调用它。

## 视觉门

每一档视觉切片在接受前跑 screenshot-critique：批评者只看截图和该档的验收句，不看实现理由。有参照作物时再跑 compare-screenshots，裁剪范围只盖住该档声明的那一个变量。参照图放进 `specs/models-gateway/assets/magpie/`，从本机 magpie 截取。不从 `usemagpie.ai` 下载。

两个视口：1440×900 与 1280×800。下面五问有一问失败，该档就不算过：

1. 这一屏在让人做哪个决定？
2. 上一档的变量是否还在抢视觉？
3. 屏幕上有没有厂商密钥或厂商 URL？
4. 忽略鸟标和品牌色，哪一张更接近 magpie 的那一裁？
5. 第一下会点哪里？

人审是非阻塞的。用 preview-shots 打开截图，大约等 5 分钟。没有回复就按证据写下决定和理由，关掉窗口，继续下一档。

## 已知未知

- cursor、kiro、grok、antigravity、copilot 的签名头以 magpie 对应该文件的测试期望为准，在 17 档抄进夹具。测试里没有的头不加。
- Alma 的 `http://localhost:23001` 与 Hanako 的 `server-info.json` 以 magpie 的 `alma.go`、`hanako.go` 为准。对方没在跑时的行为已写在 21、22 档。
- Windows 的 WSL 探针只在 Windows 上跑。其他系统只跑路径拼写夹具。
- `ort` / Laya 与本网关无关，不要接进路由。

## 人可以怎么看

| 时候 | 看什么 |
| --- | --- |
| 现在 | 本文件和 [visualizations/ladder.html](visualizations/ladder.html) |
| 01–03 之后 | `cargo test -p skillstar-gateway` 的夹具差异 |
| 04 之后 | `skillstar gateway serve`，再对环回发一条夹具请求 |
| 07 之后 | 桌面应用的 Models 页：Agents、Providers、Gateway 三栏 |

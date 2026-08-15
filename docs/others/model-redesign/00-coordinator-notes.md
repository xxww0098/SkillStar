状态：research

# 协调者备注（给 T5 综合定案）

本文件由编排协调者撰写，不是调研产物。它记录三类信息：协调者**亲自复核过**的事实、四份调研之间的**交叉印证**、以及各份输入的**已知局限**。T5 应把第 1 节当作既定事实处理，不必重新论证；但仍需在方案里给出应对设计。

## 1. 协调者已复核的事实（可直接引用，勿再当作待验证传闻）

### 1.1 Codex `wire_api = "chat"` 已被上游删除，SkillStar 现在写出的是不可解析的配置

- Codex 侧：`/tmp/model-research/codex`（HEAD `da89849`，2026-08-14）的 `codex-rs/model-provider-info/src/lib.rs:61-65`，`pub enum WireApi` 只剩 `Responses` 一个变体（`#[default]`），`"chat"` 已不存在。
- SkillStar 侧：`crates/skillstar-models/src/providers/crud.rs:22-28` 的 `recommended_codex_defaults()` 对任何不含 `api.openai.com` 的 base URL 返回 `("chat", "third_party")`。
- 测试正在锁死错误行为：`crates/skillstar-models/src/providers/tests/part2.rs:124-141` 对 DeepSeek、Kimi、kimi-coding、MiniMax、GLM、OpenRouter、SiliconFlow、Grok 八个真实 provider 逐个断言 `wire == "chat"`。
- 后果：用户把第三方 Provider 绑到 Codex → 写出的 `config.toml` 含 `wire_api = "chat"` → 当前 Codex 反序列化失败 → **整个配置文件解析不了，Codex 起不来**。

**这不是改个字符串能了结的。** Codex 现在只支持 Responses API，因此不实现 `/v1/responses` 的第三方 Provider 从能力上就无法投影给 Codex。新数据模型必须能表达「某 Provider 支持哪些 wire protocol」，并让 UI 在绑定前就把不可能的组合挡掉或明确降级说明。方案需明确回答：这个能力位从哪来（模型目录？探测？preset 声明？），以及存量已写坏的用户配置怎么修复。

### 1.2 前端创建页绕过后端 preset 注册表，产出永远绑不上 Claude 的 Provider

- 后端注册表 `crates/skillstar-models/src/providers/presets.rs` 共 **14 条** `ProviderPresetFlat`（T4 文档记为 13，以 14 为准），其中 `deepseek` 的 `base_url_anthropic` 是 `https://api.deepseek.com/anthropic`（:118），`kimi` 是 `https://api.moonshot.cn/anthropic`（:135）。
- 前端 `src/features/models/components/hub/prototype/EditorPage.tsx:84-125` 自带一张 5 条的硬编码 `CREATE_PRESETS`，其中 deepseek / kimi / openrouter 的 `anthropic` 字段是**空串**。
- 后果：经该路径创建的 Provider 缺少 anthropic 端点，按激活校验规则永远绑不上 Claude CLI / Claude Desktop，且已污染存量 store。迁移方案必须处理存量脏数据，不能只修代码路径。

### 1.3 Claude 角色映射从不落盘（前后端断链，非后端缺失）

- 前端 `src/features/models/components/hub/prototype/matrix/rich/VariantB2b.tsx:44` 只有 `useState<Record<string, ClaudeMapState>>({})`。
- 后端早已就绪：`crates/skillstar-models/src/tool_sync/sync.rs:100-119` 读 `meta.claude_haiku_model` / `claude_sonnet_model` / `claude_opus_model` 并写入 `ANTHROPIC_DEFAULT_HAIKU_MODEL` / `SONNET` / `OPUS`。
- 即断链在前端，后端契约可直接复用。`docs/features/models/README.md` 已自认「Claude mapping UI 仍是前端本地状态」，属已知未完成，但对用户表现为「填了没用的表单」。

## 2. 四份调研之间的交叉印证（独立得出同一结论，可信度高）

1. **模型目录的三级回退**：T3 从 Crush 的 catwalk 推荐 `embedded → cache → remote`；T2 从 Kilo Code 独立推荐 `远端 models.dev（磁盘缓存 TTL 5min）→ 编译期快照 → 网络`，且含跨进程 Flock、原子 rename、坏缓存自愈。两路互不知情却撞车，且 T1 从 Chatbox 得出同构的「编译期快照 → 运行时刷新 → 手写默认」并强调**必须显式声明谁赢**。三路一致 ⇒ 建议在方案中作为高置信度结论采纳。
2. **模型引用必须是三元组**：T3 从写盘侧论证 `(provider, model, effort)` 缺一不可（否则 OMP / Crush / Codex 丢推理档位）；T2 从 Zed `LanguageModelSelection` 与 Kilo `Model.Ref{providerID,id,variant}` 独立得出同一形状，并额外给出「换模型时把参数**投影**到新模型能力集合而非清空」的处理法。SkillStar 现有 `OmpRoleTarget` 已是该形状 ⇒ 应提升为域内通用类型，而非 OMP 私有。
3. **preset 与用户存储必须分层**：T1 从 Cherry Studio 得出「preset 只放代码，用户存储只存 delta + null 继承」，正是 1.2 那个 bug 的结构性解法；T2 从 Kilo 得出「用户配置逐字段稀疏覆盖」。两者同构。
4. **T2 与 T4 对 OMP 角色的评价看似相反、实则一致**：T4 把「角色路由只有 OMP 有」列为能力缺失；T2 深入六个项目后判定 OMP 的设计属业界上游，避开了 Cline 字段复制、Roo 扁平前缀、Continue 枚举不同步、Void eager copy 四个陷阱。结论应是**推广而非重做**——把 `OmpRoleTarget` 泛化，并让每个 Agent 在 `tool_sync/agents.rs` 注册表里声明自己的角色清单（T2 建议分三档：无角色 / 单角色+兜底 / 多角色）。

## 3. 输入的已知局限（引用时请注意）

- **T1**：Jan 仓库 1.5 GB，只定向抓了 15 个关键文件，未做全仓 grep；其 UI 组件层细节未覆盖。
- **T2**：文档 1140 行，超出原定 400-800 行上限（为覆盖 6 项目 × 5 子节）。另发现 **Kilo Code 已不再是 Roo Code 的 fork**，当前 HEAD 已重构为 OpenCode 基座（`packages/{core,schema,opencode,tui,server}`，类型来自 `@opencode-ai/schema`）——引用 Kilo/Roo/OpenCode 的关系时勿用过时表述。
- **T3**：Pi 0.84.1 / OMP 17.3.2 的结论仅对当前版本有效；Codex `model_catalog_json` 的最小 JSON 未实测；Claude Code 接非 Anthropic 协议中转的实际成功率待验；Aider 已 3 个月无提交，是否还值得写盘属产品决策。
- **T4**：preset 计数误差已在 1.2 更正。其余字段级结论未逐条复核，T5 遇冲突时以自查源码为准并标注。
- **共同空白**：六个桌面客户端**没有一个**做余额查询、真正的多 key 轮询负载均衡（都只做 401/403/429 故障转移）、或写盘同步到第三方 Agent。这三块 SkillStar 没有外部先例，方案需自行设计并说明取舍。

## 4. 方案必须回答的问题（T4 提出，协调者确认必答）

1. 存量 preset 漂移（1.2 造成的脏数据）如何迁移？
2. Claude Desktop 列是未完成功能，还是应当移除？
3. 模型角色归 `provider.meta` 还是 `ToolBinding.settings`？（结合第 2 节第 2、4 条一并回答）

## 5. 授权范围

用户已明确授权：**数据模型可以推翻重来，最终要落到生产代码**。但 T5 本身只产出方案文档，不改 `src/` `crates/` `src-tauri/`。实施由后续工作包按 T5 的拆解派工。方案的实施拆解必须明确标出：Rust 类型变更与 `bun run types:gen` 的串行关系，以及哪些工作包会互相冲突不能并行。

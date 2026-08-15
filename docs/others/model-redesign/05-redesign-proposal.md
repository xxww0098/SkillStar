状态：proposal

# SkillStar Models 重设计方案（数据模型 + IA + 迁移 + 实施拆解）

> 本文是 T1–T4 四份调研的综合定案，目标是**可以照着改生产代码**。
> 输入：`01-desktop-clients.md`（桌面客户端）、`02-coding-agents.md`（编码 Agent）、
> `03-cli-config-and-catalog.md`（写盘格式与模型目录）、`04-skillstar-baseline.md`（现状基线）、
> `00-coordinator-notes.md`（协调者复核）。
> 本轮**未修改任何生产代码**（`src/` / `crates/` / `src-tauri/` 一律未动），只新增本文件。
> 撰写日期：2026-08-15。复核基线 commit：`00737df`。

---

## 0. 基线校正：我自己复核过的事实

本节只列**与输入文档不一致**或**输入文档未确认而本方案强依赖**的事实。其余事实以 04 为准。

### 0.1 基线文档有误 / 协调者备注有误

| # | 出处 | 原文 | 复核结果 |
| --- | --- | --- | --- |
| 1 | `00-coordinator-notes.md` §1.2 | 「后端注册表共 **14 条** `ProviderPresetFlat`（T4 文档记为 13，以 14 为准）」 | **协调者备注有误，14 是误数。** `grep -c 'ProviderPresetFlat {' presets.rs` 返回 14，但其中一条是第 79 行的 **struct 定义本身**。实际字面量在 113/130/144/158/175/189/203/220/238/252/269/283/298 共 **13 条**，且 `providers/tests/part1.rs:250` 的 `assert_eq!(presets.len(), 13)` 当前是绿的。**T4 的 13 是对的。** 本方案按 13 条计算迁移面。 |
| 2 | `04-skillstar-baseline.md` §2.2 | 「`scripts/internal/check_generated_types.sh` … Models 类型不在生成范围内」 | **结论正确，但脚本注释与之矛盾，需一并修。** 脚本第 12–13 行写「`crates/skillstar-models/src/providers/types.rs`（ProviderPreset）」携带 `#[derive(TS)]`，实际 `grep 'derive(.*TS\|ts(export)' providers/types.rs` 无命中，`src/types/generated/` 里也没有任何 Provider 类型。**脚本注释是过期的导航说明**（其头部自称 "a navigation aid, not an SSOT"），WP-1 要顺手改掉。 |
| 3 | `04-skillstar-baseline.md` §5.11 | 未提及仓库已有 DTO 投影决策 | **`docs/decisions.md` D-034（2026-08-14）已经为「Rust 类型怎么上生成面」定了规矩**：纯数据无行为的类型直接 derive `TS`，有自己重构节奏的域类型走 `skillstar-app` 的 DTO 投影层。本方案的 §5 WP-1 必须遵守 D-034，而不是简单地给 `providers/types.rs` 全部加 `#[derive(TS)]`。同时 D-034 明确记录了 `u64` → `bigint` 的坑：64 位字段必须标 `#[ts(type = "number")]`。 |

### 0.2 已复核为真、本方案强依赖的事实

- `recommended_codex_defaults()`（`crates/skillstar-models/src/providers/crud.rs:22-28`）对任何不含 `api.openai.com` 的 base URL 返回 `("chat", "third_party")`，且**函数 doc comment 明确写着这是有意为之**（"third-party OpenAI-compatible endpoints only implement `/v1/chat/completions`"）。这条注释在 Codex ≥ 0.95 之后已经变成错误陈述。
- `ProviderEntryFlat` 共 17 个字段，其中 `codex_wire_api` / `codex_auth_mode` 是 Codex 独占（`providers/types.rs:124-166`）。
- 前端 `CREATE_PRESETS`（`src/features/models/components/hub/prototype/EditorPage.tsx:84-125`）确实只有 5 条，且 `deepseek` / `kimi` / `openrouter` 的 `anthropic` 是空串；而 Rust preset 里 deepseek 是 `https://api.deepseek.com/anthropic`、kimi 是 `https://api.moonshot.cn/anthropic`。**§5.5 的生产 bug 属实。**
- Claude 角色映射确实只有前端 `useState`（`matrix/rich/VariantB2b.tsx:44`），后端 `sync.rs` 的 `managed_fields` 已经在读 `meta.claude_haiku_model` / `claude_sonnet_model` / `claude_opus_model` 并写 `ANTHROPIC_DEFAULT_{HAIKU,SONNET,OPUS}_MODEL`。**断链在前端，后端契约可直接复用。**
- `OmpRoleTarget { provider_id, model, thinking }` 的 doc comment 已经写明「存 SkillStar provider id 而非磁盘 key，键在写盘时现算」，且明确说明「角色可以指向任意已绑定 provider，所以挂在 `ToolBinding::settings` 而不是单条 entry」。**这份设计意图就是本方案要泛化的东西。**
- `ts-rs` 已在 `skillstar-models` / `skillstar-app` / `skillstar-usage` / `skillstar-marketplace` / `skillstar` 五个 crate 的 `Cargo.toml` 里，`bun run types:gen` = `cargo test -p … export_bindings`。**接生成面不需要新增依赖，只需要加 derive。**
- 所有远程 HTTP 已统一走 `skillstar_core::infra::http_client::probe_http_client`（`crates/skillstar-models/src/diagnostics.rs:11,57,176,216`）。**新增的 models.dev 拉取必须走同一入口。**
- `docs/decisions.md` D-012 已给出「什么时候值得抽象写盘骨架」的判据（出现第二个同类 Agent 时才抽）；D-025 已给出 binding 级设置袋的完整背景。**这两条不推翻，本方案在其上继续。**

---

## 1. 问题陈述与设计目标

### 1.1 一句话问题陈述

SkillStar 后端已经有一个干净的 flat store 与表驱动 Agent 注册表，但这套模型是**围绕「一个 provider 一个 URL 一个模型」的 v1 心智增量长出来的**：
它无法表达「这家 Provider 支持哪种线路协议」（于是给 Codex 写出起不来的配置）、
无法表达「模型是什么样的」（于是无法投影到 Crush/OpenCode 需要的价格与上下文字段）、
把「角色路由」这个跨 Agent 的一等概念锁在 OMP 一家（同一需求在 Claude 侧走 `provider.meta`，两套存储）、
并且前端生产主界面是一份原型代码，它绕过后端 preset 注册表、不持久化 Claude 角色映射、把 multi-provider 语义做错了三列。

### 1.2 设计目标（每条可证伪）

| # | 目标 | 证伪方法 |
| --- | --- | --- |
| **G1** | **新增一个 Agent 只需改 2 处：`AGENT_SPECS` 一行 + 一个 writer 模块。** 前端的 toolId union、列定义、`CONFIG_FILE_TOOLS` 全部由生成类型或运行时命令派生。 | 加一个假 Agent，统计改动文件数；`grep -c '"claude-desktop"\|"omp"'` 在 `src/features/models/` 下应为 0（除生成类型外）。当前基线是 11 处（04 §5.4）。 |
| **G2** | **写盘目标能力位是数据，不是猜测。** 「这家 Provider 能不能接 Codex」由 `Provider.caps.responses_api` 回答，UI 在绑定前就挡住不可能的组合。 | 新建一个只有 `/v1/chat/completions` 的 provider，Codex 列必须是禁用态并给出理由；`grep -rn 'wire_api = "chat"' crates/` 应为 0。 |
| **G3** | **角色路由是跨 Agent 的一等概念。** Claude 的 haiku/sonnet/opus、OMP 的 10 个角色、OpenCode 的 `small_model` 用同一份 `BTreeMap<RoleId, ModelRef>` 表达，每个 Agent 在注册表里声明自己支持哪些角色。 | `provider.meta` 里不再有 `claude_*_model` 键；`OmpRoleTarget` 这个类型名在 `tool_sync/` 之外不存在；Claude 列的角色面板与 OMP 列复用同一个 React 组件。 |
| **G4** | **模型元数据足够写出一份合法的 Crush 配置。** Crush 的 `Model` schema 有 10 个 required 字段（含 4 个价格 + context + max_tokens），是所有目标里最严格的验收者。 | 一个 `#[test] crush_projection_satisfies_all_required_fields`，用真实 preset 数据生成 JSON 并对 `schema.json` 校验。 |
| **G5** | **跨 IPC 的类型零手抄。** Provider / Model / Binding / Role / Preset 全部由 ts-rs 生成或由 `skillstar-app` 的 DTO 投影生成，`src/types/models.ts` 退化为 re-export barrel。 | `check_generated_types.sh` 覆盖这些类型；`src/types/models.ts` 里的 `interface`/`type` 声明数从 14 降到 ≤3（只留前端自有的表单能力声明）。 |
| **G6** | **store 损坏不等于用户配置消失。** 解析失败保留原文件并报错，而不是静默返回空 store 再被覆盖写。 | 手工写坏 `model_providers.json` 的一个字节，重启 SkillStar：必须看到明确错误 + 文件仍在，而不是「所有 provider 都没了」+ 文件被两个 Official 种子覆盖。 |
| **G7** | **模型目录三级回退，离线可用、联网更新、失败静默降级。** L0 编译期快照 → L1 models.dev（带 ETag 的磁盘缓存）→ L2 provider 自己的 `/v1models`。 | 断网启动：模型选择器仍有 context window 与价格；`meta.model_catalog` 不再出现在 `model_providers.json` 里（catalog 移出 provider 行）。 |
| **G8** | **前端生产代码不在名为 `prototype` 的目录里，且死代码有门禁。** | `src/features/models/components/hub/prototype/` 不存在；一个 `check_ts_orphan_modules.sh` 通过。 |

### 1.3 非目标（本轮明确不做）

1. **不做真正的多 key 轮询负载均衡。** 六个桌面客户端没有一个做（01 §7.1 第 13 条），Jan 的 primary + fallbacks 故障转移链是够用的形状。本方案只在数据模型上为多 key 留位（`Credential::ApiKey { keys: Vec<ApiKeyEntry> }`），不实现调度。
2. **不做 Cherry Studio 式的 provider-registry 生成管线**（独立包 + 双向 CI 校验 + canonical id 归一化）。01 §1.4 已判定「SkillStar 现阶段抄这套是明显过度」。
3. **不做可切换的 profile 树**（Roo 的 `apiConfigs` + `currentApiConfigName`）。02 §8.2 B1 的建议是先做导入/导出 + 命名快照。本方案连快照也不做，只保证数据模型不阻塞它。
4. **不新增 Agent。** Crush / Aider 在本方案里只作为**投影验收测试的目标**（G4），不进 `AGENT_SPECS`。Aider 已 3 个月无提交（03 §8.5），写盘价值待定。
5. **不改余额查询与延迟探测的现有实现。** 它们与本次重设计正交，只需要跟着 `Provider` 类型改字段引用。
6. **不做 SQLite。** OMP 用 `models.db` 缓存模型是它的选择；SkillStar 的 catalog 缓存用独立 JSON 文件即可，不引入新的存储引擎。
7. **不动 `crates/skillstar-usage/src/fetchers/oauth/cursor.rs`**（AGENTS.md 硬性约束）。

---

## 2. 新数据模型

### 2.1 三个候选方案

#### 候选 A｜就地改良（保留 `ProviderEntryFlat` 单表 + 类型标签）

保持一张 provider 表，只做四件事：给 `ProviderEntryFlat` 加 `kind: ProviderKind`（`ThirdParty | VendorOfficial | NativeLogin`）区分 Official 种子；把 `codex_wire_api` / `codex_auth_mode` 挪进 `BindingEntry.settings`；给 `meta` 里的 `claude_*_model` 一个具名 struct；catalog 保持在 `meta` 里但加体积上限。

- **取舍（+）**：迁移面最小，v3→v4 只动两个字段；现有 2352 行 provider 测试大部分不动；一周内可落地。
- **取舍（−）**：**解决不了 G2 和 G4**。`base_url_openai` 一个字段无法同时表达「支持 chat」和「支持 responses」两件事，于是 Codex 的能力位无处安放；`ModelCatalogEntry` 仍是扁平结构，写不出 Crush 需要的 10 个字段。G3 也只能半解决（角色仍在无 schema 的 `Value` 袋里）。
- **判定**：**否决。** 它把本次重设计的两个最硬的外部约束（Codex 只剩 Responses、Crush 的 10 个 required 字段）留在原地。用户已授权推翻重来，没有理由选一个明知解决不了主要问题的方案。

#### 候选 B｜四层分离：Catalog / Provider / Credential / Binding

把今天挤在一张表里的四件事拆开：

```
Catalog（不可变事实：模型是什么样的）        —— 独立文件，三级来源
   ↑ 引用
Provider（连接：端点 + 能力位 + 采纳的模型清单）—— model_providers.json
   ↑ 引用
Credential（凭据：判别联合，可以是 None）      —— 内嵌 Provider，但 DTO 层剥离明文
   ↑ 引用
AgentBinding（编排：entries + active 指针 + roles）—— model_providers.json
```

- **取舍（+）**：每一层都能独立回答一类问题，四个硬约束（G2 能力位 / G3 角色 / G4 目录 / G6 健壮性）各有落点；Credential 判别联合直接对上 Codex 的 `env_key`、OMP 的 `!cmd`、Claude 的 `apiKeyHelper`、OpenCode 的 `{env:}`/`{file:}` 四种语义（03 §6.1）；catalog 移出 provider 行同时解决 04 §5.12 的体积问题。
- **取舍（−）**：需要一次 v3→v4 迁移；`providers/tests/` 的 5 个 part 要大改；层间引用完整性（角色指向已删 provider、模型 id 不在 catalog）需要显式的悬空防护，而这在单表里是「不可能发生」。
- **判定**：**推荐骨架。**

#### 候选 C｜完全采用 models.dev 目录 + 本地覆盖层

不自己定义模型元数据，直接把 models.dev 的 `api.json` 当作 catalog 的唯一 schema（Kilo/OpenCode 的做法，02 §5.3），用户配置只做逐字段稀疏覆盖。

- **取舍（+）**：schema 免费、每小时更新、6372 条 provider-model、`reasoning_options` 结构化；02 §5.3 证明了「远端目录 → 快照 → 用户逐字段覆盖」这条链在真实项目里跑得通。
- **取舍（−）**：**单独不成立，有三个致命缺口。**
  1. models.dev 收录的是**公开 Provider**，SkillStar 的核心用户场景是**中转站**——用户填的 base URL 大概率不在 186 个 provider 目录里，模型 id 也常被改名。02 §5.4 坑③已经点明：「只要依赖远端目录，就一定有一批模型需要在代码里打补丁」（Kilo 的 GLM-5.2 硬编码）。
  2. Native Official（`claude-official` / `codex-official`）在 models.dev 里没有对应条目——它们不是 API endpoint。
  3. 3.71 MB 的 `api.json` 不能全量内置，而只内置子集又要自己定义「子集是什么」，等于还是要有一层自己的 schema。
- **判定**：**采纳为 catalog 层的数据来源与 schema 蓝本，不作为整体架构。**

### 2.2 推荐：B 为骨架，C 为 catalog 层的填充策略

**明确推荐 = 候选 B ∘ 候选 C。**

理由按重要性排序：

1. **只有 B 能同时满足 G2 与 G4。** G2 要求 Provider 能声明「支持哪些线路协议」，这必然把单一 `base_url_openai` 拆成 `Endpoints{openai_chat, openai_responses, anthropic_messages}`；G4 要求模型元数据分「模型本身的事实」与「这家怎么卖它」两层（03 §4.1 的 `base_model` 继承），这必然把 catalog 从 provider 行里拿出来。两件事都是**结构性的**，不是加字段能解决的。
2. **C 提供了 B 里 catalog 层的免费 schema。** `ModelFacts` / `Serving` / `Cost` / `Reasoning` 直接照抄 models.dev 的 Zod schema（03 §4.1），字段名保持一致，于是投影到 OpenCode 时几乎是逐字段映射（03 §7.2 已论证「OpenCode 的 Model schema 与 models.dev 一一对应，因为 OpenCode 就是 models.dev 的主要消费方」）。
3. **三份独立调研撞车的结论必须采纳。** T3 从 Crush 的 catwalk、T2 从 Kilo 的 `models-dev.ts`、T1 从 Chatbox 的 `snapshot.generated.ts` 各自推出「内置快照 + 运行时刷新 + 缓存」三级结构（00 §2.1）。三路互不知情却同构，这是本次调研里置信度最高的结论。
4. **A 的低迁移成本是假的省。** A 之后仍然要做 B 的拆分（因为 Codex 与 Crush 的约束不会消失），届时是**两次**迁移，且第二次要迁移的是已经被 A 改过一遍的数据。

### 2.3 字段级 Rust 类型定义

以下是 v4 的类型草案。文件归属：域类型进 `crates/skillstar-models/src/providers/`（拆成 `provider.rs` / `credential.rs` / `binding.rs` / `catalog.rs` 四个模块，各自 < 400 行）；跨域 DTO 投影按 D-034 进 `crates/skillstar-app/src/models/dto.rs`。

#### 2.3.1 Provider —— 一个可写盘的连接

```rust
/// 一个 Provider = 一个可写盘的连接目标。
///
/// 不变量（照抄 Cherry Studio 的做法，01 §1.1）：
///   一个 Provider 实例 = 一个 API host。同一家开了两个 host（官方 + 中转）
///   就是两行，靠 `preset_id` 关联、UI 折叠成组，而不是在一行里塞两个 URL。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    /// 稳定 id。第三方 = UUIDv4；Native Official = 固定 slug（`claude-official`）。
    /// 【为什么】01 §7.3 的第 1、2 条反面教材：Open WebUI 用位置下标、Jan 用名字，
    /// 前者删除即重排、后者无法重命名。id 一旦落盘永不变。
    pub id: String,

    /// 展示名，用户可改。
    pub name: String,

    /// 指向 preset 注册表。`None` = 完全自定义。
    /// 【为什么】01 §7.1 第 1 条：preset 只在代码里，用户存储只存 overlay。
    /// preset 提供的是**默认值**，不是不可覆盖的事实（02 §8.1 A10）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,

    /// 这家提供哪些协议端点。取代 v3 的 `base_url_openai` / `base_url_anthropic`。
    pub endpoints: Endpoints,

    /// 凭据。判别联合，可以是 `None`（Native Official / 本地服务）。
    pub credential: Credential,

    /// 自定义请求头。投影到 OpenCode `options.headers` / Codex `http_headers` /
    /// Crush `extra_headers` / Claude `ANTHROPIC_CUSTOM_HEADERS`（换行分隔！）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,

    /// 能力位。回答「这家能不能接 Codex / 能不能接 Claude Code」。
    /// 【为什么】03 §0 第 1 条：Codex ≥0.95 只支持 Responses API，
    /// 不实现 `/v1/responses` 的 Provider 从能力上就无法投影给 Codex。
    #[serde(default)]
    pub caps: ProviderCaps,

    /// 用户**采纳**的模型 id 清单（白名单）。不是「这家有哪些模型」。
    /// 【为什么】01 §7.2 第 3 条：中转站动辄返回 300 个模型，
    /// 不能把发现结果直接灌进选择器。发现结果在 catalog 缓存里，采纳的才在这里。
    #[serde(default)]
    pub models: Vec<String>,

    /// 绑定时的兜底模型。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    #[serde(default)]
    pub sort_index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// 创建时间，**毫秒**。
    /// 【为什么显式命名】v3 的 `created_at` 是毫秒而 `last_sync_at` 是秒（04 §5.14 第 7 条），
    /// 同一个 store 内两种单位。v4 全部字段带 `_ms` 后缀，单位进名字。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_ms: Option<u64>,

    /// 唯一的无 schema 口袋。v3 的 `meta` 有 5 类用途且无集中定义（04 §1.2）；
    /// v4 把其中 4 类提升为具名字段，只留下真正无法预知的扩展。
    /// 【门禁】一个 `#[test] ext_keys_are_documented` 断言实际出现的 key 在白名单内。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// 一个 Provider 可能同时暴露多种协议端点。
///
/// 【为什么不是一个 URL + 一个枚举】同一个中转站常常同时开
/// `/v1/chat/completions` 和 `/anthropic`（deepseek preset 就是这样：
/// `base_url_openai = .../v1`，`base_url_anthropic = .../anthropic`）。
/// 协议不是 Provider 的属性，是端点的属性。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Endpoints {
    /// `/v1/chat/completions` —— OpenCode / Pi / OMP / Crush 的入口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_chat: Option<String>,
    /// `/v1/responses` —— **Codex ≥0.95 的唯一入口**（03 §2.2 坑 1）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_responses: Option<String>,
    /// `/v1/messages` —— Claude Code 的唯一入口。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_messages: Option<String>,
    /// 模型发现端点（L2）。v3 的 `models_url`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub models_list: Option<String>,
}

/// 三态能力位。`Unknown` 是初始值，探测后变 `Yes`/`No`。
/// 【为什么三态】v3 用「字段等于 serde 默认值」推断 Codex 默认（`crud.rs:88-93`），
/// 于是「用户显式选了 responses」与「用户没选」不可区分（04 §5.14 第 8 条）。
/// 三态把「不知道」变成一个可表达的值。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Tri { #[default] Unknown, Yes, No }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProviderCaps {
    /// 支持 `/v1/responses`。决定 Codex 列是否可绑。
    #[serde(default)] pub responses_api: Tri,
    /// 支持 Anthropic Messages 协议。决定 Claude Code 列是否可绑。
    /// 【注意】03 §2.3 记录了官方立场：「Anthropic doesn't support routing
    /// Claude Code to non-Claude models through any gateway」。所以这一位
    /// **必须来自探测而不是假设**，UI 要说清楚这是文档外用法。
    #[serde(default)] pub anthropic_messages: Tri,
    /// `/v1/models` 可用。决定「拉取模型」按钮是否可点。
    #[serde(default)] pub models_list: Tri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probed_at_ms: Option<u64>,
}
```

**能力位从哪来（三级，与 catalog 的三级正交）**：

| 级 | 来源 | 何时写 | 覆盖谁 |
| --- | --- | --- | --- |
| P0 | preset 声明（`ProviderPreset.caps`） | 从 preset 创建时 | 初值 |
| P1 | 端点探测（`HEAD`/`OPTIONS` 或一次最小请求） | 用户点「检测连接」、保存 URL 后台探测 | 覆盖 P0 |
| P2 | 用户手动覆盖（高级区） | 用户显式勾选 | 覆盖 P1，并标记来源 |

UI 必须显示当前值来自哪一级（02 §8.1 A8：「把来源显示在 UI 上」）。

#### 2.3.2 Credential —— 凭据的判别联合

```rust
/// 凭据。
///
/// 【为什么是判别联合而不是「字符串 + auth_mode 枚举」】
/// 03 §6.1 列出四种**语义不同**的凭据通道：Codex 的 `env_key` 存的是**变量名**、
/// OMP 的 `apiKey` 先当变量名查再当字面值、OpenCode 支持 `{env:}`/`{file:}` 插值、
/// Claude 的 `apiKeyHelper` 是一条命令。v3 的 `api_key: String` +
/// `codex_auth_mode: String` 只能表达其中一种，且 auth_mode 是 Codex 专属的。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Credential {
    /// 无凭据。Native Official（浏览器登录）、本地服务（ollama）。
    /// 【为什么不用 Option<Credential>】01 §7.1 第 6 条：Cherry Studio 把
    /// `authOptional`（本地服务，仍需 baseUrl）与「登录型」（连 host 输入都不显示）
    /// 刻意区分。`None` 变体让 UI 能问「为什么没有 key」并给出不同答案。
    None { reason: NoCredentialReason },

    /// 明文 key。可以有多把，语义是**故障转移链**（primary + fallbacks），不是轮询。
    /// 【为什么】01 §7.1 第 13 条：六个桌面客户端没有一个实现轮询负载均衡，
    /// 都只做 401/403/429 故障转移。这是个信号。
    ApiKey { keys: Vec<ApiKeyEntry> },

    /// 环境变量名。Codex `env_key` / OpenCode `{env:NAME}` / Crush `$NAME`。
    EnvVar { name: String },

    /// 文件路径。OpenCode `{file:path}`。
    File { path: String },

    /// 命令。Codex `auth.command` / OMP `!cmd` / Claude `apiKeyHelper`。
    Command { command: String, args: Vec<String> },

    /// 外部 CLI 已登录（凭据在别人的 store 里，SkillStar 不持有）。
    /// 【为什么单独一档】01 §7.1 第 6 条：Cherry Studio 的 `'external-cli'`。
    /// Claude Official / Codex Official 就是这一档——SkillStar 的同步动作是
    /// **清空自己写的托管字段**，把控制权还给 CLI 自己的登录态。
    ExternalCli { surface: &'static str },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoCredentialReason { LocalService, NativeLogin }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiKeyEntry {
    /// 稳定 id。编辑时按值精确匹配 → 按位置复用 → 最后才发新 id
    /// （01 §1.2 `toApiKeyEntries` 的做法），避免改一个字符就让所有条目换 id。
    pub id: String,
    /// 明文。**永远不进前端 DTO**（见 2.3.6）。
    pub secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
```

#### 2.3.3 Catalog —— 模型是什么样的

照抄 models.dev 的两层拆分（03 §4.1）。**不进 `model_providers.json`**，独立文件。

```rust
/// 「这家 Provider 怎么卖这个模型」。catalog 的行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelEntry {
    /// Provider 侧模型 id。写盘用这个。
    pub id: String,
    pub display_name: String,
    /// 指向共享的 `ModelFacts`（models.dev 的 `base_model`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_model: Option<String>,
    /// 客观状态（alpha/beta/deprecated/active）。
    /// 【为什么与 enabled 分开】02 §8.2 B5：`status` 是客观事实、`enabled` 是用户意图，
    /// 分开才能支撑「隐藏废弃模型但保留已选中的」（02 §8.1 A7）。
    #[serde(default)] pub status: ModelStatus,
    pub serving: Serving,
    pub facts: ModelFacts,
    /// 数据来源，用于 UI 显示「这条元数据是哪来的」。
    #[serde(default)] pub source: CatalogSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CatalogSource {
    /// L0 编译期快照
    Snapshot,
    /// L1 models.dev 运行时
    Registry,
    /// L2 provider 的 /v1/models（只有 id 和名字，无价格无上下文）
    #[default] Discovered,
    /// 用户手填 / 高级覆盖
    UserOverride,
}

/// 「这家怎么卖」——价格与限额。Crush 的 10 个 required 字段全部落在这里。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Serving {
    /// 必填 —— Crush required、OpenCode 需要（否则算不出剩余上下文，03 §2.1）。
    pub context: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input: Option<u64>,
    /// 必填 —— Crush required（`default_max_tokens`）。
    pub max_output: u64,
    /// 必填 —— Crush 的 4 个价格字段。单位 **USD / 1M token**（与 models.dev 一致）。
    /// 【坑】Aider 用 USD/token，差 1e6，必须有单位换算测试（03 §7.2）。
    pub cost: Cost,
    /// 写这个模型用哪种线路。
    pub wire: WireShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireShape { OpenaiChat, OpenaiResponses, AnthropicMessages }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cache_read: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub cache_write: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub reasoning: Option<f64>,
    /// 分层定价。Pi 用 `inputTokensAbove` 键，本类型用 `above_input_tokens`，
    /// 差异在 writer 里处理（03 §7.2 Pi 段）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub tiers: Vec<CostTier>,
}

/// 「模型本身的事实」——与在哪买无关。models.dev 的 `models/<vendor>/<model>.toml`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelFacts {
    #[serde(default, skip_serializing_if = "Option::is_none")] pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub knowledge_cutoff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub release_date: Option<String>,
    #[serde(default)] pub modalities_in: Vec<Modality>,   // text|image|audio|video|pdf
    #[serde(default)] pub modalities_out: Vec<Modality>,
    #[serde(default)] pub tool_call: bool,
    #[serde(default)] pub attachment: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub structured_output: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub temperature: Option<bool>,
    #[serde(default)] pub reasoning: Reasoning,
}

/// 推理能力。**结构化，不是布尔，也不是全局枚举。**
///
/// 【为什么】04 §5.14 + 02 §9.3 缺口 2：`OMP_THINKING_LEVELS` 是全局 9 元枚举，
/// 所有模型都显示 9 个选项。实际上 Anthropic 系是 budget（token 数）、
/// OpenAI 系是 effort（枚举）、很多模型完全不支持。
/// 【业界正解】Void 的 `reasoningSlider: {type:'budget_slider'|'effort_slider'}`
/// —— 能力元数据**直接声明该渲染哪种控件**（02 §8.1 A9）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reasoning {
    #[default] None,
    /// 只能开/关。
    Toggle { can_disable: bool },
    /// 枚举档位。取值域是 models.dev 的 `ReasoningEffortValue` 超集。
    Effort { values: Vec<Effort>, default: Option<Effort>, can_disable: bool },
    /// token 预算。
    BudgetTokens { min: Option<u32>, max: Option<u32>, default: Option<u32> },
}

/// 规范推理档位。取 models.dev 的枚举作为**内部规范值**（最宽的超集），
/// 投影时按目标收窄：Crush 只认 low|medium|high（向下取最近值），
/// Codex 是自由字符串，OMP 可任意重映射（03 §5.3）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Effort { None, Minimal, Low, Medium, High, Xhigh, Max }
```

**「关闭思考」与「思考强度 = none」是两件事**（02 §2.2 引 Roo 的 `ThinkingBudget.tsx` 规范）：
前者请求里完全省略 reasoning 段（`ModelRef.effort = None` 且 `reasoning_enabled = false`），
后者带 `reasoning = "none"`（`effort = Some(Effort::None)`）。本方案用 `Option<Effort>` +
`Effort::None` 两级表达，与 Roo 的规范同构。

#### 2.3.4 ModelRef 与角色路由

```rust
/// 唯一的模型引用形状。**域内通用**，不是 OMP 私有。
///
/// 【为什么是三元组】03 §5.2 观察 4：OMP 的 `provider/model:level`、
/// Crush 的 `{provider, model, reasoning_effort}`、Codex 的
/// `model` + `model_provider` + `model_reasoning_effort` —— 三家传的是同样三个东西。
/// 任何只传 `provider/model` 的抽象都会在写盘时丢掉档位。
/// 【业界对照】Zed 的 `LanguageModelSelection`、Kilo 的 `Model.Ref{providerID,id,variant}`。
/// 【现状】`OmpRoleTarget{provider_id, model, thinking}` 已经是这个形状，只是被关在 OMP 里。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelRef {
    /// SkillStar 内部 provider id，**不是磁盘上的 `skillstar_*` 键**。
    /// 键在写盘时由 `skillstar_managed_key` 现算（D-025 已确立的规则，保持）。
    pub provider_id: String,
    pub model: String,
    /// 规范推理档位。投影时按目标模型的 `Reasoning` 能力裁剪。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Agent 特有的额外参数（OMP 的 `thinking` 已被 `effort` 取代；
    /// Claude 可能是 `fallback_model`）。稀疏袋，typed accessor 读。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

/// Agent 的全部绑定。取代 v3 的 `ToolBinding`。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgentBinding {
    pub entries: Vec<BindingEntry>,
    /// active 指针。越界靠 clamp 兜底（保持 v3 行为）。
    #[serde(default)] pub active_index: usize,

    /// 角色路由。**从 `settings` 袋提升为一等字段。**
    /// 键是规范角色 id（见 2.6），开放 map 而非枚举。
    /// 【为什么开放 map】02 §9.2：Continue 的 `z.enum` 导致加角色改三处且
    /// `summarize` 已不同步；Zed 的扁平字段要改五处。SkillStar 的开放 map 避开了两个坑。
    /// 【为什么提升为一等字段】04 §5.3：Claude 的层级模型走 `provider.meta`、
    /// OMP 的角色走 `binding.settings`，同一概念两套存储。二选一的答案是 binding 级。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, ModelRef>,

    /// Agent 私有的**非角色**配置。Codex 的 profile 开关、OpenCode 的 `small_model`
    /// 之外的东西。typed accessor 读（`CodexSettings::from_binding`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BindingEntry {
    pub provider_id: String,
    pub model: String,
    /// per-entry 设置。Codex 的 wire/auth 从 Provider 行搬到这里。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
    /// **毫秒**（v3 是秒，见 2.3.1 的单位说明）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at_ms: Option<u64>,
}
```

#### 2.3.5 Store

```rust
pub const STORE_VERSION: u32 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvidersStoreV4 {
    pub version: u32,
    pub providers: Vec<Provider>,
    /// 键是 Agent id。**改名**：v3 叫 `tool_activations` 但值是 `ToolBinding`
    /// （04 §5.1：键名说 activation，类型说 binding）。v4 叫 `bindings`。
    #[serde(default)]
    pub bindings: HashMap<String, AgentBinding>,
}
```

#### 2.3.6 前端 DTO（按 D-034）

| 类型 | 上生成面的方式 | 理由 |
| --- | --- | --- |
| `Endpoints` / `ProviderCaps` / `Tri` / `ModelRef` / `Effort` / `Reasoning` / `Cost` / `Serving` / `ModelFacts` / `ModelStatus` / `CatalogSource` / `Modality` / `WireShape` | **直接 `#[derive(TS)]`** | 纯数据无行为，本来就是 wire 形状（D-034 的第一类）。`u64` 字段全部标 `#[ts(type = "number")]`。 |
| `Provider` → `ProviderDto` | **投影**，定义在 `skillstar-app/src/models/dto.rs` | `credential` 必须被剥离：DTO 只带 `credential_kind: CredentialKind` + `credential_summary: String`（掩码或变量名）+ `has_secret: bool`。**明文永不进渲染进程**（01 §7.1 第 4 条，Cherry Studio 的 `RuntimeApiKeySchema`）。`impl From<Provider> for ProviderDto` 用**完全解构**，上游加字段就编译错误（D-034 的后果条款）。 |
| `AgentBinding` → `AgentBindingDto` | 直接 derive（无秘密） | 纯数据。 |
| `ProviderPreset` | 直接 derive | 纯数据，且已经是命令返回值。**顺手修掉 04 §5.5**：前端不再有 `CREATE_PRESETS`。 |
| `AgentSpec` → `AgentDescriptorDto` | **投影**（含函数指针，不能 derive） | `{ id, display_name, kind, required_wire, roles: Vec<RoleDefDto>, config_files: Vec<...> }`。**这是 G1 的关键**：前端的 toolId union、列定义、`CONFIG_FILE_TOOLS` 全部由这一个命令派生。 |

`src/types/models.ts`（248 行、14 个手抄类型）退化为 re-export barrel，只保留没有 Rust 对应物的前端自有物。

### 2.4 概念冗余的逐条裁决

回答 04 §5.1 的每一条。

| v3 概念 | 裁决 | v4 形态 | 理由 |
| --- | --- | --- | --- |
| `tool_activations`（字段名） | **改名** | `bindings` | 键名说 activation，值是 binding，错位了三个版本。改名需要 v3→v4 迁移，而本方案本来就要迁移，成本为零。 |
| `ToolActivation`（类型名） | **改名** | `BindingEntry` | 它的 doc 自己说 "One provider+model binding entry"。 |
| `ToolBinding` | **改名 + 加字段** | `AgentBinding`（`entries` / `active_index` / **`roles`** / `settings`） | `roles` 从设置袋提升为一等字段。 |
| `activate_tool` | **拆成三个命令** | `bind_provider(agent, provider, model?)` / `set_active_binding` / `update_binding_entry` | v3 的 `activate_tool` 既做新增又做切换（`crud.rs:376-391`），而 `set_active_binding` 已经存在但生产不可达。拆开后每个命令一件事。 |
| `deactivate_tool` | **拆成两个** | `unbind_provider(agent, provider)`（摘一条）/ `unbind_agent(agent)`（清空全部） | v3 的 `deactivate_tool` 不是 `activate_tool` 的逆，它清空**全部** entry（04 §5.7 的生产 bug 根源：三个 multi 列的解绑按钮调的是它）。 |
| `preset` 与 `official seed` 混在同一张表 | **拆 category** | `ProviderPreset.category: PresetCategory{Domestic, Relay, VendorOfficial, NativeLogin}` | v3 的 `category: "official"` 里混了 Native Official 种子（无 key、空端点）和 Grok（有 key 的官方厂商），靠 id 白名单区分（`presets.rs:266-311` 的注释自己承认）。`is_native_official_preset_id` 的 `matches!` 白名单随之删除。 |
| `ProviderIdentity`（第 4 套 id） | **保留，收窄职责** | 仍在 `skillstar-providers`，只负责 catalog_id ↔ preset_ids 的粒度不对称映射 | 04 §1.1 说明这张表存在的全部理由就是粒度不对称（glm → glm + glm-coding）。这个不对称是真实的，不能消除。但 `claude-official` / `codex-official` 的 preset_ids 条目要跟着 §2.7 的 NativeLogin 改动更新。 |
| `ai_provider` 的 `app_id`（第 5 套 id，只认 `"claude"`/`"codex"`） | **合并进 Agent id 空间** | `AiProviderRef { agent_id: String, provider_id: String }`，`agent_id` 取 `AGENT_SPECS` 的 id | 04 §5.14 第 10 条。迁移 `ai.json`：`"claude"` → `"claude-code"`，`"codex"` → `"codex"`。 |
| `codex_wire_api`（Provider 行字段） | **删除** | 由 `Endpoints.openai_responses` 是否存在取代 | 03 §7.1：真正的问题从来不是「我要告诉 Codex 用哪个协议」，而是「这家支不支持 Codex 要求的协议」。Codex 只剩 `responses`，这个字段编码的是一个已经消失的选择。 |
| `codex_auth_mode`（Provider 行字段） | **搬到 BindingEntry.settings** | `CodexSettings { auth_mode }`，从 `Credential` 变体推导默认值 | 04 §6.2 第 6 条已经论证迁移路径清晰（per-entry `CodexSettings` 已存在，`activate_tool` 已会从 provider 行兜底）。 |
| `meta.claude_{haiku,sonnet,opus}_model` | **迁到 `AgentBinding.roles`** | `roles["fast"]` / `roles["default"]` / `roles["deep"]` | 04 §5.3 + §5.6 的必答题。二选一的答案是 binding 级 roles。 |
| `meta.model_catalog` | **移出 provider 行** | 独立缓存文件 `~/.skillstar/cache/model_catalog/<provider_id>.json` | 04 §5.12：`ModelCatalogEntry.raw` 保存整个上游 JSON，OpenRouter 数百模型被 pretty-print 进 `model_providers.json`，无裁剪无上限。 |
| `meta.baseURL`（v1 遗留） | **删除** | — | 唯一读取方是 `ai_provider/resolve.rs` 的 legacy 路径，随 v1 store 一起退休（见 §3.4）。 |
| `ModelCatalogFetchResult.metadata_sources` | **实现** | `Vec<CatalogSource>`，真的填 | v3 里硬编码 `Vec::new()`，永远是空数组（04 §5.14 第 3 条）。02 §8.1 A8 要求把来源显示在 UI 上，所以要真的填。 |
| `AgentConfigFileSpec.format` 的 `"env"` | **删除死枚举值** | `enum ConfigFormat { Json, Toml, Yaml }` | 没有任何 spec 在用（04 §5.14 第 2 条）。顺手把字符串换成枚举。 |
| 两层设置袋（`ToolActivation.settings` + `ToolBinding.settings`） | **保留两层，但语义收窄** | entry 级 = 「这个 provider 在这个 Agent 下的连接参数」（Codex wire/auth）；binding 级 = 「这个 Agent 的非角色编排」 | 角色抽出去之后，binding 级设置袋只剩很少的东西，两层不再容易混淆。`update_tool_settings` / `update_tool_binding_settings` 改名为 `update_binding_entry_settings` / `update_agent_settings`（名字只差 `binding` 一个词是 v3 的实际问题）。 |

### 2.5 模型目录的来源策略

**四级，优先级从低到高：**

| 级 | 来源 | 存储位置 | 更新方式 | 回答什么 |
| --- | --- | --- | --- | --- |
| **L0** | 编译期快照 | `crates/skillstar-models/assets/models_dev_snapshot.json`（内置） | 随 SkillStar 版本发布，由 `scripts/internal/gen_model_snapshot.sh` 生成 | 离线可用、首次启动即有数据 |
| **L1** | models.dev `api.json` | `~/.skillstar/cache/models_dev.json` + `.etag` | 带 ETag 的按需刷新（TTL 6h），失败**静默回落 L0** | 价格、上下文、能力、模态、推理档位 |
| **L2** | Provider 的 `/v1/models` | `~/.skillstar/cache/model_catalog/<provider_id>.json` | 用户点「拉取模型」时 | 这家中转**实际**开了哪些模型 |
| **L3** | 用户覆盖 | `Provider.ext.model_overrides` | 高级区手填 | 目录错了或没收录时的兜底 |

**合并规则（显式声明谁赢，01 §7.1 第 7 条的硬要求）**：

```
模型是否存在        ← L2 ∪ L3（发现和手填决定"有哪些"）
context / max_output / cost / modalities / tool_call / reasoning
                    ← L3 > L1 > L0（客观事实，目录更权威）
display_name        ← L3 > L2 > L1 > L0（用户和 provider 的叫法优先）
status              ← L1 > L0（deprecated 只有目录知道）
```

**匹配算法**（L2 拿到的模型 id 怎么找到 L1/L0 的元数据）：
1. `(provider_id, model_id)` 精确匹配（走 `preset_id` → models.dev provider id 的映射表）。
2. 跨 provider 的 `model_id` 精确匹配。
3. **最长前缀匹配 + 边界字符校验**（01 §5.1 Chatbox 的 `findModelInRegistry`）：前缀后必须是 `-`/`:`/`.` 或结尾，避免 `gpt-4` 匹配到 `gpt-4o`；对微调 id（`gpt-4o:ft-xxx`）有效。
4. 都不中 → `CatalogSource::Discovered`，`facts` 全空，UI 显示「未识别，元数据不完整」并给手填入口。
   【必须做】02 §8.2 B7：把「模型 id 被识别成了什么」显示给用户（Void 的 `recognizedModelName`）。

**失效退化**（照抄 catwalk 的语义，03 §4.3，值得逐字借鉴）：

> 拉取失败是**通知性**的，它意味着目录没能被缓存，或上游返回了不可用的内容。
> **它绝不意味着没有 provider 可用**，调用方应当把它当作警告显示并继续使用返回的列表。
> 单纯连不上网**根本不算错误**。

对应到实现：`fetch_model_registry()` 返回 `(Catalog, Vec<Warning>)`，永不 `bail!`。
UI 在模型选择器顶部显示一条可关闭的 warning 条，不是 toast、不是 modal。

**并发与原子性**（02 §8.1 A8 引 Kilo 的 `models-dev.ts`）：
写缓存用 temp file + `rename` 原子替换；坏 JSON 删掉重拉；
跨进程用 advisory file lock（SkillStar 是单进程桌面应用，但 CLI 模式下同一个二进制可能并发）。

**体积决策**（03 §4.5）：L0 快照不塞 3.71 MB 全量。只冻结
①`models.json`（267 KB，provider-agnostic 的「模型本身的事实」，最稳定）
②13 条内置 preset 对应的 provider 段。预计 < 400 KB。

**LiteLLM 的定位**：补漏来源，**本轮不做**。它 gzip 只有 83 KB、覆盖 3020 条长尾，
但 models.dev 已经有 6372 条 provider-model，先上一个来源、把三级结构跑通更重要。
数据模型（`CatalogSource` 枚举）为它留了位。

### 2.6 角色路由的统一抽象

#### 2.6.1 规范角色集

采纳 03 §5.3 的最小公共抽象，**五个规范角色 + 开放扩展**：

```rust
/// 规范角色 id。这不是枚举——磁盘 schema 是开放 map，这些只是「有跨 Agent 语义」的键。
pub const ROLE_DEFAULT:  &str = "default";   // 必填。7/7 家都有。
pub const ROLE_FAST:     &str = "fast";      // 5/7。OpenCode small_model / Crush small /
                                             //      Claude DEFAULT_HAIKU / Aider weak / OMP smol
pub const ROLE_PLAN:     &str = "plan";      // 3/7。OMP plan / OpenCode agent.plan / Codex plan mode
pub const ROLE_VISION:   &str = "vision";    // 1/7，但**能力驱动**：多模态请求打到不支持图片的模型上会直接失败
pub const ROLE_SUBAGENT: &str = "subagent";  // 3/7。Codex default_subagent_model /
                                             //      Claude CLAUDE_CODE_SUBAGENT_MODEL / OpenCode agent.<name>
```

**回落链（读时解析，绝不写时复制）**：

```
fast     → default
plan     → default
vision   → default
subagent → fast → default
<extra>  → default
```

【为什么读时回落】02 §8.3 C2：Cline 的写时双写和 Void 的 eager copy 让落盘状态无法区分
「显式设成一样」和「继承默认」，关掉开关拿不回原值。Zed 的 `Option<T> + or_else` 是正解。
【为什么不写盘】D-025 已确立：未分配的角色不写，把回落语义留给目标工具自己。这条保持。

**`slow` 不是角色**（03 §5.2 观察 3）：OMP 的 `slow` 表达的是「深思档」，在其它工具里这是
**推理档位**问题而不是模型选择问题。v4 里它由 `roles["default"].effort = Some(Max)` 表达，
或者用户显式在 `extra` 里保留一个 `slow` 角色（OMP 认识它）。迁移时：
v3 的 `roles["slow"]` 原样保留为 extra 角色（不丢用户配置），但 UI 上不再是主要角色。

#### 2.6.2 Agent 声明自己的角色清单

```rust
pub struct RoleDef {
    /// 规范角色 id 或 Agent 私有 id。
    pub id: &'static str,
    /// 写盘时该 Agent 用的键名。
    /// omp → "smol"；claude-code → "ANTHROPIC_DEFAULT_HAIKU_MODEL"；opencode → "small_model"。
    pub agent_key: &'static str,
    /// 主要角色平铺，次要角色折叠（Continue 与 SkillStar 独立收敛到同一 UX，02 §9.2）。
    pub primary: bool,
    /// 回落目标。**要渲染成 UI 的 placeholder**（02 §8.1 A6，Continue 的 "Using Chat model"）。
    pub inherits: Option<&'static str>,
    /// 候选过滤。`Vision` 表示候选集必须过滤到 modalities_in 含 image 的模型
    /// （Void 的 `modelFilterOfFeatureName`，02 §6.1）。
    pub requires: RoleCapability,
}

pub enum RoleCapability { Any, Vision, ToolCall }

// AgentSpec 新增两个字段：
pub struct AgentSpec {
    // … 现有字段 …
    /// 空 slice = 该 Agent 不支持角色路由，UI 只渲染单一 provider+model 选择。
    pub roles: &'static [RoleDef],
    /// 取代 `required_url`。表达「绑定这个 Agent 需要哪种线路端点」。
    pub required_wire: WireShape,
}
```

**三档划分**（02 §9.4）：

| 档 | Agent | 角色 UI |
| --- | --- | --- |
| 无角色（`roles: &[]`） | Pi | 单一 provider + model 选择 |
| 单角色 + 兜底 | Claude Code（default + fast + subagent） | 主模型 + 两行 |
| 多角色 | OMP（10）、Codex（default/subagent/extra）、OpenCode（default/fast/extra） | 完整角色面板（主要平铺 + 次要折叠） |

#### 2.6.3 逐 Agent 投影论证

对 03 §3 写盘目标能力矩阵的**每一行**论证 `RoleMap` 能无损（或可接受降级地）投影。

---

**① OpenCode** — `~/.config/opencode/opencode.json`，JSON，多 provider ✅，角色 ✅

```
Provider  → provider.skillstar_<id8>
  name                        → .name
  endpoints.openai_chat       → .options.baseURL, npm = "@ai-sdk/openai-compatible"
  endpoints.openai_responses  → .options.baseURL, npm = "@ai-sdk/openai"（优先）
  credential::ApiKey          → .options.apiKey（明文）
  credential::EnvVar{name}    → .options.apiKey = "{env:NAME}"     ← 优于现状（03 §6.3 第 3 条）
  credential::File{path}      → .options.apiKey = "{file:PATH}"
  credential::Command         → 降级：先执行取值再写明文，UI 提示不原生支持
  headers                     → .options.headers
ModelEntry → provider.<key>.models.<id>
  serving.context/max_output  → .limit.context / .limit.output   ← 缺了 OpenCode 算不出剩余上下文
  cost.{input,output,cache_read,cache_write} → .cost.*  （字段名逐字相同）
  facts.{attachment,reasoning,tool_call,temperature,family,release_date,modalities,status} → 同名
RoleMap
  default   → 顶层 .model = "<key>/<id>"
  fast      → 顶层 .small_model
  plan/vision/subagent/extra.<k> → .agent.<name>.model
```
**损失**：`Reasoning::BudgetTokens` 的 min/max 无对应字段 → 放进 `models.<id>.options`。
**结论：≈95% 无损。**

---

**② Codex CLI** — `~/.codex/config.toml`，TOML，多 provider ✅，角色 ✅

```
Provider → [model_providers.skillstar_<id8>]
  name                        → name
  endpoints.openai_responses  → base_url          ← 【硬门】None 则该 Provider 不可绑 Codex
  wire_api                    → 恒定 "responses"  ← 【修复】不再写 "chat"
  credential::EnvVar{name}    → env_key
  credential::ApiKey          → 派生一个 env_key = SKILLSTAR_<ID8>_API_KEY，
                                 并在 UI 上告知用户需要 export（03 §0 第 5 条）
  credential::Command         → auth.command / auth.args
  credential::ExternalCli     → 完全不写 model_providers，只清掉指向托管表的指针（保持现状）
  headers                     → http_headers
RoleMap
  default          → 顶层 model + model_provider；effort → model_reasoning_effort
  subagent         → agents.default_subagent_model + default_subagent_reasoning_effort
  extra.<name>     → agents.<name>
  fast/plan/vision → 无直接对应；**降级为 UI 标灰**，不写
ModelEntry 的 cost / modalities / tool_call
  → 【旁路】SkillStar 生成一份 ModelsResponse JSON 到
    ~/.skillstar/cache/codex_model_catalog.json，config.toml 写 model_catalog_json 指过去
    （注意：Codex 只在启动时读一次；且 03 §8.1 标注最小 JSON 未实测 → 见 §6 风险 R-4）
【建议新增】为每个绑定的 Provider 生成 [profiles.skillstar_<id8>]（含 model / model_provider /
  model_reasoning_effort），用户可 `codex --profile skillstar_<id8>` 切换。
  比只维护一个全局 model_provider 指针干净（03 §2.2）。本轮列为可选增强。
```
**结构性损失**：不支持 Responses 的 Provider 完全不可投影 —— 这不是建模缺陷，是 Codex 删掉
chat/completions 造成的既成事实。**新数据模型的正确反应就是 G2：UI 提前挡住。**
**结论：≈60% 无损，且损失全部是外部造成的、可解释的。**

---

**③ Claude Code** — `~/.claude/settings.json`，JSON，单 provider，角色 ✅（档位形式）

```
Provider
  endpoints.anthropic_messages → env.ANTHROPIC_BASE_URL   ← 【硬门】None 则不可绑
  credential::ApiKey           → env.ANTHROPIC_AUTH_TOKEN
  credential::Command          → 顶层 apiKeyHelper（+ CLAUDE_CODE_API_KEY_HELPER_TTL_MS）
  credential::ExternalCli      → 清空 6 个托管 env key（保持现状）
  headers                      → env.ANTHROPIC_CUSTOM_HEADERS（**换行分隔的 "Name: Value"**，不是 JSON）
RoleMap
  default   → env.ANTHROPIC_MODEL
  fast      → env.ANTHROPIC_DEFAULT_HAIKU_MODEL      ← 【修复】不再用已废弃的 ANTHROPIC_SMALL_FAST_MODEL
  extra.sonnet → env.ANTHROPIC_DEFAULT_SONNET_MODEL   ← v3 的 meta.claude_sonnet_model 迁到这里
  extra.opus   → env.ANTHROPIC_DEFAULT_OPUS_MODEL     ← v3 的 meta.claude_opus_model
  subagent  → env.CLAUDE_CODE_SUBAGENT_MODEL
Serving
  context     → env.CLAUDE_CODE_MAX_CONTEXT_TOKENS
  max_output  → env.CLAUDE_CODE_MAX_OUTPUT_TOKENS
```
**关于「档位不是角色」**（03 §5.2 观察 1）：`ANTHROPIC_DEFAULT_OPUS_MODEL` 语义是
**别名映射**（「当有人要求 Opus 时实际用哪个模型 id」），不是「用 Opus 干什么」。
所以 sonnet/opus 落在 `extra` 而不是规范角色，只有 haiku 因为官方 schema 明写
"Haiku-class model to use for background and low-complexity tasks" 才对应 `fast`。
**新增要求**：投影后检测项目级 `.claude/settings.json` 是否也设了 `env.ANTHROPIC_BASE_URL`
（它优先级更高，会静默盖住 SkillStar 的绑定，03 §2.3）→ 进 `ConflictType::HigherPrecedenceConfig`。
**结论：≈50% 无损（cost / plan / vision / 多 provider 无处安放），且损失是 `AgentKind::Single` 的正确理由。**

---

**④ Crush** — 不在 `AGENT_SPECS`，作为**验收测试目标**（G4）

```
Provider → providers.skillstar_<id8>
  endpoints.openai_chat  → base_url，type = "openai-compat"
  endpoints.openai_responses 存在时 → type = "openai"
  credential::EnvVar     → api_key = "$NAME"
  credential::Command    → api_key = "$(cmd)"
  credential::ApiKey     → api_key = 明文，**必须单引号包裹或强制走 $VAR**
                            （Crush 的 api_key 走完整 shell 展开，key 里含 $ 会被展开，03 §6.3 第 7 条）
ModelEntry → models[]，**10 个 required 全部命中**：
  id ← id                     name ← display_name
  cost_per_1m_in ← cost.input          cost_per_1m_out ← cost.output
  cost_per_1m_in_cached ← cost.cache_write   （注意语义是"写缓存"）
  cost_per_1m_out_cached ← cost.cache_read
  context_window ← serving.context     default_max_tokens ← serving.max_output
  can_reason ← facts.reasoning != None supports_attachments ← facts.attachment
  另：reasoning_levels ← Reasoning::Effort.values
RoleMap: default → models.large，fast → models.small，effort → SelectedModel.reasoning_effort
         （收窄到 low|medium|high，超出向下取最近值）
```
**这是最好的可行性检验**：满足 Crush 的 10 个 required 字段，对其它目标都够用。
**验收测试**：`#[test] crush_projection_satisfies_schema` —— 用 13 条 preset 生成 JSON，
对仓库内固化的 `crush.schema.json` 做校验。
**结论：≈85%。损失是 plan/vision/subagent（Crush 只有两档）与 modalities 压成布尔。**

---

**⑤ Aider** — 不在 `AGENT_SPECS`，仅论证可投影性

```
endpoints.openai_chat → .aider.conf.yml 的 openai-api-base
credential            → .env / --api-key provider=KEY
RoleMap: default → model，fast → weak-model，extra.editor → editor-model
ModelEntry → .aider.model.metadata.json（**LiteLLM 格式**）
  max_input_tokens ← serving.max_input ?? context
  input_cost_per_token ← cost.input / 1e6     ← 【唯一需要单位换算的目标，必须有测试】
```
**结论：≈50%。无 provider 概念。产品上不建议接（03 §8.5：3 个月无更新）。**

---

**⑥ Pi** — `~/.pi/agent/{models.json, settings.json}`，JSON，多 provider ✅，**无角色**

```
Provider → models.json 的 providers.skillstar_<id8>
  name / baseUrl / apiKey / api / headers / authHeader / compat
  api ← WireShape：OpenaiResponses → "openai-responses"，否则 "openai-completions"
ModelEntry → models[]：id / name / reasoning / input(←modalities_in 过滤 text|image) /
  cost{input,output,cacheRead,cacheWrite,tiers(键名 inputTokensAbove)} / contextWindow / maxTokens
Reasoning::Effort → thinkingLevelMap（把规范档位映射到 provider 侧实际取值，不支持的档写 null）
RoleMap: default → settings.json 的 defaultProvider + defaultModel；effort → defaultThinkingLevel
```
**结论：≈80%。fast/plan/vision/subagent 全部丢失 —— 这是可接受的降级，不是建模错误
（Pi 本身没有角色系统）。UI 上 Pi 列不渲染角色面板（`roles: &[]`）。**

---

**⑦ OMP** — `~/.omp/agent/{models.yml, config.yml}`，YAML，多 provider ✅，**角色最全**

```
Provider → models.yml 的 providers.skillstar_<id8>
  baseUrl / apiKey / api / headers / authHeader / auth / compat / models[] / modelOverrides
  api ∈ 9 个闭合枚举值，WireShape 三值是其子集，映射直接
  credential::Command → apiKey = "!cmd <command>"
  credential::ApiKey  → apiKey 明文，**但必须避免全大写下划线形式**
                        （OMP 会先当环境变量名查，03 §2.7 的坑）
【写盘前置校验】（03 §2.7，必须在 SkillStar 侧做，不要等 OMP 启动报错）：
  1. 有 models 就必须有 baseUrl
  2. 有 models 且 auth != "none" 就必须有 apiKey
  3. 每个 model 必须在 provider 级或 model 级有 api
RoleMap → config.yml 的 modelRoles
  default → default，fast → smol，plan → plan，vision → vision，subagent → task，
  extra.<k> → <k>（OMP 支持任意自定义角色）
  值形状：skillstar_<id8>/<model_id>[:<effort>]   ← 【修复】v3 丢掉 :level 后缀的问题
```
**结论：≈95%，且是唯一能吃下完整 `RoleMap`（含 extra）的目标。**

---

**⑧ Claude Desktop**（SkillStar 独有的第 8 行）

见 §2.8 的定性：**从 `AGENT_SPECS` 移出**，因此无投影。

---

### 2.7 Official（无 Key Provider）在新模型里的表达

**结论：不再需要「种子行」这种特例，但保留两个稳定 id。**

v3 的做法：`ensure_official_providers` 在读命令里往 store 插两行 `claude-official` / `codex-official`，
靠 `is_native_official_preset_id` 的 `matches!` 白名单在 6 个地方分支
（跳过 URL 校验、强制 oauth、稳定 id、重复检查、同步 = 清空 env、detect）。

v4 的做法：**Official 就是 `Credential::ExternalCli` 的 Provider**，所有特例变成数据：

| v3 特例 | v4 数据表达 |
| --- | --- |
| `is_native_official_preset_id` 白名单 | `matches!(provider.credential, Credential::ExternalCli { .. })` |
| 激活时跳过 URL 校验 | `required_wire` 校验加一条前置：`ExternalCli` 的 Provider 免检（因为它的端点由 CLI 自己持有） |
| Codex Official 强制 `auth_mode = "oauth"` | `Credential::ExternalCli { surface: "codex" }` → writer 的 `auth_mode` 派生，不存字段 |
| 同步 = 清空托管 env / 不写 auth.json | writer 里对 `ExternalCli` 分支走 `unsync_managed_fields`，这是**一个函数指针分支，不是 tool_id 字符串比较** |
| `sort_index: -1`（前端）vs `max+1`（后端）不一致 | `ProviderKind::NativeLogin` 的排序由 UI 决定（固定置顶区），不写 `sort_index` |

**种子行还要不要？** 要，但理由变了。
- v3 的理由是「Official 是一种 preset，所以要在 store 里有一行」——这是把 Official 当普通 preset 处理的结果。
- v4 的理由是「用户要能在 UI 上把 Claude Code 从『第三方中转』切回『原生登录』，这个切换动作需要一个可绑定的对象」。
- 所以：**保留 `claude-official` / `codex-official` 两个固定 id 的 Provider 行**（04 §6.1 第 6 条：改 id = 用户的原生登录绑定失效，这是硬约束），
  但 `ensure_official_providers` 从「读命令的副作用」改为**迁移期一次性写入 + 缺失时按需补**，
  不再每次 `get_providers_flat` 都判定是否需要写盘（这也顺手修掉 04 §5.12 的「损坏文件被空 store + 两个种子覆盖」路径）。

**Claude Official 被两条 binding 共用的问题**（04 §6.1 第 7 条）随 §2.8 的 Claude Desktop 处置一并消失。

**Native Official 无法作为应用内 AI 的 provider**（04 §5.14 第 9 条，`resolve.rs` 空 key 直接 bail）：
v4 里 `Credential::ExternalCli` 让 `resolve_provider_ref` 能给出**明确的错误**
（「原生登录的 Provider 不能用于应用内 AI，请选一个有 API Key 的 Provider」）而不是笼统的 bail。

### 2.8 Claude Desktop 的定性（04 §6.3 必答题 2）

**裁决：从 `AGENT_SPECS` 移出，降级为「已规划未实现」。**

事实：它今天在磁盘上**不产生任何对 Claude Desktop 有意义的效果**——写的是 SkillStar 自造的
`~/.claude-desktop/skillstar-binding.json` 标记文件（`sync.rs:288-292` 的注释自己写着
"native write-path TBD"），`detect_provider` 读的就是自己写的标记。
它在注册表里的每一列都是特例（04 §5.2 逐项列了 6 条）。

为什么不是「保留并补齐原生投影」：Claude Desktop 的原生配置格式没有公开 schema，
本轮四份调研都没能给出它的 provider 配置写法。补齐是一个**未知工作量**的调研任务，
不能挂在重设计的关键路径上。

为什么不是「直接删除」：用户可能已经在 UI 上点过绑定。直接删会让绑定无声消失。

**具体形态**：

```rust
/// 已规划但尚未实现写盘的 Agent。UI 渲染为禁用卡片 + 明确原因，绑定入口关闭。
pub struct PlannedAgent {
    pub id: &'static str,
    pub display_name: &'static str,
    /// i18n key，说明为什么还不能用。
    pub reason_key: &'static str,
}
pub static PLANNED_AGENTS: &[PlannedAgent] = &[PlannedAgent {
    id: "claude-desktop",
    display_name: "Claude Desktop",
    reason_key: "models.agents.planned.claudeDesktop",
}];
```

**迁移**（见 §3.3）：删除 `~/.claude-desktop/skillstar-binding.json`；
丢弃 `bindings["claude-desktop"]`；如果它绑的 provider 与 `claude-code` 不同，
在迁移报告里列出来让用户知道（不自动搬到 claude-code —— 那是替用户做决定）。

**退出条件**：若 6 个月内没有可用的原生写盘路径，从 `PLANNED_AGENTS` 里也删掉。
这个条件写进 `docs/features/models/README.md`。

---

## 3. 迁移方案

### 3.1 迁移面盘点

| 磁盘对象 | 路径 | v3 形态 | v4 形态 | 破坏性 |
| --- | --- | --- | --- | --- |
| Provider store | `~/.skillstar/config/model_providers.json` | v3 flat | v4 | 需迁移 |
| 应用内 AI 引用 | `~/.skillstar/config/ai.json` | `{app_id: "claude"\|"codex", provider_id}` | `{agent_id, provider_id}` | 需迁移 |
| Claude Code 配置 | `~/.claude/settings.json` | 6 个托管 env key | 同左 + 可能新增 SUBAGENT/MAX_TOKENS | **无破坏**（merge 写） |
| Codex 配置 | `~/.codex/config.toml` | `skillstar_*` 表 + `wire_api = "chat"` | 同左但 `wire_api = "responses"` 或**整表移除** | **有破坏，见 3.4** |
| OpenCode / Pi / OMP | 各自路径 | `skillstar_*` 块 | 同左（键规则不变） | 无破坏 |
| Claude Desktop 标记 | `~/.claude-desktop/skillstar-binding.json` | 存在 | **删除** | 有破坏（但该文件本来无用） |
| catalog 缓存 | 在 `provider.meta.model_catalog` 里 | — | `~/.skillstar/cache/model_catalog/<id>.json` | 无破坏（迁移时搬走） |

### 3.2 v3 → v4 迁移步骤

实现位置：`crates/skillstar-models/src/providers/migrate/v3_to_v4.rs`，
**纯函数**（`fn migrate_v3_to_v4(v3: FlatProvidersStore) -> (ProvidersStoreV4, MigrationReport)`，无 IO）。
【为什么纯函数】01 §7.1 第 15 条：Chatbox 的 `migrateLegacyProviderSettings` 头注释
`Pure: no I/O, no platform deps`，让多条路径复用同一份逻辑并可 proptest。

```
步骤 0｜前置备份（在纯函数之外）
  create_rolling_backup(model_providers.json)  → model_providers.json.bak.<ms>
  额外写一份 model_providers.v3.json（不参与 rolling 清理，永久保留）
  ── 失败则**中止迁移**并保持 v3 运行（v3 的「备份失败只 warn 不中止」是错的，改掉）

步骤 1｜每个 ProviderEntryFlat → Provider
  id / name / preset_id / sort_index / icon_color / notes  → 原样
  created_at        → created_at_ms（v3 已经是毫秒，只改名）
  base_url_openai   → endpoints.openai_chat（空串 → None）
  base_url_anthropic→ endpoints.openai_... 不，→ endpoints.anthropic_messages（空串 → None）
  models_url        → endpoints.models_list
  endpoints.openai_responses → 由 §3.2.1 的规则推导
  api_key           → Credential（见 §3.2.2）
  models / default_model → 原样（default_model 空串 → None）
  caps              → 由 §3.2.1 推导，未知一律 Tri::Unknown
  codex_wire_api    → 丢弃（值已无意义）
  codex_auth_mode   → 暂存，进步骤 3
  meta.model_catalog       → 抽出，进步骤 4
  meta.claude_*_model      → 暂存，进步骤 3
  meta.baseURL             → 丢弃（v1 遗留）
  meta 的其余 key           → ext（保留，不猜）

步骤 2｜每个 tool_activations 条目 → bindings
  entries[].{provider_id, model, settings} → 原样
  entries[].last_sync_at（秒） → last_sync_at_ms = ×1000
  active_index → 原样（仍 clamp）
  settings.roles（OmpSettings） → binding.roles（键名映射见 §3.2.3）
  settings 的其余 key → binding.settings

步骤 3｜角色收敛
  对 agent_id == "claude-code" 的 binding：
    active entry 的 provider 的 meta.claude_haiku_model  → roles["fast"]
    ... claude_sonnet_model → roles["opus" 的兄弟] = roles["sonnet"]（extra 角色）
    ... claude_opus_model   → roles["opus"]（extra 角色）
    provider.meta.claude_main_model → roles["default"]（若 entry.model 为空）
  对 agent_id == "codex" 的每条 entry：
    entry.settings.auth_mode 缺失时 ← provider 的 codex_auth_mode（暂存值）
  丢弃 tool_activations["claude-desktop"]，记进 report.dropped_bindings

步骤 4｜catalog 外迁
  meta.model_catalog 里的每条 → ~/.skillstar/cache/model_catalog/<provider_id>.json
  转成 ModelEntry：id / display_name 直接；context_length → serving.context；
  max_completion_tokens → serving.max_output；cost（Value）→ 尽力解析成 Cost，失败留 default；
  source = CatalogSource::Discovered；raw 丢弃（不再全量保存上游 JSON）
  ── 缓存写失败**不中止迁移**（catalog 是可重建的派生数据）

步骤 5｜preset 漂移回填（见 §3.2.4）

步骤 6｜写盘
  temp file → fsync → rename（原子）
  写完立即读回校验 version == 4，失败则 rename 回备份
```

#### 3.2.1 `openai_responses` 与 `caps` 的推导

```rust
// 迁移期不做网络探测（迁移必须是纯函数且离线可用）。
let responses = if base_url_openai.contains("api.openai.com") {
    (Some(base_url_openai.clone()), Tri::Yes)
} else {
    (None, Tri::Unknown)   // ← 不是 No！留给运行时探测
};
let anthropic_caps = if base_url_anthropic.is_empty() { Tri::Unknown } else { Tri::Unknown };
// preset 已知的能力位在迁移后由 `reconcile_caps_from_preset()` 补一次（P0 级）。
```
【为什么是 `Unknown` 而不是 `No`】把「没探测过」记成「不支持」会让用户已有的 Codex 绑定
在升级后突然变成不可用且无法恢复。`Unknown` 让 UI 显示「需要检测」并提供一键探测。

#### 3.2.2 `api_key` → `Credential`

```
provider.id ∈ {claude-official, codex-official}  → ExternalCli { surface }
api_key.is_empty() && endpoints 全空             → None { reason: NativeLogin }
api_key.is_empty()                               → None { reason: LocalService }
否则                                             → ApiKey { keys: [ApiKeyEntry {
                                                      id: uuid, secret: api_key,
                                                      label: None, enabled: true }] }
```

#### 3.2.3 OMP 角色键名映射

```
v3 roles 键（OMP 原生名）  →  v4 规范键
  default   → default
  smol      → fast
  plan      → plan
  vision    → vision
  task      → subagent
  slow      → 保留为 extra 角色 "slow"（见 §2.6.1：它不是角色，但不能丢用户配置）
  designer / commit / tiny / advisor → 保留为 extra 角色
  任意自定义 → 保留为 extra 角色
OmpRoleTarget.thinking → ModelRef.effort
  合法值（在 OMP_THINKING_LEVELS 里）→ 映射到 Effort 枚举
  "inherit" / 非法值 → None
  【注意】OMP 的 9 个 level 与规范 Effort 的 7 个值不完全重合，
  映射表要显式写死并有测试（off→None{不设}、minimal→Minimal、xhigh→Xhigh、max→Max…）
```

#### 3.2.4 preset 漂移回填（04 §6.3 必答题 1）

**问题**：经前端 `CREATE_PRESETS` 创建的 deepseek / kimi / openrouter provider
`base_url_anthropic = ""`，永远绑不上 Claude。要不要按 `preset_id` 回填？

**裁决：回填，但只在三个条件同时成立时。**

```rust
// 只有这三条同时成立才回填，任何一条不成立就跳过并记进 report。
fn should_backfill_anthropic(p: &ProviderEntryFlat, preset: &ProviderPresetFlat) -> bool {
    p.base_url_anthropic.is_empty()                       // ① 当前为空
        && !preset.base_url_anthropic.is_empty()          // ② preset 有值
        && p.base_url_openai == preset.base_url_openai    // ③ OpenAI URL 与 preset 一致
                                                          //    （证明用户没有把它改成别的中转）
}
```
【为什么条件 ③ 是关键】它区分了两种「anthropic 为空」：
「前端 bug 造成的」（openai URL 与 preset 逐字相同，说明用户没动过）
与「用户主动清空的」（用户改过 openai URL，说明这行已经是他自己的配置）。
后者回填会覆盖用户的选择，前者不回填用户就永远绑不上 Claude。

**同样规则回填 `models_url`**（前端创建时硬写 `""`，导致「获取模型列表」按钮永远是灰的，04 §5.5）。

**用户可见性**：迁移完成后弹一次 modal（不是 toast），列出
「已为 N 个 Provider 补齐 Anthropic 端点」+ 具体列表 + 一个「撤销」按钮
（撤销 = 把这些字段改回空，因为 `model_providers.v3.json` 还在）。
【为什么必须可见】静默改用户配置是 02 §8.3 C8 明确禁止的（Void 的
`_validatedModelState` 静默改写是反面教材）。

### 3.3 已写盘的 Agent 配置的迁移

迁移 store 之后，**必须立刻对每个有 binding 的 Agent 触发一次 re-sync**，否则磁盘上的
`wire_api = "chat"` 会一直留着让 Codex 起不来。

```
for (agent_id, binding) in store.bindings:
    if agent_id == "claude-desktop":
        删除 ~/.claude-desktop/skillstar-binding.json；跳过
    if agent_id == "codex":
        对每条 entry：
          若 provider.endpoints.openai_responses.is_none()
            → 【不写这条 entry】，并把它记进 report.codex_dropped
            → 同时执行 unsync_codex_entry(provider) 清掉磁盘上已有的 skillstar_ 表
              （否则旧的 wire_api = "chat" 表留在 config.toml 里，Codex 仍然起不来）
          否则 → 正常写，wire_api = "responses"
        若清空后 binding 无 entry → 整条 unsync
    else:
        正常 sync_binding
```

**这是本次迁移最重要的一步**：不是把 store 改好就完了，磁盘上已经写坏的 Codex 配置
必须被主动清理。修复报告要明确告诉用户「你的 N 个 Provider 因为不支持 Responses API
已从 Codex 配置中移除，Codex 现在可以正常启动了」。

### 3.4 破坏性变更清单与最小化

| # | 破坏性变更 | 用户可感知的表现 | 最小化措施 |
| --- | --- | --- | --- |
| **B1** | 不支持 Responses API 的 Provider 无法绑定 Codex | 之前「绑上了」的 Codex 列变成禁用 | ① 迁移报告明确说明这是修复了一个会让 Codex 无法启动的问题；② UI 上给出「检测 Responses 支持」按钮，探测到就恢复可绑；③ 保留原绑定数据在 `report`，探测成功后一键恢复 |
| **B2** | Claude Desktop 列消失 | 卡片变成禁用的「即将支持」 | ① 保留在 `PLANNED_AGENTS` 而非彻底删除；② 迁移报告列出它之前绑的 provider；③ 不自动搬到 claude-code |
| **B3** | 前端不再能读到明文 api_key | 表单里 key 显示为掩码，编辑要点「更换 Key」 | ① 保留「显示」按钮走一次独立命令（一次性读取，不进 query cache）；② 这是安全上的正确方向（01 §7.1 第 4 条），不打折 |
| **B4** | `deactivate_tool` 语义拆分 | 之前点「解绑」清空全部，现在只解绑当前行 | **这是修 bug 不是破坏**（04 §5.7）。仍提供「解绑全部」的显式菜单项 |
| **B5** | `meta.model_catalog` 从 store 消失 | `model_providers.json` 体积骤降 | 迁移时搬到 cache 目录，无功能变化 |
| **B6** | v1 legacy store 读取路径删除 | 从未升级过的极老用户（v1 → v4 直跳）可能丢配置 | ① **不删 v1→v2→v3 链**，只在其后接 v3→v4；② v1 的 proptest（`prop_migration_preserves_all_provider_data`）保留 |
| **B7** | `OmpRoleTarget` / `OmpSettings` 类型消失 | 无用户可感知变化 | 磁盘 `roles` 值形状 `{provider_id, model, thinking}` → `{provider_id, model, effort}`，字段改名需要迁移（已在 §3.2.3） |

### 3.5 失败与回滚

```
迁移失败的三种情况与处置：
  ① 备份写失败            → 中止，保持 v3 运行，UI 报错。绝不在无备份的情况下迁移。
  ② 纯函数 panic / 数据不合法 → 中止，保持 v3 运行，把有问题的 provider id 写进日志。
                             （纯函数本身应当 `Result`，只在真正不可能的情况 panic）
  ③ 写盘后读回校验失败    → rename 备份回原位，报错。
迁移成功后：
  model_providers.v3.json 永久保留（不进 rolling 清理），
  并在设置里提供「回退到 v3 备份」的显式入口，保留一个大版本。
```

**store 读取健壮性（G6）同时修掉**：
`read_store` 对解析失败**不再返回空 store**，而是返回 `Err(StoreError::Corrupted { path, detail })`。
命令层把它变成一个可操作的 UI 状态：「配置文件损坏，已保留原文件在 X。
[打开文件] [从备份恢复] [重置为空]」——三个动作都由用户点，SkillStar 不替他决定。

---

## 4. 信息架构与 UI 规格

### 4.1 IA 候选

#### IA-1｜矩阵 2.0（现有 Provider × Agent 的改良版）

```
┌────────────────────────────────────────────────────────────────────────────┐
│  Models                                    [ 搜索 Provider… ]  [+ 添加]     │
├────────────────────────────────────────────────────────────────────────────┤
│  ▸ 显示列： [Claude][Codex][OpenCode][Pi][OMP]        [列设置 ⚙]            │
├──────────────┬──────────┬──────────┬──────────┬──────────┬─────────────────┤
│ Provider     │ Claude   │ Codex    │ OpenCode │ Pi       │ OMP             │
│ (sticky)     │          │          │          │          │                 │
├──────────────┼──────────┼──────────┼──────────┼──────────┼─────────────────┤
│ ▾ 原生登录 (2)                                                              │
│ ◐ Claude 官方│ ● 已绑   │   —      │   —      │   —      │   —             │
│ ◐ Codex 官方 │   —      │ ● 已绑   │   —      │   —      │   —             │
│ ▾ 第三方 (18)                                                               │
│ ● DeepSeek   │ ● 已绑   │ ⊘ 不支持 │ ○ 未绑   │ ○ 未绑   │ ◉ 3 角色        │
│              │ v3-chat  │ Responses│          │          │ default+fast+…  │
│ ● Kimi       │ ○ 未绑   │ ⊘        │ ◐ 已绑   │ ○        │ ○               │
│              │          │          │ 非当前   │          │                 │
│ ● 某中转站   │ ⚠ 冲突   │ ○        │ ● 已绑   │ ● 已绑   │ ◉ 1 角色        │
│ … 15 more（虚拟滚动）                                                        │
└──────────────┴──────────┴──────────┴──────────┴──────────┴─────────────────┘
  图例：● 已绑且 active   ◐ 已绑非 active   ○ 未绑   ⊘ 不兼容   ⚠ 冲突   ◉ 多角色
```

**相对 v3 的改良**：行分组（原生登录 / 第三方 / 已禁用）+ 搜索 + 虚拟滚动；
Claude CLI 与 Claude Desktop 两列合一（Desktop 已移出）；
单元格 5 态（v3 只有 3 态，04 §5.7）；每列都能开角色抽屉（不只 OMP）。

**压力表现**：

| 压力 | 表现 |
| --- | --- |
| Provider 多（20+） | **可接受**。虚拟滚动 + 分组 + 搜索能撑住。 |
| Agent 多（10+） | **崩溃**。列宽已经在 6 列时逼近 1280px 上限（04 §5.8 实测 992px + filler），第 8 列必然横滚，而 Provider 列 sticky 会持续遮挡。列设置隐藏列只是把问题推给用户。 |
| 状态复杂 | **崩溃**。单元格固定 `h-14`，信息密度上限两行。角色数（OMP 10 个）、冲突、延迟、余额、last_sync 五类信息挤不进去，只能靠抽屉，于是「矩阵一眼看全」这个唯一优势消失。 |

#### IA-2｜Agent 优先双栏（真正不同的方案）

```
┌──────────────────┬─────────────────────────────────────────────────────────┐
│  ⌂ 按 Agent  |  按 Provider │  Claude Code                    [同步] [⋯]   │
├──────────────────┼─────────────────────────────────────────────────────────┤
│ ● Claude Code    │  ┌─ 绑定的 Provider ───────────────────────────────┐    │
│   DeepSeek       │  │  ● DeepSeek        api.deepseek.com/anthropic  │    │
│                  │  │    ✓ 已同步 3 分钟前                    [解绑] │    │
│ ● Codex          │  │  [+ 绑定 Provider]                             │    │
│   ⚠ 1 个已移除   │  └───────────────────────────────────────────────┘    │
│                  │                                                         │
│ ● OpenCode       │  ┌─ 模型角色 ─────────────────────────────────────┐    │
│   3 个 Provider  │  │  主模型  default    DeepSeek / deepseek-chat ▾ │    │
│                  │  │  快模型  fast       DeepSeek / …-lite      ▾  │    │
│ ○ Pi             │  │  子代理  subagent   未配置 → 继承 fast         │    │
│   未绑定         │  │  ▸ 更多角色 (sonnet, opus)                     │    │
│                  │  └───────────────────────────────────────────────┘    │
│ ● OMP            │                                                         │
│   2 个 · 5 角色  │  ┌─ 写盘预览 ~/.claude/settings.json ─────────────┐    │
│                  │  │  env.ANTHROPIC_BASE_URL   = https://…/anthropic│    │
│ ▨ Claude Desktop │  │  env.ANTHROPIC_MODEL      = deepseek-chat     │    │
│   即将支持       │  │  env.ANTHROPIC_DEFAULT_HAIKU_MODEL = …-lite   │    │
│                  │  │  ⚠ 项目级 .claude/settings.json 会覆盖此配置  │    │
│                  │  └───────────────────────────────────────────────┘    │
└──────────────────┴─────────────────────────────────────────────────────────┘
```

切到「按 Provider」是同一份数据的另一个视角：左列 Provider（搜索/分组/虚拟滚动），
右侧是该 Provider 的连接设置 + 模型清单 + 「被哪些 Agent 使用」的只读反向索引。

**压力表现**：

| 压力 | 表现 |
| --- | --- |
| Provider 多（20+） | **好**。Provider 从来不需要一次全看，只在「绑定 Provider」的选择器里出现（带搜索、按 provider 分组、收藏置顶——Zed 的扁平模糊列表，02 §8.2 B4）。 |
| Agent 多（10+） | **好**。左列是纵向列表，10 项和 6 项没有区别。 |
| 状态复杂 | **好**。右侧是完整页面而不是 `h-14` 单元格，角色表、写盘预览、冲突警告、同步时间各有位置。 |

#### IA-3｜Provider 优先左列表 + 右详情（业界主流）

Cherry Studio / Chatbox / Jan 三家一致的形态。左列 Provider，右侧连接设置 + 模型清单 + Agent 开关。

**否决理由（决定性）**：**角色路由是跨 Provider 的**。
`roles["fast"]` 可以指向与 `roles["default"]` 不同的 Provider（D-025 的核心动机：
「default 用便宜的快模型、slow 用推理模型、smol 用最便宜的」）。
以 Provider 为编辑单元时，角色表无处安放——它不属于任何单个 Provider。
六个桌面客户端都没有这个问题，因为它们不做写盘同步、没有跨 Provider 的角色编排。
**这是 SkillStar 与它们的结构性差异，直接决定了 IA。**

### 4.2 推荐：IA-2

**推荐 IA-2（Agent 优先双栏 + Provider 视角切换）。**

理由按重要性：
1. **角色路由跨 Provider ⇒ 编辑单元必须是 Agent。** 这是唯一的硬约束（见 IA-3 否决理由）。
2. **Agent 数量有界、Provider 数量无界。** 6–10 个 Agent 用纵向列表永远够；20+ Provider 用矩阵行永远不够。矩阵把有界维度和无界维度放在同一个二维平面上，是错配。
3. **写盘预览需要空间。** SkillStar 相对所有调研对象的独特价值就是「我到底往你的配置文件里写了什么」。03 §6.3 第 6 条、01 §7.1 第 12 条都强调这个。矩阵单元格给不了这个空间。
4. **矩阵的唯一优势（一眼看全）可以用一个只读总览条保留**：页面顶部一行紧凑的 Agent × 状态 chip 带，点击跳转到对应 Agent。不承担编辑职责，因此不受列宽限制。

### 4.3 推荐方案的组件树

```
src/features/models/
├─ ModelsPage.tsx                    页面壳：视角切换 + 总览条
├─ components/
│  ├─ overview/
│  │  └─ AgentStatusStrip.tsx        只读总览条（保留矩阵的"一眼看全"）
│  ├─ agentView/                     ← 主视角
│  │  ├─ AgentList.tsx               左列：Agent 列表（含 PLANNED_AGENTS 的禁用卡片）
│  │  ├─ AgentDetail.tsx             右栏容器
│  │  ├─ BoundProvidersCard.tsx      绑定的 Provider（多 provider 排序 + active 指针 + 解绑）
│  │  ├─ RolePanel.tsx               ← 从 OmpRolePanel 泛化，由 AgentDescriptor.roles 驱动
│  │  ├─ RoleRow.tsx                 ← 从 OmpRoleRow 泛化
│  │  ├─ WritePreviewCard.tsx        写盘预览（按 format 高亮 json/toml/yaml）
│  │  └─ ConflictCard.tsx            冲突与外部修改
│  ├─ providerView/                  ← 第二视角
│  │  ├─ ProviderList.tsx            搜索 + 分组 + 虚拟滚动
│  │  ├─ ProviderDetail.tsx
│  │  ├─ ConnectionSection.tsx       endpoints + credential + caps 探测
│  │  ├─ ModelListSection.tsx        采纳的模型 + 拉取 Modal
│  │  └─ UsedBySection.tsx           反向索引（只读）
│  ├─ pickers/
│  │  ├─ ModelPicker.tsx             ← 扁平模糊列表 + Favorite/Recommended/<provider> 分隔条
│  │  └─ ProviderPicker.tsx
│  └─ shared/
│     ├─ CapabilityBadge.tsx         Tri 三态徽章 + 来源说明
│     ├─ SyncStatusPill.tsx
│     └─ MaskedSecretField.tsx
├─ hooks/  api/  lib/
```

**文件体积纪律**（01 §7.3 第 12 条 + AGENTS.md 1000 行硬线）：
从第一天就按 `agentView/` + `providerView/` + `pickers/` + `shared/` 分目录。
反面教材是 Chatbox 的 `$providerId.tsx`（1241 行）和 v3 的 `EditorPage.tsx`（773 行同时承担
生产创建流程和原型编辑页）。**单个组件文件目标 < 300 行。**

### 4.4 关键交互态

每个态给出「什么时候出现 / 显示什么 / 用户下一步能做什么」。
【原则】02 §8.1 A5：分级、可操作的诊断，而不是布尔「有效/无效」。Void 的五档是标杆。

| 态 | 触发 | 显示 | 下一步动作 |
| --- | --- | --- | --- |
| **空态 · 无 Provider** | store 里只有 Official 种子 | 「还没有 Provider。从预置模板开始，或手动填一个端点。」+ 13 条 preset 的卡片墙 | 点卡片 → 创建 Modal |
| **空态 · Agent 未绑定** | `binding.entries.is_empty()` | 「Claude Code 还没有绑定 Provider。」+ 兼容的 Provider 数量提示 | 「绑定 Provider」 |
| **空态 · 无兼容 Provider** | 所有 Provider 的 `required_wire` 都不满足 | 「没有 Provider 提供 Anthropic Messages 端点。」 | 「添加 Provider」/「检测已有 Provider 的能力」 |
| **空态 · 角色无候选** | `RoleCapability::Vision` 过滤后为空 | 「没有支持图片输入的模型」（Void 的 `emptyMessage`） | 「拉取模型列表」 |
| **加载 · 模型目录刷新** | L1 拉取中 | 选择器顶部一条细进度条，**列表仍可用**（显示 L0 数据） | 无需动作 |
| **加载 · 能力探测** | 保存 URL 后台探测 | `CapabilityBadge` 显示脉冲的 `Unknown` | 可取消 |
| **校验失败 · 字段级** | URL 非法 / 角色名含 `/` | 字段下方红字 + 具体规则 | 就地修改 |
| **校验失败 · 面板级** | 缺 required 字段 | 底部动作条上方一条，**只报第一条** | 【规范】校验拆两路避免同一错误显示两次（02 §2.3 引 Roo 的注释） |
| **连接失败 · 分四态** | 探测返回 | `401 密钥无效或已吊销` / `403 无权限` / `429 限流或额度用尽` / `网络错误`；401/403/429 与网络错误用**黄色**（可恢复），其余红色 | 401→「更换 Key」；429→「查看余额」；网络→「重试/检查代理」 |
| **写盘冲突 · 外部修改** | mtime > last_sync_at | 「Claude Code 的配置在 SkillStar 之外被修改过」+ diff 摘要 | 「保留外部修改」/「用 SkillStar 覆盖」 |
| **写盘冲突 · 更高优先级配置** | 项目级 `.claude/settings.json` 也设了 `ANTHROPIC_BASE_URL` | 「项目级配置会覆盖此绑定」+ 文件路径 | 「打开该文件」 |
| **写盘冲突 · env 覆盖** | 全局环境变量已设 | 同上，**扩展到 OpenCode/Pi/OMP**（v3 只检 Claude/Codex，04 §1.5） | 「查看如何取消」 |
| **不兼容** | `caps.responses_api == No` | 单元格/按钮禁用 + tooltip「Codex 只支持 Responses API，这家端点不提供」 | 「重新检测」 |
| **角色被丢弃** | writer 跳过了指向未绑定 provider 的角色 | 该角色行标黄「该 Provider 未绑定到此 Agent，已跳过」 | 「一并绑定」 |
| **模型不在列表** | 角色指向 catalog 里没有的模型 | **保留为显式选项**「`<id>` (不在当前列表)」，绝不静默清空 | 「拉取模型」/「保留」 |
| **破坏性操作 · 删除 Provider** | — | Modal 列出「将同时解除 N 个 Agent 的绑定、清除 M 个角色分配」 | 输入名称确认 |
| **破坏性操作 · 解绑全部** | — | Modal 列出「将从磁盘移除 X 个托管块」 | 确认 |
| **迁移报告** | v3→v4 首次启动 | Modal（不是 toast）：回填了什么、丢弃了什么、Codex 修复了什么 | 「撤销回填」/「知道了」 |

### 4.5 保存策略

**结论：分层。**

| 对象 | 策略 | 理由 |
| --- | --- | --- |
| 连接字段（name / URL / notes / 排序） | **autosave，300ms debounce，三态指示（保存中 / 已保存 / 失败）** | 字段少且互相独立；01 §7.2 第 10 条的个人判断与 LobeChat 的实践一致。失败**保留脏态**不丢用户输入（Cherry Studio 的 `useProviderApiKey`）。卸载时 `flush()`。 |
| 凭据 | **显式提交**（「保存 Key」按钮） | 半截的 key 被 autosave 写进去会触发一次失败的探测和一次无效写盘。且 DTO 层已不回传明文，autosave 的「回灌协调」无从做起。 |
| 角色分配 | **autosave**（选中即保存） | 单个下拉选择是原子的，没有半截状态。 |
| 新建 Provider | **Modal + 显式创建 + 前置校验** | Chatbox / Jan / Open WebUI 三家一致（01 §7.2 第 10 条）。校验失败**自动展开出错字段所在的折叠区**（Open WebUI 的 `showAdvanced = true`）。 |
| 拉取模型 | **拉取与采纳分离** | Fetch 结果进 Modal 逐条添加/移除，绝不让一次 Fetch 冲掉用户整理过的列表（01 §7.1 第 8 条）。 |
| **写盘同步** | **绝不由 autosave 触发** | 改一个字符就重写用户的 `~/.codex/config.toml` 是不可接受的。同步只在两种时机发生：① 用户显式点「同步」；② 绑定/解绑/切换 active 这类**已经是显式意图**的动作作为其原子的一部分。这条也满足 04 §6.1 第 16 条（`~/.zshrc` 只在显式点击时写）。 |

**乐观更新**：保持 v3 已确立的 `onMutate` 写缓存 → `onError` 回滚 + toast → `onSettled` invalidate 模式。
补上 v3 唯一的例外（`updateSettingsMutation` 没有 optimistic 分支，04 §3.4）。

### 4.6 i18n key 组织约定

**问题**：v3 的 `models.*` 有 28 个子命名空间、429 个叶子 key，命名空间与组件树不对齐，
其中 110 个（26%）的全部引用方都在死代码岛里。

**约定**：`models.<surface>.<block>.<key>`，surface 与组件目录一一对应。

```
models.common.*         跨视角复用（状态词、动作词、单位）
models.overview.*       AgentStatusStrip
models.agentView.*      list / detail / boundProviders / roles / preview / conflicts
models.providerView.*   list / detail / connection / modelList / usedBy
models.pickers.*        model / provider
models.presets.*        13 条 preset 的名称与提示（key 由 preset id 派生：models.presets.<id>.hint）
models.diagnostics.*    连接测试、能力探测、余额
models.errors.*         结构化错误码 → 文案（见下）
models.migration.*      迁移报告
```

**三条硬规则**：
1. **一个 namespace 只服务一个组件目录。** 加一条门禁：`check_i18n_namespace_ownership.sh`，
   对每个 `models.<surface>` 反查引用方目录，跨目录引用（除 `common`）即失败。
   这直接防止 04 §3.5 的「110 个 key 没有渲染方」重演。
2. **后端不产出面向用户的文案。** v3 的 `conflicts.rs:68-71` 等三处硬编码中文（04 §1.5）
   改为结构化 `ConflictDetail { code: ConflictCode, params: BTreeMap<String, String> }`，
   前端按 `models.errors.<code>` 渲染。同理适用于所有 `ToolSyncResult` 的消息字段。
3. **删除 110 个死 key**（两个 locale 各一份），随 WP-0 的死代码清理一起做。

### 4.7 `hub/prototype/` 的处置

**总原则：这个目录消失。** 生产代码搬出去，DEV-only 代码删掉。

| 文件 | 行数 | 处置 |
| --- | --- | --- |
| `ia/VariantD1.tsx` | 6 | **删**（纯别名，晋升的唯一标记） |
| `matrix/rich/VariantB2b.tsx` | 413 | **拆解并删除**。它的 `ClaudeCodeCell` / `OmpRoleCell` / `InlineSelectCell` 三段逻辑被 `AgentDetail` 取代 |
| `matrix/rich/RichMatrixShell.tsx` | 312 | **删**（IA-2 不用矩阵；总览条是新写的 60 行组件） |
| `matrix/rich/ClaudeMappingPanel.tsx` | 361 | **拆**：「获取模型列表」逻辑提升到 `ModelListSection`；映射表单被 `RolePanel` 取代；文件删除 |
| `matrix/rich/OmpRolePanel.tsx` | 256 | **提升为 `agentView/RolePanel.tsx`**，泛化为由 `AgentDescriptor.roles` 驱动 |
| `matrix/rich/OmpRoleRow.tsx` | 196 | **提升为 `agentView/RoleRow.tsx`** |
| `matrix/MatrixChrome.tsx` | 53 | **删** |
| `matrix/AgentColumnCarousel.tsx` | 96 | **删**（列显隐是矩阵专有问题） |
| `matrix/ClaudeSurfaceIcon.tsx` | 64 | **提升**到 `shared/AgentIcon.tsx`（Claude Desktop 分支随 §2.8 删除） |
| `matrix/matrixColumns.ts` | 71 | **删**（列定义由 `AgentDescriptorDto` 派生，这是 G1 的一部分） |
| `usePrototypeHub.ts` | 157 | **拆**：数据聚合提升到 `hooks/useModelsData.ts`；`stub`（PROD 下空函数）与 `stateDump`（30 行调试对象）**删除** |
| `types.ts` | 87 | **删**（被生成类型取代） |
| `modelsNavBridge.ts` | — | **提升**到 `lib/navBridge.ts` |
| `EditorPage.tsx` | 773 | **拆**：`CreatePage` → `providerView/CreateProviderModal.tsx`（**改用 `get_provider_presets_flat` 的 13 条，删掉 `CREATE_PRESETS`——这是修 04 §5.5 的生产 bug**）；`app-ai` overlay → `settings/`；`agent-settings` overlay → `AgentDetail`；`detailStyle` 三态删到只剩一种 |
| `ModelsHubPrototype.tsx` / `ia/VariantD2.tsx` / `ia/VariantD3.tsx` / `StateDump.tsx` / `PrototypeOverlays.tsx` | ~718 | **全删**（DEV-only 岛） |
| `matrix/rich/VariantB2a.tsx` / `VariantB2c.tsx` | 400 | **全删**（零引用死代码） |

**同时删除 prototype 目录之外的 ~2906 行死代码**（04 §3.3 的 17 个文件）。
**但先做一次判定**：`AgentSettingsDialog` 岛里实现了三个产品上不存在的能力
（Codex `wire_api`/`auth_mode` 切换、配置文件编辑器、Claude 层级模型编辑）。裁决：

| 能力 | 裁决 |
| --- | --- |
| Codex wire/auth 切换 | **不接回**。v4 里 wire 由能力位决定、auth 由 Credential 变体派生，这个表单没有存在理由 |
| 配置文件编辑器（`AgentConfigFiles` + 5 条命令） | **接回**，作为 `AgentDetail` 的「高级 → 直接编辑配置文件」。它是 SkillStar 相对所有调研对象的差异化能力，且后端 5 条命令已经写好且有测试 |
| Claude 层级模型编辑 | **接回**，但形态变成 `RolePanel` 的 extra 角色行，不是独立表单 |

**门禁**：新增 `scripts/internal/check_ts_orphan_modules.sh`，对 `src/features/*/` 下的 `.tsx`/`.ts`
做可达性分析（从 `src/pages/` 与 `src/main.tsx` 出发），孤儿即失败。
【为什么必须有】04 §5.10：`check_no_orphan_modules.sh` 脚本头注释明写它只处理 `.rs`，
于是 4000 行 TS 死代码在 lint/build/test 全绿的情况下活了下来。

---

## 5. 实施拆解

**六个工作包。串行/并行关系画在下面，不能靠猜。**

```
WP-0 清场 ──┬─→ WP-1 类型与生成面 ──┬─→ WP-2 目录层 ─┐
            │                        ├─→ WP-3 角色与写盘 ─┼─→ WP-5 凭据强化（可延后）
            │                        └─→ WP-4 前端 IA ────┘
            └─（WP-0 独占，与任何包都不能并行）
```

**关于 `bun run types:gen` 的串行关系（AGENTS.md 硬性要求）**：
`src/types/generated/` 由 `cargo test … export_bindings` 生成，**不能手改**。
凡是改动 `#[derive(TS)]` 类型或 DTO 投影的包（WP-1 / WP-2 / WP-3 / WP-5）都会重写
`src/types/generated/` 下的文件，因此：
- **同一时刻只能有一个包在改生成类型。** WP-2 与 WP-3 虽然在 Rust 侧文件不重叠，
  但都会碰 `src/types/generated/` → 必须约定**先合 WP-2 再合 WP-3**，或严格分配到不同的 `export_to` 文件名并接受 rebase 冲突。
- 每个包的提交必须包含「改 Rust → 跑 `bun run types:gen` → 提交生成结果」三步，不能只提交前两步。
- WP-4（前端）只**消费**生成类型，不产生。它可以与 WP-2/WP-3 并行，但要在对方合入后 rebase。

---

### WP-0｜清场：删死代码、搬 prototype、加 TS 门禁

**依赖**：无。**必须最先，且独占**（触及 ~4000 行删除 + 大规模文件移动，与任何包并行都会产生无法自动解决的冲突）。

**改哪些文件**
- 删：04 §3.2 的 DEV-only 岛（~1130 行）+ §3.3 的 17 个文件（~2906 行）+ 它们的 3 个测试文件。
- 移：`hub/prototype/` 下判定为「生产」的文件按 §4.7 的表搬到新目录（本包只搬不改逻辑，改逻辑留给 WP-4）。
- 删 i18n：`models.card` / `status` / `dialog` / `gallery` / `configFiles` / `launch` 六组共 110 key × 2 locale。
  【注意】`configFiles` 与 `launch` 按 §4.7 的裁决要**接回**，所以这两组 key 保留（实际删 ~96 个）。
- 改：`scripts/internal/i18n_hardcoded_baseline.txt`（4 行 prototype 路径随目录消失而删除）。
- 新增：`scripts/internal/check_ts_orphan_modules.sh` + 接进 `.githooks` 与 `.github/workflows/ci.yml`。
- 修：生产路径上未走 i18n 的裸英文（`Bind` / `Unbind` / `← Back` / `Provider × Agent` 表头）。

**验收**
```bash
bun run lint && bun run build && bun run test
bash scripts/internal/check_ts_orphan_modules.sh      # 新脚本，必须 0 孤儿
bash scripts/internal/check_i18n_hardcoded.sh
bash scripts/internal/check_file_size.sh
```
- 新增测试：`check_ts_orphan_modules.sh` 自身的 fixture 测试（造一个孤儿文件，脚本必须红）。
- 手工验收：`ls src/features/models/components/hub/prototype` 应当报「不存在」。
- **`cargo` 侧零改动**（本包不碰 Rust）。

---

### WP-1｜v4 类型、迁移与生成面

**依赖**：WP-0。**产出后 WP-2/3/4 才能开工。**

**改哪些文件**
- 新增 `crates/skillstar-models/src/providers/{provider.rs, credential.rs, binding.rs, catalog_types.rs}`
  （§2.3 的类型；每个文件 < 400 行）。
- 改写 `providers/types.rs` → 只保留 v1/v2/v3 的历史类型（供迁移读），标 `pub(crate)`。
- 新增 `providers/migrate/v3_to_v4.rs`（**纯函数** + `MigrationReport`）。
- 改 `providers/store.rs`：`read_store` 返回 `Result<_, StoreError>`，损坏不再静默返回空（G6）；
  迁移链 v1→v2→v3→v4；备份失败中止。
- 改 `providers/crud.rs`：命令语义按 §2.4 拆分与改名。
- 新增 `crates/skillstar-app/src/models/dto.rs`：`ProviderDto` / `AgentDescriptorDto` 投影（按 D-034，
  `impl From` 用完全解构；`u64` 标 `#[ts(type = "number")]`）。
- 改 `src-tauri/src/commands/models_commands/`：命令签名跟随；`FlatProvidersResponse` 换成生成 DTO
  （**注意 04 §6.1 第 13 条的 snake_case 坑：改序列化风格前先确认前端消费点已换成生成类型**）。
- 改 `scripts/internal/check_generated_types.sh` 的过期注释（§0.1 第 2 条）。
- 改 `src/types/models.ts` → re-export barrel。
- 改 `crates/skillstar-models/src/ai_provider/`：`app_id` → `agent_id`，`ai.json` 迁移。

**冲突提示**：本包几乎重写 `providers/`，**与 WP-2、WP-3 在 `providers/` 和 `tool_sync/types.rs` 上必然冲突**，所以它们必须等本包合入。

**验收**
```bash
cargo check --workspace --locked
cargo test -p skillstar-models --locked
bun run types:gen && git diff --exit-code src/types/generated/   # 生成结果必须已提交
bash scripts/internal/check_generated_types.sh
bash scripts/internal/check_command_boundaries.sh
bun run lint && bun run build && bun run test
```
- 新增测试：
  - `migrate_v3_to_v4_preserves_every_provider_field`（proptest，对照 v3 的两个现有 proptest 写）
  - `migrate_v3_to_v4_maps_omp_role_keys`（§3.2.3 的映射表逐条）
  - `migrate_backfills_anthropic_url_only_when_openai_url_matches_preset`（§3.2.4 的三条件）
  - `migrate_leaves_user_edited_urls_alone`（条件 ③ 的反例）
  - `corrupted_store_returns_error_and_keeps_file`（G6）
  - `provider_dto_never_contains_plaintext_secret`（对 `serde_json::to_string(&dto)` 断言不含 key）
- 删除的测试：`test_get_all_presets_flat_count`（纯计数断言，价值低，换成
  `every_preset_id_maps_through_skillstar_providers`——后者已存在且必须保留）；
  `binding_settings_survive_a_reactivation` / `entry_settings_and_binding_settings_are_independent`
  按 §2.4「保留两层」的裁决**继续保留**。

---

### WP-2｜模型目录三级回退

**依赖**：WP-1。**与 WP-3 在 Rust 文件上不重叠，但都改生成类型 ⇒ 约定先合本包。**

**改哪些文件**
- 新增 `crates/skillstar-models/src/catalog/{mod.rs, snapshot.rs, registry.rs, discovery.rs, merge.rs}`。
- 新增 `crates/skillstar-models/assets/models_dev_snapshot.json`（L0，< 400 KB）。
- 新增 `scripts/internal/gen_model_snapshot.sh`（从 `https://models.dev/api.json` 生成快照，
  **走 `probe_http_client`**；脚本本身跑在开发机，不在用户机）。
- 改 `providers/model_catalog.rs` → 收缩为「L2 发现」的解析层；`metadata_sources` 真的填。
- 改 `src-tauri/src/commands/models_commands/diagnostics.rs`：
  `fetch_provider_model_catalog` 返回合并后的 `Vec<ModelEntry>` + `Vec<Warning>`。
- 新增命令 `refresh_model_registry()`（L1 手动刷新）。

**硬性约束**
- 所有 HTTP 走 `skillstar_core::infra::http_client::probe_http_client`（AGENTS.md 红线）。
- 缓存路径走 `skillstar_core::infra::paths`，尊重 `SKILLSTAR_DATA_DIR`。
- 拉取失败**永不 `bail!`**（§2.5 的退化语义）。

**验收**
```bash
cargo test -p skillstar-models --locked catalog
bun run types:gen && git diff --exit-code src/types/generated/
cargo check --workspace --locked
```
- 新增测试：
  - `registry_falls_back_to_snapshot_when_offline`（注入一个必失败的 client）
  - `discovery_only_models_get_source_discovered_and_empty_facts`
  - `longest_prefix_match_respects_boundary_chars`（`gpt-4` 不匹配 `gpt-4o`）
  - `merge_precedence_is_user_over_registry_over_snapshot`（§2.5 的合并表逐条）
  - `atomic_write_survives_interrupted_rename`
  - `corrupt_cache_is_deleted_and_refetched`
- **必须用临时目录**：catalog 缓存测试设 `SKILLSTAR_DATA_DIR`。

---

### WP-3｜角色抽象、Agent 注册表与写盘修复

**依赖**：WP-1。**与 WP-2 串行（生成类型）。** 与 WP-4 可并行（WP-4 只消费 `AgentDescriptorDto`，本包定型后即可）。

**改哪些文件**
- 改 `tool_sync/agents.rs`：`AgentSpec` 加 `roles: &'static [RoleDef]` 与 `required_wire`；
  移除 `claude-desktop` 到 `PLANNED_AGENTS`；`format` 字符串 → `ConfigFormat` 枚举。
- 改 `tool_sync/types.rs`：删 `OmpRoleTarget` / `OmpSettings`（被通用 `ModelRef` + `AgentBinding.roles` 取代）；
  `OMP_THINKING_LEVELS` → 保留为「OMP 侧取值域」，但不再是全局唯一枚举。
- 改 `tool_sync/sync.rs`（Claude Code）：从 `binding.roles` 读而不是 `provider.meta`；
  `ANTHROPIC_SMALL_FAST_MODEL` → `ANTHROPIC_DEFAULT_HAIKU_MODEL`；
  新增 `CLAUDE_CODE_SUBAGENT_MODEL` / `MAX_CONTEXT_TOKENS` / `MAX_OUTPUT_TOKENS`；
  删除 `sync_to_claude_desktop` 与其标记文件逻辑。
- 改 `tool_sync/multi_provider.rs`（Codex）：`wire_api` 恒定 `"responses"`；
  `endpoints.openai_responses.is_none()` 的 entry 拒绝写并返回结构化原因；
  修 `unsync` 无条件删 `model_provider` 与 sync 路径条件删的不一致（04 §5.14 第 4 条）。
- 改 `tool_sync/omp_provider.rs`：角色从通用 `roles` 读；`:effort` 后缀按 §3.2.3 反向映射写出。
- 改 `tool_sync/conflicts.rs`：文案结构化（`ConflictCode` + params）；
  env 检查扩展到 OpenCode/Pi/OMP；新增 `HigherPrecedenceConfig`（项目级 `.claude/settings.json`）。
- 新增 `tool_sync/projection/crush.rs`（**只用于验收测试，不进 `AGENT_SPECS`**）+ 固化的 `crush.schema.json`。
- 拆 `tool_sync/tests/part4.rs`（974 行，距 1000 行硬线只剩 26 行）为 `part4a/part4b`。

**验收**
```bash
cargo test -p skillstar-models --locked tool_sync
cargo test --workspace --locked
bash scripts/internal/check_file_size.sh
```
- 新增测试：
  - `crush_projection_satisfies_all_ten_required_fields`（**G4 的验收测试**）
  - `codex_never_writes_wire_api_chat`（grep 式断言 + 逐字节 TOML 断言）
  - `codex_refuses_provider_without_responses_endpoint`
  - `claude_writes_roles_from_binding_not_provider_meta`
  - `omp_role_value_keeps_effort_suffix`（修 03 §7.4 第 3 条）
  - `registry_role_ids_are_projectable`（每个 `RoleDef.id` 要么是 5 个规范角色之一，要么 `primary == false`）
- 改的测试：`registry_covers_exactly_the_known_agents_in_order`（6 → 5 个 Agent）；
  `registry_display_names_match_legacy_targets`（补上遗漏的 omp，04 §5.14 第 1 条）；
  前端 `agentRegistry.test.ts` 的成对字面量。
- **保留不动**：`*_unsync_leaves_user_owned_default_pointer_alone`（pi/omp）、
  `omp_skips_roles_that_would_dangle`、`omp_output_is_accepted_by_the_real_binary`
  —— 这三类是用户数据安全的守卫（04 §4.1）。
- **测试隔离**：所有 tool-sync 测试持有 `use_sandbox_home()` guard；
  集成测试显式设 `SKILLSTAR_TOOL_SYNC_HOME`（AGENTS.md 硬性要求）。

---

### WP-4｜前端 IA-2

**依赖**：WP-1（生成类型）+ WP-3 的 `AgentDescriptorDto` 定型（可在 WP-3 的第一个提交后开工）。

**改哪些文件**：§4.3 的整棵组件树；`src/pages/Models.tsx`；i18n 两个 locale 按 §4.6 重组。

**验收**
```bash
bun run lint && bun run build && bun run test
bash scripts/internal/check_feature_imports.sh
bash scripts/internal/check_file_size.sh          # 每个组件 < 300 行
bash scripts/internal/check_ts_orphan_modules.sh
```
- 新增测试：`RolePanel` 由 descriptor 驱动渲染行数（给 3 个不同 `roles` 长度的 descriptor）；
  `AgentList` 渲染 `PLANNED_AGENTS` 为禁用；四态连接错误文案；写盘预览的三种 format。
- **视觉验证**：按仓库既有做法用 headless Chrome + vite 截图（见 memory `ui-visual-verification`）。
- 手工验收：`grep -rn '"claude-code"\|"omp"\|"claude-desktop"' src/features/models/ --include=*.tsx --include=*.ts | grep -v generated` 应为 0（**G1 的证伪命令**）。

---

### WP-5｜凭据强化（可延后）

**依赖**：WP-1。**可以推迟到下一个迭代**，因为 §2.3.2 的 `Credential` 枚举在 WP-1 就已落地，
本包只是把「明文存 JSON」换成「keyring + 加密文件回退」。

**改哪些文件**
- 新增 `crates/skillstar-models/src/providers/secrets.rs`：照抄 Jan 的 `provider_secrets.rs`
  （keyring 优先 + AES-256-GCM 文件回退 + `KEYRING_DOWN` latch + 原子写 + 0600 + `spawn_blocking`）。
- 改 `Credential::ApiKey`：`secret` 落盘改为 secret handle，明文进 keyring。
- 新增命令 `set_provider_secret` / `reveal_provider_secret`（后者一次性、不进 query cache）。

**铁律**：**停用/解绑 Provider 绝不删除持久化凭据**（Jan 的血泪注释，01 §4.1）。
**验收**：`deactivating_a_provider_keeps_its_stored_secret`；`keyring_failure_falls_back_to_encrypted_file`；
`fallback_file_is_0600_on_unix`。

---

### 5.7 不能并行的组合（明确列出）

| 组合 | 冲突点 | 处置 |
| --- | --- | --- |
| WP-0 × 任何包 | 大规模删除与移动 | WP-0 独占窗口 |
| WP-1 × WP-2 | `providers/model_catalog.rs`、`providers/types.rs` | 串行 |
| WP-1 × WP-3 | `providers/types.rs`、`tool_sync/types.rs` | 串行 |
| WP-2 × WP-3 | `src/types/generated/`（都会重写） | 串行，约定先 WP-2 |
| WP-3 × WP-4 | `agentRegistry.test.ts` 的成对字面量 | 由 WP-3 一次改完，WP-4 只删该文件（union 改由生成类型提供） |
| WP-4 × WP-5 | 无 | 可并行 |

---

## 6. 风险清单

| # | 风险 | 触发条件 | 影响 | 缓解 |
| --- | --- | --- | --- | --- |
| **R-1** | **迁移丢数据** | v3→v4 纯函数有未覆盖的字段组合 | 用户的 provider / 绑定 / 角色消失 | ① 迁移前双备份（rolling + 永久 `model_providers.v3.json`）；② 备份失败即中止；③ 写后读回校验；④ proptest 覆盖「每个 v3 字段都在 v4 里可找到」；⑤ 设置里保留「回退到 v3」入口一个大版本 |
| **R-2** | **`caps` 推导过严，用户已有绑定大面积失效** | 迁移把「没探测过」记成「不支持」 | 用户升级后发现 Codex 全部解绑，且不知道怎么恢复 | ① 迁移期一律写 `Tri::Unknown` 而非 `No`（§3.2.1）；② UI 对 `Unknown` 显示「需检测」+ 一键探测，不是禁用；③ 只有探测明确返回 `No` 才禁用 |
| **R-3** | **Codex 修复本身造成新的破坏** | 用户的中转不支持 Responses，迁移把它从 Codex 移除 | 用户觉得「SkillStar 把我的配置删了」 | ① 迁移报告用 modal 明说「这修复了一个会让 Codex 无法启动的问题」；② 报告里列出被移除的 provider 与原因；③ 提供「重新检测」入口；④ 文案要说清楚这是 Codex 上游删掉 chat/completions 造成的，不是 SkillStar 的选择 |
| **R-4** | **Codex `model_catalog_json` 的最小 JSON 未实测** | 生成的 `ModelsResponse` 缺 required 字段 | Codex 启动报错（比不写更糟） | ① 03 §8.1 已标为未解决项；② **本轮把 `model_catalog_json` 列为可选增强，默认不写**；③ 实现前必须先手工喂一份最小 JSON 给真实 Codex 验证；④ 加一个类似 `omp_output_is_accepted_by_the_real_binary` 的真二进制测试 |
| **R-5** | **models.dev 上游变动** | 域名迁移 / schema 变更 / 服务下线 | L1 层失效 | ① pin 域名 `https://models.dev/api.json` 而非 GitHub raw（上游主体已从 `sst` 迁到 `anomalyco`，03 §4.1）；② L0 快照保证离线可用，L1 失效只是数据变旧；③ `CatalogSource` 枚举为 LiteLLM 补漏源留位 |
| **R-6** | **中转站模型 id 大面积无法匹配目录** | 用户接的是改过模型名的中转 | 大量模型显示「未识别，元数据不完整」，写不出 Crush/OpenCode 需要的字段 | ① 这是**已知且不可完全消除**的（02 §5.4 坑③：Kilo 也要为 GLM-5.2 打硬编码补丁）；② 四级匹配（精确 → 跨 provider → 最长前缀 → 未识别）尽量提高命中；③ 未识别时给手填入口 + 显示「被识别成了什么」；④ 写盘时对缺元数据的模型**降级但不失败**（OpenCode/Pi 可以只写 id） |
| **R-7** | **`bun run types:gen` 的串行约束被违反** | 两个包同时改生成类型 | `src/types/generated/` 冲突，且 rebase 后可能出现「生成结果与 Rust 不一致」的静默漂移 | ① §5.7 明确列出不可并行组合；② `check_generated_types.sh` 在 CI 与 pre-push hook 都跑，漂移必红；③ 每个包的提交必须含生成结果 |
| **R-8** | **前端不再拿到明文 key 后，现有功能断链** | 余额查询、连接测试、延迟探测都传 `apiKey` 参数 | 这些功能报错 | ① 把这些命令改为传 `provider_id` 让后端自己取 key（本来就该这样）；② 这个改动放在 WP-1 而不是 WP-5，因为它是 DTO 形状的直接后果；③ 逐个命令清点：`test_provider_connection` / `fetch_provider_models` / `query_provider_balance` / `test_endpoints_latency` |
| **R-9** | **IA 全换导致用户重新学习** | 从矩阵切到 Agent 双栏 | 老用户找不到入口 | ① 保留只读总览条承接「一眼看全」的心智；② 「按 Provider」视角保留 provider-first 的入口；③ 首次进入新 IA 时一个一次性引导；④ 不做 A/B —— 04 §5.10 已经证明「留着旧 IA 万一回滚」的代价是 4000 行死代码 |
| **R-10** | **工作包 WP-0 的删除误伤** | 可达性分析漏判（动态 import、字符串路由） | 删掉在用的代码 | ① `check_ts_orphan_modules.sh` 先以**报告模式**跑一轮，人工复核清单；② 删除分两次提交（先删零引用、再删传递性死）；③ `bun run build` + 手工点一遍每个入口 |
| **R-11** | **`tool_sync/tests/part4.rs` 拆分与 WP-3 的改动叠加** | 974 行文件同时被拆分和修改 | 冲突不可自动解决，且拆分后 diff 无法审查 | ① **先拆后改**：WP-3 的第一个提交只做机械拆分（零逻辑改动，测试全绿），第二个提交才改逻辑 |
| **R-12** | **Claude Desktop 移除引发用户投诉** | 用户以为它本来是能用的 | 感知为功能倒退 | ① 保留为 `PLANNED_AGENTS` 的禁用卡片而不是彻底消失；② 文案明说「此前的绑定不会写入 Claude Desktop 的原生配置，因此不产生实际效果」——这是诚实地承认已有的问题；③ 迁移报告列出它之前绑的 provider |

---

## 附录 A：本方案对 04 §6.3 三个必答题的回答

| # | 问题 | 回答 | 出处 |
| --- | --- | --- | --- |
| 1 | preset 漂移影响了多少存量数据？迁移时回填还是留给用户？ | **有条件回填**：仅当「anthropic 为空」且「preset 有值」且「openai URL 与 preset 逐字相同」三条同时成立。第三条区分「前端 bug 造成」与「用户主动清空」。回填后用 modal 告知并提供撤销。 | §3.2.4 |
| 2 | Claude Desktop 列是未完成还是应当移除？ | **从 `AGENT_SPECS` 移出到 `PLANNED_AGENTS`**，UI 渲染为禁用的「即将支持」卡片。删除标记文件、丢弃 binding、迁移报告列出。6 个月无进展则彻底删除。 | §2.8 |
| 3 | 模型角色归 `provider.meta` 还是 `ToolBinding.settings`？ | **都不是——提升为 `AgentBinding.roles` 一等字段**。`provider.meta` 的 `claude_*_model` 迁到 `roles`；`ToolBinding.settings.roles` 也迁到 `roles`。设置袋保留但只放非角色配置。 | §2.3.4、§2.4、§3.2.3 |

## 附录 B：需要更新的文档（与实现同一变更序列完成）

| 文档 | 更新内容 | 归属工作包 |
| --- | --- | --- |
| `docs/boundaries.md` | `providers/` 拆成四个模块、新增 `catalog/`、`skillstar-app/src/models/dto.rs` 的新接缝 | WP-1 / WP-2 |
| `docs/architecture.md` | 模型目录的三级来源与缓存位置、catalog 移出 provider 行的数据所有权变化 | WP-2 |
| `docs/decisions.md` | 新增 4 条：D-035 四层分离的数据模型；D-036 角色路由提升为跨 Agent 一等概念；D-037 模型目录三级回退与 models.dev 为源；D-038 Claude Desktop 降级为 planned | 各自的包 |
| `docs/errors.md` | 新增：`wire_api = "chat"` 让 Codex 无法启动（根因、自检方法、修复路径） | WP-3 |
| `docs/features/models/README.md` | 行为与契约全面重写：新 IA、保存策略、角色语义、能力位、Claude Desktop 的状态与退出条件 | WP-3 / WP-4 |
| `README.md` | 若 CLI 侧有对应命令变化 | 视情况 |
| `scripts/internal/check_generated_types.sh` | 修掉 §0.1 第 2 条的过期注释 | WP-1 |

## 附录 C：本轮实际执行的验证命令

```bash
grep -c 'ProviderPresetFlat {' crates/skillstar-models/src/providers/presets.rs   # 14（含 struct 定义），实际字面量 13
grep -n 'ProviderPresetFlat {' crates/skillstar-models/src/providers/presets.rs   # 逐行确认
grep -rn 'derive(.*TS|ts(export)' crates/skillstar-models/src/providers/types.rs  # 无命中 → Provider 类型确未上生成面
ls src/types/generated/ | wc -l                                                   # 73
grep -rn "probe_http_client" crates/skillstar-models/src/                         # 确认 HTTP 统一入口
```

未运行 `cargo test` / `bun run test`（本轮为只读方案撰写，未改动任何生产代码，无需回归验证）。
所有代码事实均通过直接阅读源文件 + `grep` 引用链取得。

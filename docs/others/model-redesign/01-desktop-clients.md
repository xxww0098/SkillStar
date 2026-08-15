状态：research

# 桌面 LLM 客户端的 Provider 配置调研（T1）

为 SkillStar Models 工作台的数据模型与 UI 重设计提供外部实证。方法：把六个项目源码拉到 `/tmp/model-research/` 后本地阅读（Cherry Studio / LobeChat / Open WebUI / AnythingLLM 用 tarball 全量解包；Jan 仓库 1.5GB，用 GitHub tree API 定向抓取 15 个关键文件）。引用格式 `项目名:path:line`，快照时间 2026-08-15。

| 项目 | 形态 | 本地路径 |
| --- | --- | --- |
| Cherry Studio | Electron + SQLite(drizzle)，preset 目录独立成包 | `/tmp/model-research/cherry-studio` |
| LobeChat | Next.js + Postgres(drizzle)，目录做成 npm 包 | `/tmp/model-research/lobe-chat` |
| Open WebUI | FastAPI + Svelte，服务端型连接管理 | `/tmp/model-research/open-webui` |
| Jan | **Tauri v2 + React**（与 SkillStar 同构） | `/tmp/model-research/jan`（部分文件） |
| Chatbox | Electron + zustand/jotai | `/tmp/model-research/chatbox` |
| AnythingLLM | Express + React，配置存 env | `/tmp/model-research/anything-llm-files`（部分文件） |

---

## 1. Cherry Studio

定位最接近 SkillStar，数据模型也做得最彻底。

### 1.1 数据模型

**目录与用户数据彻底分层。** “世界上有哪些 provider/model” 放在独立 workspace 包 `@cherrystudio/provider-registry`（产出三个**生成的** JSON），“用户配了什么” 放在 SQLite 两张表，**只存 delta**。

目录侧最关键的是 **creator（模型出品方）与 provider（服务端点）分离，二者 M:N**（`cherry-studio:packages/provider-registry/docs/architecture.md:26`）：

| 文件 | 角色 | 主键 |
| --- | --- | --- |
| `data/models.json` | Creator 目录：capabilities / 模态 / contextWindow / maxOutputTokens / ownedBy | canonical model id |
| `data/providers.json` | 连接配置：endpointConfigs（baseUrl + adapterFamily）、defaultChatEndpoint、apiFeatures | provider id |
| `data/provider-models.json` | M:N 覆盖：apiModelId、价格、reasoning 契约、disabled | (providerId, modelId) |

覆盖表是**稀疏表**：“第一方/标准情况不产生任何行”（同文件 `:36`）。对 SkillStar 有直接意义——“同一个 Claude 模型被 5 个中转站提供”正是这个形状。目录是代码生成的，CI 双向卡：改 `data/*.json` 不改 `src/` → `catalog-hand-edit-check` 失败；改 `src/` 忘了重生成 → `catalog-source-sync` 测试失败（`packages/provider-registry/CLAUDE.md:41`）。

**`user_provider`：一行 = 一个 apiHost。**

```ts
// cherry-studio:src/main/data/db/schemas/userProvider.ts:36
providerId: text().primaryKey(),                                    // :39
presetProviderId: text(),                                           // :45  null = 完全自定义
endpointConfigs: text('endpoint_configs', { mode: 'json' })
  .$type<Partial<Record<EndpointType, StoredEndpointConfigOverride>>>(),  // :68
defaultChatEndpoint: text().$type<EndpointType>(),
apiKeys: text({ mode: 'json' }).$type<ApiKeyEntry[]>().default([]),  // :72
authConfig: text({ mode: 'json' }).$type<AuthConfig>(),              // :75
apiFeatures / providerSettings: json,
isEnabled: integer({ mode: 'boolean' }).notNull().default(false),    // :84
...orderKeyColumns,   // fractional index
```

文件头把不变量写死（`:1`）：`One Provider instance = One apiHost (1:1)` / `One apiHost can have multiple API Keys (1:N)`。于是“一个 provider 多套凭据/多 endpoint”分两层解决：**多 key 同 host → 一行，`apiKeys` 是数组；多 host → 多行 `user_provider` 共享同一个 `presetProviderId`**，UI 侧折叠成组。

`endpointConfigs` 只存用户拥有的字段。注释解释（`:52`）：registry 拥有的 `modelsApiUrls`/`adapterFamily` 在读时解析，持久化等于冻结一个会过期的快照（issue #17096）。写入 DTO 被专门收窄成 `EndpointConfigOverrideSchema = z.object({ baseUrl })`（`src/shared/data/types/provider.ts:258`）。

**`user_model`：null = 继承 preset。**

```ts
// cherry-studio:src/main/data/db/schemas/userModel.ts:36
id: text().primaryKey(),          // "providerId::modelId"，确定性 PK
presetModelId: text(),            // :47  溯源标记
name / capabilities / contextWindow / supportsStreaming ...   // null = 继承 preset
isEnabled / isHidden / isDeprecated: boolean                  // :98
// :110 CHECK：presetModelId IS NOT NULL
//              OR (name IS NOT NULL AND capabilities IS NOT NULL AND supportsStreaming IS NOT NULL)
```

三个亮点：确定性字符串 PK（幂等 upsert、跨设备同步都简单）；CHECK 在数据库层拒绝“半残的自定义模型”；`isDeprecated` 让 `/models` 不再返回的模型只打标不删除，避免历史会话里的 modelId 悬空。

**OAuth 与 API Key 共存。** registry 层用**集合**而非枚举：

```ts
// cherry-studio:packages/provider-registry/src/schemas/provider.ts:181
authMethods: z.array(z.enum(['api-key', 'oauth', 'external-cli'])).optional(),
// 缺省 ⇒ ['api-key']。"登录型" 是派生的 !includes('api-key')，不是独立取值。
authOptional: z.boolean().default(false),                       // :190  本地服务无需任何凭据
modelListSource: z.enum(['api', 'registry']).default('api'),    // :166
```

注释说明为什么是集合：CherryIN 同时接受用户 key 和 app 托管的 OAuth。`authOptional` 与“登录型”刻意区分——本地 provider 仍需 baseUrl 输入框，登录型连 host 输入都不显示。用户侧凭据是判别联合：

```ts
// cherry-studio:src/shared/data/types/provider.ts:150
z.discriminatedUnion('type', [ AuthConfigApiKey, AuthConfigOAuth, AuthConfigIamAws,
  AuthConfigApiKeyAws, AuthConfigIamGcp, AuthConfigIamAzure ])
```

`AuthConfigApiKeyAws` 的注释（`:126`）是“不为一家污染通用形状”的正例：AWS 的 api-key 也要 region，但 region 不放进通用 `api-key` 变体。

**密钥永不进渲染进程**——全场最值得抄的一条：

```ts
// cherry-studio:src/shared/data/types/provider.ts:79
ApiKeyEntrySchema = z.object({ id, key, label?, isEnabled })
RuntimeApiKeySchema = ApiKeyEntrySchema.omit({ key: true })        // :91
// :326 ProviderSchema.apiKeys: z.array(RuntimeApiKeySchema)  ——「without actual key values」
```

**PATCH 三态语义写进 schema 注释**（`:175`）：key absent = 不变；`null` = 显式清除；有值 = 设置。避免“前端传 undefined 到底是不改还是清空”的扯皮。

### 1.2 UI 形态

**左列表 + 右详情。** 左列两个细节值得偷：

- **同 preset 的多实例折叠成组**（`src/renderer/pages/settings/ProviderSettings/ProviderList/providerGrouping.ts:20`）：只有 ≥2 个共享同一 `presetProviderId` 才成组；组的位置锚定在**第一个成员**的下标，所以在 1↔2 阈值附近增删时侧边栏不跳动（`:42`）。
- **过滤器代替“已启用/未启用”分区**（`providerFilterMode.ts:5`）：列表是平的，`enabled | disabled | all | agent` 过滤器是隐藏禁用项的唯一旋钮。

排序用 fractional index 字符串键而非整数（`src/main/data/db/schemas/_columnHelpers.ts:68`，模型排序用 provider 内作用域的 `scopedOrderKeyIndex`，`:91`），拖拽只改一行。启用时不是简单置位，而是把新启用的 provider 顶到列表最前（`hooks/providerSetting/useProviderEnable.ts:5`）。Onboarding 期间服务端确认有可用 key 就自动启用，且用 ref 保证只在 false→true 那次触发（`useProviderOnboardingAutoEnable.ts:23`）。

**表单保存：autosave + 脏态 + 服务端回灌协调。** `useProviderApiKey` 是本次调研质量最高的 hook：

```ts
// cherry-studio:.../hooks/providerSetting/useProviderApiKey.ts:122
interface ApiKeyValue { serverApiKey; inputApiKey; hasPendingSync }
```

1. 150ms debounce 写入（`:157`）；2. 失败**回滚到脏态**而非丢弃（`:161`）；3. 卸载时 flush：`useEffect(() => () => saveLater.flush(), [])`（`:182`）；4. 服务端值变化时三方协调（`syncApiKeyValueFromServer`, `:105`）：无脏态直接接受服务端值，有脏态但规范化后一致也接受，否则保留用户输入只更新 `serverApiKey`；切换 provider 则硬重置（`:167`）。

多 key 的 UI 是一个逗号分隔输入框，落库是带稳定 id 的数组。`toApiKeyEntries`（`:53`）把字符串 diff 回条目并**尽量复用已有 id**（先按 key 值精确匹配 → 再按位置复用 → 最后才 `uuidv4()`），同时保留用户手动禁用的条目。

**连通性检查。** 类型上就分三层聚合（`src/renderer/types/healthCheck.ts:11`）：`HealthStatus{SUCCESS,FAILED,NOT_CHECKED}` → `ApiKeyWithStatus{key,status,error,latency}`（`:31`）→ `ModelWithStatus{model,status,keyResults[]}`（`:38`）。聚合规则：任一 key 失败 → 模型 FAILED，但 latency 取所有成功 key 的最小值，错误去重后 `; ` 拼接（`utils/healthCheck.ts:42`）。错误文案优先用 provider 响应体，因为 `error.message` 常常只是 HTTP statusText（"Forbidden"，同文件 `:20`）。交互是抽屉 + 显式选 (model, apiKey) 后手动触发（`ProviderConnectionCheckDrawer.tsx:69`），key 以 `maskApiKey` 显示。

**模型清单。** 自定义 provider 建立时列出 4 种文本端点 + 2 种图像端点，并**实时预览完整请求 URL**：

```ts
// cherry-studio:.../ProviderList/customProviderCreation.ts:40
OPENAI_CHAT_COMPLETIONS: '/chat/completions'   OPENAI_RESPONSES: '/responses'
ANTHROPIC_MESSAGES: '/messages'                GOOGLE_GENERATE_CONTENT: '/models/{model}:generateContent'
// :83 buildCustomProviderEndpointPreview(baseUrl, endpointType) → 完整 URL
```

同步远端模型**只补不删**（`useProviderModelSync.ts:71`）。删除模型有引用完整性保护：被设为默认模型的删不掉，前端逐个剔除重试并汇总跳过项（`ModelList/useProviderModelPullReconcile.ts:33`）。

**按用途分配模型**：四个独立 preference key（`hooks/useModel.ts:29`）`chat.default_model_id` / `feature.quick_assistant.model_id` / `feature.translate.model_id` / `feature.paintings.default_model_id`。`setDefaultModel` 把尚未单独设置的 quick/translate 一并写入（`:48`）；**painting 故意不参与级联**，因为它需要图像生成模型（`:23` 注释）。

### 1.3 值得偷的点

delta 表 + null 继承（`userModel.ts`）；确定性 PK `providerId::modelId`；`presetProviderId` 让“多实例共享一个 preset”成一等概念；凭据在 DTO 层剥离（`RuntimeApiKeySchema`）；`authMethods` 是集合、“登录型”是派生属性；fractional index 排序；endpoint URL 实时预览；`isDeprecated` 而非删除；PATCH 三态语义写进注释。

### 1.4 明显的坑

- **复杂度极高**：provider-registry 是带生成脚本、双向 CI 校验、canonical id 归一化（`canonOf`/`normalizeModelId`，处理 `org/` 前缀、`-free`/`-thinking` 变体、`-fp8` 量化、日期后缀、`4.6`→`4-6`）的独立包。SkillStar 现阶段抄这套管线是明显过度。
- **`imageGeneration` 整体替换而非深合并**，文档专门警告模型级块绝不能带 provider 专属 `vendorTransport`，否则所有未覆盖 provider 继承错误端点（`architecture.md:88`）。这是 delta 模型的固有税。
- `ProviderApiOptions` 里堆了 4 个 `@deprecated` 的 `isNotSupportXxx` 布尔（`src/renderer/types/provider.ts:32`），后来才改成 `isSupportXxx`。**能力开关不要用否定命名。**

---

## 2. LobeChat

“目录即数据包”路线的代表（`model-bank` + `model-runtime`）。

### 2.1 数据模型

```ts
// lobe-chat:packages/database/src/schemas/aiInfra.ts:20
aiProviders: { id: varchar(64),           // 注意：不是 PK
  _id: uuid().primaryKey(),               // 代理主键
  userId, sort, enabled, fetchOnClient, checkModel,
  keyVaults: text('key_vaults'),          // :46  加密密文
  source: varchar({ enum: ['builtin','custom'] }),   // :47
  settings: jsonb<AiProviderSettings>, workspaceId }
// :75 aiModels 同构，source: ['remote','custom','builtin']（:104）—— 多了 'remote' 这一态
```

`id` 不是主键，业务唯一性靠 **partial unique index**：`(id,userId) WHERE workspace_id IS NULL` 与 `(id,userId,workspaceId) WHERE NOT NULL`（`:60`）。注释说明是 migration 0110 为 workspace 化重建时从复合 PK 改来的，**主键本身不再承载唯一语义**。

**内置 preset 在代码里，DB 只存用户行，合并发生在读路径**：

```ts
// lobe-chat:packages/database/src/repositories/aiInfra/index.ts:142  getEnabledModels
const user = allModels.find(m => m.id === item.id && m.providerId === provider.id)
if (!user) return { ...item, providerId: provider.id }
const mergedModel = {                                        // :165
  abilities: !isEmpty(user.abilities) ? user.abilities : item.abilities || {},
  contextWindowTokens: typeof user.contextWindowTokens === 'number' ? user.contextWindowTokens : item.contextWindowTokens,
  displayName: user?.displayName || item.displayName,
  enabled: typeof user.enabled === 'boolean' ? user.enabled : item.enabled,
  settings: isEmpty(user.settings) ? item.settings : merge(item.settings || {}, user.settings || {}),
}
```

每个字段的空值判定都不一样（`isEmpty` / `typeof === 'number'` / `typeof === 'boolean'` / `||`）。这是**“用同一形状存 delta 但不用 null 语义”的代价**：每个字段要手写继承规则，且 `displayName: user || item` 意味着用户无法把它改成空串。用户行是惰性创建的，切开关时 upsert（`packages/database/src/models/aiModel.ts:190` `onConflictDoUpdate`）。

**模型能力元数据**（`packages/model-bank/src/types/aiModel.ts:36`）：`ModelAbilities { audio, files, functionCall, imageOutput, reasoning, search, structuredOutput, video, vision }`；模型类型 8 类 `chat|embedding|tts|asr|image|video|text2music|realtime`（`:16`）；来源三态 `builtin|custom|remote`（`:7`）。

`asr` 从旧的 `stt` 改名，处理方式值得学——**不做批量迁移，在读写边界归一化**：

```ts
// lobe-chat:packages/model-bank/src/types/aiModel.ts:31
export const normalizeAiModelType = <T extends string|null|undefined>(type: T): T =>
  (type === 'stt' ? 'asr' : type) as T
// 头注释：只有真正被读写的数据才会被转换，未触碰的老行保持有效
```

**模型退役重定向**比 Cherry 的 `isDeprecated` 又前进一步：

```ts
// lobe-chat:packages/model-bank/src/types/aiProvider.ts:433
/** Retired `${providerId}/${modelId}` → successor model id (same provider). */
modelRedirects?: Record<string, string>;   // 请求老 id 被后继透明服务，UI 显示"已被 X 取代"
```

**Provider settings 是 UI 能力声明**（`aiProvider.ts:139`）：

```ts
authType?: 'apiKey' | 'oauthDeviceFlow'    // :145      oauthDeviceFlow?: OAuthDeviceFlowConfig  // :171
modelEditable?  // :179    showAddNewModel?  // :201    showApiKey?  // :206（ollama 不需要 key）
showChecker?    // :210    showModelFetcher? // :212    showDeployName?    proxyUrl?: {...} | false
maxToolCount?      // :158  GitHub Copilot 全模型上限 128 个 tool
maxToolPayloadBytes?  // :165  Cloudflare ~100KB
sdkType? / disableBrowserRequest? / searchMode?
```

**provider 定义直接声明表单里出现哪些控件**，UI 不写 if-else；`maxToolCount`/`maxToolPayloadBytes` 把上游会 422 的约束前置成本地校验。OAuth 走 device flow，token 存 `keyVaults`，注释标注 xAI 的 refresh token 每次刷新都轮换、必须持久化最新那对（`:87`）。

### 2.2 UI 形态

**卡片墙**（`src/routes/(main)/settings/provider/(list)/ProviderGrid/Card.tsx`）+ 左侧 `ProviderMenu` + 详情页，卡片上直接有 `EnableSwitch`。**排序放在独立的 `SortProviderModal`**（`ProviderMenu/SortProviderModal/`），不在列表里直接拖，避免与“点卡片进详情”的手势冲突；模型排序同理有 `ModelList/SortModelModal/`。

模型列表按类型分 tab 带计数、**空 tab 隐藏**（`features/ModelList/index.tsx:73`），并分 `EnabledModelList` / `DisabledModels` / `SearchResult` / `EmptyModels` 四个状态组件（空态是独立组件而不是内联三元）。

**表单保存：500ms debounce autosave**，无保存按钮，保存中反馈是密码框内的 suffix 转圈（`features/ProviderConfig/index.tsx:273` + `:571` + `:288`）。两个真实的坑被注释记录：

- **连通性测试与 debounce 竞态**（`:255`）：测试按钮自己先写一次配置，所以 `handleValueChange` 里用 `isCheckingConnection.current` 短路，否则 500ms 后 debounce 又写一次。
- **切换 provider 必须 `resetFields()` 再 `setFieldsValue()`**（`:244`）：否则上一个 provider 的字段值泄漏到下一个凭据为空的 provider。用 `lastInitializedIdRef` 保证只在 id 变化时重置，不打断正在编辑的输入（`:225`）。

模型编辑则是**显式 Modal + 保存按钮**（`ModelConfigModal/` + `CreateNewModelModal/Form.tsx`）：**高频轻量字段 autosave，结构化对象编辑用 modal**。

`Checker.tsx` 里检查模型下拉的排序规则很细（`:83`）：provider 推荐的 `checkModel` 永远第一 → 已启用的在前 → `releasedAt` 倒序 → 无 `releasedAt` 最后。错误展示是 `Alert` + 可折叠的原始 JSON（`:38`），既给人话也给排障用的响应体。

### 2.3 值得偷的点

读写边界归一化替代批量迁移（`normalizeAiModelType`）；`modelRedirects` 退役重定向；provider settings 声明 UI 能力开关；`maxToolCount`/`maxToolPayloadBytes` 前置上游硬限制；模型 tab 按类型分组 + 计数 + 空 tab 隐藏；排序放独立 modal；切换 provider 时 `resetFields()` 防字段泄漏。

### 2.4 明显的坑

- **合并规则散落且不一致**（`aiInfra/index.ts:165`），`displayName` 无法被清空。
- **`ExtendParamsType` 是 40+ 成员的字符串联合**（`aiModel.ts:329`：`gpt5_2ProReasoningEffort` / `grok4_5ReasoningEffort` / `kimiK3ReasoningEffort` / `thinkingLevel2/3/4` …），每出一个新模型就加一个枚举值。把“模型特有参数”硬编码成全局枚举的必然结局。
- **`id` 不是主键**带来的心智负担：任何 join / upsert 都要记得带 `userId`(+`workspaceId`)。桌面单用户场景完全没必要。

---

## 3. Open WebUI

服务端型（管理员配置全局连接），最大价值是**反面教材**。

### 3.1 数据模型

连接不是表，是**三个平行数组 + 一个字典，靠下标关联**：

```python
# open-webui:backend/open_webui/config.py:325
OPENAI_API_KEYS      = [k.strip() for k in os.getenv('OPENAI_API_KEYS','').split(';')]
OPENAI_API_BASE_URLS = [...]     # :331
OPENAI_API_CONFIGS   = {}        # :339  JSON 对象，key 是下标字符串
# :2794 持久化成三个独立 config key：openai.api_keys / api_base_urls / api_configs

# backend/open_webui/routers/openai.py:300
async def get_openai_connection(idx: int):
    url, key = api_base_urls[idx], api_keys[idx]
    api_config = api_configs.get(str(idx), api_configs.get(url, {}))   # :304  兼容旧的 URL 键
```

模型对象上带 `urlIdx`（`:564`, `:692`），聊天请求靠它路由回连接（`:1242`）。**删除一个连接就要重排整张表**：

```ts
// open-webui:src/lib/utils/connections.ts:50  removeOpenAIConnection
const newUrls = urls.filter((_, i) => i !== idx)
newUrls.forEach((_, newIdx) => { newConfigs[newIdx] = configs[newIdx < idx ? newIdx : newIdx + 1] })  // :66
```

删中间一个后所有靠后连接下标前移，任何缓存 `urlIdx` 的模型立刻指向错误连接；且这段 re-index 在 admin Svelte 组件和这个 util 里写了**两遍**（文件头自认 “Mirrors the logic in admin/Settings/Connections.svelte”）。后来新增的 terminal 连接就学乖了，是对象数组（`:88`）。

单连接的 config 里有几个好设计（`src/lib/components/AddConnectionModal.svelte:200`）：

```ts
config: { enable, tags,
  prefix_id: prefixId,   // :204  给该连接的模型 id 加前缀，解决多中转站同名模型冲突
  model_ids: modelIds,   // :205  白名单：只暴露这些模型
  connection_type,       // 'local' | 'external'
  auth_type,             // 'bearer' | 'azure_ad' | 'microsoft_entra_id' | ...
  headers, passthrough_params, api_version, api_type }
```

`prefix_id` 尤其值得注意：同时接三个都提供 `claude-opus-4` 的中转站时，前缀是扁平模型列表里唯一能区分它们的手段。

### 3.2 UI 形态

管理员设置里的**扁平行列表**，每行 URL + key + 齿轮按钮，齿轮开 `AddConnectionModal`。禁用态用绝对定位的 `opacity-60` 遮罩层表示，URL 在行内 `readonly`（`OpenAIConnection.svelte:52`, `:63`）。Tooltip 直接告诉你请求会打到哪：`WebUI will make requests to "{{url}}/chat/completions"`（`:47`）——Cherry endpoint 预览的轻量版。

**保存策略：Modal + 显式提交 + 前置校验**，与 LobeChat 完全相反：

```ts
// open-webui:src/lib/components/AddConnectionModal.svelte:150
if (!ollama && !url) { toast.error('URL is required'); return }
if (azure && !apiVersion)      { showAdvanced = true; toast.error('API Version is required'); return }
if (azure && !modelIds.length) { showAdvanced = true; toast.error('Deployment names are required for Azure OpenAI'); return }
if (headers) { try { JSON.parse(headers) } catch { toast.error('Headers must be a valid JSON object'); return } }
url = url.replace(/\/$/, '')   // 提交时统一归一化
```

注意 `showAdvanced = true`——**校验失败自动展开出错字段所在的折叠区**。

**连通性检查用 `GET /models` 而非真实补全**（`backend/open_webui/routers/openai.py:796`）：不消耗 token、不受模型可用性影响，代价是无法验证该 key 对具体模型是否有权限。Cherry / Chatbox 走的是另一端。

### 3.3 值得偷的点

`prefix_id` 模型 id 命名空间前缀；`model_ids` 白名单（不把上游 300 个模型全灌进选择器）；校验失败自动展开对应折叠区；URL 归一化统一在提交时做；轻量连通性检查（`GET /models`）作为默认、重检查作为可选。

### 3.4 明显的坑（重点）

- **位置下标当身份**是本次调研发现的最严重反模式：删除即重排、`urlIdx` 悬空、re-index 逻辑重复实现。
- 三个平行数组 + 一个下标字典意味着**没有任何一处能原子地表达“一个连接”**。
- 兼容代码 `api_configs.get(str(idx), api_configs.get(url, {}))` 说明中途从“按 URL 键”改成“按下标键”，两套键并存至今，`:304`/`:544`/`:674` 三处重复。**换主键的代价永远比想象的大。**

---

## 4. Jan

**唯一一个 Tauri v2 + React 的同构项目**，Rust 侧密钥处理直接可抄。

### 4.1 数据模型

前端 provider 是扁平对象，settings 是**声明式控件描述符数组**：

```ts
// jan:web-app/src/types/modelProviders.d.ts:57
type ProviderObject = {
  active: boolean
  provider: string              // 既是 id 又是显示名
  api_key?: string
  api_key_fallbacks?: string[]  // 401/403/429 后依次尝试的备用 key
  base_url?: string
  settings: ProviderSetting[]   // :17 { key, title, description(markdown), controller_type, controller_props }
  models: Model[]
  persist?: boolean
  custom_header?: ProviderCustomHeader[] | null
  api_type?: 'openai' | 'anthropic'
}
```

预置 provider 就是一坨这样的字面量（`web-app/src/constants/providers.ts:52` 起）。好处：扩展可注册新 provider 而不改 UI 代码；坏处见 4.4。

**多 key = 主 key + fallback 链**（不是轮询）：

```ts
// jan:web-app/src/lib/provider-api-keys.ts:18  providerRemoteApiKeyChain
return [...(primary ? [primary] : []), ...fallbacks]   // 去重后的有序链
// Rust 侧同样合并去重：src-tauri/src/core/server/remote_provider_commands.rs:32 merge_register_api_keys
```

语义是**故障转移**而非负载均衡，比轮询简单得多，也更贴合“一把主 key + 几把备用”的真实需求。

**密钥存储：Keyring + 加密文件回退（Rust）** —— 对 SkillStar 最直接可用的一段：

```rust
// jan:src-tauri/src/core/server/provider_secrets.rs:1
//! Secrets never touch the settings file or webview storage. ... stored as a JSON array
//! under a stable service/account pair in the OS keyring so an out-of-process consumer
//! (jan CLI) can read it.
//! The Linux Secret Service needs a D-Bus session + an unlocked keyring, often absent on
//! headless/CI/SSH boxes. When unavailable we fall back to an encrypted, permission-restricted
//! file (`<jan_data>/provider_secrets.enc`, AES-256-GCM, `0600` on unix).

static KEYRING_DOWN: AtomicBool = AtomicBool::new(false);        // :44
fn is_infra_failure(err) -> bool {                               // :52
    matches!(err, PlatformFailure(_) | NoStorageAccess(_))       // NoEntry 是正常情况，绝不触发 latch
}
pub fn store_provider_keys / load_provider_keys / delete_provider_keys   // :166 / :201 / :184
```

四个细节：① `KEYRING_DOWN` latch——第一次基础设施级失败后本会话不再重试，否则每次调用都要等 D-Bus 超时并重复打日志；② 写 keyring 成功后主动删掉回退文件（`:180`），避免两份不一致；③ 回退文件原子写 + 0600：写 `.enc.tmp` → 限权 → `rename` → 再限权（`:120`）；④ 威胁模型写在注释里：per-machine id 派生的密钥只防“随手翻磁盘/备份”，不防已拿到本机代码执行权限的攻击者。keyring 访问阻塞，所有 Tauri 命令都 `spawn_blocking`（`:222`, `:236`）。

一条血泪教训：

```rust
// jan:src-tauri/src/core/server/remote_provider_commands.rs:127
/// [unregister] is called during routine reconciliation (e.g. deactivating a provider at boot),
/// so it MUST NOT touch the persisted keyring secret — otherwise a provider whose in-memory key
/// has not been re-seeded yet would have its stored key destroyed.
```

**“停用 provider” ≠ “删除凭据”**。前端持久化前还会主动剥离密钥（`web-app/src/hooks/useModelProvider.ts:17` `stripProviderSecrets`）。

### 4.2 UI 形态

`/settings/providers` 是 **Card 列表 + 每行一个 Switch**（`routes/settings/providers/index.tsx:179`），右上角 “Add Provider” 开 Dialog；点进去是 `$providerName.tsx` 详情页。

**保存策略：draft state + onBlur commit**（`$providerName.tsx:81` 的 `apiKeysDraft`/`baseUrlDraft`，`:295`/`:358` 的 commit，`:1084`/`:1106` 的 `onBlur`）。commit 时先比 `changed`，没变直接返回（`:305`）。多 key 编辑有两态：简单模式一个输入框（第一行是主 key），`showAdvancedApiKeys` 展开后每行一个输入框 + 增删（`:377`/`:383`，index 0 不允许删）。

**清空 key 是显式销毁动作**（`:350`）：`if (nextPrimary.length === 0 && nextFallbacks.length === 0) deleteProviderKeys(providerName)`，注释说明不这么做的话下次启动会从 keyring 重新灌回内存。

**逐 key 连通性检查 + 分状态文案**——本次调研错误文案做得最好的一处：

```ts
// jan:web-app/src/routes/settings/providers/$providerName.tsx:394
ok:            'OK'
unauthorized:  'Invalid / revoked key (401)'
forbidden:     'Forbidden (403)'
rate_limited:  'Rate limited / out of credit (429)'
network_error: 'Network error'
// :410 ok 绿色；401/403/429/网络错误 黄色；其余红色
// :426 handleTestApiKeys 逐把 key 发 GET {base_url}/models，表里显示 maskApiKey + 状态 + `${status} ${statusText}`
```

“invalid key”“没权限”“额度用完了”对用户是三种完全不同的行动。本地地址还会补 `Origin: tauri://localhost` 头（`:456`）——Tauri 应用打本机服务的 CORS 处理，SkillStar 会遇到同样问题。

### 4.3 值得偷的点

`provider_secrets.rs` 整个文件（keyring + AES-256-GCM 文件回退 + `KEYRING_DOWN` latch + 原子写 + 0600 + `spawn_blocking`）；“停用 ≠ 删凭据”；`api_key_fallbacks` 故障转移链；401/403/429/网络错误四态文案；持久化前 `stripProviderSecrets`；draft + onBlur commit + `changed` 短路（比 debounce 更容易推理、无竞态）。

### 4.4 明显的坑

- **`name` 就是 id**（`index.tsx:49` 用 `provider.toLowerCase()` 做重名校验），于是**无法重命名**，改名等于新建。
- **preset 与用户状态用一段 170 行手写 merge 缝合**（`useModelProvider.ts:120`–`:225`）：MLX 平台过滤、`llama.cpp` 遗留过滤、cortex 一次性迁移开关、`deletedModels` 墓碑数组、本地模型重现时清墓碑、`_userConfiguredCapabilities` 私有标记决定 capability 以引擎还是用户为准、settings 只继承 `value` 而 `recommended`/`options` 取最新…… 这是**没有 delta 表、把 preset 和用户数据混存在同一对象里**的最终形态。
- **声明式 `ProviderSetting` 描述符被持久化了**，markdown 文案 / placeholder / options 这些纯展示数据进了用户存储，升级时要专门做“只继承 value”的处理（`:180` 注释）。**展示元数据不该持久化。**

---

## 5. Chatbox

在“简单”与“够用”之间平衡得最好，适合作为 SkillStar 的**下限参考**。

### 5.1 数据模型

核心只有两个字段（挂在全局 `Settings` 上）：

```ts
// chatbox:src/shared/types/settings.ts:427
providers: z.record(z.string(), ProviderSettingsSchema).optional().catch(undefined),
customProviders: z.array(CustomProviderBaseInfoSchema).optional().catch(undefined),
```

内置 provider 的 base info **只在代码里**（registry），用户存储只有 `providers[providerId]` 这层 overlay；自定义 provider 的 base info 才落 `customProviders`。于是**“升级后重复插入”根本不存在**——内置 provider 从来没被写进用户数据。这是最省事的解法。

```ts
// chatbox:src/shared/types/settings.ts:66  ProviderSettingsSchema
apiKey, apiHost, apiPath, models[], excludedModels[], useProxy,
oauth: OAuthCredentialsSchema.optional(),                    // :75
activeAuthMode: z.enum(['apikey','oauth']).optional(),       // :77
endpoint, deploymentName, dalleDeploymentName, apiVersion,   // azure（平铺）
accessKey, secretKey, sessionToken, region                   // bedrock（平铺）
```

**OAuth 与 API Key 共存的建模代价被明码标价**：一个 `oauth` 子对象 + 一个 `activeAuthMode` 开关，其余异形凭据平铺。没有判别联合，代价是 schema 随 provider 数量线性变胖，且类型上无法阻止“bedrock provider 填了 deploymentName”。对个人桌面应用这个代价可接受。

内置/自定义用 `isCustom` 判别联合（`:92`/`:110`）：内置 id 是 `z.nativeEnum(ModelProviderEnum)`，自定义是 `custom-provider-${uuidv4()}`（`components/settings/provider/AddProviderModal.tsx:25`），天然不冲突；导入配置时还有显式防撞检查（`importProviderState.ts:60` 抛 `conflicts with a builtin provider ID`）。

**每个字段都 `.catch()`**：`.catch(undefined)` / `.catch([])` / `.catch(14)` 遍布整个 schema，**单个字段解析失败不会让整份配置失效**，退化粒度是字段级。

**模型目录：编译期快照 + 运行时刷新，三级回退。**

```ts
// chatbox:src/shared/model-registry/snapshot.generated.ts:1
// Auto-generated by scripts/generate-model-snapshot.ts  Source: https://models.dev/api.json
export const MODELS_DEV_SNAPSHOT: ModelRegistryData = { ... }   // 7125 行

// chatbox:src/shared/model-registry/enrich.ts:26
function getRegistry() { return runtimeRegistry ?? MODELS_DEV_SNAPSHOT }
```

回退链：runtime（用户拉过的最新 models.dev）→ 编译期快照 → provider 的 `defaultSettings.models` 手写清单。离线有数据、联网会更新、不依赖 provider 的 `/models`（大多数 `/models` 只返回 id，不返回 context window）。

**元数据合并显式声明谁赢**（`enrich.ts:85`，注释在 `:66`）：

```ts
capabilities / contextWindow / maxOutput:  registry 覆盖（客观事实，registry 更权威）
nickname / type:                            只在缺失时填（用户可能定制过）
```

模型 id 匹配支持**最长前缀匹配 + 边界字符校验**（`findModelInRegistry`, `:35`）：前缀后必须是 `-`/`:`/`.` 或结尾，避免 `gpt-4` 匹配到 `gpt-4o`；对微调 id（`gpt-4o:ft-xxx`）很有用。

provider 定义里同时有 `modelsDevProviderId` 和 `curatedModelIds`（`src/shared/providers/types.ts:59`/`:67`）：**默认精选 + 按需发现**——models.dev 里有但不在精选列表的算 "discovered"，点 Fetch Models 时按 `release_date` 过滤后单独展示，避免默认把 300 个模型糊进选择器。

**迁移**：线性版本链 15 版（`src/renderer/stores/migration.ts:65` `CurrentVersion = 15`，`:197` 起 `migrate_0_to_1` … `migrate_14_to_15`，每步可返回 `needRelaunch`；首次运行直接设成当前版跳过全部迁移，`:177`）。失败时用 WeakMap 附加 `{configVersion, targetConfigVersion}` 再抛出（`stores/migration-error.ts:8`），报错能定位到具体哪一步。

`migrate_9_to_10` 是典型的**扁平字段 → 结构化表**迁移（pre-v10 把 `openaiKey`/`claudeApiKey`/`geminiAPIHost`/`aiProvider`/`model` 都平铺在 settings 顶层）。关键是它被抽成**纯函数**放在 shared 层：

```ts
// chatbox:src/shared/migration/legacy-provider-settings.ts:101
export function migrateLegacyProviderSettings(oldSettings: LegacyFlatSettings): MigratedProviderSettings
// 头注释：Pure: no I/O, no platform deps —— 让 web 和 native 两条迁移路径复用同一份逻辑
// :72 LEGACY_PROVIDER_MODEL_KEYS：旧版本里"哪个字段存的是选中模型"的映射表
```

### 5.2 UI 形态

**左列表 + 右详情。** 排序是派生的、不可拖拽（`ProviderList.tsx:36`）：ChatboxAI 置顶 → 已激活的 + 自定义的 → `FEATURED_PROVIDER_IDS` 推荐 preset。“已激活”不是字段而是推导：

```ts
// chatbox:src/renderer/hooks/useProviders.ts:33
(!p.isCustom && (apiKey || isUsingOAuth(...) || (Bedrock && accessKey && secretKey)))
|| ((p.isCustom || Ollama || LMStudio) && models?.length)
```

好处：没有 enabled 字段要维护，配了就出现（左列表用绿色 `Indicator` 小圆点标记，`:105`）。坏处：**无法在保留 key 的前提下临时停用 provider**，且判定逻辑随 provider 种类线性变复杂。

**模型清单交互。** 主区是当前列表（增删改），旁边 **Reset**（恢复 `defaultSettings.models`）和 **Fetch** 两个按钮。Fetch 的结果**不直接覆盖**，而是开 Modal 展示远端清单（带搜索，每行按是否已在本地显示"添加/移除"，`$providerId.tsx:989`–`:1006`）。**“拉取”与“采纳”分离**避免一次 Fetch 冲掉用户精心整理的列表。Fetch 前若有 `modelsDevProviderId` 会先 `forceRefreshRegistry()`（`:353`）。手动添加走 `NiceModal.show('model-edit')`（`:317`），重复 id toast "already existed"。

**连通性检查是真实能力探测**：

```ts
// chatbox:src/renderer/utils/model-tester.ts:50  testModelCapabilities
// 三步渐进，每步完成即 onStateChange：basicTest（文本）→ visionTest（1x1 png base64, :28）
// → toolTest（get_weather 假工具, :31）；basicTest 失败就不再往下测
// 结果 { testing, basicTest, visionTest, toolTest }，每项 'success'|'error'|'pending'
```

这比“通/不通”有用得多：**同时校验了用户标注的 capabilities 是否属实**。按钮禁用文案也分情况（`$providerId.tsx:684`）：`API Key is required to check connection` / `Add at least one model to check connection`。

**按用途分配模型**：`Settings` 上一排独立字段（`settings.ts:450` 附近）`titleGenerationModel` / `searchTermConstructionModel` / `ocrModel` / `defaultEmbeddingModel` / `defaultRerankModel`，形状统一是 `{ provider, model }`。另有 `favoritedModels` 做跨 provider 收藏（`useProviders.ts:60`），选择器里单独一栏。模型选择器（`ModelSelectorV2/`）按 provider 分组可折叠、带搜索和收藏分组，还有一条 "More Providers / BYOK" 分隔条把官方托管和自带 key 的分开（`GenericProviderRows.tsx:11`）。

**保存策略：纯 autosave，无 debounce。** 每次 `onChange` 直接 `setProviderSettings({...})`（`$providerId.tsx:290` 起约 15 处），浅合并（`stores/providerSettings.ts:6`）。没有脏态、没有校验、没有失败处理——最简单的一版。

### 5.3 值得偷的点

内置 preset 不入用户存储、只存 overlay（一举消灭“升级重复插入”）；`custom-provider-{uuid}` + `isBuiltinProviderId` 防撞；每字段 `.catch()` 字段级退化；模型目录三级回退；元数据合并显式声明谁赢；`curatedModelIds` 精选 + 按 release_date 发现；Fetch 结果进 Modal 逐个采纳；能力探测式连通性检查；迁移逻辑抽成纯函数放 shared 层。

### 5.4 明显的坑

- **没有 enabled 字段**，“已激活”靠推导（`useProviders.ts:33`），无法保留 key 的同时停用，判定逻辑随 provider 类型膨胀。
- **`ProviderSettingsSchema` 是所有 provider 字段的并集**（azure 4 个 + bedrock 4 个平铺），类型上无法约束归属。代码里自留 TODO：`// TODO: provider的 base info 和 settings混在一起了`（`settings.ts:576`）。
- **`$providerId.tsx` 1241 行**：OAuth 三种 flow（callback / code-paste / device-code）、api key、api host、azure、bedrock、模型列表、模型测试、代理开关全塞一个文件。
- **零校验的 autosave**：base URL 填错、key 带空格都没有即时反馈。

---

## 6. AnythingLLM

价值几乎全在反面。

### 6.1 数据模型

**没有 provider 表**，全部配置是扁平 env 变量，通过一张手写映射表读写：

```js
// anything-llm:server/utils/helpers/updateENV.js:4   （整个文件 1546 行）
const KEY_MAPPING = {
  LLMProvider:     { envKey: 'LLM_PROVIDER',      checks: [isNotEmpty, supportedLLM] },
  OpenAiKey:       { envKey: 'OPEN_AI_KEY',       checks: [isNotEmpty, validOpenAIKey] },
  OpenAiModelPref: { envKey: 'OPEN_MODEL_PREF',   checks: [isNotEmpty] },
  AnthropicApiKey: { envKey: 'ANTHROPIC_API_KEY', checks: [isNotEmpty, validAnthropicApiKey] },
  GenericOpenAiBasePath: { ... },   // :204
  ... }
```

**同一时刻只有一个活跃 LLM provider**（`LLM_PROVIDER` 单值）。要接两个中转站？只能选一个。

前端目录是手写数组，每条挂一个**专属 React 组件**：

```jsx
// anything-llm:frontend/src/pages/GeneralSettings/LLMPreference/index.jsx:107
{ name: 'OpenAI', value: 'openai', logo: OpenAiLogo,
  options: (settings) => <OpenAiOptions settings={settings} />,
  requiredConfig: ['OpenAiKey'] },                        // :111
{ name: 'Generic OpenAI', value: 'generic-openai',
  requiredConfig: ['GenericOpenAiBasePath', 'GenericOpenAiModelPref'],
  connectionConfig: ['GenericOpenAiBasePath'] },          // :425
```

`frontend/src/components/LLMSelection/` 下有 **40+ 个手写的 `XxxOptions/index.jsx`**。加一个 provider = 加一个组件 + 加 N 个 KEY_MAPPING 条目 + 加一段 import。`requiredConfig` 是唯一的抽象，用来判断该 provider 是否算配置完成。

### 6.2 UI 形态

单页：搜索框 + provider 单选卡片列表（`LLMItem`）+ 选中后在下方渲染专属选项区 + 一个 Save 按钮。

**保存是 FormData 裸抓 + 显式提交**：

```jsx
// anything-llm:frontend/src/pages/GeneralSettings/LLMPreference/index.jsx:452
const formData = new FormData(e.target)
for (var [key, value] of formData.entries()) data[key] = value   // 抓表单里所有 name 属性
const { error } = await System.updateSystem(data)
setHasChanges(!!error)     // :467  成功清脏态，失败保持脏态（这条是对的）
```

脏态靠一个**全局 DOM 自定义事件**：

```jsx
// :439 export const LLM_PREFERENCE_CHANGED_EVENT = 'llm-preference-changed'
// :496 注释：Some more complex LLM options do not bubble up the change event,
//      so we need to listen to the custom event we can emit from the LLM options component
```

即子组件的受控控件不冒泡 change，只好手动 `window.dispatchEvent` 通知父组件“我脏了”。校验全在服务端 `KEY_MAPPING[key].checks`（`updateENV.js:1370`），前端只做必填。

### 6.3 值得偷的点

`requiredConfig: string[]` 声明“配置完成”的判据；`checks: [isNotEmpty, validOpenAIKey]` 声明式校验链与字段定义放在一起；保存失败保留脏态。

### 6.4 明显的坑

- **单活跃 provider** 是根本性的能力缺失。
- **N 个 provider = N 个 React 组件 + N×M 个 env 键**，零复用；provider 超过 10 个就失控（现在 40+）。
- **FormData 裸抓 + 全局 DOM 事件传脏态**：绕过 React 数据流，任何嵌套控件漏一次 `dispatchEvent` 就是“改了但保存按钮不亮”的隐性 bug。
- **配置存 env**：无法表达数组、无法表达同一 provider 多实例、无法做 schema 版本迁移。

---

## 7. 给 SkillStar 的可迁移结论

SkillStar 的独特约束（六个项目都没有）：Tauri v2，Rust 持有文件系统与密钥、前端只能 `invoke()`；Models 工作台除了自用还要**写盘同步**到 Claude Code / Claude Desktop / Codex CLI / OpenCode / Pi / OMP 六个外部 Agent 的真实配置文件；有余额与延迟探测两个别人都没有的维度。

第二条意味着 provider 记录**必须能无损映射到 6 种外部格式**，因此 endpoint / 认证方式 / 模型 id 的建模要比 Chatbox 精确，但不必到 Cherry Studio 的 registry 生成管线那种程度。

### 7.1 强烈建议采纳

1. **preset 只在代码里，用户存储只存 overlay/delta。**
   `chatbox:src/shared/types/settings.ts:427`（`providers: Record<id, ProviderSettings>`）、`cherry-studio:src/main/data/db/schemas/userModel.ts:36`（null = 继承）。同时解决“升级后重复插入”和“preset 更新用户拿不到”。反例：`jan:web-app/src/hooks/useModelProvider.ts:120` 那段 170 行 merge。

2. **稳定 id：从不用位置、不用名字。**
   内置用固定 slug；自定义用 `custom-provider-{uuid}`（`chatbox:.../AddProviderModal.tsx:25`）并在导入时校验不撞内置 id（`chatbox:.../importProviderState.ts:60`）。反例：`open-webui:backend/open_webui/routers/openai.py:300`（下标即身份）、`jan:web-app/src/routes/settings/providers/index.tsx:49`（名字即身份，无法重命名）。

3. **模型行主键用确定性复合 id `providerId::modelId`。**
   `cherry-studio:src/main/data/db/schemas/userModel.ts:36`。幂等 upsert、跨设备同步、外键引用都受益。

4. **凭据永不进前端 DTO。**
   `cherry-studio:src/shared/data/types/provider.ts:91`（`RuntimeApiKeySchema = ApiKeyEntrySchema.omit({ key: true })`）、`:326`。SkillStar 对应：Tauri 命令返回 `{ id, label, isEnabled, masked }`，明文只在 Rust 侧。

5. **密钥存储抄 Jan 的 Rust 实现。**
   `jan:src-tauri/src/core/server/provider_secrets.rs:1`（keyring 优先 + AES-256-GCM 文件回退 + `0600` + 原子 rename）、`:44`（`KEYRING_DOWN` latch）、`:52`（`NoEntry` 不算基础设施失败）。配套铁律：`jan:src-tauri/src/core/server/remote_provider_commands.rs:127` —— **停用/注销 provider 绝不能删持久化凭据**。

6. **认证方式用集合 + 判别联合，不用单一枚举。**
   `cherry-studio:packages/provider-registry/src/schemas/provider.ts:181`（`authMethods: ('api-key'|'oauth'|'external-cli')[]`，“登录型”是派生的 `!includes('api-key')`）、`:190`（`authOptional` 与登录型区分）；用户侧 `cherry-studio:src/shared/data/types/provider.ts:150` 的判别联合。SkillStar 尤其需要 `'external-cli'` 这一档——Claude Code 的凭据就在外部 CLI 的 store 里。Rust 天然适合 `enum AuthConfig { ApiKey{..}, OAuth{..}, ExternalCli{..} }`。

7. **模型能力元数据用“编译期快照 + 运行时刷新 + 手写默认”三级回退。**
   `chatbox:src/shared/model-registry/enrich.ts:26`、`chatbox:src/shared/model-registry/snapshot.generated.ts:1`。合并规则要显式写死谁赢（`enrich.ts:85`）：capabilities/contextWindow/maxOutput 由 registry 覆盖，nickname/type 只在缺失时填。

8. **“拉取模型”与“采纳模型”分离。**
   `chatbox:src/renderer/routes/settings/provider/$providerId.tsx:989`（Fetch 结果进 Modal 逐个添加/移除）；对比 `cherry-studio:.../useProviderModelSync.ts:71` 只补不删。绝不要让一次 Fetch 冲掉用户整理过的列表。

9. **模型退役用标记而非删除。**
   `cherry-studio:src/main/data/db/schemas/userModel.ts:98`（`isDeprecated`）、`lobe-chat:packages/model-bank/src/types/aiProvider.ts:433`（`modelRedirects` 映射到后继）。SkillStar 的历史会话和写盘同步都会引用 modelId，硬删会留下悬空引用。

10. **连通性检查做能力探测，不只做“通不通”。**
    `chatbox:src/renderer/utils/model-tester.ts:50`（basic → vision → tool 渐进，前步失败即停）。额外价值：顺带校验 capabilities 标注是否属实，写盘同步时才不会把错误能力信息带给外部 Agent。轻量版（`GET /models`，`open-webui:backend/open_webui/routers/openai.py:796`）适合“保存后自动后台探测”，重版适合用户手动点的“完整检测”。

11. **错误状态至少分 401 / 403 / 429 / 网络错误四态。**
    `jan:web-app/src/routes/settings/providers/$providerName.tsx:394`（四种文案）、`:410`（可恢复的黄色、未知的红色）。错误文案取值优先用 provider 响应体而非 HTTP statusText：`cherry-studio:.../utils/healthCheck.ts:20`。

12. **endpoint 完整 URL 实时预览。**
    `cherry-studio:.../ProviderList/customProviderCreation.ts:83`；轻量版 `open-webui:src/lib/components/admin/Settings/Connections/OpenAIConnection.svelte:47`。SkillStar 更需要——同一个 base URL 写盘到 6 个 Agent 时各家拼接规则不同，预览能提前暴露问题。

13. **多 key 语义先做“故障转移链”，不做轮询负载均衡。**
    `jan:web-app/src/lib/provider-api-keys.ts:18`（primary + fallbacks 有序去重）、`jan:src-tauri/.../remote_provider_commands.rs:32`；条目形态参考 `cherry-studio:src/shared/data/types/provider.ts:79`（`{id, key, label?, isEnabled}`）。**六个项目里没有一个实现了真正的轮询负载均衡**——这是个信号。

14. **配置解析做字段级容错，不做整份失败。**
    `chatbox:src/shared/types/settings.ts`（几乎每字段 `.catch(...)`）。Rust 侧对应：`#[serde(default)]` + 可疑字段用 `Option<T>` + 自定义 deserializer 吞错，避免一个坏字段让用户所有 provider 消失。

15. **迁移逻辑抽成纯函数并单测。**
    `chatbox:src/shared/migration/legacy-provider-settings.ts:101`（`Pure: no I/O, no platform deps`，多端共用）、版本链 `chatbox:src/renderer/stores/migration.ts:65`、失败上下文 `chatbox:src/renderer/stores/migration-error.ts:8`。

### 7.2 值得考虑

1. **“一个 provider 实例 = 一个 host”，多 host 用多实例 + `presetProviderId` 关联。**
   `cherry-studio:src/main/data/db/schemas/userProvider.ts:1`（不变量）、`cherry-studio:.../ProviderList/providerGrouping.ts:20`（UI 折叠）。价值取决于“同一家开多个中转站”是否真实用例。

2. **`prefix_id` 模型 id 命名空间前缀。** `open-webui:src/lib/components/AddConnectionModal.svelte:204`。多个中转站都提供 `claude-opus-4` 时，扁平选择器里没有别的办法区分；写盘同步会放大这个问题。

3. **`model_ids` 白名单 / `excludedModels` 黑名单。** `open-webui:.../AddConnectionModal.svelte:205`、`chatbox:src/shared/types/settings.ts:71`。中转站动辄返回 300 个模型。

4. **provider 定义声明 UI 能力开关。** `lobe-chat:packages/model-bank/src/types/aiProvider.ts:206`（`showApiKey`/`showChecker`/`showModelFetcher`/`showDeployName`/`modelEditable`/`showAddNewModel`）。让 provider 数据驱动表单渲染，而不是组件里写 `if (id === 'ollama')`。

5. **fractional index 排序键。** `cherry-studio:src/main/data/db/schemas/_columnHelpers.ts:68`、`:91`。只有确定要做拖拽排序才值得，否则整数 `sort` 够用。

6. **按用途分配模型：独立 preference key + 单向级联。** `cherry-studio:src/renderer/hooks/useModel.ts:29`/`:48`/`:23`（painting 因需要图像模型故意不参与级联）。

7. **上游硬限制前置为本地校验。** `lobe-chat:packages/model-bank/src/types/aiProvider.ts:158`/`:165`。

8. **保存失败保留脏态而非静默丢弃。** `cherry-studio:.../useProviderApiKey.ts:161`、`anything-llm:.../LLMPreference/index.jsx:467`。

9. **校验失败时自动展开对应折叠区。** `open-webui:src/lib/components/AddConnectionModal.svelte:158`。

10. **保存策略分层：连接字段 autosave，结构化对象编辑用 Modal + 显式保存。**
    LobeChat 就是这么分的（`lobe-chat:.../ProviderConfig/index.tsx:273` autosave vs `ModelConfigModal` 显式提交）。若选 autosave，`cherry-studio:.../useProviderApiKey.ts` 是最完整参考（debounce + 脏态 + 卸载 flush + 服务端回灌协调）；若选显式保存，`open-webui:AddConnectionModal.svelte:150` 的前置校验链是好范本。
    **个人判断**：SkillStar 的 provider 表单字段少且互相独立，autosave + 150–300ms debounce + 明确的“保存中/已保存/失败”三态指示最合适；但“新建自定义 provider”应该是 Modal + 显式创建（Chatbox / Jan / Open WebUI 三家一致）。

11. **`requiredConfig: string[]` 声明“配置完成”的判据。** `anything-llm:.../LLMPreference/index.jsx:111`。比 Chatbox 那段推导式判定（`chatbox:src/renderer/hooks/useProviders.ts:33`）可维护得多。

12. **`ModelCapability.isUserSelected` 三态（用户开 / 用户关 / 默认）。** `cherry-studio:src/renderer/types/model.ts:25`。比 Jan 的 `_userConfiguredCapabilities` 私有布尔干净：能区分“用户明确关掉了 vision”和“默认没有 vision”。

### 7.3 明确不要学

1. **不要用位置下标做身份。** `open-webui:backend/open_webui/routers/openai.py:300`、`open-webui:src/lib/utils/connections.ts:66`（删除后 re-index 全表，重复实现两份）。
2. **不要用名字做 id。** `jan:web-app/src/routes/settings/providers/index.tsx:49` —— provider 无法重命名。
3. **不要把 preset 与用户数据混存在同一对象、靠加载时手写 merge 缝合。** `jan:web-app/src/hooks/useModelProvider.ts:120`–`:225`（170 行里塞了平台过滤、遗留过滤、一次性迁移开关、墓碑数组、墓碑清理、私有标记、settings 只继承 value）。
4. **不要把展示元数据（title/description/placeholder/options）持久化进用户配置。** `jan:web-app/src/types/modelProviders.d.ts:17` + `jan:.../useModelProvider.ts:180` 的注释：`recommended`/`options` 必须反映最新 fetch。
5. **不要每个字段一套 ad-hoc 空值继承规则。** `lobe-chat:packages/database/src/repositories/aiInfra/index.ts:165`，副作用是 `displayName` 无法被清空。用 `null = 继承` 统一语义（`cherry-studio:src/main/data/db/schemas/userModel.ts:50` 起）。
6. **不要把“模型特有参数”做成全局字符串枚举。** `lobe-chat:packages/model-bank/src/types/aiModel.ts:329`（`ExtendParamsType` 40+ 成员）。要用结构化描述（effort 取值域 + budget 范围 + wire 格式）。
7. **不要用能力开关的否定命名。** `cherry-studio:src/renderer/types/provider.ts:32`（4 个 `isNotSupportXxx` 全部 `@deprecated`，后改成 `isSupportXxx`）。
8. **不要 N 个 provider = N 个手写组件 + N×M 个扁平键。** `anything-llm:frontend/src/components/LLMSelection/`（40+ 组件）+ `anything-llm:server/utils/helpers/updateENV.js:4`（1546 行）。加一个 provider 要改 4 个地方。
9. **不要用 FormData 裸抓 + 全局 DOM 事件传脏态。** `anything-llm:.../LLMPreference/index.jsx:452` + `:439`/`:496`。
10. **不要“同一时刻只有一个活跃 provider”。** `anything-llm:server/utils/helpers/updateENV.js:5`。
11. **不要零校验的纯 autosave。** `chatbox:src/renderer/routes/settings/provider/$providerId.tsx:290` 起：base URL 打错、key 带空格都没有即时反馈。
12. **不要让 provider 详情页变成巨型文件。** `chatbox:.../$providerId.tsx`（1241 行，OAuth 三种 flow + api key + host + azure + bedrock + 模型列表 + 测试 + 代理全塞一起）。SkillStar 有 800/1000 行红线，从第一天就按 `ConnectionSettings/` + `ModelList/` + `ProviderSpecific/` 分目录（参考 `cherry-studio:src/renderer/pages/settings/ProviderSettings/` 的组织）。
13. **不要在同一字典里并存两套键。** `open-webui:backend/open_webui/routers/openai.py:304` 的 legacy 兼容行在 `:304`/`:544`/`:674` 重复三次并保留至今。要换主键就一次性迁移干净。

---

## 附：本次调研没能覆盖的

- **余额查询**：六个项目没有一个做 provider 账户余额查询。Cherry Studio 的 `reportedCostCurrency` / `reportsActualCost` 记录的是**本次请求**的实际成本，不是余额。SkillStar 这块没有可抄的先例。
- **延迟探测**：只有 Cherry Studio 在健康检查结果里带 `latency`（`cherry-studio:src/renderer/types/healthCheck.ts:26`），且是检查时的一次性测量，没有持续探测或历史曲线。
- **写盘同步到外部 Agent**：六个项目都不做，SkillStar 这条独有，无外部参考。
- Jan 因仓库体积（1.5GB）只定向抓取 15 个关键文件，未做全仓 grep；其 UI 组件层只读了 provider 相关的两个 dialog。

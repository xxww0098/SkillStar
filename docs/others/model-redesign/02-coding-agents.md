状态：research

# 编码 Agent / IDE 扩展的模型配置调研

> 六个项目的真实源码（`git clone --depth=1` 到 `/tmp/model-research/`，未修改 SkillStar 任何代码）。
> 引用格式 `项目名:path:line`，全部来自 2026-08-15 的 HEAD。
> 克隆记录见[文末](#附克隆记录)。

---

## 0. 先给结论：角色路由只有三种范式

**范式 A｜角色 → 选择（role-keyed map）。** 全局一张 `Record<Role, ModelSelection>`。
Void 的 `modelSelectionOfFeature`（`void:src/vs/workbench/contrib/void/common/voidSettingsTypes.ts:366`）、
Zed 的一组扁平 `Option<LanguageModelSelection>` 字段（`zed:crates/agent_settings/src/agent_settings.rs:214-224`）、
SkillStar 现有的 OMP `modelRoles` 都属于这类。角色独立、可同时生效、写盘直白；代价是角色多了键/字段膨胀。

**范式 B｜模型声明角色（model-declares-roles）。** 每个模型条目自带 `roles: [...]`，
运行时再从候选集里挑一个当前选中项。Continue 独此一家
（`continue:packages/config-yaml/src/schemas/models.ts:185`、`continue:core/config/yaml/loadYaml.ts:282`）。
好处是配置文件可分享；代价是必须额外维护一层「当前选中」的本地状态。

**范式 C｜角色 → 配置组 id。** 角色指向一整套 provider+model+参数的命名 profile。
Roo 的 `modeApiConfigs: Record<modeSlug, configId>`（`Roo-Code:packages/types/src/global-settings.ts:193`）、
Kilo/OpenCode 的 agent（`kilocode:packages/schema/src/agent.ts:20-31`）。复用度最高，多一层间接。

**Cline 是第四种，也是反面教材**：把角色维度编码进字段名前缀，靠字符串替换同步（见 §1.1）。

---

## 1. Cline（cline/cline）

代码在 `apps/vscode/`。

### 1.1 角色路由：plan/act 的字段前缀复制

只有 Plan / Act 两个角色，实现方式是**把每个 provider 字段复制两份**：

```ts
// cline:apps/vscode/src/shared/storage/state-keys.ts:148-160
planModeApiModelId:            { default: undefined as string | undefined },
planModeThinkingBudgetTokens:  { default: undefined as number | undefined },
planModeReasoningEffort:       { default: undefined as string | undefined },
planModeOpenRouterModelId:     { default: undefined as string | undefined },
planModeOpenRouterModelInfo:   { default: undefined as ModelInfo | undefined },
// … 一直到 planModeSapAiCoreDeploymentId / planModeOcaReasoningEffort
```

`grep -c planMode` = **44**，`actMode` 也 44。469 行文件里近 90 行是机械复制。两侧同步靠字符串替换：

```ts
// cline:apps/vscode/src/core/controller/models/updateApiConfiguration.ts:43-51
function getAlternateModeField(fieldName: string): string | null {
	if (fieldName.startsWith("planMode")) return fieldName.replace("planMode", "actMode")
	if (fieldName.startsWith("actMode"))  return fieldName.replace("actMode", "planMode")
	return null
}
```

`planActSeparateModelsSetting`（默认 `false`，`state-keys.ts:270`）为假时，每次写入都镜像到对侧
（`updateApiConfiguration.ts:119-131`）——**这是写时双写，不是读时回落**。

- 角色挂在：全局，且以字段名形式硬编码进 schema。
- 回落链路：无。`undefined` 就用 provider 默认模型。
- 配置组：**完全没有 profile 概念**，想切几套配置只能反复手改表单。

### 1.2 数据模型

`ModelInfo`（`cline:apps/vscode/src/shared/api.ts:70-99`）是唯一能力载体：
`maxTokens / contextWindow / supportsImages / supportsPromptCache / supportsReasoning /
inputPrice / outputPrice / cacheWritesPrice / cacheReadsPrice / tiers[] / apiFormat`，
外加一个嵌套的 `thinkingConfig{maxBudget, outputPrice, outputPriceTiers, geminiThinkingLevel, supportsThinkingLevel}`。
OpenAI 兼容 provider 用子类型多加 4 个字段
（`OpenAiCompatibleModelInfo{systemRole, supportsReasoningEffort, supportsTools, supportsStreaming}`，`api.ts:101-107`）。
注意 `supportsPromptCache: boolean` 类型里就带注释 `// this value is hardcoded for now`。

`ApiProvider` 是 **52 个字面量的联合类型**（`api.ts:4-53`）。自定义 provider 不进这个联合，走双重否定兜底：

```ts
// cline:apps/vscode/webview-ui/src/components/settings/ApiOptions.tsx:104-112
// These are edited through the OpenAI-compatible form so they always get Base URL,
// Custom Headers, Model Configuration and Reasoning Effort sections
const isCustomProvider = !hasCustomProviderSettings(selectedProvider) && !isKnownGenericProvider(selectedProvider)
```

元数据三条路混用：**远端目录**（一组 `refresh*Models` controller）→ **硬编码 sane defaults**
（`openAiModelInfoSafeDefaults`，`api.ts:172-180`；`clinePassModelInfoSaneDefaults`，`:139-150`）→
**模型 id 后缀匹配**（`getModelSlug()` 取 `/` 后最后一段 + `buildModelInfoNameMap()` 映射，`api.ts:154-170`）。

### 1.3 UI

Provider 选择器是自绘可搜索下拉（Fuse.js + 键盘导航），数据源是**运行时 catalog 而非静态数组**，
所以自定义 provider 自动出现（`ApiOptions.tsx:121-127`）。

模型选择器分两形态。小列表用原生 `VSCodeDropdown`，附带一个值得抄的 hack：VSCodeDropdown 有 bug
（动态 option 不自动选中 `value`），解决办法是把 selection + 全部 id 拼成 `key` 强制重挂载
（`cline:.../settings/common/ModelSelector.tsx:50-51`）。
大列表（OpenRouter 数百模型）用 Fuse.js + 高亮 + **收藏永久置顶**
（`OpenRouterModelPicker.tsx:139-163`：`threshold: 0.6, includeMatches: true`，
`return [...favoritedModels, ...searchResults]`），且 Enter 时若无高亮项**直接把输入串当自定义模型 id 提交**（`:181-186`）。

**最值得偷的组件是 `ModelPickerWithManualEntry`**（145 行，`cline:.../settings/providers/ModelPickerWithManualEntry.tsx`），
它把「下拉选 + 手填」的边界状态一次收齐：

```tsx
// :75-83 三种非正常态各有 role 语义
{isStale   && <div role="status">Model list may be stale for the current provider configuration.</div>}
{isLoading && <div role="status">Loading models…</div>}
{error     && <div role="alert">{error}</div>}

// :100-111 已保存但不在列表的模型 → 保留成显式选项，绝不静默清空
{!selectedModelInList && allowsCustomIds && selectedModel.modelId && (
  <VSCodeOption value="">{selectedModel.modelId} (not in current list)</VSCodeOption>)}
{allowsCustomIds && <VSCodeOption value="__custom__">Use custom model ID…</VSCodeOption>}

// :53-56 手填框自动展开的五个触发条件
const showManualEntry = allowsCustomIds &&
  (isManualEntryVisible || !hasModels || isLoading || Boolean(error) || !selectedModelInList)
```

错误呈现按类型分支到专用组件而非塞原始文本（`cline:.../chat/ErrorRow.tsx:47-115`）：
`Balance`（带 `currentBalance` 与充值入口）、`SpendLimit`、`Entitlement`、`ClinePassLimit`、
`ClineFreeModelLimit`、`RateLimit`（带 Request ID）、`QuotaExceeded`、`Auth`。

### 1.4 值得偷 / 明显的坑

**偷**：`ModelPickerWithManualEntry` 的完整状态矩阵；「不在列表」保留为显式选项；
大列表 Fuse + 收藏置顶 + Enter 落自定义 id；provider 列表来自运行时 catalog；错误按类型分支且额度错误带余额数字。

**坑**：
① `planMode*`/`actMode*` 字段复制是灾难——加 provider 写 2 份，加第三个角色写 3 份，
`getAlternateModeField` 的前缀替换是类型系统完全看不见的耦合。
② 写时双写让落盘状态分不清「显式设成一样」和「继承默认」，打开分离开关时历史值已被覆盖。
③ 52 个字面量联合 + 40 多个专用 provider 表单组件，自定义 provider 靠双重否定识别。

---

## 2. Roo Code（RooCodeInc/Roo-Code）

### 2.1 角色路由：profile 一等公民，角色存 profile **id**

```ts
// Roo-Code:src/core/config/ProviderSettingsManager.ts:27-40
export const providerProfilesSchema = z.object({
	currentApiConfigName: z.string(),
	apiConfigs:      z.record(z.string(), providerSettingsWithIdSchema),  // Record<name, {id, …}>
	modeApiConfigs:  z.record(z.string(), z.string()).optional(),         // Record<modeSlug, profileId>
	migrations: z.object({ /* rateLimitSecondsMigrated 等 5 个布尔 */ }).optional(),
})
```

另有两个不走 mode 的角色 pointer，同样指向 profile id（`Roo-Code:packages/types/src/global-settings.ts:193-208`）：
`enhancementApiConfigId`（「增强提示词」用哪套配置）、`profileThresholds: Record<profileId, number>`（每 profile 的压缩阈值）。
**角色指 id 而不是 name，所以重命名 profile 不断链。**

回落是三档，全程不报错（`Roo-Code:src/core/webview/ClineProvider.ts:1302-1334`）：

```ts
if (savedConfigId) {
  const profile = listApiConfig.find(({ id }) => id === savedConfigId)
  if (profile?.name) {
    const fullProfile = await this.providerSettingsManager.getProfile({ name: profile.name })
    // CLI 场景会有只含 id/name 的空壳 profile，激活它会清空可用配置
    const hasActualSettings = !!fullProfile.apiProvider
    if (hasActualSettings) await this.activateProviderProfile({ name: profile.name })
    // else: The task will continue with the current/default configuration.
  }
} else {
  // 该 mode 从没配过 → 把当前 profile 自动钉住（learn-by-use）
  const config = listApiConfig.find((c) => c.name === currentApiConfigNameAfter)
  if (config?.id) await this.providerSettingsManager.setModeConfig(newMode, config.id)
}
```

迁移时用当前 profile 给所有 mode 播种，**永不出现「角色未配置」的空态**
（`ProviderSettingsManager.ts:108-117`：`Object.fromEntries(modes.map((m) => [m.slug, seedId]))`）。

`profileThresholds` 用 **`-1` 作为「继承全局」哨兵，非法值也静默回落**
（`Roo-Code:src/core/context-management/index.ts:180-190`）。

**⚠️ 语义差异**：Roo 的 per-mode profile 不是并行路由表，而是「每个 mode 最后用过的 profile 的记忆」——
切 mode 会真正改写全局 `currentApiConfigName`，两个 mode 不能同时生效。这和 Void/Zed/OMP 不是同一个模型。

### 2.2 数据模型

`ProviderSettings` 是**扁平的按 provider 加前缀的大对象**（639 行）：

```ts
// Roo-Code:packages/types/src/provider-settings.ts:174-207, 237-248
const baseProviderSettingsSchema = z.object({          // 公共基座
	modelTemperature: z.number().nullish(), rateLimitSeconds: z.number().optional(),
	consecutiveMistakeLimit: z.number().min(0).optional(),
	enableReasoningEffort: z.boolean().optional(),      // ← reasoning 存成两个独立字段
	reasoningEffort: reasoningEffortSettingSchema.optional(),
	modelMaxTokens: z.number().optional(), modelMaxThinkingTokens: z.number().optional(),
	verbosity: verbosityLevelsSchema.optional(),
})
const openRouterSchema = baseProviderSettingsSchema.extend({
	openRouterApiKey: z.string().optional(), openRouterModelId: z.string().optional(),
	openRouterBaseUrl: z.string().optional(), openRouterSpecificProvider: z.string().optional() })
const openAiSchema = baseProviderSettingsSchema.extend({
	openAiBaseUrl: …, openAiApiKey: …, openAiModelId: …,
	openAiCustomModelInfo: modelInfoSchema.nullish(),   // ← 自定义 provider 的能力元数据由用户填
	openAiHeaders: z.record(z.string(), z.string()).optional() })
```

补救手段是另维护一份 discriminated union，**只在导出时剥掉其他 provider 的残留字段**
（`ProviderSettingsManager.ts:508-513`：`// Avoid leaking properties from other active providers.`）。

provider 分三类决定模型清单来源（`provider-settings.ts:37-54`）：
`dynamicProviders`（openrouter / vercel-ai-gateway / litellm / requesty / unbound / poe，远端拉）、
`localProviders`（ollama / lmstudio，本地探测）、其余 static（硬编码在 `packages/types/src/providers/*.ts`）。

`ModelInfo` 是六个项目里**最细的能力元数据**（`Roo-Code:packages/types/src/model.ts:72-148`），
除常规字段外有：`promptCacheRetention: "in_memory"|"24h"`、`supportsVerbosity`、
`supportsReasoningBudget`、`supportsReasoningBinary`（只有开/关）、`supportsTemperature`、
`requiredReasoningBudget`、`requiredReasoningEffort`、`preserveReasoning`、
`longContextPricing{thresholdTokens, inputPriceMultiplier, …}`、`deprecated`、`isFree`、
`excludedTools`/`includedTools`、`tiers[]`。最关键的一条：

```ts
// model.ts:91-93 —— 能力字段本身承载「支持哪些取值」，不只是 boolean
supportsReasoningEffort: z
  .union([z.boolean(), z.array(z.enum(["disable","none","minimal","low","medium","high","xhigh"]))])
  .optional(),
```

**reasoning effort 的存储规范**写在 `ThinkingBudget.tsx:1-33` 的文件头注释里，可以直接当规范抄：

```
- modelInfo.supportsReasoningEffort: true → UI 显示 ["low","medium","high"]；array → 精确显示给定值
- "disable": enableReasoningEffort = false；持久化 reasoningEffort = "disable"；请求里完全省略 reasoning 段
- "none":    enableReasoningEffort = true； 持久化 "none"；请求里带 reasoning = "none"
- 两者 UI 上都显示 "None"，但接线不同
- Current selection is normalized to the capability: unsupported persisted values are not shown.
```

即**「关闭思考」和「思考强度=none」是两件事**。

### 2.3 校验

保存前在 webview 侧按 provider 分支决定必填项——一个 60 分支的 `switch`
（`Roo-Code:webview-ui/src/utils/validate.ts:38-120`，如
`case "openai": if (!openAiBaseUrl || !openAiApiKey || !openAiModelId) return t("validation.openAi")`）。

模型 id 只对 dynamic provider 校验，且有一条防御条件（`validate.ts:224-246`）：

```ts
// 只有目录里确实有多个模型（说明是真拉下来的清单，不是占位）才敢报「模型不存在」
if (models && Object.keys(models).length > 1 && !Object.keys(models).includes(modelId)) {
  return i18next.t("settings:validation.modelAvailability", { modelId })
}
```

而且**刻意拆成两路避免同一错误显示两次**（`validate.ts:277-282`）：
`getModelValidationError()` 给模型选择器，`validateApiConfigurationExcludingModelErrors()` 给面板顶部错误条
（注释原文：`to prevent duplication when model errors are shown in the model selector`）。

### 2.4 UI

Profile 切换器 = `SearchableSelect` + 新增/重命名/删除三个图标按钮
（`Roo-Code:webview-ui/src/components/settings/ApiConfigManager.tsx:236-292`）。两个细节：

```tsx
options={listApiConfigMeta.map((config) => ({
  value: config.name, label: config.name,
  disabled: !isProfileValid(config),                                   // ← 无效 profile 仍显示，但不可选
  icon: !valid ? <AlertTriangle className="text-vscode-errorForeground" /> : undefined,
}))}
// :279-288 最后一个 profile 不能删，tooltip 说明原因
<Button onClick={handleDelete} disabled={isOnlyProfile} />
```

模型选择器是 Popover + cmdk combobox（`ModelPicker.tsx:206-240`），两个细节：
**已选中的 deprecated 模型永远可见**（`:118-124`：`if (modelId === selectedModelId) return true`）、
关闭 popover 后延迟 100ms 清搜索词避免动画中列表跳变（`:163-166`）。
设置面板另有全局搜索（`SettingsSearch.tsx` / `useSettingsSearch.ts`）和 `simplifySettings` / `hidePricing`
两个降密度 prop（`ModelPicker.tsx:56-57`）。

### 2.5 值得偷 / 明显的坑

**偷**：profile 一等公民且角色存 **id**；迁移时给所有角色播种；未配置角色自动钉住当前（learn-by-use）；
`-1` 哨兵表示继承全局；无效 profile 保留但 disabled + 警告图标；最后一个 profile 不能删；
已选中的废弃模型永远可见；校验拆两路避免重复；`supportsReasoningEffort: boolean | string[]`；
导出时用 discriminated union 剥离残留字段。

**坑**：
① 扁平前缀命名空间（`openRouterApiKey`/`openAiApiKey`/`ollamaModelId`/…），639 行 schema，
加一个 provider 要改 schema + 校验 switch + `modelIdKeysByProvider` 映射三处。
② per-mode profile 是「记忆」不是「路由」，两 mode 不能同时生效，与用户「plan 用便宜、act 用强、
同一任务内自动切」的预期不符。
③ profile 的键是 name 值里又有 id，代码里到处 `find(({id}) => id === x)?.name` 再 `getProfile({name})` 绕圈。

---

## 3. Continue（continuedev/continue）

### 3.1 角色路由：模型声明角色 + 本地选中态

```ts
// continue:packages/config-yaml/src/schemas/models.ts:23-32, 185
export const modelRolesSchema = z.enum([
  "chat", "autocomplete", "embed", "rerank", "edit", "apply", "summarize", "subagent" ])
// baseModelFields 里：
roles: modelRolesSchema.array().optional(),                    // ← 角色是模型上的字段
capabilities: modelCapabilitySchema.array().optional(),
defaultCompletionOptions: completionOptionsSchema.optional(),
promptTemplates: promptTemplatesSchema.optional(),
autocompleteOptions: autocompleteOptionsSchema.optional(),
```

加载时反转成「角色 → 候选列表」，共 8 个手写 `if`（`continue:core/config/yaml/loadYaml.ts:280-336`）：

```ts
const defaultModelRoles: ModelRole[] = ["chat", "summarize", "apply", "edit"]
model.roles = model.roles ?? defaultModelRoles   // Default to all 4 chat-esque roles if not specified
if (model.roles?.includes("chat")) continueConfig.modelsByRole.chat.push(...llms)
// … summarize / apply / edit / autocomplete / embed / rerank / subagent
```

**「当前选中哪个」是另一层状态，存在本地 GlobalContext 按 profileId 分桶，不进配置文件。**
这是六个项目里最完整的回落链路（`continue:core/config/selectedModels.ts:9-80`）：

```ts
for (const role of roles) {
  const currentSelection = currentForProfile[role] ?? null
  // 1) 记住的 title 还在候选里 → 用它
  if (currentSelection) {
    const match = continueConfig.modelsByRole[role].find((m) => m.title === currentSelection)
    if (match) newModel = match
  }
  // 2) 否则回落到候选列表第 0 个
  if (!newModel && continueConfig.modelsByRole[role].length > 0) newModel = continueConfig.modelsByRole[role][0]
  if (!(currentSelection === (newModel?.title ?? null))) fellBack = true
  // 3) apply 角色额外检查配置有效性；无效就跳过（保持 undefined）
  if (role === "apply" && newModel?.getConfigurationStatus() !== LLMConfigurationStatuses.VALID) continue
  configCopy.selectedModelByRole[role] = newModel
}
// 4) 发生过回落 → 回写持久层，不让 UI 显示一个已不成立的选择
if (fellBack) globalContext.update("selectedModelsByProfileId", { …, [profileId]: … })
```

Continue 的 profile 粒度是**整个 config.yaml / Hub assistant**，而且校验错误挂在 profile 上
（`continue:core/config/ProfileLifecycleManager.ts:18-26`：`errors: ConfigValidationError[] | undefined`）。
比 Roo 的 profile（单套 provider 设置）粗得多。

### 3.2 数据模型与能力元数据

provider 是**自由字符串** `provider: z.string()`，不是枚举。自定义 OpenAI 兼容 provider 就是
`provider: openai` + `apiBase: <任意 URL>`，跟内置走同一条 schema，**没有第二套类型**。

能力元数据在独立包 `@continuedev/llm-info`：硬编码表 + **正则匹配模型 id**：

```ts
// continue:packages/llm-info/src/types.ts:19-40
export interface LlmInfo {
  model: string; displayName?: string; contextLength?: number; maxCompletionTokens?: number
  regex?: RegExp                    // ← 按模型 id 正则匹配
  mediaTypes?: MediaType[]          // 不写就当纯文本
  recommendedFor?: UseCase[]        // "chat" | "autocomplete" | "rerank" | "embed"
  extraParameters?: Parameter[]     // provider 特有的额外必填参数
}
// continue:packages/llm-info/src/index.ts:41-60
export function findLlmInfo(model: string, preferProviderId?: string) {
  if (preferProviderId) {                        // 1) 先按指定 provider 精确匹配
    const info = allModelProviders.find((p) => p.id === preferProviderId)
      ?.models.find((llm) => (llm.regex ? llm.regex.test(model) : llm.model === model))
    if (info) return { ...info, provider: preferProviderId }
  }
  return allLlms.find((llm) => (llm.regex ? llm.regex.test(model) : llm.model === model))  // 2) 再跨 provider
}
```

正则用得很务实（`packages/llm-info/src/providers/openai.ts:84-159`）：
`/^gpt-5$/`、`/gpt-5-codex/`、`/^gpt-4\.1$/`、`/gpt-4\.1-mini/`、`/codex-mini/`。
**「先按 provider 精确、失败再跨 provider 模糊」正好对应代理商用任意模型名转发的场景。**

能力枚举留了向前兼容逃生口（`schemas/models.ts:35-44`）：
`z.union([z.literal("tool_use"), z.literal("image_input"), z.literal("next_edit"), z.string()])`，
注释指向 PR #7676。

reasoning 只有 `reasoning: boolean` + `reasoningBudgetTokens: number`
（`schemas/models.ts:47-63` 的 `completionOptionsSchema`）——**没有 effort 枚举**，
OpenAI 系的 `low/medium/high/xhigh` 表达不出来。这是 Continue 相对 Roo/Zed 的明显缺口。

### 3.3 UI：和 SkillStar 的 OMP 面板结构几乎一致

`continue:gui/src/pages/config/sections/ModelsSection.tsx`：
第一张 Card 平铺 Chat / Autocomplete / Edit（各带快捷键和 Learn more 链接，`:76-168`），
第二张 Card 是折叠区（`:170-219`）：

```tsx
<Toggle isOpen={showAdditionalRoles} title="Additional model roles" subtitle="Apply, Embed, Rerank">
  <ModelRoleRow role="apply"  description="Used to apply generated codeblocks to files" … />
  <ModelRoleRow role="embed"  description="Used to generate and query embeddings for @codebase / @docs" … />
  <ModelRoleRow role="rerank" … />
</Toggle>
```

每个角色一个 `ModelRoleSelector`，三个态（`gui/src/pages/config/components/ModelRoleSelector.tsx:69-100`）：

```tsx
{models.length === 0 ? (
  // 态 1：该角色零候选 → 按钮变成 "Setup <Role> model"，点击打开文档
  <ListboxButton onClick={() => ideMessenger.post("openUrl", setupURL)}>Setup {displayName} model</ListboxButton>
) : noConfiguredModels ? (
  // 态 2：有候选但全部配置无效 → 明确告知会回落到哪
  <span className="italic">{`No valid ${displayName} models${
    ["Chat","Apply","Edit"].includes(displayName) ? ". Using Chat model" : ""}`}</span>
) : <span>{selectedModel?.title ?? `Select ${displayName} model`}</span>}
```

**「. Using Chat model」是本次调研里唯一一处把回落链路直接写进 UI 的实现。**
选项里的无效模型也是 disabled + 具体原因（`:117-131`）：
`(Invalid config)` / `(Missing env secret)` / `(Missing API Key)`。

### 3.4 值得偷 / 明显的坑

**偷**：**两层分离——配置文件声明能力（可分享）、本地状态记录选择（不可分享）**；
回落后**回写持久层**（`fellBack` → `globalContext.update`）；不写 `roles` 默认四个聊天系角色；
`findLlmInfo()` 的 provider 优先 + 正则模糊；能力枚举留 `z.string()` 逃生口；
UI 明说回落目标；主要角色平铺 + 次要角色折叠（`Toggle` 带 subtitle 列出内容）；
无效候选 disabled + 具体原因标签。

**坑**：
① `modelsByRole` 是 8 个手写 `if`，加角色要改 3 处，且 `summarize` 已经不同步了
（`core/config/selectedModels.ts:23` 的 `// summarize not implemented yet`）。
② 没有 reasoning effort 枚举。
③ 「角色候选集」和「角色选中项」两个概念对用户不透明——UI 里选的是选中项，
候选集由 yaml 的 `roles` 决定，改 yaml 才能让某个模型出现在下拉里。

---

## 4. Zed（zed-industries/zed）

### 4.1 角色路由：扁平命名字段 + 每角色一条显式回落链

```rust
// zed:crates/agent_settings/src/agent_settings.rs:214-227
pub default_model:          Option<LanguageModelSelection>,
pub subagent_model:         Option<LanguageModelSelection>,
pub inline_assistant_model: Option<LanguageModelSelection>,
pub commit_message_model:   Option<LanguageModelSelection>,
pub thread_summary_model:   Option<LanguageModelSelection>,
pub compaction_model:       Option<LanguageModelSelection>,
pub inline_alternatives:    Vec<LanguageModelSelection>,
pub favorite_models:        Vec<LanguageModelSelection>,
pub default_profile: AgentProfileId,
pub profiles: IndexMap<AgentProfileId, AgentProfileSettings>,
pub model_parameters: Vec<LanguageModelParameters>,
```

settings.json 侧的**回落目标直接写在 doc comment 里**（`zed:crates/settings_content/src/agent.rs:246-263`）：
`/// Model to use for the inline assistant. Defaults to default_model when not specified.` ×3。

回落在读时解析，**四条不同的链，每条都有理由**（`zed:crates/language_model/src/registry.rs:459-527`）：

```rust
pub fn default_fast_model(&self, cx: &App) -> Option<ConfiguredModel> {
    let configured = self.default_model()?;
    let fast_model = configured.provider.default_fast_model(cx)?;   // ← 同 provider 内的「快模型」
    Some(ConfiguredModel { provider: configured.provider, model: fast_model })
}
pub fn inline_assistant_model(&self) -> Option<ConfiguredModel> {
    self.inline_assistant_model.clone().or_else(|| self.default_model.clone())   // 要跟主模型同质量
}
pub fn commit_message_model(&self, cx: &App) -> Option<ConfiguredModel> {
    self.commit_message_model.clone()
        .or_else(|| self.default_fast_model(cx))    // 廉价任务优先便宜
        .or_else(|| self.default_model())
}
// thread_summary_model 同上
/// Returns the configured compaction model without falling back through
/// `default_fast_model`/`default_model`. Callers that want a fallback to
/// the thread's primary model should handle `None` themselves.
pub fn compaction_model(&self) -> Option<ConfiguredModel> { self.compaction_model.clone() }
```

还有一层「用户完全没配 default」的兜底（`registry.rs:377-397`）：
`should_use_fallback` 为真时，取「第一个已认证 provider 的 `default_model()` 或 `recommended_models()[0]`」。
**用户写了 `default_model` 就绝不猜**（`:459-470`）。
每个角色变更 emit 独立事件（`registry.rs:110-116`），且**只在 `is_same_as` 判定值真变时才 emit**（`:368-374`）。

**Zed 的 profile 是「工具权限包」不是「模型包」**：`builtin_profiles` 只有 `write`/`ask`/`minimal`
（`zed:crates/agent_settings/src/agent_profile.rs:16-26`），管的是 tools + MCP 开关，
`default_model` 只是可选附属项（`:71-78`）。所以 Zed 是双轴：角色 × profile。

### 4.2 数据模型：模型+参数打包成一个值（本调研最佳答案之一）

```rust
// zed:crates/settings_content/src/agent.rs:597-606
pub struct LanguageModelSelection {
    pub provider: LanguageModelProviderSetting,
    pub model: String,
    #[serde(default)] pub enable_thinking: bool,
    pub effort: Option<String>,
    pub speed: Option<language_model_core::Speed>,
}
```

**换模型时把参数投影到新模型的能力集合上，而不是清空也不是照抄**
（`zed:crates/agent_settings/src/agent_settings.rs:289-322`）：

```rust
Some(current) => LanguageModelSelection {
    provider, model: model_name,
    enable_thinking: current.enable_thinking && model.supports_thinking(),     // 与能力求交
    effort: current.effort.clone()
        .filter(|v| model.supported_effort_levels().iter().any(|l| l.value.as_ref() == v.as_str()))
        .or_else(|| model.default_effort_level().map(|e| e.value.to_string())),  // 不支持就回落到模型默认
    speed: current.speed.filter(|_| model.supports_fast_mode()),
},
```

温度是正交机制：`model_parameters: Vec<{provider: Option<_>, model: Option<String>, temperature: Option<f32>}>`
（`settings_content/src/agent.rs:608-615`），**通配匹配 + 倒序遍历后写优先**
（`agent_settings.rs:252-267` 的 `for setting in settings.model_parameters.iter().rev()`）。

**自定义 provider 与内置 provider 共存的方式最干净**——内置是具名字段，自定义是用户命名的 HashMap 兄弟：

```rust
// zed:crates/language_models/src/settings.rs:17-34
pub struct AllLanguageModelSettings {
    pub anthropic: AnthropicSettings,
    pub anthropic_compatible: HashMap<Arc<str>, AnthropicCompatibleSettings>,   // ← 任意个自定义
    pub openai: OpenAiSettings,
    pub openai_compatible: HashMap<Arc<str>, OpenAiCompatibleSettings>,         // ← 任意个自定义
    // bedrock / deepseek / google / llama_cpp / lmstudio / mistral / ollama /
    // opencode / open_router / vercel_ai_gateway / x_ai / zed_dot_dev …
}
```

provider id 的 JSON Schema 用 `anyOf: [{enum: [内置 id…]}, {type: "string"}]`——
**内置 id 参与自动补全但不限制取值**（`zed:crates/settings_content/src/language_model.rs:620-640`，
注释：`list the builtin providers as a subset so that we still auto complete them`）。

自定义模型的能力元数据**全靠用户手填**（`settings_content/src/language_model.rs:434-459`）：
`OpenAiCompatibleAvailableModel{name, display_name, max_tokens, max_output_tokens,
max_completion_tokens, reasoning_effort, capabilities}`，其中 `capabilities` 又有
`{tools, images, parallel_tool_calls, prompt_cache_key, chat_completions, interleaved_reasoning, max_tokens_parameter}`。
**Zed 没有远端模型目录，也不做能力探测。**

### 4.3 UI

模型选择器是**一个扁平的模糊搜索列表，不是两级下拉**
（`zed:crates/agent_ui/src/language_model_selector.rs:244-305`）：

```rust
struct GroupedModels { favorites: Vec<ModelInfo>, recommended: Vec<ModelInfo>,
                       all: IndexMap<LanguageModelProviderId, Vec<ModelInfo>> }
fn entries(&self) -> Vec<LanguageModelPickerEntry> {
    // Separator("Favorite") → Separator("Recommended") → 每个 provider 一个 Separator(provider_name)
}
// :374-386 搜索候选串是 provider/model 拼接，所以打 "ol" 能命中 ollama 的所有模型（测试见 :748-752）
StringMatchCandidate::new(index, &format!("{}/{}", model.provider_name().0, model.name().0))
```

模糊搜索 top 100（`:340-346`），另有 `exact_search()` contains 兜底；
`favorite_models` 支持快捷键循环（`cycle_favorite_models`，`:208-234`）。

自定义 provider 表单三个必填字段（`zed:crates/settings_ui/src/pages/llm_providers_page.rs:664-682`），
字段说明极简且**告知密钥去向**：`"API Key" / "Stored in the system keychain, not in settings.json."`。

能力勾选是**依赖驱动的渐进披露**（`llm_providers_page.rs:847-932`）：
`supports_chat_completions` 勾上才出现 `max_tokens_parameter`；
`supports_thinking` 勾上才出现 reasoning effort 下拉；两者都勾上才出现 `interleaved_reasoning`。

**校验是提交时一次、只报第一条、错误显示在底部动作条上方**（`:1101-1111` + `:1189-1249`）：
provider 名非空 → **与现有 provider 的 id 和 name 都查重** → API URL 非空 → API Key 非空 →
逐个 parse 模型数值字段 → 模型名唯一。
而且**依赖关系在 parse 时强制归一化，不信任 UI 状态**（`:1272-1285`）：

```rust
interleaved_reasoning: model.supports_thinking && model.supports_chat_completions && model.interleaved_reasoning,
max_tokens_parameter:  model.supports_chat_completions && model.max_tokens_parameter,
reasoning_effort:      model.supports_thinking.then_some(model.reasoning_effort),
```

诊断错误是 3 变体枚举、文案挂在 `#[error]` 上（`zed:crates/language_model/src/registry.rs:24-32`）：
`NoProvider` / `ModelNotFound` / `ProviderNotAuthenticated(provider)`。

### 4.4 值得偷 / 明显的坑

**偷**：`LanguageModelSelection` 把 provider+model+thinking+effort+speed 打包成一个值；
换模型时把参数投影到新模型能力集合；每角色一条写进 doc comment 的回落链，廉价角色回落到同 provider 的 fast model；
「用户完全没配」才启用自动兜底；角色变更 emit 独立事件且只在真变时 emit；
内置 provider = 具名字段 / 自定义 = 用户命名 HashMap；provider id schema 用 `anyOf: [enum, string]`；
扁平模糊列表 + `Favorite/Recommended/<provider>` 分隔条 + `provider/model` 拼接搜索串；
`model_parameters` 通配匹配 + 倒序后写优先；能力勾选按依赖渐进披露且 parse 时归一化；
「API Key 存钥匙串不存 settings.json」写在字段说明里。

**坑**：
① 自定义 provider 的 7 个 capability + 3 个 token 上限全要用户手填，无探测无目录，门槛极高。
② 没有远端模型目录，内置模型表随版本发布。
③ 角色是扁平字段，加一个角色要改 `AgentSettings` / `SettingsContent` / `LanguageModelRegistry` /
`Event` 枚举 / `from_settings` 五处。
④ `AgentProfileSettings.default_model` 与 `AgentSettings.default_model` 的优先关系不在类型里体现。

---

## 5. Kilo Code（Kilo-Org/kilocode）

**首要发现：Kilo Code 已经不是 Roo Code 的 fork 了。** 当前 HEAD 的 `packages/` 下有
`opencode` / `server` / `tui` / `sdk` / `protocol` / `schema` / `core`，
类型来源是 `@opencode-ai/schema`（`kilocode:packages/core/src/model.ts:2`），
`packages/kilo-vscode` 与 `packages/kilo-jetbrains` 只是前端。**它是基于 OpenCode 重写的**，
所以调研它相当于顺带调研了 OpenCode 的模型层。

### 5.1 角色路由：命名 agent 就是配置组

```ts
// kilocode:packages/schema/src/agent.ts:19-31
export const Info = Schema.Struct({
  id: ID,
  model: Model.Ref.pipe(optional),                        // agent 自带模型引用（可缺省）
  request: Provider.Request,                              // agent 级 headers/body 覆盖
  system: Schema.String.pipe(optional),
  mode: Schema.Literals(["subagent", "primary", "all"]),
  hidden: Schema.Boolean,
  steps: PositiveInt.pipe(optional),
  permissions: Permission.Ruleset,
})
```

回落三档（`kilocode:packages/core/src/agent.ts:66-79`）：
配置的 default agent（且 `mode !== "subagent" && !hidden`）→ 内置 `build` → 任意第一个可选 agent。

### 5.2 `Model.Ref` + `variants`：「模型+参数」的最佳答案

```ts
// kilocode:packages/schema/src/model.ts:14-19, 59-85
export const Ref = Schema.Struct({ id: ID, providerID: Provider.ID, variant: VariantID.pipe(optional) })
export const Info = Schema.Struct({
  id: ID, providerID: Provider.ID,
  family: Family.pipe(optional),                          // 「claude opus」「claude sonnet」家族分组
  name: Schema.String, api: Api,
  capabilities: Capabilities,                             // { tools: boolean, input: string[], output: string[] }
  request: Schema.Struct({ ...Provider.Request.fields, variant: Schema.String.pipe(optional) }),
  variants: Schema.Struct({ id: VariantID, ...Provider.Request.fields }).pipe(Schema.Array),  // ← 关键
  cost: Schema.Array(Cost),
  status: Schema.Literals(["alpha", "beta", "deprecated", "active"]),
  enabled: Schema.Boolean,
  limit: Schema.Struct({ context: Schema.Int, input: …, output: Schema.Int }),
})
```

**variant 是挂在模型上的具名「请求参数包」，用户配置只存 `variant: "high"` 这个 id。**
一个真实 variant（`kilocode:packages/core/src/plugin/variant.ts:30-39`）：

```ts
export function generate(model: ModelV2Info): ModelV2Info["variants"] {
  if (model.api.type !== "aisdk" || model.api.package !== "@ai-sdk/openai-compatible") return []
  const ids = `${model.id} ${model.api.id}`.toLowerCase()
  if (!["glm-5.2", "glm-5-2", "glm-5p2"].some((n) => ids.includes(n))) return []
  return ["high", "max"].map((id) => ({ id, headers: {}, body: { reasoning_effort: id } }))
}
```

即 **reasoning effort 不是专门字段，而是 variant 的 request body 覆盖**。
生成的 variant 会被同 id 的显式定义替换（`variant.ts:17-23`）：
`[...generated.map((v) => explicit.get(v.id) ?? v), ...draft.variants.filter((v) => !generatedIDs.has(v.id))]`。

`Model.parse()` 允许模型 id 含 `/`（`packages/core/src/model.ts:33-39`：`split("/")` 后 `slice(1).join("/")`）。

### 5.3 元数据来源与合并优先级：本调研最完整的分层

基座是远端目录 **models.dev**，三层加载（`kilocode:packages/core/src/models-dev.ts:222-244`）：

```ts
const populate = Effect.gen(function* () {
  const fromDisk = yield* loadFromDisk          // 1) 磁盘缓存，TTL 5 分钟
  if (fromDisk) return fromDisk
  const snapshot = yield* loadSnapshot          // 2) 编译进二进制的快照 KILO_MODELS_DEV
  if (snapshot) return snapshot
  if (Flag.KILO_DISABLE_MODELS_FETCH) return {}
  return yield* Effect.scoped(Effect.gen(function* () {
    yield* Flock.effect(lockKey)                // 3) 跨进程文件锁（并发 CLI 会抢）
    const rechecked = yield* loadFromDisk       //    拿到锁后再查一次，别人可能已修好
    if (rechecked) return rechecked
    return JSON.parse(yield* fetchAndWrite())   //    GET https://models.dev/api.json
  }))
})
```

写入是 temp file + rename 原子替换（`:207-219`），坏 JSON 删掉重拉（`:195-203`）。

然后是插件层派生（`variant` / `models-dev` 插件），最后是**用户配置的逐字段稀疏覆盖**
（`kilocode:packages/core/src/config/plugin/provider.ts:52-105`）：

```ts
catalog.model.update(providerID, id, (model) => {
  if (config.family       !== undefined) model.family = config.family
  if (config.api          !== undefined) model.api = { ...model.api, ...config.api }
  if (config.capabilities !== undefined) model.capabilities = { …config.capabilities }
  if (config.request      !== undefined) Object.assign(model.request.headers, config.request.headers)  // ← 合并非替换
  if (config.variants     !== undefined) { /* 按 variant id 合并，不存在则新建 */ }
  if (config.disabled     !== undefined) model.enabled = !config.disabled
  if (config.limit        !== undefined) model.limit = { ...model.limit, ...config.limit }
})
```

**优先级：远端/快照目录 < 插件派生 < 用户配置文件，逐字段。**

而「定义新 provider」和「覆盖内置 provider」用**同一个全 optional 的 schema**
（`kilocode:packages/core/src/config/provider.ts:65-71`：
`Info{name?, env?, api?, request?, models?: Record<string, Model>}`）。
配置里是 `providers: Record<providerID, Info>`——已知 id 是覆盖，新 id 是新增。
**没有「内置 provider」和「自定义 provider」两套类型。**

### 5.4 值得偷 / 明显的坑

**偷**：`Model.Ref = {providerID, id, variant?}` 作为唯一模型引用，参数细节留在 catalog 的 `variants` 里；
variant 三层优先级（代码生成 < 用户显式，按 id 合并）；
远端目录三层加载（磁盘缓存 → 编译期快照 → 网络）+ 跨进程锁 + 原子 rename + 坏缓存自愈；
用户配置对 catalog 的逐字段稀疏覆盖，headers/body 合并而非替换；
新增与覆盖 provider 共用一个全 optional schema；
`status`（客观事实）与 `enabled`（用户意图）分开；`family` 做家族分组；`parse()` 允许模型 id 含 `/`。

**坑**：
① Effect-TS 全套抽象学习成本极高，对 SkillStar（Rust + serde）参考价值在**数据形状**而非实现方式。
② 同时存在 `config/` 和 `v1/config/` 两套 provider schema，且 `v1/config/provider.ts:73-77` 的注释
`// allow null values so removed variants can be deleted via stripNulls on save` 暴露一个真实坑：
**稀疏合并模型下「删除」无法用「不写」表达，需要 null 哨兵。**
③ `variant.ts` 里的 GLM-5.2 硬编码 hack 说明：**只要依赖远端目录，就一定有一批模型需要在代码里打补丁。**

---

## 6. Void（voideditor/void）

形状与 SkillStar 的 OMP modelRoles 最接近的一个。

### 6.1 角色路由：`Record<FeatureName, ModelSelection | null>`

```ts
// void:src/vs/workbench/contrib/void/common/voidSettingsTypes.ts:358-367
export type ModelSelection = { providerName: ProviderName, modelName: string }
export const featureNames = ['Chat', 'Ctrl+K', 'Autocomplete', 'Apply', 'SCM'] as const
export type ModelSelectionOfFeature = Record<(typeof featureNames)[number], ModelSelection | null>
```

角色的显示名与内部名分离（`:369-385`）：`Ctrl+K` → "Quick Edit"，`SCM` → "Commit Message Generator"。

**角色候选受能力过滤，且每角色自带「无候选文案」**
（`void:src/vs/workbench/contrib/void/common/voidSettingsService.ts:107-121`）：

```ts
export const modelFilterOfFeatureName: { [f in FeatureName]: {
  filter: (o: ModelSelection, opts: {chatMode, overridesOfModel}) => boolean
  emptyMessage: null | { message: string, priority: 'always' | 'fallback' } } } = {
  'Autocomplete': { filter: (o, opts) => getModelCapabilities(o.providerName, o.modelName, opts.overridesOfModel).supportsFIM,
                    emptyMessage: { message: 'No models support FIM', priority: 'always' } },
  'Chat': { filter: o => true, emptyMessage: null }, /* Ctrl+K / Apply / SCM 同 */
}
```

回落是一个**状态不变量协调器，每次 mutation 后跑一遍**（`voidSettingsService.ts:146-197`）：
① 重算每个 provider 的 `_didFillInProviderSettings`（所有必填项非空，派生字段不手工维护）；
② 重算全局可选模型列表（provider 填全 + 模型未隐藏）；
③ 每个角色若已存选择不在（经能力过滤的）候选里 → **静默改成第 0 个，否则 null**。

**「跟随主角色」是显式用户开关，不是隐式回落**（`voidSettingsTypes.ts:445-446`）：
`syncApplyToChat: boolean` / `syncSCMToChat: boolean`。UI 表达很好（`Settings.tsx:1273-1281`）：
开关副标签写 `Same as Chat model` / `Different model`，开着就 `hidden` 掉该角色的选择器。
**但实现是写时复制**（`voidSettingsService.ts:403-410`：`setModelSelectionOfFeature('Apply', deepClone(…['Chat']))`）。

### 6.2 数据模型

```ts
// void:.../voidSettingsTypes.ts:487-510
export type ModelSelectionOptions = { reasoningEnabled?: boolean; reasoningBudget?: number; reasoningEffort?: string }
export type OptionsOfModelSelection = {                  // 四层嵌套：角色 → provider → 模型 → 选项
  [f in FeatureName]: Partial<{ [p in ProviderName]: { [modelName: string]: ModelSelectionOptions | undefined } }> }
export type OverridesOfModel = {                         // 三层嵌套：provider → 模型 → 能力覆盖
  [p in ProviderName]: { [modelName: string]: Partial<ModelOverrides> | undefined } }
```

能力元数据是 1586 行硬编码表。类型里最值得抄的是
**能力元数据直接声明该渲染哪种控件**（`void:.../modelCapabilities.ts:176-190`）：

```ts
reasoningCapabilities: false | {
  readonly supportsReasoning: true
  readonly canTurnOffReasoning: boolean         // 只支持推理的模型不能关
  readonly canIOReasoning: boolean              // o1 能控制思考但不输出思考
  readonly reasoningReservedOutputTokenSpace?: number
  readonly reasoningSlider?: undefined
    | { type: 'budget_slider'; min: number; max: number; default: number }   // anthropic
    | { type: 'effort_slider'; values: string[]; default: string }           // openai-compatible
  readonly openSourceThinkTags?: [string, string]                           // ollama 的 <think>
}
```

能力解析三级优先级（`modelCapabilities.ts:1483-1513`）：
① provider 硬编码表精确匹配（大小写不敏感）→ ② provider 的 `modelOptionsFallback(modelName)` 名字启发式 →
③ 全局默认 + `isUnrecognizedModel: true`。
**用户 `overrides` 永远最后 spread，优先级最高**；`isUnrecognizedModel` / `recognizedModelName` 暴露给 UI。

最终 reasoning 参数压成一个判别联合（`:1533-1586`）：
`{type:'budget_slider_value', reasoningBudget} | {type:'effort_slider_value', reasoningEffort} | null`。
注意 `getIsReasoningEnabledState()` 里 **reasoning 的默认开启状态依赖角色**：
`const defaultEnabledVal = featureName === 'Chat' || !canTurnOffReasoning`。

### 6.3 诊断：本调研最好的一处

```ts
// void:.../voidSettingsTypes.ts:399-431
export const isProviderNameDisabled = (providerName, settingsState) => {
  if (settingsAtProvider.models.length === 0) {
    return isAutodetected ? 'providerNotAutoDetected'                    // 本地 provider 没探测到
      : (!settingsAtProvider._didFillInProviderSettings ? 'notFilledIn'  // 必填项没填
                                                        : 'addModel')    // 填了但没加模型
  }
  return false
}
export const isFeatureNameDisabled = (featureName, settingsState) => {
  const selectedProvider = settingsState.modelSelectionOfFeature[featureName]
  if (selectedProvider) return isProviderNameDisabled(selectedProvider.providerName, settingsState)
  if (canTurnOnAModel) return 'needToEnableModel'      // 有被隐藏的模型可以打开
  if (anyFilledIn)     return 'addModel'               // 有 provider 填好了只是没加模型
  return 'addProvider'
}
```

**五档具体可操作的结论**，不是一句「配置无效」，而是直接告诉用户下一步做什么。

设置面板按 Tab 分组（`Settings.tsx:1038-1047, 1168-1540`）：
Models / Local Providers / Main Providers / Feature Options / MCP / General，支持 `'all'` 一次全展开。

高级覆盖 = 开关 + JSON textarea + 指向源码的参考链接（`Settings.tsx:322-355`），
而且**先告诉用户 Void 认不认识这个模型**：
`Model not recognized by Void.` / `Void recognizes ${modelName} ("${recognizedModelName}").`
——对代理商任意模型名的场景极其有用。

### 6.4 值得偷 / 明显的坑

**偷**：五档可操作诊断；`_didFillInProviderSettings` 是每次 mutation 重算的派生字段；
角色候选受能力过滤 + 每角色自带 `emptyMessage`；状态不变量协调器；
「跟随主角色」做成显式开关且 UI 写明当前含义、开着就隐藏选择器；
`reasoningSlider: {type:'budget_slider'|'effort_slider'}` 直接决定渲染哪个控件；
`isUnrecognizedModel` / `recognizedModelName` 暴露给 UI；reasoning 默认开启状态依赖角色；
高级覆盖 = 开关 + JSON textarea + 源码链接。

**坑**：
① `syncApplyToChat` 是写时复制而非读时解析——落盘状态里「继承 Chat」和「恰好和 Chat 一样」不可区分，
关掉开关拿不回原值。Zed 的 `Option<T> + or_else` 是更好的做法。
② `OptionsOfModelSelection` 四层嵌套：换角色模型后旧模型的选项残留成垃圾，
而「同一模型在两角色里用不同 effort」这个需求极罕见，不值这个代价。
③ 1586 行硬编码表且无远端目录，代理商的怪名字大概率落到 `isUnrecognizedModel: true`。
④ `_validatedModelState` 静默改写用户选择（`:190-196`），无任何提示。

---

## 7. 横向对比表

| 维度 | Cline | Roo Code | Continue | Zed | Kilo/OpenCode | Void |
|---|---|---|---|---|---|---|
| 角色路由范式 | 字段名前缀复制 | 角色 → profile **id** | 模型声明 roles + 本地选中态 | 扁平 `Option<Selection>` | 角色 = 命名 agent | `Record<Role, Selection\|null>` |
| 角色数量 | 2 | mode 数 + enhance | 8 | 6 + 2 列表 | 不限 | 5 |
| 能同时生效 | 是 | **否**（切 mode 改全局激活） | 是 | 是 | 是 | 是 |
| 未配置回落 | 无（写时双写） | 保存的 → 保留当前 → 自动钉住 | 记住的 → 候选第 0 → null，**并回写** | 每角色独立链，含「同 provider fast model」 | default → `build` → 任意可选 | 候选第 0 → null（静默改写） |
| 配置组 | **无** | 有（provider 设置包） | 有（整个 config.yaml） | 有（工具权限包） | 有（agent） | 无 |
| 模型+参数打包 | 散在 `*ModelId`/`*ThinkingBudgetTokens` | profile 字段（`enableReasoningEffort`+`reasoningEffort`） | `defaultCompletionOptions` | **`LanguageModelSelection` 单一结构** | **`Model.Ref{…,variant}` + catalog variants** | 四层嵌套 `OptionsOfModelSelection` |
| 自定义 provider | 落 OpenAI 兼容表单兜底 | `openAi*` 前缀 + `openAiCustomModelInfo` | `provider: string` 自由值 | `HashMap<name,_>` 与内置字段并列 | 与内置共用全 optional schema | `openAICompatible` + `OverridesOfModel` |
| 能力元数据来源 | 远端 + 硬编码 defaults + 后缀匹配 | 硬编码(static) + 远端(dynamic) + 用户填(openai) | 硬编码 + **正则匹配** | 硬编码 + **用户手填** | **远端 models.dev + 快照 + 插件派生 + 用户逐字段覆盖** | 1586 行硬编码 + 名字启发式 + 用户 overrides |
| 校验时机 | 保存前 | 保存前，拆两路避免重复 | 加载时（`profile.errors`） | 提交时，只报第一条 | Schema 解析时 | 每次 mutation 后协调 |
| 模型选择控件 | 小列表 select / 大列表 Fuse | Popover + cmdk combobox | 每角色一个 Listbox | **扁平模糊列表 + 分隔条分组** | — | 下拉（`模型 (provider)`） |

---

## 8. 给 SkillStar 的可迁移结论

### 8.1 强烈建议采纳

**A1. 把「模型引用」做成单一可存储结构，而不是散落字段。**
Zed 的 `LanguageModelSelection{provider, model, enable_thinking, effort, speed}`
（`zed:crates/settings_content/src/agent.rs:597-606`）和 Kilo 的 `Model.Ref{providerID, id, variant?}`
（`kilocode:packages/schema/src/model.ts:14-19`）是两个正确答案。SkillStar 的
`OmpRoleTarget{provider_id, model, thinking}`（`crates/skillstar-models/src/tool_sync/types.rs:115-119`）
已经是这个形状——**应该把它从 OMP 专用提升成 Models 域的通用类型**。

**A2. 换模型时把参数投影到新模型能力集合，不要清空也不要照抄。**
证据：`zed:crates/agent_settings/src/agent_settings.rs:289-322`。
SkillStar 现在 `to_role_value()` 只做了「不在 `OMP_THINKING_LEVELS` 全局枚举里就丢弃」
（`types.rs:135-140`），方向对但太粗——应按具体模型能力裁剪。

**A3. 每角色一条显式、写进注释/文档的回落链；廉价角色回落到「同 provider 的便宜模型」。**
证据：`zed:crates/language_model/src/registry.rs:483-527`（四条不同链）+
`crates/settings_content/src/agent.rs:246-263`（回落目标写在 doc comment 里）。

**A4. 回落/丢弃发生后要回写或反馈，让 UI 与实际一致。**
证据：`continue:core/config/selectedModels.ts:55, 65-77`（`fellBack` → `globalContext.update`）。
SkillStar 的 `resolve_omp_roles()` 会静默 drop 指向未绑定 provider 的角色
（`crates/skillstar-models/src/tool_sync/omp_provider.rs:245-261`），前端不知道
——**这是当前一个真实的一致性缺口**。

**A5. 分级、可操作的诊断，而不是布尔「有效/无效」。**
证据：`void:src/vs/workbench/contrib/void/common/voidSettingsTypes.ts:399-431`（五档）。
SkillStar 的 provider 有 Base URL / Key / 模型清单 / 余额 / 延迟五个维度，
诊断枚举应覆盖：未填 Base URL / 未填 Key / 模型清单为空 / 清单拉取失败 / 探测超时 / 余额为 0 /
绑定的模型不在清单里，每档配一个「下一步」动作。

**A6. UI 明确告知回落目标。**
证据：`continue:gui/src/pages/config/components/ModelRoleSelector.tsx:86-92`（`No valid Apply models. Using Chat model`）。
SkillStar 已经有 `OMP_ROLES_INHERITING_DEFAULT` 和 `OMP_DEFAULT_CYCLE_ORDER`
（`src/features/models/lib/ompRoles.ts:53, 59`）这两份知识，**要把它们渲染成角色行的 placeholder**。

**A7. 「不在当前列表里」的模型必须保留成显式选项。**
证据：`cline:apps/vscode/webview-ui/src/components/settings/providers/ModelPickerWithManualEntry.tsx:100-118`
+ `Roo-Code:webview-ui/src/components/settings/ModelPicker.tsx:118-124`。
SkillStar 的角色可以指向 provider 清单里没有的模型（`omp_provider.rs:176-179` 的注释已承认），
UI 必须能表达。**不要学 Void 的 `_validatedModelState` 静默改写。**

**A8. 模型清单/能力元数据用「远端目录 → 编译期快照 → 网络」三层 + 原子写 + 跨进程锁。**
证据：`kilocode:packages/core/src/models-dev.ts:222-244, 207-219, 195-203`。
SkillStar 的 `ModelCatalogFetchResult{models, catalog, metadata_sources, missing_cost_count}`
（`crates/skillstar-models/src/providers/types.rs:198-206`）里的 `metadata_sources` 说明已在往多来源走
——**应把优先级固化成「远端目录 < 名字正则派生 < 用户覆盖」的逐字段合并**，并把来源显示在 UI 上。

**A9. 能力元数据里直接声明「该渲染哪种控件」。**
证据：`void:.../modelCapabilities.ts:180-184`（`reasoningSlider: budget_slider | effort_slider`）+
`Roo-Code:packages/types/src/model.ts:91-93`（`supportsReasoningEffort: boolean | string[]`）。
SkillStar 的 `OMP_THINKING_LEVELS` 是全局 9 元枚举（`tool_sync/types.rs:103-105`），
所有模型都显示 9 个选项——**至少要区分「支持 budget（token 数）」和「支持 effort（枚举）」两类**。

**A10. 「定义新 provider」和「覆盖内置 preset」用同一个全 optional schema。**
证据：`kilocode:packages/core/src/config/provider.ts:65-71` + `zed:crates/language_models/src/settings.rs:17-34`。
SkillStar 已有 `preset_id: Option<String>`，方向对；**要确认 preset 提供的是「默认值」而非「不可覆盖的事实」**。

### 8.2 值得考虑

**B1. 引入「配置组」层。** Roo 的 `apiConfigs` + `currentApiConfigName`
（`Roo-Code:src/core/config/ProviderSettingsManager.ts:27-40`）让用户一键切几套配置。
但 SkillStar 的配置组语义应是「一组 Agent 绑定的快照」（「公司代理套餐」vs「自购 key 套餐」），
不是单 provider 的设置包。**建议先做导入/导出 + 命名快照，别一上来做可切换的 profile 树。**

**B2. 角色 → 配置组 id 的间接层。** 若做了 B1，用**稳定 id 而非 name**
（`Roo-Code:packages/types/src/global-settings.ts:193`）。SkillStar 的 `OmpRoleTarget` 存内部
`provider_id`、写盘时才派生 `skillstar_*` key（`tool_sync/types.rs:110-113`），思路已一致。

**B3. 「跟随主角色」做成显式开关。** Void 的 `syncApplyToChat`（`voidSettingsTypes.ts:445-446`）
在 UI 表达上很好，**但必须做成 `Option<T>` + 读时解析，不要学它的写时复制**。

**B4. 收藏/推荐/家族分组的模型选择器。** Zed 的三段分隔条 + `provider/model` 拼接串模糊搜索
（`zed:crates/agent_ui/src/language_model_selector.rs:274-305, 374-386`）比两级下拉好用得多，
尤其当用户接的是几百个模型的聚合代理商。Kilo 的 `family`（`packages/schema/src/model.ts:63`）可做二级折叠。

**B5. `status` 与 `enabled` 分开。** `kilocode:packages/schema/src/model.ts:79-80`
区分客观状态与用户意图，能支撑「隐藏废弃模型但保留已选中的」。SkillStar 的模型清单目前只是 `Vec<String>`。

**B6. 校验拆成「面板级」和「字段级」两路。** Roo 明确注释了原因
（`Roo-Code:webview-ui/src/utils/validate.ts:277-282`）。SkillStar 的 provider 卡片 + 模型选择器
同时存在时会遇到同样问题。

**B7. 「模型 id 被识别成了什么」要显示给用户。** Void 的 `recognizedModelName` / `isUnrecognizedModel`
（`modelCapabilities.ts:1484-1513`）。若引入正则派生的能力表（A8），这是必须的配套。

**B8. 「删除」需要显式哨兵。** Kilo 踩过：
`kilocode:packages/core/src/v1/config/provider.ts:73-77`
（`// allow null values so removed variants can be deleted via stripNulls on save`）。
SkillStar 的 `ToolBinding.settings` 是稀疏 JSON 袋，同样面临「不写 = 继承 vs 不写 = 删除」的歧义。

### 8.3 明确不要学

**C1. 不要用字段名前缀表达角色维度。**
`cline:apps/vscode/src/shared/storage/state-keys.ts:148-238`（`planMode*`×44 + `actMode*`×44）+
`cline:apps/vscode/src/core/controller/models/updateApiConfiguration.ts:43-51`（前缀字符串替换）。
角色维度必须是**数据**（map 的键），不能是**标识符的一部分**。

**C2. 不要用「写时双写」代替「读时回落」。**
`cline:.../updateApiConfiguration.ts:119-131` + `void:.../voidSettingsService.ts:403-410`。
落盘状态必须能区分「显式设置」和「继承默认」。正确做法是 `Option<T>` + `or_else`
（`zed:crates/language_model/src/registry.rs:483-527`）。

**C3. 不要按 provider 展开扁平前缀字段命名空间。**
`Roo-Code:packages/types/src/provider-settings.ts:195-270`（639 行）+ `cline:apps/vscode/src/shared/api.ts:4-53`（52 个字面量）。
SkillStar 的 `ProviderPatchFlat.codex_wire_api` 已有这个味道——
**provider/工具特有的配置应进 `settings: Option<Value>` 袋 + typed accessor**
（正如现有的 `OmpSettings::from_binding()` / `CodexSettings::from_value()`），不要往顶层 struct 加字段。

**C4. 不要让「角色切换」改写全局激活配置。**
`Roo-Code:src/core/webview/ClineProvider.ts:1302-1334`。Roo 的 per-mode profile 是「记忆」不是「路由」。
SkillStar 的 OMP 角色是真正的并行路由（一次运行里 `default` 和 `smol` 都活着），**不要退化。**

**C5. 不要让用户手填全部能力元数据。**
`zed:crates/settings_ui/src/pages/llm_providers_page.rs:847-932`（7 个 capability + 3 个 token 上限）。
Zed 的用户是重度 IDE 用户；SkillStar 是给普通用户「一键同步代理商到 6 个 Agent」的工具，
门槛必须是「Base URL + Key + 选模型」三步。手填只能是**藏在高级区的兜底**
（学 Void 的 JSON textarea + 开关，`Settings.tsx:322-355`）。

**C6. 不要为「同一模型在不同角色下用不同参数」付四层嵌套的代价。**
`void:.../voidSettingsTypes.ts:493-497`。把参数打包进「模型引用」本身（A1）就够了——
同一模型在两角色里用不同 effort，就是两个不同的 `RoleTarget` 值，天然分开。

**C7. 不要只维护硬编码能力表而不接远端目录。**
`void:.../modelCapabilities.ts`（1586 行无远端）+ `zed:crates/language_models/src/provider/*.rs`（随版本发布）。
SkillStar 的用户接的是**变化最快的第三方代理商**，硬编码表一定过期。

**C8. 不要在候选集失效时静默改写用户的选择。**
`void:.../voidSettingsService.ts:190-196`。要么保留失效值并标红（A7），要么改写并明确提示（A4）。

---

## 9. 专项评价：SkillStar 的 OMP modelRoles 设计

### 9.1 现状

角色挂在 **binding 级（tool-level）的 settings 袋**上，类型注释已经把理由写清楚了：

```rust
// crates/skillstar-models/src/providers/types.rs:230-247
/// `settings` is the **tool-level** settings bag … OMP's `modelRoles` map is
/// the first consumer, because one role can point at a different provider than
/// the active one (`smol` on a cheap provider, `slow` on a reasoning provider).
pub struct ToolBinding { pub entries: Vec<ToolActivation>, pub active_index: usize,
                         pub settings: Option<serde_json::Value> }
// crates/skillstar-models/src/tool_sync/types.rs:107-119, 152-160
pub struct OmpRoleTarget { pub provider_id: String, pub model: String, pub thinking: Option<String> }
pub struct OmpSettings   { pub roles: BTreeMap<String, OmpRoleTarget> }
```

写盘时（`tool_sync/omp_provider.rs:237-270`）：角色名不合法 / provider 未绑定 / model 为空 → 丢；
没有 `default` → 用 active entry 补。

### 9.2 评价：设计是对的，而且比业界多数实现更好

**✅ 「角色 → 单一选择结构」是主流范式 A**，Zed / Void / Kilo 都是这个形状。
`OmpRoleTarget` 与 `LanguageModelSelection`（`zed:crates/settings_content/src/agent.rs:597-606`）、
`Model.Ref`（`kilocode:packages/schema/src/model.ts:14-19`）是同一抽象层次。**不是自创，是收敛到业界正解。**

**✅ 挂在 binding 级而非 entry 级是必然的**——角色要能跨 provider，所以不可能挂在单个 `ToolActivation` 上。
业界对照：Void 的 `modelSelectionOfFeature` 每个值自带 `providerName`（`voidSettingsTypes.ts:358-366`）；
Zed 的角色字段每个值自带 provider（`agent_settings.rs:214-223`）。**跨 provider 是角色路由的基本要求。**

**✅ 存内部 `provider_id`、写盘时派生 `skillstar_*` key 比存最终字符串好**（`tool_sync/types.rs:110-113`）。
业界对照：Roo 的 `modeApiConfigs` 存 profile **id** 而非 name，正是为了重命名不断链
（`ProviderSettingsManager.ts:29-30`）。**同一个道理，做法一致。**

**✅ 「角色名是开放字符串 map」比枚举好。** `OMP_MODEL_ROLES`（`tool_sync/types.rs:91-93`）
只是 UI 展示清单，磁盘 schema 是开放 map（`is_valid_omp_role_name` 只禁 `/`、空白、`@` 前缀）。
业界对照：Continue 的 `z.enum` 导致加角色改三处且 `summarize` 已不同步
（`continue:core/config/selectedModels.ts:23`）；Zed 的扁平字段要改五处。**SkillStar 避开了这两个坑。**

**✅ 「未配置就不写盘」是正确的写盘策略**（`tool_sync/types.rs:150-155` 的注释：
`absent roles are never written to config.yml — OMP falls back to default on its own`）。
在「SkillStar 写盘、外部工具解释」的架构下这是唯一正确做法——把回落语义留给 OMP，不越权。
业界对照：Zed 也是「字段 None 就读时 `or_else`」，从不把回落结果写进 settings.json
（`registry.rs:490-491`）。**Cline 的写时双写和 Void 的 eager copy 都是反例，SkillStar 没犯。**

**✅ `thinking: Option<String>` 的 `inherit`/缺省等价处理是对的**
（`src/features/models/lib/ompRoles.ts:67-69` + `tool_sync/types.rs:135-140`）。
业界对照：Roo 用 `-1` 哨兵表示继承全局（`Roo-Code:src/core/context-management/index.ts:182-183`），
`inherit` 是同一模式的更可读版本。

**✅ 主要/次要角色分层展示与 Continue 完全一致。**
`OMP_PRIMARY_ROLES` / `OMP_SECONDARY_ROLES`（`ompRoles.ts:45-46`）对应
Continue 的第一张 Card + `Toggle title="Additional model roles"`
（`continue:gui/src/pages/config/sections/ModelsSection.tsx:76-219`）。**独立收敛到同一 UX，是好信号。**

### 9.3 三个真实缺口

**⚠️ 缺口 1｜写盘时静默丢弃的角色没有反馈到 UI。**
`resolve_omp_roles()` 丢掉指向未绑定 provider 的角色（`omp_provider.rs:245-261`），前端不知道；
用户看到 `smol → 某 provider/某模型`，磁盘上什么都没写。
业界正解：Continue 的 `fellBack` + 回写（`continue:core/config/selectedModels.ts:55, 65-77`）。
**修法**：同步命令返回值带 `dropped_roles: Vec<{role, reason}>`，UI 在对应角色行标黄
「该 provider 未绑定到此 Agent，已跳过」。

**⚠️ 缺口 2｜`OMP_THINKING_LEVELS` 是全局 9 元枚举，没有按模型裁剪。**
`tool_sync/types.rs:103-105` 的 9 个等级对所有模型都显示。实际上 Anthropic 系是 budget（token 数）、
OpenAI 系是 effort（枚举）、很多模型完全不支持。
业界正解：`void:.../modelCapabilities.ts:180-184` 的 `reasoningSlider`；
`Roo-Code:packages/types/src/model.ts:91-93` 的 `boolean | string[]`；
`zed:crates/agent_settings/src/agent_settings.rs:302-312` 的 `supported_effort_levels()` 求交。
**修法**：`ModelCatalogEntry` 加 `reasoning: Option<ReasoningCapability>`，
形如 `{ kind: Budget{min,max,default} | Effort{values,default}, can_disable: bool }`。

**⚠️ 缺口 3｜UI 没有告知回落目标。**
`OMP_ROLES_INHERITING_DEFAULT`（`ompRoles.ts:59`）这份知识存在但只用来「不催用户填」。
业界正解：Continue 把回落写进按钮文案（`ModelRoleSelector.tsx:86-92`）；
Zed 把回落链写进 schema doc comment（`zed:crates/settings_content/src/agent.rs:246-263`）。
**修法**：未配置角色的行显示 `未配置 → 继承 default` 或 `未配置 → 由 OMP 自行选择`。

### 9.4 该不该推广到所有 Agent？

**该推广，但要分三档，不是一刀切给所有 Agent 开同样的角色 UI。**

**理由 1｜角色路由是 Agent 的属性，不是 SkillStar 的选择。** Void 有 5 个 feature、
Zed 6 个角色字段、Continue 8 个 role、Roo 是 mode 数 + enhance、Kilo 任意个 agent、
Claude Code 只有一个 model + 一个 fallback。**SkillStar 只能写盘目标工具支持的角色**——
给 Claude Code 显示 10 个角色选择器是错的。统一 UI 必须建在「每个 Agent 声明自己的角色清单」之上。

**理由 2｜数据模型统一、UI 按能力裁剪。** `ToolBinding.settings` + typed accessor
（`OmpSettings::from_binding()`，`tool_sync/types.rs:170-176`）已是正确的通用形状。建议：
把 `OmpRoleTarget` 提升为域内通用的 `RoleTarget`（或 `ModelRef`），加一个通用 `extra: Option<Value>`
承载工具特有参数（OMP 是 `thinking`，Codex 可能是 `reasoning_effort`，Claude Code 是 `fallback_model`）；
每个 Agent 在 `tool_sync/agents.rs` 注册表里声明
`roles: &[RoleDef{id, flag, primary, inherits_from, capability_filter}]`，空 slice = 不支持角色；
角色面板是同一个组件，从注册表读 `roles` 决定渲染几行。

**理由 3｜三档划分**

| 档 | Agent | 角色 UI |
|---|---|---|
| 无角色 | Claude Desktop | 单一 provider+model 选择 |
| 单角色 + 兜底 | Claude Code（model + fallback） | 主模型 + 一个「降级模型」行 |
| 多角色 | OMP（10 个）、Codex（profiles）、OpenCode（agents） | 完整角色面板（主要平铺 + 次要折叠） |

第三档不是只有 OMP 一家：**`kilocode:packages/schema/src/agent.ts:22` 的
`Agent.Info.model: Model.Ref` 就是 OpenCode 侧的同一个概念**，
SkillStar 可以把 OMP 的角色面板直接复用到 OpenCode 的 agent 上。

**理由 4｜不要为了统一而给不支持角色的 Agent 造角色。**
反例是 Cline：只有 2 个角色却付了 88 个复制字段的代价
（`cline:apps/vscode/src/shared/storage/state-keys.ts:148-238`）。
SkillStar 的价值在于「忠实同步到目标工具的真实配置文件」，
造一个目标工具不认识的角色等于制造无效配置。

### 9.5 一句话结论

**OMP modelRoles 的设计（角色 → `{provider_id, model, thinking}`，挂 binding 级 settings 袋，
未配置就不写盘）在横向对比中属于上游水平**——它避开了 Cline 的字段复制、Roo 的扁平前缀命名空间、
Continue 的枚举角色不同步、Void 的 eager copy 四个已知陷阱，形状与 Zed / Kilo 的正解一致。
**应该推广，但要先把 `OmpRoleTarget` 提升成域内通用类型 + 让每个 Agent 在注册表里声明自己的角色清单**，
并补上三个缺口：写盘丢弃要反馈、thinking 等级要按模型裁剪、回落目标要显示在 UI 上。

---

## 附：克隆记录

```
/tmp/model-research/cline       cline/cline            apps/vscode/{src,webview-ui}
/tmp/model-research/Roo-Code    RooCodeInc/Roo-Code    packages/types, src, webview-ui/src
/tmp/model-research/continue    continuedev/continue   core, gui, packages（sparse）
/tmp/model-research/zed         zed-industries/zed     crates/{agent_settings,language_model,language_models,
                                                       settings_content,settings_ui,agent_ui}
/tmp/model-research/kilocode    Kilo-Org/kilocode      packages/{core,schema,llm}（sparse；已重构为 OpenCode 基座）
/tmp/model-research/void        voideditor/void        src/vs/workbench/contrib/void/{common,browser}
```

本次调研未修改 SkillStar 的 `src/`、`crates/`、`src-tauri/` 任何文件。

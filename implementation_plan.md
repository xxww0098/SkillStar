# 模型配置界面：三合一最完美方案

## 核心洞察

三个 Coding Agent 有**统一的设计模式** — 都是「连接层」+「行为层」的组合：

| App | 连接层 | 行为层 | 配置文件 | 格式 |
|---|---|---|---|---|
| **Claude** | `env.*` (API Key, Base URL, Model mapping) | 顶层字段 (model, effortLevel, permissions...) | `~/.claude/settings.json` | JSON |
| **Codex** | `model_providers.*` + API Key (auth.json) | 顶层字段 (model, approval_policy, sandbox...) | `~/.codex/config.toml` + `~/.codex/auth.json` | TOML + JSON |
| **OpenCode** | `provider.*` (apiKey, baseURL) | 顶层字段 (model, permission, compaction...) | `~/.config/opencode/opencode.json` | JSON |

**现有设计的根本问题**：三个 App 都把行为配置和连接配置混在 ProviderCard 里，但行为偏好（"我要高思考力"/"自动批准"）跟用哪个供应商无关。

## 新架构：统一双区分治

每个 App tab 下都是同一个布局模式 — 上方行为面板 + 下方供应商卡片：

```
┌──────────────────────────────────────────────────┐
│ [Claude]  Codex  OpenCode            [配置文件]   │
├──────────────────────────────────────────────────┤
│                                                  │
│ ┌─ 🎛 行为配置（即时生效）──────────────────────┐ │
│ │  视 App 而定，展示该 App 独有的行为控件       │ │
│ └────────────────────────────────────────────────┘ │
│                                                  │
│ ── 供应商（连接配置）──────────────────           │
│ ┌── OpenRouter ──── [当前] ━━━ ▼ ────┐           │
│ │ API Key: ****  Endpoint: ...       │           │
│ └────────────────────────────────────┘           │
│ ┌ + 添加供应商 ──────────────────────┐           │
│ └────────────────────────────────────┘           │
└──────────────────────────────────────────────────┘
```

**设计原则：**
- 行为配置区 **始终可见**，不需展开卡片
- 供应商卡片 **只管连接**：API Key + Endpoint + Model mapping
- 行为配置是 **全局的**，不跟供应商走
- 所有控件 **即时生效**，改一个值就写一个值
- 切换供应商只改写连接层（env/auth），不动行为层

---

## 各 App 行为配置字段

### Claude Code 行为面板

| 控件 | 类型 | JSON Key | 值 |
|---|---|---|---|
| 默认模型 | Select | `model` | `default` / `sonnet` / `opus` / `haiku` |
| 权限模式 | Select | `permissions.defaultMode` | `default` / `acceptEdits` / `plan` / `auto` / `bypassPermissions` |
| Effort Level | SegmentSlider | `effortLevel` | `low` / `medium` / `high` / `max` |
| Thinking | Toggle | `alwaysThinkingEnabled` | `true` / `false` |
| 语言 | Select | `language` | `""` / `chinese` / `english` / `japanese` ... |
| **高级（折叠）** |
| Model Overrides | KV 编辑器 | `modelOverrides` | `{ "model-name": "arn:..." }` |
| Fast Mode | Toggle | `fastModePerSessionOptIn` | `true` / `false` |
| Git Instructions | Toggle | `includeGitInstructions` | `true` / `false` |
| Auto Updates | Select | `autoUpdatesChannel` | `stable` / `latest` |

### Codex 行为面板

| 控件 | 类型 | TOML Key | 值 |
|---|---|---|---|
| 默认模型 | Input | `model` | `gpt-5.4` / `gpt-5-codex` / ... |
| 审批策略 | Select | `approval_policy` | `untrusted` / `on-request` / `never` |
| 沙箱模式 | Select | `sandbox_mode` | `workspace-write` / `danger-full-access` |
| 推理力度 | Select | `model_reasoning_effort` | `low` / `medium` / `high` |
| 沟通风格 | Select | `personality` | `friendly` / `pragmatic` / `none` |
| 网页搜索 | Select | `web_search` | `cached` / `live` / `disabled` |
| **高级（折叠）** |
| 推理摘要 | Select | `model_reasoning_summary` | `none` / `auto` |
| 输出详度 | Select | `model_verbosity` | `low` / `medium` / `high` |
| 上下文窗口 | Input | `model_context_window` | 数字 |
| OSS Provider | Input | `oss_provider` | `ollama` / `lmstudio` |
| OpenAI Base URL | Input | `openai_base_url` | URL |
| **Features（开关组）** |
| Fast Mode | Toggle | `features.fast_mode` | `true` / `false` |
| Shell Snapshot | Toggle | `features.shell_snapshot` | `true` / `false` |
| Undo | Toggle | `features.undo` | `true` / `false` |
| Multi Agent | Toggle | `features.multi_agent` | `true` / `false` |

### OpenCode 行为面板

| 控件 | 类型 | JSON Key | 值 |
|---|---|---|---|
| 默认模型 | Input | `model` | `anthropic/claude-sonnet-4-5` / ... |
| 小模型 | Input | `small_model` | `anthropic/claude-haiku-4-5` / ... |
| 默认代理 | Select | `default_agent` | `build` / `plan` / 自定义 |
| 权限 - edit | Select | `permission.edit` | `allow` / `ask` |
| 权限 - bash | Select | `permission.bash` | `allow` / `ask` |
| 分享 | Select | `share` | `manual` / `auto` / `disabled` |
| 自动更新 | Tristate | `autoupdate` | `true` / `false` / `"notify"` |
| **高级（折叠）** |
| 压缩 - auto | Toggle | `compaction.auto` | `true` / `false` |
| 压缩 - prune | Toggle | `compaction.prune` | `true` / `false` |
| 压缩 - reserved | Input | `compaction.reserved` | 数字 |
| 禁用提供商 | KV | `disable_provider` | 字符串数组 |
| 实验性功能 | KV | `experimental` | 开关对象 |

---

## Proposed Changes

---

### 第一层：Backend — 统一的 set_field / get_field

---

#### [MODIFY] [claude.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/core/model_config/claude.rs)

新增即时写入函数（JSON 单字段级别）：

```rust
/// Set a single top-level field in settings.json, preserving others.
pub fn set_field(key: &str, value: Value) -> Result<()> {
    let mut settings = read_settings().unwrap_or(Value::Object(Map::new()));
    if !settings.is_object() { settings = Value::Object(Map::new()); }
    let obj = settings.as_object_mut().unwrap();
    if value.is_null() { obj.remove(key); } else { obj.insert(key.to_string(), value); }
    write_settings(&settings)
}
```

---

#### [MODIFY] [codex.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/core/model_config/codex.rs)

新增 TOML 单字段写入（使用已有的 `toml_edit`）：

```rust
/// Set a single top-level field in config.toml, preserving formatting.
pub fn set_toml_field(key: &str, value: &str) -> Result<()> {
    let text = read_config_text()?;
    let updated = update_toml_field(&text, key, value)?;
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    atomic_write(&config_toml_path(), updated.as_bytes())
}

/// Set a boolean field in config.toml.
pub fn set_toml_bool(key: &str, value: bool) -> Result<()> {
    let text = read_config_text()?;
    let mut doc: DocumentMut = text.parse()?;
    doc[key] = toml_edit::value(value);
    atomic_write(&config_toml_path(), doc.to_string().as_bytes())
}

/// Set a TOML field using nested key path (e.g., "features.fast_mode").
pub fn set_toml_nested(path: &str, value: toml_edit::Value) -> Result<()> {
    let text = read_config_text()?;
    let mut doc: DocumentMut = text.parse().unwrap_or_default();
    let keys: Vec<&str> = path.split('.').collect();
    // Navigate/create nested tables and set value
    // ...
    atomic_write(&config_toml_path(), doc.to_string().as_bytes())
}
```

---

#### [MODIFY] [opencode.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/core/model_config/opencode.rs)

新增即时写入函数（JSON 单字段级别，与 claude 同构）：

```rust
/// Set a single top-level field in opencode.json, preserving others.
pub fn set_field(key: &str, value: Value) -> Result<()> {
    let mut config = read_config().unwrap_or(Value::Object(Map::new()));
    if !config.is_object() { config = Value::Object(Map::new()); }
    let obj = config.as_object_mut().unwrap();
    if value.is_null() { obj.remove(key); } else { obj.insert(key.to_string(), value); }
    write_config(&config)
}
```

---

#### [MODIFY] [commands/models.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/commands/models.rs)

新增 3 个统一的 Tauri 命令，行为面板直接调用：

```rust
/// Set a single behavior field for Claude (settings.json top-level).
#[tauri::command]
pub async fn set_claude_setting(key: String, value: Value) -> Result<(), AppError> { ... }

/// Set a single behavior field for Codex (config.toml top-level).
#[tauri::command]
pub async fn set_codex_setting(key: String, value: String) -> Result<(), AppError> { ... }

/// Set a single behavior field for OpenCode (opencode.json top-level).
#[tauri::command]
pub async fn set_opencode_setting(key: String, value: Value) -> Result<(), AppError> { ... }
```

在 `commands.rs` 和 `tauri.conf.json` 中注册新命令。

---

#### [MODIFY] [providers.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/core/model_config/providers.rs)

`apply_config_to_app` 改为 **只写连接层**：

```rust
fn apply_config_to_app(app_id: &str, config: &Value) -> Result<()> {
    match app_id {
        "claude" => {
            // Only merge the "env" block, preserve top-level behavior fields
            let mut existing = claude::read_settings().unwrap_or_default();
            if let Some(new_env) = config.get("env") {
                existing.as_object_mut().unwrap().insert("env".to_string(), new_env.clone());
            }
            claude::write_settings(&existing)?;
        }
        "codex" => {
            // Only write auth.json (API key), preserve config.toml behavior fields
            let auth = config.get("auth").cloned().unwrap_or_default();
            // Read existing TOML, only merge provider-related keys (model_provider, model_providers.*)
            let existing_toml = codex::read_config_text()?;
            let config_text = merge_codex_provider_fields(&existing_toml, config)?;
            codex::write_atomic(&auth, &config_text)?;
        }
        "opencode" => {
            // Only merge the "provider" block, preserve behavior fields
            let mut existing = opencode::read_config().unwrap_or_default();
            if let Some(new_provider) = config.get("provider") {
                existing.as_object_mut().unwrap().insert("provider".to_string(), new_provider.clone());
            }
            opencode::write_config(&existing)?;
        }
        _ => {}
    }
    Ok(())
}
```

> [!IMPORTANT]
> 这是架构核心改动：**切换供应商只改连接层（env/auth/provider），不动 model/effortLevel/permissions/approval_policy 等行为字段**。

---

### 第二层：Frontend — 统一组件架构

---

#### [NEW] [BehaviorPanel.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/BehaviorPanel.tsx)

通用行为面板容器，根据 `appId` 渲染对应的行为控件。提供统一的即时保存语义：

```tsx
interface BehaviorPanelProps {
  appId: "claude" | "codex" | "opencode";
}

export function BehaviorPanel({ appId }: BehaviorPanelProps) {
  switch (appId) {
    case "claude":  return <ClaudeBehaviorSection />;
    case "codex":   return <CodexBehaviorSection />;
    case "opencode": return <OpenCodeBehaviorSection />;
  }
}
```

---

#### [NEW] [ClaudeBehaviorSection.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/behavior/ClaudeBehaviorSection.tsx)

Claude 行为配置区，使用 `useClaudeSettings` hook：

**常用区（2 行 grid）:**
- 默认模型 Select
- 权限模式 Select
- EffortSlider（4 段）
- Thinking Toggle
- 语言 Select

**高级区（折叠）:**
- ModelOverridesEditor
- Fast Mode Toggle
- Git Instructions Toggle
- Auto Updates Select

---

#### [NEW] [CodexBehaviorSection.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/behavior/CodexBehaviorSection.tsx)

Codex 行为配置区，使用 `useCodexSettings` hook：

**常用区（2 行 grid）:**
- 默认模型 Input（支持 `gpt-5.4` / `gpt-5-codex` 等）
- 审批策略 Select（`untrusted` / `on-request` / `never`）
- 沙箱模式 Select（`workspace-write` / `danger-full-access`）
- 推理力度 SegmentSlider（`low` / `medium` / `high`）
- 沟通风格 Select（`friendly` / `pragmatic` / `none`）
- 网页搜索 Select（`cached` / `live` / `disabled`）

**高级区（折叠）:**
- Features 开关组（fast_mode, shell_snapshot, undo, multi_agent）
- 输出详度 / 推理摘要 / 上下文窗口
- OSS Provider / OpenAI Base URL

---

#### [NEW] [OpenCodeBehaviorSection.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/behavior/OpenCodeBehaviorSection.tsx)

OpenCode 行为配置区，使用 `useOpenCodeSettings` hook：

**常用区（2 行 grid）:**
- 默认模型 Input（`anthropic/claude-sonnet-4-5` 格式）
- 小模型 Input（`anthropic/claude-haiku-4-5`）
- 默认代理 Select（`build` / `plan`）
- 权限 - edit Select（`allow` / `ask`）
- 权限 - bash Select（`allow` / `ask`）
- 分享 Select（`manual` / `auto` / `disabled`）

**高级区（折叠）:**
- 自动更新 Tristate
- 压缩配置（auto/prune toggles + reserved input）

---

#### [NEW] [shared/EffortSlider.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/shared/EffortSlider.tsx)

可复用 4 段 segmented control（Claude effortLevel + Codex reasoning_effort 共用）：

```
 low    medium    high    max
  ○━━━━━━○━━━━━━●━━━━━━○
```

- 每段不同颜色渐变（蓝 → 绿 → 橙 → 红）
- 点击时 spring 动画过渡
- 悬停 tooltip

---

#### [NEW] [shared/ModelOverridesEditor.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/shared/ModelOverridesEditor.tsx)

Claude 的 `modelOverrides` 和 Codex 的 `model_providers.*` 通用 KV 编辑器。

---

#### [NEW] [hooks/useAppSettings.ts](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/hooks/useAppSettings.ts)

统一的即时读写 hook，三个 App 共用同一个接口：

```ts
export function useAppSettings(appId: "claude" | "codex" | "opencode") {
  const [settings, setSettings] = useState({});
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    // 根据 appId 调用不同的 get 命令
    // claude: get_claude_model_config → full settings.json
    // codex:  get_codex_config → { auth, config (toml text) }
    // opencode: get_opencode_config → full opencode.json
  }, [appId]);

  const set = useCallback(async (key: string, value: unknown) => {
    // Optimistic update
    setSettings(prev => ({ ...prev, [key]: value }));
    try {
      // 根据 appId 调用 set_claude_setting / set_codex_setting / set_opencode_setting
      await invoke(`set_${appId}_setting`, { key, value });
    } catch (e) {
      toast.error(`设置失败: ${e}`);
      load(); // rollback
    }
  }, [appId, load]);

  const setNested = useCallback(async (path: string, value: unknown) => {
    // Deep clone + set for paths like "permissions.defaultMode"
  }, [settings, load]);

  return { settings, loading, load, set, setNested };
}
```

---

### 第三层：整合 ModelsPanel

---

#### [MODIFY] [ModelsPanel.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/ModelsPanel.tsx)

每个 App tab 下改为双区布局：

```tsx
{activeApp && (
  <>
    {/* 行为配置区 - 始终可见 */}
    <BehaviorPanel appId={activeApp} />

    {/* 供应商连接卡片列表 */}
    <div className="mt-4 space-y-3">
      {localProviders.map(provider => (
        <ProviderCard ... />
      ))}
    </div>
  </>
)}
```

---

#### [MODIFY] [ProviderCard.tsx](file:///Users/xxww/Code/REPO/SkillStar/src/features/models/components/ProviderCard.tsx)

所有 App 的 ProviderCard 精简为**只展示连接字段**：
- Claude: API Key + Endpoint + Model mapping（env 变量部分）
- Codex: API Key + Provider base_url + model_provider ID
- OpenCode: apiKey + baseURL + provider 名称

删除所有行为相关字段（已移到 BehaviorPanel）。

---

## 文件变更清单

| 操作 | 文件 | 说明 |
|---|---|---|
| MODIFY | `src-tauri/src/core/model_config/claude.rs` | 新增 `set_field` |
| MODIFY | `src-tauri/src/core/model_config/codex.rs` | 新增 `set_toml_field`, `set_toml_bool`, `set_toml_nested` |
| MODIFY | `src-tauri/src/core/model_config/opencode.rs` | 新增 `set_field` |
| MODIFY | `src-tauri/src/commands/models.rs` | 新增 `set_claude_setting`, `set_codex_setting`, `set_opencode_setting` |
| MODIFY | `src-tauri/src/commands.rs` | 注册新命令 |
| MODIFY | `src-tauri/src/core/model_config/providers.rs` | `apply_config_to_app` 只写连接层 |
| NEW | `src/features/models/components/BehaviorPanel.tsx` | 统一行为面板容器 |
| NEW | `src/features/models/components/behavior/ClaudeBehaviorSection.tsx` | Claude 行为区 |
| NEW | `src/features/models/components/behavior/CodexBehaviorSection.tsx` | Codex 行为区 |
| NEW | `src/features/models/components/behavior/OpenCodeBehaviorSection.tsx` | OpenCode 行为区 |
| NEW | `src/features/models/components/shared/EffortSlider.tsx` | 共用 effort/reasoning 滑块 |
| NEW | `src/features/models/components/shared/ModelOverridesEditor.tsx` | 共用 KV 编辑器 |
| NEW | `src/features/models/hooks/useAppSettings.ts` | 统一即时读写 hook |
| MODIFY | `src/features/models/components/ModelsPanel.tsx` | 集成 BehaviorPanel |
| MODIFY | `src/features/models/components/ProviderCard.tsx` | 精简连接字段 |

## User Review Required

> [!IMPORTANT]
> **Codex 用 TOML 格式** — 处理比 JSON 复杂。已有 `toml_edit` crate 用于语法保留编辑，但嵌套字段（如 `features.fast_mode`）需要特殊处理。方案中已考虑此差异。

> [!WARNING]
> **破坏性变更** — `apply_config_to_app` 从完整覆写改为连接层 merge。现有 ProviderCard 的存量数据（`settingsConfig` 可能混有行为字段）在首次使用时需要兼容旧格式。

## Verification Plan

### Automated Tests
- `claude.rs` / `opencode.rs`: `set_field` 单元测试 — 写入不破坏其他字段
- `codex.rs`: `set_toml_field` 单元测试 — TOML 格式保持
- `providers.rs`: 切换供应商只改连接层的集成测试
- `EffortSlider`: vitest 组件测试

### Manual Verification
1. 切 Claude tab → 行为面板可见 → 拖 Effort slider → 立即检查 `~/.claude/settings.json`
2. 切 Codex tab → 行为面板可见 → 改 approval_policy → 检查 `~/.codex/config.toml`
3. 切 OpenCode tab → 行为面板可见 → 改 default_agent → 检查 `~/.config/opencode/opencode.json`
4. 三个 App 各自切换供应商 → 行为字段不变
5. 打开配置文件编辑器 → JSON/TOML 和 UI 控件读到同一份数据

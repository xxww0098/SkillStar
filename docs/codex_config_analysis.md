# Codex 配置体系深度分析（v3.1 — 含重构最佳实践）

## 核心结论

Codex 有 **2 种认证方式**，它们共享同一个 `auth.json`；同时，它将**连接设置**与**行为设置**共享同一个 `config.toml`。这种共享设计要求 SkillStar 在写入时必须采用**精细化合并（Merge）**策略，而绝不能使用全量覆盖。

---

## 一、认证方式：2 种共存的混合结构

| 方式 | 谁用 | `auth.json` 里存什么 | 谁写入 |
|------|------|-------------------|--------|
| **ChatGPT 登录**（OAuth） | ChatGPT Plus/Enterprise 订阅用户 | 浏览器回传的 session token（`access_token` / `refresh_token` 等） | **Codex CLI 自己写** |
| **API Key** | Platform 按量付费用户 + 第三方供应商 | 环境变量名 → 密钥字符串（如 `OPENAI_API_KEY: "sk-..."`） | **用户 / SkillStar 写** |

### auth.json 的真实结构

```json
{
  // ─── ChatGPT OAuth 会话（Codex CLI 自动管理） ───
  // 不能破坏，否则导致用户被自动登出
  "access_token": "eyJhbGciOi...",
  "refresh_token": "rt-xxxx...",
  "expires_at": 1712345678,
  
  // ─── API Keys（用户手动管理） ───
  "OPENAI_API_KEY": "sk-openai-xxx",
  "DEEPSEEK_API_KEY": "sk-deepseek-xxx"
}
```

> [!WARNING]
> **Merge-Not-Overwrite!** SkillStar 的 `save_codex_auth` 必须只更新传入的 API Key 字段。绝不能用 `serde_json::to_string` 直接覆盖整个结构。

---

## 二、配置合并法则：`toml_edit` 局部更新

`config.toml` 中既包含**供应商配置**（API URL、Model），也包含全局的**行为配置**（如 `model_reasoning_effort`、`approval_policy`）。

### 错误的做法（v2 遗留）
直接将第三方供应商的 `settingsConfig.config` 字符串写入 `config.toml`：
```rust
// ❌ 错误：如果 provider_config_text 是空字符串（如官方 OpenAI），会将整个配置清空！
codex::write_config(&provider_config_text)?;
```

### 正确的做法（v3.1 引入）
使用 `toml_edit` 进行语法树级别的合并（DOM-like merge）。切换供应商时，仅更新连接指针，保留用户的全局行为设置。

```rust
// ✅ 正确：只合并所需的字段和表格，保留无相关的设置
let mut doc: DocumentMut = existing_text.parse().unwrap();

// 如果是第三方：
doc["model"] = value("gpt-4o");
doc["model_provider"] = value("deepseek");
// 插入 [model_providers.deepseek] 块...

// 如果是官方直连（空配置）：
doc.remove("model_provider");
doc.remove("openai_base_url");
// 写入
codex::write_config(&doc.to_string())?;
```

---

## 三、完整场景矩阵与 UI 设计原则

| # | 场景 | TOML 指针状态 | `auth.json` 状态 | UI 行为要求 |
|---|------|------------|-----------|-------------|
| **A** | ChatGPT 登录 | 无 `model_provider` | 存在 `access_token` | 显示 🟢 "已通过 ChatGPT 登录"的盾牌 banner，API Key 留空 |
| **B** | OpenAI API Key | 无 `model_provider` | `OPENAI_API_KEY: "sk..."` | 正常显示 API Key 输入框 |
| **C** | 第三方供应商 | 设置了 `model_provider="xxx"` | `<ENV_KEY>: "sk..."` | 显示 API Key + Endpoint 输入框 |

### 关键 UI 原则总结

1. **始终显示"当前模型"输入框**：
   在最初的设计中，只有检测到 TOML 里有 `model = "..."` 才会渲染 `<ModelInput>`。这导致如果用户是使用默认配置（没有显式指定 model 行），输入框就不会出现。
   **修正**：无论 TOML 是否含有 model 行，始终渲染该输入框。当 TOML 中没有时，`onChange` 事件应当执行前置追加（Prepend）操作：`model = "${v}"\n${configText}`。

2. **区分展示与保存的数据模型**：
   从后端 `get_codex_auth()` 会拉取完整的含 OAuth 的 token 回来用于回显 API Key，但前端的 `save()` 应该只向后端发送发生实际改变的 `dirtyFields`（增量 Patch），防止并发写入时产生覆盖冲突。前端还需要监听后端的 `get_codex_auth_status` 获取结构化的安全字段存在标志（如 `hasChatgptSession: true`）。

3. **双栏透明机制**：
   由于所有第三方配置都被压在同一个 `config.toml` 文件中，底层到底发生了什么对用户极不透明。利用一个带 `ChevronDown` 箭头可折叠的 **原始 TOML 编辑器框** 放在卡片底部，让高玩可以随时查看 SkillStar 生成的 TOML 语法树是否符合预期，是一种兼顾易用与高级控制的好设计。

---

## 四、安全与高级配置：沙盒内的环境变量穿透

随着对 Codex 能力挖掘的加深，不可避免地要求执行各类带权限的命令和脚本（例如使用 `GH_TOKEN` 操作代码仓库）。为了将凭据注入 Agent 并打穿安全屏障，必须在 `config.toml` 中精细调校 `shell_environment_policy` 设置。此配置涉及严重的系统破坏和敏感凭证泄漏风险，**从产品定义上必须归类到「高级配置」中。**

### 1. 终极的变量投递配方

如果确实需要向 Sandbox 的 Shell 注入 Token，该块的标准形式如下：

```toml
[shell_environment_policy]
# 1. 以“核心精简变量集”作为初始隔离沙箱，取代默认的 all
inherit = "core"
# 2. 【高风险开关】：禁用系统底层的“含 KEY/SECRET/TOKEN 关键词变量自动阻挠”机制
ignore_default_excludes = true

# 3. 【极度关键白名单】：不仅要有业务 Token，还必须挂载系统基础指令变量集。
#    否则 Codex 子进程将变得无法执行类似 `ls` 或 `git` 命令。
include_only = [
  "PATH", "HOME", "LANG", "TERM", "SHELL",
  "GH_TOKEN"          # ← 这里最终混入欲投递的值
]
```

### 2. 结合 UI 的具体产品抽象

正如“双栏透明机制”强调的，面对此类**高杠杆、易翻车**的参数，推荐在管理面板：
1. **开箱即用层**：UI 做可视化“关联 GitHub”或“增加特权白名单”按钮，用户只负责在密码框填写；SkillStar 后端拦截到写入时，由后端的 Rust 逻辑智能合并并安全补全 `PATH`, `inherit` 等枯燥基础节点。
2. **“重度硬核”高级选项**：在折叠起来的高级编辑区内，为高端玩家展露这部分结构并可自行编辑。必须附加明显的 **[警告]** 标识，阐明去掉 `include_only` 会触发本机所有凭据泄露，或者漏加 `PATH` 将导致 Agent 失明变蠢。

# Launch Deck：可视化多 CLI 编排启动（最终版）

## 用户需求

在 SkillStar **Projects 页面**内，每个项目支持**两种启动模式**：
- **单终端模式（Single）** — 直接在 Terminal.app 打开一个 CLI，无需 tmux
- **多终端模式（Multi）** — tmux 多面板，支持可视化布局设计

模式可切换，系统记住每个项目的配置。

## 已确认事项

| 事项 | 决定 |
|---|---|
| tmux 依赖 | 仅 Multi 模式需要，未安装时提示 `brew install tmux` |
| API Key 安全 | 接受明文内联命令行 |
| 支持的 Agent | **仅 4 个 CLI**：Claude Code / Codex CLI / OpenCode / Gemini CLI |
| **启动限制** | **严禁启动桌面 App** — 只 spawn CLI 二进制 |
| 入口位置 | Projects 页面 ProjectDetailPanel 新增折叠 section |
| 配置持久化 | 每个项目一个 launch config，保存在 `launch_decks.json` |

---

## 双模式数据模型

```typescript
interface LaunchConfig {
  id: string;                    // 与 project name 一一对应
  projectName: string;
  mode: "single" | "multi";     // 启动模式
  layout: LayoutNode;           // 布局树（single 模式时只有一个叶节点）
  updatedAt: number;
}

// 布局二叉树
type LayoutNode =
  | { type: "split"; direction: "h" | "v"; ratio: number; children: [LayoutNode, LayoutNode] }
  | { type: "pane"; id: string; agentId: string; providerId?: string; providerName?: string;
      safeMode: boolean; extraArgs: string[] };
```

模式切换逻辑：
- **Single → Multi**：当前单面板保留为布局树的第一个叶节点，用户可继续分割
- **Multi → Single**：如果有多个面板，提示用户选择保留哪个面板（或保留第一个）

---

## 深度 Bug 分析与修复

### Bug 1：tmux pane 索引计算错误 ⚠️ 严重

**原方案问题**：原递归算法用全局自增 `pane_idx` 计数器，假设 tmux pane 按顺序编号。但当对左子树递归分割时，`pane_idx` 已经超前，导致后续 split 命令的 `-t` 目标指向错误的 pane。

**修复：分配式模型**

```rust
/// 正确的递归算法：每个节点知道自己"拥有"哪个 pane
fn collect_commands(
    node: &LayoutNode,
    allocated_pane: usize,     // 此子树被分配的 pane ID
    next_pane: &mut usize,     // 全局下一个可用 pane ID
    splits: &mut Vec<String>,  // Phase 1: split 命令
    cmds: &mut Vec<(usize, String)>, // Phase 2: (pane_id, command) 对
    project_path: &str,
) {
    match node {
        Pane { app_id, .. } => {
            cmds.push((allocated_pane, build_command(app_id, ...)));
        }
        Split { direction, ratio, children } => {
            let new_pane = *next_pane;
            *next_pane += 1;
            let flag = if *direction == H { "-h" } else { "-v" };
            let percent = ((1.0 - ratio) * 100.0) as u32;
            splits.push(format!(
                "tmux split-window {flag} -t \"$S:0.{allocated_pane}\" -p {percent} -c \"$D\""
            ));
            // 左/上子树 → 使用原始 pane（被 split 后缩小但保持原 ID）
            collect_commands(&children[0], allocated_pane, next_pane, splits, cmds, project_path);
            // 右/下子树 → 使用新创建的 pane
            collect_commands(&children[1], new_pane, next_pane, splits, cmds, project_path);
        }
    }
}
```

**验证**（5 面板）：
```
Tree: HSplit(0.6, [VSplit(0.5, [A, B]), VSplit(0.33, [C, HSplit(0.5, [D, E])])])

Step 1: HSplit → split pane 0 horizontally, new pane = 1
         Left subtree gets pane 0, right subtree gets pane 1
Step 2: VSplit(left) → split pane 0 vertically, new pane = 2
         A gets pane 0, B gets pane 2
Step 3: VSplit(right) → split pane 1 vertically, new pane = 3
         C gets pane 1, HSplit subtree gets pane 3
Step 4: HSplit → split pane 3 horizontally, new pane = 4
         D gets pane 3, E gets pane 4

Generated splits:
  tmux split-window -h -t 0 -p 40    # → pane 1
  tmux split-window -v -t 0 -p 50    # → pane 2
  tmux split-window -v -t 1 -p 67    # → pane 3
  tmux split-window -h -t 3 -p 50    # → pane 4

Send commands:
  pane 0 → A, pane 2 → B, pane 1 → C, pane 3 → D, pane 4 → E
```

### Bug 2：两阶段脚本生成

**问题**：如果 split 和 send-keys 交替执行，CLI 启动可能干扰 tmux 布局调整。

**修复**：将脚本分为两阶段：
```bash
#!/bin/bash
S="ss-myproject"; D="/path/to/project"
tmux kill-session -t "$S" 2>/dev/null

# ── Phase 1: 创建所有面板 ──
tmux new-session -d -s "$S" -c "$D"
tmux split-window -h -t "$S:0.0" -p 40 -c "$D"
tmux split-window -v -t "$S:0.0" -p 50 -c "$D"

# ── Phase 2: 发送命令 ──
tmux send-keys -t "$S:0.0" 'ANTHROPIC_API_KEY="sk-xxx" claude' Enter
tmux send-keys -t "$S:0.1" 'OPENAI_API_KEY="sk-yyy" codex' Enter
tmux send-keys -t "$S:0.2" 'GEMINI_API_KEY="zzz" gemini' Enter

# ── Phase 3: 附加并自删除 ──
tmux attach -t "$S"
rm -f "$0"    # 脚本自删除
```

### Bug 3：临时脚本文件残留

**问题**：shell 脚本写到 `/tmp/` 后通过 Terminal.app 执行，但进程结束后文件残留。

**修复**：脚本末尾加 `rm -f "$0"` 自删除。

### Bug 4：部署前验证缺失

**问题**：如果用户配置的 agent CLI 未安装，tmux 面板会显示 "command not found"。

**修复**：
```rust
fn validate_before_deploy(layout: &LayoutNode) -> Result<(), Vec<String>> {
    let mut errors = vec![];
    for pane in collect_leaf_panes(layout) {
        // 检查 agent 是否已选择
        if pane.app_id.is_empty() {
            errors.push(format!("面板 {} 未指定 Agent CLI", pane.id));
        }
        // 检查 CLI 二进制是否存在
        if !pane.app_id.is_empty() && find_cli_binary(&pane.app_id).is_none() {
            errors.push(format!("{} CLI 未安装", pane.app_id));
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors) }
}
```

### Bug 5：tmux session 命名冲突

**问题**：session 名 `skillstar-myproject` 可能与用户其他 tmux session 冲突。

**修复**：使用前缀 `ss-` + 项目名 hash 后缀：
```rust
fn session_name(project_name: &str) -> String {
    let hash = &format!("{:x}", md5(project_name))[..6];
    format!("ss-{}-{}", sanitize(project_name), hash)
}
```

### Bug 6：Provider 环境变量提取不完整

**问题**：不同 agent 的 provider settingsConfig 结构不同，提取逻辑需覆盖所有格式。

**修复**：每个 agent 独立的提取函数 + 完整单元测试：

```rust
fn extract_env_for_claude(provider: &ProviderEntry) -> HashMap<String, String> {
    // 从 settingsConfig.env 提取 ANTHROPIC_* 变量
    let env = provider.settings_config.get("env").and_then(|v| v.as_object());
    // ...
}

fn extract_env_for_codex(provider: &ProviderEntry) -> HashMap<String, String> {
    // 从 settingsConfig.auth 提取 OPENAI_API_KEY
    // 从 settingsConfig.config (TOML) 提取 base_url
    // ⚠️ Codex 的 model_providers 表无法通过 env var 表达
    //     → 此类 provider 标记为 "不支持 Launch"
}

fn extract_env_for_gemini(_provider: &ProviderEntry) -> HashMap<String, String> {
    // GEMINI_API_KEY — 从 provider meta 或新增字段提取
}
```

---

## 性能优化

### 优化 1：SplitHandle 拖拽不触发 React 重渲染

**问题**：拖拽分割线时每帧更新 `ratio` → 整棵布局树重渲染 → 卡顿。

**修复**：拖拽期间只操作 CSS，mouseup 时才更新 React state：

```tsx
function SplitHandle({ onRatioChange, direction }) {
  const handleRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const onMouseDown = useCallback((e: MouseEvent) => {
    const container = containerRef.current?.parentElement;
    if (!container) return;

    const onMouseMove = (e: MouseEvent) => {
      // 直接操作 CSS flex-basis，不触发 React setState
      const rect = container.getBoundingClientRect();
      const newRatio = direction === "h"
        ? (e.clientX - rect.left) / rect.width
        : (e.clientY - rect.top) / rect.height;
      const clamped = Math.max(0.15, Math.min(0.85, newRatio));
      container.style.setProperty("--split-ratio", String(clamped));
    };

    const onMouseUp = (e: MouseEvent) => {
      // 只在松手时更新 React state
      const ratio = parseFloat(container.style.getPropertyValue("--split-ratio") || "0.5");
      onRatioChange(ratio); // 触发一次 setState
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [direction, onRatioChange]);

  // ...
}
```

### 优化 2：递归组件 memo 化

```tsx
const PaneCell = React.memo(function PaneCell({ pane, onSplit, onAssign, onRemove }) {
  // ...
});

const LayoutRenderer = React.memo(function LayoutRenderer({ node, onSplit, ... }) {
  if (node.type === "pane") return <PaneCell ... />;
  return (
    <div style={{ display: "flex", flexDirection: node.direction === "h" ? "row" : "column" }}>
      <div style={{ flex: `var(--split-ratio, ${node.ratio})` }}>
        <LayoutRenderer node={node.children[0]} ... />
      </div>
      <SplitHandle ... />
      <div style={{ flex: `calc(1 - var(--split-ratio, ${node.ratio}))` }}>
        <LayoutRenderer node={node.children[1]} ... />
      </div>
    </div>
  );
});
```

### 优化 3：CLI 二进制检测缓存

```typescript
// useAgentClis.ts — 只在 mount 时检测一次，不是每次渲染
function useAgentClis() {
  const [agents, setAgents] = useState<AgentCliInfo[]>([]);
  useEffect(() => {
    invoke("list_available_agent_clis").then(setAgents);
  }, []); // 空依赖 = 只调一次
  return agents;
}
```

### 优化 4：持久化读写不阻塞

后端 `launch_deck.rs` 的读写操作使用 `spawn_blocking` 避免阻塞 tokio runtime：

```rust
#[tauri::command]
pub async fn save_launch_config(config: LaunchConfig) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        core::launch_deck::save_config(&config)
    }).await.map_err(|e| AppError::from(anyhow::anyhow!(e)))?
}
```

---

## 具体变更

### 组件 1：后端 — 数据模型与持久化

#### [NEW] [launch_deck.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/core/launch_deck.rs)

```rust
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum LaunchMode { Single, Multi }

#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub enum SplitDirection { #[serde(rename = "h")] H, #[serde(rename = "v")] V }

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum LayoutNode {
    #[serde(rename = "split")]
    Split { direction: SplitDirection, ratio: f64, children: Box<[LayoutNode; 2]> },
    #[serde(rename = "pane")]
    Pane {
        id: String,
        app_id: String,              // "claude" | "codex" | "opencode" | "gemini" | ""
        provider_id: Option<String>,
        provider_name: Option<String>,
        safe_mode: bool,
        extra_args: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LaunchConfig {
    pub project_name: String,       // 主键，一个项目一个配置
    pub mode: LaunchMode,
    pub layout: LayoutNode,
    pub updated_at: u64,
}

// 持久化路径：~/.skillstar/config/launch_configs.json
// 结构：HashMap<project_name, LaunchConfig>

pub fn load_config(project_name: &str) -> Option<LaunchConfig>;
pub fn save_config(config: &LaunchConfig) -> Result<()>;
pub fn delete_config(project_name: &str) -> Result<()>;

/// 验证配置是否可部署
pub fn validate(config: &LaunchConfig) -> Result<(), Vec<String>>;

/// 统计面板数
pub fn count_panes(node: &LayoutNode) -> usize;

/// 创建默认单面板配置
pub fn default_config(project_name: &str) -> LaunchConfig;
```

#### [NEW] [terminal_backend.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/core/terminal_backend.rs)

```rust
/// 检测 tmux
pub fn is_tmux_available() -> bool;
pub fn tmux_version() -> Option<String>;

/// CLI 二进制检测
pub fn find_cli_binary(agent_id: &str) -> Option<PathBuf>;
pub fn list_available_clis() -> Vec<AgentCliInfo>;

/// --- 单终端模式 ---
/// 生成直接执行的 shell 命令（无 tmux）
pub fn generate_single_script(pane: &LayoutNode, project_path: &str) -> String;

/// --- 多终端模式 ---
/// 两阶段生成 tmux 脚本（分配式 pane ID 算法 + split/send 分离）
pub fn generate_multi_script(config: &LaunchConfig, project_path: &str) -> String;

/// 部署执行
pub fn deploy(config: &LaunchConfig, project_path: &str) -> Result<DeployResult>;
```

单终端模式脚本（无 tmux 依赖）：
```bash
#!/bin/bash
cd "/path/to/project"
ANTHROPIC_API_KEY="sk-xxx" ANTHROPIC_BASE_URL="https://..." claude
rm -f "$0"
```

多终端模式脚本（两阶段）：
```bash
#!/bin/bash
S="ss-myproject-a1b2c3"; D="/path/to/project"
tmux kill-session -t "$S" 2>/dev/null

# ── Phase 1: 创建面板 ──
tmux new-session -d -s "$S" -c "$D"
tmux split-window -h -t "$S:0.0" -p 40 -c "$D"
tmux split-window -v -t "$S:0.0" -p 50 -c "$D"

# ── Phase 2: 发送命令 ──
tmux send-keys -t "$S:0.0" 'ANTHROPIC_API_KEY="sk-xxx" claude' Enter
tmux send-keys -t "$S:0.1" 'OPENAI_API_KEY="sk-yyy" codex' Enter
tmux send-keys -t "$S:0.2" 'GEMINI_API_KEY="zzz" gemini' Enter

# ── Phase 3: 附加 + 清理 ──
tmux attach -t "$S"
rm -f "$0"
```

---

### 组件 2：后端 Tauri 命令

#### [NEW] [commands/launch.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/commands/launch.rs)

```rust
#[tauri::command]
pub async fn get_launch_config(project_name: String) -> Result<Option<LaunchConfig>, AppError>;

#[tauri::command]
pub async fn save_launch_config(config: LaunchConfig) -> Result<(), AppError>;

#[tauri::command]
pub async fn delete_launch_config(project_name: String) -> Result<(), AppError>;

/// 部署：生成脚本 → 验证 → 打开终端
#[tauri::command]
pub async fn deploy_launch(project_name: String, project_path: String) -> Result<DeployResult, AppError>;

/// 检测 tmux 可用性
#[tauri::command]
pub async fn check_tmux() -> Result<TmuxStatus, AppError>;

/// 列出系统中可用的 agent CLI
#[tauri::command]
pub async fn list_agent_clis() -> Result<Vec<AgentCliInfo>, AppError>;
```

#### [MODIFY] [commands.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/commands.rs) — 注册 `launch` 模块

#### [MODIFY] [lib.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/lib.rs) — 注册命令

---

### 组件 3：CLI — `skillstar launch`

#### [MODIFY] [cli.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/cli.rs)

```rust
Launch {
    #[command(subcommand)]
    action: LaunchAction,
}

enum LaunchAction {
    /// 部署项目的 Launch 配置
    Deploy { project_name: String },
    /// 直接启动单个 agent CLI
    Run {
        agent: String,  // claude | codex | opencode | gemini
        #[arg(long)] provider: Option<String>,
        #[arg(long)] safe: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}
```

#### [MODIFY] [main.rs](file:///Users/xxww/Code/REPO/SkillStar/src-tauri/src/main.rs) — 添加 `"launch"`

---

### 组件 4：前端 — 可视化布局设计器

#### [NEW] `src/features/launch/`

```
src/features/launch/
├── components/
│   ├── LaunchDeckSection.tsx       # ProjectDetailPanel 折叠 section 入口
│   ├── PaneLayoutEditor.tsx        # 核心：递归渲染布局树
│   ├── PaneCell.tsx                # 单个面板色块 (memo)
│   ├── SplitHandle.tsx             # 分割线拖拽 (CSS-only drag)
│   ├── PaneConfigPopover.tsx       # 面板配置弹出框
│   ├── TmuxPrompt.tsx              # tmux 未安装提示
│   ├── ModeSwitch.tsx              # Single / Multi 模式切换
│   └── DeployButton.tsx            # 部署按钮 + 验证状态
└── hooks/
    ├── useLaunchConfig.ts          # 单项目 config CRUD + 自动保存
    ├── useLayoutTree.ts            # 二叉树操作（split/remove/resize/assign）
    └── useAgentClis.ts             # CLI 可用性（mount 一次）
```

#### `LaunchDeckSection.tsx` — 入口

```tsx
// ProjectDetailPanel 中 AgentAccordion 之后
<LaunchDeckSection
  projectName={selectedProject.name}
  projectPath={selectedProject.path}
/>
```

内部结构：
```
┌─────────────────────────────────────────────────────┐
│ 🚀 Launch                    [Single ○ | ● Multi]   │  ← 标题 + 模式切换
├─────────────────────────────────────────────────────┤
│                                                     │
│   ┌────────────────────┬───────────────────────┐    │
│   │  🟠 Claude Code    │   🟢 Codex CLI        │    │
│   │  OpenRouter        │   官方                │    │  ← PaneLayoutEditor
│   │         [↔] [↕]    │        [↔] [↕]        │    │     (Multi 模式显示)
│   ├────────┬───────────┴───────────────────────┤    │
│   │ 🔵 Gem │   ┌─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐        │    │
│   │ API Key│   │     + 选择 Agent     │        │    │
│   └────────┴───┴─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘────────┘    │
│                                                     │
│  ⚠️ tmux 未安装                                     │  ← TmuxPrompt (条件显示)
│  brew install tmux                    [📋 复制]     │
│                                                     │
├─────────────────────────────────────────────────────┤
│  [自动保存 ✓]                        [🚀 部署]      │  ← 底部操作栏
└─────────────────────────────────────────────────────┘
```

Single 模式下只显示一个大面板，不显示分割按钮。

#### `ModeSwitch.tsx` — 模式切换

```tsx
<div className="flex items-center rounded-lg border border-border p-0.5 bg-muted/30">
  <button
    className={cn("px-3 py-1 rounded-md text-xs", mode === "single" && "bg-card shadow-sm")}
    onClick={() => onModeChange("single")}
  >
    单终端
  </button>
  <button
    className={cn("px-3 py-1 rounded-md text-xs", mode === "multi" && "bg-card shadow-sm")}
    onClick={() => onModeChange("multi")}
  >
    多面板
  </button>
</div>
```

#### `useLaunchConfig.ts` — 自动保存

```typescript
function useLaunchConfig(projectName: string) {
  const [config, setConfig] = useState<LaunchConfig | null>(null);
  const [saving, setSaving] = useState(false);

  // 加载
  useEffect(() => {
    if (!projectName) return;
    invoke("get_launch_config", { projectName }).then((c) => {
      setConfig(c ?? defaultConfig(projectName));
    });
  }, [projectName]);

  // 防抖自动保存（配置变更后 800ms 自动存盘）
  useEffect(() => {
    if (!config) return;
    const timer = setTimeout(async () => {
      setSaving(true);
      await invoke("save_launch_config", { config });
      setSaving(false);
    }, 800);
    return () => clearTimeout(timer);
  }, [config]);

  return { config, setConfig, saving };
}
```

---

## 新增依赖

| Crate | Purpose |
|---|---|
| `which` | CLI 二进制路径查找（使用 `cargo add which`） |

前端无新增依赖。

---

## 验证计划

### 自动化测试

**后端**：
- `launch_deck.rs`：LayoutNode 序列化/反序列化、默认配置创建、validate 逻辑
- `terminal_backend.rs`：
  - 分配式 pane ID 算法正确性（多种树形状：线性链、完全二叉树、左偏树、右偏树）
  - 单终端脚本生成
  - 多终端两阶段脚本生成（split 在前、send-keys 在后）
  - 环境变量提取（每种 agent type）
  - `validate` 拦截未配置面板、未安装 CLI
- `cargo build` 通过

**前端**：
- `useLayoutTree` 测试：split → 树结构正确、remove → 兄弟提升、resize → ratio 范围限制
- `bun run lint` 通过

### 手动验证
1. 安装 tmux：`brew install tmux`
2. 单终端模式：选择 agent + provider → 部署 → Terminal.app 打开 CLI
3. 多终端模式：创建 3+ 面板布局 → 部署 → tmux 按设计布局打开
4. 模式切换：Multi → Single → 验证配置保留
5. tmux 未安装时：Multi 模式显示安装提示
6. 配置持久化：切换项目 → 切回 → 布局配置恢复
7. 验证 Codex 只启动 CLI，不启动 App
8. CLI：`skillstar launch deploy myproject`

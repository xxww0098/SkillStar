<div align="center">

<img src="./public/skillstar-icon.svg" alt="SkillStar Logo" width="110" />

# SkillStar 技能星球

### _Your Second Brain for Agent CLIs_

**统一编排 Skill，按项目精准分发到不同 Agent CLI。**

[![Version](https://img.shields.io/badge/version-0.1.2-blueviolet)](https://github.com/xxww0098/SkillStar/releases/latest)
[![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue?logo=tauri&logoColor=white)](https://v2.tauri.app)
[![React 18](https://img.shields.io/badge/React-18-61dafb?logo=react&logoColor=white)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](./LICENSE)

</div>

## SkillStar 是什么
SkillStar 是一个 Tauri 桌面应用（也支持 CLI），用于统一管理 AI Agent Skills：

- 从 `skills.sh` 或 GitHub 仓库安装技能
- 在 `My Skills` 里维护技能和 Agent 链接
- 用 `Decks` 组合并分发一组技能
- 在 `Projects` 里按项目/按 Agent 精准同步
- 通过符号链接（symlink）实现零文件拷贝、低污染工作流

<br/>

### SkillStar 是怎么工作的？

<div align="center">
  <img src="./public/diagrams/skillstar-architecture.svg" width="100%" alt="SkillStar Architecture — 技能从哪来 → 怎么管 → 怎么用" />
</div>

<br/>

## 核心能力

### ⚡️ 核心分发与管理
- **多 Agent 生态**: 原生支持 Gemini CLI、Claude Code、Codex CLI、OpenCode CLI、OpenClaw、Antigravity，还支持自定义 Agent 无限扩展。
- **纯 Symlink 注入**: 项目目录不落地副本，完全无侵入，避免触发 `git status` 污染或影响构建产物。
- **项目级精准调度**: 在 `Projects` 中注册工程，按需为不同 Agent 配置独立技能池，自动处理共享路径和冲突。

### 🧠 AI 深度赋能
- **AI 辅助阅读与翻译**: 基于 SQLite 持久化缓存的流式 SKILL.md 翻译与摘要，短文本采用双引擎并行加速。
- **智能技能推荐 (Smart Pick)**: 本地先验排序大模型结合多轮共识打分，在海量技能中精准推荐最匹配当前任务的工具。

### 🛡️ 安全与可信
- **三模安全雷达扫描**: 提供 Static（静态）、Smart（智能辅助）、Deep（大模型源码级深度推断） 三种安全扫描模式。
- **源与沙箱隔离**: Hub（全量远端拉取）与 Local（本地开发）存储分离，基于文件 Hash 校验缓存，一旦篡改自动拦截。

### 🛍️ 生态大市场
- **Local-first Marketplace**: 基于 SQLite + FTS 全文检索的本地聚合快照，无需等待网络加载，支持离线浏览和秒级响应。
- **Deck 组合与分发**: 一键将多个技能打包为 `.agd` 套牌，或提取单包 `.ags`。通过 Share Code 在网络/内网无缝流转。
- **本地创作周期 (Authoring)**: 图形化界面直接创建并编辑技能（`skills-local`），通过 GitHub CLI 一键发布至 GitHub 开源。

### 🖥️ 极客体验
- **Dark Glassmorphism UI**: 基于 Framer Motion 和 TailwindCSS 4 的沉浸式暗黑玻璃质感设计语言，动画无缝串联每个操作。
- **托盘后台自动巡检**: 通过系统 Tray 控制节点，低频静默更新云端知识与工具包，随时保持最新的能力集。
- **全界面双语适配**: 原生支持中文/英文（基于设备区域自动探测 + 个人强制配置）。

## 安装与 CLI 配置

### 1. macOS (苹果系统)
**通过 Homebrew 安装（推荐）**：
```bash
brew tap xxww0098/skillstar
brew install --cask skillstar
```
*注：Homebrew 会自动将 CLI 映射到全局路径，安装后即可在终端直接使用 `skillstar` 命令行入口。*

**下载 `.dmg` 手动安装**：
将应用拖入“应用程序”后，系统不会自动注册环境变量。需手动创建软链接以启用全局 CLI 调用：
```bash
sudo ln -sf /Applications/SkillStar.app/Contents/MacOS/SkillStar /usr/local/bin/skillstar
```

### 2. Windows 系统
**通过 `.exe` 安装程序**：
运行 Setup 安装程序时，SkillStar 默认会自动将安装目录加入到系统的**环境变量 (Path)**。
安装完成后，**重启终端 (PowerShell 或 CMD)** 即可在全局使用 `skillstar` 命令。

### 3. Linux 系统
- **`.deb` / `.rpm` 安装包**：使用包管理器（如 `apt`/`yum`）安装，会自动配置环境（位于 `/usr/bin/skillstar`），直接可用。
- **`.AppImage` 便携版**：赋予执行权限并放入系统 `PATH` 中：
  ```bash
  chmod +x SkillStar_x.x.x_amd64.AppImage
  sudo mv SkillStar_x.x.x_amd64.AppImage /usr/local/bin/skillstar
  ```

### 手动下载地址
从 [GitHub Releases](https://github.com/xxww0098/SkillStar/releases/latest) 下载获取所有平台的独立发行版包：

| 平台 | 安装包 |
|------|--------|
| macOS (Apple Silicon) | `SkillStar_x.x.x_aarch64.dmg` |
| macOS (Intel) | `SkillStar_x.x.x_x64.dmg` |
| Windows | `SkillStar_x.x.x_x64-setup.exe` |
| Linux | `SkillStar_x.x.x_amd64.AppImage` / `.deb` / `.rpm` |

> [!NOTE]
> macOS 首次启动若出现损坏提示，请在终端执行：
> ```bash
> xattr -cr /Applications/SkillStar.app
> ```

## 前置要求
至少安装一个 Agent CLI：

- [Gemini CLI](https://github.com/google-gemini/gemini-cli)
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
- [Codex CLI](https://github.com/openai/codex)
- [OpenCode](https://github.com/opencode-ai/opencode)
- [OpenClaw](https://github.com/openclaw/openclaw)
- [Antigravity](https://github.com/google-gemini/gemini-cli)

## 从源码构建
需要 [Bun](https://bun.sh/) 和 [Rust](https://rustup.rs/)：

```bash
git clone https://github.com/xxww0098/SkillStar.git
cd SkillStar
bun install
bun run tauri dev
bun run tauri build
```

## 典型工作流
1. `Marketplace` 浏览并安装技能
2. `My Skills` 管理技能、编辑 SKILL.md、配置 Agent 链接
3. `Security Scan` 扫描已安装技能的安全风险（支持 AI 深度分析）
4. `Decks` 组合技能并一键部署到项目
5. `Projects` 注册项目并执行按 Agent 同步
6. 需要命令行时使用内置 CLI（`skillstar list/install/update/...`）

## CLI 快速用法

SkillStar 提供了丰富的命令行工具用于无缝工作流集成。

### 1. 安装技能 (`install`)
```bash
# 默认：安装到 Hub 并链接到当前项目（自动识别项目内已有 agent 目录）
skillstar install https://github.com/user/my-agent-skill

# 全局：仅安装到 Hub，不写入当前项目目录
skillstar install --global https://github.com/user/my-agent-skill

# 指定项目目录执行项目级安装
skillstar install --project /path/to/project https://github.com/user/my-agent-skill

# 指定目标 agent（可重复或逗号分隔）
skillstar install --agent opencode https://github.com/user/my-agent-skill
skillstar install --agent codex,claude https://github.com/user/my-agent-skill

# 当一个仓库中包含多个技能时，可指定具体的技能名称
skillstar install --name cool-skill https://github.com/user/multi-skill-repo
```

### 2. 技能管理 (`list`, `update`, `scan`)
```bash
# 列表：查看已安装的所有技能
skillstar list

# 更新：更新特定的技能（如果省略名称，则更新所有技能）
skillstar update [name]

# 扫描：对指定技能目录执行安全威胁扫描（支持 AI 分析）
skillstar scan /path/to/skill
skillstar scan /path/to/skill --static-only  # 仅执行静态模式扫描，跳过 AI 分析
```

### 3. 创建与发布 (`create`, `publish`)
```bash
# 创建：在当前目录生成新的技能模板 (包含基础的 SKILL.md)
skillstar create

# 发布：将当前目录下的技能代码推送为 GitHub 开源技能（依赖 GitHub CLI）
skillstar publish
```

### 4. 工具包与健康维护 (`pack`, `doctor`)
```bash
# 列表：列出所有已安装的技能组合包 (Pack/Deck)
skillstar pack list

# 移除：卸载指定的组合包
skillstar pack remove <name>

# 健康检查：检查指定的包（或所有包）是否完整且正常存在于本地
skillstar doctor [name]
```

### 5. 启动图形界面 (`gui`)
```bash
# 从终端中强制唤起桌面图形界面 (GUI) 模式
skillstar gui
```

## 技术架构
| Layer | Technology | Purpose |
|-------|------------|---------|
| Desktop Shell | Tauri v2 | 桌面容器与 IPC |
| Backend | Rust 2024 + tokio + reqwest 0.13 | 业务逻辑与异步任务 |
| Git Engine | gix 0.80 (gitoxide) | 克隆/拉取/哈希对比 |
| Frontend | React 18 + TypeScript + Vite 5 | SPA UI |
| UI | TailwindCSS v4 + Framer Motion 12 + Radix | 设计系统与交互 |
| Storage | JSON files + SQLite | 配置持久化 + 翻译/安全扫描缓存 |
| Crypto | AES-256-GCM | API Key 加密存储 |

## 目录概览
```text
SkillStar/
├── src/                # React 前端
│   ├── hooks/          #   数据 hooks（skills, projects, marketplace, AI, updater, security）
│   ├── pages/          #   MySkills, Marketplace, SecurityScan, SkillCards, Projects, Settings
│   ├── components/     #   ui/, layout/, skills/, marketplace/, security/
│   ├── lib/            #   共享工具
│   └── types/          #   共享 TS 类型
├── src-tauri/          # Rust 后端（Tauri + CLI）
│   ├── src/commands/   #   marketplace, agents, projects, github, ai, patrol
│   ├── src/core/       #   domain modules（skills, sync, repo, security_scan, ai_provider ...）
│   └── prompts/        #   AI/Security 系统提示词
├── docs/
│   ├── Error.md           # 关键问题与修复记录
│   ├── CHANGELOG.md       # 版本变更日志
│   └── impeccable.md      # 设计语义与视觉基线
├── scripts/
│   ├── release/           # 发布脚本
│   ├── security_scan/     # 安全扫描脚本
│   └── internal/          # 内部维护脚本
├── AGENTS.md              # 后端/全局工程规范
└── AGENTS-UI.md           # 前端规范
```

## 支持的 Agent CLI
| Agent | Global Config |
|------|----------------|
| Gemini CLI | `~/.gemini/` |
| Antigravity | `~/.gemini/antigravity/` |
| Claude Code | `~/.claude/` |
| Codex CLI | `~/.codex/` |
| OpenCode CLI | `~/.config/opencode/` |
| OpenClaw | `~/.openclaw/` |
| Cursor | `~/.cursor/` |
| Qoder | `~/.qoder/` |
| Trae | `~/.trae/` |
| **自定义 Agent** | 自由配置路径，无限扩展 |

## 开发与协作
- 后端结构或流程调整：先更新 `AGENTS.md`
- 前端结构或交互规范调整：先更新 `AGENTS-UI.md`
- 重要 bug 修复：在 `docs/Error.md` 追加条目

## 许可证
[MIT](./LICENSE)

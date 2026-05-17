<div align="center">

<img src="./public/skillstar-icon.svg" alt="SkillStar Logo" width="110" />

# SkillStar 技能星球

### _Your Second Brain for Agent CLIs_

**统一编排 Skill、按项目精准分发到不同 Agent CLI，并实时洞察各家 AI 订阅的余额与到期。**

[![Tauri v2](https://img.shields.io/badge/Tauri-v2-blue?logo=tauri&logoColor=white)](https://v2.tauri.app)
[![React 19](https://img.shields.io/badge/React-19-61dafb?logo=react&logoColor=white)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-green.svg)](./LICENSE)

</div>

## SkillStar 是什么

SkillStar 是一个 Tauri 桌面应用（同时附带 CLI），围绕 AI Agent 开发者三件事：

1. **Skill 管理与分发** — 从 `skills.sh` / GitHub 拉技能，按项目精准同步到 Claude Code、Codex、Gemini CLI、Antigravity、Cursor、Qoder、Trae、OpenCode、OpenClaw 等 Agent。
2. **Model 配置** — 集中管理各家 API Provider、配额、健康度，按工具维度切换底层模型。
3. **Usage 用量** — 把分散在各家控制台的订阅、余额、Token 配额、续费日期、套餐徽章聚合到本地一个面板，到期 / 用量预警直接弹。

侧边栏顶部的胶囊 toggle 在三种 mode 之间切换：

```
┌──────────────┐
│  ▣  ◴  ▤   │   Skills · Usage · Models
└──────────────┘
```

---

## 核心能力

### ⚡️ Skill 管理与分发

- **多 Agent 生态**: 原生支持 Gemini CLI、Claude Code、Codex CLI、OpenCode、OpenClaw、Antigravity、Cursor、Qoder、Trae，自定义 Agent 任意扩展。
- **纯 Symlink 注入**: 项目目录不落地副本，无侵入，避免 `git status` 污染或影响构建产物。
- **项目级精准调度**: 在 `Projects` 注册工程，按需为不同 Agent 配置独立技能池，自动处理共享路径和冲突。
- **Local-first Marketplace**: SQLite + FTS 全文检索的本地聚合快照，无需等待网络加载，离线可用，秒级响应。
- **Deck 组合分发**: 一键将多个技能打包为 `.agd` 套牌，或提取单包 `.ags`。通过 Share Code 在网络/内网无缝流转。
- **本地创作周期 (Authoring)**: 图形化界面直接创建并编辑技能（`skills-local`），通过 GitHub CLI 一键发布开源。

### 📊 Usage 用量记账（v0.3 新增）

把所有 AI 订阅的额度和续费日聚合到一个手机桌面式网格面板。

- **18 家固定 catalog**：
  - **OAuth 自动同步（5 家）**：Cursor (PKCE + 轮询) · Codex (OpenAI OAuth) · Antigravity (Google OAuth) · Trae (区域感知 CN/SG/US/TTP) · Qoder (Device Flow + `/api/v2/user/plan`)
  - **API Key 自动同步（4 家）**：DeepSeek (`/user/balance`) · 智谱 GLM Coding Plan (`/monitor/usage/quota/limit`，5h + 周窗口 + `data.level`) · MiniMax Token Plan (`/v1/token_plan/remains`) · Kimi (`/v1/users/me/balance`)
  - **手动录入（9 家）**：小米 MiMo · 火山方舟 · 腾讯 Hy3 · 阶跃 Step · 阿里百炼 · 讯飞 Astron · 百川 · 零一万物 · 商汤日日新
- **卡片信息**：右上角 plan 徽章（PRO 蓝/PLUS 绿/MAX·ULTRA 紫/TEAM·BUSINESS 橙/ENTERPRISE 红/FREE·PAYG 灰），5h/7d/月度自适应进度条，余额型展示 CNY/USD，续费倒计时
- **告警**：5h 剩余 < 20% 黄 / < 5% 红，7 天内到期 banner，OAuth 401 时卡片红框提示重新登录
- **加密存储**：AES-256-GCM + machine_uid 派生密钥，API key / access_token / refresh_token 全部加密，存放于 `~/.skillstar/config/usage/`
- **OAuth 体验**：表单选 OAuth → 点击「用浏览器登录」→ 系统浏览器打开 → 后端长轮询完成 → 卡片自动同步用量
- **月度支出**：Header 按 billing cycle (Monthly/Annual/OneTime) 折算并按币种分组累加

> ⚠️ Cursor / Codex / Antigravity / Trae / Qoder 的 OAuth 端点是社区蓝本（cockpit-tools），随官方升级可能失效，失败时可降级为手动录入。

### 🧠 AI 深度赋能

- **AI 辅助阅读与翻译**: 基于 SQLite 持久化缓存的流式 SKILL.md 翻译与摘要，短文本双引擎并行加速。
- **智能技能推荐 (Smart Pick)**: 本地先验排序 + 大模型多轮共识打分，从海量技能里精准推荐最匹配当前任务的工具。

### 🛡️ 安全与可信

- **三模安全雷达扫描**: Static（静态规则）、Smart（智能辅助）、Deep（大模型源码级深度推断）三档扫描模式。
- **源与沙箱隔离**: Hub（远端拉取）与 Local（本地开发）存储分离，文件 Hash 校验缓存，一旦篡改自动拦截。

### 🖥️ 极客体验

- **Dark Glassmorphism UI**: Framer Motion + TailwindCSS 4 沉浸式暗黑玻璃质感设计，每个动画都精雕。
- **托盘后台自动巡检**: 系统 Tray 控制节点，低频静默更新云端知识与工具包。
- **全界面双语适配**: 原生 中文/英文（基于设备区域自动探测 + 个人强制配置）。

---

## 安装

### 1. macOS

**Homebrew（推荐）**:

```bash
brew tap xxww0098/skillstar
brew install --cask skillstar
```

**手动 `.dmg`**:

```bash
sudo ln -sf /Applications/SkillStar.app/Contents/MacOS/SkillStar /usr/local/bin/skillstar
```

> macOS 首次启动若提示"已损坏":
>
> ```bash
> xattr -cr /Applications/SkillStar.app
> ```

### 2. Windows

`.exe` 安装程序会自动注册环境变量。重启终端后 `skillstar` 全局可用。

### 3. Linux

`.deb` / `.rpm` 直接安装到 `/usr/bin/skillstar`；`.AppImage` 便携版：

```bash
chmod +x SkillStar_x.x.x_amd64.AppImage
sudo mv SkillStar_x.x.x_amd64.AppImage /usr/local/bin/skillstar
```

### 手动下载

[GitHub Releases](https://github.com/xxww0098/SkillStar/releases/latest):

| 平台 | 安装包 |
|------|--------|
| macOS (Apple Silicon) | `SkillStar_x.x.x_aarch64.dmg` |
| macOS (Intel) | `SkillStar_x.x.x_x64.dmg` |
| Windows | `SkillStar_x.x.x_x64-setup.exe` |
| Linux | `.AppImage` / `.deb` / `.rpm` |

---

## 前置要求

至少安装一个 Agent CLI：

- [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
- [Codex CLI](https://github.com/openai/codex)
- [Gemini CLI](https://github.com/google-gemini/gemini-cli)
- [Cursor](https://cursor.com)
- [Qoder](https://qoder.com)
- [Trae](https://trae.ai)
- [OpenCode](https://github.com/opencode-ai/opencode)
- [OpenClaw](https://github.com/openclaw/openclaw)
- [Antigravity](https://antigravity.google)

---

## 从源码构建

```bash
git clone https://github.com/xxww0098/SkillStar.git
cd SkillStar
bun install
bun run tauri dev      # 开发模式
bun run tauri build    # 打包
```

需要 [Bun](https://bun.sh/) 和 [Rust](https://rustup.rs/)。

---

## 典型工作流

### Skills mode (▣)
1. `Marketplace` 浏览并安装技能
2. `My Skills` 管理已装技能、编辑 SKILL.md、配置 Agent 链接
3. `Security Scan` 扫描已安装技能的安全风险（支持 AI 深度分析）
4. `Decks` 组合技能，一键部署到项目
5. `Projects` 注册工程并按 Agent 同步

### Usage mode (◴)
1. 左侧 catalog 选一家供应商，或点底部「➕ 新增订阅」
2. 选 Auth 模式：API Key / OAuth / Manual
   - **API Key**：直接粘贴 key + 月费 + 续费日，保存后立即同步
   - **OAuth**：点击「用浏览器登录」→ 完成授权 → 卡片自动出现
   - **Manual**：录入「已用 / 总额」+ 窗口标签（本月 / 5h / 周）
3. 卡片右上角看 plan 徽章；进度条和余额随刷新更新
4. 接近上限或临近续费时自动弹告警

### Models mode (▤)
管理各家 Provider 配置、健康度、按工具维度切换底层模型（Provider / Health / Tool Configs / Settings）。

---

## CLI 快速用法

### 安装技能
```bash
skillstar install https://github.com/user/my-agent-skill                  # 默认：装到 Hub + 链接当前项目
skillstar install --global https://github.com/user/my-agent-skill         # 全局：仅 Hub
skillstar install --project /path/to/project https://github.com/u/skill   # 指定项目
skillstar install --agent codex,claude https://github.com/u/skill         # 指定 Agent
skillstar install --name cool-skill https://github.com/u/multi-skill-repo # 仓库多技能时指定
```

### 管理 / 扫描
```bash
skillstar list                                  # 列出已装技能
skillstar update [name]                         # 更新（无名则更新全部）
skillstar scan /path/to/skill                   # 安全扫描（含 AI 深度）
skillstar scan /path/to/skill --static-only     # 仅静态规则
```

### 创建 / 发布
```bash
skillstar create                                # 当前目录生成技能模板
skillstar publish                               # 发布到 GitHub（依赖 gh CLI）
```

### 工具包 / 健康
```bash
skillstar pack list                             # 列出 Deck/Pack
skillstar pack remove <name>                    # 卸载组合包
skillstar doctor [name]                         # 健康检查
```

### GUI
```bash
skillstar gui                                   # 强制唤起桌面图形界面
```

---

## 技术架构

| Layer | Technology | 作用 |
|-------|------------|------|
| Desktop Shell | Tauri v2 | 桌面容器 / IPC |
| Backend | Rust 2024 + tokio + reqwest 0.13 | 业务逻辑 + 异步任务 |
| Git Engine | gix 0.80 (gitoxide) | 克隆 / 拉取 / 哈希对比 |
| OAuth | tiny_http 1455/1456/1457 + PKCE + JWT exp | Cursor / Codex / Trae / Qoder / Antigravity 登录 |
| Frontend | React 19 + TypeScript + Vite 8 | SPA UI |
| UI | TailwindCSS v4 + Framer Motion 12 + Radix | 设计系统 / 交互 |
| Storage | JSON files + SQLite | 配置持久化 + 翻译/扫描缓存 |
| Crypto | AES-256-GCM + machine_uid → SHA-256 | API Key / OAuth token 加密存储 |

---

## 目录概览

```text
SkillStar/
├── src/                            # React 前端
│   ├── features/
│   │   ├── my-skills/              # Skills mode 域
│   │   ├── marketplace/
│   │   ├── projects/
│   │   ├── security/
│   │   ├── models/                 # Models mode 域
│   │   ├── usage/                  # Usage mode 域（v0.3 新增）
│   │   │   ├── components/         #   UsagePanel / Sidebar / Grid / Card / Dialog / Banner ...
│   │   │   ├── hooks/              #   useUsageData
│   │   │   ├── api.ts              #   invoke 包装层
│   │   │   └── types.ts
│   │   └── settings/
│   ├── pages/                      # 顶级页面入口
│   ├── components/layout/          # Sidebar + ModeSwitcher (▣ ◴ ▤)
│   ├── hooks/useNavigation.tsx     # AppMode = skills | usage | models
│   ├── lib/                        # 共享工具
│   └── types/                      # 共享 TS 类型
├── src-tauri/                      # Rust 后端（Tauri + CLI）
│   ├── src/commands/
│   │   ├── usage_commands.rs       # /usage 14 个命令（v0.3 新增）
│   │   ├── usage_dto.rs            # 前后端 DTO
│   │   ├── models_commands.rs
│   │   ├── marketplace.rs
│   │   └── ...
│   └── src/core/                   # domain modules
├── crates/
│   ├── skillstar-core/             # 基础设施 (paths / db_pool / error / migration)
│   ├── skillstar-app/              # Tauri 集成层
│   ├── skillstar-skills/
│   ├── skillstar-models/
│   ├── skillstar-marketplace/
│   ├── skillstar-projects/
│   ├── skillstar-ai/
│   └── skillstar-usage/            # 订阅 / 用量 / OAuth 子系统（v0.3 新增）
│       └── src/
│           ├── catalog.rs          #   18 家固定 catalog
│           ├── subscription.rs     #   数据模型
│           ├── storage.rs          #   JSON 持久化
│           ├── crypto.rs           #   AES-256-GCM
│           ├── alerts.rs           #   阈值告警
│           ├── oauth/              #   PKCE / local_server / poll_flow / token_refresh
│           └── fetchers/           #   api_key/ + oauth/ 9 个 fetcher
├── docs/
└── scripts/
```

---

## 支持的 Agent CLI

| Agent | Global Config | OAuth 用量同步 |
|------|---------------|----------------|
| Claude Code | `~/.claude/` | — |
| Codex CLI | `~/.codex/` | ✅ PKCE + 1455 |
| Gemini CLI | `~/.gemini/` | — |
| Antigravity | `~/.gemini/antigravity/` | ✅ Google OAuth |
| Cursor | `~/.cursor/` | ✅ PKCE 轮询 |
| Qoder | `~/.qoder/` | ✅ Device Flow |
| Trae | `~/.trae/` | ✅ OAuth + 4 区域 |
| OpenCode CLI | `~/.config/opencode/` | — |
| OpenClaw | `~/.openclaw/` | — |
| 自定义 Agent | 自由配置 | — |

---

## 开发与协作

- 后端结构或流程调整：先更新 `AGENTS.md`
- 前端结构或交互规范调整：先更新 `AGENTS-UI.md`
- 重要 bug 修复：在 `docs/Error.md` 追加条目
- 新增 AI 厂商订阅类型：扩展 `crates/skillstar-usage/src/catalog.rs` + 视情况加 fetcher

---

## 许可证

[Apache-2.0](./LICENSE)

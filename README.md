<div align="center">

# SkillStar

### _Your Second Brain for Agent CLIs_

**面向多 Agent 开发者的桌面工作台与 CLI：统一管理 Skill 的发现、分发、项目编排、团队共享频道，以及多账号切换与额度监控。**

[![GUI](https://img.shields.io/badge/GUI-nsh%200.6-6366F1)](./gui)
[![Rust](https://img.shields.io/badge/Rust-1.99%2B%20(Edition%202024)-orange?logo=rust)](./Cargo.toml)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Windows%20%7C%20Linux%20(CLI)-informational)](#安装与桌面使用)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-green.svg)](./LICENSE)

</div>

---

## SkillStar 是什么

SkillStar 将桌面 GUI（`nsh` 原生 AppView 宿主）与 CLI 收敛在同一个 `skillstar` 二进制中，围绕两大能力域提供高密度、可预测且本地优先的工作流：

- **Skills 工作台（5 个子页 + 设置）**：
  - **我的技能（My Skills）**：按本机与 GitHub 共享频道管理已安装和自研 Skill，支持安全更新、`keep-local` 冲突保留、本地技能一键发布到 GitHub、外部工具技能收纳修复与备份恢复。
  - **技能市场（Marketplace）**：基于 SQLite + FTS 的本地优先索引，离线即可检索，并支持按需联网回填与在线种子补充。
  - **技能卡组（Decks / Skill Cards）**：将常用 Skill 组合打包为 `.ags` / `.agd` 或 Share Code，方便跨设备与团队分享。
  - **项目管理（Projects）**：声明项目级 Skill 并执行 reconciliation，识别多 Agent 共享路径冲突，并在桌面页或终端批准本机 MCP（`skillstar mcp serve`）推荐的应用计划。
  - **外部 MCP（MCP）**：无损纳管外部 MCP 服务、浏览内置商店与项目片段，并按工具级精细禁用与跨 35 个 Agent 同步配置。
- **Accounts 工作台（Accounts）**：
  - 按 Agent 管理多登录账号（OAuth、本机导入、令牌导入），实时对账客户端磁盘生效状态，事务化切号并在写回失败时自动回滚。
  - 支持的账号客户端以 [账号工作台](./docs/features/accounts/README.md)为准。

> 内置同步 [vercel-labs/skills](https://github.com/vercel-labs/skills) 全部支持目标（外加自有扩展与品牌图标资源），并提供零代码扩展的自定义 Agent 规则。SkillStar 不主动扫描探测本机安装软件来擅自开启集成——始终在「设置」中按需手动启用目标 Agent。

---

## 核心能力

### 1. Skill 获取、分发与更新
- **多源导入**：支持 `owner/repo`、GitHub URL（含 `tree/...` 子目录与单 Skill 路径）、本地目录、`.ags` / `.agd` 归档、Share Code，以及直接粘贴 `npx skills add ...` 命令。
- **增量缓存与精准部署**：重复扫描默认复用本地 Git 缓存（加 `--refresh` 或在界面点刷新检查上游）；部署优先使用 symlink，平台受限时自动回退到 junction（Windows）或安全 copy。
- **可控更新策略**：「设置 → 技能更新」支持在**手动更新**（默认）与**定时自动检查应用**（15 分钟 / 30 分钟 / 1 小时 / 6 小时 / 每天）间切换；遇到本地已修改的目录，支持保留为 `.local` 副本或明确覆盖。
- **备份与自愈修复**：卸载或覆盖前保留恢复快照；`skillstar doctor` 支持无损诊断、`--fix` 断链修复与 `--adopt` 外部技能纳管。

### 2. GitHub 私有共享频道与本机 MCP
- **不可变频道发布**：通过 GitHub App 将私有仓库注册为共享频道，发布绑定精确 commit 与目录哈希的不可变版本；成员权限与邀请始终以 GitHub 为唯一真相。
- **受保护订阅消费**：订阅端跨重启记住已选 Skill，支持按频道启用受保护自动升级、单 Skill 历史版本回滚与固定，并可将频道导出为 Claude Code 插件市场目录（`export-marketplace`）。
- **本机项目技能 MCP**：内置 `skillstar mcp serve --stdio`，暴露 `recommend_project_skills`、`get_project_skills`、`apply_project_skills`，支持 MCP elicitation 或终端/桌面显式批准（`skillstar mcp approve <plan_id>`）。

### 3. 多账号切号与额度监控
- **本地加密凭证库**：账号秘密以 AES-256-GCM 加密保存在 `~/.skillstar/secrets/accounts/`。
- **实况对账与可回滚切号**：通过客户端真实磁盘凭证判定当前生效账号；切换失败时保持索引不变并自动还原旧凭据。
- **配额窗口与重置倒计时**：卡片聚焦展示剩余额度、重置倒计时与套餐信息，清晰区分鉴权失效与临时网络异常。

---

## 安装与桌面使用

从 [GitHub Releases](https://github.com/xxww0098/SkillStar/releases/latest) 下载对应平台的 `skillstar` 可执行文件并加入 `PATH`：

- **无参数运行 `skillstar`**（或 `skillstar gui`）：启动桌面 GUI（当前提供 **macOS AppKit** 与 **Windows Win32** 宿主；Linux 请使用 CLI）。
- **传入子命令运行 `skillstar <cmd>`**：在同一二进制内直接执行 CLI，也可作为本机 MCP Server 运行。
- **状态栏额度窗与后台运行**：macOS 菜单栏与 Windows 系统托盘支持常驻图标及快捷键（`⌥⇧Q` / `Alt+Shift+Q`）一键呼出独立额度速览窗；「设置 → 常规」的「后台运行」开关（默认关闭）开启后，关闭主窗口仅隐藏而不退出进程，后台检查与定时唤醒继续保持（macOS 点击 Dock 图标恢复，Windows 点击系统托盘图标或再次启动恢复；通过退出菜单或 `⌘Q` 彻底结束进程）。
- **版本检查**：应用每日最多检查一次 GitHub Releases 并在「设置」提示更新，升级由用户手动替换二进制完成。

---

## 快速开始

1. 启动桌面端后，进入 **「设置 → Agents」** 手动启用你要使用的内置 Agent（或添加自定义 Agent）。
2. 在 **「技能市场」** 检索或从仓库导入需要的 Skill，并在 **「我的技能」** 中按 Agent 勾选启用。
3. 在 **「MCP」** 页面或 `skillstar mcp` 命令中管理外部 MCP 服务、从内置商店一键安装，并无损同步到兼容的 Coding Agent。
4. 在 **「项目管理」** 添加代码仓库，对项目级 Skill 进行规划与同步，或为 Coding Agent 配置 `skillstar mcp serve --stdio`。
5. 在 **「账号工作台」** 导入或登录对应 Agent 的多份订阅账号，查看实时剩余额度并一键切换。

---

## CLI 常用命令

### 发现与安装

```bash
skillstar find "code review"
skillstar add owner/repo
skillstar add owner/repo@frontend-design
skillstar add https://github.com/owner/repo/tree/main/skills/web-design-guidelines
skillstar add owner/repo --skill frontend-design --agent codex,claude-code
skillstar add owner/repo --skill '*' --agent '*' --yes
skillstar add owner/repo --all          # 安装全部 Skill 到已启用/全部目标并跳过确认
skillstar add owner/repo --global       # 部署到 Agent 全局目录
skillstar add owner/repo --copy         # 强制使用复制而非链接
skillstar add "npx skills add owner/repo --skill demo"
skillstar add owner/repo --list
skillstar add owner/repo --json --yes
```

> `install` 与 `add` 等价；`find` 与 `search` 等价。未加 `-y/--yes` 时按需进入交互选择；tree 子路径 URL 会钉住对应副本直到卸载。

### 日常管理、诊断与备份

```bash
skillstar list [--local | --hub] [--json]
skillstar use <source> [--skill name] [--agent claude-code|codex|sarvam-code] [--full-depth]
skillstar update [name] [--check | --dry-run]
skillstar doctor [--json]
skillstar doctor --fix [--dry-run]     # 仅修复可验证所有权的链接与残余状态
skillstar doctor --adopt [--apply]     # 预览或纳管 Agent 目录中的外部技能
skillstar remove <name> [name...]      # 删除指定 Skill（uninstall / rm 为别名）
skillstar remove --all [--yes]
skillstar package-skills [--project PATH] [--json]
skillstar init [name]                  # 在当前目录初始化 Skill 模板（别名 create）
skillstar publish                      # 打包并发布 Skill
skillstar backups list [--json]
skillstar backups restore <id>
skillstar backups clear [--yes]
```

### GitHub 共享频道

```bash
# 本地只读、自动升级开关与导出（无需登录）
skillstar channel list [--json]
skillstar channel auto-update <repository_id> [--enable | --disable] [--json]
skillstar channel export-marketplace <repository_id> --out <DIR> [--json]

# 组织频道管理、发布、成员与收件箱
skillstar channel orgs [--json]
skillstar channel create <organization> <name> [--description ...] [--json]
skillstar channel resume-create <repository_id> [--json]
skillstar channel register-scan <organization_id> [repository_id] [--json]
skillstar channel register-confirm <organization_id> <repository_id> [-y] [--json]
skillstar channel publish-preview <repository_id> [--json]
skillstar channel publish <repository_id> --title "..." [--notes "..."] [--commit <sha>] [-y] [--json]
skillstar channel members <repository_id> [--json]
skillstar channel invite <repository_id> <username> [--role subscriber|publisher] [--json]
skillstar channel cancel-invite <repository_id> <invitation_id> [--json]
skillstar channel reinvite <repository_id> <invitation_id> [--json]
skillstar channel revoke <repository_id> <username> [--json]
skillstar channel inbox [--json]
skillstar channel accept-invite [<invitation_id> | --resume-repo <repository_id>] [--json]
skillstar channel decline-invite <invitation_id> [--json]

# 订阅审阅、升级与回滚生命周期
skillstar channel review <repository_id> [--json]
skillstar channel subscribe <repository_id> [-s a,b | --empty] [--json]
skillstar channel check [repository_id] [--json]
skillstar channel apply <repository_id> [--keep-local a,b] [--discard-local c] [--json]
skillstar channel rollback <repository_id> <skill> [--revision N] [--keep-local | --discard-local] [--json]
skillstar channel resume-follow <repository_id> <skill> [--json]
skillstar channel removed-uninstall <repository_id> <skill> [--revoked] [--json]
skillstar channel removed-to-local <repository_id> <skill> [--local-name <name>] [--revoked] [--json]
```

### 本机 MCP 与团队上下文

```bash
# 本机项目技能 MCP（stdio JSON-RPC 与计划审批）
skillstar mcp serve --stdio
skillstar mcp approve <plan_id>

# 外部 MCP 无损管理与内置商店（详见 docs/features/mcp/README.md）
skillstar mcp agents [--installed] [--json]
skillstar mcp list [--json]
skillstar mcp store [query] [--online] [--limit N] [--json]
skillstar mcp store-install <item> [--name <name>] [--input K=V] [--agent claude,codex] [--online] [--json]
skillstar mcp add <name> ... [--agent claude,codex] [--replace] [--json]
skillstar mcp import [agent] [--name server] [--all] [--replace] [--json]
skillstar mcp paste <raw|-> [--agent claude,codex] [--replace] [--json]
skillstar mcp remove <name> [--json]
skillstar mcp remove-found <agent> <name> [--json]
skillstar mcp pause <name> [--json]
skillstar mcp resume <name> [--json]
skillstar mcp rename <old_name> <new_name> [--replace] [--json]
skillstar mcp tools <name> [--disable a,b] [--enable c] [--enable-all] [--json]
skillstar mcp snippets [--project DIR] [--agent claude,codex] [--json]
skillstar mcp check <name> [--json]
skillstar mcp sync [--json]
skillstar mcp backups list [--limit N] [--json]
skillstar mcp backups restore <backup_id> [--target PATH] [--json]

# 本地 Team 检索、健康度与摩擦记录
skillstar team recall "pull request tests" [-n 12] [--json]
skillstar team health [--json]
skillstar team digest [--json]
skillstar team share --title "..." --body "..." [--skill name]
skillstar team notes [--json]
skillstar team friction --interrupts 2 --retries 4 --task "..."
skillstar team used <skill-name>
```

完整参数与选项请以 `skillstar --help` 及各子命令 `--help` 为准。

---

## 从源码构建

### 环境要求

- **Rust**：`1.99.0`（仓库根目录已通过 [rust-toolchain.toml](./rust-toolchain.toml) 固定工具链与 `clippy`/`rustfmt`）
- **nsh CLI**：`0.6.1`（需与 [gui/host/Cargo.toml](./gui/host/Cargo.toml) 和 `gui/native/Cargo.toml` 锁定的 `0.6.1` 保持一致）

### 启动开发环境

```bash
git clone https://github.com/xxww0098/SkillStar.git
cd SkillStar
cp .env.example .env   # 可选：按需填写 SKILLSTAR_GITHUB_APP_CLIENT_ID 与各 OAuth 变量
./dev.sh --no-pull     # 或直接执行：cargo run -p ss-bin --bin skillstar --locked
```

> 本地从源码构建如需使用 GitHub App 设备流登录共享频道，请在 `.env` 中配置 `SKILLSTAR_GITHUB_APP_CLIENT_ID`。

### 验证与门禁

```bash
# GUI（nsh TSX + 原生类型校验与测试）
nsh check gui
nsh test gui

# Rust workspace 编译与测试
cargo check --workspace --locked
cargo test --workspace --locked

# 结构与静态护栏
bash scripts/internal/check_workspace_deps.sh
bash scripts/internal/check_file_size.sh
bash scripts/internal/check_error_strings.sh
bash scripts/internal/check_no_orphan_modules.sh
bash scripts/internal/check_dep_graph_doc.sh
```

### Git hooks

建议在克隆后安装仓库自带的 Git hooks（直接写入 `.git/hooks/`，不引入额外依赖）：

```bash
bash scripts/internal/install_hooks.sh
```

- `pre-commit`：检查 workspace 依赖归一化、单文件行数上限、错误字符串规范、孤儿模块以及依赖图文档同步。
- `pre-push`：在 `pre-commit` 检查基础上追加 `cargo test --workspace --locked` 与 clippy 棘轮检查。

### 清理构建缓存

Rust 增量构建产物长期积累后会占用较大磁盘空间，建议使用 `cargo-sweep` 按天淘汰过期缓存（避免使用 `cargo clean` 清空增量缓存）：

```bash
cargo sweep --time 15
```

---

## 数据目录与架构文档

SkillStar 默认将数据存放在 `~/.skillstar`（支持通过 `SKILLSTAR_DATA_DIR`、`SKILLSTAR_HUB_DIR`、`SKILLSTAR_TOOL_SYNC_HOME` 覆盖）：

| 目录 | 用途 |
| --- | --- |
| `config/` | 运行偏好与显式设置（`profiles.toml`、`gui_prefs.json`、代理、`github_mirror.json`、翻译配置、技能自动更新偏好） |
| `data/` | 持久业务资产（`skills/installed/`、`library.json`、`.skill-lock.json`、`backups/`、`groups.json`、`projects/`、`channels/`、`mcp/`） |
| `secrets/` | AES-256-GCM 加密的账号密钥（`accounts/`）、GitHub 登录凭据（`github/auth.json`）与翻译大模型 API Key（`translation/api_key`，目录 `0700`、文件 `0600`） |
| `cache/` | 可重建派生缓存（`marketplace/marketplace.db`、`skill-imports/`、`github-trees/`、`skill-publish/`、`avatars/`、`translations/`） |
| `state/` | 本机运行投影（`skill_updates.json`、GitHub API 限流冷却、应用版本检查结果、Team 记录等） |
| `runtime/` / `logs/` | 跨进程互斥锁（`runtime/locks/`）与按大小/时间轮转的诊断日志 |

更多架构与设计文档：

- 开发约束与红线：[AGENTS.md](./AGENTS.md)
- 领域术语表：[CONTEXT.md](./CONTEXT.md)
- 目录所有权与 12-crate 依赖图：[docs/boundaries.md](./docs/boundaries.md)
- 运行拓扑与核心不变量：[docs/architecture.md](./docs/architecture.md)
- 存储分类规范：[docs/storage-layout.md](./docs/storage-layout.md)
- 关键架构决策（ADR）：[docs/decisions.md](./docs/decisions.md)
- 故障排查与教训录：[docs/errors.md](./docs/errors.md)
- 各领域功能契约：[docs/features/](./docs/features/)（含 [MCP 管理与内置商店](./docs/features/mcp/README.md)）

---

## 许可证

基于 [Apache-2.0 License](./LICENSE) 开源。

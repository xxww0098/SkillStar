# 和我讲中文

# SkillStar — Code Framework

> 本文件只维护三件事：**技术栈**、**项目树**、**文档索引**。它是项目**结构的单一事实来源**。
> 后端的行为约束（各子系统实现细节）放 [docs/backend.md](./docs/backend.md)，前端约定放 [AGENTS-UI.md](./AGENTS-UI.md)，
> 以免重复与漂移。

## 维护规则（先改文档，再写代码）

- **结构 / 技术栈变更**（新增删除依赖、升级版本、新增/移动/删除 crate 或顶层目录）→ **先更新本文件**的「技术栈 / 项目树 / Workspace Crates」再写代码。AGENTS.md 落后于代码即视为缺陷。
- **后端行为变更**（某子系统的实现约束）→ 先更新 [docs/backend.md](./docs/backend.md)。
- **前端结构 / 约定变更** → 先更新 [AGENTS-UI.md](./AGENTS-UI.md)。
- **新增 Agent CLI** → 按 [ADDING-AN-AGENT.md](./ADDING-AN-AGENT.md) 走。
- 重要 bug 调查与修复 → 在 [docs/Error.md](./docs/Error.md) 追加条目。
- 文档与代码同 PR 提交，不留「待补」。

## 文档治理（防漂移）

功能越来越多，乱的根因是**同一个事实在多处各写一份、各自漂移**。规范只有两条核心：

1. **每个事实只有一个 SSOT（单一事实来源）**，其余地方一律链接过去、不复制：

   | 事实类别 | SSOT | 反面教材 |
   | --- | --- | --- |
   | 技术栈 / 目录 / crate 划分 | 本文件 | 在 README/CLAUDE 重列一份目录树 |
   | 后端子系统行为约束 | [docs/backend.md](./docs/backend.md) | 把实现细节抄进 AGENTS.md |
   | 前端结构 / 约定 | [AGENTS-UI.md](./AGENTS-UI.md) | — |
   | **可枚举 / 计数型清单**（usage catalog、builtin agents、命令列表…） | **代码 + 单测**（如 `catalog.rs` 的 `catalog_has_16_entries`） | 在文档里硬写「16 家」却无人同步 |
   | 产品对外话术 / 安装 | [README.md](./README.md) · [PRODUCT.md](./PRODUCT.md) | — |

   推论：文档里**不写代码已测试锁定的数字 / 清单**——要么描述性带过，要么指向代码；确需写明数字时，改动必须与对应单测同 PR 同步。

2. **一个功能域 = 一个 crate + 一个 feature 切片 + backend.md 一节**。新增大功能时落点固定：
   域逻辑进新的 `crates/skillstar-<域>`（在本文件 Workspace Crates 表登记）→ 前端进 `src/features/<域>/` → 行为约束进 `docs/backend.md` 一个小节。
   不要把新功能塞进既有 crate 的杂项模块，也不要在命令包装层堆逻辑。

## 架构

SkillStar 是 Tauri v2 桌面应用（同一 `skillstar` 二进制内还含 CLI），React 19 SPA 前端 + Rust 后端。

- 前端只通过 `invoke()` 与 Tauri 事件调用后端，不直接触达文件系统 / 网络。
- Tauri 命令在 `src-tauri/src/commands/`（`mod.rs` 注册），保持**薄层**，无重逻辑。
- 域逻辑全部住在 `crates/` 下的 workspace crate；`src-tauri/src/core/` 只保留 Tauri 专用胶水（State 句柄、事件发射、窗口绑定包装）。
- 持久化在 `~/.skillstar/` 混合存储：config/project 元数据用 JSON，marketplace + 翻译缓存用 SQLite。
- 技能向项目分发以 symlink 为先、自动回退到目录拷贝（Windows 无开发者模式时），保持项目目录干净。
- 侧边栏顶部胶囊切换三种 mode：**Skills**（技能管理/分发）·**Usage**（订阅/配额面板）·**Models**（Provider 配置 + 工具同步）。

数据根可被 `SKILLSTAR_DATA_DIR` / `SKILLSTAR_HUB_DIR` 覆盖。关键路径：

| 用途 | 路径 |
| --- | --- |
| 数据根 / 配置 / 数据库 / 日志 / 状态 | `~/.skillstar/{,config/,db/,logs/,state/}` |
| Hub（远端技能 / 本地创作 / 仓库缓存） | `~/.skillstar/hub/{skills,local,repos,content}/` |
| AI / 代理 / GitHub 镜像配置 | `~/.skillstar/config/{ai,proxy,github_mirror}.json` |
| SSH 主机 / 已信任主机键 | `~/.skillstar/config/ssh_hosts.toml` · `ssh_known_hosts.json` |
| S3 同步目标 / 设备 id | `~/.skillstar/config/s3_targets.toml` · `state/sync_device.json` |

## 技术栈

> 版本以 `package.json` / `Cargo.toml`（各 crate）为单一事实来源；下表按主.次粒度标注。
> 新增/移除依赖、或任何改变表内所标版本的升级都要同步本表。

### 前端（React）

| 层 | 技术 | 版本 |
| --- | --- | --- |
| UI 运行时 | react / react-dom | 19.x |
| 构建 | vite | 5.x |
| 语言 | TypeScript | 5.x |
| 样式 | tailwindcss（仅 utilities） | 4.x |
| IPC | @tauri-apps/api | 2.x |
| 数据获取 | @tanstack/react-query | 5.x |
| 动效 | framer-motion | 12.x |
| 图标 / Toast / Markdown | lucide-react · sonner · react-markdown | 0.4x / 2.x / 10.x |
| 无障碍原语 | @radix-ui/* | latest |
| i18n | i18next（`src/i18n/locales/{en,zh-CN}.json`，需同步） | — |
| Lint+Format / 测试 | @biomejs/biome · vitest + @testing-library/react（dev） | 2.x / 3.x + 16.x |

### 后端（Rust）

| 层 | 技术 | 版本 |
| --- | --- | --- |
| 语言 / 包结构 | Rust 2024，cargo workspace（`src-tauri` + `crates/*`） | edition 2024 |
| 桌面框架 | tauri（+ updater / deep-link / dialog / shell / process 插件） | 2 |
| 异步运行时 | tokio | 1 (full) |
| HTTP | reqwest（统一经 `core::infra::http_client::probe_http_client`，遵循 `config/proxy.json`） | 0.13 |
| Git | gix（gitoxide）+ git 子进程 | 0.80 |
| CLI 解析 | clap | 4 |
| 序列化 | serde / serde_json / serde_yaml / toml / toml_edit | 1 / 0.9 / 1.1 |
| SQLite | rusqlite (bundled) + r2d2 连接池 | 0.39 |
| 加密 | aes-gcm (AES-256-GCM) + pbkdf2 + sha2 + machine-uid | 0.10 |
| 打包 | flate2 + tar（`.ags`/`.agd` bundle） | 1 / 0.4 |
| Markdown | html2md · pulldown-cmark | 0.2 / 0.13 |
| 日志 | tracing + tracing-subscriber | 0.1 / 0.3 |
| ACP | agent-client-protocol + async-trait + tokio-util + futures | 0.10 |
| 错误 / 时间 / 其它 | anyhow · thiserror · chrono · regex · uuid · which · sys-locale | — |
| Windows | junction（symlink 回退） | 1 |

### Workspace Crates（域逻辑 SSOT）

> 依赖关系：`skillstar-ai` → `skillstar-models` → `skillstar-providers`；`skillstar-usage` → `skillstar-providers` + `skillstar-fingerprint`。
> `skillstar-providers` 是零依赖叶子 crate，保持无依赖。

| Crate | 职责 |
| --- | --- |
| `skillstar-core` | 共享类型 + 基础设施（paths / fs_ops / db_pool / migration / error / util）+ 用户配置（proxy / github_mirror / ACP）+ `http_client::probe_http_client`（所有远程 HTTP 必须走它） |
| `skillstar-providers` | 零依赖叶子：Provider 元数据（余额端点、鉴权方案）的单一事实来源；usage fetcher 与 models preset 都从这里派生 |
| `skillstar-skills` | 技能生命周期（install / update / bundle / local 创作 / repo scan / discovery）+ git 操作 |
| `skillstar-marketplace` | 本地优先 marketplace 快照 + SQLite FTS + MCP registry/curated |
| `skillstar-models` | Provider store + presets + 外部工具同步（Claude Code / Codex / OpenCode / Gemini）+ latency + circuit breaker |
| `skillstar-ai` | 推理：chat completion、流式 summarize/translate、skill pick（依赖 skillstar-models 解析 Provider） |
| `skillstar-usage` | 订阅/配额：固定 catalog、OAuth + API-key fetchers、AES-256-GCM token 存储 |
| `skillstar-fingerprint` | TLS/HTTP 指纹感知 HTTP 客户端（JA3/JA4/H2 模拟，经可选 `impersonate`/`wreq` feature）+ IDE projector |
| `skillstar-projects` | 项目注册 + agent profiles + patrol + 终端（Launch Deck） |
| `skillstar-ssh` | SSH 远程技能管理：russh 连接 + SFTP 推送/列出/删除 + 主机配置 + keyring 凭证 + TOFU 主机键 |
| `skillstar-sync` | S3 云同步：aws-sdk-s3 + manifest.json + 本地技能 tar.gz 打包 + keyring 凭证 |
| `skillstar-app` | Tauri-agnostic 命令助手（shell / network / marketplace / ACP）+ 跨 crate 胶水（`usage_switch`：CLI 账号切换，桥接 usage+models）+ CLI 入口（`skillstar` 二进制） |

## 项目树（精简）

```text
SkillStar/
├── src/                       # React 19 SPA（前端约定见 AGENTS-UI.md）
│   ├── features/              # 域切片: my-skills · marketplace · projects · models · usage · mcp · fingerprints · ssh · s3 · settings
│   ├── pages/                 # 轻量路由壳（含 settings/）
│   ├── hooks/                 # 全局 hooks: useNavigation · useUpdater · useAiConfig · useAiStream · useAiTranslate ...
│   ├── components/            # ui/ · layout/（Sidebar + ModeSwitcher）· shared/
│   ├── lib/                   # 共享工具: ipc/ · tauriInvoke · shareCode · frontmatter · markdown ...
│   ├── i18n/                  # i18next（locales/en.json + zh-CN.json）
│   └── types/                 # 共享 TS 类型
├── src-tauri/
│   └── src/
│       ├── cli.rs             # CLI 入口（GUI 与 skillstar CLI 共用二进制）
│       ├── lib.rs / main.rs   # Tauri 启动 + 插件装配
│       ├── commands/          # Tauri 命令薄层（mod.rs 注册）
│       │   ├── ai/            #   AI 命令: summarize / skill pick
│       │   ├── models_commands/  # provider CRUD / 健康面板
│       │   └── *.rs           #   skills · bundles · projects · agents · github · patrol · mcp_commands
│       │                      #     · usage_{commands,dto,switch,windows} · ssh_hosts · s3_sync · fingerprints ...
│       └── core/              # Tauri 专用胶水（State / 事件 / 适配器）
│           ├── acp_client/    #   ACP 客户端
│           ├── marketplace_snapshot/  # 本地优先 marketplace DB（包 Tauri State）
│           ├── skills/        #   skillstar-skills 薄适配层
│           └── *.rs           #   marketplace · patrol · path_env · lockfile · update_checker ...
├── crates/                    # workspace 域逻辑（见上方 Workspace Crates 表）
├── docs/                      # backend.md（后端行为）· Error.md（故障记录）
├── scripts/                   # release/ + internal/（维护脚本）
├── public/                    # 静态资源（agent SVG 图标等）
└── AGENTS.md · AGENTS-UI.md · CLAUDE.md · PRODUCT.md · ADDING-AN-AGENT.md · README.md
```

## 文档索引

| 文档 | 内容 | 何时更新 |
| --- | --- | --- |
| `AGENTS.md`（本文件） | 结构 SSOT：技术栈、项目树、Workspace Crates、文档索引。 | 依赖/版本/crate/顶层目录变更时，**先于代码**更新。 |
| [docs/backend.md](./docs/backend.md) | 后端行为规则：各子系统实现约束（skills/sync、项目检测、model/usage、AI、fingerprint、marketplace、SSH、S3、patrol、storage、github mirror、ACP、auto-update）。 | 任何后端行为变更，**先改这里再写代码**。 |
| [AGENTS-UI.md](./AGENTS-UI.md) | 前端结构与约定、Models/Usage UI、流式 UX、视觉系统。 | 前端结构/约定变更时先改这里。 |
| [ADDING-AN-AGENT.md](./ADDING-AN-AGENT.md) | 新增 Agent CLI 的步骤指南（builtin 数据表 + 图标为核心路径，tool-sync/usage 为可选轴）。 | 新增/调整 Agent CLI 时。 |
| [PRODUCT.md](./PRODUCT.md) | 产品北极星、用户、领域边界、设计与可访问性原则（含品牌视觉方向 `Precise. Unified. Effortless.`）。 | 产品定位或设计原则变化时。 |
| [docs/Error.md](./docs/Error.md) | 错误速查与故障记录：根因不直观 / 可能复发的 bug。 | 定位到非显而易见的 bug 后，当次记录现象/根因/涉及文件/自检方法。 |
| [docs/ROADMAP.md](./docs/ROADMAP.md) | 结构治理路线图：结构债现状量化 + 收敛清单 + 防回归护栏。 | 发现新的结构债、或完成整改项打勾时。 |
| [README.md](./README.md) | 面向用户：安装、能力概览、CLI 用法、支持的 Agent。 | 对外能力或安装方式变化时。 |

## Do NOT

- **不**在未更新本文件「技术栈 / 项目树 / Workspace Crates」前就改依赖或目录结构。
- **不**手改 `Cargo.toml` 加依赖：一律用 `cargo add`。
- **不**在命令包装层（`src-tauri/src/commands/`）写重逻辑：域逻辑进 `crates/*`，Tauri 胶水进 `src-tauri/src/core/`。
- **不**绕过 `core::infra::http_client::probe_http_client` 直接发远程 HTTP（会无视 `config/proxy.json`）。
- **不**让单个源文件超过 ~1000 行；超出即拆模块。
- **不**在测试里写真实 `$HOME`：tool-sync 测试必须设 `SKILLSTAR_TOOL_SYNC_HOME` 指向临时目录（曾真实清掉过开发者的 `~/.codex/config.toml`）。
- **不**修改 `crates/skillstar-usage/src/fetchers/oauth/cursor.rs`，除非被明确要求。
- **不**把运行产物（`target/` / `dist/` / `node_modules/`）当作结构维护或提交。

## 质量与 CI

- Lint + Format：`bun run lint` / `bun run lint:fix` / `bun run format`（Biome）。
- 前端测试：`bun run test` / `bun run test:watch`（Vitest + jsdom）；测试文件 `*.test.ts(x)` / `*.spec.ts(x)` 与源码同放或在 `src/test/`。Tauri IPC 在 `src/test/setup.ts` 自动 mock。
- 后端测试：`cargo test`（workspace）/ `cargo test -p <crate>` / `cargo test -p <crate> <test_fn>`；`cargo check` 快速编译检查。
- CI：`.github/workflows/windows-ci.yml`（Windows 上 `npm ci` → lint → `npm run build` → `npm test` → `cargo test --workspace --locked`），用来兜住 macOS/Linux 本地开发漏掉的 Windows 路径 / shell / 换行回归。
- 供应链策略：`src-tauri/deny.toml`（cargo-deny advisories / licenses / sources）。

> 注：本地包管理器是 Bun（`bun.lock`）；Windows CI 用 npm（`package-lock.json`）。两套 lockfile 并存，改依赖后两者都要更新。

## 提交规范

Conventional Commits：`type(scope): description`

- `type`：`feat` / `fix` / `docs` / `style` / `refactor` / `perf` / `test` / `chore`
- `scope`：功能域，如 `skills` / `projects` / `agents` / `layout` / `usage` / `models`
- 提交信息用英文。

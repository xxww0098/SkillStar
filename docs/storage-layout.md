# SkillStar 存储目录布局

状态：active

本文件是 `~/.skillstar` 数据根的目录分类规则、默认位置与旧位置迁移映射的单一事实来源。可枚举的路径清单以 `crates/ss-core/src/infra/paths.rs` 及其测试为准，本文只描述规则与映射，不手抄每个文件的完整清单。数据所有权（哪段数据归哪个 crate）见 [architecture.md](./architecture.md)；分类动机与决策记录见 [decisions.md](./decisions.md) 的 D-087。

## 顶层分类（v3）

```text
~/.skillstar/               # data_root()
├── config/                 # 用户可编辑的声明式设置
├── data/                   # 持久业务数据与用户内容（不可自动清理）
│   ├── skills/installed/   # 已安装技能规范副本（不写入 ~/.agents/skills）
│   ├── skills/.skill-lock.json # 安装锁（vercel v3）
│   ├── skills/local/       # 本地创作 Skill（经 skills/installed/<name> 链接暴露）
│   ├── skills/install_baselines.json # 已安装 Skill 的安装内容 hash（判断本地修改）
│   └── instances/          # 桌面多开 profile 与清单 app_instances.json
├── secrets/                # 凭据与含 token 的存储（0700 目录；文件 0600）
│   ├── accounts/usage/     # Usage 订阅存储（含加密 token，整体按秘密保护）
│   ├── accounts/cli/       # CLI 凭据托管快照 <catalog>/<subscription_id>.json
│   ├── github/auth.json    # GitHub 用户登录凭据（AES-256-GCM sealed）
│   └── ssh/credentials.json # SSH 加密凭据（AES-256-GCM sealed）
├── cache/                  # 可重建的派生数据，可整目录删除
│   ├── marketplace/        # 市场快照 SQLite（marketplace.db 及 -wal/-shm）
│   └── sessions/           # Agent 会话解析 checkpoint 索引 index.json
├── state/                  # 需跨重启保留的运行状态
│   ├── patrol/status.json  # 后台巡检状态
│   ├── skills/github_api_cooldown.json # GitHub API 限流截止时间
│   └── ...                 # 项目 manifest、team、更新投影等（Phase 3 前留在原地）
├── runtime/                # 进程协作文件；不参与备份
│   └── locks/              # 跨进程锁（accounts/storage.lock、accounts/catalog-*.lock、skills/update.lock、skills/skill-lock.lock）
└── logs/                   # 日志，轮转
```

| 目录 | 判定规则 | 备份 | 清理 |
| --- | --- | --- | --- |
| `config/` | 用户希望软件如何运行；手写或显式编辑的设置 | 备份 | 不自动清理 |
| `data/` | 用户内容或不可重建的业务事实 | 必须备份 | 仅显式卸载 |
| `secrets/` | 凭据，或含 token/密文的存储（即使已加密） | 按用户选择 | 不自动清理 |
| `cache/` | 丢失后可从远端或源文件完整重建 | 不备份 | 专用清理器可整目录删 |
| `state/` | 跨重启的运行连续性（进度、投影、journal） | 建议备份 | 逐项判断，禁止整目录删 |
| `runtime/` | 进程间互斥与临时协作 | 不备份 | 活跃进程持有时不删；退出后可删 |
| `logs/` | 诊断日志 | 不备份 | 按时间/大小轮转 |

## 新增数据的落位决策

按顺序回答，命中即停：

1. 是否凭据或含 token/密文？是 → `secrets/<域>/`，不能因可重新获取而当作缓存。
2. 丢失后能否从远端或源文件完整重建？能 → `cache/<域>/`。
3. 是否进程间互斥或临时协作文件？是 → `runtime/locks/<域>/`。
4. 是否用户内容或不可重建的业务事实？是 → `data/<域>/`。
5. 是否用户显式编辑的运行偏好？是 → `config/`。
6. 其余跨重启运行状态 → `state/<域>/`。

约束：一条数据只有一个家；锁文件永不进 `config/` 或 `state/`；新持久化位置必须先在 `ss-core::infra::paths` 增加具名函数，调用方不得自行 join 默认路径。

技能描述和 SKILL.md 的译文是可重建缓存：`cache/translations/entries.json`（`translation_cache_path`）。引擎、描述和 SKILL.md 两个开关、主题、所选账户和模型 id 在 `config/translation.json`（`translation_config_path`）。两个开关默认关；关掉只是不再显示、不再新译，缓存文件还在。大模型只用账户里已有的 OpenCode Go、Ollama、Command Code 密钥，不另写一份翻译密钥，密钥也不进缓存键。没手动改过模型时，读出来会写成该服务当前的推荐模型，OpenCode Go 是 `deepseek-v4.1-flash`。手动选定后保持该 id，直到换账户。

导入 Git 缓存位于 `cache/skill-imports/`（`paths::skill_import_cache_dir`），进程锁位于 `runtime/locks/skill-imports/`（`paths::skill_import_locks_dir`）。清理 checkout 不删除锁文件。缓存可含私有仓库内容，Unix 目录权限为 0700。

技能事务锁是 `runtime/locks/skills/update.lock`，安装锁文件的读改写锁是 `runtime/locks/skills/skill-lock.lock`。规范副本旁边的 `.skillstar-*` 暂存不在数据根里，不进入上面的备份分类；清扫规则见 [Skills 生命周期](./features/skills/README.md#生命周期)。

## 环境变量覆盖（不变）

| 变量 | 语义 |
| --- | --- |
| `SKILLSTAR_DATA_DIR` | 重植整个数据根（开发/测试隔离），同时重植 canonical 技能根与锁 |
| `SKILLSTAR_HUB_DIR` | 测试沙箱专用：重植 legacy hub 与 canonical 技能根；设置时本地创作 Skill 落在 `<hub>/local`。生产不设置 |
| `SKILLSTAR_TOOL_SYNC_HOME` | 外部工具配置路径沙箱（不改变数据根） |

跨根迁移（如 `hub/local` → `data/skills/local`）在任一覆盖激活时整体跳过，避免沙箱触碰真实数据。

## 迁移语义

- 迁移只在进程启动时执行。GUI 与 CLI 走 `ss-app::bootstrap::prepare_process`。MCP serve 只做自己的路径迁移，不调用 `prepare_process`。前提是旧写入进程已退出。
- 市场 SQLite 是可重建缓存：不搬移旧主库、WAL 或 SHM，保留旧文件，在新位置通过市场刷新重建；首次离线启动可能没有市场列表。已有 v3 数据库保持不变。清理缓存必须停用相关读写，或通过持有域锁的专用清理器执行。
- 幂等：目标已存在则保留目标、不动源；源条目逐项迁移，失败记 warn 不阻塞启动。
- 涉及链接的数据（本地创作 Skill）迁移时同步重建 Agent 目录里的链接。按链接真实目标相对于旧根的路径重指，保留别名与子路径；旧技能条目仍存在（包括同名冲突、断链或探测失败）时保持原链接，不能切换到另一份内容。
- `legacy_cleanup`（D-081 清理）只删除指向旧 hub `skills/`、`repos/`、`content/` 的链接与目录；指向 `hub/local` 的链接由 v3 迁移重指，不被清理。
- 已安装技能从 `~/.agents/skills` 迁到 `data/skills/installed/`（[D-100](./decisions.md#d-100已安装技能的规范副本放在-skillstar-数据根)）。旧锁 `~/.agents/.skill-lock.json` 或 `$XDG_STATE_HOME/skills/.skill-lock.json` 在新锁不存在时迁到 `data/skills/.skill-lock.json`。新位置已有同名条目则保留新的，旧条目留在原地。仍指向旧目录的 Agent 链接改指新目录。`SKILLSTAR_DATA_DIR` 或 `SKILLSTAR_HUB_DIR` 激活时跳过，避免沙箱碰真实家目录。

## 旧 → 新映射（Phase 1，已实施）

| 旧位置（v2） | 新位置（v3） | 类别 |
| --- | --- | --- |
| `accounts/<catalog>/` | 已有目录原位使用（旧 CLI 链接依赖其路径）；无旧目录的 catalog 使用 `secrets/accounts/cli/<catalog>/` | secrets |
| `config/usage/*.json` | `secrets/accounts/usage/*.json` | secrets |
| `config/usage/.storage.lock` | `runtime/locks/accounts/storage.lock` | runtime |
| `config/usage/locks/catalog-*.lock` | `runtime/locks/accounts/catalog-*.lock` | runtime |
| `db/marketplace.db{,-wal,-shm}` 或 v1 根目录同名文件 | 旧文件保留；`cache/marketplace/` 独立重建，不逐文件迁移 | cache |
| `sessions/index.json` | `cache/sessions/index.json` | cache |
| `state/patrol.json` | `state/patrol/status.json` | state |
| `state/skill-update.lock` | `runtime/locks/skills/update.lock` | runtime |
| `state/github_auth.json` | `secrets/github/auth.json` | secrets |
| `state/ssh_credentials.json` | `secrets/ssh/credentials.json` | secrets |
| `instances/` | `data/instances/` | data |
| `config/app_instances.json` | `data/instances/app_instances.json` | data |
| `hub/local/` | `data/skills/local/`（随迁移重建 Agent 链接） | data |
| 根目录散件（v1 `patrol.json` 等，市场数据库除外） | 直接落入上表 v3 目标 | — |

## 未实施阶段

- **CLI custody 物理迁移**：必须与外部 CLI 链接、账号锁和回滚一起在 Usage 域完成，通用启动迁移不能直接搬走目录。已有旧 catalog 优先于同名新目录，避免切换凭据真相；Unix 启动时将旧 `accounts/` 根收紧为 0700。旧根同样属于敏感备份范围，不可按残留缓存清理。
- **Phase 2（config 细分）**：`config/*.json|toml` 归入 `config/{app,agents,sync}/`；`oauth_clients.json`、`antigravity_oauth.json`（含 client secret）迁 `secrets/oauth/`。
- **Phase 3（state 业务记录归 data）**：`state/{projects*,groups.json,team.json,repo_history.json,skill_update_states.json,project-skill-plans,project-skill-approvals,project-skill-receipts}` 迁 `data/` 对应域；`state/` 只留运行状态。
- 模型域遗留数据（`config/model_providers*.json`、`gateway/`、`models/`、`cache/{model_catalog,gateway-catalog}/`）不迁移、不清理（[D-082](./decisions.md)）。

## 所有权

- 路径解析：`ss-core::infra::paths`（唯一入口）。
- v3 迁移：`ss-core::infra::migration`（数据根内条目）+ `ss-skills::storage_migration`（本地创作 Skill 与其 Agent 链接）。
- 数据归属域（谁写谁读）见 [architecture.md](./architecture.md) 的数据所有权表。

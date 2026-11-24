# SkillStar 存储目录布局

状态：active

本文件是 `~/.skillstar` 数据根的目录分类规则、默认位置与旧位置迁移映射的单一事实来源。可枚举的路径清单以 `crates/ss-core/src/infra/paths.rs` 及其测试为准，本文只描述规则与映射，不手抄每个文件的完整清单。数据所有权（哪段数据归哪个 crate）见 [architecture.md](./architecture.md)；分类动机与决策记录见 [decisions.md](./decisions.md) 的 D-087 与 D-113。

## 顶层分类（v4）

```text
~/.skillstar/                        # data_root()
├── config/                          # 纯人工编辑或明确声明的运行偏好（可跨机备份，不放锁或业务资产）
│   ├── profiles.toml                # Agent 启用、自定义 Agent、default_deploy_mode 与 agent_deploy_modes
│   └── ...                          # proxy.json / gh_mirror.json / translation.json / skill_updates.json / gui_prefs.json 等
├── data/                            # 持久业务数据与统一管理树（必须备份，不可自动清理）
│   ├── skills/                      # Skill 统一管理根
│   │   ├── library.json             # 统一管理清单（origin/called/baselineHash/desiredAgents/appliedTargets/localId）
│   │   ├── .skill-lock.json         # 安装来源锁（schema v3，与 npx skills CLI 互通）
│   │   ├── installed/<name>/        # 唯一规范技能实体目录（远程与本地创作统一实体存放，取消 local/ 软链双跳）
│   │   ├── backups/<ts>-<reason>/   # 滚动备份树（卸载、手动覆盖修改、Copy 冲突归档；默认保留最近 25 批）
│   │   ├── groups.json              # 技能卡组（由 state/ 迁入）
│   │   ├── favorites.json           # 仓库收藏（由 data/skills/repo-favorites.json 迁入）
│   │   └── repo_history.json        # 最近导入仓库历史（由 state/ 迁入）
│   ├── projects/                    # 项目部署注册表（projects.json + <name>/skills-list.json，由 state/ 迁入）
│   ├── channels/                    # 共享频道本地注册与订阅表（shared_channels.json + subscriptions.json，由 config/ 迁入）
│   └── mcp/                         # 外部 MCP 服务定义、逐 Agent 应用台账（library.json）与写入前滚动备份（backups/）
├── secrets/                         # 凭据与含 token 的存储（0700 目录；文件 0600）
│   ├── accounts/                    # accounts.json + records/<id>.json（D-108）
│   ├── github/auth.json             # GitHub 用户登录凭据（AES-256-GCM sealed）
│   └── translation/api_key          # 翻译大模型手动 API Key（0600，独立于 Accounts）
├── cache/                           # 可重建的派生数据，一键清理安全
│   ├── marketplace/                 # marketplace.db{,-wal,-shm} + description_cache.json
│   ├── skill-imports/               # 按仓库+ref 隔离的 Git 稀疏导入缓存（0700）
│   ├── github-trees/                # GitHub Trees/Commits ETag 条件请求缓存
│   ├── skill-publish/               # GitHub 发布本地 Git 暂存缓存
│   ├── avatars/                     # 仓库与发布者头像缓存
│   └── translations/                # 描述与 SKILL.md 译文缓存（entries.json）
├── state/                           # 仅本机运行投影（删除仅丢失徽标与检查时间戳，不丢核心资产）
│   ├── skill_updates.json           # 更新可用状态与同源新技能发现（由 skill_update_states.json 迁入）
│   ├── skills/github_api_cooldown.json # GitHub API 限流截止时间
│   └── app/release_check.json       # 应用版本检查最后结果（D-103）
├── runtime/                         # 跨进程互斥与临时协作文件；不参与备份
│   └── locks/                       # 统一锁目录（skills/、projects/、channels/、accounts/、skill-imports/、mcp/）
└── logs/                            # 日志，按大小/时间轮转
```

| 目录 | 判定规则 | 备份 | 清理 |
| --- | --- | --- | --- |
| `config/` | 用户希望软件如何运行；手写或显式编辑的设置 | 备份 | 不自动清理 |
| `data/` | 用户内容或不可重建的业务事实（含管理树、项目清单、频道归属与备份快照） | 必须备份 | 仅显式卸载或清理备份 |
| `secrets/` | 凭据，或含 token/密文的存储（即使已加密） | 按用户选择 | 不自动清理 |
| `cache/` | 丢失后可从远端或源文件完整重建 | 不备份 | 专用清理器可整目录删 |
| `state/` | 跨重启的运行投影与时间戳（无核心资产） | 可选 | 逐项清理或按白名单重置 |
| `runtime/` | 进程间互斥与临时协作 | 不备份 | 活跃进程持有时不删；退出后可删 |
| `logs/` | 诊断日志 | 不备份 | 按时间/大小轮转 |

## 新增数据的落位决策

按顺序回答，命中即停：

1. 是否凭据或含 token/密文？是 → `secrets/<域>/`，不能因可重新获取而当作缓存。
2. 丢失后能否从远端或源文件完整重建？能 → `cache/<域>/`。
3. 是否进程间互斥或临时协作文件？是 → `runtime/locks/<域>/`。
4. 是否用户内容或不可重建的业务事实？是 → `data/<域>/`。
5. 是否用户显式编辑的运行偏好？是 → `config/`。
6. 其余跨重启运行投影 → `state/<域>/`。

约束：一条数据只有一个家；锁文件与业务记录永不进 `config/`；核心用户资产永不滞留 `state/`；新持久化位置必须先在 `ss-core::infra::paths` 增加具名函数，调用方不得自行 join 默认路径。

技能描述和 SKILL.md 的译文是可重建缓存：`cache/translations/entries.json`（`translation_cache_path`）。引擎、描述和 SKILL.md 两个开关、主题、大模型端点 `llm_base_url` 与模型 id 在 `config/translation.json`（`translation_config_path`）。两个开关默认开（新装即翻译描述与 SKILL.md）；关掉只是不再显示、不再新译，缓存文件还在。大模型翻译不从 Accounts 读取账户密钥，而在「设置 → 翻译」直接手填并保存到 `secrets/translation/api_key`（`translation_api_key_path`，Unix `0600`），旧配置遗留的 `llm_account_id` / `llm_account_catalog` 字段会在规范化时清空，密钥也不进缓存键。

界面偏好（语言、背景风格、后台运行等）在 `config/gui_prefs.json`，由 `Settings` facade（`gui/native/src/facades/settings.rs`）读写：`background_style` 决定 `current` 深色 / `paper` 浅色，`background_run` 控制关闭主窗口时是否隐藏到后台继续运行（默认 `false`），`accounts_auto_refresh_ms` 是账号页的自动刷新周期（缺键即关闭）。设置页与账号页都写这份文件，两边都是「先读整张表、只替换自己的键」。

导入 Git 缓存位于 `cache/skill-imports/`（`paths::skill_import_cache_dir`），进程锁位于 `runtime/locks/skill-imports/`（`paths::skill_import_locks_dir`）。清理 checkout 不删除锁文件。缓存可含私有仓库内容，Unix 目录权限为 0700。

技能事务锁是 `runtime/locks/skills/update.lock`，安装锁与统一管理清单写锁是 `runtime/locks/skills/skill-lock.lock`，共享频道锁统一收口至 `runtime/locks/channels/`。规范副本旁边的 `.skillstar-*` 暂存与 `.preserve-staging/` 不进入用户列表，退出清理规则见 [Skills 生命周期](./features/skills/README.md#生命周期)。

## 环境变量覆盖（不变）

| 变量 | 语义 |
| --- | --- |
| `SKILLSTAR_DATA_DIR` | 重植整个数据根（开发/测试隔离），同时重植 canonical 技能根、管理树与锁 |
| `SKILLSTAR_HUB_DIR` | 测试沙箱专用：重植 legacy hub 与 canonical 技能根（生产不设置） |
| `SKILLSTAR_TOOL_SYNC_HOME` | 外部工具配置路径沙箱（不改变数据根） |

跨根迁移（如 `~/.agents/skills` → `data/skills/installed` 或 `data/skills/local` → `data/skills/installed`）在任一覆盖激活时按需约束，确保沙箱测试绝不触碰宿主真实 `$HOME`。

## 迁移语义与 v3 → v4 映射（已实施）

- 迁移只在进程启动时由 `ss_app::bootstrap::prepare_process` 幂等执行（`ss-core::infra::migration::migrate_paths` 迁移同根条目，`ss-skills::storage_migration` 迁移本地技能消除 `local/` 双跳并重绑 Agent/Project 链接）。MCP serve 不调用 `prepare_process`。
- 幂等保护：目标文件或目录已存在时保留新目标、不覆盖；旧位置临时 `.lock` 文件直接安全移除。
- **v3 → v4 收敛映射**：

| 旧位置（v2/v3） | 新位置（v4） | 类别 |
| --- | --- | --- |
| `data/skills/local/<name>/` + `data/skills/installed/<name>` 软链 | `data/skills/installed/<name>/`（直接实体目录）+ `library.json` (`origin: local`)，同步重绑 Agent/项目软链 | data |
| `data/skills/install_baselines.json` | 合并入 `data/skills/library.json` 的 `baselineHash` | data |
| `data/skills/repo-favorites.json` | `data/skills/favorites.json` | data |
| `state/groups.json` | `data/skills/groups.json` | data |
| `state/repo_history.json` | `data/skills/repo_history.json` | data |
| `state/projects.json` & `state/projects/<name>/` | `data/projects/projects.json` & `data/projects/<name>/` | data |
| `config/shared_channels.json` & `config/shared_channel_subscriptions.json` | `data/channels/shared_channels.json` & `data/channels/subscriptions.json` | data |
| `config/shared_channels*.lock` | `runtime/locks/channels/`（启动清理 `config/` 下遗留空锁） | runtime |
| `hub/publish/` | `cache/skill-publish/` | cache |
| `data/marketplace_description_cache.json` | `cache/marketplace/description_cache.json` | cache |
| `state/skill_update_states.json` | `state/skill_updates.json` | state |

## 所有权

- 路径解析：`ss-core::infra::paths`（唯一入口）。
- v3/v4 数据根条目迁移：`ss-core::infra::migration`。
- 统一管理清单与局部迁移：`ss-skills::library` + `ss-skills::storage_migration`（消除 `local/` 双层跳板并重指 Agent/项目链接）。
- 滚动备份与恢复：`ss-skills::backup`。
- 数据归属域（谁写谁读）见 [architecture.md](./architecture.md) 的数据所有权表。

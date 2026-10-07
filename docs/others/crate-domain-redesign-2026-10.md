# SkillStar crate 域重设计（2026-10）

状态：historical（已实施）

> 本文件是 [D-086](../decisions.md#d-086按当前-skills--accounts-功能收敛-crate-域) 的执行记录，不是 SSOT。现行 crate 所有权与依赖方向以 [boundaries.md](../boundaries.md)「当前功能的域划分」为准；运行不变量见 [architecture.md](../architecture.md)；测试隔离教训见 [errors.md](../errors.md)。

## 1. 背景与目标

模型域移除（D-082）与 Usage 顶层模式移除（D-085）之后，crate 边界落后于实际功能：

- `skillstar-channels` 与技能安装、锁、部署共用事务，却独立成 crate，并要求 GUI/CLI/MCP 每个入口手动注册保护策略，未注册时回退 allow-all。
- 账号 facade、DTO 与消费汇总放在 `skillstar-app::usage`，但只消费 `skillstar-usage`，不是跨域编排。
- `skillstar-core::providers` 只剩账号域消费者，却让所有 crate 都能看见。
- 三个单域部署模块（Agent 技能暂停/恢复、安装后部署、Deck 链接回填）放在 app，绕经编排层。

目标：按数据所有权与事务内聚性重新划分，消除 `channels → skills`、`app → usage` 两条编译边；频道所有权保护默认生效；不按页面数量建 crate。

## 2. 设计结果

| 域 | 变化 | 理由 |
| --- | --- | --- |
| `skillstar-skills` | 吸收 `skillstar-channels` 为 `channels` 命名空间；新增 `workflows` 承载单域部署用例 | 频道订阅与技能安装/锁/升级共享同一事务与数据；部署用例只依赖技能域 |
| `skillstar-usage` | 新增 `accounts` 公开 facade（service/dto/consumption 私有）；`providers` 成为私有模块 | 账号用例与消费汇总已无跨产品域消费者；Provider 元数据只服务账号目录与余额 fetcher |
| `skillstar-app` | 只保留 CLI、项目技能 MCP、技能×市场编排与全局存储维护 | `skill_group_deploy` 需要 marketplace 来源解析，是真实跨域编排，留在 app |
| `skillstar-core` | 移除 `providers` | 共享层不再承载单域元数据 |
| `skill-spec` / `skillstar-git` / `skillstar-marketplace` / `skillstar-sync` | 保留独立 | 各自拥有独立变更节奏、依赖集合或传输边界；合并收益无法通过 deletion test |

## 3. 迁移映射（旧 → 新）

| 旧位置 | 新位置 |
| --- | --- |
| `crates/skillstar-channels/**` | `crates/skillstar-skills/src/channels/`（patrol / policy / shared_channels） |
| `crates/skillstar-app/src/usage/**` | `crates/skillstar-usage/src/accounts/` |
| `crates/skillstar-core/src/providers/**` | `crates/skillstar-usage/src/providers/`（私有；identity 仅 `#[cfg(test)]` 的一致性夹具） |
| `crates/skillstar-app/src/agent_managed_skills.rs` | `crates/skillstar-skills/src/workflows/agent_managed_skills.rs` |
| `crates/skillstar-app/src/global_deploy.rs` | `crates/skillstar-skills/src/workflows/global_deploy.rs` |
| `crates/skillstar-app/src/skill_group_links.rs` | `crates/skillstar-skills/src/workflows/skill_group_links.rs` |
| `crates/skillstar-app/src/skill_group_deploy.rs` | 不迁移（marketplace 来源解析留在 app） |

## 4. 行为不变量变化

- **mutation gate**：`skill_mutation::policy()` 固定返回域内 `channels::policy::ChannelAwarePolicy`；删除 allow-all 回退、`set_skill_mutation_policy` 与测试替换口。GUI、CLI 与直接库调用获得同一频道所有权保护，注册遗漏不再可能。
- **测试环境锁**：合并后的测试共用目标 crate 的唯一环境锁（skills 用 crate 级 std mutex；accounts 用 usage 自带加锁的 `EnvGuard`，不再外层重复拿锁）。夹具必须自行保存/恢复 `SKILLSTAR_HUB_DIR`——外层统一设置会盖过夹具的 data root 隔离。
- **契约不变**：磁盘布局、IPC 形状、ts-rs 生成类型零字段变化（仅源码路径注释变化）。

## 5. 防回退门禁

`scripts/internal/check_workspace_deps.sh` 升级为完整 workspace 边白名单：normal/dev/build/target 依赖一律检查，rename 别名不可绕过，未分类 crate 直接拒绝。`scripts/internal/test_workspace_deps.py` 提供负向用例（域反向边、别名、未分类 crate、app 二进制）。

## 6. 验证记录（实施完成时）

- `cargo check --workspace --locked` 与 `cargo test --workspace --locked` 全绿（默认并行、临时 HOME）。
- `bun run lint` / `bun run build` / `bun run test`（515 项）通过。
- `bun run types:gen` + `check_generated_types.sh` 一致。
- 四项结构门禁（workspace deps / feature imports / file size / command boundaries）通过。

下面是对当前后端结构的审查结论，以及一套「更少文件、更少迁移痕迹」的目标设计与落地顺序。

现状：你在「域拆分」和「兼容层」之间
core/mod.rs 已经写明两层：新的域目录（infra、git、config、projects、terminal、ai）和「迁移中保留」的顶层模块。


mod.rs
Lines 1-45
// ═══════════════════════════════════════════════════════════════════
//  New domain-grouped modules
// ═══════════════════════════════════════════════════════════════════
pub mod infra;
pub mod ai;
pub mod config;
pub mod git;
pub mod projects;
pub mod terminal;
// ═══════════════════════════════════════════════════════════════════
//  Existing modules (kept during incremental migration)
// ═══════════════════════════════════════════════════════════════════
pub mod acp_client;
pub mod agent_profile;
pub mod ai_provider;
pub mod db_pool;
pub mod dismissed_skills;
pub mod error;
pub mod gh_manager;
pub mod git_ops;
pub mod github_mirror;
pub mod launch_deck;
pub mod lockfile;
pub mod marketplace;
// ...
大量「只有几行」的顶层文件不是业务太碎，而是刻意 shim：把旧路径 crate::core::paths 等转发到新位置，例如：


repo_history.rs
Lines 1-3
//! Backward-compatibility shim — delegates to `git::repo_history`.
pub use super::git::repo_history::*;
同类 2 行级 shim 还有：dismissed_skills、github_mirror、proxy、source_resolver、sync、translation_cache、launch_deck、agent_profile 等。paths.rs、error.rs、util.rs、db_pool.rs 稍长，但也是「兼容 + 聚合 re-export」。

因此：可以删、可以并，但本质是「愿不愿意做一次全仓 import 迁移」；删 shim 不会减少运行时逻辑，只会减少文件数和认知分叉。

目标结构（更简洁、单一事实来源）
原则：每个概念只在一个域里实现；core 顶层只保留「粗粒度域 + 少数横切大块」。

建议的稳定树形（与 AGENTS.md 方向一致，只是把迁移收口）：

域	职责	从现状迁入 / 删除的 shim
core::infra
paths、fs、db、migration、error、util
删除顶层 paths / error / util / db_pool shim，调用方改为 infra::*
core::git
clone/fetch、repo 历史、URL 解析、dismissed
删除 repo_history、source_resolver、dismissed_skills shim；把 git_ops.rs、gh_manager.rs 挪成 git/ops.rs、git/publish.rs（或保留文件名但放进 git/）
core::config
proxy、github_mirror
删除顶层 proxy、github_mirror shim
core::projects
agents、sync
删除 agent_profile、sync shim（若 agent_profile 只是 agents 的别名）
core::terminal
Launch Deck 配置
删除 launch_deck shim
core::ai
translation_cache 等
删除 translation_cache shim；长期可把 ai_provider 收进 ai::provider 或保持并列（见下）
core::skills / marketplace / security_scan / model_config
保持现有子目录深度
不强行并为单文件
不建议为「少几个文件」把 security_scan、repo_scanner、ai_provider 里已经分开的类型/策略硬塞进一个巨型 .rs；当前行数统计里真正「碎」的主要是 shim，而不是子模块实现。

「小文件能不能删 / 合并」——分三类
纯 shim（2～5 行）

可以删：全项目把 crate::core::repo_history 改成 crate::core::git::repo_history（以此类推），然后从 core/mod.rs 去掉 pub mod repo_history。
不推荐再合并成另一个物理文件里的「大 shim 桶」——那只是把分散的转发集中一处，mod.rs 会膨胀，长期仍不如直接改 import。
带一点应用逻辑的薄文件（例如 lockfile.rs 里的 lockfile_path()）

保留或挪到 infra / skills 下单一小模块即可，不必和几百行的文件合并。
commands 下多个 skill_*.rs

若团队更在意「命令入口文件数」，可以收成 commands/skills/{paths,content,groups}.rs + commands/skills/mod.rs，属于纯组织变更，与 core 域收口可分开做。
推荐落地顺序（改动可控）
机械替换 import（可分批：paths → infra::paths，error → infra::error 等），每批 cargo check。
从 core/mod.rs 删除对应 pub mod xxx 与 shim 文件。
把仍躺在 core/ 根下的 git_ops、gh_manager 移入 core/git/（并更新 git/mod.rs 的 pub use），消除「git 域 + 根上又一坨 git」的双轨。
文档：AGENTS.md 里「domain-first」一节与真实目录对齐（你规则里要求架构变更先改文档；本次若只出设计、未改代码可略）。
小结与风险
项	说明
changed
无代码修改；以上为结构设计。
verified
基于对 core/mod.rs、shim 文件内容与 wc -l 的仓库内审查。
unverified
未跑 cargo check；若执行删除 shim，需全量编译与按需测 IPC。
结论：整体可以更简洁，主要抓手是结束「顶层 shim + 根目录 git 相关」的双轨，而不是把已有子目录里的小实现文件硬合并。那些几乎没代码的文件，绝大多数可以删，前提是统一改 import；不建议用「合并成一个大 shim 文件」代替这一步。

如果你希望下一步在仓库里真正做一版（例如先做 paths / error 一批），可以说一下优先域（infra / git / commands），我可以按批次改并跑 cargo check 作为验证证据。
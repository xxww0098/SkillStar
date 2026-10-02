# 03 请求只物化被选中的一份

## 解锁什么

点 cursor 图标或 `--agent cursor` 时，只多物化 `.cursor/skills/impeccable` 这一份。按名字请求、不指定 harness 时，不物化任何新目录，因为 02 之后每个身份都已有一份代表副本在磁盘上。

## 根因

`inventory.rs:117-129` 的过滤条件是 `name_hit || prefix_hit`。两个调用方都会传名字：
- `skill_install.rs:134`：缓存扫描分支；
- `skill_install.rs:349-358`：`choose_install_skills`。

结果是，带名字的 harness 请求会把所有 basename 相同的延迟副本都物化出来。

## 接缝

在 `inventory.rs` 里，用下面两个函数取代 `materialize_deferred_matching`（`:103-174`）：

```rust
pub(crate) fn materialize_for(repo_dir, session, wants: &[(&str /*identity*/, CopyRequest)]) -> bool;
pub(crate) fn materialize_dirs(repo_dir, session, dirs: &[String]) -> bool; // 04 的钉住也用它
```

`materialize_for` 对每个请求执行 `inventory.copy_for(identity, req)`，得到**一个**目录。如果它还没在磁盘上，就交给 `materialize_dirs`。

`materialize_dirs` 沿用原来 `:134-173` 的函数体，git 和 tarball 两条分支都保留。

**安装侧**：把 `choose_install_skills`、`existing_same_repo_action`、`push_unique`、`nameless_root_skill`、`requested_skill_not_found_error`（`skill_install.rs:277-427`）移到新的兄弟模块 `install_choice.rs`，让 `skill_install.rs` 回到 800 行以下。

`choose_install_skills` 的新流程：
1. 从 lock 查出这个名字的 `source_folder`，作为 `installed`（`:366-372` 已经在查）。
2. 调用 `materialize_for`。
3. 再调用 `resolve_install_skills`。

inventory 和 discovery 用同一个 `choose_copy`，输入也相同，所以两边选出的副本必然一致。

**删除** `skill_install.rs:127-145` 的物化调用，连同它的注释「requested identity may exist only as a deferred duplicate copy」。「缺失就 fetch 一次」的回退保留，它来自 errors.md 2026-09-18。

## 测试

命令：`cargo test -p skillstar-skills --locked skill_install_harness_tests`

**新增：**
- `harness_install_materializes_only_the_chosen_copy`：表驱动，每个 case 用一个新的 Sandbox，覆盖 agent 为 cursor、deepseek、gemini-cli、codex、antigravity、windsurf（最后一个没有对应的副本）。断言：
  - cache 里 impeccable 的 `SKILL.md` 集合正好是 {代表副本, 被选副本}；
  - lock 的 `source_folder` 等于被选副本。
- `name_request_without_harness_materializes_nothing_new`
- `harness_fallback_materializes_the_installed_source_folder`：lock 指向的 `source_folder` 处于延迟状态时，它会被物化。
- `inventory_choice_matches_install_choice`：接缝一致性断言。

**必须保持绿：** README「验证门」列出的回归测试。

**离线 CLI 探针**（见 00）：执行 `--agent cursor` 之后，`find … -name SKILL.md` 里 impeccable 恰好 2 份，外加陷阱副本。

## 文档

- 决策条目补上「请求只物化一份」。D-045 里「稀疏检出保留全部嵌套 `SKILL.md` 父目录」标注已被取代。
- `docs/errors.md` 新增一条：harness 请求按名字物化全部同名副本。根因是 `name_hit || prefix_hit`，而调用方总是传名字。自检用 `harness_install_materializes_only_the_chosen_copy`。
- `docs/features/skills/README.md:41`：删掉「回写 plan」的旧说法。

## 可改

- `install_choice.rs` 的模块名和可见性。
- `materialize_for` 是逐个请求调用，还是批量调用。

## 什么反馈会改变本档

- 发现某条 harness 回退链路依赖「全部副本都在磁盘上」：先在 01 的表里补上对应的层级，不要在这里加特例。

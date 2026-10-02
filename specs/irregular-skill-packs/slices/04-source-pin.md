# 04 Source 贯通与子路径硬钉

## 解锁什么

`skillstar add https://github.com/pbakaus/impeccable/tree/main/.claude/skills/impeccable` 只装这一份副本，并在 lock 里写 `pinned: true`。之后：
- 点任何 harness，部署的都是这份内容，不会换副本。
- 更新时跟着这个路径走。
- 子路径指向 `tests/` 内部时照样能装。

这一档还把来源规格（ref、subpath、skill filter）收进 `Source` 一个 owner，从入口一路传到底。

## 现状

- `source_resolver.rs:238-249` 能从 tree URL 解析出 ref 和 subpath。
- 但 `choose_install_skills` 在整个仓库上调用 `resolve_install_skills`（`skill_install.rs:550-554`），`scan_parsed_checkout` 已经算好的子路径扫描结果被丢成 `_scan`。
- 一次安装里 `Source::parse` 被调用了约 4 次（`skill_install.rs:78, 110, 515, 708`）。`skill_migration.rs:63-81` 先 parse，然后又拆成 `(short, repo_url)` 往下传。
- 本地路径形式的 `Source`（`file://`）的 subpath 永远是 `None`（`source_resolver.rs:132-138`）。所以离线测试必须用 `insteadOf` 把 GitHub URL 映射到本地夹具，现成写法见 `Sandbox::map_github_url`（`skill_install_harness_tests.rs:54-65`）。

## 接缝

**`source_resolver::Source`**
- 加 `Serialize` 和 `Deserialize`。字段改名：`#[serde(rename = "source_url")] repo_url`、`#[serde(rename = "source")] short`。
- 这样 `ScanResult` 可以平铺它，前端现有字段不变（05 使用）。

**`lockfile::LockEntry`**
- 新增 `#[serde(default, skip_serializing_if = "std::ops::Not::not")] pub pinned: bool`。lockfile 版本不变，只是加一个字段。
- 编译器会指出所有结构体字面量，大约 7 处在生产代码：`scan_install.rs:157`、`skill_update/mod.rs:223`、`cache.rs:554`、`detect.rs:277`、`git_skill.rs:327`、`gh_manager.rs:615`、`skill_update/plan.rs:156`。全部补上 `pinned: false`，`scan_install.rs` 那处改为取 target 的值。

**`SkillInstallTarget`**
- 新增 `#[serde(default)] pub pinned: bool`。IPC 契约向后兼容。

**`discovery.rs`**
```rust
impl<'a> SkillDiscovery<'a> { pub fn within(self, scope: &'a str) -> Self } // folder_path 仍相对仓库根
pub(crate) struct InstallQuery<'a> { pub scope: Option<&'a str>, pub name: Option<&'a str>, pub copy: CopyRequest<'a> }
pub(crate) fn resolve_install_skills(repo_dir: &Path, q: &InstallQuery) -> Result<Vec<DiscoveredSkill>, String>;
```
- `scan_skills_in_repo_at`（`scan.rs:22-52`）改为委托给 `within`，删掉手工拼前缀的代码。
- 忽略目录的判断**相对作用域根**进行。这样子路径指向 `tests/` 内部时自然能发现，不需要写特判。

**安装流程（`skill_install.rs` 与 `install_choice.rs`）**
- 只在公共入口解析一次 `Source`：`install_skill_in_session`、`install_skills_batch_in_session`、`fetch_repo_scanned*`。
- 下面这些函数改成接收 `&Source`：`install_from_source`、`prepare_install_from_source`、`scan_repo_preferring_local_cache_for_skill`，以及 `acquire_repo_lock_for_url`（改名为 `acquire_repo_lock(&Source)`）。
- 有 `spec.subpath` 时：
  1. 先 `inventory::materialize_dirs(&[subpath])`；
  2. 查询 `InstallQuery { scope: subpath, copy: CopyRequest { pinned: Some(subpath) } }`；
  3. 目标写 `pinned = true`。
- lock 里已经 `pinned` 的条目，再遇到 harness 请求时：
  - `existing_same_repo_action` 返回 `Reuse`，不返回 `Retarget`；
  - `CopyRequest.pinned = source_folder`；
  - 在 `tracing::warn!` 里记一行「已钉住，忽略 harness」。
- 用普通 URL 重装已钉住的技能：**不解钉**。只有卸载才清掉钉住状态。
- 更新流程不需要改，它本来就按 `source_folder` 走（`update_checker.rs:342-395`）。这里只补测试证明。

## 测试

命令：`cargo test -p skillstar-skills --locked pinned_ tree_url_subpath lockfile:: discovery::`

- `tree_url_subpath_installs_exactly_that_copy`：URL 用 `https://github.com/acme/impeccable/tree/main/.claude/skills/impeccable`，通过 `insteadOf` 映射到 00 的夹具。
- `pinned_skill_is_not_retargeted_by_harness_click`：先钉住 `.claude`，再点 cursor。lock 不变，cursor 拿到的是 `.claude` 那份内容。
- `plain_reinstall_keeps_pin`
- `pinned_subpath_inside_tests_dir_installs`
- `pinned_skill_update_follows_its_folder`
- `pinned_flag_roundtrips_and_is_omitted_when_false`
- `discovery::explicit_scope_inside_tests_is_discovered`
- `source_resolver::source_serde_uses_scan_field_names`

**离线 CLI 探针：**
```bash
$E cargo run -q -p skillstar -- add https://github.com/pbakaus/impeccable/tree/main/.claude/skills/impeccable -g -y
$E cargo run -q -p skillstar -- add https://github.com/pbakaus/impeccable --agent cursor -g -y
jq '.skills[] | {name, source_folder, pinned}' $T/hub/lock.json   # source_folder 仍是 .claude/...，pinned: true
```

注意：在 06 完成之前，带 ref 的 URL 走完整浅克隆（`cache.rs:196-203`），所以本档**只保证结果正确，不保证下载量**。

## 文档

- 决策条目补上两点：
  - 「子路径是硬钉；用普通 URL 重装不解钉；`reconstruct_lock_entry` 会丢钉住标记」。
  - D-045 的后果「`source_folder` 跟随最近一次明确请求的 harness」补上钉住例外。
- `docs/features/skills/README.md:46`（钉住例外）和 `:52`（tree URL 等于硬钉）。
- `README.md:129`：tree URL 示例旁加一句「子路径会钉住该副本，之后不随 Agent 换副本」。

## H3 审查点（不阻塞）

把 lock JSON 片段和上面两条 CLI 命令的输出发给用户。用户看的是：钉住以后点其他 harness，拿到的是被钉住那份的内容。这符合决定 4，但可能出乎用户意料。

约 5 分钟没有回应，就按决定 4 执行，并在 `choices.md` 里记一笔。

## 可改

- `InstallQuery` 的具体字段名。
- 解析一次之后，是传 `&Source` 还是 `Arc<Source>`。

## 什么反馈会改变本档

- 用户希望钉住只影响默认值：把 `CopyRequest.pinned` 换成 `installed`，并删掉 `Reuse` 分支。

# 02 inventory 按名字确认身份

## 解锁什么

首装 impeccable 形态的仓库时，一个身份只物化一份，默认是 `.agents/skills/impeccable`。其余副本先延迟，被点名时再物化。测试夹具永远不物化。`.claude-plugin/` 总会物化。

## 接缝：`repo_scanner/inventory.rs` v2

```rust
const INVENTORY_FORMAT: u32 = 2; // 必填字段、不设 serde default → 旧 sidecar 解析失败 → 重新规划
#[derive(Serialize, Deserialize)]
pub(crate) struct Inventory {
    format: u32,
    pub revision: String,
    pub identities: BTreeMap<String, IdentityCopies>, // key = pack_layout::identity_key
    pub manifest_dirs: Vec<String>,
}
pub(crate) struct IdentityCopies { pub representative: String, pub copies: Vec<String> }

impl Inventory {
    /// representatives ∪ extra ∪ on_disk ∪ [".claude-plugin"]；合并父目录时不吞掉延迟的兄弟目录
    pub fn sparse_dirs(&self, extra: &[String], on_disk: &[String]) -> Vec<String>;
    pub fn copy_for(&self, identity: &str, req: CopyRequest) -> Option<&str>; // 内部就是 choose_copy
}
pub(crate) fn load_or_plan(repo_dir, session) -> Result<Inventory>; // 去掉 installed 参数
pub(crate) fn apply(repo_dir, session, extra: &[String]) -> Result<()>;
/// 纯规划器；读名函数由外部注入，不用 git 就能测退化路径
fn plan_from_entries(entries: &[GitTreeEntry], manifest: (Option<&str>, Option<&str>),
    read_name: impl Fn(&str /*SKILL.md blob sha*/) -> Option<Option<String>>) -> Inventory;
```

## 规划流程

1. `list_tree_entries_at` 取出所有 `SKILL.md` 条目，每条自带 blob sha。
2. 用 `is_under_ignored_dir` 过滤。测试夹具不花任何读取。
3. 按 basename 分组。组内只有 1 个成员，或者 tree SHA 全部相同时，不读 name。
4. 其余组收集各成员的 `SKILL.md` blob sha 并去重。然后按 00 的结论二选一：
   - 调一次 `skillstar_git::prefetch_blobs_in_session`（新文件 `crates/skillstar-git/src/blobs.rs`，照 `tree.rs` 的方式 `mod` 加 `pub use`；`ops.rs` 已经 830 行，不往里加）；
   - 或者逐个懒取，总时限 15 秒。
5. 用 `repo_scanner::detect::skill_at_revision`（`detect.rs:151-190`）解析 name。它和发现逻辑用的是同一套身份规则。
6. 按 `identity_key` 细分。每个身份的代表由 `choose_copy(默认模式)` 选出，其余成员进 `copies` 并延迟。
7. **读失败时退化**：这一组任何一次读取失败，整组退回 D-063 的旧规则，只延迟 tree SHA 相同的副本。加 `ponytail:` 注释，写明上限：「读不出 name 时省不下」。
8. 预取失败时 `load_or_plan` **不能返回 Err**，只做退化处理。否则 `cache.rs:442` 的 `?` 会把整次克隆打回 tarball 路径。
9. tree 里有 `.claude-plugin/` 时，把它固定放进 sparse。今天 cone 模式只带根目录的文件，导致发现逻辑在稀疏 cache 里读不到 manifest（`plugin_manifest.rs:95-102`）。

## 粘滞规则（防止链接悬空）

`apply` 在重新应用 sparse 之前，先列出 cache 里**已经在磁盘上**的技能目录（所有含 `SKILL.md` 的目录），作为 `on_disk` 传给 `sparse_dirs`。同一个 cache 的 sparse 集合只增不减。

原因是：被钉住的 Agent 链接直接指向 cache 里的副本（`deployment/mod.rs:99-134`），而 `installed_source_folders`（`repo_scanner/ops.rs:68-86`）只读 lock。收缩 sparse 会让这些链接悬空。errors.md 2026-08-14 记录过同一类故障。

`load_or_plan` 无论是命中 sidecar 还是重新规划，调用方拿到的集合都要合进 installed 集和 on_disk。今天 `inventory.rs:84-88` 命中 sidecar 时会忽略 installed，这一档一起修掉。

## 合并父目录的保护

`compact_to_common_parents`（`inventory.rs:368-418`）：如果某个父目录下还有延迟的或被忽略的技能目录，就不把这个父目录合并进去。否则延迟会悄悄失效。

## 删除与收拢

- 删除 `SparsePlan`、`deferred_matches`（它是 `#[allow(dead_code)]`）、`is_duplicate_of`、`dir_shas`。同时删掉注释 `inventory.rs:273-276, 322-325`：「内容不同的副本一律物化」这条理由已经被 name 身份取代。
- 原来有三份「plan 为空就 disable 加 checkout，否则 apply」的复制：`cache.rs:174-192`、`cache.rs:442-451`、`repo_scanner/ops.rs:44-60`。三处都改为调用 `inventory::apply(repo, session, &installed)`。
- tarball 分支（`cache.rs:212-234`）改用 `inventory.sparse_dirs(&[], &[])`。tarball 路径不能懒取 blob，读 name 必然失败，按第 7 步退化。代价只是多占一点本地磁盘，不多花网络，因为 archive 本来就已经整包下载。

## 测试

命令：`cargo test -p skillstar-skills --locked repo_scanner::inventory:: pack_collapses`

**纯规划器（不用 git）：**
- `unreadable_manifest_degrades_to_tree_sha_rule`
- `compaction_never_swallows_deferred_sibling`
- `same_basename_different_name_materializes_both`：陷阱副本 `.windsurf` 必须物化。

**git 夹具：**
- `impeccable_fixture_materializes_one_copy_per_identity`：sparse 里 impeccable 只有 `.agents/skills/impeccable`，外加陷阱副本和 `.claude-plugin`；`tests/**` 和 `crates/` 都不物化。
- `divergent_same_name_copy_defers`：由 `divergent_harness_copy_stays_materialized`（`inventory/tests.rs:114`）反转而来。
- `test_fixture_dirs_are_never_planned`
- `old_format_sidecar_is_rebuilt`
- `plugin_manifest_dir_is_always_materialized`
- `installed_folders_union_even_on_sidecar_hit`
- `on_disk_copies_stay_materialized_after_replan`：先物化 `.cursor`，把 lock 指向 `.agents`，然后 fetch 并重新应用，`.cursor` 必须还在。
- `identity_resolution_costs_one_fetch`：只有 P0a 通过时才写。在 partial 夹具上断言 promisor pack 只多 1 个。

**改写：**
- `pack_collapses_duplicate_copies_and_heavy_content`（`skill_install_harness_tests.rs:735-791`）：`.cursor` 那条断言反转为「未物化」。

## 文档

- 决策条目补上：name 身份、每个身份一份、读失败退化、粘滞规则、sidecar 格式 2（丢弃旧文件）。D-063 标注「内容不同一律物化」被取代。
- `docs/features/skills/README.md:41` 重写 inventory 一段。

## H2 审查点（真网络，不阻塞）

```bash
export SKILLSTAR_DATA_DIR=$(mktemp -d) SKILLSTAR_HUB_DIR=$SKILLSTAR_DATA_DIR/hub HOME=$SKILLSTAR_DATA_DIR/home
GIT_TRACE=1 skillstar install pbakaus/impeccable --list 2>trace.log
grep -c 'fetch' trace.log; find $SKILLSTAR_HUB_DIR/repos -name SKILL.md | wc -l   # 期望 1
cat $SKILLSTAR_HUB_DIR/repos/*impeccable*/.git/skillstar-inventory.json
```

要求：首装耗时不比 main 慢。把 fetch 次数、耗时、份数写进 `choices.md`。如果变慢了，停在这里：先回到 00 看 P0a，改预取方式，不要进 03。

## 可改

- `Inventory` 的内部字段布局，前提是 sidecar 中每个身份的代表副本和全部副本都能直接读出来。
- 「列出磁盘上已有技能目录」的具体实现，可以用 `git ls-files`，也可以 walk 目录。

## 什么反馈会改变本档

- H2 变慢：调整预取方式。
- 用户要求读失败时也延迟：改第 7 步。

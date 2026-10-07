# 01 选副本规则只有一个 owner

## 解锁什么

「选哪份副本」「什么算同一身份」「哪些目录忽略」「manifest 指向哪里」这四类规则，都收进纯函数，不碰 IO。02–04 只调用它们，不再各自排序。

这一档合并了四件小事：唯一排名表、忽略测试夹具目录、`.gemini/skills`、plugin.json 字符串的容器语义。它们的审查面是同一张表，拆开交付没有价值。

## 接缝：`pack_layout.rs`

```rust
pub(crate) const IGNORED_DIR_NAMES: &[&str] = &[".git", "node_modules", ".venv", "venv",
    "__pycache__", "target", "dist", "build", ".next", ".nuxt",
    "tests", "test", "__tests__", "fixtures"];
/// 只看祖先段：`skills/test` 仍是技能；`tests/oracle/**/impeccable` 不是。
pub(crate) fn is_under_ignored_dir(folder: &str) -> bool;
/// 小写；frontmatter name 缺失时用 basename（与 detect.rs:188-194 同规则）
pub(crate) fn identity_key(frontmatter_name: Option<&str>, folder: &str) -> String;

#[derive(Clone, Copy, Default)]
pub(crate) struct CopyRequest<'a> {
    pub harness: Option<&'a str>,   // ".cursor"
    pub installed: Option<&'a str>, // 该身份在 lock 里的 source_folder
    pub pinned: Option<&'a str>,    // 子路径硬钉（04 起使用）
}
pub(crate) fn choose_copy<'a, T>(copies: &'a [T], folder: impl Fn(&T) -> &str,
    req: CopyRequest<'_>, manifest_dirs: &[String]) -> Option<&'a T>;
```

**唯一的层级表**：数字越小越优先，同层按目录路径字典序，结果确定。

| 模式 | 层级 |
|---|---|
| 默认（无 harness） | 0 根 `""` → 1 `skills/…` 或 `source/skills/…` → 2 `.agents/skills/…` → 3 父目录属于 `manifest_dirs` → 4 其他 |
| harness `h` | 排除根 → 0 `h/skills/…` → 1 `h` 本身 → 2 canonical（`skills/`、`source/skills/`）→ 3 等于 `installed` → 4 `.agents/skills/…` → 5 manifest → 6 其他 |
| pinned `p` | 只能选 `== p`；不存在就返回 `None` |

- harness 模式和 D-046 现有的回退链一致，唯一的变化是 `.agent` 不再与 `.agents` 同级（README「默认决定」）。
- 默认模式不看 lock，所以 inventory 的代表副本只由 revision 决定。已安装的目录由 02 另外并进物化集。
- 根目录排第 0 只是为了让表完整。D-044 剥离根垫片（`discovery.rs:154-173`）发生在更早的步骤，所以根目录和同身份的嵌套副本不会同时进候选。

**删除**（全部改为调用 `choose_copy`）：
- `pack_layout.rs`：`source_priority`、`discovered_folder_priority`、对外公开的 `harness_folder_rank`（`pack_layout.rs:42-60, 139-148`）。
- `discovery.rs`：对上面这些函数的 re-export（`discovery.rs:24-27`），以及 `better_nested_copy`、`select_canonical_skill`、`select_nested_skill_copy`、`select_preferred_nested_skill`（`discovery.rs:402-453`）。
- `inventory.rs`：`choose_representative`（`inventory.rs:296-320`），并去掉它的 installed 参数。

**改接线：**
- `dedupe_discovered_skills`、`collapse_pack_identity_copies` 改用默认模式的 `choose_copy`。后者的碰撞语义（`independents > 1`）不变。
- `resolve_install_skills` 的 harness 分支改成调用一次 `choose_copy`。
- discovery 用 `plugin_manifest::declared_skill_dirs` 取 `manifest_dirs`。02 之后，稀疏 cache 里也读得到 manifest。

## 接缝：`discovery.rs`

- `PRIORITY_SKILL_DIRS`（`:204-250`）加上 `".gemini/skills"`。`detect::is_container_skill_dir` 会自动跟随。
- `find_all_skill_md_files`（`:641-684`）改用 `IGNORED_DIR_NAMES`。遇到被忽略的目录，只看它自己有没有 `SKILL.md`，不再往下递归。这样得到「只看祖先段」的语义。

## 接缝：`plugin_manifest.rs`

- `deserialize_path_list`（`:73-87`）改成保留原形：`enum PathList { Container(String), Paths(Vec<String>) }`。
  - plugin.json 的 `skills` 是**字符串**时，表示容器路径，把该路径本身推进去，不取父目录。
  - **数组**里的每一项仍按技能路径处理，取父目录。
  - marketplace 条目的处理不变。

## 测试

命令：`cargo test -p skillstar-skills --locked pack_layout:: discovery:: plugin_manifest:: repo_scanner::inventory::`

**新增：**
- `pack_layout::tests::copy_selection_table`：以 00 夹具的注册表为输入，逐行断言：
  - 默认 → `.agents/…`
  - `.cursor` → `.cursor/…`
  - `.agent` → `.agent/…`（不是 `.agents`）
  - `.windsurf` 加上 installed `.dsh/…` → `.dsh/…`
  - `.windsurf` 不带 installed → `.agents/…`
  - pinned `plugin/skills/impeccable` → 它本身
  - pinned 一个不存在的目录 → `None`
- `harness_request_never_selects_repo_root`、`ties_break_lexicographically`。
- `gemini_skills_is_a_priority_container`、`full_depth_skips_test_fixture_trees`（用夹具）、`skill_named_test_inside_container_is_kept`。
- `plugin_json_string_is_a_container_path`：`"./.claude/skills/"` 的结果包含 `.claude/skills`，不包含 `.claude`。
- `plugin_json_array_entries_are_skill_paths`。

**改写：**
- `catalog_outranks_harness_copies`（`pack_layout.rs:192`）。
- `discovery/tests.rs` 里的 `source_priority_ordering`（原来 `.agent == .agents` 的断言要翻过来）、`dedupe_keeps_higher_priority`。
- `plugin_json_skills_string_is_accepted`：原来是碰巧通过的，改名为上面的新测试。同步更新 `string_version_matches_disk_version_shape`。
- `manifest_declared_container_wins`（`inventory/tests.rs:187`）：改成 `canonical_and_agents_beat_manifest_container`。

**必须保持绿：**
- `discovery/tests.rs` 里 `resolve_install_skills_*` 那几个测试。
- `pack_layout.rs` 里 `discovery_integration` 那几个测试。
- `select_harness_skill_keeps_agent_and_agents_distinct`。
- `cargo test -p skillstar-channels --locked`。

## 文档

- `docs/decisions.md` 新增决策条目（下一个空号），先写三部分：层级表、忽略目录、plugin.json 字符串等于容器。
  - D-063 状态改为「部分被新条目取代」，范围是排名那句。
  - D-044 里「`skills` 同时接受字符串路径和数组」改为引用新条目。
- `docs/features/skills/README.md`：
  - :47、:51 的排名描述，改成链接到新条目，不再抄表。
  - 补充 `.gemini/skills`、忽略测试目录、字符串等于容器。
- `docs/errors.md` 新增一条：plugin.json 的 `skills` 字符串被当成技能路径，测试碰巧通过。自检用 `plugin_json_string_is_a_container_path`。

## H1 审查点（不阻塞）

把 `copy_selection_table` 的表和上面那张层级表发给用户。用户看的是两处可见的行为变化：
- manifest 声明的副本从第 1 位降到第 4 位。
- `.agent` 不再与 `.agents` 同级。

约 5 分钟没有回应，就按表执行，并在 `choices.md` 里记下「H1 无回应，按表执行」。

## 可改

- 层级在内部用枚举还是 `u8` 表示。
- 测试辅助函数的写法。

## 什么反馈会改变本档

- 用户要求 `.agent` 与 `.agents` 同级：在表里加一行，并恢复原来的断言。
- 用户要求 manifest 优先：把默认模式里的第 1、3 层对调。

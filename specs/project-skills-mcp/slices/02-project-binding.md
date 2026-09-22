# 02 — 项目绑定

## 契约

调用方给的路径字符串变成 `ProjectBinding`。`observe` 不写 `projects.json`。规范注册只在项目写锁内插入新行，不改已有行的 `path`。

## 缝

模块 `skillstar_skills::projects::binding`。

```text
observe_project(path) -> Result<ObservedProject>
register_canonical_project(path) -> Result<ProjectEntry>
```

`ObservedProject` 含规范根、可选的已有 `name`、以及 `ambiguous` 当且仅当两条仍存在的条目 canonicalize 到同一根。`ambiguous` 时两个函数都返回错误，不写索引。

匹配：对 `projects.json` 里 `path` 仍是目录的条目做 canonicalize，与调用方路径的 canonicalize 比较。目录已经不存在的旧条目不参与匹配。不调用 `ensure_project_root_exists` 代替 canonicalize。不改 `register_project`。

`register_canonical_project` 找到一条时复用 `name`，不回写 `path`。找不到时插入新 `ProjectEntry`，`path` 用规范路径。本函数不拿锁。唯一的生产调用点是 12 的 apply，并且必须已经在 `with_project_write_lock` 里。本档测试可以直接调用。09 和 10 不得调用它。

祖先检查函数 `contained_child(root, target)`：从目标向上找到最深的已存在祖先，canonicalize 后必须仍在规范根内。供 09 使用。本档用夹具测它，不部署。

## 人可以运行

`cargo test -p skillstar-skills project_binding_`

夹具要覆盖 macOS 的 `/tmp` 与 `/private/tmp`（或该平台等价的符号链接根）。Windows 夹具覆盖 junction 指向外部、以及 `\\?\` 前缀能比较的情况。比较的是 canonicalize 后的路径，不是用户字符串。

## 验证

- `observe_does_not_create_projects_json`
- `observe_matches_noncanonical_registered_path_without_rewriting_it`
- `observe_rejects_two_live_entries_for_one_canonical_root`
- `register_inserts_canonical_path_once`
- `contained_child_rejects_symlink_that_escapes_root`

临时目录设置 `SKILLSTAR_DATA_DIR`、`HOME`，Windows 再设置 `USERPROFILE`。

## 可改

错误类型用 `anyhow` 还是现有 `AppError`。字段是结构体还是枚举，只要上述状态都在。

## 不可改

不改旧 `path` 字符串。不在 observe 时注册。不把技能名或相对目录当作绑定身份。

## 必须保持绿

现有 `register_project` 测试。`cargo test -p skillstar-skills --locked`。

## 会改这一档的反馈

canonicalize 在必须支持的路径上不稳定，或逃逸只有事后删除才能测到。出现时停，收窄接受的路径类，不要进 09。

# 05 GUI 接上 Source 与插件提示

## 解锁什么

两件事：
- 在 GUI 导入框粘贴 tree URL 后，扫描和安装都保留 ref 与子路径，并且会钉住。
- 仓库是 Claude 插件时，CLI 和 GUI 都显示一行提示：「hooks/agents 不会安装，完整插件请用 `/plugin marketplace add`」。

## 现状

- GUI 扫描走 `scan_repo_with_mode_in_session`（`repo_scanner/mod.rs:51-68`），会丢掉 ref、subpath 和 skill filter。
- GUI 安装走 `install_from_scan`，只传 `repoUrl` 和 `source`（`ImportModal.tsx:462-467`）；后端 `install_from_repo_in_session`（`scan_install.rs:19-29`）调用不带 ref 的 `clone_or_fetch_repo_in_session`。
- `ScanResult`（`repo_scanner/mod.rs:28-32`）只有 `source`、`source_url`、`skills` 三个字段。

## 接缝

**后端（`skillstar-skills`）**
- `ScanResult { #[serde(flatten)] spec: Source, skills, plugin: Option<PluginHint> }`。平铺之后，JSON 对前端向后兼容，只是多了 `git_ref`、`subpath`、`skill_filter`、`plugin` 几个字段。
- `scan_repo_with_mode_in_session` 改为委托 `clone_or_fetch_repo_at_in_session(git_ref)` 加 `scan_parsed_checkout`。
- `install_from_repo_in_session(spec: &Source, targets, session)`：
  - 取代原来的 `(source, repo_url)` 参数；
  - 物化子路径；
  - `targets.pinned = spec.subpath.is_some()`。
- `GitSkillFacade::install_from_scan(&Source, targets)` 和 `graduate_local_skill_from_scan` 同步修改；`skill_migration.rs:63-81` 直接传 `&source`。
- `plugin_manifest::plugin_hint(paths: impl Iterator<Item = &str>) -> Option<PluginHint { hooks: bool, agents: bool }>`：
  - 输入是 `git_ops::list_tree_paths(repo_dir)`。它对稀疏 cache、完整克隆、tarball 合成仓库都可用，tarball 合成树的结果会退化，可以接受。
  - 判定规则：存在 `.claude-plugin/plugin.json` 或 `.claude-plugin/marketplace.json`，并且 manifest 指向的插件目录下有 `hooks/` 或 `agents/`。

**命令层（只做适配）**
- `src-tauri/src/commands/github/repo.rs:161` 的 `install_from_scan(spec: Source, skills, …)`。

**前端**
- `src/types/marketplace.ts:87`：加 `git_ref?`、`subpath?`、`skill_filter?`、`plugin?`。这个类型是手写的，不走 `types:gen`。
- `src/lib/ipc/commands/github.ts:33` 的参数改为 `spec`。`ImportModal.tsx:462` 和 `useSkills.ts:364, 390` 传 `spec: scanResult`，整个扫描结果本身就是 spec。
- `src/lib/ipc/devMock/github.ts:65-81` 补上新字段。
- `ImportModal.tsx` 在列表上方加一行提示，文案用 `t("githubImportModal.claudePluginHint")`；中英文放进 `src/i18n/locales/{en,zh-CN}.json`。不新增组件。

**CLI**
- `--list` 和 install（`crates/skillstar-app/src/cli/install.rs:535` 附近）在 `plugin.is_some()` 时打印同样的一行提示。

## 测试

- `cargo test -p skillstar-skills --locked`：
  - `scan_repo_honors_tree_url_ref_and_subpath`
  - `install_from_scan_keeps_ref_and_pin`
  - `plugin_hint_detects_hooks_and_agents`
  - `plugin_hint_is_none_without_manifest`
- `bun run lint && bun run build && bun run test`：如果已有 ImportModal 或 useSkills 的前端测试，同步更新其中调用参数的断言。
- `bash scripts/internal/check_command_boundaries.sh`、`bash scripts/internal/check_feature_imports.sh`。
- 离线 CLI 探针：`$E cargo run -q -p skillstar -- add https://github.com/pbakaus/impeccable.git --list`，输出里能看到插件提示那一行。

## 截图（本档有可见 UI）

用 `bun run tauri dev`，或者 devMock 页面，打开导入框，粘贴 `https://github.com/pbakaus/impeccable/tree/main/.claude/skills/impeccable`，分别截两张图：扫描后的列表（含插件提示），以及安装完成后的状态。

对这两张图跑 screenshot-critique，这是本档接受前的最后一项检查。它提供的是不带预设的第二意见，只判断提示行是否清楚、有没有挤压列表。ImportModal 的其他视觉问题不在本档范围内。

如果有改动前的导入框截图，再用 compare-screenshots 对比新旧两张，确认除了新增的那一行之外没有布局回退。

## 文档

- `docs/features/skills/README.md` 前端接缝一节（`:183` 起）：扫描结果就是来源规格；插件提示。
- 决策条目补上「插件只提示，不安装 hooks/agents」，并附上理由（见 README 的非目标）。

## H4 审查点（不阻塞）

用 preview-shots 把截图打开给用户，等约 5 分钟。没有回应就按 screenshot-critique 的结论决定，把决定写进 `choices.md`，关闭打开的截图，然后继续。

## 可改

- 提示文案的具体措辞。
- 提示行在弹窗里的位置，只要在技能列表附近即可。
- `PluginHint` 是否细分 hooks 和 agents：GUI 可以只显示一句总括。

## 什么反馈会改变本档

- 用户想要一个「用 Claude 插件安装」按钮：属于范围 C，另开一个规格，本档不做。

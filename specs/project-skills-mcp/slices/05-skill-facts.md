# 05 — 项目技能事实

## 契约

`get` 和 apply 之后的复核读同一份事实。事实来自清单和技能目录的直接子项，不来自项目源码。

## 缝

`skillstar_skills::projects::facts::inspect_project_skills(binding) -> ProjectSkillFacts`。

只对 `has_project_skills()` 的 profile，`read_dir` 其 `project_skills_rel` 的直接子项。同一相对路径合成一行，带 owner、`deploy_modes`、受影响 Agent，以及每个技能名的：清单里有没有、磁盘是链接 / junction / 真实目录 / 缺失、链接目标、Hub 里是否存在。

不调用 `scan_project_skills` 或 `detect_project_agents`。它们按 profile 重复行，并按精确路径字符串找清单。

本档不计算计划哈希。链接目标用现有 `read_link_resolved`。不读 `SKILL.md` 正文。

## 人可以运行

`cargo test -p skillstar-skills project_skill_facts_`

## 验证

- `facts_collapse_shared_agents_skills_to_one_physical_row`
- `facts_distinguish_manifest_only_disk_only_and_foreign_symlink`
- `facts_do_not_list_files_outside_project_skill_dirs`
- `facts_do_not_register_a_project`

未注册的 observe 结果得到空清单，且不创建 `projects.json`。

## 可改

行结构里可选字段的 serde 名字。本档的类型可以先不派生给前端。

## 不可改

不遍历项目根。不把 junction 当成真实目录。不在这里部署或注册。

## 必须保持绿

现有 scan / detect 测试。

## 会改这一档的反馈

共享目录的事实被拆成每个 Agent 一行，导致确认文案会把同一次写入说成多次。

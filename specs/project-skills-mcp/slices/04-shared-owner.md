# 04 — 共享路径 owner

## 契约

给定 profile 列表、现有清单、一条 `project_skills_rel` 和本次选中的 Agent，得到唯一的 manifest owner，以及其余会看到该目录的 Agent。

## 缝

`skillstar_skills::projects::owner::shared_path_owner`。

纯函数，无 IO。规则与 `add_skills_to_project_with_mode` 里现有的 owner 选择相同：该相对路径上已有清单键的 profile 优先；否则用本次选中的 Agent。`add_skills_to_project_with_mode` 改为调用它。其余分支不动，包括空 Agent 回退、缺失技能过滤、拆建链接、copy 模式。

披露名单是所有 `project_skills_rel` 相同的 profile，再加文档写明也会读取该目录的 profile。DeepSeek 读取 `.agents/skills`，但部署目标仍是选中 Agent 的相对路径。

## 人可以运行

`cargo test -p skillstar-skills shared_path_owner_`

## 验证

- `shared_path_owner_preserves_existing_owner`
- `shared_path_owner_uses_requested_agent_when_unowned`
- `shared_path_disclosure_includes_deepseek_for_agents_skills`
- 现有 `add_skills_to_shared_universal_path_uses_one_owner_and_honors_copy_mode` 仍绿

## 可改

返回结构的字段名。

## 不可改

不在这个函数里回退空 Agent。不写死三份 Agent id。不改宽松函数的其余语义。

## 必须保持绿

`cargo test -p skillstar-skills --locked`。

## 会改这一档的反馈

现有共享目录测试开始要求不同的 owner。

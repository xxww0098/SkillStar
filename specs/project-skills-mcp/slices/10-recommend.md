# 10 — 推荐编排

## 契约

推荐返回候选，并且只在显式 selection 合法时返回未批准的计划。不注册项目，不启用技能，不写批准。

## 缝

`skillstar_app::project_skills_mcp::recommend::recommend_project_skills`。

输入：绝对路径、查询字符串（最多 2000 字符）、可选 `constraints`（最多 20 条，每条 200 字符）、可选 `focus_paths`（最多 50 条，每条 200 字符）、可选 selection（一个 agent id，最多 8 个技能名）、`catalog_scope`。

`catalog_scope` 不是 `installed` 时返回错误。超限返回错误，不截断后继续。

顺序：

1. `observe_project`。ambiguous 则错误。
2. `search_installed_skills`。查询词是用户查询，加上 constraints 与 focus_paths 的文本。不读这些路径。
3. `SkillReranker::rerank`。本档只提供 `PassthroughReranker`，原序返回。trait 返回 `Vec`，不返回 `Result`，不接收计划或批准。
4. 没有 selection：不调用 `content::snapshot`，不写计划，`plan` 为 `None`。
5. 有 selection：每个名字必须在已安装集合里。然后 `content::snapshot`、`inspect_project_skills`、`shared_path_owner`。分类规则与 09 的预检相同，但这里不写链接。任一项是 missing、stale、conflict、rejected_copy，或绑定 ambiguous：不写计划，返回候选和原因。全部是 create 或 already 时才 `create_plan`。`will_register` 取自 observe 是否已有 `name`。

返回四个独立字段：`candidates`、`plan`、`approval: absent`、`runtime_visibility` 不出现在推荐里。分数不进入计划。

trait 放在 `ranker.rs`。17 档增加 `OrtCpuReranker`，不改这个函数的返回形状。

## 人可以运行

`cargo test -p skillstar-app recommend_project_skills_`

## 验证

- `recommend_without_selection_does_not_write_a_plan_or_register`
- `recommend_selection_unknown_to_the_hub_does_not_write_a_plan`
- `recommend_conflict_does_not_write_a_plan`
- `recommend_does_not_read_project_source_files`
- `recommend_keeps_bm25_order_when_ranker_is_passthrough`
- `recommend_rejects_unknown_catalog_scope`
- `recommend_does_not_append_recall_events`

用一个项目内的标记文件证明推荐没有打开它：测试把读取钩子放在事实检查只能看到的技能目录上，项目根下的 `SECRET.txt` 字节不变即可。

## 可改

候选 DTO 的展示字段，至少要有技能名、描述摘录、分数。摘录来自 frontmatter description，不读正文剩余部分。

## 不可改

不 `register_canonical_project`。不按分数填 selection。重排失败这个概念不存在；直通就是成功。

## 必须保持绿

06 与 07 的测试。`state/team.json` 的 recall 事件数量不变。

## 会改这一档的反馈

无 selection 的调用创建了计划文件或 `projects.json`。

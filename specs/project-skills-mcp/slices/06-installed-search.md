# 06 — 已安装技能检索

## 契约

查询字符串得到仅含已安装技能的排序命中。不写 `state/team.json`，语料不含 learning。

## 缝

`skillstar_skills::team::recall::search_installed_skills(query, limit) -> Vec<InstalledSkillHit>`。

复用同文件的 `tokenize` 与 BM25。语料只用现有 corpus 里的技能文档循环，不含 learning，不加 `NEIGHBOR_BOOST`。不调用 `store::load` 或 `store::mutate`。

现有 `recall()` 继续为 `skillstar team recall` 写 `recall_events`，行为不变。

`limit` 夹在 1 到 12。查询空或只有停用词时返回空列表，不报错。

## 人可以运行

`cargo test -p skillstar-skills search_installed_skills_`

## 验证

- `search_installed_skills_does_not_append_recall_events`
- `search_installed_skills_omits_learnings`
- `search_installed_skills_orders_the_more_specific_skill_first`

最后一条用 `team/tests.rs` 里 pr-review 对 frontend-design 的同一夹具，并断言 store 文件字节数不变。

## 可改

命中结构是否直接复用 `RecallHit` 的字段子集。不要让调用方依赖 `RecallKind::Learning`。

## 不可改

不从推荐路径调用 `recall()` 再滤掉 learning。那仍然会写事件，并让 learning 进入 IDF。

## 必须保持绿

`bm25_ranks_the_more_specific_skill_first` 与 `skillstar team recall` 的现有测试。

## 会改这一档的反馈

检索写了 `recall_events`，或把 learning 排进技能候选。

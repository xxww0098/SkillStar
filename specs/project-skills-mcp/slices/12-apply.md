# 12 — 应用编排

## 契约

在一把项目写锁里核对计划、批准和幂等回执，然后严格启用，再用同一检查器复核。本函数不向用户要确认。

## 缝

`skillstar_app::project_skills_mcp::apply::apply_project_skills(plan_id, idempotency_key, now)`。

幂等键必须匹配 `^[A-Za-z0-9_-]{1,64}$`，否则错误。

持有 `with_project_write_lock` 的全过程：

1. 读计划。过期或文件哈希与内容不一致：零写入。
2. 读回执。同键且 `plan_hash` 相同：返回原回执，不调用严格启用，不拆链接。同键且哈希不同：冲突，零写入。
3. 读批准。没有记录：返回 `approval_required`，零写入。记录哈希与计划不一致：零写入。
4. 重新 `content::snapshot`。哈希或事实分类与计划不符：零写入。
5. `will_register` 为真时才 `register_canonical_project`。调用时发现同一规范根已经有一条注册：复用它的 `name`，不回写旧 `path`，这仍然算计划执行，不算哈希失效。`will_register` 为假但原条目已经找不到：零写入。ambiguous：零写入。
6. `enable_project_skills_strict`。
7. `inspect_project_skills`。任一项不是计划中的 create 或 already：返回 `partial`，不把回执标成整份成功。
8. 全部符合时写回执到 `state/project-skill-receipts/<idempotency_key>.json`。

回执含逐项状态、`deployment_status`（仅全部为 applied 或 already 时为 `applied`）、`project_scope`、`runtime_visibility: unverified`、`next_action: load_skill`、项目内相对 `SKILL.md` 路径。

不调用 `save_and_sync`、`add_skills_to_project`、市场安装、技能脚本或项目命令。

## 人可以运行

`cargo test -p skillstar-app apply_project_skills_`

测试通过直接调用 `record_from_skillstar` 放置批准。那是测试替身，不是第三种生产写入。

## 验证

- `apply_without_approval_writes_nothing`
- `apply_rejects_expired_and_hash_mismatch`
- `apply_replays_the_same_receipt_without_relinking`
- `apply_same_key_different_hash_conflicts`
- `apply_registers_only_when_the_plan_says_so`
- `apply_does_not_call_save_and_sync_or_add_skills`
- `apply_recheck_matches_get_facts`

## 可改

`approval_required` 在 Rust 里是 `Err` 变体还是 `Ok` 里的状态，只要项目树和索引字节不变，并且调用方分得清它和部署失败。

## 不可改

本函数不调用 `record_from_elicitation`。不在拒绝路径创建链接。幂等回放不调用 `remove` 链接。

## 必须保持绿

09 的严格启用测试。

## 会改这一档的反馈

拒绝、过期或哈希冲突留下了新链接或新的索引行。

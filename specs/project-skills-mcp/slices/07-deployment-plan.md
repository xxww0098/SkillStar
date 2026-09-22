# 07 — 部署计划

## 契约

显式 selection 变成一份不可变计划。没有 selection 就不落盘。`plan_hash` 不包含分数、过期时间和批准。

## 缝

`skillstar_app::project_skills_mcp::plan`。

```text
create_plan(draft) -> Result<Option<DeploymentPlan>>
load_plan(plan_id, now) -> Result<DeploymentPlan>
```

`PlanDraft` 由测试夹具或 10 档构造。`selection` 为空时 `create_plan` 返回 `Ok(None)`，目录里不出现新文件。

非空 selection：一个 `agent_id`，1 到 8 个已通过 `validate_skill_name` 的技能名。`agent_id` 必须是 `has_project_skills()` 的 profile。本档不访问 Hub；技能是否安装由 10 档在调用前检查。夹具可以传入已经算好的 `content_hash`。

哈希域分隔是字节串 `skillstar.project-skill-plan.v1\0`，然后是规范根、`will_register`、排序后的技能名、每项 `content_hash`、`SNAPSHOT_HASH_VERSION`、物理相对路径、owner id、排序后的受影响 Agent id、每项 `create` 或 `already`。过期、`plan_id`、分数、重排器名不进入哈希。`will_register` 在 observe 没有已有 `name` 时为真。

TTL 15 分钟，时钟由参数注入。过期的 `load_plan` 返回错误，不改文件。写入用同目录临时文件加 rename。

`plan_id` 是 UUID。文件在 `state/project-skill-plans/<plan_id>.json`。

## 人可以运行

`cargo test -p skillstar-app deployment_plan_`

## 验证

- `plan_requires_explicit_selection`
- `plan_hash_ignores_scores_and_expiry`
- `expired_plan_is_rejected_without_rewriting_it`
- `plan_store_follows_skillstar_data_dir`
- `plan_rejects_more_than_one_agent_or_more_than_eight_skills`

## 可改

JSON 字段顺序。路径就是 `state/project-skill-plans/<plan_id>.json`。

## 不可改

服务器不凭分数生成 selection。不把计划写进项目树或 `skills-list.json`。

## 必须保持绿

`cargo test -p skillstar-app --locked`。

## 会改这一档的反馈

两份只差分数或 TTL 的计划得到了不同的 `plan_hash`。

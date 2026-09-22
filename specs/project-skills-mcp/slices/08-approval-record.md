# 08 — 批准记录

## 契约

批准只有两个写入函数。工具参数没有通向记录的转换。同一计划不能同时拥有两种来源。

## 缝

`skillstar_app::project_skills_mcp::approval`。

```text
record_from_elicitation(plan_id, plan_hash) -> Result<ApprovalRecord>
record_from_skillstar(plan_id, plan_hash) -> Result<ApprovalRecord>
load_approval(plan_id) -> Result<Option<ApprovalRecord>>
```

记录含 `plan_id`、`plan_hash`、`source`（`elicitation` 或 `skillstar`）、写入时间。没有 `user_confirmed`。

同一来源、同一哈希再次写入：返回原记录。另一来源已存在，或哈希不一致：错误，不覆盖文件。文件在 `state/project-skill-approvals/<plan_id>.json`。

本模块不引用 rmcp，不读项目目录。14 和 15 才是这两个函数的生产调用点。

## 人可以运行

`cargo test -p skillstar-app approval_record_`

## 验证

- `elicitation_then_skillstar_is_rejected`
- `skillstar_then_elicitation_is_rejected`
- `same_source_same_hash_is_idempotent`
- `approval_json_has_no_user_confirmed_field`
- `approval_store_follows_skillstar_data_dir`

## 可改

时间字段用 RFC3339 字符串还是 unix 秒，测试不依赖它的展示格式。

## 不可改

不增加第三种 `source`。不从 `serde_json::Value` 的工具参数构造记录。

## 必须保持绿

`cargo test -p skillstar-app --locked`。

## 会改这一档的反馈

任何 MCP 工具参数能写成这份 JSON。

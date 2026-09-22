# 13 — 三个 MCP 工具

## 契约

stdio 服务只广告这三个工具，参数里没有批准字段。本档在客户端未声明 elicitation 时打通推荐、查询和应用；应用没有 SkillStar 批准时得到 `approval_required`。

## 缝

`skillstar_app::project_skills_mcp::protocol` 把 rmcp 工具参数转换成 10、11、12 的入参。

依赖用：

```bash
cargo add rmcp --package skillstar-app --no-default-features --features server,macros,schemars,transport-io,elicitation
```

不要打开 `client`、`auth` 或 HTTP transport。01 档的空 handler 换成这个协议实现。进程规则不变。

工具与参数，全部 `deny_unknown_fields`：

| 工具 | 参数 |
| --- | --- |
| `recommend_project_skills` | `project_path`，`query`，可选 `constraints`，可选 `focus_paths`，可选 `selection`（`skill_names` + 一个 `agent_id`），`catalog_scope` |
| `get_project_skills` | `project_path` |
| `apply_project_skills` | `plan_id`，`idempotency_key` |

没有 `user_confirmed`、批准来源、`plan_hash` 入参。`plan_hash` 出现在推荐的结构化结果里，供人和下一档的 elicitation 展示，不作为 apply 的授权。

Server capabilities 启用 tools。不启用 roots。不提供资源，技能正文不进结果。

结构化结果与一段短文本同时返回。短文本供只显示文本的宿主阅读，字段以结构化结果为准。

## 人可以运行

在 01 的管道探针上继续发送 `tools/list`、一次无 selection 的 recommend、一次无批准的 apply。`tools/list` 恰好三个名字。recommend 不创建 `projects.json`。apply 不改项目目录。

`cargo test -p skillstar-app project_skills_mcp_protocol_`

优先用 rmcp 3.4 的进程内传输。方法名以该版本为准。

## 验证

- `tools_list_is_exactly_the_three_project_skill_tools`
- `recommend_tool_rejects_user_confirmed`
- `apply_tool_has_no_approval_field`
- `server_capabilities_omit_roots`
- `unapproved_apply_does_not_mutate_the_project`

同时更新 `docs/features/project-skills-mcp/README.md`、`docs/boundaries.md`、`docs/architecture.md`、`docs/decisions.md`，并在 `Agents.md` 功能入口加上该文档。不要改 `docs/features/mcp/README.md` 的三类模型；可以加一句交叉链接，指向新文档。

## 可改

短文本的句子。rmcp 宏是 `#[tool]` 还是手写 router，只要工具名和参数形状不变。

## 不可改

不把领域类型放进 `skillstar_app::mcp`。不增加第四个工具。apply 在本档不发起 elicitation。

## 必须保持绿

`bash scripts/internal/check_command_boundaries.sh`、`check_workspace_deps.sh`、`check_file_size.sh`。`cargo check --workspace --locked`。

## 会改这一档的反馈

`tools/list` 出现业务工具以外的名字，或未知字段被静默忽略后仍执行。

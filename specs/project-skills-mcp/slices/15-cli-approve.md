# 15 — CLI 批准

## 契约

人在终端里对一份已存在的计划写入 SkillStar 批准。这不是 MCP 工具。声明了 elicitation 的客户端不会因为这份记录而跳过 14。

## 缝

`skillstar mcp approve <plan_id>` 进入现有 CLI 分发，不经过 serve 的 stdout 规则。它本来就要被人读。

命令从计划存储读取计划，把 14 所用的同一份差异打到 stdout，然后从 stdin 读一行。该行等于 `approve <plan_hash>` 时调用 `record_from_skillstar`。其他输入退出非 0，不写记录。

没有对应的 MCP 工具。`is_cli_subcommand("mcp")` 已经为真；子命令解析必须接受 `approve`，且 `serve` 仍要求 `--stdio`。

非交互环境（stdin 不是 tty 且没有把确认行从管道送入）退出非 0，不写记录。测试用管道送入确认行。

## 人可以运行

```bash
cargo run -p skillstar -- mcp approve <plan_id>
```

先用 13 的 recommend 工具带上 selection 生成计划，再在另一个终端批准，然后无 elicitation 的 apply 才能启用。

## 验证

- `cli_approve_requires_the_exact_plan_hash_line`
- `cli_approve_rejects_a_plan_already_approved_by_elicitation`
- `cli_approve_does_not_deploy`

## 可改

差异的排版。确认行的字面量不变。

## 不可改

不在 serve 的 stdout 上打印差异。不调用 `enable_project_skills_strict`。

## 必须保持绿

`known_cli_subcommands_are_detected`。01 的 stdout 纯度测试。

## 会改这一档的反馈

批准命令本身创建了项目链接。

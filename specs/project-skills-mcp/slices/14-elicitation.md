# 14 — elicitation

## 契约

客户端在 `initialize` 里声明了 form elicitation 时，`apply_project_skills` 在领域写入之前向人展示计划差异。这次交互的接受才写入 elicitation 批准。事先存在的 SkillStar 批准不能代替它。

客户端没有该能力时，本档不发 elicitation，行为与 13 相同。

## 缝

逻辑留在 `protocol`。领域 `apply_project_skills` 仍然不调用 rmcp。

能力位只读客户端 initialize 的 capabilities。rmcp 3.4 的具体方法名以实现时的文档为准，记录在本切片末尾的决定记录里。

有能力时：

1. 加载计划，向用户展示规范根、是否会注册、owner、受影响 Agent、每个技能的操作与相对路径、`plan_hash`。
2. 用户拒绝或超时：不调用 `record_from_elicitation`，不调用领域 apply。
3. 用户接受：`record_from_elicitation`，然后领域 apply。接受值必须针对当前 `plan_hash`。
4. 若该 `plan_id` 已有 SkillStar 来源的记录，不把那份记录当作本通道的批准。`record_from_elicitation` 按 08 的规则拒绝另一来源。

无能力时：不发送 elicitation 请求。领域 apply 只认 SkillStar 记录。

## 人可以运行

`cargo test -p skillstar-app project_skills_mcp_elicitation_`

用进程内传输构造两份 initialize：一份带 form elicitation，一份不带。

## 验证

- `elicitation_decline_writes_nothing`
- `elicitation_accept_records_elicitation_source_then_applies`
- `preexisting_skillstar_approval_does_not_skip_elicitation`
- `client_without_elicitation_does_not_receive_an_elicit_request`
- `tool_arguments_cannot_supply_the_accept`

## 可改

表单里除 `plan_hash` 和差异字段以外的说明文字。

## 不可改

不把模型在工具参数里写的接受当成 elicitation 结果。不在无能力的客户端上假装已经确认。

## 必须保持绿

13 的协议测试。08 的互斥测试。

## 会改这一档的反馈

某个客户端的能力位被置上，但接受结果实际上由同一轮模型消息填写，而不是客户端的 elicitation 响应。该客户端改判为无能力，并记在本文件。若 rmcp 3.4 没有 form 模式的服务器 API，停在本档，把无能力路径留成唯一通道，删掉发不出去的请求。

# 项目技能 MCP

开发 Agent 通过本机 `skillstar mcp serve --stdio` 调用 SkillStar。stdout 只有换行分隔的 JSON-RPC。tracing 写 stderr。

## 进程

`argv[1] == "mcp"` 时，进程在 Git askpass 和桌面窗口之前进入 `skillstar_app::project_skills_mcp::serve`。`is_cli_subcommand("mcp")` 为真，所以这条参数不会打开 GUI。

serve 安装全局技能变更策略（`install_global_policy`）并迁移旧数据目录（`migrate_legacy_paths`）。它不初始化 marketplace snapshot，也不走 CLI 的 `migrate_and_run`。

没有 `serve --stdio` 时向 stderr 写用法，退出码非 0，stdout 为空。

Windows release 仍使用 `windows_subsystem = "windows"`，不分配控制台。父进程接上的管道就是传输。这个探针还没有在 Windows release 二进制上跑过。

## 工具

工具参数和结果在 `skillstar_app::project_skills_mcp::protocol`。名字是 `recommend_project_skills`、`get_project_skills`、`apply_project_skills`。参数使用 `deny_unknown_fields`。没有 `user_confirmed`、批准来源或 `plan_hash` 入参。`plan_hash` 只出现在推荐的结构化结果里，不能当作应用的授权。

每次调用同时返回结构化结果和一段短文本。字段以结构化结果为准。结果不包含技能正文，也不包含 Hub 绝对路径。Server capabilities 启用 tools，不启用 roots 或 resources。

客户端在 `initialize` 里声明 `capabilities.elicitation.form` 时，`apply_project_skills` 先发送 form 模式的 `elicitation/create`。用户交回的 `plan_hash`、项目根、是否注册、owner、受影响 Agent 和变更列表必须与当前计划一致，然后才写入 elicitation 批准并调用领域 apply。拒绝、取消或超时不写批准，也不改项目。已经存在的 SkillStar 批准不能跳过这次确认，也不能被这次接受覆盖。

没有 form 能力时不发送 elicitation。应用只认已经写好的 SkillStar 批准。没有这份批准时，结果是 `approval_required`，项目树和项目索引都不改。

确认发生在项目写锁之外。

## CLI 批准

`skillstar mcp approve <plan_id>` 走普通 CLI，不进入 serve 的 stdout。它打印与 form elicitation 相同的差异，再从 stdin 读一行。该行必须是 `approve <plan_hash>`。其他输入和非交互且没有管道输入时退出非 0，不写批准，也不创建项目链接。这份 SkillStar 批准不能让声明了 form elicitation 的客户端跳过确认。

## 桌面批准

Projects 页在选中项目且存在未过期计划时，在列表上方展示规范路径、是否注册、owner、受影响 Agent 和每一项操作。批准按钮调用 `approve_project_skill_plan`，只写 SkillStar 批准，不部署。没有计划时不显示这块区域。

## 重排

推荐的默认顺序是已安装技能的 BM25。`SKILLSTAR_LAYA_ONNX` 指向含 `laya.onnx`、`laya.onnx.data`、`laya_config.json` 和 `tokenizer/` 的目录时，第一次推荐在 CPU 上加载 Laya。每个候选单独问一次该技能是否适用于这次任务，只用名称和 frontmatter 描述，不送 `SKILL.md` 正文。至多打分 12 个，按 true 的概率从高到低排；平分保持 BM25 原序，低分也不删候选。

含汉字的任务只有 multilingual 导出才打分。Hugging Face `receptron/laya-onnx` 是英文包。`laya_config.json` 可以用 `language` 写成 `en` 或 `multilingual`；没有这个字段时，用 tokenizer 的特殊符号区分。看不出语种、文件缺失或 Session 失败都保持 BM25，推荐仍然成功。分数不进入 `plan_hash`。模型不进仓库和安装包，应用也不下载。PyTorch 只用于另行导出，不进应用依赖。`ort` 只注册 CPU Execution Provider。

## 应用

`apply_project_skills` 在项目写锁里核对计划、批准和幂等回执，然后调用严格启用。没有批准时返回 `ApprovalRequired`，不改项目树和项目索引。回执只在每一项都是链接意义上的 applied 或 already 时写入 `state/project-skill-receipts/<idempotency_key>.json`。回执里的 `runtime_visibility` 是 `unverified`。

这个函数不向用户要确认，也不调用宽松部署或市场安装。

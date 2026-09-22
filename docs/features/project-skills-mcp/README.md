# 项目技能 MCP

开发 Agent 通过本机 `skillstar mcp serve --stdio` 调用 SkillStar。stdout 只有换行分隔的 JSON-RPC。tracing 写 stderr。

## 进程

`argv[1] == "mcp"` 时，进程在 Git askpass 和桌面窗口之前进入 `skillstar_app::project_skills_mcp::serve`。`is_cli_subcommand("mcp")` 为真，所以这条参数不会打开 GUI。

serve 安装全局技能变更策略（`install_global_policy`）并迁移旧数据目录（`migrate_legacy_paths`）。它不初始化 marketplace snapshot，也不走 CLI 的 `migrate_and_run`。

没有 `serve --stdio` 时向 stderr 写用法，退出码非 0，stdout 为空。

Windows release 仍使用 `windows_subsystem = "windows"`，不分配控制台。父进程接上的管道就是传输。这个探针还没有在 Windows release 二进制上跑过。

本档不注册业务工具。`tools/list` 是空列表。

# 16 — 桌面批准

## 契约

Projects 页对一份待批准计划展示与 CLI 相同的差异。用户点击批准后调用 `record_from_skillstar`。页面不部署。

## 缝

新 Tauri command 放在 `src-tauri/src/commands/project_host.rs`。不要放进 `mcp_commands.rs`。函数体只调 `record_from_skillstar` 和计划读取。command 不直接读写项目技能目录，不发 HTTP。

若 DTO 跨过 IPC，用 ts-rs，然后 `bun run types:gen`。不手改 `src/types/generated/`。协议工具的参数类型不进生成目录。

页面落在现有 Projects 页，展示规范路径、注册与否、owner、受影响 Agent、每项操作。没有计划时不显示这块区域。

## 人可以运行

`bun run tauri dev`，打开有待批准计划的项目。点击批准前后，项目技能目录不变，批准文件出现。再到无 elicitation 的 MCP apply，部署才发生。

## 验证

- `skillstar_approve_command_rejects_a_plan_already_approved_by_elicitation`
- `skillstar_approve_command_does_not_deploy`
- 前端测试覆盖空状态和有计划时的差异字段

视觉收尾：对批准区域截图，用 screenshot-critique 做一次不带实现者结论的复查，通过后再接受本档。没有旧界面可对照，不做 compare-screenshots。

这一档的人工查看不阻塞。用 preview-shots 打开截图，大约等 5 分钟。没有回复就按截图和测试自己决定，把决定写进本文件，关掉预览窗口，继续。不要停在这里等签收。

桌面与窄宽度都要看：差异列表在窄宽度下仍能读到技能名和操作，不与 Projects 页已有控件重叠。

## 可改

按钮文案、区域放在页面的上侧还是项目卡片内，只要差异字段都在。

## 不可改

不在前端自己写批准 JSON。不调用 `save_and_sync`。

## 必须保持绿

`bun run lint`、`bash scripts/internal/check_feature_imports.sh`、`check_command_boundaries.sh`。改了 Rust 导出则生成文件与源一起提交。

## 会改这一档的反馈

批准点击后项目目录里出现了新的技能链接，或差异里看不到受影响的其他 Agent。

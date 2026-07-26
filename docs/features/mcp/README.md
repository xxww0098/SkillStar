# MCP 管理与市场投影

状态：active

本文件维护 MCP 本地 store、Agent tool sync、市场类型接缝和前端页面职责。

## 三类模型不可混用

- `skillstar_models::mcp`：用户本地 MCP store、server patch、preset、tool status 和 sync result。
- `skillstar_marketplace::mcp_models`：Marketplace Publisher、registry package/remote 和 detail。
- `src-tauri::commands::mcp_commands::McpServerWithSync`：命令层返回的 server + per-tool sync DTO。

这三组 Rust 类型通过 ts-rs 导出到 `src/types/generated/`，`src/types/mcp.ts` 只做 re-export。修改字段后运行 `bun run types:gen`；不得在 TypeScript 手写第二份大型 wire type。

## Store 与工具同步

- Rust 侧 MCP 工具事实（label、配置路径、安装探测、wire-format 的计数/读取/写入/移除 dispatch）的 SSOT 是 `skillstar_models::mcp` 的 `McpToolSpec` 注册表；新增工具只加一行 spec（新 wire format 才需要新的 spec builder）。隐藏的 legacy `claude-desktop` cleanup id 刻意不进注册表。
- MCP store 与 Marketplace snapshot 是不同数据源：市场只负责发现，安装后进入 Models MCP store。
- create/update/delete/rename 通过统一 store facade 编排各 Agent projector；部分失败要返回每个目标结果，不静默吞掉。
- live config 路径使用与 Models tool-sync 相同的 `SKILLSTAR_TOOL_SYNC_HOME` resolver，测试不写真实 home。
- MCP 可操作 target 遵循 [Skills 的本机 Agent 可见性规则](../skills/README.md#agent-注册手动启用与项目检测)，只与 MCP 支持映射取交集；不再用实际 tool probe 隐藏用户已手动启用的 Agent。同步时若目标配置不可写，按目标返回明确失败。

## Claude 兼容边界

- 公开 target 只有 `claude-code`，对应 `~/.claude.json`，同时服务 CLI 与 Desktop Code。
- 旧 `claude-desktop=true` 只表示 Desktop Chat cleanup tombstone：可以清理旧条目，但永不作为新 target、永不重新写入或渲染。
- cleanup 只删除 SkillStar 管理的 named server，保留其他 JSON 字段；malformed JSON fail closed，原文件不动。
- rename/delete 在 cleanup 失败时不提交新 store 状态，以便下次重试。

## Marketplace 接缝

- curated rows 与远端 GitHub registry snapshot 合并，远端刷新不得覆盖或删除 curated rows。
- Publisher 顺序、source id 和 server 清单以 `skillstar-marketplace` seed/query 代码为准，不在文档复制枚举。
- curated `raw_server_json` 与 registry shape 对齐，从而复用同一个 install form/转换路径。

## 前端职责

- Marketplace MCP tab：Publisher grid → `McpPublisherDetail` → server install drawer。
- MCP 页面：已安装 server、推荐 preset、编辑和 Agent tool sync；不嵌入另一个市场浏览器。
- Agent rail 复用 `AgentTargetCarousel`，显示名和图标来自 Settings profile，而不是 MCP 自己维护 SVG registry。

## 验证

```bash
cargo test -p skillstar-models -p skillstar-marketplace -p skillstar export_bindings
bun run test -- src/features/mcp
bun run types:gen
```

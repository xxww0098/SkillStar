# 01 — stdio 进程入口

## 契约

`skillstar mcp serve --stdio` 进入无界面进程。stdout 只有 JSON-RPC。`initialize` 能在 stdin 仍打开时返回。本档不暴露三个业务工具。

## 缝

- `src-tauri/src/main.rs`：`argv[1] == "mcp"` 时，在 `handle_internal_askpass` 之前调用 `skillstar_app::project_skills_mcp::serve`。
- `serve` 调用 `install_global_policy` 与 `migrate_legacy_paths`。不调用 marketplace snapshot `initialize`。不走 `src-tauri/src/cli/mod.rs` 的 `migrate_and_run`。
- tracing subscriber 的 writer 固定 stderr。CLI 路径今天不初始化 tracing；GUI 的 subscriber 没有 `with_writer`，serve 不能复用它。
- `is_cli_subcommand` 与 `known_cli_subcommands_are_detected` 同时加上 `"mcp"`。没有 `--stdio` 时向 stderr 报错，退出码非 0，stdout 为空。
- 本档的 handler 只回答 `initialize` 和空的 `tools/list`。13 档换成真正的工具 handler，不改本档的进程规则。

## 人可以运行

```bash
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"probe","version":"0"}}}' \
  | cargo run -p skillstar -- mcp serve --stdio
```

stdout 第一字节是 `{`。`RUST_LOG=trace` 与 `SKILLSTAR_GIT_ASKPASS_MODE=1` 再跑一次，stdout 仍无日志、无 askpass 文本。

## 验证

- `mcp_is_a_cli_subcommand_and_not_gui`
- `serve_without_stdio_writes_only_stderr`
- `serve_stdio_initialize_emits_only_jsonrpc`
- `askpass_env_does_not_swallow_mcp`

`cargo test -p skillstar-app mcp_stdio_` 与更新后的 `known_cli_subcommands_are_detected`。

Windows release 二进制用继承管道再做一次。失败则停止后续档，更新本切片，不改 `windows_subsystem`，不调用 `AllocConsole`。当前机器不是 Windows 时，在规格状态里写明该探针未跑，不要标成已通过。

本档同时新增 `docs/features/project-skills-mcp/README.md` 的进程小节，并在 `docs/boundaries.md` 记下模块名。工具行为留到 13 档写全。`Agents.md` 的功能入口在 13 档再加，避免链到一份还没有工具的文档。

## 可改

handler 的内部类型名、JSON-RPC 库调用的具体函数名（必须是 rmcp 3.4 的 stdio server）。

## 不可改

stdout 独占、跳过 askpass、跳过 marketplace init、本档不注册业务工具。

## 必须保持绿

`cargo check --workspace --locked`。`is_cli_subcommand("gui")` 仍只走 GUI。

## 会改这一档的反馈

Windows release 管道读不到响应，或任何启动日志出现在 stdout。

## 决定

- `rmcp` 在本档加入，feature 用 13 档列出的集合，另加 `transport-async-rw`，测试用内存 duplex，不占用进程 stdout。
- 客户端 `initialize` 请求 `2026-07-28` 时，rmcp 3.4 把该版本视为没有 initialize 握手，响应里的 `protocolVersion` 是 `2025-11-25`。探针只要求 stdout 第一字节是 `{`。
- Windows release 继承管道探针未跑。macOS debug 单测不能代替它。

# 04 — 环回监听

## 契约

`skillstar gateway serve` 在 `127.0.0.1:21847` 上提供 01 档已经证明的 Chat 路径。`SKILLSTAR_GATEWAY_ADDR` 可以改地址。值里的端口是 `3425` 时，进程以非 0 退出，不绑定。GUI 与这条 CLI 调用同一个 `serve`。桌面应用启动后网关就在听，离开 Models 页也不停。

同一条夹具经真实 HTTP 走一遍，出站正文与 01 档一致。上游用测试里的假服务器。

## 缝

`skillstar_gateway::serve`。`skillstar-app` 用 `cargo add -p skillstar-app skillstar-gateway` 依赖它，并提供 CLI 入口。`src-tauri/src/main.rs` 在打开窗口之前认出 `gateway`，走 app 的 serve，不走 GUI。`is_cli_subcommand` 同时加上 `gateway`。没有子命令 `serve` 时错误写 stderr，退出码非 0，stdout 为空。

`src-tauri` 不直接依赖 `skillstar-gateway`。

流式客户端加在 `skillstar_core::infra::http_client`：与 `probe_http_client` 同一代理指纹，连接超时 10 秒，没有覆盖响应体的总超时，响应头最多等 10 分钟。入站服务器读头 30 秒，空闲 5 分钟。短探测仍走 `probe_http_client`。

`docs/boundaries.md` 的 mermaid 加上 `app → gateway` 与 `gateway → core`。`docs/architecture.md` 写监听地址、谁启动 serve、配置文件将放在哪。`docs/features/models/README.md` 加一小节：本机网关的监听，并写明旧的「本轮不做 proxy takeover」不再描述当前目标。`README.md` 加上这两条命令：`skillstar gateway serve`、以及 05 档才会用到的 helper 先不写。

依赖守卫增加正向断言：`skillstar-app` 依赖 `skillstar-gateway`。

## 人可以运行

```bash
cargo run -p skillstar -- gateway serve
```

另一个终端对 `http://127.0.0.1:21847/v1/chat/completions` 发送 01 档的 Chat 夹具。`SKILLSTAR_GATEWAY_ADDR=127.0.0.1:3425` 时期望非 0，且 `lsof` 看不到监听。

## 验证

- `serve_binds_default_port`
- `serve_refuses_magpie_port`
- `serve_chat_fixture_matches_translate`
- `gateway_is_a_cli_subcommand_and_not_gui`
- `stream_client_uses_probe_proxy_fingerprint`

本档路由只有直连一个假上游。不加载 Usage。

## 可改

serve 的内部任务结构。

## 不可改

默认地址、拒绝 3425、CLI 不打开窗口、stdout 不打日志、流式客户端不另起忽略 `proxy.json` 的客户端。

## 必须保持绿

`cargo check --workspace --locked`。`cargo test -p skillstar-gateway`。`bash scripts/internal/check_workspace_deps.sh`。

## 会改这一档的反馈

假上游收到的正文和 01 档夹具不一致，或 `3425` 能绑定成功。

## 决定

- 占位 bearer 常量本档就定成 `skillstar`。鉴权失败的测试在 08 档补全路径时一起锁。
- 桌面应用与 CLI 同时启动时，后一个 serve 把「地址已被占用」写到 stderr 并不再监听第二份。不杀掉先启动的那份。

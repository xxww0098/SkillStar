# 34 — 局域网

## 契约

监听模式可以显式改成局域网。改完后进程听 `0.0.0.0:<端口>`。写进任何 Agent 文件的 URL 仍是 `http://127.0.0.1:<端口>` 加原来的路径。默认仍是环回。`SKILLSTAR_GATEWAY_ADDR` 若指定端口 `3425`，无论是否局域网，都拒绝绑定。

配额和账户相关的调试输出不因为听了局域网就对非环回可见。本产品本来就没有配额路由。

## 缝

开关写入 `model_gateway.json` 的监听字段。`serve` 读它。Agent writer 读的是公布 URL，不是监听地址。公布 URL 的测试与 magpie `lan_test.go` 的断言同一形状：监听 `0.0.0.0` 时公布值仍是 `127.0.0.1`。

## 人可以运行

打开开关，确认监听地址变了，再保存 Codex，toml 里仍是 `127.0.0.1`。

## 视觉

变量只有一个：局域网开关的状态。

裁剪：开关和它的一行说明。

screenshot-critique。无参照则跳过 compare-screenshots 并记入 `choices.md`。视口两档。preview-shots 约 5 分钟。

验收句：开关能看出当前是环回还是局域网；说明里写的 Agent 地址仍是 127.0.0.1。

## 验证

- `lan_off_listens_loopback`
- `lan_on_listens_unspecified_and_publishes_loopback`
- `lan_refuses_3425`
- `lan_agent_file_stays_loopback`

## 可改

说明文字的用词。地址规则不可改。

## 不可改

默认环回、公布 URL、拒绝 3425。

## 必须保持绿

04 档 `serve_refuses_magpie_port`。06 与 18 的 URL 断言。

## 会改这一档的反馈

Agent 文件里出现 `0.0.0.0` 或局域网网卡地址。

## 决定

- NAT 下的 WSL 使用 24 档的 Windows 主机地址，不受「公布 URL 一律 127.0.0.1」这条限制。那是 WSL 自己的拼法，只在 `codex@wsl:` 上出现。

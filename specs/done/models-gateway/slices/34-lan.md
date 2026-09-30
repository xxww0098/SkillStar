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
- 字段是 `listen`。去掉两端空白后恰好是 `lan` 才听 `0.0.0.0`。环回时删掉这个字段。其它值、缺文件或读不出来都当环回，读的时候不改文件。
- 端口仍来自 `SKILLSTAR_GATEWAY_ADDR`，缺省 `21847`。`3425` 在改主机之前拒绝，局域网打开也不绑定。
- 公布地址总是 `http://127.0.0.1:<端口>`。文件型 Agent 走 `published_origin`。Codex 的 `apply_agent` 仍由调用方传入 origin；这一档的测试传入 `published_origin()`。
- 不移植 `sk-magpie-`、`lanGuard` 或局域网地址列表。没有配额路由。非环回看到的路由和环回相同。
- 已经在听的进程不重新绑定。开关在下一次 `serve` 解析地址时生效。
- Gateway 栏里，配置档下面是一组「环回 / 局域网」，加上一句写给 Agent 的地址仍是 `127.0.0.1`。查询失败不画出来。保存失败留下 `listen_store`，不改已经按下的按钮。
- 参照作物 `assets/magpie/34-lan.png` 不在仓库里，不下载。1440×900 环回按下，1280×800 点过局域网后按下。裁剪只有这一组。五问：这一裁决定听环回还是局域网；配置档名字不在裁剪里；这一裁没有密钥和厂商 URL；没有 magpie 对照，所以不比较；第一下点环回或局域网。

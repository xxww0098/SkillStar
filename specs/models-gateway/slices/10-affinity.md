# 10 — 亲和

## 契约

会话留在上次回答它的钥匙或账户上。模式：空字符串是 auto，另外有 session、turn、off。auto 在回合内保持；跨回合时，上次缓存读入不少于 `cacheWorth`（1024）且距离不超过 `cacheCold`（5 分钟）才保持。记忆保留 `stickKeep`（24 小时）。

## 缝

`skillstar-gateway` 的纯函数，输入是模式、上次 stick、本请求的回合信息、现在的时钟。会话 id 来自 `X-Skillstar-Session`，否则来自 README 列出的 Agent 自有头。不读 `X-Magpie-Session`。

夹具目录 `tests/fixtures/magpie/affinity/`，每组一个模式一种结果：`session`、`turn`、`off`、`cache`、`cold`、`no-cache`、`first`。

## 人可以运行

```bash
cargo test -p skillstar-gateway affinity_
```

## 验证

- 上面七个 why 各一条
- `affinity_ignores_magpie_session_header`
- 24 小时之后 stick 不再命中

## 可改

stick 表的键的内部拼法，只要会话隔离仍然成立。

## 不可改

四个模式、1024、5 分钟、24 小时、why 的取值集合。

## 必须保持绿

09 档的顺序测试在亲和为 off 时结果不变。

## 会改这一档的反馈

某个 why 和 magpie `affinity.go` 的注释不一致。

## 决定

- 休息中的 stick 本档标成 `resting`，真正跳过候选在 11 档。

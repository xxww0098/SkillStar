# 22 — Alma

## 契约

Alma 在跑时，经 `http://localhost:23001` 写入名为 `skillstar` 的 openai provider，base URL 是网关 `/v1`，并把默认模型设成所选 id。Alma 没在跑不是错误：不写文件，返回成功，字节数 0。

## 缝

`skillstar-gateway`。测试把 Alma 的基址指到假服务器。生产默认是 `http://localhost:23001`，与 magpie `alma.go` 一致。假服务器不在时走「没在跑」分支。

## 人可以运行

```bash
cargo test -p skillstar-gateway alma_
```

## 验证

- `alma_live_sets_provider_and_default_model`
- `alma_down_is_ok_and_writes_nothing`
- `alma_does_not_touch_files`

## 可改

探测「没在跑」的连接超时，上限 1 秒，测试注入。

## 不可改

没在跑不算失败、不写本地文件冒充 Alma 的数据库、base URL 为环回 `/v1`。

## 必须保持绿

Hanako 的文件路径测试，确认 Alma 不写 Hanako 的目录。

## 会改这一档的反馈

Alma 没启动时返回错误，或在临时 HOME 里写出了 provider 文件。

## 决定

- 默认模型的字符串形状是 `<providerId>:<model>`，providerId 来自 Alma 创建 provider 的响应，不使用 SkillStar 的 v4 id。
- 没连上，或沙箱里没设 `SKILLSTAR_ALMA_URL`，都算没在跑：返回成功，不写文件。连上之后 HTTP 不是 2xx 才是错误。
- 生产基址固定 `http://localhost:23001`。沙箱只认 `SKILLSTAR_ALMA_URL`，主机只能是 `localhost` 或 `127.0.0.1`。探测超时默认 1 秒；沙箱可用 `SKILLSTAR_ALMA_TIMEOUT_MS` 注入，超过 1 秒按 1 秒。
- settings 先 GET 再整份 PUT，只改 `chat.defaultModel`。模型列表只放这一次的 `model_ref`。取消托管删掉名为 `skillstar` 的 provider；默认模型若指向它就清空。不写本地数据库，Alma 不进 `FILE_AGENTS`。

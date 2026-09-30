# 11 — 休息与换上游

## 契约

02 档锁定了「内容字节之后不换」。本档锁定换的时候跳过谁、跳多久。时长以 README 常量表为准：`creditRest` 30 分钟，`quotaRest` 15 分钟，`longestWait` 1 小时，`longestQuota` 8 天，`longestRetry` 10 分钟，`verifyRest` 30 分钟，`verifyHold` 1 分钟，`fallbackCooldown` 1 分钟。分类失败的 30 秒休息不在本档。频率限制没有 Retry-After 时用 `fallbackCooldown`，不按配额休息。配额恢复时间读正文和 `X-Skillstar-Resets-At`，不读 `X-Magpie-Resets-At`。

错误正文的归类（余额、配额、频率、验证）跟 magpie `routing.go` 的那些正则。中文词也在里面。

## 缝

纯函数：失败状态与正文进，休息截止时间出。下一次 `plan` 去掉未到期的候选。测试注入时钟。不发真实 HTTP。

## 人可以运行

```bash
cargo test -p skillstar-gateway rest_
```

## 验证

- 每种休息时长一条
- `rest_rate_limit_is_not_quota`
- `rest_after_content_byte_does_not_call_second_upstream`（把 02 的保持写者和本档的候选表接在一起）

## 可改

正则的代码组织。词表与 magpie 保持一致，不自行增删。

## 不可改

时长、频率不按配额休息 15 分钟、内容字节之后的调用次数。

## 必须保持绿

`hold_` 与 `route_mode_`。

## 会改这一档的反馈

一条 429 被休息成 15 分钟，而正文是频率限制。

## 决定

- 不向 Usage 拉取「何时重置」。快照里没有重置时间就用上表的缺省时长。

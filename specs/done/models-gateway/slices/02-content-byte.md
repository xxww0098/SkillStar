# 02 — 首内容字节

## 契约

上游已经写出内容字节之后，这一回复不再发给第二个上游。内容字节之前的失败可以换。`message_start`、`response.created`、`response.in_progress`、`response.queued`、`ping`、只带 `role` 的 Chat 块、`codex.` 前缀事件都不是内容。等到 `holdLongest`（15 秒）或缓冲超过 `holdMost`（1 MiB）则放行，此后也不再换。

## 缝

`skillstar-gateway` 内的保持写者。测试用内存 `Write`，不监听。时钟由测试注入。数字以 README 常量表为准。

夹具：`crates/skillstar-gateway/tests/fixtures/magpie/hold/`。每组说明事件序列、哪一拍算内容、换上游的次数。

## 人可以运行

```bash
cargo test -p skillstar-gateway hold_
```

## 验证

- `hold_swaps_upstream_when_error_precedes_content`
- `hold_keeps_upstream_after_first_content_byte`
- `hold_treats_lead_events_as_not_content`
- `hold_releases_at_15s_or_1mib`

本档不决定下一个候选是谁，那是 11 档。

## 可改

保持写者的内部缓冲类型。

## 不可改

哪些事件算内容、两个阈值、放行后不再换上游。

## 必须保持绿

`cargo test -p skillstar-gateway translate_` 与 `hold_`。

## 会改这一档的反馈

magpie `fallback.go` 的 `streamEvent` 把某类事件改了归类。先改 README 常量表和本档夹具，再改代码。

## 决定

- 阈值用测试时钟，不在单元测试里真等 15 秒。

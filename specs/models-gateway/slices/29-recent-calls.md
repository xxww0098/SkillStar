# 29 — 最近请求

## 契约

Gateway 栏列出进程内最近 60 条调用：时间、Agent、模型、状态、本次补全的 token 数。没有配额条，没有「剩余额度」，不请求 Usage 的配额接口。重启后列表为空。

## 缝

环在 `skillstar-gateway`，`traceKeep = 60`。`skillstar-app` 用事件或查询把它交给页面。DTO 不含密钥和上游 URL。

## 人可以运行

对 `gateway serve` 发一条 Chat 夹具，Models 的 Gateway 栏出现一行。第 61 条挤掉第 1 条。

## 视觉

变量只有一个：最近请求列表的内容。

裁剪：Gateway 栏里的那张表。

本档不评：路由控件、分组编辑、空栏句子。

screenshot-critique。参照 `assets/magpie/29-recent.png` 时 compare-screenshots，裁剪只盖住表。视口两档。preview-shots 约 5 分钟。

验收句：表是一条条调用，能看到 token 数；看不到配额剩余，看不到厂商 URL。

## 验证

- `trace_keeps_60`
- `trace_is_memory_only`
- 页面探针：配额字样不在这张表的列名里

## 可改

列的左右顺序。列集合不可少 Agent、模型、状态、token。

## 不可改

60、不落库、不接 Usage 配额、DTO 无上游 URL。

## 必须保持绿

`/v1/skillstar/quotas` 仍然 404。

## 会改这一档的反馈

表上出现剩余额度，或重启后旧记录还在。

## 决定

- token 数来自这一次补全的用法字段。没有用法时这一格为空，不写成 0 冒充。

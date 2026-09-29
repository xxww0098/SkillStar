# 39 — 不写环回 URL 的文件 Agent

## 契约

Goose、Cursor CLI、Copilot CLI、Devin 在 magpie 里的保存不写环回 URL，也不写占位 bearer。`apply_gateway` 对 `goose`、`cursor`、`copilot`、`devin` 返回 `agent_not_managed`，写入字节数为 0。不为它们发明 base URL 或密钥字段。

## 缝

行为已经落在 18 档的 `apply_gateway`。本档不新增第二种写入。

## 人可以运行

临时目录里对这四个 id 调用 `apply_gateway`，目录仍是空的。

## 验证

- `apply_gateway_name_only_still_unmanaged`

## 可改

测试名字。

## 不可改

这四个 id 仍是 `agent_not_managed`。不发明 URL 字段。

## 必须保持绿

`apply_gateway_` 里 18 档的夹具。

## 会改这一档的反馈

这四个 id 写出了环回 URL，或测试开始把厂商主机写进它们的配置。

## 决定

- 出处：`internal/agent/agents.go` 的 goose、cursor、copilot 只写模型名；`internal/agent/devin.go` 写明 Devin 没有可替换的 endpoint。

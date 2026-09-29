# 08 — HTTP 表面

## 契约

04 档的监听补齐 README「网关听什么」里的其余路由。包含图像生成与编辑、Gemini 的 `/v1beta/models`、`/backend-api/codex/`、`GET /api/hello`（`name` 为 `skillstar`）、`GET /v1/models`。WebSocket Upgrade 到 Codex 路径时返回 426。模型 id 含 `/` 的 Codex 请求留在本机，假上游若是 OpenAI 则收不到它。

不注册 `/v1/magpie/quotas`，也不注册 `/v1/skillstar/quotas`。未知路径返回 404，正文点名本网关提供的路径，品牌是 skillstar。

Codex 的压缩提示词 `codexCompactPrompt` 与 `codexSummaryPrefix` 按 magpie 源码逐字复制。压缩标记是 `skillstar1:`。

## 缝

路由都在 `skillstar_gateway::serve` 的那一个 handler 上。翻译夹具继续放在 `tests/fixtures/magpie/`，按协议分子目录。比较规则仍是 README 的允许表。

## 人可以运行

`skillstar gateway serve` 之后，用 01 档同一方式对各路径发一条夹具。`curl` 请求 `/v1/magpie/quotas` 得到 404。

## 验证

- 每个 README 列出的路径有一条夹具测试
- `codex_slash_model_does_not_reach_openai`
- `codex_websocket_is_426`
- `quotas_route_is_absent`
- `hello_name_is_skillstar`
- `compact_prompt_bytes_match_magpie`

本档上游仍是假服务器。订阅签名在 17 档。

## 可改

handler 内部如何分发到各协议函数。

## 不可改

路径清单、426、含 `/` 的模型不落到 OpenAI、没有配额路由、压缩提示词逐字。

## 必须保持绿

`cargo test -p skillstar-gateway`。04 档的 CLI 探针。

## 会改这一档的反馈

某条路径的出站正文差在允许表之外，或配额路径返回 200。

## 决定

- 图像路由本档只证明请求被接受并转给假上游。视觉转述是 16 档。

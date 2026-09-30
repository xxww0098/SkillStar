# 21 — OpenHanako

## 契约

OpenHanako 在跑的时候，经它 `server-info.json` 里的本地 API 更新 provider 和主 Agent 的 chat 模型。没在跑的时候，写它的 catalog 与 agent 配置文件，下次启动时读到。provider 名是 `skillstar`。URL 是网关的 `/v1`。密钥是占位 bearer。

## 缝

`skillstar-gateway`。测试里的「在跑」是一个假 HTTP 服务器，端口写进临时目录的 `server-info.json`。不连接用户真的 Hanako。

## 人可以运行

```bash
cargo test -p skillstar-gateway hanako_
```

## 验证

- `hanako_live_uses_local_api`
- `hanako_stopped_writes_files`
- `hanako_url_is_loopback_v1`

## 可改

假服务器的实现。

## 不可改

活着走 API、没活着写文件、不把厂商密钥写进 catalog。

## 必须保持绿

`apply_gateway_specials_still_unmanaged` 改为 Hanako 已纳入、Alma 与 Cindy 与 WSL 仍未纳入。

## 会改这一档的反馈

进程在跑时仍然只改了文件、没打本地 API，或反过来。

## 决定

- API 的路径以 magpie `hanako.go` 文件头的那两段 PUT 为准。先 `GET /api/server/identity`，Bearer 是 `server-info.json` 的 token，200 且 `serverId` 非空才算在跑。然后 `PUT /api/config` 和 `PUT /api/agents/<id>/config`。PUT 失败不改文件。
- 没在跑时整份写 `provider-catalog.json` 和该 agent 的 `config.yaml`，不逐键拼接。取消托管放回整份文件。不写 `meta.deletedProviders`。`provider-plugins/skillstar` 在写文件时删掉，不放进 stash。
- provider 名是 `skillstar`。`base_url` 是 `{origin}/v1`。`api` 是 `openai-completions`。密钥是 `skillstar-hanako`。`models` 只有这一次的 `model_ref`，`image` 和 `reasoning` 为 false。chat 的 provider 是 `skillstar`，id 是整个 `model_ref`（斜杠留在 id 里）。
- 没有已有 agent 时（`user/preferences.json` 的 `primaryAgent`，否则 `agents/` 里按名字排序后第一个有 `config.yaml` 的）返回错误，一个字节不写。
- `SKILLSTAR_TOOL_SYNC_HOME` 非空时不读 `HANA_HOME`。正式运行读它，`~/` 展开成 home，相对路径也拼到 home。HTTP 只连 `127.0.0.1`，探测 1.5 秒，PUT 15 秒。Hanako 不进 `FILE_AGENTS`。

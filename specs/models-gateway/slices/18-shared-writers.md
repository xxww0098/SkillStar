# 18 — 共享写入

## 契约

一个函数 `apply_gateway(agent_id, model_ref)` 把文件型 Agent 指到环回网关。`model_ref` 是 `provider/model`、`group/<id>` 或空（取消托管）。写出的 URL 是 `http://127.0.0.1:<端口>` 加该 Agent 在 magpie 里使用的后缀。bearer 是 `skillstar` 或 `skillstar-<agent-id>`，与 magpie 用 `Token` 还是 `TokenFor` 的选择一致。

本档的 Agent：Gemini CLI、OpenCode、MiMo Code、Pi、Goose、Cursor CLI、Copilot CLI、Crush、DeepSeek Harness、Command Code、fx、omp、Devin、Hermes、Cline、Qoder、Qoder CN、Grok Build、ZCode、WorkBuddy。

Codex、Claude Code、Claude Desktop、Hanako、Alma、Cindy、WSL 不在本档。它们仍返回 `agent_not_managed`，直到各自的档。

每个 Agent 一份夹具：magpie 该 Agent 测试的期望字节，经 README 允许表归一。文件里不出现厂商密钥和厂商主机。

## 缝

函数在 `skillstar-gateway`。路径与格式只在这里。`skillstar-models` 不增加 writer。`skillstar-app` 的保存命令按 id 调用这一个函数。

测试目录用 `SKILLSTAR_TOOL_SYNC_HOME`。取消托管走 stash。

`docs/features/models/README.md` 的花名册改成「以 gateway 的注册表测试为准」，不手抄一份会漂移的人数。

## 人可以运行

```bash
cargo test -p skillstar-gateway apply_gateway_
```

挑一个临时 HOME 保存 OpenCode，打开配置看到环回 `/v1` 和占位 bearer。

## 验证

- 上表每个 id 一条 `apply_gateway_<id>_matches_fixture`
- `apply_gateway_specials_still_unmanaged`
- `apply_gateway_writes_no_vendor_secret`

某一条对不上时，这一档不算完成。不要为这一条另造格式。若契约本身盖不住，先把该 id 从本档移到新切片，再继续。

## 可改

注册表在 crate 内的文件拆分。

## 不可改

一个函数、环回 URL、占位 bearer、零厂商密钥、特例仍是 `agent_not_managed`。

## 必须保持绿

06 档的 Codex 测试。依赖守卫。

## 会改这一档的反馈

任一夹具在允许表之外失败，或 models 里重新出现这些 Agent 的 writer。

## 决定

- 探测「Agent 是否安装」可以没有。未安装时写入仍按 magpie 的做法：该写的文件就写，测试用临时目录。

# 18 — 共享写入

## 契约

一个函数 `apply_gateway(agent_id, model_ref)` 把文件型 Agent 指到环回网关。`model_ref` 是 `provider/model`、`group/<id>` 或空（取消托管）。写出的 URL 是 `http://127.0.0.1:<端口>` 加该 Agent 在 magpie 里使用的后缀。bearer 是 `skillstar` 或 `skillstar-<agent-id>`，与 magpie 用 `Token` 还是 `TokenFor` 的选择一致。

本档的 Agent：Gemini CLI、OpenCode、MiMo Code、Pi、Crush、DeepSeek Harness、Command Code、fx、omp、Hermes、Cline、Qoder、Qoder CN、Grok Build、ZCode、WorkBuddy。

Codex、Claude Code、Claude Desktop、Hanako、Alma、Cindy、WSL 不在本档。它们仍返回 `agent_not_managed`，直到各自的档。Goose、Cursor CLI、Copilot CLI、Devin 也不在本档：magpie 的保存不写环回 URL，见 [39-name-only.md](39-name-only.md)。

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
- Goose、Cursor CLI、Copilot CLI、Devin 移到 [39-name-only.md](39-name-only.md)。出处：goose 只写 `GOOSE_PROVIDER` / `GOOSE_MODEL`；cursor 只写 `model.modelId`；copilot 只写 `model`；devin 的文件头写明没有可替换的 endpoint。不为它们发明 URL 字段。
- provider 块里的模型只有这次的 `model_ref`。整份 catalog 要读密钥表，这个函数不读。没有目录里的上下文、effort 或图像时就省略；Crush、WorkBuddy、ZCode 的窗口缺省 `200000`，与 magpie 在缺省时写的数相同。
- Pi 的这一条模型不标 `anthropic-messages`。没有目录就不知道原生 API，缺省 `openai-completions`，base URL 带 `/v1`。
- JSON 键序固定。Go 的 map 顺序不是契约。
- 配置目录是 `<SKILLSTAR_TOOL_SYNC_HOME 或 home>/.config`。这个变量设着时不读 `XDG_CONFIG_HOME`，也不读 `GROK_HOME`、`HERMES_HOME`、`DSH_HOME`、`CLINE_DIR`、`QODER_CONFIG_DIR`、`QODERCN_CONFIG_DIR`、`WORKBUDDY_CONFIG_DIR`。变量没设时，这些目录覆盖照常生效。
- Cline 的数据目录是 `<CLINE_DIR 或 ~/.cline>/data`。不读 `CLINE_DATA_DIR`。不写 `updatedAt`，也不新建 `globalState.json` 或 `secrets.json`。
- Pi 已有的 `enabledModels` 不保留在当次写入里。取消托管才把接管前的整份文件放回。omp 不把已有的 `models.json` 转成 YAML，直接写 `models.yml`。
- 取消托管把接管前的文件整份从 `agent_stash.json` 放回。fx、omp、Command Code 与 magpie 一样不写 bearer 字符串：fx 和 omp 是 `auth: none`，Command Code 的 `apiKey` 是 `false`。
- URL 后缀：Gemini 为空；ZCode 的 Anthropic `baseUrl` 为空；WorkBuddy 为 `/v1/chat/completions`；其余为 `/v1`。Qoder、Qoder CN、WorkBuddy 的 bearer 是 `skillstar-<id>`。DeepSeek Harness 没有 profile 时写旧版 `config.yaml`，标记是 `# skillstar`，`provider` 名仍是它自己的 `deepseek-official`。

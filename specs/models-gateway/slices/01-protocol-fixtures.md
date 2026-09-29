# 01 — 协议夹具

## 契约

给定一份入站正文和协议，得到将要发给上游的正文。本档覆盖两件事：Chat Completions 原样穿过；一条 Anthropic Messages 请求译成 Chat Completions，并且带一次工具调用。没有套接字，没有进程，没有密钥。

## 缝

新建 `crates/skillstar-gateway`。包头与 `skillstar-decision` 的 Cargo.toml 同一套：`version` / `edition` / `rust-version` / `license` 用 workspace，`publish = false`，`doctest = false`，`[lints] workspace = true`。

`skillstar-core` 用 `cargo add -p skillstar-gateway skillstar-core` 加入，写成 `path = "../skillstar-core"`。本档代码可以还不用到它。manifest 里其他 skillstar crate 一个都不出现。

翻译函数放在 crate 内，测试从 crate 根调用。夹具目录：

`crates/skillstar-gateway/tests/fixtures/magpie/translate/`

每组四个文件：`inbound.json`、`upstream_request.json`、`upstream_response.json`、`outbound.json`，外加 `allow.json` 列出 README 允许的字节差异。比较前只按那张允许表归一品牌。CI 不编译 magpie。

`scripts/internal/check_workspace_deps.sh` 增加禁止边：`models → gateway`、`usage → gateway`、`gateway → models`、`gateway → usage`、`gateway → decision`、`gateway → app`、`core → gateway`、`models → decision`。并断言 `skillstar-gateway` 的 skillstar 依赖集合等于 `{skillstar-core}`。

`docs/boundaries.md` 加上这个 crate 的一行（拥有网关与 Agent 配置写入；不拥有密钥表、Usage、决策模型），写明删除测试，mermaid 先不要画 `app → gateway`（那条边在 04 档才存在）。`docs/decisions.md` 追加下一条：用户要求网关独立成 crate，依赖只到 core。

## 人可以运行

```bash
cargo test -p skillstar-gateway translate_
```

失败时打印归一后的差异，而不是只给一条 `assert`。

## 验证

- `translate_chat_passthrough_matches_fixture`
- `translate_anthropic_tool_call_matches_fixture`
- `gateway_manifest_depends_only_on_skillstar_core`
- 更新后的 `bash scripts/internal/check_workspace_deps.sh`

夹具的上游形状从 magpie 的翻译测试抄预期字节，再按允许表改品牌。本档不翻译 Responses、Gemini 或图像。

## 可改

模块文件名、函数名、夹具分组的目录名。

## 不可改

crate 名、依赖方向、两条夹具的语义、允许差异表、本档不监听端口。

## 必须保持绿

`cargo check -p skillstar-models --locked` 与 `cargo check -p skillstar-usage --locked` 在不编译 gateway 的前提下通过。`cargo test -p skillstar-gateway translate_`。

## 会改这一档的反馈

Anthropic 工具调用的上游字段和 magpie 夹具对不上，而且对不上的地方不在允许表里。

## 决定

- crate 在本档诞生，不拖到有监听之后。
- 密钥与账户类型本档不定义。03 档再引入注入用的快照。

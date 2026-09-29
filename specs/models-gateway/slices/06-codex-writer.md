# 06 — Codex 写入

## 契约

保存 Codex 时只写环回网关，不写厂商密钥。两种形态与 magpie 相同：

- 已登录：`openai_base_url` 指向 `http://127.0.0.1:<端口>/backend-api/codex`。
- API 形态：`[model_providers.skillstar]`，`base_url` 是 `http://127.0.0.1:<端口>/v1`，`wire_api = "responses"`，`experimental_bearer_token = "skillstar"`，目录文件 `skillstar-models.json`。

表留下不删，和 magpie 一样：已经用这张表开过的线程还要找得到它。未纳入本档的 Agent 保存时返回 `agent_not_managed`，磁盘字节数不变。

接管某个用户字段之前，旧值写入 `agent_stash.json`（`0600`，原子替换）。取消托管时按 stash 写回。

## 缝

写入函数在 `skillstar-gateway`。`skillstar-app` 的保存命令调用它。`skillstar-models` 不依赖 gateway，因此不能再留一套会写厂商 URL 或密钥的 writer。本档删除 `tool_sync` 里六个 Agent（claude-code、claude-desktop、codex、opencode、pi、omp）写出厂商 base URL 与 API key 的路径，包括 Desktop 的 `skillstar-binding.json` marker。

`load_store_and_repair` 不再调用 `repair_agent_configs`。provider 保存不再调用 `resync_active_tools`。这两条若没有别的调用者，函数一起删掉。启动和保存密钥都不改 Agent 文件。

测试设置 `SKILLSTAR_TOOL_SYNC_HOME`、`SKILLSTAR_DATA_DIR`、`HOME` 到临时目录。

`docs/features/models/README.md` 改写 tool sync：托管模型配置的所有者改为 `skillstar-gateway`，旧的直写厂商密钥不再是当前行为。`docs/decisions.md` 追加硬切换：不迁移、启动时不改写 Agent 文件、v4 不加路由字段。`docs/architecture.md` 写上 `model_gateway.json`、`agent_stash.json` 与密钥表的分界。

## 人可以运行

在临时 `HOME` 里放一份最小 `~/.codex/config.toml`，调用 app 的 Codex 保存，检查文件里只有环回 URL 和占位 bearer。再保存一个尚未实现的 Agent，确认文件修改时间不变。

## 验证

- `codex_logged_in_writes_backend_api_url`
- `codex_api_mode_writes_skillstar_provider_table`
- `codex_never_writes_vendor_key`
- `unmanaged_agent_writes_zero_bytes`
- `startup_and_provider_save_do_not_touch_agent_files`
- `stash_roundtrip`
- 全仓库搜索不再有把厂商密钥写进这六个 Agent 配置的路径。`wire_api = "chat"` 仍然不出现。

## 可改

stash 记录的内部字段顺序。

## 不可改

两种 Codex 形态、占位 bearer、启动时不改文件、models 不依赖 gateway、`agent_not_managed` 的零写入。

## 必须保持绿

`cargo test -p skillstar-gateway codex_writer_`。`cargo test -p skillstar-models` 里仍存在的 tool-sync 测试改为断言「不再写这些文件」或随函数删除一起删。`bash scripts/internal/check_workspace_deps.sh`。

## 会改这一档的反馈

保存后的 toml 里出现厂商主机名或真密钥，或启动测试的临时目录里 Agent 文件被改写。

## 决定

- 已有 Agent 文件里的旧厂商 URL 保持不动，直到这一次保存。保存只改 Codex 这一档声明的键。
- v4 `model_providers.json` 的 `version` 仍是 4，不加列。

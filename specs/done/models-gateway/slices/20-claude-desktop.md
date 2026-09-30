# 20 — Claude Desktop

## 契约

Claude Desktop 写入原生 `claude_desktop_config.json` 和 Claude-3p 配置档。配置档 id 是 `00000000-0000-4000-8000-736b696c6c73`。别名是 `anthropic/skillstar-` 加 FNV-1a 64 位对 `1e10` 取模、补零到 10 位的数。effort 别名前缀是 `mythos-skillstar-`。`inferenceGatewayBaseUrl` 是网关根，`inferenceGatewayApiKey` 是 `skillstar-claude-desktop`。

不写 `skillstar-binding.json`。

## 缝

writer 在 `skillstar-gateway`。路径规则抄 magpie `desktopDirs`：macOS 的 Application Support、Windows 的 LOCALAPPDATA、其余的 XDG。测试在临时 HOME 上跑全部三个 `goos` 的路径函数，不碰真实家目录。

`docs/features/models/README.md` 删掉「Desktop 只是 marker」作为当前行为的说法。

## 人可以运行

临时 HOME 保存 Desktop，确认两份 `claude_desktop_config.json` 和配置档文件。确认没有 `skillstar-binding.json`。

## 验证

- `desktop_profile_id`
- `desktop_alias_fnv64a`
- `desktop_writes_native_config`
- `desktop_does_not_write_marker`
- `desktop_paths_per_os`

## 可改

测试里构造 HOME 的辅助函数。

## 不可改

配置档 id、FNV 算法、别名前缀、marker 文件不再出现。

## 必须保持绿

06 档「启动不改 Agent 文件」。

## 会改这一档的反馈

id 写成了 magpie 的 `6d6167706965`，或 marker 文件又被写出来。

## 决定

- `/v1/models` 给 Desktop 的 id 过滤规则抄 `desktopAccepts` 与 `desktopDenied`。词表不自行增删。别名后的 id 必须能通过这个检查。过滤是纯函数，不挂进现在返回空列表的 `/v1/models`。等目录进网关，再按 Desktop 的 User-Agent 过滤。
- 两份 `claude_desktop_config.json`、配置档和 `_meta.json` 整份替换，取消托管放回接管前的整份文件。不移植 magpie 的逐键拼接。托管期间原文件里的其它键不留着。空目录不删。
- `_meta.json` 的 `appliedId` 是配置档 id，`entries` 里这一条的 `name` 是 `skillstar`。配置档还写 `inferenceProvider` 为 `gateway`、`inferenceGatewayAuthScheme` 为 `bearer`、`disableDeploymentModeChooser` 为 true、`coworkEgressAllowedHosts` 为 `["*"]`。两份 config 写 `deploymentMode` 为 `3p`。`model_ref` 不写进这些文件。
- effort 的 Claude 形态（`skillstar-<number>.anthropic.<model>`）本档不生成。`apply_gateway` 没有 efforts，也没有目录。`mythos-skillstar-` 只由别名函数给出。
- `SKILLSTAR_TOOL_SYNC_HOME` 非空时，写者不读 `XDG_CONFIG_HOME` 和 `LOCALAPPDATA`，落在该 home 下的系统默认位置。路径函数本身仍接受调用方传入的 getenv，测试用它覆盖 darwin、windows 和其余系统。不写 `%APPDATA%\Claude`，也不写 `skillstar-binding.json`。

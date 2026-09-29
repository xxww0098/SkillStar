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

- `/v1/models` 给 Desktop 的 id 过滤规则抄 `desktopAccepts` 与 `desktopDenied`。词表不自行增删。别名后的 id 必须能通过这个检查。

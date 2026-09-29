# 33 — 配置档

## 契约

一份命名配置档记下若干 Agent 当前选中的 `model_ref`。应用它时，对每个已实现的 Agent 调用现有 writer。未实现的 id 跳过并出现在结果里，不写文件。配置档不是技能库，不同步 MCP，不备份到 WebDAV。

## 缝

配置档存在 `model_gateway.json` 的 profiles 数组，所有者 `skillstar-gateway`。`skillstar-app` 只转发。

## 人可以运行

保存两份档，切换，检查两个 Agent 文件里的模型 id 跟着变。stash 仍能在取消托管时恢复切换前的用户原值。

## 视觉

变量只有一个：配置档的名字列表。

裁剪：名字列表。不评应用后的 Agent 行。

screenshot-critique。无参照则不做 compare-screenshots，记入 `choices.md`。视口两档。preview-shots 约 5 分钟。

验收句：列表是档的名字；应用结果不在这张裁剪里；没有密钥和厂商 URL。

## 验证

- `profile_apply_calls_existing_writers`
- `profile_skips_unmanaged_without_write`
- `profile_is_not_a_library_sync`

## 可改

名字在界面上如何截断。长度上限是 64 个 Unicode 标量，超出则拒绝保存。

## 不可改

应用必须走已有 writer、不包含库或 WebDAV。

## 必须保持绿

18 与 06 的写入测试。

## 会改这一档的反馈

应用配置档时出现了第二条写文件实现。

## 决定

- 档里只存 Agent id 和 model ref。不存密钥，不存 URL。

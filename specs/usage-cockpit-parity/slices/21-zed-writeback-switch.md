# 21 · zed — 切号写回（L4，macOS-only）

> 依赖：20 + 04 + 07。

## 解锁的契约

macOS 上切号写回 `https://zed.dev` internet-password 条目；非 macOS `available()=false`
→ UI 不出切号入口（能力诚实的范例）。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/zed.rs` | adapter impl：`security add-internet-password -s https://zed.dev -a <user_id> -w <token> -U` + 回读校验；`forget` 不删条目（保守——只解除登记）；非 macOS `available()=false` |
| app | `usage_switch.rs` | 注册 zed |

## 人能看见

macOS 上 Zed 卡切号 + 三态 badge；真机 Zed 重启换号（人工记录）。非 macOS 无入口。

## 验证

- keychain_cli internet-password 写→读→删回环（测试 service，非真条目）。
- 沙箱下 keychain 整体关闭的断言。
- 人工 smoke 记录。

## 委托给实现者的决定

- 无。

## 必须保持绿

- keychain 写是 read-modify-write 风险点：只动 `-s https://zed.dev` 自己的条目（anthropic 教训）。
- zed 永不进 `DesktopAppId`（D10）。

## 会改变本片的人类反馈

- 无。

## 结果

`zed` 走 `IdeCredentialAdapter`，注册在 `usage_switch/ide.rs`。实现在 `usage_switch/zed.rs`（没有 `ide/` 子目录，和 Kiro / Trae 同一层）。没有进 `DesktopAppId`。`supports_switch("zed")` 为 true。非 macOS，或设置了 `SKILLSTAR_TOOL_SYNC_HOME` 时 `available()` 为 false，`reconcile` 返回 `None`（目录里没有这项），不是假的 `LinkedTo`。

macOS 且未沙箱时，切号只动 server `https://zed.dev` 的 internet-password。account 是订阅的 `oauth_account_id`（user id），secret 是 access token。`activate` 用 `add-internet-password -U` 写入，再用 `find-internet-password` 回读；账号和 token 都一致才 pin。写入失败或回读不一致不移动 pin，也不删除其它 server。`sync` 把刷新后的 token 写回同一条，不改 pin。`reconcile` 用钥匙串里该 server 的条目对比当前 pin：一致 `LinkedTo`，条目在但不一致 `Diverged`，没有条目 `Missing`。`forget` 删除这一条账号（`-a <user_id> -s https://zed.dev`），不走 `keychain_cli::delete_internet_password` 那种按 server 循环删光，也不动其它账号或其它 server。切片正文里的「forget 不删条目」没有采用。

测试不调用 `/usr/bin/security`。行为走适配器旁边的 `CommandRunner`：假 runner 记录 argv 并返回脚本化的 stdout/stderr。真正的 `SecurityCli` 只在沙箱测试里调用，`SKILLSTAR_TOOL_SYNC_HOME` 在 `Command::new` 之前返回。非 macOS 的 `available()` 由纯函数覆盖，不依赖当前主机。

真机钥匙串写入：**未验证**。没有让 `security` 接受过一次写，也不能说 Zed 重启后会换号。

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

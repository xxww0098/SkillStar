# 11 · windsurf — 切号写回（L4）

> 依赖：10（凭据形态）+ 04（adapter 注册表）+ 05（若写 `secret://` 键）。
> **这是第一个新 IdeCredentialAdapter 实现——它的形状就是 qoder/codebuddy 的模板。**

## 解锁的契约

Usage 卡片切号把账号写进 `…/Windsurf/User/globalStorage/state.vscdb`，重启 Windsurf 换号生效；
`reconcile` 三态正确（LinkedTo/Diverged/Missing）。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/windsurf.rs`（新） | `IdeCredentialAdapter` impl：`activate` 写 `windsurfAuthStatus`（含 `userStatusProtoBinaryBase64`）+ `windsurf_auth-*` session 键（safe-storage `v10` 值走 `tool_store::safe_storage`）+ `windsurf.apiServerUrl`；事务内写，回读校验后落 pin；`reconcile` 按 refresh_token/auth1 内容比对；`sync` 投影 refresh 轮换；`forget` 清键不删库 |
| app | `usage_switch.rs` | 注册表加 windsurf adapter |

## 写回纪律（继承 cursor/antigravity 先例）

- catalog 锁内；备份 → 写 → 回读 → 最后落 pin；任何一步失败 pin 不动。
- **只动 auth 键**——`windsurf_auth-*` 里的本地 usage 缓存键不许清（provider-reference 已记）。

## 人能看见

Windsurf 卡出现切号按钮；切号后 badge 三态；真机 Windsurf 重启识别新账号（人工记录）。

## 验证

- 临时 vscdb 写→回读→校验回环；保留无关行。
- reconcile 三态测试（含 CLI 自改文件后 Diverged）。
- 人工 smoke：官方 Windsurf 重启后读到目标账号（记录写进 PR）。

## 委托给实现者的决定

- session 键的完整 key 清单（对齐 cockpit `windsurf_account.rs` 的写回集合）。

## 必须保持绿

- `SKILLSTAR_TOOL_SYNC_HOME` 沙箱下不碰真实 Windsurf 目录。

## 会改变本片的人类反馈

- 若 safe-storage spike（05）结论为「macOS 可写/其它平台不可写」，非 macOS 的写回键集降级为明文键子集 + UI 文案说明。

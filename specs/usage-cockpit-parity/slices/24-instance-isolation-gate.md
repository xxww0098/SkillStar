# 24 · 多开实例注册 — 逐 app 隔离验证门

> 依赖：全部 (b) 片落地后。**本片的主体是人工验证协议，不是代码**——
> 代码改动是参数表扩展，真正门槛是逐 app 的隔离实证。

## 解锁的契约

`DesktopAppId`/`LaunchSpec`/`desktopApps.ts` 扩展，但**只注册隔离实证的 app**；
每个 app 一份 `instance_capability` 报告（Verified/Blocked）。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `instances/apps.rs` | 新变体 + `LaunchSpec`（`macos_app_name`、`user_data_dir_form` Separate/Equals、extra args）+ `catalog_id()` 映射 + `parse`；每个候选带能力标志，默认 Pending，验证通过翻 Verified |
| app | `instances/launch.rs`（如需） | zcode 若需 env 注入（`ZCODE_DATA_BASE_DIR` 等而非 `--user-data-dir`），加 `EnvSpawn` 变体；`pids_for_user_data_dir` 的关联方式适配 |
| 前端 | `lib/desktopApps.ts` | `INSTANCE_CATALOG_IDS`/`desktopAppIdForCatalog`/`desktopAppsForFilter` 按 Verified 集填充 |

## 候选判定（预登记，最终以验证为准）

| app | 预期 | 说明 |
| --- | --- | --- |
| Windsurf / Kiro / Qoder / CodeBuddy / CodeBuddy CN / ZCode | 大概率可交付 | Electron/Chromium，`--user-data-dir` 标准机制（zcode 可能走 env） |
| Trae / TRAE SOLO / Trae CN / TRAE SOLO CN | 大概率可交付 | 4 个独立 bundle 各验 |
| Zed | **Blocked（结构性）** | 原生 app 无 `--user-data-dir` + 全局 keychain；不进注册表，理由写 `UnsupportedApp` 分支（Claude Desktop 先例） |
| github-copilot | **不做** | 无独立 app；VS Code `--user-data-dir` profile 是另一机制（可选增强：注入 `secret://github.*` 进实例 vscdb——依赖 05，人类拍板才做） |

## 每 app 验收清单（全过才算 Verified）

1. `open -n -a <App>.app --args --user-data-dir[=]<dir>`（**逐个验证空格 vs `=` 形式**——Antigravity 教训：空格形式会被丢参数）。
2. 进程存活且实例目录内出现独立 `User/globalStorage/state.vscdb`。
3. 与默认 profile 登录态互不影响。
4. Stop 按 `user-data-dir` argv 匹配只杀该实例（多实例同 app 名时关键）。
5.（若该 app 有 adapter）`inject_into_root` 或切号到实例 profile 后，实例内读到目标账号。

## 人能看见

Usage 页「桌面应用」区出现验证过的 app；创建/启动/停止实例真实生效；
Blocked 的 app 不出入口。

## 验证

- `apps.rs` parse/argv 单测（含两种 user-data-dir 形式）。
- 每 app 的人工验证记录（命令输出+截图贴 PR/本文件「结果」节）。
- `bun run types:gen`（DesktopAppId 是 ts-rs 导出）。

## 委托给实现者的决定

- zcode `EnvSpawn` 的具体 env 集（对齐 cockpit `zcode_instance.rs`）。

## 必须保持绿

- 既有 Cursor/GrokBot/Antigravity 实例行为回归。

## 会改变本片的人类反馈

- 是否投入做 VS Code profile 注入（copilot 多开的唯一路径）。

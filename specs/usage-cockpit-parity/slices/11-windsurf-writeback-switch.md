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

## 结果

适配器在 `crates/skillstar-app/src/usage_switch/windsurf.rs`，注册在 `usage_switch/ide.rs` 里，跟 Antigravity、Cursor 并列。没有再套 `ide/` 目录（切片 04 已经这么放）。`supports_switch("windsurf")` 为 true。`oauth_completion_rewrites_live_store("windsurf")` 因为 IDE adapter 存在而为 true；没有把 codex / opencode 拉进来。

官方 Windsurf 重启：**未验证**。没有对真机写过，也不能说官方 app 接受了这次写回。不因此把 `supports_switch` 设成 false。

### 写入的键（只动 auth）

事务里一次 upsert/delete。回读不一致则把 rolling backup 拷回去，pin 不动。

| 键 | 内容 |
| --- | --- |
| `windsurfAuthStatus` | 明文 JSON。有才写：`apiKey`、`apiServerUrl`、`email`、`name`、`authToken`、`refreshToken`、`auth1Token`、`userStatusProtoBinaryBase64`。有 `auth1Token` 时加 `authMethod: "auth1"` |
| `secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.sessions"}` | Safe Storage 密文。明文是一条 session：`accessToken` 优先 apiKey，否则 authToken，否则 auth1；`account.label/id` 是邮箱，否则名字，否则 `windsurf_user` |
| `secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.apiServerUrl"}` | 有 `apiServerUrl` 才加密写入 |
| `codeium.windsurf-windsurf_auth` | 上面的账号 label |
| `codeium.windsurf` | 在原对象上合并 `apiServerUrl`，其它字段保留 |
| `windsurf.apiServerUrl` | 有 URL 才写的明文键（cockpit 写回集合里没有这一条，规格单独要求） |

不写、不删 `windsurf_auth-*`。测试里的 `windsurf_auth-ada-usages` 和无关行在切号、forget 之后都还在。

`forget` 清上面的 auth 键，并从 `codeium.windsurf` 去掉 `apiServerUrl`（`installationId` 这类字段留下）。不删库文件。只有 live 的 refresh / auth1 / apiKey / authToken 对得上这张卡才清；删另一张卡不会把 IDE 登出。对不上或库不存在则不动文件。

`reconcile`：没有可用凭据是 `Missing`（路径解析得到但文件不存在也是 `Missing`，不是缺 adapter）。对得上某张订阅是 `LinkedTo`（文件赢，不必是 pin）。有凭据但没有订阅对上是 `Diverged`，包括 pin 之下把 auth 材料改掉、邮箱还在的情况。比对不看邮箱。

`sync` 把刷新后的 refresh / auth1 投影回 live db，不改 pin。

### 测试

`cargo test -p skillstar-app --locked --lib -- usage_switch`：58 passed。含 8 个新的 Windsurf 测试，`custody_tests` 仍绿。临时 sqlite，`SKILLSTAR_TOOL_SYNC_HOME` / `SKILLSTAR_DATA_DIR` 沙箱，口令 `injected-password`。没有碰真实 Windsurf 目录，也没有调用钥匙串。

覆盖：写→回读、无关行和 usage 缓存保留、`codeium.windsurf` 里原有 `installationId` 保留、两账号切换、Missing / LinkedTo / Diverged、forget 只清 auth、无库或无口令时 pin 不动、回读失败恢复备份且 pin 不动、sync 投影、cockpit `{"type":"Buffer","data":[...]}` 会话在 auth status 没有 apiKey 时仍能 LinkedTo。

### 静默决定

- Safe Storage 口令只认 `SKILLSTAR_WINDSURF_SAFE_STORAGE_PASSWORD`。不读 macOS 钥匙串、Linux secret-tool 或 Windows DPAPI。没注入口令时，需要写 `secret://` 的切换直接失败，pin 不动。
- 密文算法跟宿主走：macOS `KeyMaterial::macos_v10`（PBKDF2 1003），Linux `linux_v10`，Windows 把口令 SHA-256 成 32 字节 `OsCryptKey`（本构建没有 DPAPI，这不是 Windsurf.exe 的密钥）。写入值是 `encrypt_secret` 的标准 base64，不是 cockpit 的 Buffer JSON。读的时候两种都认。
- 订阅没有 `apiServerUrl` 时不发明 `https://server.codeium.com`。
- `codeium.windsurf` 若不是 JSON 对象，写的时候换成只含 `apiServerUrl` 的对象。
- 裸的 provider state（纯 `sk-ws-` / `auth1_` 字符串）仍当成凭据，和导入一致。

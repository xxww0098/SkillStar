# 13 · kiro — 切号写回（L4）

> 依赖：12 + 04。**风险先行：Kiro 的 `~/.aws/sso/cache/kiro-auth-token.json` 是全局共享文件**
> ——写回前先做内嵌 kill 实验（见下「隔离检查」），结论决定本片与实例级能力。

## 解锁的契约

切号写回 `~/.aws/sso/cache/kiro-auth-token.json` + IDC 注册缓存 + Kiro `state.vscdb`
（`kiro.kiroAgent` 等键）；reconcile 按 token 内容比对。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/kiro.rs`（新） | adapter impl：文件写回（JSON 原子写+回读）+ vscdb 键写回；`available()`=任一存储存在 |
| app | `usage_switch.rs` | 注册 kiro |

## 内嵌 kill 实验（本片第一个任务）

1. 读真实 `~/.aws/sso/cache/` 结构，确认 kiro-auth-token.json 是否全局单文件。
2. 若是全局共享：写回会踢掉 AWS CLI/其它工具的登录态——**降级方案**：切号前明确警告文案
   「会覆盖本机 AWS SSO 登录态」，或本片降级为不写 `~/.aws`（只写 state.vscdb）。
   结论写本文件「结果」节；若证实共享冲突，kiro `instance_capability=Blocked`（切片 24 登记）。

## 人能看见

Kiro 卡切号 + 三态 badge；若有共享冲突降级，卡片文案说明「仅监控/不切号」。

## 验证

- 临时 `.aws` fixture 写回+回读；vscdb 键写回。
- 共享文件备份/回滚测试（写前备份，失败还原）。
- 人工 smoke：Kiro IDE 重启识别新账号（记录）。

## 委托给实现者的决定

- 共享文件冲突时的 UI 文案与是否加确认弹窗。

## 必须保持绿

- 不动 AWS CLI 非 kiro 键；沙箱测试。

## 会改变本片的人类反馈

- 共享 `~/.aws` 写回的取舍（写+警告 vs 不写）值得人类拍板。

## 结果

`kiro` 走 `IdeCredentialAdapter`，注册在 `usage_switch/ide.rs`，和 Antigravity、Cursor、Windsurf 并列。实现在 `usage_switch/kiro.rs`，磁盘读写在 `usage_switch/kiro_store.rs`（单文件会超过行数上限）。没有 `ide/` 子目录，也没有把 Kiro 放进实例注册表。`supports_switch("kiro")` 为 true。

官方 Kiro 重启：**未验证**。测试只写临时目录，没有对真机写过，不能说官方 app 接受了这次写回。

### 隔离检查

没有读本机真实 `~/.aws` 或 `~/.kiro`。路径契约已经说明这是全局单文件：`tool_paths::aws_sso_cache_dir()` 在每个系统上都是 `~/.aws/sso/cache`，Kiro 登录态固定叫 `kiro-auth-token.json`。IdC 注册文件名是 start URL 的 SHA-1（Builder ID 为 `cc18142e2bfa693e309f59d910dcef90c3c47767.json`），和 AWS CLI 旧版 SSO cache 的算法可能撞同一个名字。

这是共享文件冲突。本片仍对这一份 live store 做 L4 切号（备份、回读失败则还原），不做多实例，也不在这里写 `instance_capability`。切片 24 应把 Kiro 记为 `Blocked`。没有加确认弹窗，也没有改 i18n：警告文案留给人类拍板。

### 写回

对齐 src-tauri 的 `write_local_auth_token_file`（不是 cockpit-core 那份 `fs::write`，那份不写注册缓存、也不把 secret 从 token 文件里拿掉）。

| 位置 | 内容 |
| --- | --- |
| `~/.aws/sso/cache/kiro-auth-token.json` | 原子写。有才写 access / refresh / expiresAt / profileArn / email / userId。IdC 写 `authMethod=IdC`、`clientId`、`clientIdHash`，**不写** `clientSecret` |
| `~/.aws/sso/cache/<sha1(startUrl)>.json` | 仅 IdC。`clientId` + `clientSecret`。同一 client 重写时保留原文件里的其它字段（例如 `expiresAt`）；换 client 则整份替换 |
| `…/Kiro/User/globalStorage/kiro.kiroagent/profile.json` | cockpit `inject_account_to_profile` 会写。邮箱、userId、arn |
| `state.vscdb` 的 `kiro.kiroAgent` | 库已经存在才写。内容是 `userInfo` + `profileArn`。不新建库，不动其它键 |

门户登录（没有 client id/secret）不写注册文件，`authMethod` 在 provider 是 Github/Google 时为 `social`。

`activate`：先备份要动的文件，再写，回读不一致则还原，**只有回读通过才 pin**。失败不挪 pin。`sync` 把刷新后的 access/refresh 投回 live，不改 pin。`reconcile`：没有 token 是 `Missing`；refresh 或 access 对上，否则 user id 或 profileArn 对上，是 `LinkedTo` 并吸收轮换；对不上是 `Diverged`。不拿邮箱当身份。`forget` 只在 live 对得上这张卡时删 `kiro-auth-token.json`、client 对得上的那一个注册文件、对得上的 `profile.json`，并清 `kiro.kiroAgent`。不删其它 AWS cache 文件，不删库。`clientIdHash` 不是 40 位小写 hex 时不当路径用。

`available()` 表示 Kiro 数据目录解析得到，不是文件已经存在。没有登录时 reconcile 仍是 `Missing`。

### 测试

`cargo test -p skillstar-app --locked --lib -- usage_switch::kiro`：10 passed。另有注册表测试 `supports_switch_follows_the_registries` 和 `forgetting_an_ide_card_does_not_need_a_live_store`。沙箱是 `SKILLSTAR_TOOL_SYNC_HOME` / `SKILLSTAR_DATA_DIR`，断言路径不离开临时目录、也不等于真实 home。

覆盖：IdC 写 token + 注册缓存 + profile + vscdb 并保留无关 cache 行和无关 ItemTable 行、两账号切换不删另一个 start URL 的注册文件、门户登录不写注册文件也不新建 vscdb、Missing / LinkedTo / Diverged、按 user id 吸收轮换、别人的 live session 不被 adopt、forget 不删 `unrelated.json` 也不跟着 `../` hash 逃出 cache、缺凭据不建文件且 pin 不动、回读失败还原备份且 pin 不动、sync 只投影当前 pin。

### 静默决定

- 没有确认弹窗。共享文件仍会写，失败会回滚。文案不在这片。
- 订阅里没有 cockpit 的 `kiro_usage_raw`，所以 `kiro.kiroAgent` 只写身份，不写配额快照。
- 切号不删除上一个 start URL 的注册文件。forget 只删当前 live 且 client 对得上的那一份。

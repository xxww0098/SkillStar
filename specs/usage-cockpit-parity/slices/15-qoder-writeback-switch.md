# 15 · qoder — 切号写回（L4）

> 依赖：14 + 04 + 05（`secret://` 键必需 safe-storage）。

## 解锁的契约

切号写回 Qoder `state.vscdb` 的 `secret://aicoding.auth.{userInfo,userPlan,creditUsage}`
（safe-storage 加密值）；reconcile 比 `userInfo.email`。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/qoder.rs` | adapter impl：vscdb 多候选路径定位（`ensure_state_db_path_for_user_data_dir` 语义——目标库不存在时复制默认库再写）；safe-storage 加密写三键；回读解密校验 |
| app | `usage_switch.rs` | 注册 qoder |

## 人能看见

Qoder 卡切号 + 三态 badge；真机 Qoder 重启换号（人工记录）。

## 验证

- safe-storage 加密键写回回环（注入 key 材料，沙箱 vscdb）。
- 默认库复制 fallback 路径测试。
- 人工 smoke 记录。

## 委托给实现者的决定

- 写错 key 名静默无效的兜底：reconcile 判 Diverged 已覆盖，不再加运行时探测。

## 必须保持绿

- vscdb 写只动 `aicoding.auth.*` 三键。

## 会改变本片的人类反馈

- 若 05 结论为某平台 safe-storage 不可写 → qoder 该平台锁 L3。

## 结果

适配器在 `crates/skillstar-app/src/usage_switch/qoder.rs`，注册在 `usage_switch/ide.rs`，跟 Antigravity、Cursor、Windsurf、Kiro 并列。没有再套 `ide/` 目录（切片 04 已经这么放）。`supports_switch("qoder")` 为 true。`oauth_completion_rewrites_live_store("qoder")` 因为 IDE adapter 存在而为 true。

官方 Qoder 重启：**未验证**。没有对真机写过，也不能说官方 app 接受了这次写回。不因此把 `supports_switch` 设成 false。

### 写入的键（只动 auth）

路径只认 `tool_paths::qoder_state_db_path()`：多候选里第一个已存在的库。事务里 upsert/delete。回读不一致则把 rolling backup 拷回去，pin 不动。

| 键 | 内容 |
| --- | --- |
| `secret://aicoding.auth.userInfo` | Safe Storage 密文。明文 JSON：`token`、有才写的 `refreshToken`、`id`（`oauth_account_id`，且不是邮箱、也不等于 token）、`email`、有才写的 `name`。`provider_state` 里的 `machineToken` / `machineId` / `machineType` / `machineCode` / `hostname` / `os` / `cosy_version` 原样抄进这份 JSON |
| `secret://aicoding.auth.userPlan` | 有 `plan_tier` 时写 `{"plan","tier"}`（两字段同一字符串）。没有则 `{}`。同一邮箱且库里已经有套餐原文、订阅又没有 `plan_tier` 时不覆盖 |
| `secret://aicoding.auth.creditUsage` | 订阅不存额度原文。换号写成 `{}`。同一邮箱且库里已经有额度原文时不覆盖，避免每次 sync 把 IDE 自己的额度清掉 |

换号时删掉明文 `aicoding.auth.userInfo` / `userPlan` / `creditUsage`。本机导入明文优先，留下旧明文等于没切。同一邮箱的 sync 若保留套餐或额度，对应的明文行也不动。`forget` 把这三对（明文 + `secret://`）都清掉。不删库文件。只有 live 的 `userInfo.email`（忽略大小写）对得上这张卡才清；删另一张卡不会把 IDE 登出。对不上、密文解不开、或库不存在则不动文件。

无关行留下。测试里的 `unrelated` 和 `qoder.sidebar` 在切号、forget 之后都还在。

`reconcile` 比 `userInfo.email`（忽略大小写）。明文键优先于 `secret://`，和导入一致。没有库、或没有 userInfo、或解开后既没有邮箱也没有 token：`Missing`（路径能解析但文件不存在也是 `Missing`，不是缺 adapter）。邮箱对上某张订阅是 `LinkedTo`（文件赢，不必是 pin）；token / refresh 若变了就吸回订阅，pin 不动。有邮箱但对不上、只有 token 没有邮箱、或密文解不开：`Diverged`。

`sync` 把刷新后的 token 投影回 live db，不改 pin。

### 测试

`cargo test -p skillstar-app --locked --lib -- usage_switch::qoder`：9 passed。临时 sqlite，`SKILLSTAR_TOOL_SYNC_HOME` / `SKILLSTAR_DATA_DIR` 沙箱，口令 `injected-password`。没有碰真实 Qoder 目录，也没有调用钥匙串。

覆盖：写→回读解密、无关行保留、两账号切换、明文键被清掉、多候选路径写到已存在的库且不创建首选路径、Missing / LinkedTo / Diverged、邮箱大小写、明文优先于 secret、token 变化吸回订阅、forget 只清 auth、无库或无口令时 pin 不动且不建库、回读失败恢复备份且 pin 不动、sync 换 token 但留下已有额度密文、cockpit `{"type":"Buffer","data":[...]}` 按邮箱 LinkedTo、口令错误是 Diverged 不是 Missing。

### 静默决定

- Safe Storage 口令只认 `SKILLSTAR_QODER_SAFE_STORAGE_PASSWORD`。不读 macOS 钥匙串、Linux secret-tool 或 Windows DPAPI。没注入口令时切换直接失败，pin 不动。
- 密文算法跟宿主走，和 Windsurf 写回相同：macOS `KeyMaterial::macos_v10`（PBKDF2 1003），Linux `linux_v10`，Windows 把口令 SHA-256 成 32 字节 `OsCryptKey`（本构建没有 DPAPI，这不是 Qoder.exe 的密钥）。写入值是 `encrypt_secret` 的标准 base64，不是 cockpit 的 Buffer JSON。读的时候两种都认。
- 没有第二套 user-data 根，所以不做切片里的「目标库不存在时复制默认库」。`qoder_state_db_path`（切片 03）已经是「先存在者优先，不建、不复制」。库文件不存在就切换失败，reconcile 为 Missing。
- 订阅没有 cockpit 那种 `auth_user_info_raw` / plan / credit 原文。userInfo 用 token、邮箱、user id 和 machine blob 拼。`plan_tier` 是唯一会写进 userPlan 的套餐字段；没有就不发明套餐名。
- 比对只看邮箱，不看 token。没有邮箱不能切（报缺少 access_token 或 email），pin 不动。
- 05 的结论是三平台密码学可写（注入口令），没有把 qoder 锁在 L3。真钥匙串和真机重启仍未做。

# 23 · zcode — 切号写回（L4）

> 依赖：22 + 04 + 08。

## 解锁的契约

切号写回 `~/.zcode/v2/credentials.json`（enc:v1 加密）+ `config.json`（api key 分流）+
`settings.json` + `zcode_device_mid`；回读解密校验。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/zcode.rs` | adapter impl：`zcode_home()` 定位（settings.json `dataBaseDir` 优先）→ 解密目标行 → 备份 → 加密写 credentials.json → config/settings → 回读解密校验 → 落 pin；oauth 账号与 api-key 账号写不同文件的分流忠实 cockpit |
| app | `usage_switch.rs` | 注册 zcode |

## 人能看见

ZCode 卡切号 + 三态 badge；真机 ZCode 重启换号（人工记录）。

## 验证

- credentials.json 加密写回→回读解密校验回环（沙箱）。
- oauth/api-key 分流断言（写对文件）。
- 人工 smoke 记录。

## 委托给实现者的决定

- `zcode_device_mid` 的生成/保留策略（对齐 cockpit）。

## 必须保持绿

- `dataBaseDir` 覆盖场景写对位置（否则写到幽灵目录）。
- 密钥推导与 08 完全一致（同一 helper）。

## 会改变本片的人类反馈

- 无。

## 结果

`zcode` 走 `IdeCredentialAdapter`，注册在 `usage_switch/ide.rs`。实现在 `usage_switch/zcode.rs`，文件读写在 `usage_switch/zcode_store.rs`（合在一个文件里会超过行数上限）。没有 `ide/` 子目录。没有进 `DesktopAppId`。`supports_switch("zcode")` 为 true。沙箱里 `available()` 仍是 true：写的是 `SKILLSTAR_TOOL_SYNC_HOME` 下的文件，不是钥匙串。

定位先读 `{home}/.zcode/v2/setting.json`（不是 `settings.json`）的 `dataBaseDir`，凭据写到 `{dataBaseDir}/.zcode/v2/`；没有覆盖时就是 `{home}/.zcode/v2/`。密钥用 `enc_v1::zcode_credential_key`。home 是操作系统 home，测试里由 `SKILLSTAR_TOOL_SYNC_HOME` 代替，`dataBaseDir` 不进密钥。

OAuth 写 `credentials.json`：`oauth:active_provider`、`oauth:{zai|bigmodel}:access_token`、`refresh_token`、`user_info`、`zcodejwttoken`，值是 `enc:v1`。API key（`provider_state.kind = api_key`）写 `config.json` 的 `providers.builtin:{zai|bigmodel}.options.apiKey`，明文，因为这是 provider 配置而不是 enc:v1 凭据库；这次写入不改 `credentials.json`。OAuth 激活不改 `config.json`。`setting.json` 只合并 `modelProviderFamilyModes.{family}` 为 `oauth` 或 `apiKey`，reconcile 用它决定看哪一份文件；`dataBaseDir` 和其它设置键保留。`telemetry-state.json` 的 `deviceMid` 不生成、不轮换、不删除——设备身份不是账号，规格里的 `zcode_device_mid` 没有写进 `setting.json`。官方客户端是否把 `modelProviderFamilyModes` 当成当前连接方式：**未验证**。

`activate` 先备份，再原子替换，解密回读（API key 则明文回读）一致之后才 pin。回读或写入失败会还原备份，不移动 pin；原来没有文件就删掉新建的文件。`sync` 把刷新后的凭据写回去，不改 pin。`reconcile`：没有可用凭据是 `Missing`，对得上某个订阅是 `LinkedTo`（文件优先于 pin），有凭据但对不上是 `Diverged`。同一上游的 mode 为 `apiKey` 时，API key 压过这份 `credentials.json` 里的 oauth 会话。`forget` 只去掉本账号写过的字段；文件里只剩这个账号时删掉文件。不删 `setting.json`，也不动另一个上游或无关键。

测试都在 `SKILLSTAR_TOOL_SYNC_HOME` 里，没有读写真 `~/.zcode`。

真机 ZCode 重启换号：**未验证**。没有启动过 ZCode，也不能说它重启后会换号。

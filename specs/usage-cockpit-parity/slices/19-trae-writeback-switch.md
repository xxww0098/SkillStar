# 19 · trae×4 — 切号写回（L4）

> 依赖：18 + 04 + 06。

## 解锁的契约

切号写回四个变体各自 `…/<App>/User/globalStorage/storage.json` 的 iCube 加密键
（`iCubeAuthInfo`/`iCubeServerData`/`iCubeEntitlementInfo`/`usertag`）；四个 app 是四个
独立安装目录，互不干扰。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/trae.rs` | `StorageJsonAdapter` 族 impl：platform→目录映射（切片 03 的表）；byte_crypto 加密写键；`platformId`/`platformName` 元数据；整文件 JSON 备份+原子替换+回读解密校验 |
| app | `usage_switch.rs` | 注册 ×4 |

## 人能看见

四张 Trae 卡各自切号 + 三态 badge；对应 app 重启换号（人工记录）。

## 验证

- storage.json 写→回读→解密校验回环 ×4（沙箱 fixture）。
- 并发写安全：原子替换+备份测试。
- 人工 smoke 记录（至少 `trae` 主变体）。

## 委托给实现者的决定

- storage.json 中需写入的完整 iCube 键清单（对齐 cockpit `trae_account_core_injection.rs`）。

## 必须保持绿

- 官方 app 运行中写文件要重试/明确报错，绝不部分写。

## 会改变本片的人类反馈

- 无。

## 结果

`trae` / `trae-solo` / `trae-cn` / `trae-solo-cn` 共用 `usage_switch/trae.rs` 一个 `IdeCredentialAdapter`，在 `usage_switch/ide.rs` 注册四次。目录仍走 `tool_paths::trae_storage_path_for`。密文是标准 base64 的 `byte_crypto` blob。官方 app 重启未验证。

写入的 iCube 认证键（只替换这些键，不整文件覆盖）：

- 用户认证键：文件里已有 `iCubeAuthInfo://icube.cloudide` 时写它；否则写已有的其它用户认证键（`iCubeAuthInfo://`，排除 `usertag` 和 `icube-dc:`）；都没有时创建默认键。明文含 `platformId` / `platformName` / `authClientId` / `authDomain` / `loginHost`，以及令牌、user id、有的话还有 `deviceKeyPair`。已有未知字段保留；上一账号的令牌、邮箱、过期时间会清掉。
- 设备键：订阅里有设备密钥，且 `provider_state` 带了 device id，或文件里已经有 `iCubeAuthInfo://icube-dc:<id>` 时，更新那一个槽。不新造 device id。

不写 `iCubeServerData://*`、`iCubeEntitlementInfo://*`、`iCubeAuthInfo://usertag`，也不动其它键。

- activate：文件必须已存在。先整文件滚动备份，内存里拼好再 `atomic_write`。写完解密回读，和准备写入的 JSON 一致才 `set_active_subscription`。回读失败或写入失败都把备份拷回去，不钉 pin。文件被占用时按 0/50/100/200ms 重试，然后明确报错，不做逐键部分写。
- reconcile：解不出认证键是 Diverged；没有令牌也没有 user id 是 Missing；access token、user id 或 refresh token 对上已有订阅是 LinkedTo，并吸收更新的令牌和设备密钥。
- forget：只删对得上的用户认证键，以及密钥对得上的 `icube-dc` 槽。其余键留下。`storage.json` 不删。

测试把 `SKILLSTAR_TOOL_SYNC_HOME` 指到临时目录，覆盖 global 与 cn（切换回环覆盖四个变体）。`cargo test -p skillstar-app --locked --lib -- usage_switch::trae` 9 passed。注册表测试 `supports_switch_follows_the_registries` 与 `forgetting_an_ide_card_does_not_need_a_live_store` 也通过。

静默决定：

- 不移植 cockpit `ensure_auth_raw_for_inject` 的整份账号 JSON（server / entitlement / usertag 默认 `"row"`）。SkillStar 没有那些原文，写空的会盖掉 IDE 里已有的键。
- 没有 storage.json 时不创建文件，报错并保持原 pin。
- 官方客户端是否正在运行没有探测；只对占用中的写入做短重试。重启换号未验证。

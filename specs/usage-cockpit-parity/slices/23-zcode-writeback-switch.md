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

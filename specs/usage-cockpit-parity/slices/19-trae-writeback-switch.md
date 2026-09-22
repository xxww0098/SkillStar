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

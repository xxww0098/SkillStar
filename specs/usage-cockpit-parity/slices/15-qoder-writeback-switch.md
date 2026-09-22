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

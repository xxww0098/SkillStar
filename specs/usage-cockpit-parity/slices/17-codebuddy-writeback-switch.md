# 17 · codebuddy×2 — 切号写回（L4）

> 依赖：16 + 04 + 05。

## 解锁的契约

切号写回两端各自的 `state.vscdb` `secret://` 键：
国际版 `planning-genie.new.accessToken`，CN 版 `…accessTokencn`（后缀差异集中进参数表）。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| app | `usage_switch/ide/codebuddy.rs` | 一个 adapter 实现 + domain/key 参数表；注册 `codebuddy` + `codebuddy-cn` 两 catalog |
| app | `usage_switch.rs` | 注册 ×2 |

## 人能看见

两卡各自切号 + 三态 badge；真机重启换号（人工记录 ×2）。

## 验证

- safe-storage 键写回回环 ×2 变体（key 名差异断言钉死——**写串后缀是已知坑**）。
- reconcile 三态 ×2。
- 人工 smoke 记录。

## 委托给实现者的决定

- 两变体共享 impl 的参数表形状。

## 必须保持绿

- 各写各的目录，`CodeBuddy` 与 `CodeBuddy CN` 不互踩。

## 会改变本片的人类反馈

- 无。

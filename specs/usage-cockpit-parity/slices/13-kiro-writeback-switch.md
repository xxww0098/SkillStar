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

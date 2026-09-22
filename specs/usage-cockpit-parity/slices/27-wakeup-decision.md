# 27 · DECISION — Antigravity LS 唤醒网关 go/no-go

> 无代码前置依赖。产出 = `docs/decisions.md` 一条决策记录。
> 参照：`/tmp/cockpit-tools/crates/cockpit-core/src/modules/wakeup_gateway.rs`（~1.8k 行）
> + `src-tauri/src/modules/wakeup_*.rs`。

## 要决策的问题

是否在 SkillStar 实现「定时唤醒 Antigravity language server 会话」能力——
 cockpit 的做法是本地 TLS 网关（自签证书）冒充官方 LS，向其发 `StartCascade`/
`SendUserCascadeMessage`（`exa.language_server_pb.LanguageServerService`），
借一条真实 Cascade 消息触发 5h/weekly 配额窗口提前重置。

## 评估维度（写进决策记录）

1. **机制复杂度**：拉起官方 LS 进程 + TLS 网关 + LS 版本模式检测（<1.21.6 随机端口分支）
   + CSRF state 文件 + 60s 启动窗口 + 8s 绑定超时——保守估计 2k+ 行 + 进程管理。
2. **ToS/账号风险**：本质是向 provider 发合成用户请求刷配额窗口——不同于只读配额查询，
   有触发风控/违反服务条款的实际可能。SkillStar 现有全部能力都是「读配额 + 写本机凭据」，
   这是第一次「主动消耗上游资源」。
3. **替代方案**：(a) 不做（推荐默认——quota 监控不依赖唤醒）；(b) 仅移植直连路径
   （`cloudcode-pa` 的 `fetchAvailableModels`/`streamGenerateContent` 探活，不经网关）；
   (c) 全量移植网关 + Cascade 拦截。
4. **与既有架构的关系**：唤醒任务切的是 LS 进程登录态而非 IDE 全局存储——
   **不得复用 `usage_switch` 的 pin/reconcile 语义**，若做需独立 provider-store 写路径。

## 输出

`docs/decisions.md` 新条目：选项 + 理由 + 若选 (c) 则另立 spec（不进本梯子）。

## 会改变本片的人类反馈

- 这是留给人的唯一决策点：唤醒任务的 ToS 风险是否可接受。

## 结果

选择 (a) 不做。记录在 `docs/decisions.md` 的 D-061。没有写唤醒网关代码。若要改成移植网关，另立 spec。

# Usage 与 OAuth

状态：active

本文件维护订阅、配额、账号切换和 Usage UI 的当前契约。详细的 Usage 卡片拆分过程已冻结在 [../../others/usage-card-refactor-2026-07.md](../../others/usage-card-refactor-2026-07.md)。

## 所有权与存储

- `skillstar-usage` 拥有 catalog、OAuth/API-key fetcher、token 加密、subscription storage 和 refresh guard。
- subscription/usage snapshot 位于 `~/.skillstar/config/usage/`。catalog 和条目数量以 `crates/skillstar-usage/src/catalog.rs` 及其测试为准。
- 不支持的旧 auth-mode 行在 load migration 中清理；文档不保留已删除 catalog 清单。
- 远程请求统一使用 `skillstar_core::infra::http_client::probe_http_client`。
- 除非用户明确要求，不修改完成态的 `fetchers/oauth/cursor.rs`。

## OAuth 与刷新

- OAuth 启动返回 auth URL、pending id 和可选 device code；前端展示后轮询/回调完成。
- 编辑既有 subscription 发起 OAuth 时，pending state 带原 subscription id；成功后原位替换并保留用户 metadata/sort order，不新增重复卡片。
- revoked/expired refresh token 的 `invalid_grant` 映射为 `AuthRequired`，持久化 reauth 状态，而不是把原始 provider body 直接暴露给 UI。
- 可覆盖 OAuth client credential 的 provider 按 env → compile-time → `oauth_clients.json` → built-in fallback 解析；要求外部凭证的 provider 保留自己的专用配置模块。
- refresh 只用窄 patch 更新 fetcher-owned runtime 字段，不能用网络请求开始时的旧整行覆盖用户刚修改的 metadata 或凭证。

## CLI 账号切换事务

- `skillstar-app::usage_switch` 是唯一跨 Usage/Models 的账号激活 facade；Tauri command 不直接理解 provider 凭证文件 schema。
- `skillstar-app::usage` 拥有前端 DTO 投影以及 CRUD、refresh、OAuth completion 与账号切换的应用 use case；`usage_commands.rs` 只适配 Tauri 命令、窗口关闭和 active-changed 事件。
- 每个 catalog 使用进程内 async mutex + OS file lock，覆盖网络等待、storage 和 CLI 文件写入，防止多进程交错。
- 事务顺序是识别/保存当前会话 → 准备目标 → 临时 pin → 原子写入 → 回读验证 → 提交或回滚。
- 任何一步不能证明目标可恢复或写入成功时，保留旧 active subscription 和旧 CLI 文件。
- provider 私有 session snapshot 保持在私有加密 store，不向通用 `Subscription` 增加 provider-specific 字段。
- 删除 subscription 时同时调用私有 session cleanup；失败必须可观察，不能遗留隐形凭证所有权。

### Grok/xAI 特例

- credits endpoint 决定当前 weekly/monthly period；weekly 是严格 percent-only，不能用 calendar-month 金额伪造绝对周额度。
- calendar-month spend 作为次级 credit 展示，不再生成第二条“monthly quota”。零 on-demand cap 不显示。
- 周额度本轮缺失时可携带上次已知 weekly window，避免闪回错误月视图。
- CLI snapshot 用稳定 subject/user identity 归属；冲突 identity fail closed。token 必须满足 Grok CLI scopes，写入前后均验证，并保护外部进程并发改写。

## Usage 卡片与 active 状态

- `setActive` 的返回值是后端真相：只有返回行 `is_active=true` 时，前端才 demote 同 catalog sibling。
- CLI 切换被拒绝时，保留旧 badge，并使用“switch not applied”反馈；不能乐观宣称目标已激活。
- card 使用 shell + body registry + primitives；特化 catalog 在 `bodyRegistry.ts` 明确注册 ownsBalance/ownsCredits/reset ownership。
- 所有额度条共享 `UsageMeter` primitive（标签+已用徽章 / 大号等宽数字 / `ProgressTrack` / 脚注+重置芯片）；货币/绝对/百分比额度读作同一套语法，各渲染器只组合它，不自绘盒子。
- 重置倒计时归属唯一律：meter 只在 `windowRendersOwnReset(window)` 为真时渲染自身重置芯片，否则由 card MetaStrip 顶部显示；二者互补，同一 reset 绝不出现两次。
- 主卡与独立窗口共享逻辑 body，不共享 chrome。浮窗使用 dark chrome + `LightBodySurface`，compact body 的品牌 CSS vars 必须来自 `brandThemes.ts`。
- 每个 catalog 的品牌 header、bar 和 glow 只在 `src/features/usage/lib/brandThemes.ts` 定义；卡片内不得硬编码另一套颜色。
- 费用/renew、delete 和 API key action 按 surface matrix 决定；浮窗不暴露 API key copy bar。

## 请求构建

- 所有 fetcher 走 `skillstar-usage::request::Req`：统一附带 header/bearer/body、把非 2xx 归一为 `RequestError::HttpStatus`，让各 provider 只写响应解析。
- 底层 client 一律由 `crate::http_client::usage_http_client()` 提供，透传 `probe_http_client` 的代理设置；fetcher 不自建 `reqwest::Client`。

## 前端刷新与错误

- `useUsageData` 管理 list、refresh、focus 和 active-changed 的 cache 更新；window 自己保持生命周期，不挂主窗口 Context。
- refresh、OAuth finalization、metadata 编辑、删除和切换必须进入后端同一 catalog serialization domain。
- load error、switch error、cliFailed 和 usage.error 分层展示，不把所有失败折成“暂无数据”。

## Dock 右键菜单（macOS）

- macOS 右键点 Dock 图标，菜单顶部列出各订阅额度：每行 `<账号> · 剩余 N%`，按最紧张（剩余最少）在前排序。N% 是该订阅「消耗最高的那条额度窗口」的剩余份额。行是信息项（无 action，自动置灰）。
- 分层：纯函数 `skillstar_usage::dock_usage::snapshot_remaining_percent`（对快照，可测）→ 读存储+拼行的 `skillstar_app::usage::dock_menu_lines` → Tauri 胶水 `src-tauri` `core::dock_menu`。
- macOS 只能经 app delegate 的 `applicationDockMenu:` 提供 Dock 菜单，Tauri/muda 均未封装。实现用 objc2 把该方法**加到 Tauri 现有 delegate 类**（不替换 delegate），AppKit 每次右键即读缓存行重建菜单。
- 触发点：启动（`install()` 装 hook + `refresh()` 读上次快照）、`refresh_all_subscriptions`/`refresh_subscription_usage`/`delete_subscription` 之后 `refresh()`。后台巡检 loop 只管 skills、不刷新用量，故不在其触发范围。

## 验证

```bash
cargo test -p skillstar-usage -p skillstar-app
bun run test -- src/features/usage
```

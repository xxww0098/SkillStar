# 01 · TokenImport 接缝 + `provider_state_encrypted`

## 解锁的契约

`AuthMode::TokenImport` 端到端落地（catalog → DTO → 前端表单 → 建卡 → refresh dispatch），
以及 `Subscription.provider_state_encrypted` 通用加密 blob。本片**不接入任何新 provider**——
纯接缝先行，第一个消费者是切片 09（github-copilot）。

## API 接缝

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 域 | `crates/skillstar-usage/src/catalog.rs` | `AuthMode::TokenImport`（serde `token-import`，ts-rs 导出）；新增元组 `OAUTH_TOKEN_IMPORT = &[OAuth, TokenImport]` 备用 |
| 域 | `crates/skillstar-usage/src/subscription.rs` | `provider_state_encrypted: Option<String>`（`serde(default, skip_serializing_if)`）；`SubscriptionBuilder::provider_state(json)`；字段 doc 写明「provider 私有版本化 JSON，AES-GCM，不进 DTO/日志」 |
| 域 | `crates/skillstar-usage/src/storage.rs` | `apply_oauth_credentials` 与 `apply_fetcher_state` 均纳入 `provider_state_encrypted` 窄 patch（refresh 会轮换它，如 zcode jwt） |
| 域 | `crates/skillstar-usage/src/fetchers/mod.rs` | `AuthMode::TokenImport => oauth::dispatch(subscription)`（导入后凭据形态与 OAuth 一致，refresh 同路） |
| 域 | `crates/skillstar-usage/src/oauth/common.rs` | `reauth_target` 的 `auth_mode == OAuth` 过滤放宽为 `OAuth \| TokenImport`（重授权落原行） |
| app | `crates/skillstar-app/src/usage/dto.rs` | `SubscriptionDto.has_credential` 覆盖 `provider_state_encrypted` 非空分支；DTO **不**暴露该字段本身 |
| app | `crates/skillstar-app/src/usage/service.rs` | `create_subscription`/`update_subscription` 对 `auth_mode=token-import` 显式拒绝并指向导入命令（导入命令在切片 04 落地，本片先保证不误建无凭据行） |
| 前端 | `src/features/usage/lib/authModes.ts` | `selectableAuthModes` 放行 `"token-import"`；注释更新 |
| 前端 | `src/features/usage/lib/usageLabels.ts`（或等价 label 处） | `token-import` 的展示名；i18n key `usage.authBadgeTokenImport` 等 |

## 人能看见

无新 UI 行为：尚无 catalog 使用 token-import（listing 由后续 provider 片开启）。
可见证据 = 生成类型里出现 `token-import` + 测试绿。

## 验证

- `cargo test -p skillstar-usage`：serde 往返（旧行无此字段可读）、`apply_oauth_credentials`/`apply_fetcher_state` patch 覆盖断言、refresh dispatch 对 TokenImport 行的路由测试（mock fetcher 断言走到了 oauth::dispatch）。
- `reauth_target` 放宽的单测。
- `bun run types:gen` + `check_generated_types.sh`；`bun run test -- src/features/usage`（authModes/label 测试更新）。
- `cargo check --workspace --locked`。

## 委托给实现者的决定

- `provider_state` blob 的内部版本化 JSON 形状（`{"v":1,...}` 由各 provider 模块自定义）。
- TokenImport 在前端表单中的展示文案（i18n 走既有模式）。

## 必须保持绿

- 既有 catalog 计数/tier/identity 测试；`auto_fetch_providers_exclude_manual_auth`（TokenImport 计入 auto-fetch 集合，断言随之更新）。
- 全部既有 OAuth 流程回归。

## 会改变本片的人类反馈

- 若你更希望复用 `platform_token_encrypted` 而不是新增字段——现在说，这是最后一个便宜时点。

## 结果

已落地，无新 catalog provider。

- `AuthMode::TokenImport`（serde `token-import`）与 `pub(crate) const OAUTH_TOKEN_IMPORT`。后者被 `reauth_target` 与 auto-fetch 测试共用，避免 dead_code。
- `Subscription.provider_state_encrypted`：`serde(default, skip_serializing_if)`。旧行缺字段可读；`None` 不写出。
- 窄 patch 只在 `apply_oauth_credentials` 赋值，`apply_fetcher_state` 经它覆盖两条路径。无关账单/排序字段保持不动。
- `AuthMode::TokenImport` 的 refresh 走 `oauth::dispatch`。`reauth_target` 接受同一 catalog 的 OAuth 或 TokenImport 行。
- `create_subscription` / `update_subscription` 拒绝该模式，错误指向尚未落地的 `import_subscription_token`。DTO 的 `has_credential` 把非空 blob 算作凭据，不暴露字段本身。
- 前端 `selectableAuthModes` 放行 `"token-import"`。徽标文案 `usage.authBadgeTokenImport`：英文 “Token import”，中文 “令牌导入”。

测试：`cargo test -p skillstar-usage --locked --lib` 191 passed；`cargo test -p skillstar-app --locked --lib usage::` 36 passed；`bun run types:gen` 与 `check_generated_types.sh` 通过；`bun run test -- src/features/usage` 143 passed；`cargo check --workspace --locked` 通过。

静默决定：

- `SubscriptionBuilder::provider_state(json)` 接收明文版本化 JSON，在 `build` 时用现有 AES-GCM 加密。JSON 形状（`{"v":1,...}`）留给各 provider，本片不解析。setter 尚无生产调用，标了 `#[allow(dead_code)]`，单测锁住加密与重授权。
- 错误字符串：`Usage: \`token-import\` 账号只能通过 \`import_subscription_token\` 导入，不能用创建或更新订阅写入`。`UpdateSubscriptionInput` 没有 `auth_mode`，因此 update 拒绝的是已存行已经是 TokenImport 的情况，避免通用编辑器改写导入行。
- `cursor.rs` 的手写 `Subscription` 字面量补了 `provider_state_encrypted: None`，否则新字段无法编译。Cursor OAuth 行为未改。
- xAI `finalize` 里单独的 `auth_mode != OAuth` 检查没有放宽；该 provider 没有 token-import。

推迟：导入命令、catalog 行、表单 UI 均不在本片。尚无 catalog 提供该模式，所以界面行为不变。

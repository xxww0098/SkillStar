# Usage × cockpit-tools 对齐 — Spec

状态：active（7/27：01–03、05–08）
更新：2026-09-22

## Next Agent Prompt

你从分支 `feat/usage-cockpit-parity` 接续。主工作区 `main` 上有无关未提交改动，不要混进提交，也不要在那个脏工作区里改本特性。

- **当前进度**：01、02、03、05、06、07、08 完成。决定见 `choices.md`。四个 spike 只证明了本地往返；真钥匙串、Windsurf 重启、ExchangeToken、ZCode app 都还没验，所以不降级。
- **下一个拾取点**：`slices/04-ide-adapter-registry.md`。它改 `usage_switch`、`local_import`、`service.rs` 和订阅对话框。做完再开 provider。
- **顺序**：04 → 09–23（各 provider 的 (a) 片可并行，(b) 片用已经落地的 spike）。24 等全部 (b) 片。
- **每次切片收尾**：更新本节的进度与下一个拾取点，把静默决定补进 `choices.md`。

## 目标

把 cockpit-tools（`github.com/jlcodes99/cockpit-tools`）已验证的 provider 覆盖面移植进
SkillStar Usage：新增 12 个 catalog 条目（9 个 provider 族），每条按「能力等级」交付——
配额监控为底线，交互登录 / Token 导入 / 本机导入 / 切号写回 / 多开实例逐级解锁，
**只宣称已证明的等级**。

参照实现：`/tmp/cockpit-tools`（稀疏克隆：`crates/cockpit-core` + `src-tauri/src` + `docs`）。
每 provider 的实现蓝本文件清单见 `provider-reference.md`。

## 已拍板决策（实现者继承，不再讨论）

| # | 决策 | 结论 |
| --- | --- | --- |
| D1 | 范围 | 全面对齐：配额 + 认证 + 本机导入 + 切号 + 多开（按能力证明解锁） |
| D2 | provider 全集 | github-copilot / windsurf / kiro / qoder / trae×4 / zed / zcode / codebuddy / codebuddy-cn；**workbuddy 不做**；cliproxy sidecar、WebSocket 插件联动出范围 |
| D3 | Token 导入 | 新增 `AuthMode::TokenImport`（serde `token-import`），粘贴裸 token 或凭据 JSON |
| D4 | provider 私有凭据 | `Subscription` 新增 `provider_state_encrypted: Option<String>`（AES-GCM 版本化 JSON blob），**不**放宽既有 `platform_token_encrypted`（它语义属于 deepseek API-key 平台 token）；DTO 只经 `has_credential` 间接感知 |
| D5 | OAuth 完成契约 | `OAuthStartInfo.flow: OAuthFlow` 四态：`LocalCallback` / `RemotePoll` / `SchemePaste` / `Immediate`；前端按 flow 渲染不按 catalog_id 分支 |
| D6 | IDE 切号 | `usage_switch::ide::IdeCredentialAdapter` trait + 注册表；antigravity/cursor 包薄壳迁移，行为不变 |
| D7 | Trae | 4 个独立 catalog 条目，一套 `TraePlatformKind` 参数化实现 |
| D8 | zcode 回调 | 基线 = `SchemePaste`（粘贴 `zcode://` URL，provider 进程内解析，不走 HTTP 重放）；内嵌 webview 拦截为可选增强，不占主线 |
| D9 | copilot 切号 | 无本地 IDE 凭据 → 不交付默认切号；VS Code profile 注入仅作实例侧可选增强 |
| D10 | zed | keychain internet-password 写回 = macOS-only；Zed.app 无 `--user-data-dir` → 永不进实例注册表 |
| D11 | 唤醒任务 | Antigravity LS 网关推迟为切片 27 的独立 go/no-go 决策 |
| D12 | 兼容/迁移 | 无：纯新增，无向后兼容层、无数据迁移脚手架 |
| D13 | codebuddy-cn | 指 `www.codebuddy.cn` 端点（非 WorkBuddy/copilot.tencent.com） |
| D14 | 能力门控 | 复用现有机制（adapter 注册表 / `INSTANCE_CATALOG_IDS` / `selectableAuthModes`），不新建运行时等级系统；L0–L5 只是验证与发布语言 |

## 能力等级（验证语言，非运行时概念）

| 级 | 名称 | 证明合同 |
| --- | --- | --- |
| L0 | quota-only | fetcher fixture 测试 + 一次真实凭据联调记录 |
| L1 | +TokenImport | parser 拒绝非法输入测试 + 导入后 refresh 成功 |
| L2 | +交互登录 | 对应 flow 端到端登录一次（人工）+ pending/cancel/timeout 测试 |
| L3 | +本机导入 | 路径解析 + 解密 + 导入 fixture 测试 |
| L4 | +切号写回 | 写→回读→校验测试 + 官方 app 重启认可的人工记录 |
| L5 | +多开实例 | `instance_capability` 隔离报告 Verified（切片 24） |

低于 L0 的 entry 不进 catalog。UI 入口由注册表推导，未达标的能力没有入口。

## Slice graph

```
01 TokenImport+provider_state 接缝 ─┐
02 OAuthFlow 完成契约 ──────────────┤
03 tool_paths + 存储基元 ───────────┼──► 09 copilot ──► 10/11 windsurf ──► 12/13 kiro
04 IDE adapter 注册表 + 导入 dispatch ┘        │
05 spike: VS Code Safe Storage ──────────────┼──► 14/15 qoder ──► 16/17 codebuddy×2
06 spike: Trae device-proof + byte_crypto ───┼──► 18/19 trae×4
07 spike: Zed RSA + keychain ────────────────┼──► 20/21 zed
08 spike: ZCode enc:v1 ──────────────────────┴──► 22/23 zcode
                                                            │
                                                            ▼
                                       24 实例隔离验证矩阵（每 app 一份报告）
                                       25 前端抛光（logo/i18n/devMock/能力徽标）
                                       26 文档同步（usage README/decisions/errors/boundaries）
                                       27 唤醒任务 go/no-go 决策
```

并行性：05–08 spike 全是本地 fixture，可并行，也只需在对应 (b) 片前完成；
各 provider 的 (a) 片互不依赖，可并行；24 依赖所有 (b) 片已落地。

## 切片索引

| # | 切片 | 解锁 |
| --- | --- | --- |
| 01 | `01-token-import-and-provider-state-seam.md` | `AuthMode::TokenImport` + `provider_state_encrypted` 垂直打通（无 provider 消费） |
| 02 | `02-oauth-flow-contract.md` | `OAuthFlow` 四态契约 + `local_server::wait` 参数 map 化 + 前端按 flow 渲染 |
| 03 | `03-tool-paths-and-store-primitives.md` | 8 个 app 的路径解析 + vscdb 通用 upsert/delete + 原子 JSON 落点 |
| 04 | `04-ide-adapter-registry.md` | `IdeCredentialAdapter` trait + 注册表 + antigravity/cursor 迁移 + 导入 dispatch 表 |
| 05 | `05-spike-vscode-safe-storage.md` | `secret://` v10/v11 加解密往返（macOS keychain pw / Win DPAPI / Linux peanuts） |
| 06 | `06-spike-trae-device-proof-bytecrypto.md` | ECDSA P-256 DeviceProof + iCube `byte_crypto` AES-128-CBC 往返 |
| 07 | `07-spike-zed-rsa-keychain.md` | RSA-OAEP/PKCS1v15 回调解密 + `security` internet-password 读写 |
| 08 | `08-spike-zcode-credential-file.md` | `enc:v1:` AES-256-GCM + 机器派生密钥往返 |
| 09 | `09-github-copilot.md` | copilot L0–L2（OAuth + TokenImport + quota） |
| 10/11 | `10-windsurf-quota-login.md` / `11-windsurf-writeback-switch.md` | windsurf L0–L3 / L4 |
| 12/13 | `12-kiro-quota-login.md` / `13-kiro-writeback-switch.md` | kiro L0–L3 / L4 |
| 14/15 | `14-qoder-quota-login.md` / `15-qoder-writeback-switch.md` | qoder L0–L3 / L4 |
| 16/17 | `16-codebuddy-quota-login.md` / `17-codebuddy-writeback-switch.md` | codebuddy×2 L0–L3 / L4 |
| 18/19 | `18-trae-quota-login.md` / `19-trae-writeback-switch.md` | trae×4 L0–L3 / L4 |
| 20/21 | `20-zed-quota-login.md` / `21-zed-writeback-switch.md` | zed L0–L3 / L4(macOS) |
| 22/23 | `22-zcode-quota-login.md` / `23-zcode-writeback-switch.md` | zcode L0–L3 / L4 |
| 24 | `24-instance-isolation-gate.md` | 实例注册表扩展 + 逐 app 隔离实证 |
| 25 | `25-frontend-polish.md` | 图标/i18n/devMock/入口名单收尾 |
| 26 | `26-docs-sync.md` | docs/features/usage、decisions、errors、boundaries |
| 27 | `27-wakeup-decision.md` | Antigravity LS 唤醒网关 go/no-go |

## 全局不变量（每片必须守住）

- HTTP 一律 `request::Req` + `usage_http_client()`（probe_http_client 代理）；
  唯一例外沿用 `manual_callback` 的 loopback-only no_proxy client。
- 标准 form-grant token 交换走 `oauth::token_endpoint::post_token`；
  **非标准 token 腿**（Kiro IDC `/token`、CodeBuddy `X-Refresh-Token`、Trae `ExchangeToken`、
  ZCode JSON 交换）实现放各自 fetcher，但错误分类语义必须与 `post_token` 一致。
- 错误三态表不动：401/invalid_grant→`AuthRequired`（置 latch、清快照）；
  429/5xx/传输→`Transient`（保快照）；其它→`Fetcher`（清快照）。403 不算 auth。
- finalize/TokenImport/local_import/切号写回都在 `with_catalog_lock`；
  refresh 只窄 patch runtime 字段（`provider_state_encrypted` 纳入两个 patch 路径）。
- 重授权落原行：`target_subscription_id` + `carry_over_user_metadata`，TokenImport 同权。
- 测试不写真实 `$HOME`/keychain：`SKILLSTAR_TOOL_SYNC_HOME`/`SKILLSTAR_DATA_DIR` 沙箱。
- `Subscription` 除 D4 的通用 blob 外不加 provider 字段；不改 `fetchers/oauth/cursor.rs`；
  不手改 `src/types/generated/`；不新增 crate。
- 文件 ≤~1000 行、~800 起拆：`local_import.rs` 退化为 dispatch 表；大 provider 预置
  `fetchers/oauth/<provider>/{mod,login,quota,import}.rs` 目录形态。
- 每 provider 片同一 PR 内同步：catalog 行 + `PROVIDER_IDENTITIES` + brand_color +
  i18n + devMock + catalog 计数/tier/conformance 测试更新。
- 默认 `DefaultUsageBody`；只有结构性新数据形状才注册 `bodyRegistry`（测试锁死集合）。
- 门禁：`cargo test -p skillstar-usage -p skillstar-app`、`bun run test -- src/features/usage`、
  `bun run lint`、`bun run types:gen` + `check_generated_types.sh`；结构片加跑
  `check_workspace_deps.sh`/`check_feature_imports.sh`/`check_file_size.sh`/`check_command_boundaries.sh`。
- 视觉/界面产出切片：收尾前跑 screenshot-critique 作为最后一道检查（无偏第二意见）。

## 停机与降级（全局）

| 触发 | 动作 |
| --- | --- |
| spike 假设证伪 | 对应 provider 锁到已证明最高级；同族同步降级；结论写进对应切片「结果」节 |
| 写回后官方 app 不认可 | 该 provider 锁 L3 以下，adapter 保留但 `supports_switch=false` |
| 共享凭据文件冲突证实（kiro `~/.aws`、zed 全局 keychain） | `instance_capability=Blocked`，UI 不出多开 |
| 私有 API 字段漂移 | fetcher 报明确错误而非静默零值；`docs/errors.md` 登记 |
| 地基片破坏既有流程 | 回滚该片，改并行 API 方案重提 |

## 已知未知（实施时回答，不是计划缺口）

- `storage.rs` 的 JSON 落盘是否已原子写——切片 03 现场确认，否则补 helper。
- Windsurf 是否有官方 `windsurf://` 深链——切片 10 先按本地回调实现，遇到再降级。
- Kiro IDC 注册缓存 hash 算法——照抄 cockpit `kiro_account.rs` `idc_client_registration_path`。
- zcode 内嵌 webview 增强（D8 的可选项）是否值得做——切片 22 完成后由用户定。

## 已知限制（有意取舍）

- **pending OAuth 态不持久化**：`pending_state` 是进程内存态，重启后登录会话作废需重试
  （cockpit 的 `oauth_pending_state/*.json` 落盘不移植）。重评条件：任一 provider 的
  device flow 有效期 > 10 分钟且用户反馈丢失。

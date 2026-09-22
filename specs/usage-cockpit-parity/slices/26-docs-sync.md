# 26 · 文档同步

> 依赖：全部实现片。**文档与代码同一变更序列收尾**（AGENTS.md 要求，不留"待补"）。
> 注：各 provider 片已在各自 PR 内更新 usage README 的对应段落；本片做总收口与架构文档。

## 接缝

| 文档 | 改动 |
| --- | --- |
| `docs/features/usage/README.md` | Auth 模式表加 `TokenImport`；「OAuth 与刷新」节改写为 flow 四分法（LocalCallback/RemotePoll/SchemePaste/Immediate）；新增「IDE 凭据适配器」节（注册表、存储基元六族、写回不变量）；各 provider 特例段（copilot 双 token、windsurf proto、kiro IDC、trae 设备签名、zed macOS-only、zcode 双文件/scheme、codebuddy UA/body-code）；catalog 描述改为「以 catalog.rs 及测试为准」原则不变 |
| `docs/decisions.md` | 新条目：`provider_state_encrypted` 通用 blob（D4）、OAuthFlow 四态（D5）、`IdeCredentialAdapter` 注册表（D6）、实例能力门控与 zed/kiro 结构性 Blocked（D10+切片 13/24 结论）、唤醒任务结论（切片 27） |
| `docs/errors.md` | 各 spike 的死因与降级（若有）、私有 API 漂移哨兵清单 |
| `docs/boundaries.md` | `tool_store/`、`token_import.rs`、`usage_switch/ide/`、`fetchers/trae/` 等新目录的所有权登记 |
| `README.md` | 能力表更新（若根 README 列了 provider 清单） |
| `graft build` | 结构变化后刷新图谱 |

## 验证

- 文档中不手抄可枚举数量（AGENTS.md 红线：计数以代码注册表及测试为准）。
- `graft check` 绿。

## 委托给实现者的决定

- 各段的详略（规则：行为/契约写清，清单链代码）。

## 必须保持绿

- 同一事实只有一个 SSOT；历史稿不混入。

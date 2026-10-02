# usage/models 项目树重构（family-ready）— Spec

状态：open（5/10）
更新：2026-10-02

## Next Agent Prompt

你是下一个接手本 spec 的实现者。从切片 01 开始，按编号顺序做；08/09 可与 03-07 并行，10 随时可做。

- **先读本 README 的「已拍板决策」和「全局防火墙」**，再读你将做的切片文件。决策不重开讨论。
- 当前状态：01 已完成（tests/gateway_roundtrip.rs）。下一片是 02（树归位），注意与 evolution spec 的 gateway 泳道（serve.rs/ledger）协调——02 移动的文件含 affinity.rs（route/），等 evolution-03 合并后再动工。08/09（usage 线）随时可做。
- 每片完成：跑该片「验证」小节列出的命令 + `bash scripts/internal/check_clippy_ratchet.sh`（ratchet=1，出现新诊断修代码、永不调 baseline），更新本节状态与 TODO，再做下一片。
- 环境注意：本机若 21847 端口被占，`serve_binds_default_port` 会环境性失败（tests/serve.rs），验证命令统一加 `-- --skip serve_binds_default_port`，不是回归。
- 结束你的 pass 前，把本节改写成下一个 agent 需要的样子。

### 全局 TODO

- [x] 01 行为锁矩阵（tests only）— `tests/gateway_roundtrip.rs` 5 测试 + `tests/fixtures/gateway/handwritten.json`；五写方交叉/profile 吞字段/幂等/坏文件拒写/缺文件默认全钉
- [x] 02 树归位：store/ route/ catalog/ 目录成形 — 12 rename（5 个 R100 整文件）；183 符号逐一相同；测试 240↔240 等价
- [x] 03 store/doc.rs schema owner — ModelGatewayDoc/OwnerRow + rest/extra 整段保留；已知字段「缺席不造」skip 语义（D-079）；DocStoreError doc_read/doc_parse/doc_write
- [ ] 04 读方改道（7 个读方走 lens）
- [ ] 05 写方改道（5 个写方走 lens，每写方一 commit）
- [ ] 06 catalog typed parse
- [ ] 07 app/models/gateway/ 投影归位
- [ ] 08 custody_tests 拆分
- [x] 09 usage service 拆分 — service/ 目录六块（helpers/projections/crud/refresh/oauth/switching），25 符号面冻结，service_tests.rs 字节级搬移
- [x] 10 families 落点契约（只写契约）— 契约即 slices/10-families-contract.md（SSOT），models README 已加指针；FamilyRow 不内联 members（默认裁决，检查点关闭）

## 目标

`skillstar-gateway` 对 `model_gateway.json` 的访问现状是 **12 个接触点（5 写方 + 7 读方）各自打开文件、各自用 `serde_json::Value` 做行级手术**，`family` 标签已经被 `visible.rs` 的 stringly `family_of` 读取。本 spec 把项目树收口成三个有主人的目录：

- `store/`：`model_gateway.json` 的唯一 schema 拥有者（typed 文档 + 每字段 lens），family 一等化时的唯一落点；
- `route/`：路由决策组（纯内存行为，不开文件）；
- `catalog/`：models.dev 缓存 + typed parse（消灭三处重复的 catalog 解析）。

外加 `skillstar-app/src/models/gateway/` 投影子目录（一个 lens 一个文件）、usage 侧两处超行数文件拆分、以及 families 的**落点契约**（只写契约不实现）。

背景：这是「usage/models = magpie + cockpit-tools 结合」对抗审查后的树重构部分（P2-6 双 catalog 清单、P3 结构债的修复）。对抗审查的 Phase 0-5（LAN 鉴权、网关账本、会话解析等）**不在本 spec**。

## 已拍板决策（实现者继承，不再讨论）

| # | 决策 | 结论 |
| --- | --- | --- |
| D1 | 范围 | 树重构 + family 落点契约；family 一等化**不实现** |
| D2 | 兼容/迁移 | 硬切，无兼容层、无迁移脚手架 |
| D3 | 对外面 | `skillstar-gateway/src/lib.rs` 的 pub use **符号清单逐字不变**（消费者只有 skillstar-app + `app/src/cli/gateway.rs`、`app/src/cli/claude_mcp.rs`；src-tauri 不直接依赖 gateway） |
| D4 | crate | 不新增 crate，全部是既有 crate 内部私有 module 移动 |
| D5 | schema 形状 | **外层 typed + 行内保留 Value 混合**：`ModelGatewayDoc` typed 承载已知顶层键与 providers/groups 行的共享字段（id/members/routing/affinity/family），未知键经 `#[serde(flatten)]` 原样回写。**不用 `deny_unknown_fields`**（`redact_*`/`vision`/`classifier`/`rules`/`note`/手编键必须存活往返）。签名级契约见切片 03 |
| D6 | profile 吞字段 | **保持现状（丢弃行内未知字段）**，由 01 的钉子测试记录；改语义另立切片，本 spec 不做 |
| D7 | 事务性 | store 收口后**仍非事务**：open→改→save 独立循环，不引入锁。声明写进 store/mod.rs 模块文档与 architecture.md |
| D8 | 读/写错误语义 | 写路径坏文件 = Err（现状 `load_object`）；读路径坏文件 = 默认值（现状 `read_doc`）。lens 用 `open()`（严格）与 `open_lenient()`（宽容）两个入口保住差异 |
| D9 | 红线口径 | 两处 = `usage_switch/custody_tests.rs`（1416）+ `app/src/usage/service.rs`（819）。`windsurf.rs`(987)/`cursor.rs`(963)/`cloud_code.rs`(946)/`codebuddy.rs`(915) 同在预警带但**出范围**，模板复用另立 |
| D10 | store 不懂行为 | `classifier`/`rules` 是 classify/rules 模块的私有 schema，进 `extra` 由各模块行级读取；store 不 import 行为模块的类型 |
| D11 | 键序 | serde_json 未开 preserve_order，现状写盘本就全键字母序重排；键序**不是契约**，测试一律 parse-back 断言 |
| D12 | Phase 0-5 | LAN 鉴权、账本、会话解析、凭据通道、交叉视图不属于本 spec，未来另立 |

## 现状事实（三份独立侦查收敛，附证据）

- **12 个接触点**。写方 5：`routing_file.rs:83-96`、`group.rs:128-155`（唯一额外校验「groups 必须是数组」）、`profile.rs:160-183`（唯一吞字段者，`write_profiles` 用 json! 重建整个数组）、`names.rs:112-125`、`listen.rs:65-78`。读方 7：`route.rs:126-142`（已有局部 typed `GatewayFile` 先例）、`rules.rs:87-107`、`classify.rs:81-105`、`visible.rs:133-184`（含 stringly `family_of`）、`effort.rs:132-158`、`vision.rs:486-497`、`redact/mod.rs:716-723`。
- `model_efforts`/`visible`/`redact_*`/`vision`/`classifier`/`rules` 七个字段**全 repo 无代码写方**——手编字段，读侧全部宽松默认。
- 写方统一 `to_vec_pretty` + `atomic_write`；未知字段靠「mutate 整个 Value」幸存。
- 重复实现：`catalog_ids` ×2（`visible.rs:146-173` 与 `app/models/picker.rs:55-77`）、models.dev 存在性检查 ×3（`names.rs:95-110`、`visible.rs:83-105`、`effort.rs`）。
- models.dev api.json 实测 schema：几乎所有字段可选；仓库目前只消费 provider key、model key、`reasoning_options[type=effort].values`。
- 测试基建 `EnvRestore::sandbox` + `lock_gateway_env()` 在 ≥8 个 tests/ 文件逐字重复（收口机会记录在案，本 spec 不强制）。

## Slice graph

```
01 行为锁矩阵（纯加测试） ──┐
02 树归位（移动+数据/行为分桶）─┤
03 store/doc.rs schema ──────┼─→ 04 读方改道 ─→ 05 写方改道 ─┐
06 catalog typed parse（依赖 02，可与 03-05 并行）───────────┤
07 app gateway/ 投影（依赖 02-06 完成）                       ├─→ 完
08 custody_tests 拆分（完全独立）─────────────────────────────┤
09 usage service 拆分（完全独立）─────────────────────────────┤
10 families 落点契约（只写文档，随时可做）────────────────────┘
```

## 全局防火墙（跨切片有效，违反即拆片重做）

1. 01 的测试与任何产品代码改动**不同提交**——测试必须先在旧代码上绿。
2. 02 的移动提交只允许 mod/use/path 与数据/行为分桶；夹带逻辑修改即重做。
3. 05 内**每个写方一个 commit**（listen → names → routing → group → profile 顺序），绝不两个写方同提交。
4. gateway 线（01-07）与 usage 线（08-09）**永不同提交**。
5. **baseline 零改动**：`file_size_baseline.txt`、`clippy_baseline.txt`、orphan baseline 本 spec 任何切片都不许碰；动了就是计划出错。
6. 任何切片 pub 函数签名冻结；每片跑 `cargo check --workspace --locked`。

## 验证门槛（每片至少）

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
cargo check --workspace --locked
bash scripts/internal/check_clippy_ratchet.sh
bash scripts/internal/check_no_orphan_modules.sh
bash scripts/internal/check_file_size.sh
```

涉及 app/前端时另加：`cargo test -p skillstar-app --locked`、`bun run types:gen && git diff --exit-code src/types/generated/`。

## 关联但出范围（勿在本 spec 内解决）

Phase 0-5 演进（LAN 鉴权、网关持久账本、会话解析、凭据通道收敛、交叉视图）**已立 spec：`specs/usage-models-evolution/`（open，13 切片）**。它对树 spec 有多处 ⛩ 前置（03/04/05/06/07/09），两 spec 的并行纪律与前置表见其 README。第一轮对抗审查遗留给它的三个开放问题已在那边拍板：会话归因经 dispatch 头管道优先、首期解析受管 6 agent（zcode 出范围）、gateway key 安装级随机纯内部。

## 已知未知

- `serve_binds_default_port` 本机端口占用是环境性失败（见 Next Agent Prompt）。
- 07 的 `types:gen` 若出 diff，说明 ts_rs `export_to` 受移动影响——预期零 diff，出 diff 当场回查而不是提交。
- custody_tests 引用的全路径测试名（`docs/errors.md:188` 有 `usage_switch::custody_tests::grok_shares_the_cli_lock_file_and_writes_its_holder_line`）在 08 拆分后必须原样可跑。

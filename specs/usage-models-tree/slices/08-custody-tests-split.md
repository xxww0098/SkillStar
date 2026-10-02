# 08 custody_tests 拆分（usage 线，可与 03-07 并行）

## 解锁的契约

`crates/skillstar-usage/src/usage_switch/custody_tests.rs`（1416 行）拆成目录：**测试名、断言、全路径零变化**，纯移动。消除 check_file_size 的 NOTE（test cap 1500，1416 未 FAIL 但越 AGENTS.md 1000 纪律线）。

## 结构（对齐 usage_switch/ 既有 per-tool 惯例，choices C9）

```
usage_switch/custody_tests/
├── mod.rs          # 共享 harness（Sandbox/write_json/read_json/fixtures/token 常量，~L29-288）
│                   # + 核心 custody 语义：activate/reconcile/badge/Windows-copy/forget/resync/file invariants
├── cursor.rs       # L637-990 段 + write_cursor_state 随行
├── antigravity.rs  # L991-1112 段 + write_antigravity_state 随行
├── opencode.rs     # L1113-1202
├── codex.rs        # L1203-1306、L1365-1386 中 codex 部分
└── pure.rs         # 纯函数断言（L1365-1416 无文件系统部分）
```

挂载方式：`usage_switch.rs:586-588` 的 `#[cfg(test)] #[path] mod custody_tests;` 改为自然目录解析（同目录已有 `target.rs` + `target/{codex,grok,opencode}.rs` 先例）。子文件对 `super::custody`/`super::target` 的引用经 mod.rs 再导出或 `super::super::`。

## 人能看/跑什么

```bash
cargo test -p skillstar-usage --locked custody -- --list > after.txt
# 与拆分前 --list 比对：名字集合与总数完全一致
cargo test -p skillstar-usage --locked custody
cargo test -p skillstar-usage --locked custody_tests::grok_shares_the_cli_lock_file_and_writes_its_holder_line -- --exact
# ↑ docs/errors.md:188 引用的全路径必须原样可跑
```

## 必须保持绿

custody 全部 ≈50 测试（拆分前后 `-- --list` 集合一致即证明）；同 crate 各 per-tool `_tests.rs`（kiro/trae/zcode/qoder/windsurf/zed/codebuddy）不受影响；orphan 门禁（新文件必须 mod 可达）。

## 委托给实现者的自由

文件数量（≥4 即可）；某测试块归 mod.rs 还是子文件（按内容块自然归属）。

## 防火墙

不改任何断言与测试名（红线）；沙箱纪律头注（`SKILLSTAR_DATA_DIR`+`SKILLSTAR_TOOL_SYNC_HOME`+临时 HOME）随文件走。

## 会改变本片的反馈

若某测试隐式依赖同文件 const 共享状态（侦查认为没有）→ 该 const 归入 mod.rs harness，不复制。

# 01 行为锁矩阵

## 解锁的契约

在动任何产品代码之前，把 `model_gateway.json` 的**现状读写语义**用测试钉死。此后 02-05 的每一步都以「这套锁不改一行仍然绿」为等价性证明。

现有测试只覆盖单写方往返（各断言自己的旁观字段），缺三样：

1. **五写方顺序交叉**：同文件依次 `save_group → save_routing → save_profile → save_model_name → save_listen`，断言各字段簇互不吞（尤其 `redact`、`note`、手编顶层键 `x-custom` 类旁观者）。
2. **profile 吞字段钉子**：明确断言 profile 行内未知字段在 save 时**被丢弃**（这是现状记录，不是期望；README D6 依据）。
3. **幂等**：同一 save 连跑两次，文件字节不变（parse-back 相等即可，键序非契约见 D11）。

## API 接缝

零新 API。只新增 `crates/skillstar-gateway/tests/gateway_roundtrip.rs`（+ 如需 fixture：`tests/fixtures/gateway/handwritten.json`，含全部已知键：providers/groups 行带 `note`/`family`/`classifier`/`rules`，顶层 `model_names`/`model_efforts`/`visible`/`listen`/`redact`/`redact_words`/`vision`/手编 `x-custom`）。

测试命名对齐仓库可证伪风格（`a_…` / 现状句）：

- `a_field_write_keeps_every_foreign_key_and_row_field`
- `profile_save_drops_unknown_fields_on_profile_rows_today`
- `saving_twice_is_byte_idempotent`
- `a_broken_file_refuses_the_write_and_keeps_the_bytes`
- `a_missing_file_reads_as_defaults_and_is_not_created`

沙箱纪律：复用既有 `EnvRestore::sandbox` + `lock_gateway_env()` 模式（tests/ 内 ≥8 处先例，本片允许复制一份，不要求抽共享 helper）。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked --test gateway_roundtrip
```

五个测试全绿即「现状语义表」成立；`profile_save_drops_…` 的断言就是 D6 的事实地基。

## 必须保持绿

gateway 全部既有测试**不改一行**。全量：

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
```

## 委托给实现者的自由

断言的具体写法、fixture 内容富余度、是否拆多个测试文件。

## 防火墙

本片与任何产品代码改动**不同提交**；测试必须先在未改动的工作区跑绿。

## 会改变本片的反馈

用户若说「profile 吞字段要顺手修成保留」→ D6 作废，本片第 2 项改为断言保留语义，且 05 的 profile commit 升级为行为变化（另立切片）。

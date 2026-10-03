# 09 usage service 拆分（usage 线，可与 03-08 并行）

## 解锁的契约

`crates/skillstar-app/src/usage/service.rs`（819 行，越过 AGENTS.md「接近 800 开始拆分」线）按用例拆分；`usage/mod.rs` 的 `pub use service::*` 对外 **25 个符号面冻结**（消费者：`src-tauri/src/commands/usage_commands.rs:9`、`app_shell.rs:127`、`dock_menu.rs:23`）。

## 结构（按用例，文件边界实现者定）

```
usage/
├── mod.rs / dto.rs / token_import.rs        # 不动
├── service/                                  # 或平铺多文件，二选一
│   ├── （读投影）list/summary/alerts/dock/api_key/get_active
│   ├── （CRUD）create/update/delete/reorder
│   ├── （刷新）refresh 族 + network hints + refresh_failure
│   ├── （OAuth 流）start/await/submit/cancel/import_from_local
│   ├── （切换）set_active/switch_to_cli/reconcile/clear
│   └── （共享 helper）map_err/fill_active/ensure_catalog
```

`service_tests.rs`（635 行）挂载点随新结构移动（`#[cfg(test)] #[path]` 是既有手段），其 `super::` 可见性保持（拆出函数保持 `pub`/`pub(super)`）。

## 人能看/跑什么

```bash
cargo test -p skillstar-app --locked usage
cargo check --workspace --locked          # 25 符号面由编译器+三消费者验证
bun run types:gen && git diff --exit-code src/types/generated/   # 零 diff
```

## 必须保持绿

service_tests 全部（含 `only_auth_required_latches_reauth…` 刷新语义锁）；ENV_LOCK/EnvGuard 模式不变。

## 委托给实现者的自由

目录 vs 平铺；六块切几刀；helper 归属模块。

## 防火墙

与 gateway 线（01-07）不同提交；不改任何函数行为。

## 会改变本片的反馈

无预期分歧；若拆分中发现 service 内隐藏用例（草稿未识别的第六类）→ 记入 choices 后按用例加文件，不塞进 helper。

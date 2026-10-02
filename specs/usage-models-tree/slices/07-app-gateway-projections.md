# 07 app/models/gateway/ 投影归位

依赖：02-06 完成（gateway pub 面与 catalog API 已稳定；实际上面全程不变，保守排在最后）。

## 解锁的契约

`crates/skillstar-app/src/models/` 的 gateway 透镜投影归入 `gateway/` 子目录，一个 lens/写入口一个文件；`models/mod.rs` 的 re-export 名集不变，src-tauri 命令层零改动。

## 文件清单（11 个）

`gateway/{routing, groups, profiles, names, listen, effort, picker, recent, gateway_save, codex_save, account_book}.rs`

- `account_book` 一并迁入：它实现的是 gateway 的 `AccountBook` trait（架在 usage 存储上的透镜）。
- 留在 `models/` 顶层：`board.rs`（看板投影，可 import gateway/ 的 pub 面）、`agents.rs`、`dto.rs`（models v4 域投影）。
- `src-tauri` 走子模块路径的两处（tools.rs、mod.rs 引 `models::agents`/`models::board`）不受影响——两者都留顶层。

## 投影规约（写进 gateway/mod.rs 模块文档）

- 投影文件之间**禁止互相 import**（`use super::` 只许指向 gateway/mod.rs 的 re-export 与 `crate::test_support`）。
- `board.rs` 消费 gateway/ 但反向禁止。

## 人能看/跑什么

```bash
cargo test -p skillstar-app --locked
bun run types:gen && git diff --exit-code src/types/generated/   # 必须零 diff
bun run lint && bun run build
```

前端 Models 页烟测一次：picker 列表、路由控件、分组、监听开关、最近调用。

## 必须保持绿

`routing_control_persists_in_gateway_json`、`routing_control_does_not_rewrite_provider_store`、`saving_a_group_id_uses_the_existing_writer`、`group_control_rejects_a_cycle_and_keeps_the_file`、picker 两测、board 三测。

## 委托给实现者的自由

无（纯移动 + mod 组织）。`types:gen` 若出 diff 当场回查 `#[ts(export)]` 的 `export_to` 路径——预期零 diff。

## 会改变本片的反馈

用户若希望 board.rs 也归 gateway/ → 拒绝并引用规约（board 是跨 models-v4 域投影，不是 gateway 透镜）。

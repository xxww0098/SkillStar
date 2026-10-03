# 02 树归位：store/ route/ catalog/ 目录成形

依赖：01（行为锁已绿）。

## 解锁的契约

gateway crate 的目标树成形；`skillstar_gateway::` 对外符号面与行为**零变化**；`git diff -M` 以 rename 为主。schema 本片不引入（03 的事），被移动的文件内部保持各自的 `gateway_path()`/`load_object()` 原样随行。

## 目标树与归属表

```
src/
├── store/            # model_gateway.json 的家（本片只是搬家，schema 归 03）
│   ├── mod.rs        #   mod 声明
│   ├── routing.rs    #   ← routing_file.rs（整文件）
│   ├── groups.rs     #   ← group.rs 的数据面：stored_group_ids/stored_groups/save_group/
│   │                 #      check_write/reaches/depth_of/insert_group/groups_in/member_list
│   ├── profiles.rs   #   ← profile.rs（整文件，apply_profile 随行走）
│   ├── names.rs      #   ← names.rs（整文件）
│   ├── listen.rs     #   ← listen.rs（整文件）
│   └── visible.rs    #   ← visible.rs 的数据面：visible_names/read_doc/family_of/catalog_ids
├── route/            # 决策组（纯内存，无 fs）
│   ├── mod.rs
│   ├── order.rs      #   ← route.rs 的排序面：route_smart/route_mode/rotate/usage_order/
│   │                 #      RouteMode/RouteCandidate/AllowanceSnapshot/USED_SHARE（stored_route_mode 留在 store 侧文件，03/04 处理）
│   ├── affinity.rs   #   ← affinity.rs
│   ├── classify.rs   #   ← classify.rs（其文件读取本片随行，04 改道）
│   ├── rules.rs      #   ← rules.rs（同上）
│   ├── rest.rs       #   ← rest.rs
│   ├── hold.rs       #   ← hold.rs
│   └── groups.rs     #   ← group.rs 的行为面：expand_group/auto_groups/same_model/slug/flatten/walk/ServedModel
├── catalog/
│   └── cache.rs      #   ← models_dev.rs（整文件）
```

crate 根留下的行为面变薄：`effort.rs`（apply_upstream_effort + fit）、`visible.rs`（model_shown/shown_model_ids/listed_ids 投影）、`codex.rs`、`agents/`、`claude/`、`serve.rs`、`surface.rs`、`translate.rs`、`sign.rs`、`redact/`、`vision.rs`、`trace.rs`、`wsl.rs`、`outbound.rs`、`chatgpt.rs`、`codex_prompt.rs`、`hold.rs`→已列 route/（以实际依赖为准，见委托自由）。

`lib.rs`：`mod store; mod route; mod catalog;` + pub use 改源路径，**符号清单逐字不变**。crate 内约 24 处 `crate::xxx` 引用改路径（草稿已盘点：affinity.rs:15、serve.rs:269-272、classify.rs:18、rest.rs:16、surface.rs:183-297、agents/body.rs:408、agents/alma.rs:34-35、agents/hanako.rs:58,152、effort.rs:74,231、sign.rs:10、trace.rs:13 等；`crate::GROUP_PREFIX` 这类经 pub use 解析的不受影响）。

## 人能看/跑什么

```bash
git diff -M --stat          # 以 R100/R09x rename 为主
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
```

## 验证

- 上面的 gateway 全量测试。
- `cargo check --workspace --locked`（app + CLI 两个消费者零改动即证 D3）。
- `bash scripts/internal/check_no_orphan_modules.sh`、`check_clippy_ratchet.sh`、`check_file_size.sh`。
- `grep -rn "skillstar_gateway::" crates/skillstar-app/src src-tauri/src | wc -l` 与拆前一致。

## 必须保持绿

01 的 gateway_roundtrip 全套；`tests/group.rs`（含 `group_expands_members` 的 raw 不变断言）、`tests/route.rs`、`tests/rename.rs`、`tests/profile.rs`、`tests/lan.rs`、`tests/visible.rs`、`tests/effort.rs`；app 侧 `routing_control_persists_in_gateway_json`。

## 委托给实现者的自由

- `hold.rs`/`rest.rs` 归 route/ 还是留根（草稿有分歧，按实际依赖：只被路由链消费则进 route/）。
- store/route 子模块内函数排序。
- `visible.rs` 数据面/行为面的具体切线（以本文件归属表为准，边界 ±1 个函数可调）。

## 人工检查点（非阻塞）

`git diff -M` 浏览一遍确认无意外重写；对照归属表核对 store/mod.rs 的 mod 列表。发现夹带逻辑修改即要求拆片重做（防火墙 2）。

## 文档（同一提交）

- `docs/boundaries.md`「Workspace crate 所有权」gateway 行补：crate 内分 `store/`（model_gateway.json schema 属主）、`route/`（决策组）、`catalog/`（models.dev 目录）。
- `docs/architecture.md` 数据所有权段同步半句。

## 会改变本片的反馈

用户若要求 `codex.rs` 一并归入 `agents/`（草稿 A 的可选项）→ 可并入本片，仍是纯移动。

# 04 读方改道：7 个读方走 lens

依赖：03（doc 容器与 roundtrip 已绿）。

## 解锁的契约

7 个只读方删除各自的私有 `gateway_path()`/`read_doc()`，改经 `ModelGatewayDoc::open_lenient()` + 各自 lens 读；**读语义逐字段不变**（缺文件→默认、坏文件→默认，README D8）。crate 内对 `model_gateway.json` 的读取收口到 store。

## 改道清单（读什么、默认值是什么）

| 模块（02 后位置） | 现状读取 | 默认值 | 顺带 |
| --- | --- | --- | --- |
| `store/…`（route 的 stored_route_mode 迁回 store 侧） | `route.rs:126-142` 局部 `GatewayFile`/`GatewayRow` Deserialize | smart/auto | **删除局部 typed 结构**（choices C8），改 doc lens |
| `route/rules.rs` | `rules.rs:87-107` groups 行 `rules` | 空 | 行级读取走 `OwnerRow.extra`（D10：store 不懂 GroupRule） |
| `route/classify.rs` | `classify.rs:81-105` groups 行 `classifier` | 默认分类器 | 同上 |
| `store/visible.rs`（数据面已在此） | `visible.rs:175-184` read_doc + `family_of` Value 手术 | 空名单 | `family_of` 改 typed：`OwnerRow.family` |
| `effort.rs` | `effort.rs:132-158` stored_names/read_gateway | 空表 | — |
| `vision.rs` | `vision.rs:486-497` | off | — |
| `redact/mod.rs` | `redact/mod.rs:716-723` | off/空 | `redact_*` 键从 `rest` 读，store 不 typed 化 |

lens 形态：自由函数对（`store/routing.rs::read_routing(&doc, owner, id)` 等），签名草案见 03 与 choices C7。

## 人能看/跑什么

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
grep -rn "model_gateway" crates/skillstar-gateway/src --include="*.rs" | grep -v store/
# 期望：只剩写方（05 处理）与注释
```

## 必须保持绿

- 01 的 gateway_roundtrip（读方改道不改写路径，锁仍全绿）。
- `tests/route.rs`（9）、`tests/rules.rs`、`tests/classify.rs`（8）、`tests/visible.rs`（5）、`tests/effort.rs`（8）、`tests/redact.rs`（3）、`tests/vision.rs`、`tests/rename.rs`、`tests/lan.rs`、serve/surface 集成测试——**全部不改一行**。

## 委托给实现者的自由

- lens 放在哪个 store 子文件（routing lens 进 store/routing.rs，visible lens 进 store/visible.rs，自然归属）。
- 顺手修不修 `routing_state` 双读怪癖（现状 `stored_route_mode` 与 `affinity_of` 各 open 一次文件）——**允许**，结果一致性由 01 锁保证。

## 防火墙

本片只动读路径；写路径文件里的读取（`load_object`）留到 05。读写混改即拆片。

## 会改变本片的反馈

若某读方的「坏文件=默认」在现状其实是 Err（侦查未发现，但以实现时实测为准）→ 以实测为准记录进本片 choices，lens 对应入口换 strict。

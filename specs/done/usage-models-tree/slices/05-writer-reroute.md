# 05 写方改道：5 个写方走 lens（一片五 commit）

依赖：04（读方已收口，lens 形态已验证）。

## 解锁的契约

5 个写方的 pub 函数内部变为 `ModelGatewayDoc::open() → lens write → save()`，删除各自的 `load_object`/`to_vec_pretty`/`atomic_write` 复制。**每个写方一个 commit，顺序固定**：

1. `listen`（最小：顶层单键）
2. `names`
3. `routing`（routing_file → store/routing.rs）
4. `group`（带 cycle/depth 校验前置；「groups 必须是数组」的特有校验**留在 store/groups.rs 的 save 路径**，不全局化——见 README D5 与草稿 B 风险 3）
5. `profile`（最后：借 01 的吞字段钉子声明「保持丢弃语义」D6）

## API 接缝

pub 签名全部不变（D3）。写 lens（store 子模块自由函数）：

```rust
// store/routing.rs
pub(crate) fn write_routing(doc: &mut ModelGatewayDoc, owner: RouteOwner,
    id: &str, mode: RouteMode, affinity: AffinityMode);
// Smart 删 routing 键、Auto 删 affinity 键（现状 apply() 语义，C7 护栏二）

// store/groups.rs / store/profiles.rs / store/names.rs / store/listen.rs 同款 write lens
```

profile 的 `write_profiles` 重建数组 = 现状吞字段语义照抄（`ProfileRow` 无 extra 保留，或保留 extra 但写入时丢弃——以 01 钉子测试断言为准实现，断言说了算）。

## 人能看/跑什么

每个 commit 后：

```bash
cargo test -p skillstar-gateway --locked -- --skip serve_binds_default_port
cargo test -p skillstar-app --locked
git grep "model_gateway.json" crates/skillstar-gateway/src --include="*.rs"
# 终态：文件名只出现在 store/（doc.rs 的路径常量一处）
```

## 必须保持绿

- 01 的 gateway_roundtrip 交叉矩阵（本片的主裁决者：五写方顺序交叉 + 幂等 + 坏文件拒写，全程不改一行）。
- `tests/group.rs`（5，含 note 保留与 `assert_key_table_untouched`）、`tests/route.rs`、`tests/rename.rs`（2）、`tests/profile.rs`（3）、`tests/lan.rs`（4）。
- app 侧 `routing_control_persists_in_gateway_json`、`routing_control_does_not_rewrite_provider_store`、`group_control_rejects_a_cycle_and_keeps_the_file`、`saving_a_group_id_uses_the_existing_writer`。

## 委托给实现者的自由

- lens 内部实现与 store 子文件内组织。
- `SaveXxxError` 枚举内部映射整理（Display 文案不动，`check_error_strings.sh` 门禁）。

## 人工检查点（非阻塞）

profile commit 合并前把 diff 给用户看一眼（唯一语义敏感点：吞字段钉子测试决定实现细节）。用户沉默按 D6 继续。

## 防火墙

- 每写方一 commit（README 防火墙 3）；绝不两个写方同 commit。
- 本片与 usage 线（08/09）不同提交。

## 会改变本片的反馈

01 的交叉矩阵若在本片某 commit 变红 → 该写方改道与现状有语义差，当场二分修复，不继续下一个写方。

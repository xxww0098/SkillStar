# choices.md — 草稿分歧裁决记录

三份独立草稿（A 最少切片 / B 风险优先 / C 接缝质量）各自侦查后合并。收敛处直接采纳；分歧处裁决如下。实现者遇到本文记录过的分歧不要重开。

## C1 排序：先移动还是先 schema

- A：先整体移动（02 片含数据/行为分桶，~15% 超出纯 rename 的 review 量），schema 后落进最终位置，零二次搬运。
- B/C：先 schema（新文件零迁移），再改道（文件位置与内部实现同时变），移动放最后。
- **裁决：A 的排序。** 理由：B 自己的防火墙「schema 引入与调用方改道不同提交」在 B/C 排序下被迫违反——改道片同时是移动片，机械与语义混在一个 diff。A 排序让 02 是纯机械（可 `git diff -M` 审）、04/05 是纯语义（小 diff 可二分）。代价（分桶不是纯 rename）被三份草稿给出的精确分桶线抵消。

## C2 写方改道：一片还是五片

- A：一片（机械等价，测试同集）。
- B：五片（每写方一片，等价性归因可二分）。
- **裁决：一片五 commit。** 切片是 spec 的验收单位，commit 是回滚单位。05 一个切片内强制每写方一 commit（listen → names → routing → group → profile），保住 B 的二分能力，少四个切片的排队开销。

## C3 families.rs 占位空模块（02 片）

- A/C 草稿都提议 02 建 `store/families.rs` 空占位。
- **裁决：不建。** 空模块是给未来实现搭的脚手架却无移除条件（refactor-clean 红线）；契约全文已由切片 10 固化，实现时新建文件即可。

## C4 文档先行独立切片（C 草稿的 00 片）

- C：单独一片先写 boundaries.md 目标态。
- **裁决：取消，嵌入各片。** 「先更新 boundaries.md」指同一变更序列内文档领先，多切片阶梯下让 02/03/10 各自携带文档义务，避免出现描述尚不存在结构的文档快照。

## C5 windsurf.rs 是否入范围

- C 发现 `usage_switch/windsurf.rs`(987) 是最大的超 800 预警带生产文件，提议入范围并留人工检查点确认口径。
- **裁决：出范围（README D9）。** 用户拍板的「两处」= custody_tests + service.rs；windsurf/cursor/cloud_code/codebuddy 同带文件留给后续按 08/09 模板复用，不扩权。

## C6 expand_group 的归属

- C：`route/groups.rs`（消费者是路由链，route/ 禁 fs）。
- A：实现者自由。
- **裁决：C。** 归属原则一句话：「打开 model_gateway.json / models_dev.json 的代码进 store/ 或 catalog/；读出数据后做判定/改写的行为进 route/（决策）或原地（改写请求体的 effort、查询投影的 visible）」。route/ 内禁止 `std::fs` 有 grep 可验的硬边界。

## C7 OwnerRow 行内 typed 程度

- C：providers/groups 共用一个 `OwnerRow`（id/members/routing/affinity/family typed，其余 flatten extra）。
- B：只 typed `id`，其余全 Value（更保守）。
- **裁决：C 的形状，附两条护栏。** 护栏一：lens 按行查找用「first-match-by-id」，重复 id 行、无 id 行原样保留不丢弃（对齐现状 `iter().find` 语义）。护栏二：serde 全字段 `#[serde(default)]`，序列化不做 skip——「Smart/Auto = 删键」的现状语义由 lens setter 负责，struct 不兜底。

## C8 route.rs 既有 GatewayFile 是否顺手收编

- B：可做可不做，不放心就列豁免。
- **裁决：04 片收编**（读者改道时删除局部 GatewayFile/GatewayRow，改走 store lens）。它正是 12 个接触点之一，留豁免会让「唯一 open 点」声明变假话。

## C9 custody_tests 拆分线

- A：按工具（cursor/antigravity/opencode/codex），对齐 usage_switch/ 既有 per-tool `_tests.rs` 惯例。
- C：按功能块（common/lifecycle/ide_targets/hygiene/pure）。
- **裁决：A 的工具线为主、纯函数单列。** 与目录既有惯例一致；`docs/errors.md` 引用的全路径测试名以 `custody_tests::` 前缀保稳。

## T3 切片 03 落地时的实现裁决（banked 2026-10-02）

- **「序列化不 skip」按「缺席不造」实现**：已知字段带 `skip_serializing_if`（空集合/None/空串跳过）——不跳则 provider 行凭空多出 `"members": []`、无 routing 行多出 `null`，切片自己的 round-trip 测试不可能过。实际语义 = 载入的不丢、缺席的不造；`None`/空 = 键缺席，正是 05 的 Smart/Auto 删键 lens 需要的把手。`rest`/`extra` 无任何 skip。
- **DocStoreError = Read/Parse/Write**（`doc_read`/`doc_parse`/`doc_write`）；严格 `open()` 对「已知字段形状坏」整体拒绝（旧 Value 写方按字段各自宽容）——01 锁未钉这类文件，差异记入 D-079 后果栏。
- **测试经 `#[path] include` 够到 pub(crate)**（lib.rs 的 pub use 冻结）：doc.rs 保持自包含（无 crate:: 引用）；04/05 给 doc.rs 加 lens/内部类型时同步调整测试挂载。
- **dead_code allow 是窗口期债务**：04/05 落调用方时删除。

## T8 切片 08 落地时的实现裁决（banked 2026-10-02）

- **真实 mod 挂载优先于 include! 平铺**：切片的「全路径零变化」与 orphan 门禁（新文件必须 mod 可达）+ 防火墙 5（baseline 零改动）在多文件结构下互斥——include! 可保逐字节一致但子文件成孤儿。取 mod 挂载：18 个搬移测试全路径多一段 `custody_tests::<tool>::`，留在 mod.rs 的 22 个逐字节不变；docs/errors.md 引用的那一个（grok_shares…）落在 mod.rs，路径原样可跑（choices C9 与 README 已知未知 #109 的权威口径）。叶子名集合 40/40 前后一致。
- **pure.rs 经 mod.rs 中转引用**（`super::subscription_identity` 而非 `super::super::`），统一一种。
- 顺手修正 docs/errors.md:198 的陈旧 self-check 前缀（-p skillstar-app → -p skillstar-usage，usage_switch 迁 crate 前的旧写法，拆分前就匹配不到）。

# 04 账本读取面与前端换源（P1 UI）

⛩ 前置：tree-07（app/models/gateway/ 投影目录成形）。

## 解锁的契约

`get_recent_calls` 的数据源从进程内 60 条环换为账本文件尾部（重启后数据仍在）；`RecentCallDto` 升级带 in_tokens/session/latency；前端 Gateway 栏展示新列。

## 接缝

- gateway 导出读原语：`ledger::load(since: Option<i64>) -> Vec<Record>` 与 `LedgerQuery`（agent/session/catalog 过滤 + newest-first 分页——维度语义留在 gateway，懂自己的文件；订阅显示名/model label 的 join 在 app）。
- app 新投影 `app/src/models/gateway/ledger.rs`（tree-07 后落位）：合并账本尾页 + 内存环（账本为准），映射 `RecentCallDto`。
- `get_recent_calls` 命令签名不变（前端 api/recent.ts 只改字段消费）；新命令 `get_ledger_page` 挂 models_commands。
- 硬切：DTO 加字段即 `types:gen` 出 diff（预期内），前端同步消费新字段。

## 人能看/跑什么

```bash
cargo test -p skillstar-app --locked
bun run types:gen && git diff --exit-code src/types/generated/
bun run lint && bun run build && bun run test -- src/features/models
# 手测：跑一轮 fake upstream 对话 → 重启应用 → ModelsHub Gateway 列仍在
```

## 必须保持绿

ModelsHub.test.tsx；recent.rs 的 secret 断言测试；`check_command_boundaries.sh`（新命令只做 DTO 适配）。

## 委托给实现者的自由

尾页大小；环与账本的合并窗口；新列的排序与密度（ModelsHub 现有行密度规约不破）。

## 会改变本片的反馈

用户若要求 Usage 页（而非 Models 页）也能看最近调用 → 属切片 13 交叉视图范围，不在本片扩。

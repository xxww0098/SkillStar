# Marketplace

状态：active

本文件维护技能市场快照、搜索和 Publisher 浏览契约。MCP 安装与工具同步见 [../mcp/README.md](../mcp/README.md)。

## 所有权

- `skillstar-marketplace` 拥有 SQLite snapshot、FTS、远程 seed/refresh、Marketplace 专用 DTO 和 MCP registry/curated catalog。
- 市场列表、搜索结果与已安装技能共用 `skillstar-core::types::Skill`；Marketplace 不定义同名副本，也不增加仅用于掩盖重复所有权的转换链。
- `db` 与远程 MCP loader 是 crate 内实现，不作为外部深路径 API；调用方消费 crate root DTO 或明确的 `snapshot`/`mcp_snapshot` use-case 入口。
- `src-tauri/src/core/marketplace_snapshot/` 只包装 Tauri State；业务查询和 schema 不应回流到该目录。
- 技能安装仍归 `skillstar-skills`；“搜索结果 → 安装”的跨域流程通过 command/`skillstar-app` 组合窄 facade。

## 本地优先

- 页面和 CLI search/find 先查询本地 snapshot/FTS，返回 freshness/seeding 状态。
- 远程同步是明确后续动作，不能让页面直接以浏览器 HTTP 替代本地数据源。
- publisher/detail 页面与主列表复用同一 local-first flow；缺 description 时不在浏览器临时 hydrate 另一份数据。
- DB 操作优先短生命周期 WAL connection，避免进程级单 connection lock 阻塞并发读。
- 所有远程 HTTP 使用 `probe_http_client`，GitHub repo 操作遵循 mirror/fallback。
- taxonomy/pack command surface 未挂前端，crate API 与 SQLite 表保留。

## 技能搜索与导入

- GitHub repo import 分为 scan 和 install 两阶段，扫描本身不改变安装状态。
- Marketplace 只返回可安装描述；repo cache、root-first discovery 和实际 install 属于 Skills 域。
- Publisher 与 curated source 的完整清单以 seed/registry 代码和测试为准，文档不复制数量或排序。

## 前端信息架构

- Marketplace 是统一发现入口，但 Skills 与 MCP 在左侧 category rail 中保持清晰分组。
- skill tab 进入技能列表；MCP 官方入口先显示 Publisher grid，再进入 Publisher detail。
- Publisher drill-down 复用主市场的 grid/list 和 toolbar 交互，不创建第二套 fetch 逻辑。
- installed MCP 管理不放 Marketplace，而在 MCP 页面处理。

## 验证

```bash
cargo test -p skillstar-marketplace
bun run test -- src/features/marketplace src/pages/Marketplace.tsx src/pages/PublisherDetail.tsx
```

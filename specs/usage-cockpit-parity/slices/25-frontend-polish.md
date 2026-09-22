# 25 · 前端抛光

> 依赖：provider 片大体落地后。**本片集中收尾 UI 表面积攒的零散项**，可与后期 provider 片并行。

## 解锁的契约

12 条新 catalog 在前端完整呈现：logo/品牌色/i18n/入口名单/devMock 全部到位；
能力入口与后端注册表一致（不切号的不出切号按钮、无实例的不出多开）。

## 接缝

| 文件 | 改动 |
| --- | --- |
| `src/features/usage/lib/brandThemes.ts` | 12 个新 provider 品牌主题（未注册时用 `brand_color` 派生渐变可接受，本片统一抛光） |
| `ProviderLogo.tsx` | 新图标（lobe 有对应 icon 用之，无则字母块 fallback） |
| `lib/desktopApps.ts` + `types.ts` 入口名单 | `LOCAL_IMPORT_CATALOG_IDS`/`INSTANCE_CATALOG_IDS` 按已落地能力填 |
| `devMock/usage.ts` | 新命令 mock + `flow` 字段 + 各 provider 假数据 |
| i18n `en.json`/`zh-CN.json` | token-import 表单、RemotePoll/SchemePaste 面板、各 provider 错误文案、能力提示 |
| `OAuthLoginPanel`/`TokenImportFields` | 文案打磨（zcode「浏览器打不开属正常」、trae「建议本机导入」等场景文案） |

## 人能看见

完整 Usage 页巡览：全部新卡、各登录面板形态、能力徽标一致。

## 验证

- `bun run test -- src/features/usage` + `bun run lint`。
- **视觉门禁**：Usage 页全览截图 + 每个新面板形态截图 → screenshot-critique；
  卡片观感与既有卡对齐用 compare-screenshots（以现有 Cursor/Codex 卡为参照）。

## 委托给实现者的决定

- 品牌色微调、图标选择、文案措辞。

## 必须保持绿

- `bodyRegistry` 特化集合测试不被动摇（新 provider 默认 DefaultUsageBody）。

## 会改变本片的人类反馈

- 卡片密度/徽标样式偏好。

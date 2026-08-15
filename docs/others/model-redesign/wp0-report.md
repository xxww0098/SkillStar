# WP-0 完成报告｜清场：删死代码、搬 prototype、加 TS 孤儿门禁

状态：**完成**，全部验收命令绿。改动全部留在工作区，**未 `git add`、未 `git commit`**。
本包**零 Rust 改动**（`cargo check --workspace --locked` 绿，且 `git status crates/ src-tauri/` 只剩开工前就存在的两个脏文件）。

---

## 1. 数字

| 项 | 数量 |
| --- | --- |
| 真删除 | **35 个文件 / 4771 行** |
| 移动（只搬不改逻辑） | **15 个文件 / 3064 行** |
| `git status` 里显示为 D 的路径 | 50（= 35 删 + 15 移） |
| i18n 删 key | **90 个叶子 key × 2 locale** |
| i18n 新增 key | 6 × 2 locale |
| 工作区总计 | 68 files changed, 231 insertions(+), 8126 deletions(-) |

删除的 4771 行拆开：

| 组 | 行数 | 内容 |
| --- | --- | --- |
| DEV-only 岛 + 两个死 Variant | 1130 | `ModelsHubPrototype`(58) `ia/VariantD2`(251) `ia/VariantD3`(318) `StateDump`(12) `PrototypeOverlays`(91) `VariantB2a`(159) `VariantB2c`(241) —— 与 04 §3.2 的「≈1130 行」逐字吻合 |
| 晋升别名 + 连带死代码 | 115 | `ia/VariantD1.tsx`(6，纯别名) + `src/components/shared/PrototypeSwitcher.tsx`(109) |
| 04 §3.3 的 17 个文件及其测试 | 3116 | 见下 |
| 复核发现的 4 个额外死文件及其测试 | 410 | 见「分歧 1」 |

04 §3.3 的 17 个文件全部核实为真死，无一误判：
`AgentSettingsDialog`(369) `PresetPicker`(359) `AgentHeroCard`(298) `MultiProviderCard`(283)
`CodexSettingsForm`(227) `AppAiCard`(222) `ProviderGalleryCard`(193 + test 46)
`useAgentActivation`(143) `AgentConfigFiles`(143) `api/configFiles`(138)
`AgentStatusPill`(110) `ClaudeModelMapping`(96) `lib/agentStatus`(95 + test 119)
`useAgentHealth`(79) `lib/launchCommand`(58 + test 45) `api/install`(47) `AgentLaunchCommand`(46)。

---

## 2. 搬了什么

`src/features/models/components/hub/prototype/` **已不存在**（`ls` 报「No such file or directory」）。

| 原路径 | 新路径 |
| --- | --- |
| `hub/prototype/matrix/{MatrixChrome,AgentColumnCarousel,matrixColumns}` | `hub/matrix/` 同名 |
| `hub/prototype/matrix/rich/{VariantB2b,RichMatrixShell,ClaudeMappingPanel(+test),OmpRolePanel(+test),OmpRoleRow}` | `hub/matrix/rich/` 同名 |
| `hub/prototype/EditorPage.tsx` | `hub/matrix/EditorPage.tsx` |
| `hub/prototype/types.ts` | `hub/matrix/types.ts` |
| `hub/prototype/matrix/ClaudeSurfaceIcon.tsx` | `components/shared/ClaudeSurfaceIcon.tsx`（见分歧 3） |
| `hub/prototype/usePrototypeHub.ts` | `hooks/useModelsData.ts` |
| `hub/prototype/modelsNavBridge.ts` | `lib/navBridge.ts` |
| `hub/prototype/ia/VariantD1.tsx` | 删除；`ModelsHub.tsx` 直接 import `VariantB2b` |

相对路径不是手改的：写了一次性脚本，把每个 specifier 按**旧位置**解析成绝对目标、跟随已移动的目标、再按**新位置**重算相对路径，所以没有靠数 `../` 的机会。

同名机械重命名（零逻辑改动）：`usePrototypeHub` → `useModelsData`，`PrototypeHubData` → `ModelsHubData`，
`PrototypeOverlay` → `ModelsHubOverlay`。目的就是让 "prototype" 这个词从 `src/` 里彻底消失（`grep -rn prototype src/` 现在零命中）。

随 DEV 岛一起死掉、因而删除的两处：`useModelsData` 的 `stateDump`（30 行调试对象，唯一消费者是
VariantD2/D3）与 `types.ts` 的 `VARIANT_KEYS` / `IaVariantKey` / `normalizeIaVariant`（唯一消费者是
`ModelsHubPrototype`）。`stub` **保留**——它仍被生产路径上的 `EditorPage` 和 `VariantB2b` 调用，删它属于 WP-4。

---

## 3. 与方案清单的分歧（共 10 条）

### 3.1 多删了 4 个文件：04 §3.3 的清单不完整

报告模式跑出 23 个孤儿，比 §3.3 的 17 + 2 个死 Variant 多 4 个。逐个 grep 复核后确认全是真死：

| 文件 | 行数 | 唯一引用方 |
| --- | --- | --- |
| `components/shared/ModelSelectPopover.tsx` | 154 | `AgentHeroCard` / `AgentSettingsDialog` / `MultiProviderCard`（均死） |
| `components/shared/SaveBadge.tsx` | 40 | `AgentSettingsDialog`（死） |
| `lib/filterProviders.ts` | 14 | 只有自己的 property test（125 行） |
| `lib/latencyColor.ts` | 8 | `AgentStatusPill` / `ProviderGalleryCard`（均死）+ 自己的测试（69 行） |

**04 §3.5 有一处错**：它写「`picker`(7) …实际仍被活代码 `ModelsTab` / `ProviderSelectPopover` / `ModelSelectPopover` 使用」——
`ModelSelectPopover` 本身就是死的（`ProviderSelectPopover` 才是活的）。`models.picker` 命名空间确实还活着，
但支撑它的是 `ModelsTab` 与 `ProviderSelectPopover`，不是三个。

不删这 4 个，新门禁就无法归零，所以没有「留着」的选项。

### 3.2 多删了 1 个方案没提到的文件

`src/components/shared/PrototypeSwitcher.tsx`（109 行）——唯一引用方是被删的 `ModelsHubPrototype.tsx:2`。
它在 `src/components/` 下，**新门禁扫不到它**（scan root 是 `src/features/`），靠 grep 发现。
这说明门禁的覆盖面本身就是一条待办：见 §6。

### 3.3 `ClaudeSurfaceIcon.tsx` 没有按 §4.7 改名成 `AgentIcon.tsx`

§4.7 那一格写的是「提升到 `shared/AgentIcon.tsx`（Claude Desktop 分支随 §2.8 删除）」——
改名和泛化是同一件事的两半，而泛化属于 WP-3/WP-4。只做改名会得到一个**只画 Claude 的
`AgentIcon.tsx`，紧挨着已经存在的 `AgentToolIcon.tsx`**，比不改名更容易误导。
所以按原名搬到 `components/shared/ClaudeSurfaceIcon.tsx`，改名留给真正泛化它的那个包。

### 3.4 保留了 6 个 `models.card.taglines.*`，实删 90 而非 ~96

`lib/agentRegistry.ts` 的每条 `AgentDescriptor` 都带 `taglineKey: "models.card.taglines.<id>"`，
而 `agentRegistry.ts` 是活文件、有前后端一致性测试。04 §3.5 把这 6 个算进「无渲染方」是对的
（今天确实没人渲染），但删掉 key 会让一个活注册表的字段指向不存在的翻译。
按 R-10 的「宁可少删」保留，并等 WP-4 的 `AgentList` 把它接回去。

实删：`status`(11) + `dialog`(41) + `gallery`(17) 整组 + `card` 的 21 个非 tagline key = **90 × 2 locale**。
按 §4.7 裁决 **`configFiles`(12) 与 `launch`(2) 全部保留**。

### 3.5 §4.7 里判「删」的文件本包只搬不删

`RichMatrixShell` / `MatrixChrome` / `AgentColumnCarousel` / `matrixColumns` / `VariantB2b` /
`ClaudeMappingPanel` / `types.ts` 在 §4.7 的处置栏是「删/拆」，但它们**今天仍是唯一的生产渲染路径**。
§4.7 描述的是 WP-4 之后的终态，不是 WP-0 的动作。本包把它们搬进 `hub/matrix/`，
好处是 WP-4 的形状变成「删掉 `hub/matrix/`，新增 `agentView/` 等」，而不是在原型目录里做手术。

### 3.6 顺手修了两个**开工前就红**的 i18n 违规

`check_i18n_hardcoded.sh` 在 `main` 上已经是红的。证据不是推断：把 `git archive HEAD` 解到临时目录、
在那份纯净树上跑同一个脚本，同样报 2 个 FAIL：

```
FAIL     1  src/features/my-skills/components/LocalSkillsContent.tsx  (NEW file with hardcoded CJK)
FAIL     2  src/components/layout/Toolbar.tsx                          (NEW file with hardcoded CJK)
```

三处都是 `t("key", { defaultValue: "中文" })` 形式的兜底，而三个 key
（`toolbar.noPendingUpdates` / `toolbar.updateFilterLabel` / `toolbar.updateAllAction` / `common.updating`）
**在两个 locale 里都已经存在**，兜底纯属冗余且与 zh 值重复。删掉 `defaultValue` 即可，行为零变化。
不修就无法满足「`check_i18n_hardcoded.sh` 必须绿」这一条验收；修法是消除债务而不是抬高 baseline。

### 3.7 `.githooks/` 目录不存在

本仓库的 hook 不是签入文件，而是由 `scripts/internal/install_hooks.sh` 生成写进 `.git/hooks/`。
新门禁加进了该脚本生成的 **pre-commit 与 pre-push 两份列表**（紧跟 `check_no_orphan_modules.sh`），
并在脚本的耗时注释里记了实测 0.32 s（pre-commit 预算 6 s，装得下）。

### 3.8 `EditorPage` 里出现了新的不可达分支，本包未动

`detailStyle` 现在恒为 `"tabs"`：唯一传 `"sections"` / `"split"` 的调用方是被删的 VariantD2/D3。
于是 `SectionsEditor` 与 `SplitEditor`（约 150 行，含 `API key` / `OpenAI base URL` / `Anthropic base URL`
等一批裸英文）成了文件内死代码。**没有删**——§4.7 明确把「`detailStyle` 三态删到只剩一种」列在
EditorPage 的 WP-4 行里，且它是逻辑改动。**留给 WP-4，此处备案。**
新门禁是文件级可达性，看不到文件内的死分支，这类残留只能靠人。

### 3.9 裸英文只修了生产 chrome，EditorPage 的 stub 文案没动

已修（7 处）：`VariantB2b` 的 `Bind` ×2 与 `title="Unbind"`、`← Back`；
`RichMatrixShell` 的 `title="Provider × Agent"` 与 `Provider` 表头；
`EditorPage` 头部的 `Back`；`ClaudeMappingPanel` 的 `aria-label="Close"`。
新增 key：`models.common.{back,close,bind,unbind}` 与 `models.matrix.{title,providerHeader}`
（`models.common.*` 的命名遵循 §4.6 的目标约定）。

**未修**：`EditorPage` 的 create / app-ai / agent-settings 三个 overlay 正文里约 30 处裸英文，
其中不少本身就是原型口吻（`Advanced stubs: timeout, wire API, headers…`、
`No models yet — fetch from /models in real UI.`）。§4.7 判定这三个 overlay 会被 WP-4 整体重写
（create 还要换成 `get_provider_presets_flat` 驱动），现在把它们搬进 locale 文件是纯浪费。

### 3.10 顺带确认了 04 §3.5 的那个未决问题

04 §3.5 说「这些没有进 baseline，**说明 `check_i18n_hardcoded.sh` 的判据抓不到它们**（未确认具体判据）」。
现已确认：该脚本的判据是 `[[ "$line" =~ [一-鿿] ]]`，**只匹配 CJK**，裸英文根本不在它的覆盖面内。
这条事实已写进 `docs/features/frontend/README.md`，免得下一个人再猜一次。

---

## 4. 新门禁：`scripts/internal/check_ts_orphan_modules.sh`

- 根：`src/main.tsx` + `src/pages/**`（13 个入口）。以 page 为根而不是只从 `main.tsx` 出发，
  是为了让一个暂时没接路由的 page 不会级联出上百个假孤儿。
- 边：`from "X"` / `import("X")` / bare `import "X"` / `require("X")`。**动态 `import()` 必须算边**——
  `App.tsx` 的全部路由、`ScopeDetailDrawer`、`Markdown` 都只走这条。
- 解析：`@/` → `src/`；候选阶梯 exact → `.ts` → `.tsx` → `.d.ts` → `index.ts` → `index.tsx`，并处理 `./x.js` → `x.ts`。
- 两处刻意保守（假孤儿会诱使别人删活代码）：specifier 用文本匹配，**注释里的路径也算边**；
  测试文件既不当根也不上报，**只被自己的测试引用仍然是孤儿**——`ProviderGalleryCard` 和
  `lib/agentStatus` 当初就是这样活下来的。
- 棘轮：`ts_orphan_modules_baseline.txt`，**空**，且应保持为空。
- 接入：`.github/workflows/ci.yml`（紧跟 Rust 版孤儿门禁）+ `install_hooks.sh` 的 pre-commit 与 pre-push。

### R-10 缓解如实执行

1. **先报告模式跑一轮 + 人工复核**：23 条全部逐个 grep 验证，发现 4 条方案清单外的真死代码（分歧 1）
   和 04 §3.5 的一处事实错误。零误判、零回退。
2. **分轮删除，每轮 `bun run build`**：① 零引用孤儿 → build 绿；② DEV-only 岛 → build 绿；
   ③ 搬 prototype + 改 import → build 绿（一次 3 个 TS6133 未用 import，已修）。
3. **人工点入口**：见 §5 的真实渲染验证。

### 门禁自身的 fixture 测试

`src/test/checkTsOrphanModules.test.ts`（3 个用例，随 `bun run test` 跑）：

- 当前仓库上必须绿；
- 种一个 `src/features/models/__orphan_fixture__/plantedOrphan.ts` 后**必须红**，且必须点名该文件；
- 种一个「只被另一个孤儿引用」+「只被 `__tests__/` 下的文件引用」的组合后**仍然红**，
  报出 2 个孤儿且不把测试侧文件算进去——这两条正是当年 4000 行隐身的两种方式。

`afterEach` 清理 fixture 目录。实测：注释掉 fixture 就绿，种下去就红，不是摆设。

---

## 5. 验收结果（全绿，实际输出）

```
########## bun run lint ##########
Checked 494 files in 136ms. No fixes applied.        # exit 0

########## bun run build ##########
✓ built in 20.11s                                     # tsc 无 error

########## bun run test ##########
 Test Files  93 passed (93)
      Tests  703 passed (703)

########## check_ts_orphan_modules.sh ##########
summary: 0 new orphan module(s) (0 lines), 0 baselined orphan(s), 0 stale baseline entry/entries
         (250 .ts/.tsx files checked from 13 entry point(s); 451 files reachable).
✓ No new TS orphan modules — every src/features file is reachable from an entry point.

########## check_i18n_hardcoded.sh ##########
summary: 44 hardcoded CJK code lines across baselined+new files, 0 new violations,
         12 baselined debt, 0 stale baseline entries.
✓ No new hardcoded CJK strings.

########## check_file_size.sh ##########
summary: 0 new over-limit, 1 baselined debt, 2 oversized test file(s) under cap, 0 stale baseline entries.
✓ No new over-limit files.

########## check_feature_imports.sh ##########
summary: 0 new cross-feature imports, 0 baselined debt, 0 stale baseline entries.
✓ No new cross-feature imports.
```

手工验收：

```
$ ls src/features/models/components/hub/prototype
ls: src/features/models/components/hub/prototype: No such file or directory

$ cargo check --workspace --locked
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.86s

$ git status --porcelain crates/ src-tauri/
 M crates/skillstar-skills/src/repo_scanner/ops.rs          ← 开工前就脏，未碰
 M crates/skillstar-skills/src/skill_update/tests/source_dropped.rs  ← 同上
```

**真实渲染验证**：`vite preview` 起生产构建 + headless Chrome 经 CDP 驱动（绕过 SplashScreen 的
localStorage 计时器），`#models` 完整渲染出矩阵 —— icon carousel、六个 Agent 列头
（Claude CLI / Claude Desktop / Codex / OpenCode / Pi / Oh My Pi）、Provider 表头、空态提示全部正常，
**page error 与 console.error 均为 0**。新接入 i18n 的 `Provider × Agent` 标题与 `Provider` 表头
在截图里确实按 locale 渲染。（浏览器里那条「无法调用后端」toast 是脱离 Tauri 时的预期行为。）

补充：把门禁的扫描面临时放大到整个 `src/`（`SCAN_ROOT=src ... --report`），除 features 外只剩
`src/test/setup.ts`（vitest setupFiles，由 vite.config.ts 引用）与 `src/vite-env.d.ts` 两个
天然无 import 入口的文件 —— 说明 `src/` 里没有别的死代码在藏。

---

## 6. 交给下一个包的三件事

1. **`EditorPage` 的 `SectionsEditor` / `SplitEditor` 已不可达**（分歧 3.8）。WP-4 删 `detailStyle`
   三态时一并清掉，约 150 行 + 一批裸英文会随之消失。
2. **`EditorPage` overlay 正文的约 30 处裸英文**（分歧 3.9）随 WP-4 的重写解决，不必单独排期。
3. **门禁的扫描面目前只有 `src/features/`**（按 §4.7 的原文）。`PrototypeSwitcher` 这次是靠人发现的。
   如果要覆盖 `src/components/` 与 `src/lib/`，脚本已经支持 `SCAN_ROOT=src`，
   今天在那个范围下只有两个合理豁免（见上），随时可以收紧。

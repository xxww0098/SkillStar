import { View, computed, type JSX } from "nsh";
import { t } from "../../services/translation";
import { PageBar, BarCount } from "../../chrome/toolbar";
import { DETAIL_COLUMN_W } from "../../layout";
import {
  allAttentionCount,
  currentRepos,
  currentScoped,
  enabledProfiles,
  localSkillCount,
  onlyUpdates,
  pageActive,
  pendingUpdateCount,
  repoFilter,
  scope,
  selectedSkill,
  sourceFilter,
} from "./state";
import { needsAttention } from "./filters";
import {
  AgentFilterStrip,
  AttentionPill,
  ImportButton,
  RefreshButton,
  ScopeSwitch,
  SearchField,
  ViewToggle,
} from "./toolbar";
import { OriginMenuButton } from "./originMenu";
import { SortMenuButton } from "./sortMenu";
import { SkillsCanvas } from "./canvas";
import { DetailDrawerTrack } from "./detailDrawerSlot";
import { SkillReaderHost } from "./skillReaderSlot";
import { ImportModalHost } from "./importModalSlot";
import { PublishModalHost } from "./publishModal";

// 我的技能页 — my_skills/mod.rs Render 对位：一条工具栏 + 页体行。
//
// 工具栏（toolbar.rs render_toolbar）：标题 + scope 两态滑块（本地 /
// 共享频道），搜索，agent 筛选条带（有已启用 agent 时），来源/仓库菜
// 单（有本地技能、有仓库来源或正在筛选时；否则计数 chip），排序菜单，
// attention 胶囊（全库有待处理时），导入（导入切片预注册），刷新并检
// 查更新，网格/列表切换。共享频道 scope 只留标题 + 滑块 + 搜索：频道
// 自己的筛选与动作不在本页工具栏。
//
// 页体行：画布（flex-1）+ 详情列固定轨道（selectedSkill 非空时占
// DETAIL_COLUMN_W）——轨道一出现，画布 onLayout 实测宽即缩 DETAIL_
// COLUMN_W，网格列数同帧退一（硬关系：默认窗宽关 3 列/开 2 列，layout
// 测试锁）。
//
// nsh 组件体只执行一次（Solid 式语义）：PageBar 的 filters/actions 数组
// 在本体内一次性构建（稳定数组），一切动态量都在各元素自己的 JSX 表达
// 式里——条件成员用 <If>（deck/marketplace 页同款纪律）；传给子组件的
// 动态 prop 是 getter，子组件必须在 JSX 里读（props 解构会冻死）。

export function MySkillsPage(): JSX.Element {
  const showOriginMenu = computed(
    () =>
      localSkillCount() > 0 ||
      currentRepos().length > 0 ||
      sourceFilter() !== "all" ||
      repoFilter() !== null,
  );
  const gatedSelected = computed<string | null>((prev) =>
    !pageActive() && prev !== undefined ? prev : selectedSkill(),
  );
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        width: "100%",
      }}
    >
      <PageBar
        title={t("sidebar.skills")}
        dragId="my-skills-toolbar-drag"
        titleExtras={[<ScopeSwitch />]}
        search={
          <If when={scope() === "local"}>
            <SearchField />
          </If>
        }
        filters={[
          <If when={scope() === "local" && enabledProfiles().length > 0}>
            <AgentFilterStrip />
          </If>,
          <If when={scope() === "local"}>
            <If
              when={showOriginMenu()}
              else={
                <View testID="my-skills-count-chip">
                  <BarCount glyph="layers" label={String(originCount())} />
                </View>
              }
            >
              <OriginMenuButton />
            </If>
          </If>,
          <If when={scope() === "local"}>
            <SortMenuButton />
          </If>,
        ]}
        actions={[
          <If when={scope() === "local" && (allAttentionCount() > 0 || onlyUpdates())}>
            <AttentionPill
              attention={scopedAttentionCount()}
              pending={pendingUpdateCount()}
            />
          </If>,
          <If when={scope() === "local"}>
            <ImportButton />
          </If>,
          <If when={scope() === "local"}>
            <RefreshButton />
          </If>,
          <If when={scope() === "local"}>
            <ViewToggle />
          </If>,
        ]}
      />
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          flex: 1,
          minHeight: 0,
          minWidth: 0,
          width: "100%",
        }}
      >
        <View
          style={{
            position: "relative",
            display: "flex",
            flexDirection: "column",
            flex: 1,
            minHeight: 0,
            minWidth: 0,
          }}
        >
          <SkillsCanvas />
        </View>
        <View
          style={{
            width: scope() === "local" && gatedSelected() !== null ? DETAIL_COLUMN_W : 0,
            flexShrink: 0,
            height: "100%",
            overflow: "hidden",
            display: "flex",
            flexDirection: "column",
          }}
        >
          <DetailDrawerTrack />
        </View>
      </View>
      {/* SKILL.md 阅读器浮层（skillReaderSlot 切片；readerOpen 驱动）。 */}
      <SkillReaderHost />
      {/* 导入对话框浮层（importModalSlot 切片；importModalOpen 驱动）。 */}
      <ImportModalHost />
      {/* 发布本地技能到 GitHub 对话框。 */}
      <PublishModalHost />
    </View>
  );
}

/** 计数口径（toolbar.rs filtered_count）：only_updates 时是 scoped 中的
 * 待处理数，否则 scoped 总数。 */
function originCount(): number {
  const scoped = currentScoped();
  return onlyUpdates() ? scoped.filter(needsAttention).length : scoped.length;
}

/** attention 胶囊的筛选半计数（toolbar.rs attention：scoped 待处理数）。 */
function scopedAttentionCount(): number {
  return currentScoped().filter(needsAttention).length;
}

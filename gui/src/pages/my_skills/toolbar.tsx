import { Scroll, Text, View, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { AgentIcon } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { IconSpin } from "../../chrome/feedback";
import { SliderSegmented, type SegmentOption, type SliderGeometry } from "../../chrome/segmented";
import {
  BAR_FONT,
  BAR_GAP,
  BAR_H,
  BAR_ICON,
  BAR_PAD_X,
  BAR_RADIUS,
  BAR_SLOT_GAP,
  BAR_SLOT_H,
  BAR_SLOT_PAD,
  BAR_SLOT_RADIUS,
  BAR_SLOT_W,
  BarIconButton,
  BarSearchInput,
  BarSegmentSlot,
  BarViewToggle,
} from "../../chrome/toolbar";
import { openImportModal } from "./importModalSlot";
import {
  agentFilter,
  busy,
  enabledProfiles,
  manualChecking,
  onlyUpdates,
  refreshAndCheck,
  scope,
  searchQuery,
  setScope,
  setViewList,
  updateAllPending,
  viewList,
} from "./state";
import type { AgentProfileRecord, MySkillsScope } from "./filters";

// 我的技能工具栏 — 浮岛式统一工具栏（accounts/marketplace/skill_cards 同款
// 46px 卡片带 + BAR_* 控件尺度）：标题 + scope 滑块（本地 / 共享频道），
// 共享搜索框，agent 品牌 SVG 筛选条，来源/排序筛选，attention 胶囊，
// 导入，刷新并检查更新，网格/列表切换。尺寸全部取自 chrome/toolbar.tsx。
//
// nsh 组件体只执行一次：一切状态读取落在 JSX 表达式（含本文件内以函数
// 调用形式挂在 JSX 属性/子节点上的派生读取）。

// 滑块沿用工具条尺度：32 高轨道（26 槽 + 2 边距 + 1 边框）、10 轨道圆角、
// 7 槽圆角、14 字形——与右侧的视图切换、计数 chip 同框同圆角。
const SCOPE_GEO: SliderGeometry = {
  slotW: 74,
  slotH: BAR_SLOT_H,
  pad: BAR_SLOT_PAD,
  iconSize: BAR_ICON,
  slotRadius: BAR_SLOT_RADIUS,
  trackRadius: BAR_RADIUS,
};

/** 带图标与双语标签的 Local / Channels 滑块。 */
export function ScopeSwitch(): JSX.Element {
  const segments: SegmentOption[] = [
    { id: "my-skills-scope-local", icon: "laptop", label: () => t("mySkills.scopeLocal") },
    { id: "my-skills-scope-channels", icon: "users", label: () => t("mySkills.scopeChannels") },
  ];
  return (
    <SliderSegmented
      motionId="my-skills-scope-motion"
      options={segments}
      geo={SCOPE_GEO}
      selected={() => scopeIndex()}
      onClick={(ix) => setScope(scopeFromIndex(ix))}
    />
  );
}

function scopeIndex(): number {
  return scope() === "channels" ? 1 : 0;
}

function scopeFromIndex(ix: number): MySkillsScope {
  return ix === 1 ? "channels" : "local";
}

/** 药丸搜索框（放大镜 + 即时清除按钮，输入即筛）。 */
export function SearchField(): JSX.Element {
  return (
    <BarSearchInput
      boxTestID="my-skills-search"
      inputTestID="my-skills-search-input"
      clearTestID="my-skills-search-clear"
      value={searchQuery()}
      placeholder={t("toolbar.searchPlaceholder")}
      onChange={(next) => searchQuery.set(next)}
    />
  );
}

// ---- agent 筛选条带（toolbar.rs agent_filter_bar） -------------------------

const AGENT_FILTER_SLOT_W = BAR_SLOT_W;
const AGENT_FILTER_GAP = 2;
const AGENT_FILTER_MAX_VISIBLE = 6;
const AGENT_FILTER_LANE_W =
  AGENT_FILTER_MAX_VISIBLE * AGENT_FILTER_SLOT_W +
  (AGENT_FILTER_MAX_VISIBLE - 1) * AGENT_FILTER_GAP;

export function AgentFilterStrip(): JSX.Element {
  return (
    <View
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: BAR_H,
        flexShrink: 0,
        borderRadius: BAR_RADIUS,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        padding: BAR_SLOT_PAD,
        gap: BAR_SLOT_GAP,
      }}
    >
      <BarSegmentSlot
        testID="agent-filter-all"
        label={t("toolbar.all")}
        selected={agentFilter() === null}
        tone="filter"
        onPress={() => agentFilter.set(null)}
      />
      <If
        when={enabledProfiles().length > AGENT_FILTER_MAX_VISIBLE}
        else={
          <View
            testID="agent-filter-lane"
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              height: "100%",
              gap: AGENT_FILTER_GAP,
            }}
          >
            <AgentFilterItems />
          </View>
        }
      >
        <Scroll
          testID="agent-filter-lane"
          horizontal
          scrollbar={false}
          bounces={false}
          style={{
            width: AGENT_FILTER_LANE_W,
            height: "100%",
            flexShrink: 0,
          }}
          contentStyle={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            height: "100%",
            gap: AGENT_FILTER_GAP,
          }}
        >
          <AgentFilterItems />
        </Scroll>
      </If>
    </View>
  );
}

function AgentFilterItems(): JSX.Element {
  return (
    <Each
      of={enabledProfiles()}
      key={(profile: AgentProfileRecord) => profile.id}
    >
      {(profile: () => AgentProfileRecord) => (
        <AgentGlyph
          id={profile().id}
          name={profile().displayName}
          icon={profile().icon}
          active={agentFilter() === profile().id}
        />
      )}
    </Each>
  );
}

/** agent 品牌 SVG 筛选槽；再点同槽清除筛选。 */
function AgentGlyph(props: {
  id: string;
  name: string;
  icon?: string;
  active: boolean;
}): JSX.Element {
  return (
    <View
      testID={`agent-filter-${props.id}`}
      accessibilityLabel={props.name}
      tooltip={<Tip text={props.name} />}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: AGENT_FILTER_SLOT_W,
        height: AGENT_FILTER_SLOT_W,
        borderRadius: BAR_SLOT_RADIUS,
        flexShrink: 0,
        cursor: "pointer",
        backgroundColor: props.active ? paletteToken("accent_soft") : "transparent",
      }}
      hoverStyle={props.active ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      onPress={() => agentFilter.set(agentFilter() === props.id ? null : props.id)}
    >
      <AgentIcon
        id={props.id}
        icon={props.icon}
        size={BAR_ICON}
        grayscale={false}
        opacity={props.active ? 1 : 0.6}
      />
    </View>
  );
}

// ---- attention 胶囊（toolbar.rs render_attention） --------------------------

/** warn 胶囊两段：筛选半（glyph + 计数，按下换 X + warn_hover 底）与
 * 实心更新半（待更新数 > 0 时出现，busy token = update_all 时 0.8 透明
 * + 旋转 + 「更新中」）。 */
export function AttentionPill(props: { attention: number; pending: number }): JSX.Element {
  return (
    <View
      testID="my-skills-attention"
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: BAR_H,
        flexShrink: 0,
        borderRadius: BAR_RADIUS,
        borderWidth: 1,
        borderColor: paletteToken("warn_border"),
        backgroundColor: paletteToken("warn_bg"),
        padding: BAR_SLOT_PAD,
        gap: BAR_SLOT_GAP,
      }}
    >
      <View
        testID="filter-updates-only"
        tooltip={<Tip text={onlyUpdates() ? t("toolbar.showAllSkills") : t("toolbar.filterUpdatesHint")} />}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          height: "100%",
          gap: BAR_GAP,
          paddingHorizontal: BAR_PAD_X,
          borderRadius: BAR_SLOT_RADIUS,
          cursor: "pointer",
          backgroundColor: onlyUpdates() ? paletteToken("warn_hover") : "transparent",
        }}
        hoverStyle={{ backgroundColor: paletteToken("warn_hover") }}
        onPress={() => onlyUpdates.set(!onlyUpdates())}
      >
        <Icon
          name={onlyUpdates() ? "x" : "listFilter"}
          size={BAR_ICON}
          color={paletteToken("warn")}
        />
        <Text style={{ fontSize: BAR_FONT, fontWeight: "600", color: paletteToken("warn") }}>
          {props.attention}
        </Text>
      </View>
      <If when={props.pending > 0}>
        <View
          testID="my-skills-update-all"
          tooltip={
            <Tip
              text={
                busy() === "update_all"
                  ? t("common.updating")
                  : tf("toolbar.updateAllHint", { count: props.pending })
              }
            />
          }
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            height: "100%",
            gap: BAR_GAP,
            paddingHorizontal: BAR_PAD_X,
            borderRadius: BAR_SLOT_RADIUS,
            cursor: "pointer",
            opacity: busy() === "update_all" ? 0.8 : 1,
            backgroundColor: paletteToken("warn"),
          }}
          hoverStyle={{ opacity: 0.9 }}
          onPress={() => updateAllPending()}
        >
          <IconSpin
            name="circleArrowUp"
            size={BAR_ICON}
            color={paletteToken("on_warn")}
            spin={busy() === "update_all"}
          />
          <Text style={{ fontSize: BAR_FONT, fontWeight: "600", color: paletteToken("on_warn") }}>
            {t("common.update")}
          </Text>
        </View>
      </If>
    </View>
  );
}

// ---- 视图切换 ---------------------------------------------------------------

export function ViewToggle(): JSX.Element {
  return (
    <BarViewToggle
      testID="my-skills-view"
      gridTestID="my-skills-view-grid"
      listTestID="my-skills-view-list"
      listMode={viewList()}
      onSelect={setView}
    />
  );
}

function setView(list: boolean): void {
  setViewList(list);
}

// ---- 导入与刷新 -------------------------------------------------------------

export function ImportButton(): JSX.Element {
  return (
    <BarIconButton
      testID="my-skills-import"
      glyph="download"
      tip={t("toolbar.import")}
      onPress={() => openImportModal()}
    />
  );
}

export function RefreshButton(): JSX.Element {
  return (
    <BarIconButton
      testID="my-skills-refresh"
      glyph="refresh"
      loading={manualChecking()}
      tip={
        manualChecking() ? t("toolbar.checkingUpdates") : t("toolbar.refreshAndCheck")
      }
      onPress={refreshAndCheck}
    />
  );
}

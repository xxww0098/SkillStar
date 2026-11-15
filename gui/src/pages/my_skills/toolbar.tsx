import { Scroll, Text, View, type JSX } from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { AgentIcon } from "../../chrome/agentRail";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { IconSpin } from "../../chrome/feedback";
import { SliderSegmented, type SegmentOption, type SliderGeometry } from "../../chrome/segmented";
import { BarIconButton, BarSearchInput, BarViewToggle } from "../../chrome/toolbar";
import { openImportModal } from "./importModalSlot";
import {
  agentFilter,
  busy,
  checkingUpdates,
  enabledProfiles,
  loading,
  onlyUpdates,
  refreshAndCheck,
  scope,
  searchQuery,
  setViewList,
  updateAllPending,
  viewList,
} from "./state";
import type { AgentProfileRecord, MySkillsScope } from "./filters";

// 我的技能工具栏 — 浮岛式统一工具栏（accounts/marketplace/skill_cards 同款
// 46px 卡片带 + 10px 边框控件）：标题 + scope 胶囊滑块（本地 / 共享频道），
// 药丸搜索框，agent 品牌 SVG 筛选条，来源/排序筛选，attention 胶囊，
// 导入，刷新并检查更新，网格/列表切换。
//
// nsh 组件体只执行一次：一切状态读取落在 JSX 表达式（含本文件内以函数
// 调用形式挂在 JSX 属性/子节点上的派生读取）。

const SCOPE_GEO: SliderGeometry = { slotW: 74, slotH: 26, pad: 2, iconSize: 13 };

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
      onClick={(ix) => scope.set(scopeFromIndex(ix))}
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
      width={168}
    />
  );
}

// ---- agent 筛选条带（toolbar.rs agent_filter_bar） -------------------------

const AGENT_FILTER_SLOT_W = 26;
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
        height: 32,
        flexShrink: 0,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        padding: 2,
        gap: 2,
      }}
    >
      <CompactTab
        testID="agent-filter-all"
        label={t("toolbar.all")}
        active={agentFilter() === null}
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

function CompactTab(props: {
  testID: string;
  label: string;
  active: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 26,
        paddingHorizontal: 9,
        borderRadius: 7,
        cursor: "pointer",
        backgroundColor: props.active ? paletteToken("accent_soft") : "transparent",
      }}
      hoverStyle={props.active ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 12,
          fontWeight: props.active ? "600" : "500",
          color: props.active ? paletteToken("accent_fg") : paletteToken("fg_muted"),
        }}
      >
        {props.label}
      </Text>
    </View>
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
        borderRadius: 7,
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
        size={14}
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
        height: 32,
        flexShrink: 0,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("warn_border"),
        backgroundColor: paletteToken("warn_bg"),
        padding: 2,
        gap: 2,
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
          gap: 6,
          paddingHorizontal: 8,
          borderRadius: 7,
          cursor: "pointer",
          backgroundColor: onlyUpdates() ? paletteToken("warn_hover") : "transparent",
        }}
        hoverStyle={{ backgroundColor: paletteToken("warn_hover") }}
        onPress={() => onlyUpdates.set(!onlyUpdates())}
      >
        <Icon
          name={onlyUpdates() ? "x" : "listFilter"}
          size={14}
          color={paletteToken("warn")}
        />
        <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("warn") }}>
          {props.attention}
        </Text>
      </View>
      {(() => {
        if (props.pending === 0) return null;
        const updating = busy() === "update_all";
        return (
          <View
            testID="my-skills-update-all"
            tooltip={
              <Tip
                text={
                  updating
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
              gap: 6,
              paddingHorizontal: 8,
              borderRadius: 7,
              cursor: "pointer",
              opacity: updating ? 0.8 : 1,
              backgroundColor: paletteToken("warn"),
            }}
            hoverStyle={{ opacity: 0.9 }}
            onPress={() => updateAllPending()}
          >
            <IconSpin
              name="circleArrowUp"
              size={14}
              color={paletteToken("on_warn")}
              spin={updating}
            />
            <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("on_warn") }}>
              {t("common.update")}
            </Text>
          </View>
        );
      })()}
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
      loading={checkingUpdates() || loading()}
      tip={
        checkingUpdates() || loading()
          ? t("toolbar.checkingUpdates")
          : t("toolbar.refreshAndCheck")
      }
      onPress={refreshAndCheck}
    />
  );
}

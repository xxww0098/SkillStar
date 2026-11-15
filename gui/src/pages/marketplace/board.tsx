import {
  List,
  Text,
  View,
  computed,
  effect,
  signal,
  type JSX,
  type ListRef,
} from "nsh";
import { paletteToken } from "../../services/themeMode";
import {
  CARD_GAP,
  CARD_ROW_W,
  DETAIL_COLUMN_W,
  PAGE_PAD,
  SKILL_CARD_H,
  SKILL_SCROLL_PB,
  SKILL_SCROLL_PT,
} from "../../layout";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { cardTrackWidth, gridColumnsForPane, packCardRows } from "../../chrome/cardShell";
import { BackToTopButton, SHOW_AFTER_PX } from "../../chrome/scrollTop";
import { t } from "../../services/translation";
import { MarketCard, MarketSkeletonTile, PublisherCard } from "./card";
import { mt, mtf } from "./marketStrings";
import type { MarketSkill, MarketTabId, PublisherRecord } from "./types";

// 市场板面 — board.rs + view.rs 的板区对位。
//
// 技能榜与官方发布者列表均走 nsh `<List>` 行虚拟化，仅构建视口附近的行，
// 滚动偏移只驱动 backTopShown 信号、绝不进入渲染函数。列数由 onLayout
// 实测宽度驱动（与 my_skills 同款 SKILL_CARD_W 固定轨道）。

const LIST_GAP = 8;
const PUBLISHER_ROW_H = 76;

export interface BoardProps {
  loading: boolean;
  tab: MarketTabId;
  skills: MarketSkill[];
  publishers: PublisherRecord[];
  query: string;
  viewList: boolean;
  busy: string | null;
  selectedName: string | null;
  onOpenSkill: (skill: MarketSkill) => void;
  onToggleInstall: (skill: MarketSkill) => void;
  onOpenPublisher: (publisher: PublisherRecord) => void;
  onClearSearch: () => void;
}

export function MarketBoard(props: BoardProps): JSX.Element {
  const paneWidth = signal(0);
  const backTopShown = signal(false);
  const listRef = signal<ListRef | null>(null);

  // tab/视图切换开新数据集：立即回顶（骨架档由 loading 上升标记）。
  effect(() => {
    void props.tab;
    void props.viewList;
    void props.loading;
    backTopShown.set(false);
    queueMicrotask(() => listRef()?.scrollToOffset({ offset: 0, animated: false }));
  });

  const handleScroll = (offsetY: number): void => {
    const shown = offsetY > SHOW_AFTER_PX;
    if (shown !== backTopShown()) backTopShown.set(shown);
  };

  const fallbackW = (): number =>
    props.selectedName !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W;

  return (
    <View
      style={{
        position: "relative",
        display: "flex",
        flex: 1,
        minHeight: 0,
        minWidth: 0,
        width: "100%",
      }}
      onLayout={(event) => {
        const w = event.bounds.size.width;
        if (w > 0 && w !== paneWidth()) paneWidth.set(w);
      }}
    >
      <If when={props.loading}>
        <View testID="mk-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
          <SkeletonBoard cardWidth={cardTrackWidth(paneWidth(), false, PAGE_PAD, fallbackW())} />
        </View>
      </If>
      <If when={!props.loading && props.tab === "official"}>
        <OfficialBoard
          publishers={props.publishers}
          onOpenPublisher={props.onOpenPublisher}
          onRef={(node) => listRef.set(node)}
          onScroll={handleScroll}
        />
      </If>
      <If when={!props.loading && props.tab !== "official" && props.skills.length === 0}>
        <View testID="mk-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
          <EmptyBoard query={props.query} onClearSearch={props.onClearSearch} />
        </View>
      </If>
      <If when={!props.loading && props.tab !== "official" && props.skills.length > 0}>
        <SkillBoard
          skills={props.skills}
          viewList={props.viewList}
          busy={props.busy}
          selectedName={props.selectedName}
          paneWidth={paneWidth()}
          onOpenSkill={props.onOpenSkill}
          onToggleInstall={props.onToggleInstall}
          onRef={(node) => listRef.set(node)}
          onScroll={handleScroll}
        />
      </If>
      <If when={backTopShown() && listRef() !== null}>
        <BackToTopButton
          label={t("mySkills.backToTop")}
          listRef={listRef()}
          onClick={() => backTopShown.set(false)}
        />
      </If>
    </View>
  );
}

/** 骨架：六片与真实卡同轨道（view.rs render_skeletons）。 */
function SkeletonBoard(props: { cardWidth: number }): JSX.Element {
  return (
    <View
      testID="mk-skeletons"
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        gap: CARD_GAP,
        justifyContent: "flex-start",
        alignItems: "flex-start",
        paddingTop: SKILL_SCROLL_PT,
        paddingBottom: SKILL_SCROLL_PB,
        paddingHorizontal: PAGE_PAD,
      }}
    >
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <MarketSkeletonTile index={index} cardWidth={props.cardWidth} />
      ))}
    </View>
  );
}

/** 官方发布者板（card.rs publisher_list）：虚拟化行列表 + 标题行计数 pill。 */
function OfficialBoard(props: {
  publishers: PublisherRecord[];
  onOpenPublisher: (publisher: PublisherRecord) => void;
  onRef: (node: ListRef | null) => void;
  onScroll: (offsetY: number) => void;
}): JSX.Element {
  return (
    <List
      ref={(node) => props.onRef(node)}
      testID="mk-scroll"
      each={props.publishers}
      keyExtractor={(publisher) => publisher.name}
      estimatedItemSize={PUBLISHER_ROW_H + CARD_GAP}
      style={{ width: "100%", height: "100%" }}
      contentStyle={{
        paddingTop: SKILL_SCROLL_PT,
        paddingBottom: SKILL_SCROLL_PB,
        paddingLeft: PAGE_PAD,
        paddingRight: PAGE_PAD,
      }}
      scrollEventThrottle={32}
      onScroll={(event) => props.onScroll(event.offset.y)}
      header={
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 8,
            paddingBottom: 16,
          }}
        >
          <View style={{ display: "flex", flexDirection: "column", gap: 1, minWidth: 0 }}>
            <Text style={{ fontSize: 14, color: paletteToken("fg") }}>
              {mt("marketplace.officialPublishersTitle")}
            </Text>
          </View>
          <View
            testID="mk-publishers-count"
            style={{
              flexShrink: 0,
              paddingHorizontal: 8,
              paddingVertical: 2,
              borderRadius: 999,
              backgroundColor: paletteToken("card"),
              borderWidth: 1,
              borderColor: paletteToken("border"),
            }}
          >
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {mtf("marketplace.publishersCount", { count: props.publishers.length })}
            </Text>
          </View>
        </View>
      }
      empty={<EmptyBoard query="" onClearSearch={() => {}} />}
    >
      {(publisher) => (
        <View style={{ paddingBottom: CARD_GAP, width: "100%" }}>
          <PublisherCard
            publisher={publisher()}
            onPress={() => props.onOpenPublisher(publisher())}
          />
        </View>
      )}
    </List>
  );
}

/** 空态（view.rs render_empty）：搜索空态带清除按钮；分栏空态提示网络。 */
function EmptyBoard(props: { query: string; onClearSearch: () => void }): JSX.Element {
  return (
    <View
      testID="mk-empty"
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        paddingVertical: 48,
        paddingHorizontal: PAGE_PAD,
      }}
    >
      <Icon name="search" size={32} color={paletteToken("fg_muted")} />
      <Text style={{ fontSize: 16, fontWeight: "700", color: paletteToken("fg") }}>
        {props.query.trim().length > 0
          ? mtf("marketplace.emptySearchTitle", { query: props.query.trim() })
          : mt("marketplace.emptySectionTitle")}
      </Text>
      <Text
        style={{
          maxWidth: 360,
          fontSize: 12,
          textAlign: "center",
          color: paletteToken("fg_muted"),
        }}
      >
        {props.query.trim().length > 0
          ? mt("marketplace.emptySearchDescription")
          : mt("marketplace.emptySectionDescription")}
      </Text>
      <If when={props.query.trim().length > 0}>
        <View
          testID="mk-empty-clear"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            height: 28,
            paddingHorizontal: 12,
            borderRadius: 6,
            cursor: "pointer",
            backgroundColor: paletteToken("accent"),
          }}
          hoverStyle={{ backgroundColor: paletteToken("accent_hover") }}
          onPress={props.onClearSearch}
        >
          <Text style={{ fontSize: 12, color: paletteToken("on_accent"), fontWeight: "500" }}>
            {mt("marketplace.clearSearch")}
          </Text>
        </View>
      </If>
    </View>
  );
}

/** 技能板：虚拟化网格（按 paneWidth 打包定宽轨道行）或单列列表。 */
function SkillBoard(props: {
  skills: MarketSkill[];
  viewList: boolean;
  busy: string | null;
  selectedName: string | null;
  paneWidth: number;
  onOpenSkill: (skill: MarketSkill) => void;
  onToggleInstall: (skill: MarketSkill) => void;
  onRef: (node: ListRef | null) => void;
  onScroll: (offsetY: number) => void;
}): JSX.Element {
  const fallbackW = (): number =>
    props.selectedName !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W;
  const packedRows = computed(() =>
    packCardRows(
      props.skills,
      gridColumnsForPane(props.paneWidth, props.viewList, PAGE_PAD, fallbackW())
    )
  );
  const trackW = (): number => cardTrackWidth(props.paneWidth, props.viewList, PAGE_PAD, fallbackW());
  const stride = (): number => SKILL_CARD_H + (props.viewList ? LIST_GAP : CARD_GAP);

  return (
    <List
      ref={(node) => props.onRef(node)}
      testID="mk-scroll"
      each={packedRows()}
      keyExtractor={(_row, idx) => `mk-row-${idx}`}
      estimatedItemSize={stride()}
      style={{ width: "100%", height: "100%" }}
      contentStyle={{
        paddingTop: SKILL_SCROLL_PT,
        paddingBottom: SKILL_SCROLL_PB,
        paddingLeft: PAGE_PAD,
        paddingRight: PAGE_PAD,
      }}
      scrollEventThrottle={32}
      onScroll={(event) => props.onScroll(event.offset.y)}
    >
      {(row, index) => (
        <View
          testID={`mk-row-${index()}`}
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "flex-start",
            gap: props.viewList ? 0 : CARD_GAP,
            height: stride(),
            paddingBottom: props.viewList ? LIST_GAP : CARD_GAP,
            width: "100%",
          }}
        >
          <Each of={row()} key="index">
            {(skill) => (
              <View
                style={
                  props.viewList
                    ? { width: "100%", minWidth: 0 }
                    : {
                        width: trackW(),
                        minWidth: trackW(),
                        maxWidth: trackW(),
                        flexShrink: 0,
                      }
                }
              >
                <MarketCard
                  skill={skill()}
                  selected={props.selectedName === skill().name}
                  busy={props.busy === skill().name}
                  listMode={props.viewList}
                  cardWidth={trackW()}
                  onPress={() => props.onOpenSkill(skill())}
                  onToggleInstall={() => props.onToggleInstall(skill())}
                />
              </View>
            )}
          </Each>
        </View>
      )}
    </List>
  );
}

/** 快照错误横幅（view.rs render_snapshot_banner）：danger 弱底 + 重试
 * 按钮（刷新中转圈文案）。只在 snapshot_error 非空时渲染。 */
export function SnapshotBanner(props: {
  error: string;
  refreshing: boolean;
  onRetry: () => void;
}): JSX.Element {
  return (
    <View
      testID="mk-snapshot-banner"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        paddingHorizontal: 16,
        paddingVertical: 8,
        borderRadius: 10,
        backgroundColor: paletteToken("danger_bg"),
        borderWidth: 1,
        borderColor: paletteToken("danger_border"),
      }}
    >
      <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, minWidth: 0 }}>
        <Icon name="alertTriangle" size={14} color={paletteToken("danger")} />
        <Text style={{ fontSize: 12, color: paletteToken("danger_fg"), numberOfLines: 1 }}>
          {mtf("marketplace.snapshotWarning", { err: props.error })}
        </Text>
      </View>
      <View
        testID="mk-banner-retry"
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 4,
          paddingHorizontal: 8,
          paddingVertical: 4,
          borderRadius: 8,
          cursor: "pointer",
          backgroundColor: paletteToken("card"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
          flexShrink: 0,
        }}
        hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
        onPress={props.onRetry}
      >
        <IconSpin
          name="refresh"
          size={11}
          color={paletteToken("fg")}
          spin={props.refreshing}
        />
        <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
          {props.refreshing ? mt("marketplace.retrying") : mt("common.retry")}
        </Text>
      </View>
    </View>
  );
}

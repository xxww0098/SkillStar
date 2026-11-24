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
import { bindDelayedFlag } from "../../chrome/delayedFlag";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { cardTrackWidth, gridColumnsForPane, packCardRows } from "../../chrome/cardShell";
import { BackToTopButton, SHOW_AFTER_PX } from "../../chrome/scrollTop";
import { createPaneWidth } from "../../shell/nav";
import { t } from "../../services/translation";
import { MarketCard, MarketSkeletonTile, PublisherCard } from "./card";
import { mt, mtf } from "./marketStrings";
import type {
  MarketSkill,
  MarketTabId,
  PublisherRecord,
  SnapshotErrorInfo,
  SnapshotStatusTag,
} from "./types";

// 市场板面 — board.rs + view.rs 的板区对位。
//
// 技能榜与官方发布者列表均走 nsh `<List>` 行虚拟化，仅构建视口附近的行，
// 滚动偏移只驱动 backTopShown 信号、绝不进入渲染函数。列数由 onLayout
// 实测宽度驱动（与 my_skills 同款 SKILL_CARD_W 固定轨道）。

const LIST_GAP = 8;
const PUBLISHER_ROW_H = 76;

export interface BoardProps {
  active?: () => boolean;
  loading: boolean;
  snapshotStatus?: SnapshotStatusTag;
  refreshing?: boolean;
  tab: MarketTabId;
  skills: MarketSkill[];
  publishers: PublisherRecord[];
  query: string;
  viewList: boolean;
  /** 在飞的安装/卸载（技能名）；每张卡只认自己的名字。 */
  busy: ReadonlySet<string>;
  selectedName: string | null;
  onOpenSkill: (skill: MarketSkill) => void;
  onToggleInstall: (skill: MarketSkill) => void;
  onOpenPublisher: (publisher: PublisherRecord) => void;
  onClearSearch: () => void;
  onSyncSnapshot?: () => void;
}

export function MarketBoard(props: BoardProps): JSX.Element {
  const pane = createPaneWidth({
    active: props.active,
    fromShell: (panelW) => panelW - (props.selectedName !== null ? DETAIL_COLUMN_W : 0),
  });
  const paneWidth = pane.width;
  const backTopShown = signal(false);
  const officialListRef = signal<ListRef | null>(null);
  const skillListRef = signal<ListRef | null>(null);
  const visitedOfficial = signal(props.tab === "official");
  const visitedSkills = signal(props.tab !== "official");

  const isEmptyData = (): boolean =>
    props.skills.length === 0 && props.publishers.length === 0;
  const isSeedingEmpty = (): boolean =>
    props.snapshotStatus === "seeding" && isEmptyData();

  // 两边都空的加载晚 300ms 出骨架；seeding 且无数据时始终保持骨架而不闪空态。
  const skeletonReady = bindDelayedFlag(
    () => props.loading && isEmptyData(),
  );

  effect(() => {
    if (props.tab === "official") {
      if (!visitedOfficial()) visitedOfficial.set(true);
    } else if (!visitedSkills()) {
      visitedSkills.set(true);
    }
  });

  const activeListRef = (): ListRef | null =>
    props.tab === "official" ? officialListRef() : skillListRef();

  // tab/视图切换开新数据集：立即回顶。已有行的刷新不升 loading，不回顶。
  effect(() => {
    void props.tab;
    void props.viewList;
    backTopShown.set(false);
    queueMicrotask(() => activeListRef()?.scrollToOffset({ offset: 0, animated: false }));
  });

  const handleScroll = (offsetY: number): void => {
    const shown = offsetY > SHOW_AFTER_PX;
    if (shown !== backTopShown()) backTopShown.set(shown);
  };

  const fallbackW = (): number =>
    props.selectedName !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W;

  const showSkeleton = (): boolean =>
    isSeedingEmpty() || (skeletonReady() && props.loading && isEmptyData());
  const showOfficial = (): boolean =>
    props.tab === "official" &&
    (props.publishers.length > 0 || (!props.loading && props.snapshotStatus !== "seeding"));
  const showEmptySkills = (): boolean =>
    !props.loading &&
    props.snapshotStatus !== "seeding" &&
    props.tab !== "official" &&
    props.skills.length === 0;
  const showSkills = (): boolean => props.tab !== "official" && props.skills.length > 0;

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
      onLayout={pane.onLayout}
    >
      <If when={showSkeleton()}>
        <View testID="mk-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
          <SkeletonBoard cardWidth={cardTrackWidth(paneWidth(), false, PAGE_PAD, fallbackW())} />
        </View>
      </If>
      <If when={visitedOfficial()}>
        <View
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            display: showOfficial() ? "flex" : "none",
          }}
        >
          <OfficialBoard
            publishers={props.publishers}
            snapshotStatus={props.snapshotStatus ?? "fresh"}
            refreshing={props.refreshing ?? false}
            onSyncSnapshot={props.onSyncSnapshot ?? (() => {})}
            onOpenPublisher={props.onOpenPublisher}
            onRef={(node) => officialListRef.set(node)}
            onScroll={handleScroll}
          />
        </View>
      </If>
      <If when={showEmptySkills()}>
        <View testID="mk-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
          <EmptyBoard
            query={props.query}
            snapshotStatus={props.snapshotStatus ?? "fresh"}
            refreshing={props.refreshing ?? false}
            onClearSearch={props.onClearSearch}
            onSync={props.onSyncSnapshot ?? (() => {})}
          />
        </View>
      </If>
      <If when={visitedSkills()}>
        <View
          style={{
            position: "absolute",
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            display: showSkills() ? "flex" : "none",
          }}
        >
          <SkillBoard
            skills={props.skills}
            viewList={props.viewList}
            busy={props.busy}
            selectedName={props.selectedName}
            paneWidth={paneWidth()}
            onOpenSkill={props.onOpenSkill}
            onToggleInstall={props.onToggleInstall}
            onRef={(node) => skillListRef.set(node)}
            onScroll={handleScroll}
          />
        </View>
      </If>
      <If when={backTopShown() && activeListRef() !== null}>
        <BackToTopButton
          label={t("mySkills.backToTop")}
          listRef={activeListRef()}
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
      accessibilityState={{ busy: true }}
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
  snapshotStatus: SnapshotStatusTag;
  refreshing: boolean;
  onSyncSnapshot: () => void;
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
      empty={
        <EmptyBoard
          query=""
          snapshotStatus={props.snapshotStatus}
          refreshing={props.refreshing}
          onClearSearch={() => {}}
          onSync={props.onSyncSnapshot}
        />
      }
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

/** 空态（view.rs render_empty）：搜索空态带清除按钮；miss / remote_error 给立即同步/重试。 */
export function EmptyBoard(props: {
  query: string;
  snapshotStatus: SnapshotStatusTag;
  refreshing: boolean;
  onClearSearch: () => void;
  onSync: () => void;
}): JSX.Element {
  const showSyncAction = (): boolean =>
    props.query.trim().length === 0 &&
    (props.snapshotStatus === "miss" || props.snapshotStatus === "remote_error");
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
          : props.snapshotStatus === "miss"
            ? mt("marketplace.snapshotMiss")
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
      <If when={showSyncAction()}>
        <View
          testID="mk-empty-sync"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            height: 28,
            paddingHorizontal: 12,
            borderRadius: 6,
            cursor: props.refreshing ? "default" : "pointer",
            backgroundColor: paletteToken("accent"),
            opacity: props.refreshing ? 0.7 : 1,
          }}
          hoverStyle={props.refreshing ? undefined : { backgroundColor: paletteToken("accent_hover") }}
          onPress={() => {
            if (!props.refreshing) props.onSync();
          }}
        >
          <IconSpin
            name="refresh"
            size={12}
            color={paletteToken("on_accent")}
            spin={props.refreshing}
          />
          <Text style={{ fontSize: 12, color: paletteToken("on_accent"), fontWeight: "500" }}>
            {props.refreshing
              ? mt("marketplace.retrying")
              : props.snapshotStatus === "miss"
                ? mt("marketplace.syncNow")
                : mt("common.retry")}
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
  /** 在飞的安装/卸载（技能名）；每张卡只认自己的名字。 */
  busy: ReadonlySet<string>;
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
      keyExtractor={(row) => row.map((s) => `${s.source ?? s.gitUrl}\u001f${s.name}`).join("\u001e")}
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
              <MarketCardSlot
                skill={skill}
                viewList={() => props.viewList}
                trackW={trackW}
                selectedName={() => props.selectedName}
                busyNames={() => props.busy}
                onOpenSkill={props.onOpenSkill}
                onToggleInstall={props.onToggleInstall}
              />
            )}
          </Each>
        </View>
      )}
    </List>
  );
}

function MarketCardSlot(props: {
  skill: () => MarketSkill;
  viewList: () => boolean;
  trackW: () => number;
  selectedName: () => string | null;
  busyNames: () => ReadonlySet<string>;
  onOpenSkill: (skill: MarketSkill) => void;
  onToggleInstall: (skill: MarketSkill) => void;
}): JSX.Element {
  const selected = computed(() => props.selectedName() === props.skill().name);
  const busy = computed(() => props.busyNames().has(props.skill().name));
  return (
    <View
      style={
        props.viewList()
          ? { width: "100%", minWidth: 0 }
          : {
              width: props.trackW(),
              minWidth: props.trackW(),
              maxWidth: props.trackW(),
              flexShrink: 0,
            }
      }
    >
      <MarketCard
        skill={props.skill()}
        selected={selected()}
        busy={busy()}
        listMode={props.viewList()}
        cardWidth={props.trackW()}
        onPress={() => props.onOpenSkill(props.skill())}
        onToggleInstall={() => props.onToggleInstall(props.skill())}
      />
    </View>
  );
}

/** 快照状态横幅：error_fallback 用警告样式（warn），remote_error 用 danger，
 * 支持可折叠 detail 详情查看。 */
export function SnapshotBanner(props: {
  error: SnapshotErrorInfo;
  refreshing: boolean;
  onRetry: () => void;
}): JSX.Element {
  const detailOpen = signal(false);
  const isWarn = (): boolean => props.error.kind === "error_fallback";
  const headline = (): string =>
    isWarn()
      ? mt("marketplace.snapshotErrorFallback")
      : mtf("marketplace.snapshotWarning", { err: props.error.detail });

  return (
    <View
      testID="mk-snapshot-banner"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        paddingHorizontal: 16,
        paddingVertical: 8,
        borderRadius: 10,
        backgroundColor: isWarn() ? paletteToken("warn_bg") : paletteToken("danger_bg"),
        borderWidth: 1,
        borderColor: isWarn() ? paletteToken("warn_border") : paletteToken("danger_border"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
        }}
      >
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, minWidth: 0, flex: 1 }}>
          <Icon
            name="alertTriangle"
            size={14}
            color={isWarn() ? paletteToken("warn") : paletteToken("danger")}
          />
          <Text
            style={{
              fontSize: 12,
              color: isWarn() ? paletteToken("warn") : paletteToken("danger_fg"),
              numberOfLines: 1,
            }}
          >
            {headline()}
          </Text>
        </View>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 6, flexShrink: 0 }}>
          <If when={props.error.detail.trim().length > 0}>
            <View
              testID="mk-banner-detail-toggle"
              style={{
                paddingHorizontal: 8,
                paddingVertical: 4,
                borderRadius: 8,
                cursor: "pointer",
                borderWidth: 1,
                borderColor: paletteToken("border"),
                backgroundColor: paletteToken("card"),
              }}
              hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
              onPress={() => detailOpen.set(!detailOpen())}
            >
              <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                {detailOpen() ? mt("marketplace.hideErrorDetails") : mt("marketplace.viewErrorDetails")}
              </Text>
            </View>
          </If>
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
      </View>
      <If when={detailOpen()}>
        <View
          testID="mk-banner-detail"
          style={{
            paddingHorizontal: 10,
            paddingVertical: 6,
            borderRadius: 6,
            backgroundColor: paletteToken("well"),
          }}
        >
          <Text style={{ fontSize: 11, fontFamily: "monospace", color: paletteToken("fg_muted") }}>
            {props.error.scope ? `[${props.error.scope}] ${props.error.detail}` : props.error.detail}
          </Text>
        </View>
      </If>
    </View>
  );
}

import {
  List,
  View,
  computed,
  effect,
  signal,
  type JSX,
  type ListRef,
} from "nsh";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { Pulse } from "../../chrome/feedback";
import { BackToTopButton, SHOW_AFTER_PX } from "../../chrome/scrollTop";
import { cardTrackWidth, gridColumnsForPane, packCardRows } from "../../chrome/cardShell";
import {
  CARD_GAP,
  CARD_ROW_W,
  DETAIL_COLUMN_W,
  PAGE_PAD,
  SKILL_CARD_H,
  SKILL_SCROLL_PB,
  SKILL_SCROLL_PT,
} from "../../layout";
import { SkillCard } from "./skillCard";
import {
  ChannelsScope,
  EmptyInstalled,
  EmptySearch,
  EmptyUpdates,
  ErrorBanner,
} from "./emptyStates";
import { SelectionBarSlot } from "./selectionBarSlot";
import {
  currentFiltered,
  error,
  loading,
  onlyUpdates,
  scope,
  searchQuery,
  selectedSkill,
  skills,
  viewList,
} from "./state";

// 画布 — my_skills/canvas.rs + view.rs 对位。
//
// 网格/列表都走 nsh List 虚拟化（uniform_list 对位）：只建视口附近的行。
// 网格模式把过滤集按 columnsFor(内容宽) 打包成「一行 N 卡」的行数组
// （grid.rs 固定 336 轨道：卡不随窗宽拉伸，剩余宽度留在行尾），列表模
// 式一行一卡全宽。列数由 onLayout 实测宽度驱动（GPUI 读 viewport 的对
// 位）——详情列开在同一行时让出 DETAIL_COLUMN_W 轨道，列数同帧退一
//（布局硬关系：默认窗宽关 3 列 / 开 2 列，layout 测试锁）。
//
// 骨架 pulse 六片与真实卡同轨道；滚动偏移不进 render（onScroll 只写
// backTop 信号）；视图切换换新数据集开顶（reset_list_scroll）。

export const LIST_GAP = 8;

/** 页体内容宽（onLayout 实测；网格列数的唯一输入）。 */
export const paneWidth = signal(0);

/** 回顶可见（onScroll 驱动，render 不读偏移——canvas.rs back_top_shown）。 */
const backTopShown = signal(false);

/** 画布唯一的列表 ref：回顶按钮与视图切换重置共用。 */
const boardRef = signal<ListRef | null>(null);

export const packRows = packCardRows;

export function SkillsCanvas(): JSX.Element {
  // 视图切换开新数据集：立即回顶（mod.rs reset_list_scroll——网格与列表
  // 不共享偏移）。
  effect(() => {
    void viewList();
    boardRef()?.scrollToOffset({ offset: 0, animated: false });
    backTopShown.set(false);
  });

  // 状态分派走 <If>（view.rs render_main_content 的分派对位）：条件在
  // when 表达式里读取信号，分支互斥；卡板常驻挂载不随筛选重建（滚动
  // 位置与虚拟化状态在筛选变化时保持，GPUI uniform_list 同款）。
  return (
    <View
      testID="skills-body"
      style={{
        position: "relative",
        display: "flex",
        flex: 1,
        minHeight: 0,
        minWidth: 0,
        width: "100%",
      }}
    >
      <If when={loading()}>
        <Pane>
          <SkeletonGrid viewList={viewList()} />
        </Pane>
      </If>
      <If when={!loading() && error() !== null}>
        <Pane>
          <ErrorBanner error={error() ?? ""} />
        </Pane>
      </If>
      <If when={scope() === "channels"}>
        <Pane>
          <ChannelsScope />
        </Pane>
      </If>
      <If when={scope() === "local" && !loading() && error() === null && skills().length === 0}>
        <Pane>
          <EmptyInstalled />
        </Pane>
      </If>
      <If
        when={
          scope() === "local" &&
          !loading() &&
          error() === null &&
          skills().length > 0 &&
          currentFiltered().length === 0
        }
      >
        <If when={onlyUpdates()} else={<EmptySearch query={searchQuery()} />}>
          <EmptyUpdates />
        </If>
      </If>
      <If
        when={
          scope() === "local" &&
          !loading() &&
          error() === null &&
          currentFiltered().length > 0
        }
      >
        <CardPane />
      </If>
      <If when={backTopShown() && boardRef() !== null}>
        <BackToTopButton
          label={t("mySkills.backToTop")}
          listRef={boardRef()}
          onClick={() => backTopShown.set(false)}
        />
      </If>
      <SelectionBarSlot />
    </View>
  );
}

/** 页滚动容器（view.rs render_main_content 的 scroller 位置）。 */
function Pane(props: { children: JSX.Element }): JSX.Element {
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
    >
      {props.children}
    </View>
  );
}

/** 卡板：虚拟化网格/列表（canvas.rs skill_list）。无 props：过滤集、列
 * 数、行距全部在 List 的属性表达式里现读（组件体只执行一次）。 */
function CardPane(): JSX.Element {
  const packedRows = computed(() => packCardRows(currentFiltered(), gridColumnsNow()));
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
      <List
        ref={(node) => {
          boardRef.set(node);
        }}
        testID="skills-scroll"
        each={packedRows()}
        keyExtractor={(_row, index) => `skills-row-${index}`}
        estimatedItemSize={stride()}
        scrollEventThrottle={32}
        style={{ width: "100%", height: "100%" }}
        contentStyle={{
          paddingTop: SKILL_SCROLL_PT,
          paddingBottom: SKILL_SCROLL_PB,
          paddingLeft: PAGE_PAD,
          paddingRight: PAGE_PAD,
        }}
        onScroll={(event) => {
          const shown = event.offset.y > SHOW_AFTER_PX;
          if (shown !== backTopShown()) backTopShown.set(shown);
        }}
      >
        {(row, index) => (
          <View
            testID={`skills-row-${index()}`}
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "flex-start",
              gap: viewList() ? 0 : CARD_GAP,
              height: stride(),
              paddingBottom: viewList() ? LIST_GAP : CARD_GAP,
              width: "100%",
            }}
          >
            <Each of={row()} key="index">
              {(skill) => (
                <View
                  style={
                    viewList()
                      ? { width: "100%", minWidth: 0 }
                      : {
                          width: gridTrackWidthNow(),
                          minWidth: gridTrackWidthNow(),
                          maxWidth: gridTrackWidthNow(),
                          flexShrink: 0,
                          flexGrow: 0,
                        }
                  }
                >
                  <SkillCard skill={skill} listMode={viewList()} cardWidth={gridTrackWidthNow()} />
                </View>
              )}
            </Each>
          </View>
        )}
      </List>
    </View>
  );
}

/** 当前列数：列表模式 1 列；网格按内容宽（onLayout 实测，减页内边距）。 */
function gridColumnsNow(): number {
  const fallbackW = selectedSkill() !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W;
  return gridColumnsForPane(paneWidth(), viewList(), PAGE_PAD, fallbackW);
}

/** 当前卡片列宽：最小 SKILL_CARD_W（336px），有剩余空间时与 PageBar 左右齐平。 */
function gridTrackWidthNow(): number {
  const fallbackW = selectedSkill() !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W;
  return cardTrackWidth(paneWidth(), viewList(), PAGE_PAD, fallbackW);
}

/** 行距（canvas.rs skill_line：CARD_H + 模式各自的 gap）。 */
function stride(): number {
  return SKILL_CARD_H + (viewList() ? LIST_GAP : CARD_GAP);
}

/** 骨架六片（view.rs render_loading_grid）：与真实卡同轨道的 pulse。 */
export function SkeletonGrid(props: { viewList: boolean }): JSX.Element {
  const skelW = (): number => cardTrackWidth(paneWidth(), props.viewList, PAGE_PAD, CARD_ROW_W);
  return (
    <View
      testID="skills-skeleton"
      style={
        props.viewList
          ? {
              display: "flex",
              flexDirection: "column",
              gap: LIST_GAP,
              padding: PAGE_PAD,
              paddingTop: SKILL_SCROLL_PT,
              paddingBottom: SKILL_SCROLL_PB,
              width: "100%",
            }
          : {
              display: "flex",
              flexDirection: "row",
              flexWrap: "wrap",
              gap: CARD_GAP,
              padding: PAGE_PAD,
              paddingTop: SKILL_SCROLL_PT,
              paddingBottom: SKILL_SCROLL_PB,
              width: "100%",
            }
      }
    >
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <View
          style={{
            height: SKILL_CARD_H,
            width: props.viewList ? "100%" : skelW(),
            flexShrink: 0,
            padding: 14,
            borderRadius: 16,
            borderWidth: 1,
            borderColor: paletteToken("border"),
            backgroundColor: paletteToken("card"),
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          <Pulse id={`skills-load-${index}-title`} style={{ height: 16, width: 140 }} />
          <Pulse id={`skills-load-${index}-line`} secondary style={{ height: 12 }} />
          <Pulse id={`skills-load-${index}-meta`} secondary style={{ height: 12, width: 200 }} />
        </View>
      ))}
    </View>
  );
}

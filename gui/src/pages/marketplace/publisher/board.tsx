import { List, Text, View, computed, effect, signal, type JSX, type ListRef } from "nsh";
import { paletteToken } from "../../../services/themeMode";
import {
  CARD_GAP,
  CARD_ROW_W,
  DETAIL_COLUMN_W,
  PAGE_PAD,
  SKILL_CARD_H,
  SKILL_SCROLL_PB,
} from "../../../layout";
import { Icon } from "../../../chrome/icons";
import { Pulse } from "../../../chrome/feedback";
import { cardTrackWidth, gridColumnsForPane, packCardRows } from "../../../chrome/cardShell";
import { BackToTopButton, SHOW_AFTER_PX } from "../../../chrome/scrollTop";
import { MarketCard } from "../card";
import { mt, mtf } from "../marketStrings";
import { formatInstalls, type MarketSkill } from "../types";
import type { PublisherRepoRow, PublisherSide } from "./readSide";

// 发布者详情板面 — marketplace/publisher/board.rs 对位。
//
// 仓库列表（76px 行卡）与仓库内技能网格（160px MarketCard）均走 nsh `<List>`
// 虚拟化打包行，仅构建视口附近行，避免大型发布者组织一次性挂载成百上千张卡。
// 组件体只执行一次：一切信号与 props 读取必须留在 JSX / computed / 回调内。

const REPO_ROW_H = 76;

export function PublisherBoardPane(props: { side: PublisherSide }): JSX.Element {
  const side = props.side;
  const paneWidth = signal(0);
  const scrolledPast = signal(false);
  const listRef = signal<ListRef | null>(null);

  // 切仓库/换发布者开新数据集：立即回顶（board.rs 换 scroll handle 的 reset 语义）。
  effect(() => {
    void side.activeRepo();
    void side.publisherName();
    scrolledPast.set(false);
    queueMicrotask(() => listRef()?.scrollToOffset({ offset: 0, animated: false }));
  });

  const fallbackW = (): number =>
    side.selected() !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W;

  const columnsNow = (): number => gridColumnsForPane(paneWidth(), false, PAGE_PAD, fallbackW());
  const trackW = (): number => cardTrackWidth(paneWidth(), false, PAGE_PAD, fallbackW());

  const handleScroll = (offsetY: number): void => {
    const past = offsetY > SHOW_AFTER_PX;
    if (past !== scrolledPast()) scrolledPast.set(past);
  };

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
      <If when={side.loadingRepos() || (side.activeRepo() !== null && side.loadingSkills())}>
        <View testID="pd-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
          <PublisherSkeletons cardWidth={trackW()} />
        </View>
      </If>
      <If when={!side.loadingRepos() && side.activeRepo() === null}>
        <RepoList
          repos={filterRepos(side.repos(), side.query())}
          columns={columnsNow()}
          cardWidth={trackW()}
          onOpen={side.openRepo}
          onRef={(node) => listRef.set(node)}
          onScroll={handleScroll}
        />
      </If>
      <If when={!side.loadingRepos() && !side.loadingSkills() && side.activeRepo() !== null}>
        <RepoSkillGrid
          skills={filterSkills(side.skills(), side.query())}
          columns={columnsNow()}
          cardWidth={trackW()}
          searching={side.query().trim().length > 0}
          busy={side.busy()}
          selectedName={side.selected()?.name ?? null}
          onOpenSkill={side.openSkill}
          onToggleInstall={side.toggleInstalled}
          onRef={(node) => listRef.set(node)}
          onScroll={handleScroll}
        />
      </If>
      <If when={scrolledPast() && listRef() !== null}>
        <BackToTopButton
          label={mt("publisherDetail.backToTop")}
          listRef={listRef()}
          onClick={() => scrolledPast.set(false)}
        />
      </If>
    </View>
  );
}

/** 仓库清单的本地过滤（board.rs render_list 的 query 分支）：repo/source 包含。 */
export function filterRepos(repos: PublisherRepoRow[], query: string): PublisherRepoRow[] {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return repos;
  return repos.filter(
    (repo) =>
      repo.repo.toLowerCase().includes(needle) || repo.source.toLowerCase().includes(needle)
  );
}

/** 仓库技能的本地过滤（board.rs render_skill_rows 的 query 分支）：name/description 包含。 */
export function filterSkills(skills: MarketSkill[], query: string): MarketSkill[] {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) return skills;
  return skills.filter(
    (skill) =>
      skill.name.toLowerCase().includes(needle) ||
      skill.description.toLowerCase().includes(needle)
  );
}

/** 骨架（mod.rs render_skeletons）：六片与真实卡同轨道的行形占位。 */
function PublisherSkeletons(props: { cardWidth: number }): JSX.Element {
  return (
    <View
      testID="pd-skeletons"
      style={{
        display: "flex",
        flexDirection: "row",
        flexWrap: "wrap",
        gap: CARD_GAP,
        justifyContent: "flex-start",
        alignItems: "flex-start",
        paddingHorizontal: PAGE_PAD,
        paddingBottom: SKILL_SCROLL_PB,
      }}
    >
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <View
          testID={`pd-skel-${index}`}
          style={{
            width: props.cardWidth,
            flexShrink: 0,
            display: "flex",
            flexDirection: "column",
            gap: 8,
            padding: 12,
            borderRadius: 16,
            borderWidth: 1,
            borderColor: paletteToken("border_soft"),
            backgroundColor: paletteToken("card"),
          }}
        >
          <Pulse id={`pd-skel-${index}-title`} style={{ width: 160, height: 16 }} />
          <Pulse id={`pd-skel-${index}-line`} style={{ width: 180, height: 12 }} secondary />
        </View>
      ))}
    </View>
  );
}

/** 空态块（mod.rs empty_block）。 */
function EmptyBlock(props: { testID: string; glyph: "folder" | "package"; label: string }): JSX.Element {
  return (
    <View
      testID={props.testID}
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
        paddingVertical: 64,
        paddingHorizontal: PAGE_PAD,
      }}
    >
      <Icon name={props.glyph} size={32} color={paletteToken("fg_muted")} />
      <Text style={{ fontSize: 16, fontWeight: "700", color: paletteToken("fg") }}>
        {props.label}
      </Text>
    </View>
  );
}

/** 仓库清单：虚拟化定宽轨道行。 */
function RepoList(props: {
  repos: PublisherRepoRow[];
  columns: number;
  cardWidth: number;
  onOpen: (source: string) => void;
  onRef: (node: ListRef | null) => void;
  onScroll: (offsetY: number) => void;
}): JSX.Element {
  const packedRows = computed(() => packCardRows(props.repos, props.columns));
  const rowStride = REPO_ROW_H + CARD_GAP;

  return (
    <If
      when={props.repos.length === 0}
      else={
        <List
          ref={(node) => props.onRef(node)}
          testID="pd-scroll"
          each={packedRows()}
          keyExtractor={(_row, idx) => `pd-repo-row-${idx}`}
          estimatedItemSize={rowStride}
          style={{ width: "100%", height: "100%" }}
          contentStyle={{
            paddingBottom: SKILL_SCROLL_PB,
            paddingLeft: PAGE_PAD,
            paddingRight: PAGE_PAD,
          }}
          scrollEventThrottle={32}
          onScroll={(event) => props.onScroll(event.offset.y)}
        >
          {(row, index) => (
            <View
              testID={`pd-repo-list-row-${index()}`}
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "flex-start",
                gap: CARD_GAP,
                height: rowStride,
                paddingBottom: CARD_GAP,
                width: "100%",
              }}
            >
              <Each of={row()} key="index">
                {(repo) => <RepoRow repo={repo} cardWidth={props.cardWidth} onOpen={props.onOpen} />}
              </Each>
            </View>
          )}
        </List>
      }
    >
      <View testID="pd-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
        <EmptyBlock testID="pd-empty" glyph="folder" label={mt("publisherDetail.noReposMatch")} />
      </View>
    </If>
  );
}

function installsLabel(repo: PublisherRepoRow): string | null {
  if (repo.installsLabel.length > 0) return repo.installsLabel;
  if (repo.installs > 0) return formatInstalls(repo.installs);
  return null;
}

function RepoRow(props: {
  repo: () => PublisherRepoRow;
  cardWidth: number;
  onOpen: (source: string) => void;
}): JSX.Element {
  return (
    <View
      testID={`pub-repo-row-${props.repo().repo}`}
      role="button"
      accessibilityLabel={`${props.repo().source}`}
      style={{
        width: props.cardWidth,
        height: REPO_ROW_H,
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        gap: 8,
        padding: 12,
        borderRadius: 16,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        cursor: "pointer",
      }}
      hoverStyle={{ backgroundColor: paletteToken("card_hover"), borderColor: paletteToken("accent") }}
      onPress={() => props.onOpen(props.repo().source)}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 8,
          minWidth: 0,
        }}
      >
        <View style={{ flex: 1, minWidth: 0 }}>
          <Text
            style={{
              fontSize: 14,
              fontWeight: "600",
              fontFamily: "monospace",
              color: paletteToken("fg"),
              numberOfLines: 1,
            }}
          >
            {`${props.repo().source}`}
          </Text>
        </View>
        <Icon name="chevronRight" size={14} color={paletteToken("fg_muted")} />
      </View>
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
        }}
      >
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
          <Icon name="package" size={12} color={paletteToken("fg_muted")} />
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {mtf("marketplace.skillCount", { count: props.repo().skillCount })}
          </Text>
        </View>
        <If when={installsLabel(props.repo()) !== null}>
          <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4 }}>
            <Icon name="download" size={12} color={paletteToken("fg_muted")} />
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {installsLabel(props.repo()) ?? ""}
            </Text>
          </View>
        </If>
      </View>
    </View>
  );
}

/** 当前仓库的技能网格：虚拟化行打包，复用市场卡身 MarketCard。 */
function RepoSkillGrid(props: {
  skills: MarketSkill[];
  columns: number;
  cardWidth: number;
  searching: boolean;
  busy: string | null;
  selectedName: string | null;
  onOpenSkill: (skill: MarketSkill) => void;
  onToggleInstall: (skill: MarketSkill) => void;
  onRef: (node: ListRef | null) => void;
  onScroll: (offsetY: number) => void;
}): JSX.Element {
  const packedRows = computed(() => packCardRows(props.skills, props.columns));
  const rowStride = SKILL_CARD_H + CARD_GAP;

  return (
    <If
      when={props.skills.length === 0}
      else={
        <List
          ref={(node) => props.onRef(node)}
          testID="pd-scroll"
          each={packedRows()}
          keyExtractor={(_row, idx) => `pd-skill-row-${idx}`}
          estimatedItemSize={rowStride}
          style={{ width: "100%", height: "100%" }}
          contentStyle={{
            paddingBottom: SKILL_SCROLL_PB,
            paddingLeft: PAGE_PAD,
            paddingRight: PAGE_PAD,
          }}
          scrollEventThrottle={32}
          onScroll={(event) => props.onScroll(event.offset.y)}
        >
          {(row, index) => (
            <View
              testID={`pd-skill-list-row-${index()}`}
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "flex-start",
                gap: CARD_GAP,
                height: rowStride,
                paddingBottom: CARD_GAP,
                width: "100%",
              }}
            >
              <Each of={row()} key="index">
                {(skill) => (
                  <View
                    style={{
                      width: props.cardWidth,
                      minWidth: props.cardWidth,
                      maxWidth: props.cardWidth,
                      flexShrink: 0,
                    }}
                  >
                    <MarketCard
                      skill={skill()}
                      selected={props.selectedName === skill().name}
                      busy={props.busy === skill().name}
                      cardWidth={props.cardWidth}
                      onPress={() => props.onOpenSkill(skill())}
                      onToggleInstall={() => props.onToggleInstall(skill())}
                    />
                  </View>
                )}
              </Each>
            </View>
          )}
        </List>
      }
    >
      <View testID="pd-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
        <EmptyBlock
          testID="pd-empty"
          glyph="package"
          label={
            props.searching
              ? mt("publisherDetail.noMatch")
              : mt("publisherDetail.noSkills")
          }
        />
      </View>
    </If>
  );
}

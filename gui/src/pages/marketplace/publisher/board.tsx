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
import { bindDelayedFlag } from "../../../chrome/delayedFlag";
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

function sameElements<T>(prev: readonly T[], next: readonly T[]): boolean {
  if (prev === next) return true;
  if (prev.length !== next.length) return false;
  for (let i = 0; i < prev.length; i += 1) {
    if (prev[i] !== next[i]) return false;
  }
  return true;
}

export function PublisherBoardPane(props: {
  side: PublisherSide;
  active?: () => boolean;
}): JSX.Element {
  const side = props.side;
  const isActive = (): boolean => (props.active ? props.active() : true);
  const paneWidth = signal(0);
  const scrolledPast = signal(false);
  const repoListRef = signal<ListRef | null>(null);
  const skillListRef = signal<ListRef | null>(null);
  const visitedSkillGrid = signal(side.activeRepo() !== null);

  effect(() => {
    if (side.activeRepo() !== null && !visitedSkillGrid()) {
      visitedSkillGrid.set(true);
    }
  });

  const activeListRef = (): ListRef | null =>
    side.activeRepo() !== null ? skillListRef() : repoListRef();

  // 还没有内容时才晚出骨架；已有行的刷新留在原处。
  const skeletonReady = bindDelayedFlag(() => {
    if (!isActive()) return false;
    if (side.activeRepo() !== null) return side.loadingSkills() && side.skills().length === 0;
    return side.loadingRepos() && side.repos().length === 0;
  });

  // 切仓库/换发布者开新数据集：立即回顶（board.rs 换 scroll handle 的 reset 语义）。
  effect(() => {
    void side.activeRepo();
    void side.publisherName();
    scrolledPast.set(false);
    queueMicrotask(() => activeListRef()?.scrollToOffset({ offset: 0, animated: false }));
  });

  const filteredRepos = computed<PublisherRepoRow[]>(
    (prev) => (!isActive() && prev !== undefined ? prev : filterRepos(side.repos(), side.query())),
    { equals: sameElements },
  );
  const filteredSkills = computed<MarketSkill[]>(
    (prev) => (!isActive() && prev !== undefined ? prev : filterSkills(side.skills(), side.query())),
    { equals: sameElements },
  );

  const fallbackW = (): number =>
    side.selected() !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W;

  const columnsNow = (): number => gridColumnsForPane(paneWidth(), false, PAGE_PAD, fallbackW());
  const trackW = (): number => cardTrackWidth(paneWidth(), false, PAGE_PAD, fallbackW());

  const handleScroll = (offsetY: number): void => {
    const past = offsetY > SHOW_AFTER_PX;
    if (past !== scrolledPast()) scrolledPast.set(past);
  };

  const showSkeleton = (): boolean =>
    (side.snapshotStatus() === "seeding" &&
      (side.activeRepo() !== null ? side.skills().length === 0 : side.repos().length === 0)) ||
    (skeletonReady() &&
      (side.activeRepo() !== null
        ? side.loadingSkills() && side.skills().length === 0
        : side.loadingRepos() && side.repos().length === 0));
  const showRepos = (): boolean =>
    side.activeRepo() === null &&
    (side.repos().length > 0 || (!side.loadingRepos() && side.snapshotStatus() !== "seeding"));
  const showSkills = (): boolean =>
    side.activeRepo() !== null &&
    (side.skills().length > 0 || (!side.loadingSkills() && side.snapshotStatus() !== "seeding"));

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
      <If when={showSkeleton()}>
        <View testID="pd-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
          <PublisherSkeletons cardWidth={trackW()} />
        </View>
      </If>
      <View
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          display: showRepos() ? "flex" : "none",
        }}
      >
        <RepoList
          repos={filteredRepos()}
          columns={columnsNow()}
          cardWidth={trackW()}
          onOpen={side.openRepo}
          onRef={(node) => repoListRef.set(node)}
          onScroll={handleScroll}
        />
      </View>
      <If when={visitedSkillGrid()}>
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
          <RepoSkillGrid
            skills={filteredSkills()}
            columns={columnsNow()}
            cardWidth={trackW()}
            searching={side.query().trim().length > 0}
            busy={side.busy()}
            selectedName={side.selected()?.name ?? null}
            onOpenSkill={side.openSkill}
            onToggleInstall={side.toggleInstalled}
            onRef={(node) => skillListRef.set(node)}
            onScroll={handleScroll}
          />
        </View>
      </If>
      <If when={scrolledPast() && activeListRef() !== null}>
        <BackToTopButton
          label={mt("publisherDetail.backToTop")}
          listRef={activeListRef()}
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
      accessibilityState={{ busy: true }}
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
    <View style={{ position: "relative", flex: 1, minHeight: 0, width: "100%" }}>
      <View style={{ width: "100%", height: "100%", display: props.repos.length === 0 ? "none" : "flex" }}>
        <List
          ref={(node) => props.onRef(node)}
          testID="pd-scroll"
          each={packedRows()}
          keyExtractor={(row) => row.map((r) => `${r.source}\u001f${r.repo}`).join("\u001e")}
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
      </View>
      <If when={props.repos.length === 0}>
        <View testID="pd-scroll" style={{ flex: 1, minHeight: 0, width: "100%" }}>
          <EmptyBlock testID="pd-empty" glyph="folder" label={mt("publisherDetail.noReposMatch")} />
        </View>
      </If>
    </View>
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
  /** 在飞的安装/卸载（技能名）；每张卡只认自己的名字。 */
  busy: ReadonlySet<string>;
  selectedName: string | null;
  onOpenSkill: (skill: MarketSkill) => void;
  onToggleInstall: (skill: MarketSkill) => void;
  onRef: (node: ListRef | null) => void;
  onScroll: (offsetY: number) => void;
}): JSX.Element {
  const packedRows = computed(() => packCardRows(props.skills, props.columns));
  const rowStride = SKILL_CARD_H + CARD_GAP;

  return (
    <View style={{ position: "relative", flex: 1, minHeight: 0, width: "100%" }}>
      <View style={{ width: "100%", height: "100%", display: props.skills.length === 0 ? "none" : "flex" }}>
        <List
          ref={(node) => props.onRef(node)}
          testID="pd-scroll"
          each={packedRows()}
          keyExtractor={(row) => row.map((s) => `${s.source ?? s.gitUrl}\u001f${s.name}`).join("\u001e")}
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
                  <RepoSkillSlot
                    skill={skill}
                    cardWidth={() => props.cardWidth}
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
      </View>
      <If when={props.skills.length === 0}>
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
    </View>
  );
}

function RepoSkillSlot(props: {
  skill: () => MarketSkill;
  cardWidth: () => number;
  selectedName: () => string | null;
  busyNames: () => ReadonlySet<string>;
  onOpenSkill: (skill: MarketSkill) => void;
  onToggleInstall: (skill: MarketSkill) => void;
}): JSX.Element {
  const selected = computed(() => props.selectedName() === props.skill().name);
  const busy = computed(() => props.busyNames().has(props.skill().name));
  return (
    <View
      style={{
        width: props.cardWidth(),
        minWidth: props.cardWidth(),
        maxWidth: props.cardWidth(),
        flexShrink: 0,
      }}
    >
      <MarketCard
        skill={props.skill()}
        selected={selected()}
        busy={busy()}
        cardWidth={props.cardWidth()}
        onPress={() => props.onOpenSkill(props.skill())}
        onToggleInstall={() => props.onToggleInstall(props.skill())}
      />
    </View>
  );
}

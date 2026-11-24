import { Text, View, computed, effect, type JSX } from "nsh";
import { Marketplace } from "skillstar-gui/native";
import { paletteToken } from "../../services/themeMode";
import { SLOW_MS } from "../../chrome/delayedFlag";
import { Icon } from "../../chrome/icons";
import {
  BarIconButton,
  BarSegmentSlot,
  BarSegmented,
  BarTextChip,
  BarViewToggle,
  PageBar,
} from "../../chrome/toolbar";
import { openUninstallConfirm, uninstallTitleNamed } from "../../chrome/dialog";
import { endJob, putJob } from "../../shell/jobBoard";
import { publishToast, requestImportSource, selectPage, selectPublisher, toast } from "../../shell/events";
import { currentPage } from "../../shell/nav";
import { DETAIL_COLUMN_W, PAGE_PAD, SKILL_SCROLL_PT } from "../../layout";
import { leaderboardCategory, type MarketSkill, type MarketTabId, type PublisherRecord } from "./types";
import {
  createReadSide,
  NO_BUSY,
  type DirectSourceMatch,
  type MarketNotice,
  type ReadSide,
} from "./readSide";
import { mt, mtf, snapshotLabel } from "./marketStrings";
import { MarketBoard, SnapshotBanner } from "./board";
import { MarketSearch, useMarketSearchKeys } from "./search";
import { MarketDetailColumn } from "./detail_drawer";

// 市场页 — marketplace/mod.rs（+ view.rs + board.rs + card.rs + types.rs）
// 对位，波 2 基座。顶栏：与技能页同款的内联搜索框（⌘F 聚焦）、四 tab 分段、
// 技能计数 chip、快照状态弱化标签、刷新（只转图标）、排序分段、网格/列表切换。
// 同步成功走短通知。页面体：快照错误横幅（重试）+ 板面（延迟骨架/
// 空态/技能网格/官方发布者列表）+ 详情列固定轨道（selected 非空时）。
//
// 读侧状态机在 readSide.ts（本地优先 + epoch 丢弃过期响应），页面只持
// 模块级单例（keep-alive 挂载一次 = 页面实体）并把信号绑进 JSX——组件
// 体只执行一次，signal 消费必须落在 JSX 响应式绑定里（NoticeBand/
// DialogHost 的教训）。

/** 状态机通知 → NoticeBand toast（notice 文案在本层映射）。 */
function notifyToast(notice: MarketNotice): void {
  switch (notice.kind) {
    case "installed":
      publishToast({ tone: "success", message: mtf("marketplace.installedName", { name: notice.name }) });
      break;
    case "uninstalled":
      publishToast({ tone: "success", message: mtf("marketplace.uninstalledName", { name: notice.name }) });
      break;
    case "installFailed":
      publishToast({
        tone: "error",
        message: mtf("marketplace.installFailedDetail", { err: notice.error }),
      });
      break;
    case "uninstallFailed":
      publishToast({
        tone: "error",
        message: mtf("marketplace.uninstallFailedDetail", { err: notice.error }),
      });
      break;
  }
}

/** mod.rs refresh 的读取路由（桥侧）：非空 query 恒 search；official 为
 * 发布者；其余按 tab 类目读 leaderboard。 */
async function bridgeLoader(tab: MarketTabId, query: string): Promise<
  | { kind: "skills"; payload: unknown }
  | { kind: "publishers"; payload: unknown }
> {
  if (query.length > 0) {
    return { kind: "skills", payload: await Marketplace.search(query) };
  }
  if (tab === "official") {
    return { kind: "publishers", payload: await Marketplace.publishers() };
  }
  return { kind: "skills", payload: await Marketplace.leaderboard(leaderboardCategory(tab)) };
}

function parseDirectSourceBridge(query: string): DirectSourceMatch | null {
  const raw = Marketplace.parseDirectSource(query) as Record<string, unknown> | null;
  if (raw === null || typeof raw !== "object") return null;
  if (typeof raw.source !== "string" || typeof raw.raw !== "string") return null;
  return {
    source: raw.source,
    raw: raw.raw,
    skills: Array.isArray(raw.skills)
      ? raw.skills.filter((item): item is string => typeof item === "string")
      : [],
    wasCommand: raw.wasCommand === true,
  };
}

const readSide: ReadSide = createReadSide({
  loader: bridgeLoader,
  syncer: (scope) => Marketplace.syncScope(scope),
  installer: (url, name) => Marketplace.installSkill(url, name),
  uninstaller: (name, source) => Marketplace.uninstallSkill(source ?? "", name),
  schedule: (fn, ms) => {
    const timer = setTimeout(fn, ms);
    return () => clearTimeout(timer);
  },
  notify: notifyToast,
  confirmUninstall: (name, commit) => {
    openUninstallConfirm(uninstallTitleNamed(name), () => {
      commit();
      return true;
    });
  },
  parseDirectSource: parseDirectSourceBridge,
  maybeSeedSearch: (query) => Marketplace.maybeSeedSearch(query),
  isActive: () => currentPage() === "marketplace",
});

function sameElements<T>(prev: readonly T[], next: readonly T[]): boolean {
  if (prev === next) return true;
  if (prev.length !== next.length) return false;
  for (let i = 0; i < prev.length; i += 1) {
    if (prev[i] !== next[i]) return false;
  }
  return true;
}

// 同步成功的短通知。flash 在开始时落下、结束时升起，避免第二次不触发。
let seenSyncFlash = false;
effect(() => {
  const flash = readSide.syncedFlash();
  const refreshing = readSide.refreshing();
  if (flash && !refreshing) {
    if (!seenSyncFlash) {
      seenSyncFlash = true;
      toast(mt("marketplace.snapshotSynced"), "success");
    }
  } else {
    seenSyncFlash = false;
  }
});

// 快照超过约 10 秒才挂任务。不能取消。刷新结束清计时器并收起任务。
effect(() => {
  if (!readSide.refreshing()) return;
  const timer = setTimeout(() => {
    putJob({
      id: "market-snapshot",
      title: mt("marketplace.refreshingSnapshot"),
    });
  }, SLOW_MS);
  return () => {
    clearTimeout(timer);
    endJob("market-snapshot");
  };
});

export function MarketplacePage(): JSX.Element {
  // ⌘F 绑定在组件体内注册（keymap 的组件生命周期语义）。
  useMarketSearchKeys();
  // 惰性首载：首次进入市场页才触发快照读取，不拖慢应用启动首帧。
  effect(() => {
    if (currentPage() !== "marketplace") return;
    readSide.start();
    readSide.flushBackfill();
  });
  return <MarketplaceBody />;
}

function MarketplaceBody(): JSX.Element {
  const pageActive = (): boolean => currentPage() === "marketplace";
  const gated = <T,>(
    read: () => T,
    fallback: T,
    equals?: (prev: T, next: T) => boolean,
  ) =>
    computed<T>(
      (prev) => (!pageActive() ? (prev !== undefined ? prev : fallback) : read()),
      equals ? { equals } : undefined,
    );

  const gTab = gated(() => readSide.tab(), "all");
  const gQuery = gated(() => readSide.query(), "");
  const gSkills = gated(() => readSide.skills(), [], sameElements);
  const gPublishers = gated(() => readSide.publishers(), [], sameElements);
  const gSort = gated(() => readSide.sort(), "stars");
  const gViewList = gated(() => readSide.viewList(), false);
  const gLoading = gated(() => readSide.loading(), true);
  const gRefreshing = gated(() => readSide.refreshing(), false);
  const gBusy = gated(() => readSide.busy(), NO_BUSY);
  const gSnapshotStatus = gated(() => readSide.snapshotStatus(), "unknown");
  const gSnapshotError = gated(() => readSide.snapshotError(), null);
  const gSelected = gated(() => readSide.selected(), null);
  const gDirectSource = gated(() => readSide.directSource(), null);
  const gSelectedSkill = gated(() => findSkill(readSide.skills(), readSide.selected()), null);
  // 详情列只跟自己这一个技能的安装走：别的技能在飞不钉住这一列。
  const selectedBusy = (): boolean => {
    const identity = gSelected();
    return identity !== null && gBusy().has(identity.name);
  };

  return (
    <View style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, width: "100%" }}>
      <PageBar
        title={mt("sidebar.market")}
        dragId="market-toolbar-drag"
        search={
          <MarketSearch
            query={gQuery()}
            onChange={(text) => readSide.onSearchInput(text)}
            onClear={() => readSide.clearSearch()}
          />
        }
        filters={[
          <MarketTabs tab={gTab()} onSelect={(tab) => readSide.selectTab(tab)} />,
          <If when={gTab() !== "official"}>
            <View testID="mk-count-chip">
              <BarTextChip label={mtf("marketplace.skillsCount", { count: gSkills().length })} />
            </View>
          </If>,
        ]}
        actions={[
          <If when={snapshotLabel(gSnapshotStatus(), false) !== null}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), flexShrink: 0 }}>
              {snapshotLabel(gSnapshotStatus(), false) ?? ""}
            </Text>
          </If>,
          <BarIconButton
            glyph="refresh"
            loading={gRefreshing()}
            tip={mt("common.refresh")}
            onPress={() => readSide.syncSnapshot()}
          />,
          <SortSegmented sort={gSort()} onSelect={(sort) => readSide.setSort(sort)} />,
          <BarViewToggle
            testID="mk-view-switcher"
            gridTestID="mk-view-grid"
            listTestID="mk-view-list"
            listMode={gViewList()}
            onSelect={(list) => readSide.setViewList(list)}
          />,
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
            display: "flex",
            flexDirection: "column",
            flex: 1,
            minHeight: 0,
            minWidth: 0,
          }}
        >
          <If when={gSnapshotError() !== null}>
            <View style={{ paddingHorizontal: PAGE_PAD, paddingTop: SKILL_SCROLL_PT }}>
              <SnapshotBanner
                error={gSnapshotError() ?? { kind: "remote_error", scope: "", detail: "" }}
                refreshing={gRefreshing()}
                onRetry={() => readSide.syncSnapshot()}
              />
            </View>
          </If>
          <If when={gDirectSource() !== null}>
            <DirectSourceBanner
              direct={gDirectSource() ?? { source: "", raw: "", skills: [], wasCommand: false }}
              onImport={() => {
                const direct = gDirectSource();
                if (!direct) return;
                selectPage.publish("my_skills");
                requestImportSource.publish({ input: direct.raw, autoScan: true });
              }}
            />
          </If>
          <View style={{ position: "relative", flex: 1, minHeight: 0, width: "100%" }}>
            <MarketBoard
              active={pageActive}
              loading={gLoading()}
              snapshotStatus={gSnapshotStatus()}
              refreshing={gRefreshing()}
              tab={gTab()}
              skills={gSkills()}
              publishers={gPublishers()}
              query={gQuery()}
              viewList={gViewList()}
              busy={gBusy()}
              selectedName={gSelected()?.name ?? null}
              onOpenSkill={(skill) => readSide.openSkill(skill)}
              onToggleInstall={(skill) => readSide.toggleInstalled(skill)}
              onOpenPublisher={openPublisher}
              onClearSearch={() => readSide.clearSearch()}
              onSyncSnapshot={() => readSide.syncSnapshot()}
            />
          </View>
        </View>
        <If when={gSelected() !== null}>
          <View style={{ width: DETAIL_COLUMN_W, flexShrink: 0, height: "100%" }}>
            <MarketDetailColumn
              skill={gSelectedSkill()}
              identity={gSelected() ?? { source: null, name: "" }}
              busy={selectedBusy()}
              onClose={() => readSide.clearSelection()}
              onToggleInstall={(skill) => readSide.toggleInstalled(skill)}
            />
          </View>
        </If>
      </View>
    </View>
  );
}

function DirectSourceBanner(props: {
  direct: DirectSourceMatch;
  onImport: () => void;
}): JSX.Element {
  const headline = (): string =>
    mtf(
      props.direct.wasCommand
        ? "marketplace.directSourceCommand"
        : "marketplace.directSourceRepo",
      { source: props.direct.source },
    );
  return (
    <View
      testID="mk-direct-source-banner"
      style={{
        paddingHorizontal: PAGE_PAD,
        paddingTop: SKILL_SCROLL_PT,
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          flexWrap: "wrap",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          paddingHorizontal: 14,
          paddingVertical: 10,
          borderRadius: 10,
          borderWidth: 1,
          borderColor: paletteToken("accent_soft_edge"),
          backgroundColor: paletteToken("accent_soft"),
        }}
      >
        <View style={{ flex: 1, minWidth: 220, display: "flex", flexDirection: "column", gap: 2 }}>
          <Text style={{ fontSize: 13, fontWeight: "600", color: paletteToken("accent_fg") }}>
            {headline()}
          </Text>
          <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
            {mt("marketplace.directSourceHint")}
          </Text>
        </View>
        <View
          testID="mk-direct-source-import"
          role="button"
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            paddingHorizontal: 12,
            paddingVertical: 7,
            borderRadius: 8,
            backgroundColor: paletteToken("accent"),
            cursor: "pointer",
          }}
          hoverStyle={{ opacity: 0.9 }}
          onPress={props.onImport}
        >
          <Icon name="download" size={13} color={paletteToken("on_accent")} />
          <Text style={{ fontSize: 12, fontWeight: "600", color: paletteToken("on_accent") }}>
            {mt("marketplace.directSourceAction")}
          </Text>
        </View>
      </View>
    </View>
  );
}

function findSkill(
  skills: MarketSkill[],
  identity: { name: string } | null
): MarketSkill | null {
  if (identity === null) return null;
  return skills.find((skill) => skill.name === identity.name) ?? null;
}

/** 官方 tab 的发布者卡：发 SelectPublisher，Shell 翻译成发布者页跳转
 * （侧栏选中框留在市场项——nav.ts railPage 的契约）。 */
function openPublisher(publisher: PublisherRecord): void {
  selectPublisher.publish({ id: publisher.repo, name: publisher.name });
}

/** 四 tab 分段（view.rs render_tabs）：原地变色式 kit TabBar segmented
 * （互斥分段两类中的原地式，非滑块）。 */
const MARKET_TAB_IDS: MarketTabId[] = ["all", "trending", "hot", "official"];

/** tab 标题键（JSX 内现查 locale，语言切换可重绘）。 */
function tabLabelKey(tab: MarketTabId): string {
  return tab === "all"
    ? "toolbar.all"
    : tab === "trending"
      ? "marketplace.trending"
      : tab === "hot"
        ? "marketplace.hot"
        : "marketplace.official";
}

function MarketTabs(props: { tab: MarketTabId; onSelect: (tab: MarketTabId) => void }): JSX.Element {
  return (
    <BarSegmented
      testID="mk-tabs"
      slots={MARKET_TAB_IDS.map((id) => (
        <BarSegmentSlot
          testID={`mk-tab-${id}`}
          label={mt(tabLabelKey(id))}
          selected={props.tab === id}
          onPress={() => props.onSelect(id)}
        />
      ))}
    />
  );
}

/** 排序分段（view.rs render_sort）：下载量 ↓ / 最近更新。 */
function SortSegmented(props: { sort: "stars" | "updated"; onSelect: (sort: "stars" | "updated") => void }): JSX.Element {
  return (
    <BarSegmented
      testID="mk-sort"
      slots={[
        <BarSegmentSlot
          testID="mk-sort-stars"
          label={mt("toolbar.stars")}
          selected={props.sort === "stars"}
          onPress={() => props.onSelect("stars")}
        />,
        <BarSegmentSlot
          testID="mk-sort-updated"
          label={mt("toolbar.updated")}
          selected={props.sort === "updated"}
          onPress={() => props.onSelect("updated")}
        />,
      ]}
    />
  );
}

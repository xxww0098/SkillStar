import { Text, View, effect, type JSX } from "nsh";
import { Marketplace } from "skillstar-gui/native";
import { paletteToken } from "../../services/themeMode";
import { Icon } from "../../chrome/icons";
import { PageBar, BarIconButton, BarTextChip, BarViewToggle } from "../../chrome/toolbar";
import { openUninstallConfirm, uninstallTitleNamed } from "../../chrome/dialog";
import { publishToast, requestImportSource, selectPage, selectPublisher } from "../../shell/events";
import { DETAIL_COLUMN_W, PAGE_PAD, SKILL_SCROLL_PT } from "../../layout";
import { leaderboardCategory, type MarketSkill, type MarketTabId, type PublisherRecord } from "./types";
import { createReadSide, type DirectSourceMatch, type MarketNotice, type ReadSide } from "./readSide";
import { mt, mtf, snapshotLabel } from "./marketStrings";
import { MarketBoard, SnapshotBanner } from "./board";
import { Spotlight, useSpotlightKeys } from "./spotlight";
import { MarketDetailColumn } from "./detail_drawer";

// 市场页 — marketplace/mod.rs（+ view.rs + board.rs + card.rs + types.rs）
// 对位，波 2 基座。顶栏：spotlight 触发器（⌘F）、四 tab 分段、技能计数
// chip、同步状态文案、快照状态弱化标签、刷新（loading 转圈吞点击）、
// 排序分段、网格/列表切换。页面体：快照错误横幅（重试）+ 板面（骨架/
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
  uninstaller: (name) => Marketplace.uninstallSkill(name),
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
});

// keep-alive：挂载即首载一次（MarketplacePage::new 的 refresh）。
effect(() => readSide.start());

export function MarketplacePage(): JSX.Element {
  // ⌘F/Esc 绑定在组件体内注册（keymap 的组件生命周期语义）。
  useSpotlightKeys();
  return <MarketplaceBody />;
}

function MarketplaceBody(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, width: "100%" }}>
      <PageBar
        title={mt("sidebar.market")}
        dragId="market-toolbar-drag"
        search={
          <Spotlight
            query={readSide.query()}
            skills={readSide.skills()}
            onPanelInput={(text) => readSide.onSearchInput(text)}
            onPick={(name) => readSide.spotlightPick(name)}
            onClear={() => readSide.clearSearch()}
          />
        }
        filters={[
          <MarketTabs tab={readSide.tab()} onSelect={(tab) => readSide.selectTab(tab)} />,
          <If when={readSide.tab() !== "official"}>
            <View testID="mk-count-chip">
              <BarTextChip label={mtf("marketplace.skillsCount", { count: readSide.skills().length })} />
            </View>
          </If>,
        ]}
        actions={[
          <If when={readSide.syncedFlash() && !readSide.refreshing()}>
            <Text style={{ fontSize: 12, color: paletteToken("ok") }}>
              {mt("marketplace.snapshotSynced")}
            </Text>
          </If>,
          <If when={snapshotLabel(readSide.snapshotStatus(), readSide.refreshing()) !== null}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted"), flexShrink: 0 }}>
              {snapshotLabel(readSide.snapshotStatus(), readSide.refreshing()) ?? ""}
            </Text>
          </If>,
          <BarIconButton
            glyph="refresh"
            loading={readSide.refreshing()}
            tip={mt("common.refresh")}
            onPress={() => readSide.syncSnapshot()}
          />,
          <SortSegmented sort={readSide.sort()} onSelect={(sort) => readSide.setSort(sort)} />,
          <BarViewToggle
            testID="mk-view-switcher"
            gridTestID="mk-view-grid"
            listTestID="mk-view-list"
            listMode={readSide.viewList()}
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
          <If when={readSide.snapshotError() !== null}>
            <View style={{ paddingHorizontal: PAGE_PAD, paddingTop: SKILL_SCROLL_PT }}>
              <SnapshotBanner
                error={readSide.snapshotError() ?? ""}
                refreshing={readSide.refreshing()}
                onRetry={() => readSide.syncSnapshot()}
              />
            </View>
          </If>
          {(() => {
            const direct = readSide.directSource();
            if (direct === null) return null;
            return (
              <DirectSourceBanner
                direct={direct}
                onImport={() => {
                  selectPage.publish("my_skills");
                  requestImportSource.publish({ input: direct.raw, autoScan: true });
                }}
              />
            );
          })()}
          <View style={{ position: "relative", flex: 1, minHeight: 0, width: "100%" }}>
            <MarketBoard
              loading={readSide.loading()}
              tab={readSide.tab()}
              skills={readSide.skills()}
              publishers={readSide.publishers()}
              query={readSide.query()}
              viewList={readSide.viewList()}
              busy={readSide.busy()}
              selectedName={readSide.selected()?.name ?? null}
              onOpenSkill={(skill) => readSide.openSkill(skill)}
              onToggleInstall={(skill) => readSide.toggleInstalled(skill)}
              onOpenPublisher={openPublisher}
              onClearSearch={() => readSide.clearSearch()}
            />
          </View>
        </View>
        <If when={readSide.selected() !== null}>
          <View style={{ width: DETAIL_COLUMN_W, flexShrink: 0, height: "100%" }}>
            <MarketDetailColumn
              skill={findSkill(readSide.skills(), readSide.selected())}
              identity={readSide.selected() ?? { source: null, name: "" }}
              busy={readSide.busy() !== null}
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
    <View
      testID="mk-tabs"
      role="tablist"
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: 32,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        padding: 2,
        gap: 2,
      }}
    >
      {MARKET_TAB_IDS.map((id) => (
        <TabSlot
          testID={`mk-tab-${id}`}
          label={mt(tabLabelKey(id))}
          selected={props.tab === id}
          onPress={() => props.onSelect(id)}
        />
      ))}
    </View>
  );
}

function TabSlot(props: { testID: string; label: string; selected: boolean; onPress: () => void }): JSX.Element {
  return (
    <View
      testID={props.testID}
      role="tab"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: 26,
        paddingHorizontal: 12,
        borderRadius: 7,
        cursor: "pointer",
        backgroundColor: props.selected ? paletteToken("card") : "transparent",
        borderWidth: 1,
        borderColor: props.selected ? paletteToken("border") : "transparent",
      }}
      hoverStyle={props.selected ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      onPress={props.onPress}
    >
      <Text
        style={{
          fontSize: 12,
          color: props.selected ? paletteToken("accent") : paletteToken("fg_muted"),
          fontWeight: props.selected ? "600" : "500",
        }}
      >
        {props.label}
      </Text>
    </View>
  );
}

/** 排序分段（view.rs render_sort）：下载量 ↓ / 最近更新。 */
function SortSegmented(props: { sort: "stars" | "updated"; onSelect: (sort: "stars" | "updated") => void }): JSX.Element {
  return (
    <View
      testID="mk-sort"
      role="tablist"
      desktop={{ windowDrag: false }}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        height: 32,
        borderRadius: 10,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("bg"),
        padding: 2,
        gap: 2,
      }}
    >
      <TabSlot
        testID="mk-sort-stars"
        label={mt("toolbar.stars")}
        selected={props.sort === "stars"}
        onPress={() => props.onSelect("stars")}
      />
      <TabSlot
        testID="mk-sort-updated"
        label={mt("toolbar.updated")}
        selected={props.sort === "updated"}
        onPress={() => props.onSelect("updated")}
      />
    </View>
  );
}

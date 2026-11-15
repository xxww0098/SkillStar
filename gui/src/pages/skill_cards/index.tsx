import { List, Text, View, computed, effect, signal, type JSX, type ListRef } from "nsh";
import { SkillCards } from "skillstar-gui/native";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { cardTrackWidth, gridColumnsForPane, packCardRows } from "../../chrome/cardShell";
import { BackToTopButton, SHOW_AFTER_PX } from "../../chrome/scrollTop";
import {
  PageBar,
  BarCount,
  BarPrimary,
  BarSearchInput,
  BarSecondary,
  BarViewToggle,
} from "../../chrome/toolbar";
import {
  CARD_GAP,
  CARD_ROW_W,
  DETAIL_COLUMN_W,
  PAGE_PAD,
  SKILL_CARD_H,
  SKILL_SCROLL_PB,
  SKILL_SCROLL_PT,
} from "../../layout";
import { publishToast, toast, groupsChanged, agentsChanged } from "../../shell/events";
import {
  type AgentRailProfile,
  type DeckRecord,
  linkReportNotice,
  matchesDeckQuery,
  parseLinkReport,
  parseSnapshot,
} from "./deckQuery";
import { dt, dtf } from "./deckStrings";
import { DeckCard, type DeckCardHandle } from "./deckCard";
// The sibling slices' surfaces this page assembles (one page directory,
// one page entity): the deck editor, the unified import dialog, the share
// sheet and the detail column. Each stays in its own file with its own
// facade; this page only opens them and reloads on GroupsChanged.
import { CreateGroupDialogHost, openCreateGroup, openEditGroup } from "./createGroup";
import { DeckImportHost, openDeckImport } from "./deckImport";
import { ShareSheetHost, openShareSheet } from "./shareSheet";
import { DeckDetailDrawer } from "./detailDrawer";

// 卡组页 — skill_cards/mod.rs parity. The deck list page: a toolbar with
// exactly two command entries (导入 / 新建卡组…) plus the count chip and the
// grid/list switch; the wrapping deck grid (the deck card wraps on its own
// — it does NOT ride the shared card_grid tracks); the empty state and the
// no-match state; and the detail-column track beside the pane when a deck
// is selected (the drawer body itself is the detail-drawer slice's file,
// mounted through sliceHooks).
//
// State lives in module-level signals like nav.ts: the shell mounts every
// page exactly once (keep-alive), so this is the page's one entity. nsh
// components run ONCE and do not track what their bodies read
// (docs/jsx-and-components): every reactive read below sits in a JSX
// expression — signal() calls and props reads inside JSX attributes and
// children are the bindings that update; derived values ride helper
// functions called from JSX so they re-run with the props they read.

const EMPTY_SET: ReadonlySet<string> = new Set();

const decks = signal<DeckRecord[]>([]);
const installedSkills = signal<ReadonlySet<string>>(EMPTY_SET);
const railAgents = signal<AgentRailProfile[]>([]);
const searchQuery = signal("");
const viewList = signal(false);
const busy = signal(false);
const loaded = signal(false);
const selectedDeckId = signal<string | null>(null);

/** The filtered list the count chip and the grid both show (query.rs). */
function filterDecks(all: DeckRecord[], query: string): DeckRecord[] {
  return all.filter((deck) => matchesDeckQuery(deck, query));
}

function findDeck(all: DeckRecord[], id: string | null): DeckRecord | null {
  if (id === null) return null;
  return all.find((deck) => deck.id === id) ?? null;
}

/** reload — refresh() parity (mod.rs:82-98): swap all three snapshots in
 * one go, and clear a selection whose deck no longer exists. */
async function refresh(): Promise<void> {
  try {
    const snapshot = parseSnapshot(await SkillCards.snapshot());
    decks.set(snapshot.decks);
    installedSkills.set(new Set(snapshot.installedSkills));
    railAgents.set(snapshot.agents);
    const id = selectedDeckId();
    if (id !== null && !snapshot.decks.some((deck) => deck.id === id)) {
      selectedDeckId.set(null);
    }
    loaded.set(true);
  } catch (err) {
    // A read failure is a notice, not a dead page: the stale list stays.
    publishToast({ tone: "error", message: String(err) });
  }
}

/** The link-job tail — run_link_job parity (mod.rs:155-192): one job at a
 * time (busy swallows further clicks), the partial-failure notice goes to
 * the notice band, and the page refreshes whatever happened. */
async function runLinkJob(job: () => Promise<unknown>): Promise<void> {
  if (busy()) return;
  busy.set(true);
  try {
    const report = parseLinkReport(await job());
    const notice = linkReportNotice(report, dtf);
    if (notice) publishToast({ tone: "warning", message: notice });
  } catch (err) {
    publishToast({ tone: "error", message: String(err) });
  } finally {
    busy.set(false);
    await refresh();
  }
}

/** The group-command tail — run_group_job parity (mod.rs:194-222). */
async function runGroupJob(job: () => Promise<unknown>): Promise<void> {
  if (busy()) return;
  busy.set(true);
  try {
    await job();
  } catch (err) {
    publishToast({ tone: "error", message: String(err) });
  } finally {
    busy.set(false);
    await refresh();
  }
}

function toggleAgentForDeck(deck: DeckRecord, agentId: string): void {
  const linked = (deck.agentLinks ?? []).includes(agentId);
  void runLinkJob(() => SkillCards.toggleDeckAgent(deck.id, agentId, !linked));
}

function duplicateDeck(deck: DeckRecord): void {
  void runGroupJob(() => SkillCards.duplicateDeck(deck.id));
}

function deleteDeck(deck: DeckRecord): void {
  void runGroupJob(() => SkillCards.deleteDeck(deck.id));
}

/** mod.rs:135-147 — the card press opens the deck's detail column; pressing
 * the same card again (or the column's close control) clears it. */
function openDeck(deck: DeckRecord): void {
  selectedDeckId.set(selectedDeckId() === deck.id ? null : deck.id);
}

/** The editor publishes GroupsChanged on commit, which the subscription
 * above answers with a reload — the create_group.rs callback's parity. */
function openDeckEditor(): void {
  openCreateGroup();
}

function openImport(): void {
  openDeckImport();
}

function openShare(deck: DeckRecord): void {
  openShareSheet(deck);
}

const cardHandle: DeckCardHandle = {
  onOpen: openDeck,
  onShare: openShare,
  onDuplicate: duplicateDeck,
  onDelete: deleteDeck,
  onToggleAgent: toggleAgentForDeck,
};

function setViewList(list: boolean): void {
  viewList.set(list);
}

// First mount loads once (the shell mounts this page exactly once); the
// keep-alive reload channels Shell subscribes land here the same way
// shell.rs:79-99 routes GroupsChanged / AgentsChanged to page.refresh().
effect(() => {
  void refresh();
});
effect(() => groupsChanged.subscribe(() => void refresh()));
effect(() => agentsChanged.subscribe(() => void refresh()));

export function SkillCardsPage(): JSX.Element {
  return (
    <View style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0, width: "100%" }}>
      <PageBar
        title={t("sidebar.groups")}
        dragId="skill-cards-toolbar-drag"
        search={
          <BarSearchInput
            boxTestID="deck-search"
            inputTestID="deck-search-input"
            clearTestID="deck-search-clear"
            value={searchQuery()}
            placeholder={dt("skillCards.searchPlaceholder")}
            onChange={(value) => searchQuery.set(value)}
            width={216}
          />
        }
        filters={[
          <View testID="deck-count-filter">
            {/* The count chip mirrors the filtered list, not the store. */}
            <BarCount glyph="layers" label={String(filterDecks(decks(), searchQuery()).length)} />
          </View>,
        ]}
        actions={[
          <If when={busy()}>
            <View
              testID="deck-busy"
              style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}
            >
              <IconSpin name="refresh" size={14} color={paletteToken("accent")} spin />
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {dt("skillCards.processing")}
              </Text>
            </View>
          </If>,
          <BarSecondary
            testID="deck-import-btn"
            glyph="download"
            label={dt("common.import")}
            disabled={busy()}
            onPress={openImport}
          />,
          <BarPrimary
            testID="deck-new-btn"
            glyph="plus"
            label={dt("skillCards.newGroup")}
            disabled={busy()}
            onPress={openDeckEditor}
          />,
          <BarViewToggle
            testID="deck-view-switcher"
            gridTestID="deck-view-grid"
            listTestID="deck-view-list"
            listMode={viewList()}
            onSelect={setViewList}
          />,
        ]}
      />
      {/* Pane fills under the toolbar; the detail column rides beside it,
          exactly like the installed-skill page (mod.rs:466-477). Empty
          states are a flex-1 child of this row, never of the wrap; the
          deck wrap is not flex-1 (mod.rs:337-341). */}
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
        <DeckPane
          didLoad={loaded()}
          isBusy={busy()}
          isList={viewList()}
          all={decks()}
          search={searchQuery()}
          installed={installedSkills()}
          agents={railAgents()}
          selectedId={selectedDeckId()}
        />
        <DrawerTrack
          deck={findDeck(decks(), selectedDeckId())}
          isBusy={busy()}
          installed={installedSkills()}
          agents={railAgents()}
        />
      </View>

      {/* The sibling dialogs float over the page on their own veils; their
          hosts mount here so the keep-alive page owns the whole vertical. */}
      <CreateGroupDialogHost />
      <DeckImportHost />
      <ShareSheetHost />
    </View>
  );
}

interface DeckPaneProps {
  didLoad: boolean;
  isBusy: boolean;
  isList: boolean;
  all: DeckRecord[];
  search: string;
  installed: ReadonlySet<string>;
  agents: AgentRailProfile[];
  selectedId: string | null;
}

/** The scrollable deck pane: empty state / no-match state / the wrapping
 * grid. The deck grid wraps on its own — 卡组卡自己换行，不走 card_grid.
 * Every props read sits in JSX (nsh bindings), so the branch expression
 * re-runs when any of them changes. */
function DeckPane(props: DeckPaneProps): JSX.Element {
  return (
    <If when={props.didLoad} else={<View style={{ flex: 1, width: "100%" }} />}>
      <If
        when={props.all.length === 0}
        else={
          <If
            when={filterDecks(props.all, props.search).length === 0}
            else={<DeckGrid {...props} filtered={filterDecks(props.all, props.search)} />}
          >
            <NoMatchState />
          </If>
        }
      >
        <DeckEmptyState busy={props.isBusy} />
      </If>
    </If>
  );
}

const DECK_LIST_GAP = 8;

/** Virtualized grid/list deck pane wrapped around `List` (nsh). */
function DeckGrid(props: DeckPaneProps & { filtered: DeckRecord[] }): JSX.Element {
  const paneWidth = signal(0);
  const backTopShown = signal(false);
  const listRef = signal<ListRef | null>(null);

  effect(() => {
    void props.isList;
    backTopShown.set(false);
    queueMicrotask(() => listRef()?.scrollToOffset({ offset: 0, animated: false }));
  });

  const stride = (): number => SKILL_CARD_H + (props.isList ? DECK_LIST_GAP : CARD_GAP);
  const trackW = (): number =>
    cardTrackWidth(
      paneWidth(),
      props.isList,
      PAGE_PAD,
      props.selectedId !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W
    );
  const packedRows = computed(() =>
    packCardRows(
      props.filtered,
      gridColumnsForPane(
        paneWidth(),
        props.isList,
        PAGE_PAD,
        props.selectedId !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W
      )
    )
  );

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
        ref={(node) => listRef.set(node)}
        testID="deck-grid"
        each={packedRows()}
        keyExtractor={(_row, index) => `deck-row-${index}`}
        estimatedItemSize={stride()}
        style={{ width: "100%", height: "100%" }}
        contentStyle={{
          paddingTop: SKILL_SCROLL_PT,
          paddingBottom: SKILL_SCROLL_PB,
          paddingLeft: PAGE_PAD,
          paddingRight: PAGE_PAD,
        }}
        scrollEventThrottle={32}
        onScroll={(event) => {
          const past = event.offset.y > SHOW_AFTER_PX;
          if (past !== backTopShown()) backTopShown.set(past);
        }}
      >
        {(row, index) => (
          <View
            testID={`deck-row-${index()}`}
            style={{
              display: "flex",
              flexDirection: "row",
              justifyContent: "flex-start",
              alignItems: "flex-start",
              gap: props.isList ? 0 : CARD_GAP,
              height: stride(),
              paddingBottom: props.isList ? DECK_LIST_GAP : CARD_GAP,
              width: "100%",
            }}
          >
            <Each of={row()} key="index">
              {(deck: () => DeckRecord) => (
                <View
                  style={
                    props.isList
                      ? { width: "100%", minWidth: 0 }
                      : {
                          width: trackW(),
                          minWidth: trackW(),
                          maxWidth: trackW(),
                          flexShrink: 0,
                          flexGrow: 0,
                        }
                  }
                >
                  <DeckCard
                    deck={deck()}
                    selected={props.selectedId === deck().id}
                    viewList={props.isList}
                    cardWidth={trackW()}
                    busy={props.isBusy}
                    installedSkills={props.installed}
                    agents={props.agents}
                    handle={cardHandle}
                  />
                </View>
              )}
            </Each>
          </View>
        )}
      </List>
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

/** 还没有卡组 — the shared empty glyph (the sidebar's deck icon), a 56px
 * accent-soft disc, the two-line pitch, and the create CTA
 * (mod.rs:343-392). */
function DeckEmptyState(props: { busy: boolean }): JSX.Element {
  return (
    <View
      testID="deck-empty"
      style={{
        flex: 1,
        width: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 12,
      }}
    >
      <View
        style={{
          width: 56,
          height: 56,
          borderRadius: 16,
          backgroundColor: paletteToken("accent_soft"),
          borderWidth: 1,
          borderColor: paletteToken("accent"),
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <Icon name="deck" size={28} color={paletteToken("accent_fg")} />
      </View>
      <Text style={{ fontSize: 18, fontWeight: "700", color: paletteToken("fg") }}>
        {dt("skillCards.emptyTitle")}
      </Text>
      <Text
        style={{
          maxWidth: 360,
          fontSize: 12,
          textAlign: "center",
          color: paletteToken("fg_muted"),
        }}
      >
        {dt("skillCards.emptyDesc")}
      </Text>
      <BarPrimary
        testID="deck-empty-create"
        glyph="plus"
        label={dt("skillCards.createFirst")}
        disabled={props.busy}
        onPress={openDeckEditor}
      />
    </View>
  );
}

/** 没有匹配的卡组 (mod.rs:393-419). */
function NoMatchState(): JSX.Element {
  return (
    <View
      testID="deck-no-match"
      style={{
        flex: 1,
        width: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
      }}
    >
      <Icon name="search" size={32} color={paletteToken("fg_muted")} />
      <Text style={{ fontSize: 16, fontWeight: "500", color: paletteToken("fg") }}>
        {dt("skillCards.noMatching")}
      </Text>
      <Text
        style={{
          maxWidth: 360,
          fontSize: 12,
          textAlign: "center",
          color: paletteToken("fg_muted"),
        }}
      >
        {dt("skillCards.tryDifferent")}
      </Text>
    </View>
  );
}

/** The detail-column track beside the pane — exactly like the installed
 * skill and marketplace pages: the pane keeps its flex-1, the track takes
 * the fixed column width, and the drawer appears in ONE repaint. The body
 * is the detail-drawer slice's component (detailDrawer.tsx); this page
 * owns the selection and hands over the same snapshot it renders the grid
 * from, so the card and the column can never disagree. */
function DrawerTrack(props: {
  deck: DeckRecord | null;
  isBusy: boolean;
  installed: ReadonlySet<string>;
  agents: AgentRailProfile[];
}): JSX.Element {
  return (
    <If when={props.deck !== null}>
      <DeckDetailDrawer
        deck={props.deck as DeckRecord}
        installedSkills={props.installed}
        agents={props.agents}
        busy={props.isBusy}
        onClose={() => selectedDeckId.set(null)}
        onEdit={(deck) => openEditGroup(deck)}
        onShare={(deck) => {
          // The drawer hands back its narrower shape; the share sheet also
          // wants skillSources, so hand it the page's own record.
          openShareSheet(
            decks().find((record) => record.id === deck.id) ?? { ...deck, skillSources: {} }
          );
        }}
        onMutated={() => void refresh()}
      />
    </If>
  );
}

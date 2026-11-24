import {
  List,
  Text,
  View,
  computed,
  effect,
  signal,
  untracked,
  type Accessor,
  type JSX,
  type ListRef,
} from "nsh";
import { SkillCards } from "skillstar-gui/native";
import { paletteToken } from "../../services/themeMode";
import { t } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { Pulse } from "../../chrome/feedback";
import { bindDelayedFlag } from "../../chrome/delayedFlag";
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
  SKILL_CARD_W,
  SKILL_SCROLL_PB,
  SKILL_SCROLL_PT,
} from "../../layout";
import { currentPage, createPaneWidth } from "../../shell/nav";
import { toast, publishToast, groupsChanged, agentsChanged } from "../../shell/events";
import {
  type AgentRailProfile,
  type DeckRecord,
  deckAgentLinkState,
  linkReportNotice,
  matchesDeckQuery,
  nextEnableForAgentState,
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
const EMPTY_PENDING: Record<string, true> = {};

const decks = signal<DeckRecord[]>([]);
const installedSkills = signal<ReadonlySet<string>>(EMPTY_SET);
const railAgents = signal<AgentRailProfile[]>([]);
const searchQuery = signal("");
const viewList = signal(false);
const loaded = signal(false);
const selectedDeckId = signal<string | null>(null);
/** Deck id whose copy or delete is in flight. Null when idle. */
const rowBusy = signal<string | null>(null);
/** `${deckId}:${agentId}` slots that already accepted a toggle. */
const linkPending = signal<Record<string, true>>({});

const pageActive = () => currentPage() === "skill_cards";

function sameElements<T>(a: readonly T[], b: readonly T[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function gated<T>(source: Accessor<T>, equals?: (a: T, b: T) => boolean): Accessor<T> {
  let last = untracked(source);
  return computed(
    () => {
      if (!pageActive()) return last;
      last = source();
      return last;
    },
    equals ? { equals } : undefined,
  );
}

function stringArraysEqual(a: readonly string[] | null, b: readonly string[] | null): boolean {
  if (a === b) return true;
  if (a === null || b === null || a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

function stringRecordsEqual(
  a: Readonly<Record<string, string>>,
  b: Readonly<Record<string, string>>,
): boolean {
  if (a === b) return true;
  const keysA = Object.keys(a);
  if (keysA.length !== Object.keys(b).length) return false;
  for (const key of keysA) {
    if (a[key] !== b[key]) return false;
  }
  return true;
}

function decksEqual(a: DeckRecord, b: DeckRecord): boolean {
  return (
    a.id === b.id &&
    a.name === b.name &&
    a.description === b.description &&
    a.icon === b.icon &&
    a.defaultAgent === b.defaultAgent &&
    a.createdAt === b.createdAt &&
    a.updatedAt === b.updatedAt &&
    stringArraysEqual(a.skills, b.skills) &&
    stringArraysEqual(a.agentLinks, b.agentLinks) &&
    stringRecordsEqual(a.skillSources, b.skillSources) &&
    stringRecordsEqual(a.agentLinkStates ?? {}, b.agentLinkStates ?? {}) &&
    memberAgentLinksEqual(a.memberAgentLinks, b.memberAgentLinks)
  );
}

function memberAgentLinksEqual(
  a?: Readonly<Record<string, readonly string[]>>,
  b?: Readonly<Record<string, readonly string[]>>,
): boolean {
  if (a === b) return true;
  if (!a || !b) return !a && !b;
  const keysA = Object.keys(a);
  if (keysA.length !== Object.keys(b).length) return false;
  for (const key of keysA) {
    if (!stringArraysEqual(a[key] ?? null, b[key] ?? null)) return false;
  }
  return true;
}

function mergeDecks(prev: readonly DeckRecord[], next: readonly DeckRecord[]): DeckRecord[] {
  const byId = new Map(prev.map((item) => [item.id, item]));
  let changed = prev.length !== next.length;
  const merged = next.map((item, idx) => {
    const existing = byId.get(item.id);
    if (existing && decksEqual(existing, item)) {
      if (prev[idx] !== existing) changed = true;
      return existing;
    }
    changed = true;
    return item;
  });
  return changed ? merged : (prev as DeckRecord[]);
}

function mergeAgents(
  prev: readonly AgentRailProfile[],
  next: readonly AgentRailProfile[],
): AgentRailProfile[] {
  const byId = new Map(prev.map((item) => [item.id, item]));
  let changed = prev.length !== next.length;
  const merged = next.map((item, idx) => {
    const existing = byId.get(item.id);
    if (
      existing &&
      existing.id === item.id &&
      existing.displayName === item.displayName &&
      existing.icon === item.icon
    ) {
      if (prev[idx] !== existing) changed = true;
      return existing;
    }
    changed = true;
    return item;
  });
  return changed ? merged : (prev as AgentRailProfile[]);
}

function setsEqual(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a === b) return true;
  if (a.size !== b.size) return false;
  for (const item of a) {
    if (!b.has(item)) return false;
  }
  return true;
}

function shallowPendingEqual(
  a: Readonly<Record<string, true>>,
  b: Readonly<Record<string, true>>,
): boolean {
  if (a === b) return true;
  const keysA = Object.keys(a);
  if (keysA.length !== Object.keys(b).length) return false;
  for (const k of keysA) {
    if (b[k] !== true) return false;
  }
  return true;
}

/** The filtered list the count chip and the grid both show (query.rs). */
function filterDecks(all: DeckRecord[], query: string): DeckRecord[] {
  return all.filter((deck) => matchesDeckQuery(deck, query));
}

const filteredDecks = computed(() => filterDecks(decks(), searchQuery()));
const gatedFilteredDecks = gated(filteredDecks, sameElements);
const gatedInstalled = gated(installedSkills);
const gatedAgents = gated(railAgents, sameElements);
const gatedViewList = gated(viewList);
const gatedSelectedId = gated(selectedDeckId);
const gatedLoaded = gated(loaded);
const gatedAllDecksCount = gated(() => decks().length);
const gatedSelectedDeck = computed(() => findDeck(decks(), gatedSelectedId()));

function findDeck(all: DeckRecord[], id: string | null): DeckRecord | null {
  if (id === null) return null;
  return all.find((deck) => deck.id === id) ?? null;
}

/** reload — refresh() parity (mod.rs:82-98): swap all three snapshots in
 * one go, and clear a selection whose deck no longer exists. */
async function refresh(): Promise<void> {
  try {
    const snapshot = parseSnapshot(await SkillCards.snapshot());
    const mergedDecks = mergeDecks(untracked(decks), snapshot.decks);
    if (mergedDecks !== untracked(decks)) decks.set(mergedDecks);
    const nextInstalled = new Set(snapshot.installedSkills);
    if (!setsEqual(untracked(installedSkills), nextInstalled)) {
      installedSkills.set(nextInstalled);
    }
    const mergedAgents = mergeAgents(untracked(railAgents), snapshot.agents);
    if (mergedAgents !== untracked(railAgents)) railAgents.set(mergedAgents);
    const id = selectedDeckId();
    if (id !== null && !snapshot.decks.some((deck) => deck.id === id)) {
      selectedDeckId.set(null);
    }
  } catch (err) {
    // A read failure is a notice, not a dead page: the stale list stays.
    toast(err instanceof Error ? err.message : String(err), "error");
  } finally {
    loaded.set(true);
  }
}

/** Copy or delete one deck. The spinner sits on that card; the list updates
 * only after the call returns (no optimistic removal). */
async function runRowJob(deckId: string, job: () => Promise<unknown>): Promise<void> {
  if (rowBusy() !== null) return;
  rowBusy.set(deckId);
  try {
    await job();
  } catch (err) {
    toast(err instanceof Error ? err.message : String(err), "error");
  } finally {
    await refresh();
    rowBusy.set(null);
  }
}

/** One rail-slot press (the deck card's optimistic toggle). The card face
 * paints no save-status row: the slot's own flip is the result. A skipped or
 * failed link rolls back and is explained by the notice band; pressing the
 * same slot again is the retry — the my-skills SkillCard contract. */
function toggleAgentForDeck(deck: DeckRecord, agentId: string): void {
  const key = `${deck.id}:${agentId}`;
  if (linkPending()[key]) return;
  const current = decks().find((item) => item.id === deck.id) ?? deck;
  const profile = railAgents().find((a) => a.id === agentId) ?? { id: agentId, displayName: agentId };
  const prevState = deckAgentLinkState(current, profile);
  const enable = nextEnableForAgentState(prevState);
  const nextLinks = enable
    ? [...new Set([...(current.agentLinks ?? []), agentId])]
    : (current.agentLinks ?? []).filter(
        (id) =>
          id.trim().toLowerCase() !== profile.id.trim().toLowerCase() &&
          id.trim().toLowerCase() !== profile.displayName.trim().toLowerCase()
      );
  const nextStates = current.agentLinkStates
    ? { ...current.agentLinkStates, [agentId]: enable ? ("linked" as const) : ("none" as const) }
    : undefined;
  const prevLinks = current.agentLinks;
  const prevStates = current.agentLinkStates;
  linkPending.set({ ...linkPending(), [key]: true });
  decks.set(
    decks().map((item) =>
      item.id === current.id
        ? { ...item, agentLinks: nextLinks, agentLinkStates: nextStates }
        : item
    )
  );
  const rollback = () => {
    decks.set(
      decks().map((item) =>
        item.id === current.id
          ? { ...item, agentLinks: prevLinks, agentLinkStates: prevStates }
          : item
      )
    );
  };
  void (async () => {
    try {
      const report = parseLinkReport(
        await SkillCards.toggleDeckAgent(current.id, agentId, enable)
      );
      const notice = linkReportNotice(report, dtf);
      if (notice) {
        rollback();
        publishToast({ tone: "warning", message: notice });
        return;
      }
    } catch (err: unknown) {
      rollback();
      toast(err instanceof Error ? err.message : String(err), "error");
    } finally {
      // A rail click links (or unlinks) the deck's member Skills on disk, so
      // every page that paints per-Skill agent links has to reload its own
      // snapshot: the keep-alive skills page never remounts. The deck drawer's
      // link jobs announce the same change (detailDrawer.tsx finish()).
      groupsChanged.publish();
      const next = { ...linkPending() };
      delete next[key];
      linkPending.set(next);
    }
  })();
}

function duplicateDeck(deck: DeckRecord): void {
  void runRowJob(deck.id, () => SkillCards.duplicateDeck(deck.id));
}

function deleteDeck(deck: DeckRecord): void {
  void runRowJob(deck.id, () => SkillCards.deleteDeck(deck.id));
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

let refreshDirty = true;
let refreshing = false;
let refreshAgain = false;

async function refreshNow(): Promise<void> {
  if (refreshing) {
    refreshAgain = true;
    return;
  }
  refreshing = true;
  try {
    do {
      refreshAgain = false;
      refreshDirty = false;
      await refresh();
    } while (refreshAgain && pageActive());
  } finally {
    refreshing = false;
  }
}

function requestRefresh(): void {
  if (!pageActive()) {
    refreshDirty = true;
    return;
  }
  void refreshNow();
}

// Keep-alive reload channels defer off-page snapshots until skill_cards is
// active again; initial mount also waits until the user enters skill_cards.
effect(() => {
  if (pageActive() && refreshDirty) {
    void refreshNow();
  }
});
effect(() => groupsChanged.subscribe(requestRefresh));
effect(() => agentsChanged.subscribe(requestRefresh));

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
          />
        }
        filters={[
          <View testID="deck-count-filter">
            {/* The count chip mirrors the filtered list, not the store. */}
            <BarCount glyph="layers" label={String(filteredDecks().length)} />
          </View>,
        ]}
        actions={[
          <BarSecondary
            testID="deck-import-btn"
            glyph="download"
            label={dt("common.import")}
            onPress={openImport}
          />,
          <BarPrimary
            testID="deck-new-btn"
            glyph="plus"
            label={dt("skillCards.newGroup")}
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
        <DeckPane />
        <DrawerTrack />
      </View>

      {/* The sibling dialogs float over the page on their own veils; their
          hosts mount here so the keep-alive page owns the whole vertical. */}
      <CreateGroupDialogHost />
      <DeckImportHost />
      <ShareSheetHost />
    </View>
  );
}

/** The scrollable deck pane: blank until the first read settles, a pulse
 * skeleton only if that read is slow, then the empty state, the no-match
 * state, or the wrapping grid. Local filtering does not add a wait. */
function DeckPane(): JSX.Element {
  const showSkeleton = bindDelayedFlag(() => pageActive() && !gatedLoaded());
  const isEmpty = computed(
    () => !showSkeleton() && gatedLoaded() && gatedAllDecksCount() === 0,
  );
  const isNoMatch = computed(
    () =>
      !showSkeleton() &&
      gatedLoaded() &&
      gatedAllDecksCount() > 0 &&
      gatedFilteredDecks().length === 0,
  );
  const hasRows = computed(
    () => !showSkeleton() && gatedLoaded() && gatedFilteredDecks().length > 0,
  );
  return (
    <View style={{ flex: 1, minWidth: 0, minHeight: 0, width: "100%" }}>
      <If when={showSkeleton()}>
        <DeckSkeleton list={gatedViewList()} />
      </If>
      <If when={isEmpty()}>
        <DeckEmptyState />
      </If>
      <If when={isNoMatch()}>
        <NoMatchState />
      </If>
      <If when={hasRows()}>
        <DeckGrid />
      </If>
    </View>
  );
}

const DECK_LIST_GAP = 8;
const SKELETON_INDICES = [0, 1, 2, 3, 4, 5];

/** Six pulse cards at the real deck-card size. Lives in this page so it
 * does not borrow another page's skeleton. */
function DeckSkeleton(props: { list: boolean }): JSX.Element {
  return (
    <View
      testID="deck-skeleton"
      accessibilityState={{ busy: true }}
      style={{
        display: "flex",
        flexDirection: props.list ? "column" : "row",
        flexWrap: "wrap",
        gap: props.list ? DECK_LIST_GAP : CARD_GAP,
        alignItems: "flex-start",
        paddingTop: SKILL_SCROLL_PT,
        paddingBottom: SKILL_SCROLL_PB,
        paddingLeft: PAGE_PAD,
        paddingRight: PAGE_PAD,
        width: "100%",
      }}
    >
      <Each of={SKELETON_INDICES} key={(index) => String(index)}>
        {(index: () => number) => (
          <View
            style={{
              width: props.list ? "100%" : SKILL_CARD_W,
              height: SKILL_CARD_H,
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
            <Pulse id={`deck-skel-${index()}-title`} style={{ height: 16, width: 140, borderRadius: 6 }} />
            <Pulse id={`deck-skel-${index()}-line`} secondary style={{ height: 12, borderRadius: 6 }} />
            <Pulse
              id={`deck-skel-${index()}-meta`}
              secondary
              style={{ height: 12, width: 200, borderRadius: 6 }}
            />
          </View>
        )}
      </Each>
    </View>
  );
}

/** Virtualized grid/list deck pane wrapped around `List` (nsh). */
function DeckGrid(): JSX.Element {
  const pane = createPaneWidth({
    active: pageActive,
    fromShell: (panelW) => panelW - (gatedSelectedId() !== null ? DETAIL_COLUMN_W : 0),
  });
  const paneWidth = pane.width;
  const backTopShown = signal(false);
  const listRef = signal<ListRef | null>(null);

  effect(() => {
    void gatedViewList();
    backTopShown.set(false);
    queueMicrotask(() => listRef()?.scrollToOffset({ offset: 0, animated: false }));
  });

  const stride = computed(
    () => SKILL_CARD_H + (gatedViewList() ? DECK_LIST_GAP : CARD_GAP),
  );
  const trackW = computed(() =>
    cardTrackWidth(
      paneWidth(),
      gatedViewList(),
      PAGE_PAD,
      gatedSelectedId() !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W,
    ),
  );
  const packedRows = computed(() =>
    packCardRows(
      gatedFilteredDecks(),
      gridColumnsForPane(
        paneWidth(),
        gatedViewList(),
        PAGE_PAD,
        gatedSelectedId() !== null ? CARD_ROW_W - DETAIL_COLUMN_W : CARD_ROW_W,
      ),
    ),
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
      onLayout={pane.onLayout}
    >
      <List
        ref={(node) => listRef.set(node)}
        testID="deck-grid"
        each={packedRows()}
        keyExtractor={(row) => row.map((d) => d.id).join("\u001e")}
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
              gap: gatedViewList() ? 0 : CARD_GAP,
              height: stride(),
              paddingBottom: gatedViewList() ? DECK_LIST_GAP : CARD_GAP,
              width: "100%",
            }}
          >
            <Each of={row()} key={(deck) => deck.id}>
              {(deck: () => DeckRecord) => (
                <DeckCardSlot deck={deck} isList={gatedViewList} cardWidth={trackW} />
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

function DeckCardSlot(props: {
  deck: Accessor<DeckRecord>;
  isList: Accessor<boolean>;
  cardWidth: Accessor<number>;
}): JSX.Element {
  const deckId = computed(() => props.deck().id);
  const selected = computed(() => gatedSelectedId() === deckId());
  const busy = computed(() => rowBusy() === deckId());
  const cardPending = computed<Record<string, true>>(
    () => {
      const prefix = `${deckId()}:`;
      const all = linkPending();
      let out: Record<string, true> | null = null;
      for (const k of Object.keys(all)) {
        if (k.startsWith(prefix)) {
          if (out === null) out = {};
          out[k] = true;
        }
      }
      return out ?? EMPTY_PENDING;
    },
    { equals: shallowPendingEqual },
  );
  return (
    <View
      style={
        props.isList()
          ? { width: "100%", minWidth: 0 }
          : {
              width: props.cardWidth(),
              minWidth: props.cardWidth(),
              maxWidth: props.cardWidth(),
              flexShrink: 0,
              flexGrow: 0,
            }
      }
    >
      <DeckCard
        deck={props.deck()}
        selected={selected()}
        viewList={props.isList()}
        cardWidth={props.cardWidth()}
        rowBusy={busy()}
        linkPending={cardPending()}
        installedSkills={gatedInstalled()}
        agents={gatedAgents()}
        handle={cardHandle}
      />
    </View>
  );
}

/** 还没有卡组 — the shared empty glyph (the sidebar's deck icon), a 56px
 * accent-soft disc, the two-line pitch, and the create CTA
 * (mod.rs:343-392). */
function DeckEmptyState(): JSX.Element {
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
function DrawerTrack(): JSX.Element {
  return (
    <If when={gatedSelectedDeck() !== null}>
      <DeckDetailDrawer
        deck={gatedSelectedDeck() as DeckRecord}
        installedSkills={gatedInstalled()}
        agents={gatedAgents()}
        onClose={() => selectedDeckId.set(null)}
        onEdit={(deck) => openEditGroup(deck)}
        onShare={(deck) => {
          // The drawer hands back its narrower shape; the share sheet also
          // wants skillSources, so hand it the page's own record.
          openShareSheet(
            decks().find((record) => record.id === deck.id) ?? { ...deck, skillSources: {} },
          );
        }}
        onMutated={() => {}}
      />
    </If>
  );
}

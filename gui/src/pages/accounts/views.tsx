// 账号页多视图（网格 / 列表 / 紧凑）；状态占位与骨架见 states.tsx。
// 三态视图均走 nsh <List> 行虚拟化 + 按 index/id 复用的 <Each>，并按行级 computed 收窄信号扇出，
// 勾选/单卡刷新/后台更新只刷新受影响卡片的局部绑定，不再整表重建。

import {
  List,
  Text,
  View,
  computed,
  effect,
  signal,
  type Accessor,
  type ContextMenuEvent,
  type JSX,
  type ListRef,
} from "nsh";
import { gridColumnsForPane, packCardRows } from "../../chrome/cardShell";
import { Icon } from "../../chrome/icons";
import { CARD_GAP, CARD_ROW_W, PAGE_PAD, SKILL_CARD_W } from "../../layout";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { createPaneWidth } from "../../shell/nav";
import {
  AccountActionButtons,
  AccountCard,
  LastUsedMeta,
  LoginSubline,
  PlanBadge,
  QuotaErrorBadge,
  ReauthRow,
  SelectCheckbox,
  ServingBadge,
} from "./card";
import {
  MAX_PAIRED_HEAD_CHARS,
  accountName,
  clampPercent,
  formatCreditValue,
  hasQuota,
  maskAccountText,
  percentText,
  remainingTone,
  resetCardCount,
  windowHeadLength,
  windowLabel,
  windowOrder,
} from "./logic";
import { openAccountContextMenu } from "./accountContextMenu";
import { toneColor } from "./meters";
import type { AccountView, UsageWindow } from "./types";

export interface AccountsViewProps {
  active?: () => boolean;
  accounts: AccountView[];
  agentId?: string;
  servingId: string | null;
  busyId: string | null;
  batchBusy: boolean;
  refreshingIds: string[];
  deletingId: string | null;
  selectedIds: string[];
  masked: boolean;
  onToggleSelect: (id: string) => void;
  onRefresh: (id: string) => void;
  onActivate: (id: string) => void;
  onDelete: (id: string) => void;
  onReauth: (id: string) => void;
}

const EMPTY_SET: ReadonlySet<string> = new Set();

function extrasSummary(account: AccountView, separator: string): string {
  const parts: string[] = [];
  const credits = account.usage?.credits;
  if (credits) {
    parts.push(`${credits.label}${separator}${formatCreditValue(credits, t("accountsPage.unlimited"))}`);
  }
  const resetCards = resetCardCount(account.usage);
  if (resetCards !== null) {
    parts.push(
      `${t("accountsPage.resetCards")}${separator}${tf("accountsPage.resetCardsCount", { count: resetCards })}`,
    );
  }
  return parts.join(" · ");
}

function useViewLookups(props: AccountsViewProps) {
  const selectedSet = computed<ReadonlySet<string>>(() =>
    props.selectedIds.length === 0 ? EMPTY_SET : new Set(props.selectedIds),
  );
  const refreshingSet = computed<ReadonlySet<string>>(() =>
    props.refreshingIds.length === 0 ? EMPTY_SET : new Set(props.refreshingIds),
  );
  return { selectedSet, refreshingSet };
}

interface SlotContext {
  account: Accessor<AccountView>;
  selectedSet: Accessor<ReadonlySet<string>>;
  refreshingSet: Accessor<ReadonlySet<string>>;
  props: AccountsViewProps;
}

function useSlotFlags(slot: SlotContext) {
  const serving = computed(() => slot.props.servingId === slot.account().id);
  const selected = computed(() => slot.selectedSet().has(slot.account().id));
  const refreshing = computed(() => slot.refreshingSet().has(slot.account().id));
  const deleting = computed(() => slot.props.deletingId === slot.account().id);
  const busy = computed(
    () => slot.props.busyId === slot.account().id || slot.props.batchBusy || refreshing(),
  );
  return { serving, selected, refreshing, deleting, busy };
}

function openSlotContextMenu(
  e: ContextMenuEvent,
  slot: SlotContext,
  flags: ReturnType<typeof useSlotFlags>,
): void {
  const acct = slot.account();
  openAccountContextMenu(e, {
    account: acct,
    serving: flags.serving(),
    busy: flags.busy(),
    refreshing: flags.refreshing(),
    onActivate: () => slot.props.onActivate(acct.id),
    onRefresh: () => slot.props.onRefresh(acct.id),
    onReauth: () => slot.props.onReauth(acct.id),
    onDelete: () => slot.props.onDelete(acct.id),
  });
}

/** 网格与紧凑视图共用的虚拟化多列行列表。 */
function VirtualizedCardGrid(opts: {
  testID: string;
  rowKeyPrefix: string;
  estimatedItemSize: number;
  props: AccountsViewProps;
  renderCard: (
    acct: Accessor<AccountView>,
    cardWidth: Accessor<number>,
    selectedSet: Accessor<ReadonlySet<string>>,
    refreshingSet: Accessor<ReadonlySet<string>>,
  ) => JSX.Element;
}): JSX.Element {
  const pane = createPaneWidth({
    active: opts.props.active,
    fromShell: (panelW) => Math.min(CARD_ROW_W, Math.max(0, panelW - PAGE_PAD * 2)),
  });
  const paneWidth = pane.width;
  const listRef = signal<ListRef | null>(null);
  const { selectedSet, refreshingSet } = useViewLookups(opts.props);

  effect(() => {
    void opts.props.agentId;
    listRef()?.scrollToOffset({ offset: 0, animated: false });
  });

  const columns = computed(() => gridColumnsForPane(paneWidth(), false, 0, CARD_ROW_W));
  const cardWidth = computed(() => {
    const w = paneWidth();
    return w > 0 && w < SKILL_CARD_W ? w : SKILL_CARD_W;
  });
  const packedRows = computed(() => packCardRows(opts.props.accounts, columns()));

  return (
    <View
      testID={opts.testID}
      style={{ display: "flex", flex: 1, minHeight: 0, width: "100%" }}
      onLayout={pane.onLayout}
    >
      <List
        ref={(node) => listRef.set(node)}
        testID="acct-scroll"
        each={packedRows()}
        keyExtractor={(row) => `${opts.rowKeyPrefix}${row.map((acct) => acct.id).join("\u001e")}`}
        estimatedItemSize={opts.estimatedItemSize}
        scrollEventThrottle={32}
        style={{ width: "100%", height: "100%" }}
        contentStyle={{ paddingBottom: PAGE_PAD - CARD_GAP }}
      >
        {(row) => (
          <View
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "stretch",
              gap: CARD_GAP,
              paddingBottom: CARD_GAP,
              width: "100%",
            }}
          >
            <Each of={row()} key="index">
              {(acct) => opts.renderCard(acct, cardWidth, selectedSet, refreshingSet)}
            </Each>
          </View>
        )}
      </List>
    </View>
  );
}

/** 网格卡片视图：基于 <List> 按行虚拟化（桌面 3 列 1040px，窄窗自适应列数）。 */
export function AccountGridView(props: AccountsViewProps): JSX.Element {
  return (
    <VirtualizedCardGrid
      testID="acct-grid-view"
      rowKeyPrefix="acct-grid-row-"
      estimatedItemSize={244}
      props={props}
      renderCard={(acct, cardWidth, selectedSet, refreshingSet) => (
        <GridCardSlot
          account={acct}
          cardWidth={cardWidth}
          selectedSet={selectedSet}
          refreshingSet={refreshingSet}
          props={props}
        />
      )}
    />
  );
}

function GridCardSlot(slot: SlotContext & { cardWidth: Accessor<number> }): JSX.Element {
  const { serving, selected, refreshing, deleting, busy } = useSlotFlags(slot);
  return (
    <AccountCard
      account={slot.account()}
      cardWidth={slot.cardWidth()}
      serving={serving()}
      busy={busy()}
      refreshing={refreshing()}
      deleting={deleting()}
      selected={selected()}
      masked={slot.props.masked}
      onToggleSelect={() => slot.props.onToggleSelect(slot.account().id)}
      onRefresh={() => slot.props.onRefresh(slot.account().id)}
      onActivate={() => slot.props.onActivate(slot.account().id)}
      onDelete={() => slot.props.onDelete(slot.account().id)}
      onReauth={() => slot.props.onReauth(slot.account().id)}
    />
  );
}

/** 紧凑卡片视图：基于 <List> 按行虚拟化，单卡压缩至姓名+徽章+单行状态芯片+底部操作。 */
export function AccountCompactView(props: AccountsViewProps): JSX.Element {
  return (
    <VirtualizedCardGrid
      testID="acct-compact-view"
      rowKeyPrefix="acct-compact-row-"
      estimatedItemSize={152}
      props={props}
      renderCard={(acct, cardWidth, selectedSet, refreshingSet) => (
        <CompactAccountCard
          account={acct}
          cardWidth={cardWidth}
          selectedSet={selectedSet}
          refreshingSet={refreshingSet}
          props={props}
        />
      )}
    />
  );
}

function CompactAccountCard(slot: SlotContext & { cardWidth: Accessor<number> }): JSX.Element {
  const flags = useSlotFlags(slot);
  const { serving, selected, refreshing, deleting, busy } = flags;
  const windows = computed(() =>
    slot.account().usage ? windowOrder(slot.account().usage!.windows) : [],
  );
  const resetCards = computed(() => resetCardCount(slot.account().usage));
  const emptyError = () => (slot.account().usage?.error ?? "").trim();

  return (
    <View
      testID={`acct-card-${slot.account().id}`}
      accessibilityState={{ busy: refreshing() || deleting() }}
      onContextMenu={(e) => openSlotContextMenu(e, slot, flags)}
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        width: slot.cardWidth(),
        paddingVertical: 10,
        paddingHorizontal: 12,
        borderRadius: 12,
        borderWidth: 1,
        backgroundColor: selected() ? paletteToken("card_active") : paletteToken("card"),
        borderColor:
          selected() || serving() ? paletteToken("accent") : paletteToken("border"),
        overflow: "hidden",
      }}
      hoverStyle={{
        backgroundColor: selected()
          ? paletteToken("card_active_hover")
          : paletteToken("card_hover"),
        borderColor: selected() || serving() ? paletteToken("accent") : paletteToken("edge"),
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 6,
        }}
      >
        <SelectCheckbox
          testID={`acct-select-${slot.account().id}`}
          checked={selected()}
          label={accountName(slot.account()) || t("accountsPage.noUsername")}
          onPress={() => slot.props.onToggleSelect(slot.account().id)}
        />
        <Text
          testID={`acct-name-${slot.account().id}`}
          style={{
            flexShrink: 1,
            minWidth: 0,
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 13,
            fontWeight: "600",
            color: paletteToken("fg"),
          }}
        >
          {accountName(slot.account())
            ? maskAccountText(accountName(slot.account()), slot.props.masked)
            : t("accountsPage.noUsername")}
        </Text>
        <If when={serving()}>
          <ServingBadge id={slot.account().id} compact />
        </If>
        <If when={Boolean(slot.account().usage?.error)}>
          <QuotaErrorBadge id={slot.account().id} error={slot.account().usage?.error ?? ""} />
        </If>
        <View style={{ marginLeft: "auto", flexShrink: 0 }}>
          <PlanBadge id={slot.account().id} account={slot.account()} compact />
        </View>
      </View>

      <LoginSubline account={slot.account()} masked={slot.props.masked} compact />

      <If
        when={slot.account().requiresReauth}
        else={
          <If
            when={windows().length > 0}
            else={
              <If
                when={Boolean(slot.account().usage?.credits) || resetCards() !== null}
                else={
                  <Text
                    style={{
                      fontSize: 11,
                      color:
                        emptyError().length > 0
                          ? paletteToken("danger_fg")
                          : paletteToken("fg_faint"),
                      numberOfLines: 1,
                      textOverflow: "ellipsis",
                    }}
                  >
                    {emptyError() || t("accountsPage.noQuota")}
                  </Text>
                }
              >
                <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                  {extrasSummary(slot.account(), " ")}
                </Text>
              </If>
            }
          >
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                flexWrap: "wrap",
                gap: 6,
              }}
            >
              <Each of={windows()} key="index">
                {(w) => <CompactWindowChip window={w} />}
              </Each>
              <If when={resetCards() !== null}>
                <CompactResetCardsChip count={() => resetCards()!} />
              </If>
            </View>
          </If>
        }
      >
        <ReauthRow
          id={slot.account().id}
          busy={busy()}
          onReauth={() => slot.props.onReauth(slot.account().id)}
        />
      </If>

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 6,
          marginTop: "auto",
          paddingTop: 6,
          borderTopWidth: 1,
          borderTopColor: paletteToken("border_soft"),
        }}
      >
        <LastUsedMeta account={slot.account()} />
        <AccountActionButtons
          account={slot.account()}
          serving={serving()}
          busy={busy()}
          refreshing={refreshing()}
          deleting={deleting()}
          onRefresh={() => slot.props.onRefresh(slot.account().id)}
          onActivate={() => slot.props.onActivate(slot.account().id)}
          onDelete={() => slot.props.onDelete(slot.account().id)}
        />
      </View>
    </View>
  );
}

function CompactWindowChip(props: { window: Accessor<UsageWindow> }): JSX.Element {
  const color = () => toneColor(remainingTone(props.window().remainingPercent));
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 5,
        maxWidth: "100%",
        minWidth: 0,
        paddingHorizontal: 7,
        paddingVertical: 3,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: paletteToken("border_soft"),
        backgroundColor: paletteToken("well"),
      }}
    >
      <View
        style={{
          width: 6,
          height: 6,
          borderRadius: 999,
          backgroundColor: color(),
          flexShrink: 0,
        }}
      />
      <Text
        style={{
          flexShrink: 1,
          minWidth: 0,
          numberOfLines: 1,
          textOverflow: "ellipsis",
          fontSize: 11,
          color: paletteToken("fg_muted"),
        }}
      >
        {windowLabel(props.window())}
      </Text>
      <Text
        testID={`acct-pct-${props.window().key}`}
        style={{
          flexShrink: 0,
          fontSize: 11,
          fontWeight: "700",
          color: color(),
          fontVariant: "tabular-nums",
        }}
      >
        {percentText(props.window().remainingPercent)}
      </Text>
    </View>
  );
}

function CompactResetCardsChip(props: { count: Accessor<number> }): JSX.Element {
  const positive = () => props.count() > 0;
  return (
    <View
      testID="acct-reset-cards"
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 4,
        paddingHorizontal: 7,
        paddingVertical: 3,
        borderRadius: 6,
        borderWidth: 1,
        borderColor: positive() ? paletteToken("ok_border") : paletteToken("border_soft"),
        backgroundColor: positive() ? paletteToken("ok_bg") : paletteToken("well"),
        flexShrink: 0,
      }}
    >
      <Icon
        name="rotateCcw"
        size={10}
        color={positive() ? paletteToken("ok") : paletteToken("fg_muted")}
      />
      <Text
        style={{
          fontSize: 11,
          color: positive() ? paletteToken("ok") : paletteToken("fg_muted"),
        }}
      >
        {t("accountsPage.resetCards")}
      </Text>
      <Text
        style={{
          fontSize: 11,
          fontWeight: "700",
          color: positive() ? paletteToken("ok") : paletteToken("fg_muted"),
          fontVariant: "tabular-nums",
        }}
      >
        {tf("accountsPage.resetCardsCount", { count: props.count() })}
      </Text>
    </View>
  );
}

/** 列表视图：基于 <List> 虚拟化，紧凑表格式单行展示多窗口进度条、最近使用与操作列。 */
export function AccountListView(props: AccountsViewProps): JSX.Element {
  const listRef = signal<ListRef | null>(null);
  const { selectedSet, refreshingSet } = useViewLookups(props);

  effect(() => {
    void props.agentId;
    listRef()?.scrollToOffset({ offset: 0, animated: false });
  });

  return (
    <View
      testID="acct-list-view"
      style={{
        display: "flex",
        flexDirection: "column",
        flex: 1,
        minHeight: 0,
        width: "100%",
        borderRadius: 14,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        overflow: "hidden",
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 12,
          height: 36,
          flexShrink: 0,
          paddingHorizontal: 14,
          backgroundColor: paletteToken("well"),
          borderBottomWidth: 1,
          borderBottomColor: paletteToken("border"),
        }}
      >
        <View style={{ width: 22, flexShrink: 0 }} />
        <Text
          style={{
            flex: 2.2,
            minWidth: 180,
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colAccount")}
        </Text>
        <Text
          style={{
            flex: 3,
            minWidth: 220,
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colQuota")}
        </Text>
        <Text
          style={{
            width: 116,
            flexShrink: 0,
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colLastUsed")}
        </Text>
        <Text
          style={{
            width: 100,
            flexShrink: 0,
            textAlign: "right",
            fontSize: 11,
            fontWeight: "600",
            color: paletteToken("fg_muted"),
          }}
        >
          {t("accountsPage.colActions")}
        </Text>
      </View>

      <List
        ref={(node) => listRef.set(node)}
        testID="acct-scroll"
        each={props.accounts}
        keyExtractor={(acct) => acct.id}
        estimatedItemSize={64}
        scrollEventThrottle={32}
        style={{ width: "100%", flex: 1, minHeight: 0 }}
      >
        {(acct) => (
          <AccountListRow
            account={acct}
            selectedSet={selectedSet}
            refreshingSet={refreshingSet}
            props={props}
          />
        )}
      </List>
    </View>
  );
}

function AccountListRow(slot: SlotContext): JSX.Element {
  const flags = useSlotFlags(slot);
  const { serving, selected, refreshing, deleting, busy } = flags;
  const quotaAvailable = computed(() => hasQuota(slot.account().usage));
  const windows = computed(() =>
    slot.account().usage ? windowOrder(slot.account().usage!.windows) : [],
  );
  const resetCards = computed(() => resetCardCount(slot.account().usage));
  const creditsOnly = computed(
    () =>
      Boolean(slot.account().usage) &&
      slot.account().usage!.windows.length === 0 &&
      (Boolean(slot.account().usage!.credits) || resetCards() !== null),
  );
  const emptyError = () => (slot.account().usage?.error ?? "").trim();

  return (
    <View
      testID={`acct-card-${slot.account().id}`}
      accessibilityState={{ busy: refreshing() || deleting() }}
      onContextMenu={(e) => openSlotContextMenu(e, slot, flags)}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 12,
        minHeight: 56,
        paddingHorizontal: 14,
        paddingVertical: 10,
        borderBottomWidth: 1,
        borderBottomColor: paletteToken("border_soft"),
        backgroundColor: selected() ? paletteToken("card_active") : "transparent",
      }}
      hoverStyle={{
        backgroundColor: selected()
          ? paletteToken("card_active_hover")
          : paletteToken("panel_hover"),
      }}
    >
      <SelectCheckbox
        testID={`acct-select-${slot.account().id}`}
        checked={selected()}
        label={accountName(slot.account()) || t("accountsPage.noUsername")}
        onPress={() => slot.props.onToggleSelect(slot.account().id)}
      />

      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 2,
          flex: 2.2,
          minWidth: 180,
        }}
      >
        <View
          style={{
            display: "flex",
            flexDirection: "row",
            alignItems: "center",
            gap: 6,
            minWidth: 0,
          }}
        >
          <Text
            testID={`acct-name-${slot.account().id}`}
            style={{
              flexShrink: 1,
              minWidth: 0,
              numberOfLines: 1,
              textOverflow: "ellipsis",
              fontSize: 13,
              fontWeight: "600",
              color: paletteToken("fg"),
            }}
          >
            {accountName(slot.account())
              ? maskAccountText(accountName(slot.account()), slot.props.masked)
              : t("accountsPage.noUsername")}
          </Text>
          <If when={serving()}>
            <ServingBadge id={slot.account().id} compact />
          </If>
          <PlanBadge id={slot.account().id} account={slot.account()} compact />
          <If when={Boolean(slot.account().usage?.error)}>
            <QuotaErrorBadge id={slot.account().id} error={slot.account().usage?.error ?? ""} />
          </If>
        </View>
        <LoginSubline account={slot.account()} masked={slot.props.masked} compact />
      </View>

      <View style={{ flex: 3, minWidth: 220 }}>
        <If
          when={slot.account().requiresReauth}
          else={
            <If
              when={quotaAvailable()}
              else={
                <Text
                  style={{
                    fontSize: 11,
                    color:
                      emptyError().length > 0
                        ? paletteToken("danger_fg")
                        : paletteToken("fg_faint"),
                    numberOfLines: 1,
                    textOverflow: "ellipsis",
                  }}
                >
                  {emptyError() || t("accountsPage.noQuota")}
                </Text>
              }
            >
              <If
                when={creditsOnly()}
                else={
                  <View
                    style={{
                      display: "flex",
                      flexDirection: "row",
                      alignItems: "center",
                      flexWrap: "wrap",
                      gap: 10,
                    }}
                  >
                    <Each of={windows()} key="index">
                      {(w) => <ListWindowBar window={w} />}
                    </Each>
                    <If when={resetCards() !== null}>
                      <CompactResetCardsChip count={() => resetCards()!} />
                    </If>
                  </View>
                }
              >
                <Text style={{ fontSize: 11, color: paletteToken("fg_muted") }}>
                  {extrasSummary(slot.account(), " · ")}
                </Text>
              </If>
            </If>
          }
        >
          <ReauthRow
            id={slot.account().id}
            busy={busy()}
            onReauth={() => slot.props.onReauth(slot.account().id)}
          />
        </If>
      </View>

      <View style={{ width: 116, flexShrink: 0 }}>
        <LastUsedMeta account={slot.account()} />
      </View>

      <View
        style={{
          display: "flex",
          flexDirection: "row",
          justifyContent: "flex-end",
          width: 100,
          flexShrink: 0,
        }}
      >
        <AccountActionButtons
          account={slot.account()}
          serving={serving()}
          busy={busy()}
          refreshing={refreshing()}
          deleting={deleting()}
          onRefresh={() => slot.props.onRefresh(slot.account().id)}
          onActivate={() => slot.props.onActivate(slot.account().id)}
          onDelete={() => slot.props.onDelete(slot.account().id)}
        />
      </View>
    </View>
  );
}

function ListWindowBar(props: { window: Accessor<UsageWindow> }): JSX.Element {
  const pct = () => clampPercent(props.window().remainingPercent);
  const color = () => toneColor(remainingTone(props.window().remainingPercent));
  const wide = () => windowHeadLength(props.window()) > MAX_PAIRED_HEAD_CHARS;

  return (
    <View
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 3,
        minWidth: wide() ? 176 : 88,
        flex: 1,
        maxWidth: wide() ? 260 : 140,
        overflow: "hidden",
      }}
    >
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 6,
          minWidth: 0,
        }}
      >
        <Text
          style={{
            flexShrink: 1,
            minWidth: 0,
            numberOfLines: 1,
            textOverflow: "ellipsis",
            fontSize: 11,
            fontWeight: "500",
            color: paletteToken("fg_muted"),
          }}
        >
          {windowLabel(props.window())}
        </Text>
        <Text
          testID={`acct-pct-${props.window().key}`}
          style={{
            flexShrink: 0,
            fontSize: 11,
            fontWeight: "700",
            color: color(),
            fontVariant: "tabular-nums",
          }}
        >
          {percentText(props.window().remainingPercent)}
        </Text>
      </View>
      <View
        style={{
          width: "100%",
          height: 4,
          borderRadius: 999,
          backgroundColor: paletteToken("well"),
          overflow: "hidden",
        }}
      >
        <View
          style={{
            width: `${pct()}%`,
            height: "100%",
            borderRadius: 999,
            backgroundColor: color(),
          }}
        />
      </View>
    </View>
  );
}

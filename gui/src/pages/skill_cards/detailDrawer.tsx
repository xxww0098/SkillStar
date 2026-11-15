import { List, Text, View, effect, signal, type JSX, type ListRef } from "nsh";
import { Menu, MenuItem, MenuSeparator, Switch } from "nsh/base";
import { paletteToken } from "../../services/themeMode";
import { t, tf } from "../../services/translation";
import { Icon } from "../../chrome/icons";
import { IconSpin } from "../../chrome/feedback";
import { Sheet } from "../../chrome/sheet";
import { Tip } from "../../chrome/tooltip";
import { AgentBadge } from "./agentRail";
import { openConfirm } from "../../chrome/dialog";
import { publishToast } from "../../shell/events";
import { motionTransition } from "../../chrome/motion";
import {
  agentLinkNotice,
  deckGlyph,
  drawerOps,
  isAgentLinked,
  missingCount,
  notifyDeckListChanged,
  type DeckDrawerOps,
  type DrawerAgent,
  type DrawerDeck,
  type LinkReport,
} from "./detailDrawerActions";

// Deck detail column — `skill_cards/detail_drawer.rs` parity.
//
// Clicking a deck card opens this track beside the grid: the full member
// list with install state (the card only previews four chips), the deploy-all
// batch, one switch per targetable agent, and the edit/share/more footer.
// The track keeps the width the grid reserves (Sheet = DETAIL_COLUMN_W) and
// paints the same floating surface the skill and market columns use, in one
// repaint — no slide-in.
//
// Members ride a virtualized `List` (the GPUI ListState): only rows near the
// viewport exist, and the agent section scrolls as the list's footer. A deck
// change scrolls back to the top — the column's switch-decks-returns-to-top
// rule (an effect below, the reset the GPUI ListState key performed).
//
// The drawer is props-driven so the deck page owns the data (the
// `SkillCards.snapshot` records); every command goes through `ops` (the
// `SkillCardsDrawer` facade by default) and finishes by publishing
// GroupsChanged — the page reloads without either importing the other.
//
// nsh components run once (Solid semantics): every dynamic value is read at
// its use site (`props.deck` in JSX expressions and handlers), never
// destructured up front.

export interface DeckDetailDrawerProps {
  /** The selected deck, while it still exists (the page clears it on delete). */
  deck: DrawerDeck;
  /** Installed skill names (`installer::installed_names`). */
  installedSkills: ReadonlySet<string>;
  /** Targetable agents (`targetable_agent_profiles`: global dir AND enabled). */
  agents: DrawerAgent[];
  /** Page-level busy (an import running): stacks with the drawer's own jobs. */
  busy?: boolean;
  /** The column's close control (clears the page's selection). Omitted
   * when the host cannot clear the selection yet (the renderDrawer seam
   * has no close channel today) — closing then rides the card press. */
  onClose?(): void;
  /** 「编辑卡组…」 opens the deck editor (create_group.rs slice). */
  onEdit(deck: DrawerDeck): void;
  /** 「分享卡组…」 opens the share sheet (share_sheet.rs slice). */
  onShare(deck: DrawerDeck): void;
  /** Optional extra refresh hook after a job settles (besides GroupsChanged). */
  onMutated?(): void;
  /** Domain commands; defaults to the SkillCardsDrawer facade binding. */
  ops?: DeckDrawerOps;
}

export function DeckDetailDrawer(props: DeckDetailDrawerProps): JSX.Element {
  // One job at a time, exactly the page's `busy` gate: repeated rail clicks
  // while a job runs are swallowed, and every control reads the same flag.
  const jobBusy = signal(false);

  const finish = () => {
    jobBusy.set(false);
    notifyDeckListChanged();
    props.onMutated?.();
  };

  // run_link_job tail: the report's partial failures become one notice-band
  // error; a domain failure rejects and surfaces the same way (the GPUI page
  // showed both in its toolbar notice slot; the nsh contract routes every
  // operation result through the band).
  const runLinkJob = (job: () => Promise<LinkReport>) => {
    if (jobBusy() || (props.busy ?? false)) return;
    jobBusy.set(true);
    job()
      .then((report) => {
        const notice = agentLinkNotice(report);
        if (notice !== null) publishToast({ tone: "error", message: notice });
      })
      .catch((err: unknown) => {
        publishToast({ tone: "error", message: errText(err) });
      })
      .finally(finish);
  };

  // run_group_job tail (duplicate/delete): only errors speak.
  const runGroupJob = (job: () => Promise<unknown>) => {
    if (jobBusy() || (props.busy ?? false)) return;
    jobBusy.set(true);
    job()
      .catch((err: unknown) => {
        publishToast({ tone: "error", message: errText(err) });
      })
      .finally(finish);
  };

  // chrome::open_confirm parity: the title names the deck, the hint says Hub
  // skills stay, the commit button is the danger-filled 「删除」. Read at
  // trigger time so it always names the open deck.
  const confirmDelete = () => {
    openConfirm({
      title: tf("skillCards.deleteDeckTitle", { name: props.deck.name }),
      body: t("skillCards.deleteDeckHint"),
      okText: t("common.delete"),
      danger: true,
      onOk: () => {
        runGroupJob(() => (props.ops ?? drawerOps).deleteDeck(props.deck.id));
      },
    });
  };

  // The full member list is virtualized; the label scrolls with it and the
  // agent section rides as the list's footer (the GPUI column puts it at the
  // end of the same viewport).
  let listRef: ListRef | null = null;
  effect(() => {
    // Switch decks (or a member-count change, the remeasure trigger the GPUI
    // key rode on) → jump back to the top in one repaint, never animated.
    void props.deck.id;
    void props.deck.skills.length;
    listRef?.scrollToOffset({ offset: 0, animated: false });
  });

  return (
    <Sheet>
      {/* Header: the card's own glyph tile and title, then the description
          and the count badges; the close control clears the selection. */}
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "flex-start",
          justifyContent: "space-between",
          gap: 8,
          flexShrink: 0,
          paddingHorizontal: 16,
          paddingTop: 16,
          paddingBottom: 12,
          borderBottomWidth: 1,
          borderBottomColor: paletteToken("border"),
        }}
      >
        <View style={{ display: "flex", flexDirection: "column", gap: 8, flex: 1, minWidth: 0 }}>
          <View
            style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8, minWidth: 0 }}
          >
            <View
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                width: 32,
                height: 32,
                flexShrink: 0,
                borderRadius: 8,
                backgroundColor: paletteToken("well"),
                borderWidth: 1,
                borderColor: paletteToken("edge"),
              }}
            >
              <Text style={{ fontSize: 18, color: paletteToken("fg") }}>{deckGlyph(props.deck)}</Text>
            </View>
            <View style={{ minWidth: 0 }}>
              <Text style={{ fontSize: 14, color: paletteToken("fg") }}>{props.deck.name}</Text>
            </View>
          </View>
          <View style={{ minWidth: 0 }}>
            <Text
              style={{
                fontSize: 12,
                color: paletteToken("fg_muted"),
                fontStyle: props.deck.description.trim().length === 0 ? "italic" : "normal",
                opacity: props.deck.description.trim().length === 0 ? 0.6 : 1,
              }}
            >
              {props.deck.description.trim().length > 0
                ? props.deck.description.trim()
                : t("skillCards.noDescription")}
            </Text>
          </View>
          <View style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 4 }}>
            <Badge
              bg={paletteToken("card_hover")}
              fg={paletteToken("fg_muted")}
              label={tf("skillCards.skillsCount", { count: props.deck.skills.length })}
            />
            {missingCount(props.deck, props.installedSkills) > 0 ? (
              <Badge
                bg={paletteToken("warn_bg")}
                fg={paletteToken("warn")}
                label={tf("skillCards.missingCount", {
                  count: missingCount(props.deck, props.installedSkills),
                })}
              />
            ) : null}
          </View>
        </View>
        {props.onClose ? (
          <GhostIconButton
            glyph="x"
            testID="close-deck-detail-drawer"
            label={t("detailPanel.dismissDrawer")}
            tip={t("detailPanel.dismissDrawer")}
            disabled={jobBusy() || (props.busy ?? false)}
            onPress={props.onClose}
          />
        ) : null}
      </View>

      {/* Body: the scrolling member list. */}
      <View
        style={{
          flex: 1,
          minHeight: 0,
          overflow: "hidden",
          paddingHorizontal: 16,
          paddingVertical: 12,
          display: "flex",
        }}
      >
        <List
          ref={(node) => {
            listRef = node;
          }}
          testID="deck-detail-scroll"
          each={props.deck.skills}
          keyExtractor={(skill) => skill}
          header={
            <View style={{ paddingBottom: 4 }}>
              {fieldLabel(t("skillCards.deckSkills"))}
            </View>
          }
          empty={
            <View style={{ paddingHorizontal: 8, paddingVertical: 6 }}>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {t("skillCards.deckEmpty")}
              </Text>
            </View>
          }
          footer={
            <View style={{ paddingTop: 16 }}>
              {/* The slot expression is the reactive context: read BOTH busy
                  sources here (the drawer's job and the page's busy) so a
                  change in either recomputes the whole agent section. */}
              {agentSection(props, jobBusy() || (props.busy ?? false), runLinkJob)}
            </View>
          }
          style={{ width: "100%", height: "100%" }}
        >
          {(skill) => memberRow(skill(), props.installedSkills)}
        </List>
      </View>

      {/* Actions: edit, share, and the same more menu the card carries. */}
      <View
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          flexShrink: 0,
          paddingHorizontal: 16,
          paddingVertical: 12,
          borderTopWidth: 1,
          borderTopColor: paletteToken("border"),
        }}
      >
        <DrawerButton
          testID="deck-drawer-edit"
          glyph="pencil"
          label={t("skillCards.editDeckAction")}
          full
          disabled={jobBusy() || (props.busy ?? false)}
          onPress={() => props.onEdit(props.deck)}
        />
        <DrawerButton
          testID="deck-drawer-share"
          glyph="share"
          label={t("skillCards.shareDeckAction")}
          full
          disabled={jobBusy() || (props.busy ?? false)}
          onPress={() => props.onShare(props.deck)}
        />
        <Menu
          placement="bottom"
          align="end"
          popupStyle={{
            minWidth: 168,
            padding: 4,
            borderRadius: 8,
            backgroundColor: paletteToken("card"),
            borderWidth: 1,
            borderColor: paletteToken("border"),
            boxShadow: "0px 8px 16px -6px rgba(0,0,0,0.35)",
          }}
          trigger={
            <GhostIconButton
              glyph="ellipsis"
              testID="deck-drawer-more"
              label={t("common.moreActions")}
              tip={t("common.moreActions")}
              disabled={jobBusy() || (props.busy ?? false)}
              // A pressable trigger: the Menu owns the press to open; a
              // view without a press handler is not pressable for it.
              onPress={() => {}}
            />
          }
        >
          <MenuItem
            onSelect={() =>
              runGroupJob(() => (props.ops ?? drawerOps).duplicateDeck(props.deck.id))
            }
            style={{
              display: "flex",
              alignItems: "center",
              paddingHorizontal: 8,
              paddingVertical: 6,
              borderRadius: 6,
            }}
            hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
          >
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
              <Icon name="copy" size={14} color={paletteToken("fg_muted")} />
              <Text style={{ fontSize: 12, color: paletteToken("fg") }}>
                {t("skillCards.duplicateDeck")}
              </Text>
            </View>
          </MenuItem>
          <MenuSeparator
            style={{ height: 1, marginVertical: 4, backgroundColor: paletteToken("border") }}
          />
          <MenuItem
            onSelect={confirmDelete}
            style={{
              display: "flex",
              alignItems: "center",
              paddingHorizontal: 8,
              paddingVertical: 6,
              borderRadius: 6,
            }}
            hoverStyle={{ backgroundColor: paletteToken("danger_bg") }}
          >
            <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 8 }}>
              <Icon name="trash" size={14} color={paletteToken("danger")} />
              <Text style={{ fontSize: 12, color: paletteToken("danger") }}>
                {t("skillCards.deleteDeck")}
              </Text>
            </View>
          </MenuItem>
        </Menu>
      </View>
    </Sheet>
  );
}

function errText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** field_label parity: the quiet xs section label. */
function fieldLabel(label: string): JSX.Element {
  return <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>{label}</Text>;
}

/** chrome::badge parity: a tiny semibold count chip. */
function Badge(props: { bg: string; fg: string; label: string }): JSX.Element {
  return (
    <View
      style={{
        paddingHorizontal: 6,
        paddingVertical: 1,
        borderRadius: 4,
        backgroundColor: props.bg,
      }}
    >
      <Text style={{ fontSize: 12, fontWeight: "600", color: props.fg }}>{props.label}</Text>
    </View>
  );
}

/** One member: the install-state glyph, then the name. Missing members dim
 * (the rows the header's warning badge counts). */
function memberRow(skill: string, installed: ReadonlySet<string>): JSX.Element {
  const isInstalled = installed.has(skill);
  return (
    <View
      testID={`deck-member-${skill}`}
      style={{
        height: 36,
        display: "flex",
        alignItems: "center",
        gap: 8,
        paddingHorizontal: 8,
        paddingVertical: 8,
        minWidth: 0,
      }}
    >
      {isInstalled ? (
        <Icon name="checkCircle" size={13} color={paletteToken("ok")} />
      ) : (
        <Icon name="alertTriangle" size={13} color={paletteToken("warn")} />
      )}
      <View style={{ minWidth: 0, overflow: "hidden" }}>
        <Text style={{ fontSize: 12, color: paletteToken(isInstalled ? "fg" : "fg_muted") }}>
          {skill}
        </Text>
      </View>
    </View>
  );
}

// The agent box: one switch per targetable agent, with deploy-all on top.
// The card footer keeps the same rail as icon slots; this is the named,
// full-width version of it, plus the batch action the rail cannot express.
// Called from a tracked footer expression, so every read here follows the
// page's data.
function agentSection(
  props: DeckDetailDrawerProps,
  busy: boolean,
  runLinkJob: (job: () => Promise<LinkReport>) => void
): JSX.Element {
  const ops = props.ops ?? drawerOps;
  const deck = props.deck;
  const links = deck.agentLinks ?? [];
  const blockRail = busy || deck.skills.length === 0;
  return (
    <View style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {fieldLabel(t("skillCards.agentLinks"))}
      <View
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 6,
          padding: 8,
          borderRadius: 8,
          backgroundColor: paletteToken("card"),
          borderWidth: 1,
          borderColor: paletteToken("border"),
        }}
      >
        {props.agents.length === 0 ? (
          <View style={{ paddingHorizontal: 8, paddingVertical: 6 }}>
            <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
              {t("selectionBar.noAgents")}
            </Text>
          </View>
        ) : (
          <>
            <DrawerButton
              testID="deck-drawer-link-all"
              glyph="link"
              label={t("skillCards.linkAll")}
              full
              loading={busy}
              disabled={blockRail}
              onPress={() => runLinkJob(() => ops.linkDeckToEnabledAgents(deck.id))}
            />
            <View>
              <Text style={{ fontSize: 12, color: paletteToken("fg_muted") }}>
                {t("skillCards.linkAllHint")}
              </Text>
            </View>
          </>
        )}
        {props.agents.map((agent) => {
          const linked = isAgentLinked(agent, links);
          return (
            <View
              testID={`deck-drawer-agent-${deck.id}-${agent.id}`}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                paddingHorizontal: 8,
                paddingVertical: 6,
                borderRadius: 6,
              }}
              hoverStyle={{ backgroundColor: paletteToken("card_hover") }}
            >
              <View style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0 }}>
                <AgentBadge
                  id={agent.id}
                  name={agent.displayName}
                  icon={agent.icon}
                  active={linked}
                  size={18}
                />
                <View style={{ minWidth: 0, overflow: "hidden" }}>
                  <Text style={{ fontSize: 12, fontWeight: "500", color: paletteToken("fg") }}>
                    {agent.displayName}
                  </Text>
                </View>
              </View>
              <Switch
                testID={`deck-drawer-toggle-${deck.id}-${agent.id}`}
                checked={linked}
                disabled={blockRail}
                accessibilityLabel={tf(
                  linked ? "skillCards.unlinkAllFrom" : "skillCards.linkAllTo",
                  { agent: agent.displayName }
                )}
                trackStyle={{
                  width: 28,
                  height: 16,
                  borderRadius: 8,
                  borderWidth: 1,
                  backgroundColor: linked ? paletteToken("ok") : paletteToken("input"),
                  borderColor: linked ? paletteToken("ok_border") : paletteToken("edge"),
                }}
                thumbStyle={{
                  width: 12,
                  height: 12,
                  borderRadius: 6,
                  backgroundColor: paletteToken("on_accent"),
                  transform: linked ? "translateX(12px)" : "translateX(0px)",
                  ...motionTransition(),
                }}
                onChange={() => {
                  // The rail toggle: enable is the opposite of the current
                  // state (toggle_agent_for_deck parity — the deck's
                  // recorded links decide).
                  runLinkJob(() => ops.setDeckAgent(deck.id, agent.id, !linked));
                }}
              />
            </View>
          );
        })}
      </View>
    </View>
  );
}

/** The column's quiet command button: hairline outline, glyph + label,
 * full-width when `full` (the footer's even split). `loading` spins the
 * glyph and swallows presses (Button::loading parity). */
function DrawerButton(props: {
  testID: string;
  glyph: "link" | "pencil" | "share";
  label: string;
  full?: boolean;
  loading?: boolean;
  disabled?: boolean;
  onPress(): void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      disabled={(props.disabled ?? false) || (props.loading ?? false)}
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "center",
        gap: 6,
        height: 32,
        minWidth: props.full ? 0 : undefined,
        width: props.full ? "100%" : undefined,
        flex: props.full ? 1 : undefined,
        paddingHorizontal: 10,
        borderRadius: 6,
        borderWidth: 1,
        borderColor:
          (props.disabled ?? false) || (props.loading ?? false)
            ? paletteToken("border_soft")
            : paletteToken("border"),
        opacity: (props.disabled ?? false) || (props.loading ?? false) ? 0.55 : 1,
      }}
      hoverStyle={
        (props.disabled ?? false) || (props.loading ?? false)
          ? undefined
          : { backgroundColor: paletteToken("card_hover") }
      }
      activeStyle={
        (props.disabled ?? false) || (props.loading ?? false)
          ? undefined
          : { backgroundColor: paletteToken("card_active") }
      }
      // `disabled` above already swallows presses; keep the handler bound.
      onPress={props.onPress}
    >
      {props.loading ?? false ? (
        <IconSpin name="refresh" size={14} color={paletteToken("accent")} spin />
      ) : (
        <Icon name={props.glyph} size={14} color={paletteToken("fg_muted")} />
      )}
      <Text style={{ fontSize: 12, color: paletteToken("fg") }}>{props.label}</Text>
    </View>
  );
}

/** 32px ghost icon command (the header close, the footer's more). The tip
 * rides the button itself — a wrapping tooltip view without a press handler
 * would swallow the press. When it only triggers a menu, `onPress` is a
 * noop so the view stays pressable and the Menu owns the press. */
function GhostIconButton(props: {
  glyph: "x" | "ellipsis";
  testID: string;
  label: string;
  tip?: string;
  disabled?: boolean;
  onPress?(): void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      accessibilityLabel={props.label}
      disabled={props.disabled ?? false}
      tooltip={props.tip ? <Tip text={props.tip} /> : undefined}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: 32,
        height: 32,
        flexShrink: 0,
        borderRadius: 6,
        opacity: props.disabled ?? false ? 0.55 : 1,
      }}
      hoverStyle={
        props.disabled ?? false ? undefined : { backgroundColor: paletteToken("panel_hover") }
      }
      // `disabled` above already swallows presses; keep the handler bound.
      onPress={props.onPress ?? (() => {})}
    >
      <Icon name={props.glyph} size={16} color={paletteToken("fg_muted")} />
    </View>
  );
}


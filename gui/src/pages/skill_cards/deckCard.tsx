import { Text, View, type JSX } from "nsh";
import { Menu, MenuItem, MenuSeparator } from "nsh/base";
import { paletteToken } from "../../services/themeMode";
import { Icon } from "../../chrome/icons";
import { Tip } from "../../chrome/tooltip";
import { skillCardFrameStyle, skillCardHoverStyle } from "../../chrome/cardShell";
import { openConfirm } from "../../chrome/dialog";
import { SKILL_CARD_W } from "../../layout";
import { dt, dtf } from "./deckStrings";
import type { AgentRailProfile, DeckRecord } from "./deckQuery";
import { AgentFooterBar, AgentRail, DeckCountBadge, type AgentRailSlot } from "./agentRail";
import { deckGlyph } from "./detailDrawerActions";

// One card per skill group — group_card.rs parity. The deck card shares the
// skill card's skeleton (card_shell's Deck face: rounded-xl, no shadow,
// fixed height), and like the skill card it is a SUMMARY: member skill
// names stay collapsed to one short chips row (4 + "+N more"); the whole
// member list lives in the detail column the card opens on click.
//
// Hover/selected follow the shared market-blue contract (card_face_paint,
// skill_card/shell.rs): hover swaps to the accent border + card_hover fill;
// selected (the drawer is open on this deck) pins the accent border over
// card_active, and hovering then only deepens to card_active_hover. Both
// are zero-frame state styles — no transitions on colors.
//
// nsh reactivity: the component runs once; derived values (glyph, chips,
// missing count, rail slots) ride helper functions called from JSX
// attribute positions, so they recompute when the props they read change.

export interface DeckCardHandle {
  /** The deck-page commands the card triggers (owned by index.tsx). */
  onOpen: (deck: DeckRecord) => void;
  onShare: (deck: DeckRecord) => void;
  onDuplicate: (deck: DeckRecord) => void;
  onDelete: (deck: DeckRecord) => void;
  onToggleAgent: (deck: DeckRecord, agentId: string) => void;
}

export interface DeckCardProps {
  deck: DeckRecord;
  selected: boolean;
  viewList: boolean;
  cardWidth?: number;
  busy: boolean;
  installedSkills: ReadonlySet<string>;
  agents: AgentRailProfile[];
  handle: DeckCardHandle;
}

function missingCount(deck: DeckRecord, installed: ReadonlySet<string>): number {
  return deck.skills.reduce(
    (missing, skill) => (installed.has(skill) ? missing : missing + 1),
    0
  );
}

function railSlots(deck: DeckRecord, agents: AgentRailProfile[], busy: boolean): AgentRailSlot[] {
  const links = new Set(deck.agentLinks ?? []);
  return agents.map((profile) => ({
    profile,
    linked: links.has(profile.id),
    pending: busy,
    label: dtf(links.has(profile.id) ? "skillCards.unlinkAllFrom" : "skillCards.linkAllTo", {
      agent: profile.displayName,
    }),
  }));
}

export function DeckCard(props: DeckCardProps): JSX.Element {
  return (
    <View
      testID={`deck-card-${props.deck.id}`}
      style={{
        ...skillCardFrameStyle(props.selected),
        width: props.viewList ? "100%" : (props.cardWidth ?? SKILL_CARD_W),
        flexShrink: 0,
        alignSelf: "flex-start",
      }}
      hoverStyle={skillCardHoverStyle(props.selected)}
      onPress={() => props.handle.onOpen(props.deck)}
    >
      {/* Header: glyph, name/description, then the deck's one number riding
          with the header actions — beside share, in the slot the skill card
          gives stars (group_card.rs:97-162). */}
      <View
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          paddingHorizontal: 14,
          paddingTop: 12,
          flexShrink: 0,
        }}
      >
        <View
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            width: 32,
            height: 32,
            flexShrink: 0,
            borderRadius: 9,
            backgroundColor: props.selected ? paletteToken("accent_soft") : paletteToken("well"),
            borderWidth: 1,
            borderColor: props.selected ? paletteToken("accent") : paletteToken("border"),
          }}
        >
          <Text style={{ fontSize: 16, lineHeight: 20, color: paletteToken("fg") }}>
            {deckGlyph(props.deck)}
          </Text>
        </View>
        <View style={{ display: "flex", flexDirection: "column", gap: 2, flex: 1, minWidth: 0 }}>
          <Text
            style={{
              fontSize: 14,
              lineHeight: 18,
              fontWeight: "600",
              color: paletteToken("fg"),
              numberOfLines: 1,
            }}
          >
            {props.deck.name}
          </Text>
          <If
            when={props.deck.description.trim().length > 0}
            else={
              <Text
                style={{
                  fontSize: 11,
                  lineHeight: 15,
                  fontStyle: "italic",
                  color: paletteToken("fg_muted"),
                  opacity: 0.6,
                  numberOfLines: 1,
                }}
              >
                {dt("skillCards.noDescription")}
              </Text>
            }
          >
            <Text
              style={{
                fontSize: 11,
                lineHeight: 15,
                color: paletteToken("fg_muted"),
                numberOfLines: 1,
              }}
            >
              {props.deck.description.trim()}
            </Text>
          </If>
        </View>
        <View style={{ display: "flex", flexDirection: "row", alignItems: "center", gap: 4, flexShrink: 0 }}>
          <DeckCountBadge count={props.deck.skills.length} />
          <ShareButton deckId={props.deck.id} onPress={() => props.handle.onShare(props.deck)} />
          <DeckActionsMenu
            deck={props.deck}
            busy={props.busy}
            onDuplicate={() => props.handle.onDuplicate(props.deck)}
            onDelete={() => props.handle.onDelete(props.deck)}
          />
        </View>
      </View>

      {/* Body: the missing-skills warning, then the collapsed chips row.
          The card is a summary, so the chips never expand in place
          (group_card.rs:182-229). */}
      <View
        style={{
          flex: 1,
          minHeight: 0,
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          justifyContent: "flex-start",
          gap: 4,
          paddingHorizontal: 14,
          paddingTop: 6,
          paddingBottom: 4,
        }}
      >
        <If when={missingCount(props.deck, props.installedSkills) > 0}>
            <View
              style={{
                display: "flex",
                flexDirection: "row",
                alignItems: "center",
                gap: 4,
                paddingHorizontal: 4,
              }}
            >
              <Icon name="alertTriangle" size={12} color={paletteToken("warn")} />
              <Text style={{ fontSize: 12, lineHeight: 16, color: paletteToken("warn") }}>
                {dtf("skillCards.missingCount", {
                  count: missingCount(props.deck, props.installedSkills),
                })}
              </Text>
            </View>
          </If>
          <View
            testID={`deck-chips-${props.deck.id}`}
            style={{
              display: "flex",
              flexDirection: "row",
              flexWrap: "wrap",
              gap: 4,
              paddingVertical: 2,
            }}
          >
            <Each of={props.deck.skills.slice(0, 4)} key="index">
              {(skill: () => string) => (
                <View
                  style={{
                    paddingHorizontal: 8,
                    paddingVertical: 2,
                    borderRadius: 6,
                    borderWidth: 1,
                    borderColor: props.installedSkills.has(skill())
                      ? paletteToken("border")
                      : paletteToken("warn_border"),
                    backgroundColor: props.installedSkills.has(skill())
                      ? paletteToken("card_hover")
                      : paletteToken("warn_bg"),
                  }}
                >
                  <Text
                    style={{
                      fontSize: 12,
                      color: props.installedSkills.has(skill())
                        ? paletteToken("fg")
                        : paletteToken("warn"),
                    }}
                  >
                    {skill()}
                  </Text>
                </View>
              )}
            </Each>
            <If when={props.deck.skills.length > 4}>
              <Text
                style={{
                  fontSize: 12,
                  paddingHorizontal: 4,
                  paddingVertical: 2,
                  color: paletteToken("fg_muted"),
                }}
              >
                {dtf("common.moreCount", { count: props.deck.skills.length - 4 })}
              </Text>
            </If>
          </View>
      </View>

      {/* Footer: the same carousel the skill card rides — one slot per
          targetable agent, linked slots highlighted, a click toggles the
          deck's link (批量链接语义). */}
      <AgentFooterBar>
        <AgentRail
          scopeKey={props.deck.id}
          slots={railSlots(props.deck, props.agents, props.busy)}
          pending={props.busy}
          onSlotClick={(slot) => props.handle.onToggleAgent(props.deck, slot.profile.id)}
        />
      </AgentFooterBar>
    </View>
  );
}

/** Ghost icon command (Button ghost small parity): 28px, quiet, tooltip. */
function GhostIconButton(props: {
  testID: string;
  glyph: "share" | "ellipsis";
  label: string;
  disabled?: boolean;
  onPress: () => void;
}): JSX.Element {
  return (
    <View
      testID={props.testID}
      accessibilityLabel={props.label}
      tooltip={<Tip text={props.label} />}
      disabled={props.disabled}
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: 28,
        height: 28,
        borderRadius: 6,
        cursor: props.disabled ? "default" : "pointer",
        opacity: props.disabled ? 0.55 : 1,
      }}
      hoverStyle={props.disabled ? undefined : { backgroundColor: paletteToken("panel_hover") }}
      onPress={props.disabled ? undefined : props.onPress}
    >
      <Icon name={props.glyph} size={14} color={paletteToken("fg_muted")} />
    </View>
  );
}

function ShareButton(props: { deckId: string; onPress: () => void }): JSX.Element {
  return (
    <GhostIconButton
      testID={`deck-share-${props.deckId}`}
      glyph="share"
      label={dt("skillCards.shareDeck")}
      onPress={props.onPress}
    />
  );
}

/** 「复制卡组 / 删除卡组…」 — deck_actions_menu parity: the trigger anchors
 * a top-right menu; delete opens the shared name-confirm dialog (destructive)
 * with the Hub-skills-stay note before the job runs. */
function DeckActionsMenu(props: {
  deck: DeckRecord;
  busy: boolean;
  onDuplicate: () => void;
  onDelete: () => void;
}): JSX.Element {
  return (
    <Menu
      placement="top"
      align="end"
      offset={4}
      popupStyle={{
        minWidth: 180,
        paddingVertical: 4,
        borderRadius: 8,
        borderWidth: 1,
        borderColor: paletteToken("border"),
        backgroundColor: paletteToken("card"),
        boxShadow: "0px 8px 16px -6px rgba(0,0,0,0.25)",
      }}
      trigger={
        <GhostIconButton
          testID={`deck-more-${props.deck.id}`}
          glyph="ellipsis"
          label={dt("common.moreActions")}
          disabled={props.busy}
          onPress={() => {}}
        />
      }
    >
      <MenuItem onSelect={props.onDuplicate}>
        <MenuRow glyph="copy" label={dt("skillCards.duplicateDeck")} tone="fg" />
      </MenuItem>
      <MenuSeparator
        style={{ height: 1, marginVertical: 4, backgroundColor: paletteToken("border") }}
      />
      <MenuItem
        onSelect={() => {
          // The destructive command confirms the deck by name and says the
          // Hub skills survive (group_card.rs:345-368).
          openConfirm({
            title: dtf("skillCards.deleteDeckTitle", { name: props.deck.name }),
            body: dt("skillCards.deleteDeckHint"),
            okText: dt("common.delete"),
            danger: true,
            onOk: props.onDelete,
          });
        }}
      >
        <MenuRow glyph="trash" label={dt("skillCards.deleteDeck")} tone="danger" />
      </MenuItem>
    </Menu>
  );
}

function MenuRow(props: {
  glyph: "copy" | "trash";
  label: string;
  tone: "fg" | "danger";
}): JSX.Element {
  const color = props.tone === "danger" ? paletteToken("danger") : paletteToken("fg");
  return (
    <View
      style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 8,
        paddingHorizontal: 12,
        paddingVertical: 6,
      }}
    >
      <Icon name={props.glyph} size={14} color={color} />
      <Text style={{ fontSize: 12, color }}>{props.label}</Text>
    </View>
  );
}

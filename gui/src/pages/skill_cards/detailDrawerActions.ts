import { SkillCardsDrawer } from "skillstar-gui/native";
import { groupsChanged } from "../../shell/events";
import { tf } from "../../services/translation";

// Deck detail drawer — pure logic half of `skill_cards/detail_drawer.rs`.
//
// The GPUI column reads its deck from the page state and drives the domain
// through `run_link_job` / `run_group_job` (skill_cards/mod.rs:135-222);
// here the same seam is `DeckDrawerOps`, with the default binding talking to
// the `SkillCardsDrawer` facade. Keeping the notice assembler, the linked
// predicate and the member math out of the component makes them testable
// without a window, the way noticeBoard.ts carries the band's rules.

/** One deck as the drawer reads it — the `SkillCards.snapshot` deck record
 * (the fields detail_drawer.rs uses; `agentLinks` null and [] both read
 * unlinked, the facade normalizes to []). */
import type { AgentLinkState } from "./deckQuery";

export interface DrawerDeck {
  id: string;
  name: string;
  description: string;
  icon: string;
  skills: string[];
  agentLinks: string[] | null;
  memberAgentLinks?: Record<string, string[]>;
  agentLinkStates?: Record<string, AgentLinkState>;
}

/** One targetable agent — `SkillCards.snapshot`'s `{id, displayName}`
 * (`targetable_agent_profiles`: a global skills dir AND enabled). */
export interface DrawerAgent {
  id: string;
  displayName: string;
  icon?: string;
}

/** One skipped or failed pair, with its reason (facade `link_issue_json`). */
export interface LinkIssue {
  agentId: string;
  skill: string;
  reason: string;
}

/** Link-report mirror the facade returns (`report_to_json`):
 * `applied` as `[agent, skill]` pairs, `skipped`/`failed` as issues. */
export interface LinkReport {
  applied: [string, string][];
  skipped: LinkIssue[];
  failed: LinkIssue[];
}

/** The domain commands the drawer issues (the facade's async exports). */
export interface DeckDrawerOps {
  setDeckAgent(deckId: string, agentId: string, enable: boolean): Promise<LinkReport>;
  linkDeckToEnabledAgents(deckId: string): Promise<LinkReport>;
  duplicateDeck(deckId: string): Promise<unknown>;
  deleteDeck(deckId: string): Promise<void>;
}

function isRegionalIndicator(cp: number): boolean {
  return cp >= 0x1f1e6 && cp <= 0x1f1ff;
}

function isGraphemeExtend(cp: number): boolean {
  return (
    (cp >= 0xfe00 && cp <= 0xfe0f) ||
    (cp >= 0xe0100 && cp <= 0xe01ef) ||
    (cp >= 0x1f3fb && cp <= 0x1f3ff) ||
    (cp >= 0x0300 && cp <= 0x036f) ||
    (cp >= 0x20d0 && cp <= 0x20ff) ||
    cp === 0x20e3
  );
}

/** Extract the first user-perceived grapheme cluster without splitting
 * UTF-16 surrogates, Regional Indicator pairs, VS-16/skin-tone modifiers, or ZWJ tails. */
function firstGrapheme(input: string): string | undefined {
  const cps = Array.from(input);
  if (cps.length === 0) return undefined;
  let end = 1;
  const firstCp = cps[0].codePointAt(0) ?? 0;
  if (end < cps.length && isRegionalIndicator(firstCp) && isRegionalIndicator(cps[1].codePointAt(0) ?? 0)) {
    end = 2;
  }
  while (end < cps.length) {
    const cp = cps[end].codePointAt(0) ?? 0;
    if (isGraphemeExtend(cp)) {
      end += 1;
      continue;
    }
    if (cp === 0x200d && end + 1 < cps.length) {
      end += 2;
      continue;
    }
    break;
  }
  return cps.slice(0, end).join("");
}

/** The glyph tile shows the deck's icon's first grapheme (detail_drawer.rs
 * `deck_glyph`): icon, else name, else the box emoji, stripping any broken U+FFFD. */
export function deckGlyph(deck: DrawerDeck): string {
  const cleanIcon = deck.icon.replace(/\uFFFD+/g, "").trim();
  const cleanName = deck.name.replace(/\uFFFD+/g, "").trim();
  const source = cleanIcon || cleanName;
  return firstGrapheme(source) ?? "\u{1f4e6}";
}

/** The rail predicate (chrome/detail.rs `profile_is_linked`): a link entry
 * matches the agent by id OR by display name — older decks recorded names. */
export function isAgentLinked(agent: DrawerAgent, links: readonly string[]): boolean {
  return links.some((link) => link === agent.id || link === agent.displayName);
}

/** Members not present in the installed set (exact names, the way the GPUI
 * page's HashSet::contains compares). */
export function missingCount(deck: DrawerDeck, installed: ReadonlySet<string>): number {
  return deck.skills.reduce((n, skill) => (installed.has(skill) ? n : n + 1), 0);
}

/** Localized one-paragraph summary of failed and skipped pairs —
 * notify.rs `agent_link_notice` parity: nothing to say when everything
 * applied (the success path stays silent); failures lead with the first
 * pair and its reason, skips follow as their own line. */
export function agentLinkNotice(report: LinkReport): string | null {
  const total = report.applied.length + report.skipped.length + report.failed.length;
  const parts: string[] = [];
  if (report.failed.length > 0) {
    const first = report.failed[0];
    parts.push(
      `${tf("skillCards.batchTogglePartialFailed", {
        failed: report.failed.length,
        total,
      })}: ${first.skill} → ${first.agentId}: ${first.reason}`
    );
  }
  if (report.skipped.length > 0) {
    parts.push(
      tf("skillCards.batchTogglePartialSkipped", {
        skipped: report.skipped.length,
        total,
      })
    );
  }
  return parts.length > 0 ? parts.join("\n") : null;
}

// The default ops binding: the SkillCardsDrawer facade. The generated
// declarations type the reports as JsonValue; the facade shapes them exactly
// like LinkReport (report_to_json), so the cast is the bridge's trust edge.
export const drawerOps: DeckDrawerOps = {
  setDeckAgent: (deckId, agentId, enable) =>
    SkillCardsDrawer.setDeckAgent(deckId, agentId, enable) as Promise<LinkReport>,
  linkDeckToEnabledAgents: (deckId) =>
    SkillCardsDrawer.linkDeckToEnabledAgents(deckId) as Promise<LinkReport>,
  duplicateDeck: (deckId) => SkillCardsDrawer.duplicateDeck(deckId),
  deleteDeck: (deckId) => SkillCardsDrawer.deleteDeck(deckId),
};

/** A finished job tells the deck page to reload (the GroupsChanged event the
 * GPUI shell routes; pages never import each other). */
export function notifyDeckListChanged(): void {
  groupsChanged.publish();
}

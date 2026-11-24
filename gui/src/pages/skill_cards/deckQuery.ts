// Deck-page data model and pure predicates — the TSX counterpart of
// crates/ss-gpui/src/skill_cards/query.rs plus the snapshot shapes the
// SkillCards facade returns. Kept UI-free so headless tests cover the
// filtering and notice-shaping rules as plain data (the same split the
// GPUI side keeps query.rs for).

export type AgentLinkState = "none" | "mixed" | "linked";

/** One skill group (ss_skills::skill_group::SkillGroup, serde field names). */
export interface DeckRecord {
  id: string;
  name: string;
  description: string;
  icon: string;
  skills: string[];
  skillSources: Record<string, string>;
  defaultAgent: string;
  /** Agent ids this deck is explicitly linked to; null reads as unlinked. */
  agentLinks: string[] | null;
  /** Actual on-disk links per installed member skill (from snapshot_body). */
  memberAgentLinks?: Record<string, string[]>;
  /** Exact per-agent link state computed from installed members ("none" | "mixed" | "linked"). */
  agentLinkStates?: Record<string, AgentLinkState>;
  createdAt: string;
  updatedAt: string;
}

/** A targetable rail agent (has a global skills dir AND is enabled). */
export interface AgentRailProfile {
  id: string;
  displayName: string;
  icon?: string;
}

/** One `SkillCards.snapshot()` call. */
export interface DeckSnapshot {
  decks: DeckRecord[];
  installedSkills: string[];
  agents: AgentRailProfile[];
}

/** `SkillCards.toggle_deck_agent`'s outcome report (AgentLinkReport). */
export interface LinkIssue {
  agentId: string;
  skill: string;
  reason: string;
}

export interface LinkReport {
  /** `[agentId, skill]` pairs now in the requested state. */
  applied: [string, string][];
  skipped: LinkIssue[];
  failed: LinkIssue[];
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null ? (value as Record<string, unknown>) : {};
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
}

function parseMemberAgentLinks(value: unknown): Record<string, string[]> | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const out: Record<string, string[]> = {};
  for (const [member, links] of Object.entries(value as Record<string, unknown>)) {
    out[member] = asStringArray(links);
  }
  return out;
}

function parseAgentLinkStates(value: unknown): Record<string, AgentLinkState> | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const out: Record<string, AgentLinkState> = {};
  for (const [agentId, state] of Object.entries(value as Record<string, unknown>)) {
    if (state === "none" || state === "mixed" || state === "linked") {
      out[agentId] = state;
    }
  }
  return out;
}

/** Unified case-insensitive match against both agent `id` and `displayName`. */
export function matchesAgentRef(
  links: readonly string[],
  profile: { id: string; displayName: string }
): boolean {
  const idLower = profile.id.trim().toLowerCase();
  const nameLower = profile.displayName.trim().toLowerCase();
  return links.some((raw) => {
    const trimmed = raw.trim().toLowerCase();
    return (idLower.length > 0 && trimmed === idLower) || (nameLower.length > 0 && trimmed === nameLower);
  });
}

/** Compute exact deck link state ("none" | "mixed" | "linked") for an agent. */
export function deckAgentLinkState(
  deck: Pick<DeckRecord, "agentLinks" | "memberAgentLinks" | "agentLinkStates">,
  profile: { id: string; displayName: string }
): AgentLinkState {
  const precomputed = deck.agentLinkStates?.[profile.id];
  if (precomputed === "none" || precomputed === "mixed" || precomputed === "linked") {
    return precomputed;
  }
  const declared = matchesAgentRef(deck.agentLinks ?? [], profile);
  if (!declared) return "none";
  if (deck.memberAgentLinks) {
    const memberLists = Object.values(deck.memberAgentLinks);
    if (memberLists.length > 0) {
      const linkedCount = memberLists.filter((links) => matchesAgentRef(links, profile)).length;
      return linkedCount === memberLists.length ? "linked" : "mixed";
    }
  }
  return "linked";
}

/** Clicking `none` or `mixed` requests enable=true (links missing members); only `linked` unlinks. */
export function nextEnableForAgentState(state: AgentLinkState): boolean {
  return state !== "linked";
}

/** Defensive parse of the facade's JSON into the page's deck record: an
 * unknown shape degrades to defaults instead of crashing the page. */
export function parseDeck(value: unknown): DeckRecord {
  const raw = asRecord(value);
  const links = raw.agentLinks;
  return {
    id: typeof raw.id === "string" ? raw.id : "",
    name: typeof raw.name === "string" ? raw.name : "",
    description: typeof raw.description === "string" ? raw.description : "",
    icon: typeof raw.icon === "string" ? raw.icon : "",
    skills: asStringArray(raw.skills),
    skillSources: asRecord(raw.skillSources) as Record<string, string>,
    defaultAgent: typeof raw.defaultAgent === "string" ? raw.defaultAgent : "",
    // None and an empty set read the same on the rail (skill_group.rs).
    agentLinks: Array.isArray(links) ? asStringArray(links) : null,
    memberAgentLinks: parseMemberAgentLinks(raw.memberAgentLinks),
    agentLinkStates: parseAgentLinkStates(raw.agentLinkStates),
    createdAt: typeof raw.createdAt === "string" ? raw.createdAt : "",
    updatedAt: typeof raw.updatedAt === "string" ? raw.updatedAt : "",
  };
}

export function parseSnapshot(value: unknown): DeckSnapshot {
  const raw = asRecord(value);
  return {
    decks: Array.isArray(raw.decks) ? raw.decks.map(parseDeck) : [],
    installedSkills: asStringArray(raw.installedSkills),
    agents: Array.isArray(raw.agents)
      ? raw.agents.map((agent) => {
          const a = asRecord(agent);
          return {
            id: typeof a.id === "string" ? a.id : "",
            displayName: typeof a.displayName === "string" ? a.displayName : "",
            icon: typeof a.icon === "string" ? a.icon : undefined,
          };
        })
      : [],
  };
}

export function parseLinkReport(value: unknown): LinkReport {
  const raw = asRecord(value);
  const issues = (list: unknown): LinkIssue[] =>
    Array.isArray(list)
      ? list.map((issue) => {
          const i = asRecord(issue);
          return {
            agentId: typeof i.agentId === "string" ? i.agentId : "",
            skill: typeof i.skill === "string" ? i.skill : "",
            reason: typeof i.reason === "string" ? i.reason : "",
          };
        })
      : [];
  return {
    applied: Array.isArray(raw.applied)
      ? raw.applied
          .filter((pair): pair is [string, string] => Array.isArray(pair) && pair.length === 2)
          .map((pair) => [String(pair[0]), String(pair[1])])
      : [],
    skipped: issues(raw.skipped),
    failed: issues(raw.failed),
  };
}

/**
 * Check if a skill deck matches a user search query across name,
 * description, or contained skills — query.rs `matches_deck_query` parity,
 * character for character (trim, lowercase, substring on all three).
 */
export function matchesDeckQuery(deck: DeckRecord, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q.length === 0) {
    return true;
  }
  return (
    deck.name.toLowerCase().includes(q) ||
    deck.description.toLowerCase().includes(q) ||
    deck.skills.some((skill) => skill.toLowerCase().includes(q))
  );
}

/** The partial-failure notice for a link job — notify.rs
 * `agent_link_notice` parity: the first failure with its skill→agent and
 * reason, then the skipped count; null when everything applied. `tr` is
 * the page's dtf so the shape stays testable without the locale table. */
export function linkReportNotice(
  report: LinkReport,
  tr: (key: string, vars: Record<string, string | number>) => string
): string | null {
  const total = report.applied.length + report.skipped.length + report.failed.length;
  const parts: string[] = [];
  const firstFailure = report.failed[0];
  if (firstFailure) {
    parts.push(
      `${tr("skillCards.batchTogglePartialFailed", {
        failed: report.failed.length,
        total,
      })}: ${firstFailure.skill} → ${firstFailure.agentId}: ${firstFailure.reason}`
    );
  }
  if (report.skipped.length > 0) {
    parts.push(
      tr("skillCards.batchTogglePartialSkipped", { skipped: report.skipped.length, total })
    );
  }
  return parts.length > 0 ? parts.join("\n") : null;
}

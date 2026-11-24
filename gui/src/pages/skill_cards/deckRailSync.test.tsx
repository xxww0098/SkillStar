// Deck rail → skills page sync — the cross-page reload lock.
//
// A deck card's rail press links (or unlinks) that deck's member Skills in the
// Agent directories on disk. The skills page paints each Skill's own agent
// links, and the shell mounts every page exactly once (keep-alive), so nothing
// there repaints unless the deck write announces itself
// (shell/events.ts groupsChanged). This test opens the whole app on stubbed
// facades — these two snapshots write nothing and the toggle is answered in
// memory — and proves the skills page reloads and repaints after a deck rail
// press. Selectors are scoped to the owning page: every keep-alive page stays
// in the tree, and the marketplace's own rails paint the same labels.
import { expect, open, test } from "nsh/test";
import { WINDOW_H, WINDOW_W } from "../../layout";

const DECK_ID = "deck-sync";
const SKILL = "alpha-sync";
const AGENT_ID = "claude";
const AGENT_NAME = "Claude Code";

const SKILLS_PAGE = "id:page-my_skills >> ";
const DECKS_PAGE = "id:page-skill_cards >> ";

/** The link state the deck store and the skill store share in these stubs. */
let linked = false;
/** How many times the skills page reloaded its snapshot. */
let skillSnapshots = 0;

/** `SkillCards.snapshot`: one deck holding one installed member. */
function deckSnapshot() {
  return {
    decks: [
      {
        id: DECK_ID,
        name: "Rail Sync Deck",
        description: "",
        icon: "🧪",
        skills: [SKILL],
        skillSources: {},
        defaultAgent: "",
        agentLinks: linked ? [AGENT_ID] : [],
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
      },
    ],
    installedSkills: [SKILL],
    agents: [{ id: AGENT_ID, displayName: AGENT_NAME, icon: "" }],
  };
}

/** `MySkills.snapshot`: the same Skill, whose links are display names (the
 * domain reads them back from the Agent directories). */
function skillsSnapshot() {
  skillSnapshots += 1;
  return {
    skills: [
      {
        name: SKILL,
        description: "Sandbox member",
        skillType: "hub",
        stars: 0,
        installed: true,
        updateAvailable: false,
        upstreamChange: null,
        lastUpdated: "2026-01-01T00:00:00Z",
        gitUrl: "",
        author: "sandbox",
        agentLinks: linked ? [AGENT_NAME] : [],
      },
    ],
    agents: [{ id: AGENT_ID, displayName: AGENT_NAME, enabled: true, targetable: true }],
    favorites: [],
    favoritesLocked: false,
  };
}

/** The bridge stand-ins the app imports as `skillstar-gui/native`. The update
 * check is answered without one so a background check cannot reload the page
 * on its own and hide a missing announcement. */
const STUBS = {
  "skillstar-gui/native": {
    "SkillCards.snapshot": () => deckSnapshot(),
    "SkillCards.toggleDeckAgent": (deckId: string, agentId: string, enable: boolean) => {
      if (deckId === DECK_ID && agentId === AGENT_ID) linked = enable;
      return { applied: [[agentId, SKILL]], skipped: [], failed: [] };
    },
    "MySkills.snapshot": () => skillsSnapshot(),
    "MySkills.checkUpdates": () => ({ pending: 0 }),
  },
};

test("a deck rail press repaints the skills page agent links", async () => {
  linked = false;
  const app = await open("../../../App.tsx", {
    width: WINDOW_W,
    height: WINDOW_H,
    modules: STUBS,
  });

  // The skills page is the startup page: the member reads as unlinked there.
  await app.waitUntil(() => app.shows(`${SKILLS_PAGE}label:链接到 ${AGENT_NAME}`));
  expect(app.shows(`${SKILLS_PAGE}label:取消链接 ${AGENT_NAME}`)).toBe(false);

  // Light the Agent on the deck card's rail and wait for that card to settle.
  await app.press("id:nav-decks");
  await app.waitUntil(() => app.shows(`id:deck-card-${DECK_ID}`));
  const snapshotsBefore = skillSnapshots;
  await app.press(`id:deck-slot-${DECK_ID}-${AGENT_ID}`);
  await app.waitUntil(() => app.shows(`${DECKS_PAGE}label:取消整组与 ${AGENT_NAME} 的链接`));

  // The keep-alive skills page defers off-page reloads while skill_cards is active,
  // then reloads its own snapshot as soon as the user navigates back to my_skills.
  expect(skillSnapshots).toBe(snapshotsBefore);

  // Back on the skills page (it never remounted) it loads the fresh snapshot,
  // the same Skill reads as linked, and its slot now offers to unlink the Agent.
  await app.press("id:nav-skills");
  await app.waitUntil(() => skillSnapshots > snapshotsBefore, { timeout: 2000 });
  await app.waitUntil(() => app.shows(`id:skill-card-${SKILL}`));
  await app.waitUntil(() => app.shows(`${SKILLS_PAGE}label:取消链接 ${AGENT_NAME}`));
  expect(app.shows(`${SKILLS_PAGE}label:链接到 ${AGENT_NAME}`)).toBe(false);
});

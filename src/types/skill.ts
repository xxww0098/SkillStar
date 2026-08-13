//! skill domain types. Split out of the old monolithic index for
//! navigability; all re-exported by `index.ts`.

export type SkillCategory = "Hot" | "Popular" | "Rising" | "New" | "None";

export interface Skill {
  name: string;
  description: string;
  localized_description?: string | null;
  /** "hub" for git-backed, "local" for user-authored local skills */
  skill_type: "hub" | "local";
  stars: number;
  installed: boolean;
  update_available: boolean;
  last_updated: string;
  git_url: string;
  tree_hash: string | null;
  category: SkillCategory;
  author: string | null;
  topics: string[];
  agent_links?: string[];
  /** Leaderboard rank position (1-indexed) */
  rank?: number;
  /** skills.sh source repo (e.g. "vercel-labs/skills") */
  source?: string;
}

export interface SkillUpdateState {
  name: string;
  update_available: boolean;
}

/** Return type of the `update_skill` command. When a repo-cached skill is
 *  updated the entire repo is pulled, which implicitly updates all siblings. */

export interface UpdateResult {
  skill: Skill;
  /** Names of sibling skills from the same repo whose update_available was
   *  also cleared by the repo pull. */
  siblings_cleared: string[];
  /** Per-agent re-link failures after the update ("Agent: error"). The update
   *  itself succeeded; warn the user that an agent deployment may be stale. */
  agent_link_failures: string[];
}

export interface SkillUpdateFailure {
  name: string;
  error: string;
}

/** Why an update stopped. `source_removed` and `source_missing` mean the Skill
 *  is no longer shipped by its source, so there is no upstream state to go back
 *  to — only `source_removed` still has content left to keep. */
export type LocalDivergenceReason =
  | "content_changed"
  | "baseline_missing"
  | "snapshot_failed"
  | "source_removed"
  | "source_missing";

export interface SkillUpdateBlocked {
  name: string;
  reason: LocalDivergenceReason;
  suggested_local_name: string;
  error: string | null;
}

export type LocalDivergenceResolution =
  | { kind: "preserve"; local_name: string }
  | { kind: "discard" }
  | { kind: "uninstall" };

export interface ResolveSkillUpdateResult {
  update: UpdateResult | null;
  local_copy: Skill | null;
  /** Skills removed because their source dropped them — they are not coming back. */
  uninstalled: string[];
  remaining_blocked: SkillUpdateBlocked[];
}

/** A stop whose Skill no longer exists at its source: it can be kept as a local
 *  copy (content permitting) or removed, but never discarded back to upstream. */
export function isSourceGone(reason: LocalDivergenceReason): boolean {
  return reason === "source_removed" || reason === "source_missing";
}

/** A Skill a shared channel owns, which the generic update path declines by
 *  design rather than fails on. */
export interface SkillUpdateChannelManaged {
  name: string;
  repository_id: number;
}

/** Return type of the `update_skills` batch command. `skipped` names were not
 *  pulled because a skill sharing their repository was — their content moved
 *  anyway. A failed update reports every name it would have covered, so
 *  nothing is quietly counted as done. `channel_managed` is declined-by-design,
 *  not a failure: those Skills update through the shared channel flow. */
export interface SkillUpdateReport {
  updated: UpdateResult[];
  blocked: SkillUpdateBlocked[];
  failed: SkillUpdateFailure[];
  skipped: string[];
  channel_managed: SkillUpdateChannelManaged[];
}

/** A complete update run, including whatever the blocked-update dialog resolved
 *  along the way. `uninstalled` Skills were removed because their source no
 *  longer ships them — they are gone from the library, not merely unchanged. */
export interface SkillUpdateRunReport extends SkillUpdateReport {
  uninstalled: string[];
}

/** A new skill found in a cached repo that the user hasn't installed yet. */

export interface SkillCardDeck {
  id: string;
  name: string;
  description: string;
  icon: string;
  skills: string[];
  skill_sources: Record<string, string>;
  /** Agent ids this deck is explicitly linked to. A new deck starts empty —
   *  it never inherits the links its Skills already have. `null` only reaches
   *  the UI if the backend backfill could not resolve a pre-existing deck. */
  agent_links: string[] | null;
  created_at: string;
  updated_at: string;
}

export interface SkillContent {
  name: string;
  description: string | null;
  triggers: string[];
  scopes: string[];
  "allowed-tools": string[];
  content: string;
}

export type SkillTutorialState = "missing" | "fresh" | "stale";

export type SkillTutorialStaleReason = "content_changed" | "generator_changed";

export type SkillTutorialStyle = "guided" | "reference" | "workshop";

export interface SkillTutorialMetadata {
  skillName: string;
  contentHash: string;
  promptVersion: string;
  schemaVersion: string;
  /** Prompt style used to generate this artifact. Missing only on legacy artifacts. */
  tutorialStyle?: SkillTutorialStyle;
  agentLabel: string;
  generatedAt: string;
  fileCount: number;
  totalBytes: number;
}

/** Persisted HTML tutorial status returned by the backend after hash validation. */
export interface SkillTutorial {
  state: SkillTutorialState;
  currentHash: string;
  html: string | null;
  metadata: SkillTutorialMetadata | null;
  staleReason: SkillTutorialStaleReason | null;
}

export interface FrontmatterEntry {
  key: string;
  value: string;
}

export interface SkillInstallTarget {
  id: string;
  folder_path: string;
}

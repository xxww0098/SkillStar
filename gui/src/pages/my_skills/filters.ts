// 我的技能页查询面 — my_skills/filters.rs + types.rs 对位。
//
// 排序与 needs_attention 语义逐行对齐 filters.rs，不自创规则：
// - needs_attention = update_available || upstream_change.is_some()
//   （updates 筛选、工具栏计数、smart 排序顶层共用这一个谓词）；
// - updated_at：RFC3339 严格解析（chrono parse_from_rfc3339 的宽松度不
//   多给——Date.parse 会吃 "2026-01-01" 这类 chrono 拒收的串），空串与
//   解析失败一律无日期，下沉；
// - smart：attention 降序 → 时间降序（无日期垫底）→ 名称升序；
//   updated：时间降序（无日期垫底）→ 名称升序；name：仅名称。
//   名称比较按码点（Rust String Ord 是 UTF-8 字节序 = 码点序；JS 默认
//   字符串比较是 UTF-16 码元序，代理对区间会排错，故手写码点比较）。
// - Stars 不是键：安装列表组装 stars:0（types.rs SortOption 注释）。

export type MySkillsScope = "local" | "channels";
export type SortOption = "smart" | "updated" | "name";
export type SourceFilter = "all" | "hub" | "local";

export const SORT_OPTIONS: SortOption[] = ["smart", "updated", "name"];
export const SOURCE_FILTERS: SourceFilter[] = ["all", "hub", "local"];

/** 卡面记录（facade skill_json 的 TS 形状；见 my_skills.rs skill_json）。 */
export interface SkillRecord {
  name: string;
  description: string;
  localizedDescription?: string;
  skillType: "hub" | "local";
  stars: number;
  installed: boolean;
  updateAvailable: boolean;
  upstreamChange:
    | { kind: "removed" }
    | { kind: "identity_changed"; upstreamName: string }
    | { kind: "local_changes" }
    | null;
  lastUpdated: string;
  gitUrl: string;
  author?: string;
  agentLinks: string[];
  rank?: number;
  source?: string;
  called?: string;
}

/** 工具栏 agent 条带一枚槽位（facade snapshot agents 项）。 */
export interface AgentProfileRecord {
  id: string;
  displayName: string;
  icon?: string;
  enabled: boolean;
  /** enabled 且有全局技能目录（targetable_agent_profiles）。 */
  targetable: boolean;
}

/** The page's one attention predicate (filters.rs needs_attention). */
export function needsAttention(skill: SkillRecord): boolean {
  return skill.updateAvailable || skill.upstreamChange !== null;
}

/** `last_updated` as a comparable instant; null when undated or unparseable
 * (filters.rs updated_at). */
export function updatedAtMs(skill: SkillRecord): number | null {
  const raw = skill.lastUpdated.trim();
  if (raw.length === 0) return null;
  // chrono::DateTime::parse_from_rfc3339: full date T time with mandatory
  // zone. Keep JS's parser behind the same gate.
  if (!/^\d{4}-\d{2}-\d{2}[Tt]\d{2}:\d{2}:\d{2}(\.\d+)?([Zz]|[+-]\d{2}:\d{2})$/.test(raw)) {
    return null;
  }
  const ms = Date.parse(raw);
  return Number.isNaN(ms) ? null : ms;
}

/** 码点序名称比较（Rust String Ord 对位；JS 默认比较在补充平面字符上
 * 与码点序不一致）。 */
export function compareNames(left: string, right: string): number {
  if (left === right) return 0;
  const a = Array.from(left);
  const b = Array.from(right);
  const len = Math.min(a.length, b.length);
  for (let ix = 0; ix < len; ix += 1) {
    const ca = a[ix].codePointAt(0) as number;
    const cb = b[ix].codePointAt(0) as number;
    if (ca !== cb) return ca - cb;
  }
  return a.length - b.length;
}

function compareUpdatedDesc(left: SkillRecord, right: SkillRecord): number {
  const lu = updatedAtMs(left);
  const ru = updatedAtMs(right);
  if (lu === null && ru === null) return 0;
  // Reverse(Option<DateTime>)：Some 在前，None 垫底。
  if (lu === null) return 1;
  if (ru === null) return -1;
  return ru - lu;
}

/** Order the card grid (filters.rs sort_skills)。 */
export function sortSkills(skills: SkillRecord[], sort: SortOption): SkillRecord[] {
  const copy = skills.slice();
  switch (sort) {
    case "smart":
      copy.sort((left, right) => {
        const la = needsAttention(left);
        const ra = needsAttention(right);
        if (la !== ra) return la ? -1 : 1;
        const byTime = compareUpdatedDesc(left, right);
        if (byTime !== 0) return byTime;
        return compareNames(left.name, right.name);
      });
      break;
    case "updated":
      copy.sort((left, right) => {
        const byTime = compareUpdatedDesc(left, right);
        if (byTime !== 0) return byTime;
        return compareNames(left.name, right.name);
      });
      break;
    case "name":
      copy.sort((left, right) => compareNames(left.name, right.name));
      break;
  }
  return copy;
}

/** skill_in_scope (filters.rs)：query 命中 名/描述/本地化描述/来源/作者
 * 任一；source 筛选 Hub 排 Local、Local 排非 Local；repo 精确匹配
 * skill.source；agent 以 display_name 匹配 agent_links。 */
export function skillInScope(
  skill: SkillRecord,
  query: string,
  source: SourceFilter,
  repo: string | null,
  agentName: string | null
): boolean {
  if (query.length > 0) {
    const nameMatch = skill.name.toLowerCase().includes(query);
    const descMatch = skill.description.toLowerCase().includes(query);
    const locMatch = (skill.localizedDescription ?? "")
      .toLowerCase()
      .includes(query);
    const sourceMatch = (skill.source ?? "").toLowerCase().includes(query);
    const authorMatch = (skill.author ?? "").toLowerCase().includes(query);
    const calledMatch = (skill.called ?? "").toLowerCase().includes(query);
    if (!nameMatch && !descMatch && !locMatch && !sourceMatch && !authorMatch && !calledMatch) {
      return false;
    }
  }
  if (source === "hub" && skill.skillType === "local") return false;
  if (source === "local" && skill.skillType !== "local") return false;
  if (repo !== null && (skill.source ?? null) !== repo) return false;
  if (agentName !== null && !skill.agentLinks.includes(agentName)) return false;
  return true;
}

/** The page's query state as one input bundle (mirrors the MySkillsPage
 * fields filtered_skills reads). */
export interface QueryState {
  query: string;
  sourceFilter: SourceFilter;
  repoFilter: string | null;
  agentFilterId: string | null;
  onlyUpdates: boolean;
  sortBy: SortOption;
}

/** Search, agent, source, repo — the set the toolbar counts before the
 * updates toggle (filters.rs scoped_skills)。 */
export function scopedSkills(
  skills: SkillRecord[],
  agents: AgentProfileRecord[],
  state: Pick<QueryState, "query" | "sourceFilter" | "repoFilter" | "agentFilterId">
): SkillRecord[] {
  const query = state.query.toLowerCase();
  const agentName =
    state.agentFilterId === null
      ? null
      : (agents.find((agent) => agent.id === state.agentFilterId)?.displayName ?? null);
  return skills.filter((skill) =>
    skillInScope(skill, query, state.sourceFilter, state.repoFilter, agentName)
  );
}

/** Returns skills matching search, source, repo, agent, updates and sort
 * (filters.rs filtered_skills)。 */
export function filteredSkills(
  skills: SkillRecord[],
  agents: AgentProfileRecord[],
  state: QueryState
): SkillRecord[] {
  const scoped = scopedSkills(skills, agents, state);
  const narrowed = state.onlyUpdates ? scoped.filter(needsAttention) : scoped;
  return sortSkills(narrowed, state.sortBy);
}

/** Non-empty distinct sources, sorted then deduped (origin_menu.rs
 * repo_sources)。 */
export function repoSources(skills: SkillRecord[]): string[] {
  const sources = skills
    .map((skill) => skill.source ?? "")
    .filter((source) => source.length > 0);
  sources.sort();
  return [...new Set(sources)];
}

/** Favorites first; each group stays in name order (origin_menu.rs
 * order_repos)。 */
export function orderRepos(repos: string[], favorites: ReadonlySet<string>): string[] {
  return repos.slice().sort((left, right) => {
    const lf = favorites.has(left) ? 1 : 0;
    const rf = favorites.has(right) ? 1 : 0;
    if (lf !== rf) return rf - lf;
    return left < right ? -1 : left > right ? 1 : 0;
  });
}

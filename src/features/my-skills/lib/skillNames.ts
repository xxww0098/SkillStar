/** Skill-name normalization helpers shared by the deck UI and install flow. */

export const normalizeSkillName = (name: string): string => name.trim();

export function uniqueNormalizedSkillNames(names: string[]): string[] {
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const rawName of names) {
    const name = normalizeSkillName(rawName);
    if (!name || seen.has(name)) continue;
    seen.add(name);
    normalized.push(name);
  }
  return normalized;
}

export function normalizeSkillSources(sources?: Record<string, string>): Record<string, string> {
  const normalized: Record<string, string> = {};
  if (!sources) return normalized;
  for (const [rawName, rawUrl] of Object.entries(sources)) {
    const name = normalizeSkillName(rawName);
    const url = rawUrl?.trim();
    if (!name || !url) continue;
    normalized[name] = url;
  }
  return normalized;
}

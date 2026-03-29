import { invoke } from "@tauri-apps/api/core";
import type {
  MarketplaceDescriptionPatch,
  MarketplaceDescriptionRequest,
  Skill,
} from "../types";

export const MARKETPLACE_DESCRIPTION_BATCH_SIZE = 24;
const ATTEMPT_COOLDOWN_MS = 5 * 60 * 1000;

const inflightKeys = new Set<string>();
const attemptedAtByKey = new Map<string, number>();

type SkillLike = Pick<Skill, "name" | "source" | "git_url" | "description">;
type DescriptionKeyTarget = {
  name: string;
  source?: string | null;
  git_url?: string | null;
};

function normalizeSource(source: string | null | undefined): string | null {
  if (!source) return null;

  const normalized = source
    .trim()
    .replace(/^https?:\/\/github\.com\//i, "")
    .replace(/\.git$/i, "")
    .replace(/\/$/, "")
    .toLowerCase();

  const parts = normalized.split("/").filter(Boolean);
  if (parts.length < 2) return null;
  return `${parts[0]}/${parts[1]}`;
}

function sourceFromGitUrl(gitUrl: string | null | undefined): string | null {
  if (!gitUrl) return null;
  return normalizeSource(gitUrl);
}

function normalizeSkillName(name: string): string | null {
  const normalized = name.trim().toLowerCase();
  return normalized.length > 0 ? normalized : null;
}

function normalizePatchKey(key: string): string {
  return key.trim().toLowerCase();
}

function shouldAttemptKey(key: string, force = false): boolean {
  if (force) return true;
  const lastAttemptAt = attemptedAtByKey.get(key);
  if (!lastAttemptAt) return true;
  return Date.now() - lastAttemptAt > ATTEMPT_COOLDOWN_MS;
}

function markAttempted(key: string) {
  attemptedAtByKey.set(key, Date.now());
}

export function buildMarketplaceDescriptionKey(
  target: DescriptionKeyTarget
): string | null {
  const source = normalizeSource(target.source) ?? sourceFromGitUrl(target.git_url);
  const name = normalizeSkillName(target.name);
  if (!source || !name) return null;
  return `${source}/${name}`;
}

export function isMissingMarketplaceDescription(
  description: string | null | undefined
): boolean {
  return !description || description.trim().length === 0;
}

export function createMarketplaceDescriptionRequest(
  target: DescriptionKeyTarget
): MarketplaceDescriptionRequest | null {
  const key = buildMarketplaceDescriptionKey(target);
  if (!key) return null;

  return {
    name: target.name,
    source: normalizeSource(target.source) ?? undefined,
    git_url: target.git_url ?? undefined,
  };
}

export function collectMarketplaceDescriptionRequests(
  skills: SkillLike[],
  limit = MARKETPLACE_DESCRIPTION_BATCH_SIZE
): MarketplaceDescriptionRequest[] {
  const requests: MarketplaceDescriptionRequest[] = [];
  const seen = new Set<string>();

  for (const skill of skills) {
    if (requests.length >= limit) break;
    if (!isMissingMarketplaceDescription(skill.description)) continue;

    const key = buildMarketplaceDescriptionKey(skill);
    if (!key || seen.has(key) || inflightKeys.has(key) || !shouldAttemptKey(key)) {
      continue;
    }

    const request = createMarketplaceDescriptionRequest(skill);
    if (!request) continue;

    seen.add(key);
    requests.push(request);
  }

  return requests;
}

function keyByRequest(
  request: MarketplaceDescriptionRequest
): { key: string; request: MarketplaceDescriptionRequest } | null {
  const key = buildMarketplaceDescriptionKey({
    name: request.name,
    source: request.source,
    git_url: request.git_url,
  });

  if (!key) return null;

  return {
    key,
    request: {
      name: request.name,
      source: normalizeSource(request.source) ?? undefined,
      git_url: request.git_url ?? undefined,
    },
  };
}

export async function hydrateMarketplaceDescriptions(
  requests: MarketplaceDescriptionRequest[],
  options?: { force?: boolean }
): Promise<MarketplaceDescriptionPatch[]> {
  if (requests.length === 0) return [];

  const force = options?.force === true;
  const deduped = new Map<string, MarketplaceDescriptionRequest>();

  for (const request of requests) {
    const keyed = keyByRequest(request);
    if (!keyed) continue;
    if (inflightKeys.has(keyed.key)) continue;
    if (!shouldAttemptKey(keyed.key, force)) continue;
    deduped.set(keyed.key, keyed.request);
  }

  const pending = Array.from(deduped.entries());
  if (pending.length === 0) return [];

  for (const [key] of pending) {
    inflightKeys.add(key);
    markAttempted(key);
  }

  try {
    const patches = await invoke<MarketplaceDescriptionPatch[]>(
      "hydrate_marketplace_descriptions",
      {
        requests: pending.map(([, request]) => request),
      }
    );

    const successKeys = new Set(
      patches
        .map((patch) => normalizePatchKey(patch.key))
        .filter((key) => key.length > 0)
    );
    for (const [key] of pending) {
      if (successKeys.has(key)) {
        // Successful patches should be retryable immediately in case
        // a later full refresh overwrites descriptions again.
        attemptedAtByKey.delete(key);
      }
    }

    return patches.filter(
      (patch) =>
        typeof patch.key === "string" &&
        patch.key.trim().length > 0 &&
        !isMissingMarketplaceDescription(patch.description)
    );
  } catch (error) {
    console.error("[marketplaceDescriptionHydration] hydrate failed:", error);
    return [];
  } finally {
    for (const [key] of pending) {
      inflightKeys.delete(key);
    }
  }
}

export async function hydrateDescriptionsForSkills(
  skills: SkillLike[],
  limit = MARKETPLACE_DESCRIPTION_BATCH_SIZE
): Promise<MarketplaceDescriptionPatch[]> {
  const requests = collectMarketplaceDescriptionRequests(skills, limit);
  return hydrateMarketplaceDescriptions(requests);
}

export async function hydrateDescriptionForSkill(
  skill: DescriptionKeyTarget
): Promise<MarketplaceDescriptionPatch[]> {
  const request = createMarketplaceDescriptionRequest(skill);
  if (!request) return [];
  return hydrateMarketplaceDescriptions([request], { force: true });
}

export function applyMarketplaceDescriptionPatches(
  skills: Skill[],
  patches: MarketplaceDescriptionPatch[]
): Skill[] {
  if (skills.length === 0 || patches.length === 0) return skills;

  const patchByKey = new Map<string, string>();
  for (const patch of patches) {
    if (isMissingMarketplaceDescription(patch.description)) continue;
    patchByKey.set(normalizePatchKey(patch.key), patch.description.trim());
  }

  if (patchByKey.size === 0) return skills;

  return skills.map((skill) => {
    const key = buildMarketplaceDescriptionKey(skill);
    if (!key) return skill;

    const nextDescription = patchByKey.get(key);
    if (!nextDescription || skill.description === nextDescription) return skill;

    return {
      ...skill,
      description: nextDescription,
    };
  });
}

export function applyMarketplaceDescriptionPatchToSkill(
  skill: Skill | null,
  patches: MarketplaceDescriptionPatch[]
): Skill | null {
  if (!skill || patches.length === 0) return skill;

  const [nextSkill] = applyMarketplaceDescriptionPatches([skill], patches);
  return nextSkill ?? skill;
}

import type { CliAccountState, Subscription, SwitchOutcome } from "../types";

/**
 * What a card may honestly say about the local tool behind it (CLI or IDE).
 *
 * - `current` — the local tool is serving *this* account right now.
 * - `diverged` — this card is pinned, but the local tool is on something else.
 * - `missing` — this card is pinned, but the local tool has no credential at all.
 * - `none` — nothing to say.
 */
export type CliAccountBadge = "current" | "diverged" | "missing" | "none";

/**
 * Which badge a row has earned.
 *
 * The pin (`is_active`) records what the user last asked for; `state` is what
 * the backend read back off disk. When they disagree the file wins — it is the
 * one the CLI opens — so a `login` done in a terminal, a failed switch, or a
 * credential wiped behind SkillStar's back all move the badge, not just the pin.
 *
 * `state` is `undefined` for catalogs with no CLI behind them (an IDE-backed
 * provider, a plain API key) and for the first render before the reconcile
 * lands. Both fall back to the pin, which for those catalogs *is* the truth:
 * there is no file to disagree with it.
 *
 * `diverged` / `missing` are only ever shown on the pinned row. They describe
 * the gap between "you asked for this" and "the CLI is doing that", and an
 * unpinned row has no such claim to contradict.
 */
export function cliAccountBadge(
  subscription: Pick<Subscription, "id" | "is_active">,
  state: CliAccountState | undefined,
): CliAccountBadge {
  const pinned = subscription.is_active === true;
  if (!state) return pinned ? "current" : "none";
  switch (state.kind) {
    case "linkedTo":
      // The account the CLI is actually serving carries the badge even when
      // the pin still names a sibling.
      return state.subscriptionId === subscription.id ? "current" : pinned ? "diverged" : "none";
    case "diverged":
      return pinned ? "diverged" : "none";
    case "missing":
      return pinned ? "missing" : "none";
  }
}

/** Look up a subscription's live CLI state in the per-catalog reconcile map. */
export function cliAccountBadgeFor(
  subscription: Pick<Subscription, "id" | "catalog_id" | "is_active">,
  statesByCatalog: Record<string, CliAccountState>,
): CliAccountBadge {
  return cliAccountBadge(subscription, statesByCatalog[subscription.catalog_id]);
}

/**
 * `true` when the switch bound the CLI by copying bytes instead of linking.
 *
 * Worth saying out loud: under a symlink the CLI's own token rotation lands in
 * SkillStar's snapshot on its own, and under a copy it does not. Windows
 * without symlink privilege is the case that hits it, and it used to be
 * visible only in a log line nobody reads.
 */
export function isDegradedCopyBinding(outcome: Pick<SwitchOutcome, "linkMode"> | null | undefined): boolean {
  return outcome?.linkMode === "copy";
}

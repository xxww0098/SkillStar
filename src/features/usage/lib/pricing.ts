import type { Subscription } from "../types";

export interface SpendEntry {
  currency: string;
  amount: number;
}

/** Normalized monthly cost; mirrors `get_usage_summary` in usage_commands.rs. */
export function monthlyEquivalentPrice(sub: Pick<Subscription, "monthly_price" | "billing_cycle">): number | null {
  const price = sub.monthly_price;
  if (price == null || !Number.isFinite(price) || price <= 0) return null;
  switch (sub.billing_cycle) {
    case "annual":
      return price / 12;
    case "one-time":
      return null;
    default:
      return price;
  }
}

/** Estimated cumulative spend since subscription start. */
export function totalSpendForSubscription(sub: Subscription, nowSec = Math.floor(Date.now() / 1000)): number {
  const price = sub.monthly_price;
  if (price == null || !Number.isFinite(price) || price <= 0) return 0;

  switch (sub.billing_cycle) {
    case "one-time":
      return price;
    case "annual":
      return billingPeriodsElapsed(sub, nowSec, "annual") * price;
    case "monthly":
      return billingPeriodsElapsed(sub, nowSec, "monthly") * price;
  }
}

function subscriptionStartSec(sub: Subscription): number {
  if (sub.start_date > 0) return sub.start_date;
  if (sub.created_at > 0) return sub.created_at;
  return Math.floor(Date.now() / 1000);
}

function billingPeriodsElapsed(sub: Subscription, nowSec: number, cycle: "monthly" | "annual"): number {
  const startSec = subscriptionStartSec(sub);
  if (nowSec <= startSec) return 1;

  const start = new Date(startSec * 1000);
  const now = new Date(nowSec * 1000);

  if (cycle === "monthly") {
    let months = (now.getFullYear() - start.getFullYear()) * 12 + (now.getMonth() - start.getMonth());
    if (now.getDate() < start.getDate()) months -= 1;
    return Math.max(1, months + 1);
  }

  let years = now.getFullYear() - start.getFullYear();
  const beforeAnniversary =
    now.getMonth() < start.getMonth() || (now.getMonth() === start.getMonth() && now.getDate() < start.getDate());
  if (beforeAnniversary) years -= 1;
  return Math.max(1, years + 1);
}

function foldSpendByCurrency(subs: Subscription[], amountFor: (sub: Subscription) => number): SpendEntry[] {
  const totals = new Map<string, number>();
  for (const sub of subs) {
    const amount = amountFor(sub);
    if (amount <= 0) continue;
    totals.set(sub.currency, (totals.get(sub.currency) ?? 0) + amount);
  }
  return Array.from(totals.entries()).map(([currency, amount]) => ({ currency, amount }));
}

export function aggregateMonthlySpend(subs: Subscription[]): SpendEntry[] {
  return foldSpendByCurrency(subs, (sub) => monthlyEquivalentPrice(sub) ?? 0);
}

export function aggregateTotalSpend(subs: Subscription[]): SpendEntry[] {
  return foldSpendByCurrency(subs, totalSpendForSubscription);
}

export function formatCurrencyAmount(amount: number, currency: string): string {
  const symbol = currency === "CNY" ? "¥" : currency === "USD" ? "$" : "";
  return `${symbol}${amount.toFixed(2)}`;
}

export function formatSpendEntries(entries: SpendEntry[]): string {
  return entries.map((entry) => formatCurrencyAmount(entry.amount, entry.currency)).join(" · ");
}

/** Reorder a movable subset within a full id list (preserves immovable positions). */
export function mergeSubscriptionOrder(allIds: string[], movableIds: string[], newMovableOrder: string[]): string[] {
  const movable = new Set(movableIds);
  const queue = [...newMovableOrder];
  return allIds.map((id) => (movable.has(id) ? (queue.shift() ?? id) : id));
}

export function subscriptionHasSpend(sub: Subscription): boolean {
  const total = totalSpendForSubscription(sub);
  const monthly = monthlyEquivalentPrice(sub);
  return total > 0 || (monthly != null && monthly > 0);
}

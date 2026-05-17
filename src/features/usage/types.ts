// Mirrors `src-tauri/src/commands/usage_dto.rs`. Keep in sync.

export type AuthMode = "api-key" | "o-auth" | "manual";

export type CatalogTier = "o-auth" | "api-key" | "manual";

export type BillingCycle = "monthly" | "annual" | "one-time";

export type AlertSeverity = "info" | "warning" | "danger";

export type AlertKind = "quota-low" | "quota-critical" | "renew-soon" | "expired" | "needs-reauth";

export interface CatalogEntry {
  id: string;
  display_name: string;
  description: string;
  tier: CatalogTier;
  auth_modes: AuthMode[];
  brand_color: string;
  default_currency: string;
  subscription_url: string;
  warning: string | null;
  regions: string[];
}

export interface ManualQuota {
  total_tokens: number | null;
  used_tokens: number | null;
  period_label: string | null;
}

export interface UsageWindow {
  label: string;
  used: number;
  total: number | null;
  percent: number | null;
  reset_at: number | null;
}

export interface MonetaryBalance {
  currency: string;
  total: number;
  granted: number;
  topped_up: number;
}

export interface SubscriptionUsage {
  subscription_id: string;
  fetched_at: number;
  plan_name: string | null;
  hourly: UsageWindow | null;
  weekly: UsageWindow | null;
  monthly: UsageWindow | null;
  balance: MonetaryBalance | null;
  error: string | null;
}

export interface Subscription {
  id: string;
  catalog_id: string;
  display_name: string;
  auth_mode: AuthMode;
  plan_tier: string | null;
  monthly_price: number | null;
  currency: string;
  billing_cycle: BillingCycle;
  start_date: number;
  renew_date: number;
  auto_renew: boolean;
  has_credential: boolean;
  requires_reauth: boolean;
  manual_quota: ManualQuota | null;
  note: string | null;
  sort_index: number;
  created_at: number;
  updated_at: number;
  usage: SubscriptionUsage | null;
}

export interface CreateSubscriptionInput {
  catalog_id: string;
  display_name?: string;
  auth_mode: AuthMode;
  plan_tier?: string;
  monthly_price?: number;
  currency?: string;
  billing_cycle?: BillingCycle;
  start_date?: number;
  renew_date?: number;
  auto_renew?: boolean;
  api_key?: string;
  oauth_region?: string;
  manual_quota?: ManualQuota;
  note?: string;
}

export interface UpdateSubscriptionInput {
  display_name?: string;
  plan_tier?: string;
  monthly_price?: number;
  currency?: string;
  billing_cycle?: BillingCycle;
  start_date?: number;
  renew_date?: number;
  auto_renew?: boolean;
  api_key?: string;
  manual_quota?: ManualQuota;
  note?: string;
}

export interface SubscriptionAlert {
  id: string;
  subscription_id: string;
  severity: AlertSeverity;
  kind: AlertKind;
  message: string;
}

export interface MonthlySpendEntry {
  currency: string;
  amount: number;
}

export interface UsageSummary {
  monthly_spend: MonthlySpendEntry[];
  total_subscriptions: number;
  alert_count: number;
  reauth_count: number;
}

export interface OAuthStart {
  pending_id: string;
  auth_url: string;
}

/** Sidebar selection: "all" | a specific catalog id. */
export type CatalogFilter = string;
export const FILTER_ALL: CatalogFilter = "__all__";

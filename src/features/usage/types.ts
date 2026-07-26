// Mirrors `src-tauri/src/commands/usage_dto.rs`. Keep in sync.
// Usage catalog is two tiers only: OAuth + API key (cookie mode removed).

export type AuthMode = "api-key" | "o-auth";

/** Auth modes shown in the subscription dialog (OAuth / API key only). */
export function selectableAuthModes(modes: AuthMode[]): AuthMode[] {
  return modes.filter((mode) => mode === "o-auth" || mode === "api-key");
}

export type CatalogTier = "o-auth" | "api-key";

/** How the user pays for this row — independent of `AuthMode`. */
export type BillingCycle = "monthly" | "annual" | "one-time" | "api-key";

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
  /** Nested sub-quotas rendered inside a visual container under this bar. */
  breakdown?: UsageWindow[];
}

export interface MonetaryBalance {
  currency: string;
  total: number;
  granted: number;
  topped_up: number;
  /** Provider-specific availability flag (e.g. DeepSeek `is_available`). */
  is_available?: boolean | null;
}

export interface CreditInfo {
  credit_type: string;
  credit_amount?: string | null;
  minimum_credit_amount_for_usage?: string | null;
}

export interface OpenCodeApiKey {
  id: string;
  name: string;
  display: string;
  email: string | null;
}

export interface DeepSeekModelUsage {
  key: string;
  name: string;
  total_tokens: number;
  request_count: number;
  cache_hit_tokens: number;
  cache_miss_tokens: number;
  response_tokens: number;
  cost: number;
}

export interface DeepSeekDailyUsage {
  date: string;
  flash_tokens: number;
  flash_cache_hit: number;
  flash_cache_miss: number;
  flash_response: number;
  pro_tokens: number;
  pro_cache_hit: number;
  pro_cache_miss: number;
  pro_response: number;
  total_tokens: number;
  total_cost: number;
}

export interface DeepSeekAnalytics {
  month_cost: number;
  today_cost: number;
  models: DeepSeekModelUsage[];
  daily: DeepSeekDailyUsage[];
}

export interface SubscriptionUsage {
  subscription_id: string;
  fetched_at: number;
  plan_name: string | null;
  hourly: UsageWindow | null;
  weekly: UsageWindow | null;
  monthly: UsageWindow | null;
  balance: MonetaryBalance | null;
  credits: CreditInfo[];
  error: string | null;
  api_keys: OpenCodeApiKey[];
  deepseek_analytics?: DeepSeekAnalytics | null;
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
  /** DeepSeek platform session token configured for usage charts. */
  has_platform_token?: boolean;
  requires_reauth: boolean;
  /** `true` when this row is the currently-pinned account for its catalog
   *  (Phase 7 multi-account). At most one per catalog_id. */
  is_active?: boolean;
  oauth_region?: string;
  manual_quota: ManualQuota | null;
  note: string | null;
  sort_index: number;
  created_at: number;
  updated_at: number;
  usage: SubscriptionUsage | null;
  /** Outcome of the last CLI account-switch attempt (set by
   *  `set_active_subscription` when it pushes credentials to the CLI).
   *  Absent when no switch was attempted. */
  switch_result?: SwitchOutcome | null;
  /** Whether this catalog maps to a CLI SkillStar can switch credentials
   *  for (codex / opencode / grok). IDE-only catalogs are `false`. */
  supports_cli_switch?: boolean;
}

/** Result of pushing a subscription's credentials into its CLI config. */
export interface SwitchOutcome {
  toolId: string;
  configPath: string;
  backupPath?: string | null;
  keychainUpdated: boolean;
  success: boolean;
  error?: string | null;
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
  /** DeepSeek platform session token (usage analytics). */
  platform_token?: string;
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
  platform_token?: string;
  clearPlatformToken?: boolean;
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
  /** GitHub Device Flow — show in UI for manual entry. */
  user_code?: string | null;
  verification_uri?: string | null;
}

/** Catalog ids that support `import_subscription_from_local`. */
export const LOCAL_IMPORT_CATALOG_IDS = ["codex", "antigravity"] as const;

/** Sidebar selection: "all" | a specific catalog id. */
export type CatalogFilter = string;
export const FILTER_ALL: CatalogFilter = "__all__";

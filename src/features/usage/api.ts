import { tauriInvokeDynamic as invoke } from "../../lib/ipc/core";
import type {
  CatalogEntry,
  CreateSubscriptionInput,
  OAuthStart,
  Subscription,
  SubscriptionAlert,
  SwitchOutcome,
  UpdateSubscriptionInput,
  UsageSummary,
} from "./types";

export const usageApi = {
  listCatalog: () => invoke<CatalogEntry[]>("list_usage_catalog"),
  listSubscriptions: () => invoke<Subscription[]>("list_subscriptions"),
  createSubscription: (input: CreateSubscriptionInput) => invoke<Subscription>("create_subscription", { input }),
  updateSubscription: (id: string, input: UpdateSubscriptionInput) =>
    invoke<Subscription>("update_subscription", { id, input }),
  deleteSubscription: (id: string) => invoke<void>("delete_subscription", { id }),
  reorderSubscriptions: (ids: string[]) => invoke<void>("reorder_subscriptions", { ids }),
  refreshSubscriptionUsage: (id: string) => invoke<Subscription>("refresh_subscription_usage", { id }),
  refreshAllSubscriptions: () => invoke<Subscription[]>("refresh_all_subscriptions"),
  getSubscriptionAlerts: () => invoke<SubscriptionAlert[]>("get_subscription_alerts"),
  dismissSubscriptionAlert: (alertId: string) => invoke<void>("dismiss_subscription_alert", { alertId }),
  getUsageSummary: () => invoke<UsageSummary>("get_usage_summary"),
  startOAuthLogin: (catalogId: string, region?: string, subscriptionId?: string) =>
    invoke<OAuthStart>("start_oauth_login", { catalogId, region, subscriptionId }),
  awaitOAuthCompletion: (pendingId: string) => invoke<Subscription>("await_oauth_completion", { pendingId }),
  submitOAuthCallback: (pendingId: string, callbackInput: string) =>
    invoke<void>("submit_oauth_callback", { pendingId, callbackInput }),
  cancelOAuthLogin: (pendingId: string) => invoke<void>("cancel_oauth_login", { pendingId }),
  importSubscriptionFromLocal: (catalogId: string) =>
    invoke<Subscription>("import_subscription_from_local", { catalogId }),
  getSubscriptionApiKey: (id: string) => invoke<string | null>("get_subscription_api_key", { id }),

  // ── Multi-account: active-per-catalog (Phase 7) ──────────────────
  getActiveSubscriptions: () => invoke<Record<string, string>>("get_active_subscriptions"),
  setActiveSubscription: (subscriptionId: string) =>
    invoke<Subscription>("set_active_subscription", { subscriptionId }),
  clearActiveSubscription: (catalogId: string) => invoke<void>("clear_active_subscription", { catalogId }),
  // Re-push the active account's credentials to its CLI config (retry after a
  // failed switch, e.g. once a missing id_token has been refreshed).
  switchActiveSubscriptionToCli: (catalogId: string) =>
    invoke<SwitchOutcome>("switch_active_subscription_to_cli", { catalogId }),

  // ── Floating card windows (multi-window) ─────────────────────────
  openUsageCardWindow: (subscriptionId: string) => invoke<void>("open_usage_card_window", { subscriptionId }),
  closeUsageCardWindow: (subscriptionId: string) => invoke<void>("close_usage_card_window", { subscriptionId }),
};

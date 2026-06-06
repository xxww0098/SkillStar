import { tauriInvokeDynamic as invoke } from "../../lib/ipc/core";
import type {
  CatalogEntry,
  CookieBridgeBindingStatus,
  CookieImportSession,
  CookieImportStatus,
  CreateSubscriptionInput,
  OAuthStart,
  Subscription,
  SubscriptionAlert,
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
  cancelOAuthLogin: (pendingId: string) => invoke<void>("cancel_oauth_login", { pendingId }),
  importSubscriptionFromLocal: (catalogId: string) =>
    invoke<Subscription>("import_subscription_from_local", { catalogId }),
  getSubscriptionApiKey: (id: string) => invoke<string | null>("get_subscription_api_key", { id }),
  startCookieImportSession: (provider: string, subscriptionId?: string) =>
    invoke<CookieImportSession>("start_cookie_import_session", { provider, subscriptionId }),
  getCookieImportStatus: (sessionId: string) => invoke<CookieImportStatus>("get_cookie_import_status", { sessionId }),
  cancelCookieImportSession: (sessionId: string) => invoke<void>("cancel_cookie_import_session", { sessionId }),
  getCookieImportSubscription: (id: string) => invoke<Subscription>("get_cookie_import_subscription", { id }),
  getCookieBridgeBindingStatus: (provider: string) =>
    invoke<CookieBridgeBindingStatus>("get_cookie_bridge_binding_status", { provider }),
  resetCookieBridgeBinding: (provider: string) => invoke<void>("reset_cookie_bridge_binding", { provider }),

  // ── Multi-account: active-per-catalog (Phase 7) ──────────────────
  getActiveSubscriptions: () => invoke<Record<string, string>>("get_active_subscriptions"),
  setActiveSubscription: (subscriptionId: string) =>
    invoke<Subscription>("set_active_subscription", { subscriptionId }),
  clearActiveSubscription: (catalogId: string) => invoke<void>("clear_active_subscription", { catalogId }),
};

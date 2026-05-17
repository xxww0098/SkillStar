import { invoke } from "@tauri-apps/api/core";
import type {
  CatalogEntry,
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
  startOAuthLogin: (catalogId: string, region?: string) =>
    invoke<OAuthStart>("start_oauth_login", { catalogId, region }),
  awaitOAuthCompletion: (pendingId: string) => invoke<Subscription>("await_oauth_completion", { pendingId }),
  cancelOAuthLogin: (pendingId: string) => invoke<void>("cancel_oauth_login", { pendingId }),
};

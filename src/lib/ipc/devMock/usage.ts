/**
 * Dev-mock fragment: usage mode — subscription/quota tracking. Sample data
 * lives in ./usageData.ts. (These commands are invoked dynamically from
 * src/features/usage/api.ts rather than declared in ../commands/*.ts.)
 */

import type { DevMockHandlers } from "./shared";
import { USAGE_ALERTS, USAGE_CATALOG, USAGE_SUBSCRIPTIONS, USAGE_SUMMARY } from "./usageData";

export const USAGE_HANDLERS: DevMockHandlers = {
  list_usage_catalog: () => USAGE_CATALOG,
  list_subscriptions: () => USAGE_SUBSCRIPTIONS,
  get_active_subscriptions: () => ({
    cursor: "sub-cursor",
    codex: "sub-codex",
    deepseek: "sub-deepseek",
    glm: "sub-glm",
  }),
  get_subscription_alerts: () => USAGE_ALERTS,
  get_usage_summary: () => USAGE_SUMMARY,
  // Returns full Subscription list (backend shape). Optional catalogId is
  // accepted for API parity; mock still returns every sample row.
  refresh_all_subscriptions: (_args?: Record<string, unknown>) => USAGE_SUBSCRIPTIONS,
  refresh_subscription_usage: (args) => USAGE_SUBSCRIPTIONS.find((s) => s.id === args?.id)?.usage ?? null,
  get_subscription_api_key: () => "sk-demo-********",
};
